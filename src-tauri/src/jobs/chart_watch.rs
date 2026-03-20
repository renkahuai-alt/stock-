use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use chrono::Utc;
use tokio::time::{sleep, Duration};
use uuid::Uuid;

use crate::board_weights::{
    board_value_mode, board_weight_snapshot, renormalize_weights, resolve_snapshot_weights,
};
use crate::errors::{AppError, AppResult};
use crate::live_quote;
use crate::models::{
    ChartWatchStatusPayload, LiveOverlayBar, LiveQuoteOverlayPayload, StartChartWatchPayload,
    StopChartWatchStatusPayload,
};
use crate::repositories::Database;
use crate::services::chart::{compose_board_bars, live_overlay_key};
use crate::services::market_data::{MarketDataProvider, MarketDataTarget, ProviderQuote};
use crate::services::{ActiveWatchHandle, AppRuntime};
use crate::telemetry;

pub type ChartLiveUpdateEmitter = Arc<dyn Fn(LiveQuoteOverlayPayload) + Send + Sync>;

const DEFAULT_WATCH_INTERVAL_SECONDS: u64 = 15;

#[derive(Clone)]
struct WatchContext {
    watch_id: String,
    target_type: String,
    target_id: String,
    granularity: String,
    normalized_board_algorithm: String,
    payload_board_algorithm: Option<String>,
    overlay_key: String,
}

