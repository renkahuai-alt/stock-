use std::collections::HashMap;

use crate::board_weights::{board_value_mode, board_weight_snapshot, resolve_snapshot_weights};
use crate::chart_engine::{aggregate_weekly, build_board_bars, trim_bars, WeightedSeries};
use crate::errors::{AppError, AppResult};
use crate::models::{
    BarPoint, ChartMeta, ChartPayload, DailyBarRecord, GetChartPayload, SymbolRecord,
};
use crate::repositories::Database;
use crate::telemetry;

use super::AppRuntime;

#[derive(Clone)]
pub struct ChartService {
    database: Database,
    runtime: AppRuntime,
}

impl ChartService {
    pub fn new(database: Database) -> Self {
        Self {
            database,
            runtime: AppRuntime::default(),
        }
    }

    pub fn with_runtime(database: Database, runtime: AppRuntime) -> Self {
        Self { database, runtime }
    }

    pub fn get_chart(&self, payload: GetChartPayload) -> AppResult<ChartPayload> {
        self.database.bootstrap()?;

        let granularity = normalize_granularity(payload.granularity.as_deref())?;
        let range = normalize_range(payload.range.as_deref())?;
        let board_algorithm = normalize_board_algorithm(payload.board_algorithm.as_deref())?;
        let cache_key = format!(
            "chart:{}:{}:{}:{}:{}",
            payload.target_type, payload.target_id, granularity, range, board_algorithm
        );
        let overlay_key = live_overlay_key(
            &payload.target_type,
            &payload.target_id,
            granularity,
            board_algorithm,
        );

        if let Some(payload) = self.runtime.get_chart_payload(&cache_key) {
            telemetry::emit(
                "chart_cache_hit",
                &[
                    ("targetType", payload.meta.target_type.clone()),
                    ("targetId", payload.meta.target_id.clone()),
                    ("granularity", granularity.to_string()),
                    ("range", range.to_string()),
                    ("boardAlgorithm", board_algorithm.to_string()),
                ],
            );
            return Ok(merge_active_overlay(
                &self.runtime,
                &overlay_key,
                granularity,
                payload,
            ));
        }

        telemetry::emit(
            "chart_cache_miss",
            &[
                ("targetType", payload.target_type.clone()),
                ("targetId", payload.target_id.clone()),
                ("granularity", granularity.to_string()),
                ("range", range.to_string()),
                ("boardAlgorithm", board_algorithm.to_string()),
            ],
        );

        let chart = match payload.target_type.as_str() {
            "index" | "symbol" => {
                self.get_symbol_chart(&payload.target_type, &payload.target_id, granularity, range)
            }
            "board" => {
                self.get_board_chart(&payload.target_id, granularity, range, board_algorithm)
            }
            other => Err(AppError::Message(format!(
                "unsupported targetType: {other}"
            ))),
        }?;

        self.runtime.put_chart_payload(&cache_key, &chart);
        Ok(merge_active_overlay(
            &self.runtime,
            &overlay_key,
            granularity,
            chart,
        ))
    }

    fn get_symbol_chart(
        &self,
        target_type: &str,
        target_id: &str,
        granularity: &str,
        range: &str,
    ) -> AppResult<ChartPayload> {
        let target = self
            .database
            .get_symbol(target_id, target_type)?
            .ok_or_else(|| {
                AppError::Message(format!("target not found: {target_type}/{target_id}"))
            })?;
        let daily_bars = if let Some(cached) = self.runtime.get_raw_daily(target_id) {
            cached
        } else {
            let bars = self.database.list_daily_bars(target_id)?;
            self.runtime.put_raw_daily(target_id, &bars);
            bars
        };

        Ok(render_chart_payload(
            &target,
            granularity,
            range,
            if daily_bars.is_empty() {
                "empty"
            } else {
                "local_cache"
            },
            infer_provider_kind(&daily_bars),
            materialize_bars(&self.runtime, target_id, &daily_bars, granularity, range),
        ))
    }

