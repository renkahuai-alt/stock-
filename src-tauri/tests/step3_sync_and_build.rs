use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{Datelike, Duration, NaiveDate};
use new_stock_lib::jobs::board_build;
use new_stock_lib::models::{
    BoardBuildStatusPayload, DailyBarRecord, GetChartPayload, SaveBoardPayload,
};
use new_stock_lib::repositories::Database;
use new_stock_lib::services::market_data::{
    MarketDataProvider, MarketDataTarget, ProviderBar, ProviderSecurity,
};
use new_stock_lib::services::{AppRuntime, BoardService, ChartService, SyncService};
use rusqlite::{params, Connection};
use tokio::time::{sleep, Duration as TokioDuration};
use uuid::Uuid;

fn temp_db_path(test_name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "new-stock-step3-{test_name}-{}.sqlite3",
        Uuid::new_v4()
    ));
    if path.exists() {
        let _ = fs::remove_file(&path);
    }
    path
}

fn chart_request(target_type: &str, target_id: &str) -> GetChartPayload {
    GetChartPayload {
        target_type: target_type.to_string(),
        target_id: target_id.to_string(),
        granularity: Some("day".to_string()),
        range: Some("1y".to_string()),
        board_algorithm: Some("equal_weight_v1".to_string()),
    }
}

fn chart_request_with_range(target_type: &str, target_id: &str, range: &str) -> GetChartPayload {
    GetChartPayload {
        target_type: target_type.to_string(),
        target_id: target_id.to_string(),
        granularity: Some("day".to_string()),
        range: Some(range.to_string()),
        board_algorithm: Some("equal_weight_v1".to_string()),
    }
}

fn insert_symbol_stub(database: &Database, target_id: &str, target_type: &str) {
    let conn = Connection::open(database.path()).expect("db should open");
    conn.execute(
        "INSERT INTO symbols (
            target_id, target_type, display_code, name, market, security_type,
            currency, total_shares, circulating_shares, updated_at
         ) VALUES (?1, ?2, ?3, ?4, 'US', 'equity', 'USD', 1000000, 900000, ?5)
         ON CONFLICT(target_id) DO UPDATE SET
            target_type = excluded.target_type,
            display_code = excluded.display_code,
            name = excluded.name,
            market = excluded.market,
            security_type = excluded.security_type,
            currency = excluded.currency,
            total_shares = excluded.total_shares,
            circulating_shares = excluded.circulating_shares,
            updated_at = excluded.updated_at",
        params![
            target_id,
            target_type,
            target_id,
            format!("{target_id} Corp"),
            chrono::Utc::now().to_rfc3339()
        ],
    )
    .expect("symbol stub should insert");
}

fn insert_legacy_history(
    database: &Database,
    target_id: &str,
    latest_trade_date: &str,
    trading_days: usize,
) {
    insert_history_with_source(
        database,
        target_id,
        latest_trade_date,
        trading_days,
        "legacy_backfill",
    );
}

fn insert_history_with_source(
    database: &Database,
    target_id: &str,
    latest_trade_date: &str,
    trading_days: usize,
    source: &str,
) {
    let bars = generate_provider_bars(target_id, latest_trade_date, trading_days)
        .into_iter()
        .map(|bar| DailyBarRecord {
            target_id: bar.target_id,
            trade_date: bar.trade_date,
            open: bar.open,
            high: bar.high,
            low: bar.low,
            close: bar.close,
            volume: bar.volume,
            source: source.to_string(),
            updated_at: bar.updated_at,
        })
        .collect::<Vec<_>>();
    database
        .save_daily_bars(&bars)
        .expect("legacy history should persist");
}

#[derive(Debug)]
struct TestProviderState {
    latest_trade_date: Mutex<String>,
    delay_millis: u64,
    history_len: usize,
    history_calls: AtomicUsize,
    concurrent_calls: AtomicUsize,
    max_concurrent_calls: AtomicUsize,
}

