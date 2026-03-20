use std::sync::Arc;
use std::time::Instant;

use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use crate::errors::{classify_error_code, AppError, AppResult};
use crate::models::{BoardBuildStatusPayload, BoardRecord, DailyBarRecord, SymbolRecord};
use crate::repositories::{now_string, Database, SyncStateRecord};
use crate::services::market_data::MarketDataTarget;
use crate::services::AppRuntime;
use crate::telemetry;

pub type BoardBuildNotifier = Arc<dyn Fn(BoardBuildStatusPayload) + Send + Sync>;
const CURRENT_BAR_ADJUSTMENT_POLICY: &str = "forward_adjust_v1";
const MIN_HISTORY_BACKFILL_DAYS: i64 = 366 * 3;

pub async fn run(
    database: Database,
    runtime: AppRuntime,
    board_id: &str,
    notifier: Option<BoardBuildNotifier>,
) -> AppResult<()> {
    let started = Instant::now();
    database.bootstrap()?;
    let _permit = runtime
        .board_build_gate()
        .acquire_owned()
        .await
        .map_err(|error| AppError::Message(format!("board build gate closed: {error}")))?;

    let mut board = database
        .get_board(board_id)?
        .ok_or_else(|| AppError::Message(format!("board not found: {board_id}")))?;
    telemetry::emit(
        "board_build_started",
        &[
            ("boardId", board.board_id.clone()),
            ("boardName", board.name.clone()),
        ],
    );
    let members = database.list_board_members(board_id)?;
    let provider = runtime.provider()?;

    if !provider.is_available() {
        fail_board(
            &database,
            &mut board,
            "Longbridge credentials are not configured".to_string(),
            notifier.as_ref(),
        )?;
        return Ok(());
    }

    let now = now_string();
    board.build_status = "running".into();
    board.build_phase = "fetching_symbols".into();
    board.build_total = members.len();
    board.build_completed = 0;
    board.build_failed = 0;
    board.build_started_at = Some(board.build_started_at.clone().unwrap_or(now.clone()));
    board.build_finished_at = None;
    board.updated_at = now.clone();
    database.update_board_build_state(&board)?;
    emit(notifier.as_ref(), &board);

    let symbol_rows = database.list_symbols_by_ids(&members)?;
    let targets: Vec<MarketDataTarget> = members
        .iter()
        .filter_map(
            |member| match symbol_rows.iter().find(|row| row.target_id == *member) {
                Some(record) if !needs_static_info_refresh(&record.updated_at) => None,
                Some(record) => Some(to_market_target(record)),
                None => Some(MarketDataTarget {
                    target_id: member.clone(),
                    target_type: "symbol".into(),
                    provider_symbol: member.clone(),
                }),
            },
        )
        .collect();

    match provider.fetch_static_info(&targets).await {
        Ok(securities) => {
            if !securities.is_empty() {
                let symbols: Vec<SymbolRecord> = securities
                    .iter()
                    .map(|security| SymbolRecord {
                        target_id: security.target_id.clone(),
                        target_type: security.target_type.clone(),
                        display_code: security.display_code.clone(),
                        name: security.name.clone(),
                        market: security.market.clone(),
                        security_type: security.security_type.clone(),
                        currency: security.currency.clone(),
                        total_shares: security.total_shares,
                        circulating_shares: security.circulating_shares,
                        updated_at: security.updated_at.clone(),
                    })
                    .collect();
                database.save_symbols(&symbols)?;
            }
        }
        Err(error) => {
            fail_board(
                &database,
                &mut board,
                error_message(&error),
                notifier.as_ref(),
            )?;
            return Ok(());
        }
    }

    let latest_trade_date = match provider.latest_trade_date("US").await {
        Ok(date) => date,
        Err(error) => {
            fail_board(
                &database,
                &mut board,
                error_message(&error),
                notifier.as_ref(),
            )?;
            return Ok(());
        }
    };
    let requires_adjustment_refresh =
        database.current_bar_adjustment_policy()?.as_deref() != Some(CURRENT_BAR_ADJUSTMENT_POLICY);
    let minimum_history_start_date = initial_history_start_date(&latest_trade_date);

    board.build_phase = "fetching_history".into();
    board.updated_at = now_string();
    database.update_board_build_state(&board)?;
    emit(notifier.as_ref(), &board);

    let mut affected_targets = Vec::new();

    for chunk in members.chunks(5) {
        let batch_started = Instant::now();
        let concurrency_gate = Arc::new(Semaphore::new(3));
        let mut jobs = JoinSet::new();
        let mut batch_bars = Vec::<DailyBarRecord>::new();
        let mut batch_states = Vec::<SyncStateRecord>::new();
        let mut replace_targets = Vec::<String>::new();

        for member in chunk {
            let latest_local = database.latest_bar_date(member)?;
            let earliest_local = database.earliest_bar_date(member)?;
            let next_start_date = latest_local.as_deref().and_then(next_trade_date);

            let target = symbol_rows
                .iter()
                .find(|record| record.target_id == *member)
                .map(to_market_target)
                .unwrap_or_else(|| MarketDataTarget {
                    target_id: member.clone(),
                    target_type: "symbol".into(),
                    provider_symbol: member.clone(),
                });

            let history_window_incomplete = history_window_incomplete(
                earliest_local.as_deref(),
                minimum_history_start_date.as_deref(),
            );
            let force_full_refresh = requires_adjustment_refresh || history_window_incomplete;
            let is_up_to_date = !force_full_refresh
                && (latest_local
                    .as_deref()
                    .is_some_and(|date| date >= latest_trade_date.as_str())
                    || next_start_date
                        .as_deref()
                        .is_some_and(|date| date > latest_trade_date.as_str()));

            if is_up_to_date {
                board.build_completed += 1;
                batch_states.push(SyncStateRecord {
                    target_type: target.target_type.clone(),
                    target_id: target.target_id.clone(),
                    latest_trade_date: Some(latest_trade_date.clone()),
                    last_sync_at: Some(now_string()),
                    last_sync_status: "ready".into(),
                    last_error_code: None,
                    last_error_message: None,
                });
                continue;
            }

            let request = BoardHistoryFetchRequest {
                member_id: member.clone(),
                target,
                latest_local,
                replace_existing: force_full_refresh,
                start_date: if force_full_refresh {
                    minimum_history_start_date.clone()
                } else {
                    next_start_date.or_else(|| minimum_history_start_date.clone())
                },
            };
            let provider = provider.clone();
            let latest_trade_date = latest_trade_date.clone();
            let concurrency_gate = concurrency_gate.clone();
            jobs.spawn(async move {
                let _permit = concurrency_gate.acquire_owned().await.map_err(|error| {
                    AppError::Message(format!("history batch gate closed: {error}"))
                })?;
                let result = provider
                    .fetch_daily_bars(
                        &request.target,
                        request.start_date.as_deref(),
                        &latest_trade_date,
                    )
                    .await;
                Ok::<_, AppError>((request, result))
            });
        }

        telemetry::emit(
            "board_build_batch_started",
            &[
                ("boardId", board.board_id.clone()),
                ("rawSymbolCount", chunk.len().to_string()),
                ("scheduledSymbolCount", jobs.len().to_string()),
            ],
        );

        while let Some(result) = jobs.join_next().await {
            let (request, fetch_result) = result.map_err(|error| {
                AppError::Message(format!("history batch task join failed: {error}"))
            })??;

            match fetch_result {
                Ok(bars) => {
                    let normalized: Vec<DailyBarRecord> = bars
                        .iter()
                        .map(|bar| DailyBarRecord {
                            target_id: bar.target_id.clone(),
                            trade_date: bar.trade_date.clone(),
                            open: bar.open,
                            high: bar.high,
                            low: bar.low,
                            close: bar.close,
                            volume: bar.volume,
                            source: bar.source.clone(),
                            updated_at: bar.updated_at.clone(),
                        })
                        .collect();
                    if !normalized.is_empty() {
                        if request.replace_existing {
                            replace_targets.push(request.member_id.clone());
                        }
                        batch_bars.extend(normalized);
                        affected_targets.push(request.member_id.clone());
                    }
                    board.build_completed += 1;
                    batch_states.push(SyncStateRecord {
                        target_type: request.target.target_type.clone(),
                        target_id: request.target.target_id.clone(),
                        latest_trade_date: Some(latest_trade_date.clone()),
                        last_sync_at: Some(now_string()),
                        last_sync_status: "ready".into(),
                        last_error_code: None,
                        last_error_message: None,
                    });
                }
                Err(error) => {
                    board.build_failed += 1;
                    batch_states.push(SyncStateRecord {
                        target_type: request.target.target_type.clone(),
                        target_id: request.target.target_id.clone(),
                        latest_trade_date: request.latest_local,
                        last_sync_at: Some(now_string()),
                        last_sync_status: "failed".into(),
                        last_error_code: Some(classify_error_code(&error).into()),
                        last_error_message: Some(error_message(&error)),
                    });
                }
            }
        }

        let write_started = Instant::now();
        database.delete_daily_bars_for_targets(&replace_targets)?;
        database.save_sync_batch(&batch_bars, &batch_states)?;
        telemetry::emit(
            "sqlite_write_completed",
            &[
                ("path", "board_build.save_sync_batch".to_string()),
                ("rows", batch_states.len().to_string()),
                ("elapsedMs", write_started.elapsed().as_millis().to_string()),
            ],
        );
        telemetry::emit(
            "board_build_batch_completed",
            &[
                ("boardId", board.board_id.clone()),
                ("completed", board.build_completed.to_string()),
                ("failed", board.build_failed.to_string()),
                ("elapsedMs", batch_started.elapsed().as_millis().to_string()),
            ],
        );
        board.updated_at = now_string();
        database.update_board_build_state(&board)?;
        emit(notifier.as_ref(), &board);
    }

    board.build_phase = "recomputing_board".into();
    board.updated_at = now_string();
    database.update_board_build_state(&board)?;
    emit(notifier.as_ref(), &board);

    let available_members: Vec<String> = members
        .iter()
        .filter_map(|member| {
            database
                .count_bars_for_target(member)
                .ok()
                .filter(|count| *count > 0)
                .map(|_| member.clone())
        })
        .collect();

    if available_members.is_empty() {
        fail_board(
            &database,
            &mut board,
            "板块构建失败：无可用成分股历史数据".to_string(),
            notifier.as_ref(),
        )?;
        return Ok(());
    }

    let board_bars = crate::services::chart::compose_board_bars(
        &database,
        &available_members,
        &board.composition_algorithm,
    )?;
    if board_bars.is_empty() {
        fail_board(
            &database,
            &mut board,
            "板块构建失败：无法生成板块日线".to_string(),
            notifier.as_ref(),
        )?;
        return Ok(());
    }
    board.build_phase = "persisting".into();
    board.updated_at = now_string();
    database.update_board_build_state(&board)?;
    emit(notifier.as_ref(), &board);

    database.save_board_chart(&board.board_id, &board.composition_algorithm, &board_bars)?;
    runtime.invalidate_targets(&affected_targets, &[board.board_id.clone()]);

    board.build_status = "succeeded".into();
    board.build_phase = "completed".into();
    board.build_message = if board.build_failed > 0 {
        Some(format!(
            "{} 个成分股拉取失败，已使用可用数据完成构建",
            board.build_failed
        ))
    } else {
        None
    };
    board.build_finished_at = Some(now_string());
    board.updated_at = now_string();
    database.update_board_build_state(&board)?;
    emit(notifier.as_ref(), &board);
    telemetry::emit(
        "board_build_completed",
        &[
            ("boardId", board.board_id.clone()),
            ("buildStatus", board.build_status.clone()),
            ("buildFailed", board.build_failed.to_string()),
            ("elapsedMs", started.elapsed().as_millis().to_string()),
        ],
    );
    Ok(())
}