type WeightedMembers = Vec<(String, f64)>;
type LiveBoardWeights = (WeightedMembers, &'static str, Option<String>);

pub fn default_watch_interval_seconds() -> u64 {
    DEFAULT_WATCH_INTERVAL_SECONDS
}

pub async fn start(
    database: Database,
    runtime: AppRuntime,
    payload: StartChartWatchPayload,
    emitter: ChartLiveUpdateEmitter,
) -> AppResult<ChartWatchStatusPayload> {
    let started = Instant::now();
    database.bootstrap()?;
    telemetry::emit(
        "chart_watch_start_requested",
        &[
            ("targetType", payload.target_type.clone()),
            ("targetId", payload.target_id.clone()),
            (
                "granularity",
                payload
                    .granularity
                    .clone()
                    .unwrap_or_else(|| "day".to_string()),
            ),
            (
                "boardAlgorithm",
                payload
                    .board_algorithm
                    .clone()
                    .unwrap_or_else(|| "none".to_string()),
            ),
        ],
    );

    let target_type = normalize_target_type(&payload.target_type)?;
    let granularity = normalize_granularity(payload.granularity.as_deref())?;
    let normalized_board_algorithm = normalize_board_algorithm(payload.board_algorithm.as_deref())?;
    let payload_board_algorithm = payload_board_algorithm(target_type, normalized_board_algorithm);
    validate_target(
        &database,
        target_type,
        &payload.target_id,
        normalized_board_algorithm,
    )?;

    if let Some(active_watch) = runtime.active_watch().await {
        if active_watch.matches(
            target_type,
            &payload.target_id,
            granularity,
            payload_board_algorithm.as_deref(),
        ) {
            telemetry::emit(
                "chart_watch_reused",
                &[
                    ("watchId", active_watch.watch_id.clone()),
                    ("targetType", active_watch.target_type.clone()),
                    ("targetId", active_watch.target_id.clone()),
                ],
            );
            return Ok(status_from_active_watch(&active_watch));
        }

        stop_active_watch(&runtime, active_watch).await;
    }

    let provider = runtime.provider()?;
    if !provider.is_available() {
        return Err(AppError::Message(
            "Longbridge credentials are not configured".to_string(),
        ));
    }
    let market_status = runtime.market_status("US")?;
    let watch_id = Uuid::new_v4().to_string();
    let updated_at = Utc::now().to_rfc3339();

    if market_status.market_state != "open" {
        telemetry::emit(
            "chart_watch_start_skipped",
            &[
                ("targetType", target_type.to_string()),
                ("targetId", payload.target_id.clone()),
                ("marketState", "closed".to_string()),
                ("tradeDate", market_status.trade_date.clone()),
            ],
        );
        return Ok(ChartWatchStatusPayload {
            watch_id,
            started: false,
            target_type: target_type.to_string(),
            target_id: payload.target_id,
            granularity: granularity.to_string(),
            board_algorithm: payload_board_algorithm,
            interval_sec: default_watch_interval_seconds(),
            market_state: "closed".to_string(),
            updated_at,
            message: Some("当前不在盘中时段".to_string()),
        });
    }

    let target_id = payload.target_id;
    let watch = WatchContext {
        watch_id: watch_id.clone(),
        target_type: target_type.to_string(),
        target_id: target_id.clone(),
        granularity: granularity.to_string(),
        normalized_board_algorithm: normalized_board_algorithm.to_string(),
        payload_board_algorithm: payload_board_algorithm.clone(),
        overlay_key: live_overlay_key(
            target_type,
            &target_id,
            granularity,
            normalized_board_algorithm,
        ),
    };
    let task_watch = watch.clone();
    let task_runtime = runtime.clone();
    let task_database = database.clone();
    let handle = tauri::async_runtime::spawn(async move {
        run_watch_loop(task_database, task_runtime, emitter, task_watch).await;
    });

    runtime
        .set_active_watch(Some(ActiveWatchHandle {
            watch_id: watch_id.clone(),
            overlay_key: watch.overlay_key.clone(),
            target_type: watch.target_type.clone(),
            target_id: watch.target_id.clone(),
            granularity: watch.granularity.clone(),
            board_algorithm: payload_board_algorithm.clone(),
            interval_sec: default_watch_interval_seconds(),
            abort_handle: handle.inner().abort_handle(),
        }))
        .await;

    telemetry::emit(
        "chart_watch_started",
        &[
            ("watchId", watch_id.clone()),
            ("targetType", watch.target_type.clone()),
            ("targetId", watch.target_id.clone()),
            ("marketState", "open".to_string()),
            ("elapsedMs", started.elapsed().as_millis().to_string()),
        ],
    );

    Ok(ChartWatchStatusPayload {
        watch_id,
        started: true,
        target_type: watch.target_type,
        target_id: watch.target_id,
        granularity: watch.granularity,
        board_algorithm: payload_board_algorithm,
        interval_sec: default_watch_interval_seconds(),
        market_state: "open".to_string(),
        updated_at,
        message: None,
    })
}

pub async fn stop(runtime: AppRuntime) -> AppResult<StopChartWatchStatusPayload> {
    let updated_at = Utc::now().to_rfc3339();
    let Some(active_watch) = runtime.set_active_watch(None).await else {
        telemetry::emit(
            "chart_watch_stopped",
            &[
                ("watchId", "none".to_string()),
                ("hadActiveWatch", "false".to_string()),
            ],
        );
        return Ok(StopChartWatchStatusPayload {
            stopped: true,
            watch_id: None,
            updated_at,
        });
    };

    active_watch.abort_handle.abort();
    runtime.clear_live_overlay(&active_watch.overlay_key);
    telemetry::emit(
        "chart_watch_stopped",
        &[
            ("watchId", active_watch.watch_id.clone()),
            ("targetType", active_watch.target_type.clone()),
            ("targetId", active_watch.target_id.clone()),
            ("hadActiveWatch", "true".to_string()),
        ],
    );

    Ok(StopChartWatchStatusPayload {
        stopped: true,
        watch_id: Some(active_watch.watch_id),
        updated_at,
    })
}

async fn run_watch_loop(
    database: Database,
    runtime: AppRuntime,
    emitter: ChartLiveUpdateEmitter,
    watch: WatchContext,
) {
    let mut consecutive_failures = 0usize;
    loop {
        if !is_current_watch(&runtime, &watch.watch_id).await {
            runtime.clear_live_overlay(&watch.overlay_key);
            break;
        }

        let market_status = match runtime.market_status("US") {
            Ok(status) => {
                telemetry::emit(
                    "chart_watch_market_status",
                    &[
                        ("watchId", watch.watch_id.clone()),
                        ("marketState", status.market_state.clone()),
                        ("tradeDate", status.trade_date.clone()),
                    ],
                );
                status
            }
            Err(error) => {
                consecutive_failures += 1;
                telemetry::emit(
                    "chart_watch_failure",
                    &[
                        ("watchId", watch.watch_id.clone()),
                        ("failureCount", consecutive_failures.to_string()),
                        ("stage", "market_status".to_string()),
                        ("error", error.to_string()),
                    ],
                );
                emit_degraded_overlay(&database, &runtime, &emitter, &watch, error.to_string())
                    .await;
                sleep(Duration::from_secs(DEFAULT_WATCH_INTERVAL_SECONDS)).await;
                continue;
            }
        };

        if market_status.market_state != "open" {
            telemetry::emit(
                "chart_watch_market_closed",
                &[
                    ("watchId", watch.watch_id.clone()),
                    ("tradeDate", market_status.trade_date.clone()),
                ],
            );
            emit_market_closed_overlay(&database, &runtime, &emitter, &watch).await;
            runtime.clear_live_overlay(&watch.overlay_key);
            let _ = runtime.clear_active_watch_if(&watch.watch_id).await;
            break;
        }

        let provider = match runtime.provider() {
            Ok(provider) => provider,
            Err(error) => {
                consecutive_failures += 1;
                telemetry::emit(
                    "chart_watch_failure",
                    &[
                        ("watchId", watch.watch_id.clone()),
                        ("failureCount", consecutive_failures.to_string()),
                        ("stage", "provider".to_string()),
                        ("error", error.to_string()),
                    ],
                );
                emit_degraded_overlay(&database, &runtime, &emitter, &watch, error.to_string())
                    .await;
                sleep(Duration::from_secs(DEFAULT_WATCH_INTERVAL_SECONDS)).await;
                continue;
            }
        };

        match build_live_overlay(&database, provider, &watch, &market_status.trade_date).await {
            Ok(payload) => {
                consecutive_failures = 0;
                maybe_emit_overlay(&runtime, &emitter, &watch, payload).await;
            }
            Err(error) => {
                consecutive_failures += 1;
                telemetry::emit(
                    "chart_watch_failure",
                    &[
                        ("watchId", watch.watch_id.clone()),
                        ("failureCount", consecutive_failures.to_string()),
                        ("stage", "build_overlay".to_string()),
                        ("error", error.to_string()),
                    ],
                );
                emit_degraded_overlay(&database, &runtime, &emitter, &watch, error.to_string())
                    .await;
            }
        }

        sleep(Duration::from_secs(DEFAULT_WATCH_INTERVAL_SECONDS)).await;
    }
}

async fn build_live_overlay(
    database: &Database,
    provider: Arc<dyn MarketDataProvider>,
    watch: &WatchContext,
    trade_date: &str,
) -> AppResult<LiveQuoteOverlayPayload> {
    match watch.target_type.as_str() {
        "symbol" | "index" => build_symbol_overlay(database, provider, watch, trade_date).await,
        "board" => build_board_overlay(database, provider, watch, trade_date).await,
        other => Err(AppError::Message(format!(
            "unsupported targetType: {other}"
        ))),
    }
}

async fn build_symbol_overlay(
    database: &Database,
    provider: Arc<dyn MarketDataProvider>,
    watch: &WatchContext,
    trade_date: &str,
) -> AppResult<LiveQuoteOverlayPayload> {
    let target = database
        .get_symbol(&watch.target_id, &watch.target_type)?
        .ok_or_else(|| {
            AppError::Message(format!(
                "target not found: {}/{}",
                watch.target_type, watch.target_id
            ))
        })?;
    let target_ref = MarketDataTarget {
        target_id: target.target_id.clone(),
        target_type: target.target_type.clone(),
        provider_symbol: if target.display_code.is_empty() {
            target.target_id.clone()
        } else {
            target.display_code.clone()
        },
    };
    let quote = provider
        .fetch_realtime_quotes(&[target_ref])
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| AppError::Message(format!("missing live quote for {}", watch.target_id)))?;

    Ok(live_quote::overlay_payload(
        &watch.watch_id,
        &watch.target_type,
        &watch.target_id,
        &watch.granularity,
        &Utc::now().to_rfc3339(),
        watch.payload_board_algorithm.clone(),
        "open",
        &quote.source_status,
        LiveOverlayBar {
            trade_date: trade_date.to_string(),
            open: round2(quote.open),
            high: round2(quote.high),
            low: round2(quote.low),
            close: round2(quote.close),
            volume: quote.volume.map(round2),
        },
        Some(target.name),
        Some(target.display_code),
        Some(provider.provider_name().to_string()),
        None,
        None,
        None,
        None,
    ))
}