impl TestProviderState {
    fn new(latest_trade_date: &str, delay_millis: u64, history_len: usize) -> Self {
        Self {
            latest_trade_date: Mutex::new(latest_trade_date.to_string()),
            delay_millis,
            history_len,
            history_calls: AtomicUsize::new(0),
            concurrent_calls: AtomicUsize::new(0),
            max_concurrent_calls: AtomicUsize::new(0),
        }
    }

    fn set_latest_trade_date(&self, latest_trade_date: &str) {
        *self.latest_trade_date.lock().expect("lock should succeed") =
            latest_trade_date.to_string();
    }

    fn latest_trade_date(&self) -> String {
        self.latest_trade_date
            .lock()
            .expect("lock should succeed")
            .clone()
    }
}

#[derive(Clone, Debug)]
struct TestProvider {
    state: Arc<TestProviderState>,
}

impl TestProvider {
    fn new(latest_trade_date: &str, delay_millis: u64) -> Self {
        Self::with_history_len(latest_trade_date, delay_millis, 900)
    }

    fn with_history_len(latest_trade_date: &str, delay_millis: u64, history_len: usize) -> Self {
        Self {
            state: Arc::new(TestProviderState::new(
                latest_trade_date,
                delay_millis,
                history_len,
            )),
        }
    }
}

#[async_trait]
impl MarketDataProvider for TestProvider {
    fn provider_name(&self) -> &'static str {
        "test-provider"
    }

    async fn latest_trade_date(&self, _market: &str) -> new_stock_lib::errors::AppResult<String> {
        Ok(self.state.latest_trade_date())
    }

    async fn fetch_static_info(
        &self,
        targets: &[MarketDataTarget],
    ) -> new_stock_lib::errors::AppResult<Vec<ProviderSecurity>> {
        Ok(targets
            .iter()
            .map(|target| ProviderSecurity {
                target_id: target.target_id.clone(),
                target_type: target.target_type.clone(),
                display_code: target.provider_symbol.clone(),
                name: format!("{} Corp", target.target_id),
                market: Some("US".to_string()),
                security_type: if target.target_type == "index" {
                    "index".to_string()
                } else {
                    "equity".to_string()
                },
                currency: Some("USD".to_string()),
                total_shares: Some(1_000_000.0),
                circulating_shares: Some(900_000.0),
                updated_at: chrono::Utc::now().to_rfc3339(),
            })
            .collect())
    }

    async fn fetch_daily_bars(
        &self,
        target: &MarketDataTarget,
        start_date: Option<&str>,
        end_date: &str,
    ) -> new_stock_lib::errors::AppResult<Vec<ProviderBar>> {
        self.state.history_calls.fetch_add(1, Ordering::SeqCst);
        let concurrent_now = self.state.concurrent_calls.fetch_add(1, Ordering::SeqCst) + 1;
        let current_max = self.state.max_concurrent_calls.load(Ordering::SeqCst);
        if concurrent_now > current_max {
            self.state
                .max_concurrent_calls
                .store(concurrent_now, Ordering::SeqCst);
        }

        if self.state.delay_millis > 0 {
            sleep(TokioDuration::from_millis(self.state.delay_millis)).await;
        }

        let all_bars = generate_provider_bars(&target.target_id, end_date, self.state.history_len);
        let filtered = match start_date {
            Some(start_date) => all_bars
                .into_iter()
                .filter(|bar| bar.trade_date.as_str() >= start_date)
                .collect(),
            None => all_bars,
        };

        self.state.concurrent_calls.fetch_sub(1, Ordering::SeqCst);
        Ok(filtered)
    }
}