pub fn recover_stale(database: &Database) -> AppResult<usize> {
    database.recover_stale_board_builds()
}

fn fail_board(
    database: &Database,
    board: &mut BoardRecord,
    message: String,
    notifier: Option<&BoardBuildNotifier>,
) -> AppResult<()> {
    board.build_status = "failed".into();
    board.build_phase = "failed".into();
    board.build_message = Some(message);
    board.build_finished_at = Some(now_string());
    board.updated_at = now_string();
    database.update_board_build_state(board)?;
    emit(notifier, board);
    telemetry::emit(
        "board_build_failed",
        &[
            ("boardId", board.board_id.clone()),
            ("message", board.build_message.clone().unwrap_or_default()),
        ],
    );
    Ok(())
}

fn emit(notifier: Option<&BoardBuildNotifier>, board: &BoardRecord) {
    if let Some(notifier) = notifier {
        notifier(board.to_build_status());
    }
}

fn to_market_target(record: &SymbolRecord) -> MarketDataTarget {
    MarketDataTarget {
        target_id: record.target_id.clone(),
        target_type: record.target_type.clone(),
        provider_symbol: if record.display_code.is_empty() {
            record.target_id.clone()
        } else {
            record.display_code.clone()
        },
    }
}

fn next_trade_date(value: &str) -> Option<String> {
    let mut date = NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()?;
    loop {
        date += Duration::days(1);
        if date.weekday().number_from_monday() <= 5 {
            return Some(date.format("%Y-%m-%d").to_string());
        }
    }
}