#[allow(clippy::too_many_arguments)]
async fn build_board_overlay(
    database: &Database,
    provider: Arc<dyn MarketDataProvider>,
    watch: &WatchContext,
    trade_date: &str,
) -> AppResult<LiveQuoteOverlayPayload> {
    let board = database
        .get_board(&watch.target_id)?
        .ok_or_else(|| AppError::Message(format!("board not found: {}", watch.target_id)))?;
    let members = database.list_board_members(&watch.target_id)?;
    if members.is_empty() {
        return Err(AppError::Message(format!(
            "board has no members: {}",
            watch.target_id
        )));
    }

    let symbol_rows = database.list_symbols_by_ids(&members)?;
    let quote_targets = dedupe_targets(
        &symbol_rows
            .iter()
            .map(|record| MarketDataTarget {
                target_id: record.target_id.clone(),
                target_type: record.target_type.clone(),
                provider_symbol: if record.display_code.is_empty() {
                    record.target_id.clone()
                } else {
                    record.display_code.clone()
                },
            })
            .collect::<Vec<_>>(),
    );
    let quotes = provider.fetch_realtime_quotes(&quote_targets).await?;
    let quotes_by_id: HashMap<String, ProviderQuote> = quotes
        .into_iter()
        .map(|quote| (quote.target_id.clone(), quote))
        .collect();
    let latest_closes = database.list_latest_closes_by_ids(&members)?;

    let snapshot = previous_board_snapshot(
        database,
        &watch.target_id,
        &watch.normalized_board_algorithm,
        &members,
    )?;
    let (weight_snapshot, weight_snapshot_trade_date) = board_weight_snapshot(
        &watch.normalized_board_algorithm,
        Some(snapshot.trade_date.as_str()),
    );
    let (weights, source_status, message) = live_board_weights(
        &members,
        &symbol_rows,
        &latest_closes,
        &quotes_by_id,
        &watch.normalized_board_algorithm,
    )?;

    if weights.is_empty() {
        return degraded_overlay_from_history(
            database,
            watch,
            "open",
            "degraded",
            message.unwrap_or_else(|| "缺少可用成分股盘中数据".to_string()),
        )
        .ok_or_else(|| {
            AppError::Message(format!(
                "missing board overlay fallback for {}",
                watch.target_id
            ))
        });
    }

    let mut open = 0.0;
    let mut high = 0.0;
    let mut low = 0.0;
    let mut close = 0.0;
    let mut volume = 0.0;

    for (target_id, weight) in &weights {
        let quote = quotes_by_id.get(target_id).ok_or_else(|| {
            AppError::Message(format!("missing live quote for board member {target_id}"))
        })?;
        if quote.prev_close <= 0.0 {
            continue;
        }
        open += weight * (quote.open / quote.prev_close);
        high += weight * (quote.high / quote.prev_close);
        low += weight * (quote.low / quote.prev_close);
        close += weight * (quote.close / quote.prev_close);
        volume += weight * quote.volume.unwrap_or_default();
    }

    Ok(live_quote::overlay_payload(
        &watch.watch_id,
        "board",
        &watch.target_id,
        &watch.granularity,
        &Utc::now().to_rfc3339(),
        watch.payload_board_algorithm.clone(),
        "open",
        source_status,
        LiveOverlayBar {
            trade_date: trade_date.to_string(),
            open: round2(snapshot.close * open),
            high: round2(snapshot.close * high),
            low: round2(snapshot.close * low),
            close: round2(snapshot.close * close),
            volume: Some(round2(volume)),
        },
        Some(board.name),
        None,
        Some("computed_board_overlay".to_string()),
        board_value_mode(),
        weight_snapshot,
        weight_snapshot_trade_date,
        message,
    ))
}