    fn get_board_chart(
        &self,
        board_id: &str,
        granularity: &str,
        range: &str,
        board_algorithm: &str,
    ) -> AppResult<ChartPayload> {
        let board = self
            .database
            .get_board(board_id)?
            .ok_or_else(|| AppError::Message(format!("board not found: {board_id}")))?;

        let mut bars: Vec<BarPoint> = self
            .database
            .list_board_daily_bars(board_id, board_algorithm)?
            .into_iter()
            .map(|bar| BarPoint {
                time: bar.trade_date,
                open: bar.open,
                high: bar.high,
                low: bar.low,
                close: bar.close,
                volume: bar.volume,
            })
            .collect();

        if bars.is_empty() {
            let members = self.database.list_board_members(board_id)?;
            bars = compose_board_bars(&self.database, &members, board_algorithm)?;
            if !bars.is_empty() {
                self.database
                    .save_board_chart(board_id, board_algorithm, &bars)?;
            }
        }

        let bars = materialize_points(
            &self.runtime,
            &format!("board:{}:{board_algorithm}", board.board_id),
            &bars,
            granularity,
            range,
        );
        let source_status = if bars.is_empty() {
            match board.build_status.as_str() {
                "queued" | "running" => "building",
                "failed" => "build_failed",
                _ => "empty",
            }
        } else {
            "local_cache"
        };
        let snapshot_trade_date = bars.last().map(|bar| bar.time.as_str());
        let (weight_snapshot, weight_snapshot_trade_date) =
            board_weight_snapshot(board_algorithm, snapshot_trade_date);

        Ok(ChartPayload {
            meta: ChartMeta {
                target_type: "board".into(),
                target_id: board.board_id.clone(),
                title: board.name.clone(),
                source_status: source_status.into(),
                provider_symbol: None,
                provider_kind: None,
                value_mode: board_value_mode(),
                weight_snapshot,
                weight_snapshot_trade_date,
                granularity: Some(granularity.to_string()),
                range: Some(range.to_string()),
            },
            latest_trade_date: bars.last().map(|bar| bar.time.clone()),
            source_status: source_status.into(),
            bars,
            active_overlay: None,
        })
    }
}

pub(crate) fn compose_board_bars(
    database: &Database,
    members: &[String],
    algorithm: &str,
) -> AppResult<Vec<BarPoint>> {
    if members.is_empty() {
        return Ok(Vec::new());
    }

    let symbol_rows = database.list_symbols_by_ids(members)?;
    if symbol_rows.len() != members.len() {
        return Err(AppError::Message(
            "board members contain unknown symbols".into(),
        ));
    }

    let mut bars_by_symbol = Vec::with_capacity(members.len());
    let mut latest_closes = HashMap::with_capacity(members.len());
    for symbol in members {
        let bars = database.list_daily_bars(symbol)?;
        if bars.is_empty() {
            return Ok(Vec::new());
        }
        latest_closes.insert(
            symbol.clone(),
            bars.last().map(|bar| bar.close).unwrap_or_default(),
        );
        bars_by_symbol.push((symbol.clone(), bars));
    }

    let weight_by_symbol =
        resolve_snapshot_weights(members, &symbol_rows, &latest_closes, algorithm)?
            .into_iter()
            .collect::<HashMap<_, _>>();
    let mut series = Vec::with_capacity(bars_by_symbol.len());

    for (symbol, bars) in bars_by_symbol {
        let weight = weight_by_symbol
            .get(&symbol)
            .copied()
            .ok_or_else(|| AppError::Message(format!("missing board weight for {symbol}")))?;
        series.push(WeightedSeries { weight, bars });
    }

    Ok(build_board_bars(&series))
}

fn materialize_bars(
    runtime: &AppRuntime,
    cache_key: &str,
    bars: &[DailyBarRecord],
    granularity: &str,
    range: &str,
) -> Vec<BarPoint> {
    let points: Vec<BarPoint> = bars
        .iter()
        .map(|bar| BarPoint {
            time: bar.trade_date.clone(),
            open: bar.open,
            high: bar.high,
            low: bar.low,
            close: bar.close,
            volume: bar.volume,
        })
        .collect();

    materialize_points(runtime, cache_key, &points, granularity, range)
}

fn materialize_points(
    runtime: &AppRuntime,
    cache_key: &str,
    points: &[BarPoint],
    granularity: &str,
    range: &str,
) -> Vec<BarPoint> {
    let trimmed = trim_bars(points, range);
    if granularity == "week" {
        let week_key = format!("week:{cache_key}:{range}");
        if let Some(cached) = runtime.get_weekly(&week_key) {
            return cached;
        }
        let aggregated = aggregate_weekly(&trimmed);
        runtime.put_weekly(&week_key, &aggregated);
        aggregated
    } else {
        trimmed
    }
}