fn error_message(error: &AppError) -> String {
    match error {
        AppError::Message(message) => message.clone(),
    }
}

#[derive(Debug)]
struct BoardHistoryFetchRequest {
    member_id: String,
    target: MarketDataTarget,
    latest_local: Option<String>,
    replace_existing: bool,
    start_date: Option<String>,
}

fn initial_history_start_date(latest_trade_date: &str) -> Option<String> {
    NaiveDate::parse_from_str(latest_trade_date, "%Y-%m-%d")
        .ok()
        .map(|date| {
            (date - Duration::days(MIN_HISTORY_BACKFILL_DAYS))
                .format("%Y-%m-%d")
                .to_string()
        })
}

fn history_window_incomplete(
    earliest_local_date: Option<&str>,
    initial_history_start_date: Option<&str>,
) -> bool {
    match (earliest_local_date, initial_history_start_date) {
        (_, None) => false,
        (None, Some(_)) => true,
        (Some(earliest_local_date), Some(initial_history_start_date)) => {
            earliest_local_date > initial_history_start_date
        }
    }
}

fn needs_static_info_refresh(updated_at: &str) -> bool {
    if updated_at.trim().is_empty() {
        return true;
    }

    DateTime::parse_from_rfc3339(updated_at)
        .map(|value| value.with_timezone(&Utc).date_naive() < Utc::now().date_naive())
        .unwrap_or(true)
}