async fn maybe_emit_overlay(
    runtime: &AppRuntime,
    emitter: &ChartLiveUpdateEmitter,
    watch: &WatchContext,
    payload: LiveQuoteOverlayPayload,
) {
    let previous = runtime.get_live_overlay(&watch.overlay_key);
    if previous
        .as_ref()
        .is_some_and(|previous| !overlay_changed(previous, &payload))
    {
        telemetry::emit(
            "chart_live_update_skipped",
            &[
                ("watchId", watch.watch_id.clone()),
                ("reason", "unchanged_overlay".to_string()),
            ],
        );
        return;
    }

    runtime.put_live_overlay(&watch.overlay_key, &payload);
    if is_current_watch(runtime, &watch.watch_id).await {
        telemetry::emit(
            "chart_live_update_emitted",
            &[
                ("watchId", watch.watch_id.clone()),
                ("targetType", watch.target_type.clone()),
                ("targetId", watch.target_id.clone()),
                ("sourceStatus", payload.source_status.clone()),
            ],
        );
        emitter(payload);
    }
}

async fn emit_degraded_overlay(
    database: &Database,
    runtime: &AppRuntime,
    emitter: &ChartLiveUpdateEmitter,
    watch: &WatchContext,
    message: String,
) {
    let Some(payload) = degraded_overlay_from_history(database, watch, "open", "degraded", message)
    else {
        return;
    };
    maybe_emit_overlay(runtime, emitter, watch, payload).await;
}

