use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};
use std::time::Instant;
use uuid::Uuid;

use crate::errors::{classify_error_code, AppError, AppResult};
use crate::models::{DailyBarRecord, SymbolRecord, SyncStatusPayload};
use crate::repositories::{now_string, Database, SyncJobRecord, SyncStateRecord};
use crate::secret_store;
use crate::telemetry;

use super::market_data::{MarketDataTarget, ProviderBar, ProviderSecurity};
use super::AppRuntime;

const CURRENT_BAR_ADJUSTMENT_POLICY: &str = "forward_adjust_v1";
const MIN_HISTORY_BACKFILL_DAYS: i64 = 366 * 3;

#[derive(Clone)]
pub struct SyncService {
    database: Database,
    runtime: AppRuntime,
}

impl SyncService {
    pub fn new(database: Database, runtime: AppRuntime) -> Self {
        Self { database, runtime }
    }

    pub async fn run(&self, mode: &str) -> AppResult<SyncStatusPayload> {
        let started = Instant::now();
        self.database.bootstrap()?;
        telemetry::emit("sync_started", &[("mode", mode.to_string())]);

        let provider = self.runtime.provider()?;
        if !provider.is_available() {
            telemetry::emit(
                "sync_skipped",
                &[("reason", "provider_unavailable".to_string())],
            );
            return self.current_status();
        }

        let had_fixture_data = self.database.has_fixture_bars()?;
        if had_fixture_data {
            self.database.purge_fixture_data()?;
        }
        self.database.cleanup_fixture_orphan_symbols()?;
        let has_data = self.database.has_any_daily_bars()?;
        let requires_adjustment_refresh = self.database.current_bar_adjustment_policy()?.as_deref()
            != Some(CURRENT_BAR_ADJUSTMENT_POLICY);
        let invalidated_board_ids = if requires_adjustment_refresh || had_fixture_data {
            self.database
                .list_boards()?
                .into_iter()
                .map(|board| board.board_id)
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let started_at = now_string();
        let job_id = format!("sync-{}", Uuid::new_v4().simple());
        self.database.insert_sync_job(&SyncJobRecord {
            job_id: job_id.clone(),
            mode: mode.to_string(),
            status: "running".to_string(),
            started_at: started_at.clone(),
            finished_at: None,
            summary_json: None,
            error_json: None,
        })?;

        let latest_trade_date = match provider.latest_trade_date("US").await {
            Ok(date) => date,
            Err(error) => {
                telemetry::emit(
                    "sync_failed",
                    &[("mode", mode.to_string()), ("error", error_message(&error))],
                );
                self.database.finish_sync_job(
                    &job_id,
                    "failed",
                    Some(now_string()),
                    None,
                    Some(format!(r#"{{"message":"{}"}}"#, error_message(&error))),
                )?;
                return self.current_status();
            }
        };
        let initial_history_start_date = initial_history_start_date(&latest_trade_date);

        let targets = self.database.list_symbols_for_sync()?;
        let static_targets: Vec<MarketDataTarget> = targets
            .iter()
            .filter(|target| needs_static_info_refresh(&target.updated_at))
            .map(to_market_target)
            .collect();
        let static_info = provider
            .fetch_static_info(&static_targets)
            .await
            .unwrap_or_default();
        if !static_info.is_empty() {
            self.database
                .save_symbols(&map_provider_securities(&static_info))?;
        }

        let mut fetched_bars = Vec::new();
        let mut updated_targets = Vec::new();
        let mut replace_targets = Vec::new();
        let mut sync_state_rows = Vec::new();
        let mut failures = Vec::new();

        for target in targets {
            let market_target = to_market_target(&target);
            let latest_local_date = self.database.latest_bar_date(&target.target_id)?;
            let earliest_local_date = self.database.earliest_bar_date(&target.target_id)?;
            let fixture_only_bars = self
                .database
                .target_uses_only_fixture_bars(&target.target_id)?;
            let history_window_incomplete = history_window_incomplete(
                earliest_local_date.as_deref(),
                initial_history_start_date.as_deref(),
            );
            let force_full_refresh = requires_adjustment_refresh || history_window_incomplete;
            let next_start_date = latest_local_date.as_deref().and_then(next_trade_date);
            let is_up_to_date = !fixture_only_bars
                && !force_full_refresh
                && (latest_local_date
                    .as_deref()
                    .is_some_and(|date| date >= latest_trade_date.as_str())
                    || next_start_date
                        .as_deref()
                        .is_some_and(|date| date > latest_trade_date.as_str()));

            if is_up_to_date {
                sync_state_rows.push(SyncStateRecord {
                    target_type: target.target_type.clone(),
                    target_id: target.target_id.clone(),
                    latest_trade_date: Some(latest_trade_date.clone()),
                    last_sync_at: Some(started_at.clone()),
                    last_sync_status: "ready".to_string(),
                    last_error_code: None,
                    last_error_message: None,
                });
                continue;
            }

            let start_date = if fixture_only_bars || force_full_refresh {
                initial_history_start_date.clone()
            } else {
                next_start_date.filter(|date| date.as_str() <= latest_trade_date.as_str())
            };

            telemetry::emit(
                "sync_target_range",
                &[
                    ("targetId", target.target_id.clone()),
                    (
                        "startDate",
                        start_date
                            .clone()
                            .unwrap_or_else(|| "full_backfill".to_string()),
                    ),
                    ("endDate", latest_trade_date.clone()),
                ],
            );

            match provider
                .fetch_daily_bars(&market_target, start_date.as_deref(), &latest_trade_date)
                .await
            {
                Ok(bars) => {
                    let normalized = map_provider_bars(&bars);
                    if !normalized.is_empty() {
                        if fixture_only_bars || force_full_refresh {
                            replace_targets.push(target.target_id.clone());
                        }
                        fetched_bars.extend(normalized);
                        updated_targets.push(target.target_id.clone());
                    }
                    sync_state_rows.push(SyncStateRecord {
                        target_type: target.target_type.clone(),
                        target_id: target.target_id.clone(),
                        latest_trade_date: Some(latest_trade_date.clone()),
                        last_sync_at: Some(started_at.clone()),
                        last_sync_status: "ready".to_string(),
                        last_error_code: None,
                        last_error_message: None,
                    });
                }
                Err(error) => {
                    failures.push(format!("{}: {}", target.target_id, error_message(&error)));
                    sync_state_rows.push(SyncStateRecord {
                        target_type: target.target_type.clone(),
                        target_id: target.target_id.clone(),
                        latest_trade_date: latest_local_date,
                        last_sync_at: Some(started_at.clone()),
                        last_sync_status: "failed".to_string(),
                        last_error_code: Some(classify_error_code(&error).to_string()),
                        last_error_message: Some(error_message(&error)),
                    });
                }
            }
        }

        let save_started = Instant::now();
        if requires_adjustment_refresh {
            self.database.delete_all_board_daily_bars()?;
        }
        self.database
            .delete_daily_bars_for_targets(&replace_targets)?;
        self.database
            .save_sync_batch(&fetched_bars, &sync_state_rows)?;

        // Board charts are stored as materialized results. When member bars update, invalidate the
        // materialization so the next get_chart(board) will rebuild with the latest data.
        let affected_board_ids = self
            .database
            .list_board_ids_by_member_targets(&updated_targets)?;
        self.database
            .delete_board_daily_bars_for_boards(&affected_board_ids)?;

        telemetry::emit(
            "sqlite_write_completed",
            &[
                ("path", "sync.save_sync_batch".to_string()),
                ("rows", sync_state_rows.len().to_string()),
                ("elapsedMs", save_started.elapsed().as_millis().to_string()),
            ],
        );
        let mut board_ids = invalidated_board_ids;
        for board_id in affected_board_ids {
            if !board_ids.contains(&board_id) {
                board_ids.push(board_id);
            }
        }
        self.runtime
            .invalidate_targets(&updated_targets, &board_ids);

        if failures.is_empty() {
            self.database
                .set_bar_adjustment_policy(CURRENT_BAR_ADJUSTMENT_POLICY)?;
        }

        let finished_at = now_string();
        if failures.is_empty() {
            telemetry::emit(
                "sync_completed",
                &[
                    ("mode", mode.to_string()),
                    ("updatedTargets", updated_targets.len().to_string()),
                    ("elapsedMs", started.elapsed().as_millis().to_string()),
                ],
            );
            self.database.finish_sync_job(
                &job_id,
                "succeeded",
                Some(finished_at),
                Some(format!(
                    r#"{{"updatedTargets":{},"latestTradeDate":"{}"}}"#,
                    updated_targets.len(),
                    latest_trade_date
                )),
                None,
            )?;
        } else {
            telemetry::emit(
                "sync_completed",
                &[
                    ("mode", mode.to_string()),
                    ("updatedTargets", updated_targets.len().to_string()),
                    ("failureCount", failures.len().to_string()),
                    ("elapsedMs", started.elapsed().as_millis().to_string()),
                ],
            );
            self.database.finish_sync_job(
                &job_id,
                "failed",
                Some(finished_at),
                Some(format!(
                    r#"{{"updatedTargets":{},"latestTradeDate":"{}"}}"#,
                    updated_targets.len(),
                    latest_trade_date
                )),
                Some(serde_json::to_string(&failures)?),
            )?;
        }

        let mut payload = self.current_status()?;
        payload.latest_trade_date = Some(latest_trade_date);
        payload.last_sync_at = Some(started_at);
        payload.status = if failures.is_empty() {
            "ready".to_string()
        } else {
            "sync_failed".to_string()
        };
        payload.message = if failures.is_empty() {
            format!(
                "{}同步完成，更新 {} 个标的",
                mode_label(mode),
                updated_targets.len()
            )
        } else {
            format!("{}同步部分失败：{}", mode_label(mode), failures.join("；"))
        };
        if !has_data && payload.latest_trade_date.is_none() {
            payload.status = "chart_empty".to_string();
        }
        Ok(payload)
    }

    pub fn current_status(&self) -> AppResult<SyncStatusPayload> {
        self.database.bootstrap()?;

        let has_credentials = secret_store::load_credentials()?.is_some();
        let has_data = self.database.has_any_daily_bars()?;
        let summary = self.database.get_sync_status_summary()?;
        let latest_job = self.database.latest_sync_job()?;

        let mut payload = SyncStatusPayload {
            status: "ready".to_string(),
            message: summary.message,
            last_sync_at: summary.last_sync_at,
            latest_trade_date: summary.latest_trade_date,
        };

        if let Some(job) = latest_job {
            match job.status.as_str() {
                "running" => {
                    payload.status = if has_data {
                        "incremental_sync_running".to_string()
                    } else {
                        "first_sync_running".to_string()
                    };
                    payload.message = match job.mode.as_str() {
                        "startup" => "启动同步进行中".to_string(),
                        "manual" => "手动同步进行中".to_string(),
                        _ => "同步进行中".to_string(),
                    };
                    return Ok(payload);
                }
                "failed" => {
                    payload.status = "sync_failed".to_string();
                    payload.message = "最近一次同步失败".to_string();
                    return Ok(payload);
                }
                _ => {}
            }
        }

        if !has_credentials {
            payload.status = if has_data {
                "offline_readable".to_string()
            } else {
                "no_credentials".to_string()
            };
            payload.message = if has_data {
                "未配置 Longbridge 凭证，当前为离线可读".to_string()
            } else {
                "请先配置 Longbridge 凭证".to_string()
            };
            return Ok(payload);
        }

        if !has_data {
            payload.status = "chart_empty".to_string();
            payload.message = "尚未同步到可用图表数据".to_string();
            return Ok(payload);
        }

        payload.status = "ready".to_string();
        payload.message = "同步状态正常".to_string();
        Ok(payload)
    }
}

fn to_market_target(target: &SymbolRecord) -> MarketDataTarget {
    MarketDataTarget {
        target_id: target.target_id.clone(),
        target_type: target.target_type.clone(),
        provider_symbol: if target.display_code.is_empty() {
            target.target_id.clone()
        } else {
            target.display_code.clone()
        },
    }
}

fn map_provider_securities(securities: &[ProviderSecurity]) -> Vec<SymbolRecord> {
    securities
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
        .collect()
}

fn map_provider_bars(bars: &[ProviderBar]) -> Vec<DailyBarRecord> {
    bars.iter()
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
        .collect()
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

fn error_message(error: &AppError) -> String {
    match error {
        AppError::Message(message) => message.clone(),
    }
}

fn mode_label(mode: &str) -> &'static str {
    match mode {
        "startup" => "启动",
        "manual" => "手动",
        _ => "本次",
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