fn render_chart_payload(
    target: &SymbolRecord,
    granularity: &str,
    range: &str,
    source_status: &str,
    provider_kind: Option<String>,
    bars: Vec<BarPoint>,
) -> ChartPayload {
    let (provider_symbol, provider_kind) = chart_provider_meta(target, provider_kind);

    ChartPayload {
        meta: ChartMeta {
            target_type: target.target_type.clone(),
            target_id: target.target_id.clone(),
            title: target.name.clone(),
            source_status: source_status.into(),
            provider_symbol,
            provider_kind,
            value_mode: None,
            weight_snapshot: None,
            weight_snapshot_trade_date: None,
            granularity: Some(granularity.to_string()),
            range: Some(range.to_string()),
        },
        latest_trade_date: bars.last().map(|bar| bar.time.clone()),
        source_status: source_status.into(),
        bars,
        active_overlay: None,
    }
}

fn infer_provider_kind(bars: &[DailyBarRecord]) -> Option<String> {
    bars.last().map(|bar| normalize_provider_kind(&bar.source))
}

fn chart_provider_meta(
    target: &SymbolRecord,
    inferred_provider_kind: Option<String>,
) -> (Option<String>, Option<String>) {
    if target.target_type != "index" {
        return (Some(target.display_code.clone()), inferred_provider_kind);
    }

    match target.target_id.as_str() {
        "GSPC" => (
            Some("SPY".to_string()),
            Some("longbridge_proxy_etf".to_string()),
        ),
        "RUT" => (
            Some("IWM".to_string()),
            Some("longbridge_proxy_etf".to_string()),
        ),
        _ => (Some(target.display_code.clone()), inferred_provider_kind),
    }
}

fn normalize_provider_kind(source: &str) -> String {
    if source.starts_with("fixture_") {
        "fixture".to_string()
    } else {
        source.to_string()
    }
}

fn merge_active_overlay(
    runtime: &AppRuntime,
    overlay_key: &str,
    granularity: &str,
    mut payload: ChartPayload,
) -> ChartPayload {
    if granularity != "day" {
        return payload;
    }

    let Some(overlay) = runtime.get_live_overlay(overlay_key) else {
        return payload;
    };

    let bar = BarPoint {
        time: overlay.overlay.trade_date.clone(),
        open: overlay.overlay.open,
        high: overlay.overlay.high,
        low: overlay.overlay.low,
        close: overlay.overlay.close,
        volume: overlay.overlay.volume,
    };

    match payload.bars.last_mut() {
        Some(last) if last.time == bar.time => *last = bar.clone(),
        Some(last) if last.time < bar.time => payload.bars.push(bar.clone()),
        None => payload.bars.push(bar.clone()),
        _ => {}
    }

    payload.latest_trade_date = Some(bar.time.clone());
    payload.source_status = overlay.source_status.clone();
    payload.meta.source_status = overlay.source_status.clone();
    payload.meta.provider_kind = overlay
        .meta
        .provider_kind
        .clone()
        .or(payload.meta.provider_kind);
    payload.meta.provider_symbol = overlay
        .meta
        .provider_symbol
        .clone()
        .or(payload.meta.provider_symbol);
    payload.meta.value_mode = overlay.meta.value_mode.clone().or(payload.meta.value_mode);
    payload.meta.weight_snapshot = overlay
        .meta
        .weight_snapshot
        .clone()
        .or(payload.meta.weight_snapshot);
    payload.meta.weight_snapshot_trade_date = overlay
        .meta
        .weight_snapshot_trade_date
        .clone()
        .or(payload.meta.weight_snapshot_trade_date);
    payload.active_overlay = Some(crate::models::ActiveOverlayPayload {
        kind: "live_quote".to_string(),
        bar,
    });
    payload
}

pub(crate) fn live_overlay_key(
    target_type: &str,
    target_id: &str,
    granularity: &str,
    board_algorithm: &str,
) -> String {
    format!(
        "overlay:{}:{}:{}:{}",
        target_type, target_id, granularity, board_algorithm
    )
}

fn normalize_granularity(value: Option<&str>) -> AppResult<&'static str> {
    match value.unwrap_or("day") {
        "day" => Ok("day"),
        "week" => Ok("week"),
        other => Err(AppError::Message(format!(
            "unsupported granularity: {other}"
        ))),
    }
}

fn normalize_range(value: Option<&str>) -> AppResult<&'static str> {
    match value.unwrap_or("all") {
        "1m" => Ok("1m"),
        "3m" => Ok("3m"),
        "6m" => Ok("6m"),
        "1y" => Ok("1y"),
        "3y" => Ok("3y"),
        "all" => Ok("all"),
        other => Err(AppError::Message(format!("unsupported range: {other}"))),
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