async fn emit_market_closed_overlay(
    database: &Database,
    runtime: &AppRuntime,
    emitter: &ChartLiveUpdateEmitter,
    watch: &WatchContext,
) {
    let payload = if let Some(current) = runtime.get_live_overlay(&watch.overlay_key) {
        let mut next = current;
        next.updated_at = Utc::now().to_rfc3339();
        next.market_state = "closed".to_string();
        next.source_status = "market_closed".to_string();
        next.meta.source_status = Some("market_closed".to_string());
        next.meta.message = Some("当前不在盘中时段".to_string());
        next
    } else {
        let Some(payload) = degraded_overlay_from_history(
            database,
            watch,
            "closed",
            "market_closed",
            "当前不在盘中时段".to_string(),
        ) else {
            return;
        };
        payload
    };

    maybe_emit_overlay(runtime, emitter, watch, payload).await;
}

fn degraded_overlay_from_history(
    database: &Database,
    watch: &WatchContext,
    market_state: &str,
    source_status: &str,
    message: String,
) -> Option<LiveQuoteOverlayPayload> {
    match watch.target_type.as_str() {
        "symbol" | "index" => {
            let target = database
                .get_symbol(&watch.target_id, &watch.target_type)
                .ok()
                .flatten()?;
            let bar = database
                .list_daily_bars(&watch.target_id)
                .ok()?
                .last()
                .cloned()?;
            Some(live_quote::overlay_payload(
                &watch.watch_id,
                &watch.target_type,
                &watch.target_id,
                &watch.granularity,
                &Utc::now().to_rfc3339(),
                watch.payload_board_algorithm.clone(),
                market_state,
                source_status,
                LiveOverlayBar {
                    trade_date: bar.trade_date,
                    open: bar.open,
                    high: bar.high,
                    low: bar.low,
                    close: bar.close,
                    volume: bar.volume,
                },
                Some(target.name),
                Some(target.display_code),
                Some("historical_fallback".to_string()),
                None,
                None,
                None,
                Some(message),
            ))
        }
        "board" => {
            let board = database.get_board(&watch.target_id).ok().flatten()?;
            let members = database.list_board_members(&watch.target_id).ok()?;
            let bars = database
                .list_board_daily_bars(
                    &watch.target_id,
                    watch
                        .payload_board_algorithm
                        .as_deref()
                        .unwrap_or("equal_weight_v1"),
                )
                .ok()
                .filter(|bars| !bars.is_empty())
                .map(|bars| {
                    bars.into_iter()
                        .map(|bar| crate::models::BarPoint {
                            time: bar.trade_date,
                            open: bar.open,
                            high: bar.high,
                            low: bar.low,
                            close: bar.close,
                            volume: bar.volume,
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_else(|| {
                    compose_board_bars(
                        database,
                        &members,
                        watch
                            .payload_board_algorithm
                            .as_deref()
                            .unwrap_or("equal_weight_v1"),
                    )
                    .unwrap_or_default()
                });
            let bar = bars.last()?.clone();
            let board_algorithm = watch
                .payload_board_algorithm
                .as_deref()
                .unwrap_or("equal_weight_v1");
            let (weight_snapshot, weight_snapshot_trade_date) =
                board_weight_snapshot(board_algorithm, Some(bar.time.as_str()));
            Some(live_quote::overlay_payload(
                &watch.watch_id,
                &watch.target_type,
                &watch.target_id,
                &watch.granularity,
                &Utc::now().to_rfc3339(),
                watch.payload_board_algorithm.clone(),
                market_state,
                source_status,
                LiveOverlayBar {
                    trade_date: bar.time,
                    open: bar.open,
                    high: bar.high,
                    low: bar.low,
                    close: bar.close,
                    volume: bar.volume,
                },
                Some(board.name),
                None,
                Some("historical_fallback".to_string()),
                board_value_mode(),
                weight_snapshot,
                weight_snapshot_trade_date,
                Some(message),
            ))
        }
        _ => None,
    }
}

struct BoardSnapshot {
    trade_date: String,
    close: f64,
}

fn previous_board_snapshot(
    database: &Database,
    board_id: &str,
    board_algorithm: &str,
    members: &[String],
) -> AppResult<BoardSnapshot> {
    let bars = database.list_board_daily_bars(board_id, board_algorithm)?;
    if let Some(bar) = bars.last() {
        return Ok(BoardSnapshot {
            trade_date: bar.trade_date.clone(),
            close: bar.close,
        });
    }

    compose_board_bars(database, members, board_algorithm)?
        .last()
        .map(|bar| BoardSnapshot {
            trade_date: bar.time.clone(),
            close: bar.close,
        })
        .ok_or_else(|| AppError::Message(format!("missing board history for {board_id}")))
}

fn live_board_weights(
    members: &[String],
    symbol_rows: &[crate::models::SymbolRecord],
    latest_closes: &HashMap<String, f64>,
    quotes_by_id: &HashMap<String, ProviderQuote>,
    board_algorithm: &str,
) -> AppResult<LiveBoardWeights> {
    match board_algorithm {
        "equal_weight_v1" => {
            let valid_members: Vec<String> = symbol_rows
                .iter()
                .filter(|row| {
                    quotes_by_id
                        .get(&row.target_id)
                        .is_some_and(|quote| quote.prev_close > 0.0)
                })
                .map(|row| row.target_id.clone())
                .collect();
            if valid_members.is_empty() {
                return Ok((
                    Vec::new(),
                    "degraded",
                    Some("缺少可用成分股盘中数据".to_string()),
                ));
            }

            let weight = 1.0 / valid_members.len() as f64;
            let degraded = valid_members.len() != symbol_rows.len();
            Ok((
                valid_members
                    .into_iter()
                    .map(|member| (member, weight))
                    .collect(),
                if degraded { "degraded" } else { "live" },
                degraded.then(|| "部分成分股盘中数据缺失，已使用可用数据继续计算".to_string()),
            ))
        }
        "market_cap_weight_v1" => {
            let snapshot_weights =
                resolve_snapshot_weights(members, symbol_rows, latest_closes, board_algorithm)?;
            let mut missing_weight_members = Vec::new();
            let mut available_members = Vec::new();

            for row in symbol_rows {
                let Some(quote) = quotes_by_id.get(&row.target_id) else {
                    missing_weight_members.push(row.target_id.clone());
                    continue;
                };
                if quote.prev_close <= 0.0 {
                    missing_weight_members.push(row.target_id.clone());
                    continue;
                }
                available_members.push(row.target_id.clone());
            }

            let weighted_members = renormalize_weights(&snapshot_weights, &available_members)?;
            if weighted_members.is_empty() {
                return Ok((
                    Vec::new(),
                    "degraded",
                    Some("缺少可用权重快照，无法生成板块盘中 overlay".to_string()),
                ));
            }

            let degraded = !missing_weight_members.is_empty();
            Ok((
                weighted_members,
                if degraded { "degraded" } else { "live" },
                degraded.then(|| "部分成分股缺少权重或 quote，已使用可用数据继续计算".to_string()),
            ))
        }
        other => Err(AppError::Message(format!(
            "unsupported boardAlgorithm: {other}"
        ))),
    }
}

fn dedupe_targets(targets: &[MarketDataTarget]) -> Vec<MarketDataTarget> {
    let mut seen = HashSet::new();
    targets
        .iter()
        .filter(|target| seen.insert(target.target_id.clone()))
        .cloned()
        .collect()
}

fn overlay_changed(previous: &LiveQuoteOverlayPayload, next: &LiveQuoteOverlayPayload) -> bool {
    previous.watch_id != next.watch_id
        || previous.target_type != next.target_type
        || previous.target_id != next.target_id
        || previous.granularity != next.granularity
        || previous.board_algorithm != next.board_algorithm
        || previous.market_state != next.market_state
        || previous.source_status != next.source_status
        || previous.overlay.trade_date != next.overlay.trade_date
        || previous.overlay.open != next.overlay.open
        || previous.overlay.high != next.overlay.high
        || previous.overlay.low != next.overlay.low
        || previous.overlay.close != next.overlay.close
        || previous.overlay.volume != next.overlay.volume
        || previous.meta.message != next.meta.message
}

async fn is_current_watch(runtime: &AppRuntime, watch_id: &str) -> bool {
    runtime
        .active_watch()
        .await
        .is_some_and(|active| active.watch_id == watch_id)
}

fn status_from_active_watch(active_watch: &ActiveWatchHandle) -> ChartWatchStatusPayload {
    ChartWatchStatusPayload {
        watch_id: active_watch.watch_id.clone(),
        started: true,
        target_type: active_watch.target_type.clone(),
        target_id: active_watch.target_id.clone(),
        granularity: active_watch.granularity.clone(),
        board_algorithm: active_watch.board_algorithm.clone(),
        interval_sec: active_watch.interval_sec,
        market_state: "open".to_string(),
        updated_at: Utc::now().to_rfc3339(),
        message: None,
    }
}

async fn stop_active_watch(runtime: &AppRuntime, active_watch: ActiveWatchHandle) {
    active_watch.abort_handle.abort();
    runtime.clear_live_overlay(&active_watch.overlay_key);
    let _ = runtime.clear_active_watch_if(&active_watch.watch_id).await;
    telemetry::emit(
        "chart_watch_replaced",
        &[
            ("watchId", active_watch.watch_id),
            ("targetType", active_watch.target_type),
            ("targetId", active_watch.target_id),
        ],
    );
}

fn normalize_target_type(value: &str) -> AppResult<&'static str> {
    match value {
        "index" => Ok("index"),
        "board" => Ok("board"),
        "symbol" => Ok("symbol"),
        other => Err(AppError::Message(format!(
            "unsupported targetType: {other}"
        ))),
    }
}

fn normalize_granularity(value: Option<&str>) -> AppResult<&'static str> {
    match value.unwrap_or("day") {
        "day" => Ok("day"),
        other => Err(AppError::Message(format!(
            "WATCH_UNSUPPORTED_GRANULARITY: unsupported granularity: {other}"
        ))),
    }
}

fn normalize_board_algorithm(value: Option<&str>) -> AppResult<&'static str> {
    match value.unwrap_or("equal_weight_v1") {
        "equal_weight_v1" => Ok("equal_weight_v1"),
        "market_cap_weight_v1" => Ok("market_cap_weight_v1"),
        other => Err(AppError::Message(format!(
            "unsupported boardAlgorithm: {other}"
        ))),
    }
}

fn payload_board_algorithm(target_type: &str, normalized_board_algorithm: &str) -> Option<String> {
    (target_type == "board").then(|| normalized_board_algorithm.to_string())
}

fn validate_target(
    database: &Database,
    target_type: &str,
    target_id: &str,
    board_algorithm: &str,
) -> AppResult<()> {
    match target_type {
        "symbol" | "index" => {
            let exists = database.get_symbol(target_id, target_type)?.is_some();
            if !exists {
                return Err(AppError::Message(format!(
                    "WATCH_TARGET_NOT_FOUND: {target_type}/{target_id}"
                )));
            }
        }
        "board" => {
            let board = database.get_board(target_id)?;
            if board.is_none() {
                return Err(AppError::Message(format!(
                    "WATCH_TARGET_NOT_FOUND: board/{target_id}"
                )));
            }
            normalize_board_algorithm(Some(board_algorithm))?;
        }
        _ => {}
    }

    Ok(())
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}