fn generate_provider_bars(
    target_id: &str,
    latest_trade_date: &str,
    history_len: usize,
) -> Vec<ProviderBar> {
    let seed = target_id
        .bytes()
        .fold(0u64, |accumulator, byte| accumulator + byte as u64) as f64;
    let mut dates = Vec::with_capacity(history_len);
    let mut current = NaiveDate::parse_from_str(latest_trade_date, "%Y-%m-%d")
        .expect("latest trade date should parse");

    while dates.len() < history_len {
        if current.weekday().number_from_monday() <= 5 {
            dates.push(current.format("%Y-%m-%d").to_string());
        }
        current -= Duration::days(1);
    }
    dates.reverse();

    let mut previous_close = 100.0 + (seed % 25.0);
    dates
        .into_iter()
        .enumerate()
        .map(|(index, trade_date)| {
            let wave = ((index as f64 / 9.0).sin() + (index as f64 / 17.0).cos()) * 1.4;
            let drift = index as f64 * 0.03 + (seed % 7.0) * 0.01;
            let open = round2((previous_close + wave * 0.35).max(5.0));
            let close = round2((open + drift * 0.08 + wave * 0.25).max(5.0));
            let high = round2(open.max(close) + 0.9);
            let low = round2((open.min(close) - 0.9).max(1.0));
            previous_close = close;

            ProviderBar {
                target_id: target_id.to_string(),
                trade_date,
                open,
                high,
                low,
                close,
                volume: Some(1_000_000.0 + seed * 100.0 + index as f64 * 250.0),
                source: "test-provider".to_string(),
                updated_at: chrono::Utc::now().to_rfc3339(),
            }
        })
        .collect()
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

#[tokio::test]
async fn step3_manual_sync_persists_remote_data_and_sync_job() {
    let database = Database::at(temp_db_path("manual-sync"));
    database.bootstrap().expect("bootstrap should succeed");
    insert_symbol_stub(&database, "AAPL", "symbol");

    let provider = TestProvider::new("2026-03-18", 0);
    let runtime = AppRuntime::for_tests(Arc::new(provider));
    let payload = SyncService::new(database.clone(), runtime)
        .run("manual")
        .await
        .expect("manual sync should succeed");

    assert_eq!(payload.status, "ready");
    assert_eq!(payload.latest_trade_date.as_deref(), Some("2026-03-18"));
    assert!(
        database.count_bars_for_target("AAPL").expect("bar count") >= 252,
        "manual sync should backfill at least one year of daily bars"
    );

    let conn = Connection::open(database.path()).expect("db should open");
    let sync_job_status: String = conn
        .query_row(
            "SELECT status FROM sync_jobs ORDER BY started_at DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .expect("sync job should exist");
    assert_eq!(sync_job_status, "succeeded");

    let sync_state: (Option<String>, Option<String>) = conn
        .query_row(
            "SELECT latest_trade_date, last_sync_status
             FROM sync_state
             WHERE target_type = 'symbol' AND target_id = 'AAPL'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("sync state should exist");
    assert_eq!(sync_state.0.as_deref(), Some("2026-03-18"));
    assert_eq!(sync_state.1.as_deref(), Some("ready"));
}

#[tokio::test]
async fn step3_manual_sync_replaces_fixture_symbol_bars_with_real_history() {
    let database = Database::at(temp_db_path("replace-fixture-bars"));
    database.bootstrap().expect("bootstrap should succeed");
    database
        .purge_fixture_data()
        .expect("fixture data should be removable for this regression test");
    insert_symbol_stub(&database, "NVDA", "symbol");
    insert_history_with_source(&database, "NVDA", "2026-03-18", 780, "fixture_symbol");

    let fixture_bars = database
        .list_daily_bars("NVDA")
        .expect("fixture bars should exist");
    assert_eq!(
        fixture_bars.len(),
        780,
        "fixture should seed full placeholder history"
    );
    assert!(
        fixture_bars
            .iter()
            .all(|bar| bar.source == "fixture_symbol"),
        "precondition: NVDA should start from fixture bars"
    );

    let provider = TestProvider::new("2026-03-18", 0);
    let runtime = AppRuntime::for_tests(Arc::new(provider));
    let payload = SyncService::new(database.clone(), runtime)
        .run("manual")
        .await
        .expect("manual sync should succeed");

    assert_eq!(payload.status, "ready");

    let synced_bars = database
        .list_daily_bars("NVDA")
        .expect("NVDA bars should remain readable");
    assert!(
        synced_bars.len() >= 750,
        "fixture bars should be fully replaced by at least three years of provider history"
    );
    assert!(
        synced_bars.iter().all(|bar| bar.source == "test-provider"),
        "all persisted NVDA bars should come from the real provider after sync"
    );
}

#[tokio::test]
async fn step3_background_board_build_fetches_members_and_persists_final_status() {
    let database = Database::at(temp_db_path("board-build"));
    database.bootstrap().expect("bootstrap should succeed");
    for symbol in ["AAPL", "MSFT", "META", "AMZN", "TSLA", "NFLX"] {
        insert_symbol_stub(&database, symbol, "symbol");
    }

    let provider = TestProvider::new("2026-03-18", 20);
    let provider_state = provider.state.clone();
    let runtime = AppRuntime::for_tests(Arc::new(provider));
    let events = Arc::new(Mutex::new(Vec::<BoardBuildStatusPayload>::new()));

    let response = BoardService::new(database.clone())
        .save(SaveBoardPayload {
            board_id: None,
            name: "MegaCap".to_string(),
            members: vec![
                "AAPL".to_string(),
                "MSFT".to_string(),
                "META".to_string(),
                "AMZN".to_string(),
                "TSLA".to_string(),
                "NFLX".to_string(),
            ],
            composition_algorithm: "equal_weight_v1".to_string(),
        })
        .expect("board should be queued for background build");

    assert!(response.background_sync_started);
    assert_eq!(response.build_status, "queued");

    board_build::run(
        database.clone(),
        runtime.clone(),
        &response.board_id,
        Some(Arc::new({
            let events = events.clone();
            move |payload| {
                events
                    .lock()
                    .expect("events lock should succeed")
                    .push(payload);
            }
        })),
    )
    .await
    .expect("board build should succeed");

    let status = BoardService::new(database.clone())
        .get_build_status(&response.board_id)
        .expect("build status should load");
    assert_eq!(status.build_status, "succeeded");
    assert_eq!(status.build_phase, "completed");
    assert_eq!(status.build_total, 6);
    assert_eq!(status.build_completed, 6);
    assert_eq!(status.build_failed, 0);

    let chart = ChartService::with_runtime(database.clone(), runtime)
        .get_chart(chart_request_with_range("board", &response.board_id, "all"))
        .expect("board chart should load");
    assert!(
        chart.bars.len() >= 750,
        "board chart should materialize at least three years of history"
    );
    assert_eq!(chart.source_status, "local_cache");

    let emitted = events.lock().expect("events lock should succeed");
    assert!(emitted
        .iter()
        .any(|payload| payload.build_phase == "fetching_history"));
    assert!(emitted
        .iter()
        .any(|payload| payload.build_phase == "persisting"));
    assert!(emitted
        .iter()
        .any(|payload| payload.build_phase == "completed" && payload.build_status == "succeeded"));
    let max_concurrent_calls = provider_state.max_concurrent_calls.load(Ordering::SeqCst);
    assert!(max_concurrent_calls <= 3);
    assert!(
        max_concurrent_calls >= 2,
        "batch fetching should use bounded parallelism"
    );

    for symbol in ["AAPL", "MSFT", "META", "AMZN", "TSLA", "NFLX"] {
        let bar_count = database
            .count_bars_for_target(symbol)
            .expect("bar count should load");
        assert!(
            bar_count >= 750,
            "board first build should backfill at least three years, got {bar_count} for {symbol}"
        );
    }
}

#[tokio::test]
async fn step3_manual_sync_backfills_insufficient_symbol_history_and_rebuilds_existing_board() {
    let database = Database::at(temp_db_path("sync-backfills-short-history"));
    database.bootstrap().expect("bootstrap should succeed");
    database
        .purge_fixture_data()
        .expect("fixture data should be removable for this regression test");
    database
        .set_bar_adjustment_policy("forward_adjust_v1")
        .expect("adjustment policy should be marked current for this regression test");
    for symbol in ["SHORTA", "SHORTB"] {
        insert_symbol_stub(&database, symbol, "symbol");
        insert_legacy_history(&database, symbol, "2026-03-18", 260);
    }

    let board = BoardService::new(database.clone())
        .save(SaveBoardPayload {
            board_id: None,
            name: "Legacy MegaBoard".to_string(),
            members: vec!["SHORTA".to_string(), "SHORTB".to_string()],
            composition_algorithm: "equal_weight_v1".to_string(),
        })
        .expect("legacy board should save through fast path");
    assert!(!board.background_sync_started);

    let chart_service = ChartService::with_runtime(database.clone(), AppRuntime::default());
    let chart_before = chart_service
        .get_chart(chart_request_with_range("board", &board.board_id, "all"))
        .expect("legacy board chart should load");
    assert!(
        chart_before.bars.len() < 300,
        "setup should simulate the old one-year board materialization"
    );

    let provider = TestProvider::new("2026-03-18", 0);
    let provider_state = provider.state.clone();
    let runtime = AppRuntime::for_tests(Arc::new(provider));
    SyncService::new(database.clone(), runtime.clone())
        .run("manual")
        .await
        .expect("manual sync should backfill missing history coverage");

    let symbol_chart = ChartService::with_runtime(database.clone(), runtime.clone())
        .get_chart(chart_request_with_range("symbol", "SHORTA", "all"))
        .expect("symbol chart should load after sync");
    assert!(
        symbol_chart.bars.len() >= 750,
        "manual sync should expand legacy symbol history to at least three years"
    );

    let rebuilt_board_chart = ChartService::with_runtime(database, runtime)
        .get_chart(chart_request_with_range("board", &board.board_id, "all"))
        .expect("board chart should rebuild after affected members are backfilled");
    assert!(
        rebuilt_board_chart.bars.len() >= 750,
        "existing board history should rebuild to at least three years after sync"
    );
    assert!(
        provider_state.history_calls.load(Ordering::SeqCst) >= 2,
        "manual sync should refetch symbols whose local history coverage is too short"
    );
}

#[tokio::test]
async fn step3_save_board_allows_missing_local_symbols_and_background_build_fetches_them() {
    let database = Database::at(temp_db_path("board-build-missing-symbols"));
    database.bootstrap().expect("bootstrap should succeed");

    let missing_members = vec![
        "MU".to_string(),
        "SNDK".to_string(),
        "WDC".to_string(),
        "STX".to_string(),
    ];
    let local_before = database
        .list_symbols_by_ids(&missing_members)
        .expect("symbol query should succeed");
    assert!(
        local_before.is_empty(),
        "regression setup expects symbols to be unavailable locally"
    );

    let provider = TestProvider::new("2026-03-18", 0);
    let runtime = AppRuntime::for_tests(Arc::new(provider));

    let response = BoardService::new(database.clone())
        .save(SaveBoardPayload {
            board_id: None,
            name: "存储芯片".to_string(),
            members: missing_members.clone(),
            composition_algorithm: "market_cap_weight_v1".to_string(),
        })
        .expect("missing local symbols should queue background build instead of failing");

    assert!(response.background_sync_started);
    assert_eq!(response.build_status, "queued");
    assert_eq!(response.build_phase, "queued");

    board_build::run(database.clone(), runtime, &response.board_id, None)
        .await
        .expect("board build should succeed with fetched static info");

    let status = BoardService::new(database.clone())
        .get_build_status(&response.board_id)
        .expect("build status should load");
    assert_eq!(status.build_status, "succeeded");
    assert_eq!(status.build_phase, "completed");
    assert_eq!(status.build_total, 4);
    assert_eq!(status.build_completed, 4);

    let local_after = database
        .list_symbols_by_ids(&missing_members)
        .expect("symbol query should succeed");
    assert_eq!(
        local_after.len(),
        missing_members.len(),
        "background build should backfill missing symbol rows"
    );

    let chart = ChartService::with_runtime(database, AppRuntime::default())
        .get_chart(GetChartPayload {
            target_type: "board".to_string(),
            target_id: response.board_id,
            granularity: Some("day".to_string()),
            range: Some("1y".to_string()),
            board_algorithm: Some("market_cap_weight_v1".to_string()),
        })
        .expect("board chart should load after build");
    assert!(!chart.bars.is_empty());
    assert_eq!(chart.source_status, "local_cache");
}

#[test]
fn step3_recover_interrupted_running_board_builds() {
    let database = Database::at(temp_db_path("recover-interrupted"));
    database.bootstrap().expect("bootstrap should succeed");

    let conn = Connection::open(database.path()).expect("db should open");
    conn.execute(
        "INSERT INTO boards (
            board_id, name, sort_order, composition_algorithm,
            build_status, build_phase, build_total, build_completed, build_failed,
            build_job_id, build_message, build_started_at, build_finished_at,
            created_at, updated_at
         ) VALUES (
            'board-recover', 'Recover Me', 99, 'equal_weight_v1',
            'running', 'fetching_history', 6, 2, 0,
            'job-recover', 'stale running job', '2026-03-18T10:00:00Z', NULL,
            '2026-03-18T10:00:00Z', '2026-03-18T10:05:00Z'
         )",
        [],
    )
    .expect("board should insert");

    let recovered = board_build::recover_stale(&database).expect("recovery should succeed");
    assert_eq!(recovered, 1);

    let status = BoardService::new(database)
        .get_build_status("board-recover")
        .expect("board status should load");
    assert_eq!(status.build_status, "failed");
    assert_eq!(status.build_phase, "failed");
    assert!(status
        .build_message
        .as_deref()
        .unwrap_or_default()
        .contains("应用中断"));
}

#[tokio::test]
async fn step3_manual_sync_invalidates_chart_cache_for_affected_symbol() {
    let database = Database::at(temp_db_path("cache-invalidation"));
    database.bootstrap().expect("bootstrap should succeed");
    insert_symbol_stub(&database, "AAPL", "symbol");

    let provider = TestProvider::new("2026-03-18", 0);
    let provider_state = provider.state.clone();
    let runtime = AppRuntime::for_tests(Arc::new(provider));
    let sync_service = SyncService::new(database.clone(), runtime.clone());
    sync_service
        .run("manual")
        .await
        .expect("initial sync should succeed");

    let chart_service = ChartService::with_runtime(database.clone(), runtime.clone());
    let first_chart = chart_service
        .get_chart(chart_request("symbol", "AAPL"))
        .expect("first chart should load");
    assert_eq!(first_chart.latest_trade_date.as_deref(), Some("2026-03-18"));

    provider_state.set_latest_trade_date("2026-03-19");
    sync_service
        .run("manual")
        .await
        .expect("second sync should succeed");

    let refreshed_chart = chart_service
        .get_chart(chart_request("symbol", "AAPL"))
        .expect("refreshed chart should load");
    assert_eq!(
        refreshed_chart.latest_trade_date.as_deref(),
        Some("2026-03-19")
    );
}

#[tokio::test]
async fn step3_sync_clears_board_materialization_when_member_bars_update() {
    let database = Database::at(temp_db_path("sync-clears-board-bars"));
    database.bootstrap().expect("bootstrap should succeed");
    for symbol in ["AAPL", "MSFT"] {
        insert_symbol_stub(&database, symbol, "symbol");
    }

    let provider = TestProvider::new("2026-03-18", 0);
    let provider_state = provider.state.clone();
    let runtime = AppRuntime::for_tests(Arc::new(provider));

    SyncService::new(database.clone(), runtime.clone())
        .run("manual")
        .await
        .expect("initial sync should succeed");

    let board = BoardService::new(database.clone())
        .save(SaveBoardPayload {
            board_id: None,
            name: "MegaBoard".to_string(),
            members: vec!["AAPL".to_string(), "MSFT".to_string()],
            composition_algorithm: "equal_weight_v1".to_string(),
        })
        .expect("board should save via fast path");
    assert!(!board.background_sync_started);

    let chart_service = ChartService::with_runtime(database.clone(), runtime.clone());
    let chart_before = chart_service
        .get_chart(chart_request("board", &board.board_id))
        .expect("board chart should load");
    assert_eq!(
        chart_before.latest_trade_date.as_deref(),
        Some("2026-03-18")
    );

    provider_state.set_latest_trade_date("2026-03-19");
    SyncService::new(database.clone(), runtime.clone())
        .run("startup")
        .await
        .expect("incremental sync should succeed");

    let chart_after = chart_service
        .get_chart(chart_request("board", &board.board_id))
        .expect("board chart should reload");
    assert_eq!(
        chart_after.latest_trade_date.as_deref(),
        Some("2026-03-19"),
        "board materialization should be cleared so get_chart(board) can rebuild with the latest bars"
    );
}

#[tokio::test]
async fn step3_sync_clears_market_cap_board_materialization_when_member_bars_update() {
    let database = Database::at(temp_db_path("sync-clears-market-cap-board-bars"));
    database.bootstrap().expect("bootstrap should succeed");
    for symbol in ["MU", "SNDK", "WDC", "STX"] {
        insert_symbol_stub(&database, symbol, "symbol");
    }

    let provider = TestProvider::new("2026-03-18", 0);
    let provider_state = provider.state.clone();
    let runtime = AppRuntime::for_tests(Arc::new(provider));

    SyncService::new(database.clone(), runtime.clone())
        .run("manual")
        .await
        .expect("initial sync should succeed");

    let board = BoardService::new(database.clone())
        .save(SaveBoardPayload {
            board_id: None,
            name: "StorageBoard".to_string(),
            members: vec![
                "MU".to_string(),
                "SNDK".to_string(),
                "WDC".to_string(),
                "STX".to_string(),
            ],
            composition_algorithm: "market_cap_weight_v1".to_string(),
        })
        .expect("board should save via fast path");
    assert!(!board.background_sync_started);

    let chart_service = ChartService::with_runtime(database.clone(), runtime.clone());
    let chart_before = chart_service
        .get_chart(GetChartPayload {
            target_type: "board".to_string(),
            target_id: board.board_id.clone(),
            granularity: Some("day".to_string()),
            range: Some("1y".to_string()),
            board_algorithm: Some("market_cap_weight_v1".to_string()),
        })
        .expect("market cap board chart should load");
    assert_eq!(
        chart_before.latest_trade_date.as_deref(),
        Some("2026-03-18")
    );

    provider_state.set_latest_trade_date("2026-03-19");
    SyncService::new(database.clone(), runtime.clone())
        .run("startup")
        .await
        .expect("incremental sync should succeed");

    let chart_after = chart_service
        .get_chart(GetChartPayload {
            target_type: "board".to_string(),
            target_id: board.board_id.clone(),
            granularity: Some("day".to_string()),
            range: Some("1y".to_string()),
            board_algorithm: Some("market_cap_weight_v1".to_string()),
        })
        .expect("market cap board chart should reload");
    assert_eq!(
        chart_after.latest_trade_date.as_deref(),
        Some("2026-03-19"),
        "market cap board materialization should also be cleared after startup sync"
    );
}

#[tokio::test]
async fn step3_manual_sync_skips_full_refetch_when_local_latest_trade_date_is_current() {
    let database = Database::at(temp_db_path("incremental-anchor"));
    database.bootstrap().expect("bootstrap should succeed");
    insert_symbol_stub(&database, "AAPL", "symbol");

    let provider = TestProvider::new("2026-03-18", 0);
    let provider_state = provider.state.clone();
    let runtime = AppRuntime::for_tests(Arc::new(provider));
    let sync_service = SyncService::new(database, runtime);

    sync_service
        .run("manual")
        .await
        .expect("initial sync should succeed");
    let history_calls_after_first_sync = provider_state.history_calls.load(Ordering::SeqCst);

    sync_service
        .run("manual")
        .await
        .expect("second sync should succeed");

    assert_eq!(
        provider_state.history_calls.load(Ordering::SeqCst),
        history_calls_after_first_sync,
        "when local latest_trade_date is already current, manual sync should not refetch history"
    );
}
