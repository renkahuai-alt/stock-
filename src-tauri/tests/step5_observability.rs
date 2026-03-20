use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use async_trait::async_trait;
use chrono::{Datelike, Duration, NaiveDate};
use new_stock_lib::errors::AppError;
use new_stock_lib::jobs::board_build;
use new_stock_lib::models::{GetChartPayload, SaveBoardPayload, StartChartWatchPayload};
use new_stock_lib::repositories::Database;
use new_stock_lib::services::market_data::{
    MarketDataProvider, MarketDataTarget, ProviderBar, ProviderMarketStatus, ProviderQuote,
    ProviderSecurity,
};
use new_stock_lib::services::{AppRuntime, BoardService, ChartService};
use new_stock_lib::telemetry;
use rusqlite::{params, Connection};
use tokio::time::{sleep, Duration as TokioDuration};
use uuid::Uuid;

fn telemetry_test_guard() -> MutexGuard<'static, ()> {
    static TELEMETRY_TEST_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    TELEMETRY_TEST_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .expect("telemetry test lock should succeed")
}

fn temp_db_path(test_name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "new-stock-step5-{test_name}-{}.sqlite3",
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

#[derive(Debug)]
struct WarmableProviderState {
    prewarm_calls: AtomicUsize,
}

#[derive(Clone, Debug)]
struct WarmableProvider {
    state: Arc<WarmableProviderState>,
}

impl WarmableProvider {
    fn new() -> Self {
        Self {
            state: Arc::new(WarmableProviderState {
                prewarm_calls: AtomicUsize::new(0),
            }),
        }
    }
}

#[async_trait]
impl MarketDataProvider for WarmableProvider {
    fn provider_name(&self) -> &'static str {
        "warmable-provider"
    }

    async fn latest_trade_date(&self, _market: &str) -> new_stock_lib::errors::AppResult<String> {
        Ok("2026-03-19".to_string())
    }

    async fn fetch_static_info(
        &self,
        _targets: &[MarketDataTarget],
    ) -> new_stock_lib::errors::AppResult<Vec<ProviderSecurity>> {
        Ok(Vec::new())
    }

    async fn fetch_daily_bars(
        &self,
        _target: &MarketDataTarget,
        _start_date: Option<&str>,
        _end_date: &str,
    ) -> new_stock_lib::errors::AppResult<Vec<ProviderBar>> {
        Ok(Vec::new())
    }

    async fn prewarm(&self) -> new_stock_lib::errors::AppResult<()> {
        self.state.prewarm_calls.fetch_add(1, Ordering::SeqCst);
        sleep(TokioDuration::from_millis(40)).await;
        Ok(())
    }
}

#[derive(Clone, Debug)]
struct RateLimitedProvider;

#[async_trait]
impl MarketDataProvider for RateLimitedProvider {
    fn provider_name(&self) -> &'static str {
        "rate-limited-provider"
    }

    async fn latest_trade_date(&self, _market: &str) -> new_stock_lib::errors::AppResult<String> {
        Ok("2026-03-19".to_string())
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
                security_type: "equity".to_string(),
                currency: Some("USD".to_string()),
                total_shares: Some(1_000_000.0),
                circulating_shares: Some(900_000.0),
                updated_at: chrono::Utc::now().to_rfc3339(),
            })
            .collect())
    }

    async fn fetch_daily_bars(
        &self,
        _target: &MarketDataTarget,
        _start_date: Option<&str>,
        _end_date: &str,
    ) -> new_stock_lib::errors::AppResult<Vec<ProviderBar>> {
        Err(AppError::Message(
            "Longbridge rate limited: http status 429".to_string(),
        ))
    }
}

#[derive(Debug)]
struct StaticBarProvider;

#[async_trait]
impl MarketDataProvider for StaticBarProvider {
    fn provider_name(&self) -> &'static str {
        "static-bar-provider"
    }

    async fn latest_trade_date(&self, _market: &str) -> new_stock_lib::errors::AppResult<String> {
        Ok("2026-03-18".to_string())
    }

    async fn fetch_static_info(
        &self,
        _targets: &[MarketDataTarget],
    ) -> new_stock_lib::errors::AppResult<Vec<ProviderSecurity>> {
        Ok(Vec::new())
    }

    async fn fetch_daily_bars(
        &self,
        target: &MarketDataTarget,
        _start_date: Option<&str>,
        end_date: &str,
    ) -> new_stock_lib::errors::AppResult<Vec<ProviderBar>> {
        Ok(generate_provider_bars(&target.target_id, end_date))
    }
}

#[derive(Debug)]
struct StaticQuoteProvider;

#[async_trait]
impl MarketDataProvider for StaticQuoteProvider {
    fn provider_name(&self) -> &'static str {
        "static-quote-provider"
    }

    async fn latest_trade_date(&self, _market: &str) -> new_stock_lib::errors::AppResult<String> {
        Ok("2026-03-19".to_string())
    }

    async fn fetch_static_info(
        &self,
        _targets: &[MarketDataTarget],
    ) -> new_stock_lib::errors::AppResult<Vec<ProviderSecurity>> {
        Ok(Vec::new())
    }

    async fn fetch_daily_bars(
        &self,
        _target: &MarketDataTarget,
        _start_date: Option<&str>,
        _end_date: &str,
    ) -> new_stock_lib::errors::AppResult<Vec<ProviderBar>> {
        Ok(Vec::new())
    }

    async fn fetch_realtime_quotes(
        &self,
        targets: &[MarketDataTarget],
    ) -> new_stock_lib::errors::AppResult<Vec<ProviderQuote>> {
        Ok(targets
            .iter()
            .map(|target| ProviderQuote {
                target_id: target.target_id.clone(),
                target_type: target.target_type.clone(),
                provider_symbol: target.provider_symbol.clone(),
                prev_close: 100.0,
                open: 101.0,
                high: 103.0,
                low: 99.0,
                close: 102.0,
                volume: Some(12_345.0),
                updated_at: chrono::Utc::now().to_rfc3339(),
                source_status: "live".to_string(),
            })
            .collect())
    }
}

fn generate_provider_bars(target_id: &str, latest_trade_date: &str) -> Vec<ProviderBar> {
    let seed = target_id
        .bytes()
        .fold(0u64, |accumulator, byte| accumulator + byte as u64) as f64;
    let mut dates = Vec::with_capacity(320);
    let mut current = NaiveDate::parse_from_str(latest_trade_date, "%Y-%m-%d")
        .expect("latest trade date should parse");

    while dates.len() < 320 {
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
                source: "static-bar-provider".to_string(),
                updated_at: chrono::Utc::now().to_rfc3339(),
            }
        })
        .collect()
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

#[test]
fn step5_telemetry_formats_event_lines() {
    let _guard = telemetry_test_guard();
    telemetry::clear_captured_lines();
    telemetry::emit(
        "chart_cache_hit",
        &[
            ("targetType", "symbol".to_string()),
            ("targetId", "AAPL".to_string()),
            ("range", "1y".to_string()),
        ],
    );

    let captured = telemetry::drain_captured_lines();
    assert_eq!(captured.len(), 1);
    let line = &captured[0];
    assert!(line.contains("event=chart_cache_hit"));
    assert!(line.contains("targetType=symbol"));
    assert!(line.contains("targetId=AAPL"));
    assert!(line.contains("range=1y"));
}

#[tokio::test]
async fn step5_chart_service_emits_cache_hit_and_miss_logs() {
    let _guard = telemetry_test_guard();
    telemetry::clear_captured_lines();
    let database = Database::at(temp_db_path("chart-cache-logs"));
    database.bootstrap().expect("bootstrap should succeed");
    insert_symbol_stub(&database, "AAPL", "symbol");

    let provider = StaticBarProvider;
    let runtime = AppRuntime::for_tests(Arc::new(provider));
    new_stock_lib::services::SyncService::new(database.clone(), runtime.clone())
        .run("manual")
        .await
        .expect("sync should succeed");

    let chart_service = ChartService::with_runtime(database, runtime);
    chart_service
        .get_chart(chart_request("symbol", "AAPL"))
        .expect("first chart load should succeed");
    chart_service
        .get_chart(chart_request("symbol", "AAPL"))
        .expect("second chart load should succeed");

    let captured = telemetry::drain_captured_lines().join("\n");
    assert!(captured.contains("event=chart_cache_miss"));
    assert!(captured.contains("event=chart_cache_hit"));
}

#[tokio::test]
async fn step5_spawn_provider_prewarm_deduplicates_and_logs_inflight() {
    let _guard = telemetry_test_guard();
    telemetry::clear_captured_lines();
    let provider = WarmableProvider::new();
    let provider_state = provider.state.clone();
    let runtime = AppRuntime::for_tests(Arc::new(provider));

    runtime.spawn_provider_prewarm();
    runtime.spawn_provider_prewarm();
    sleep(TokioDuration::from_millis(120)).await;

    assert_eq!(provider_state.prewarm_calls.load(Ordering::SeqCst), 1);

    let captured = telemetry::drain_captured_lines().join("\n");
    assert!(captured.contains("event=provider_prewarm_started"));
    assert!(captured.contains("event=provider_prewarm_skipped"));
    assert!(captured.contains("event=provider_prewarm_succeeded"));
}

#[tokio::test]
async fn step5_board_build_classifies_rate_limited_symbol_failures() {
    let database = Database::at(temp_db_path("board-build-rate-limit"));
    database.bootstrap().expect("bootstrap should succeed");
    for symbol in ["AAPL", "MSFT", "META"] {
        insert_symbol_stub(&database, symbol, "symbol");
    }

    let runtime = AppRuntime::for_tests(Arc::new(RateLimitedProvider));
    let response = BoardService::new(database.clone())
        .save(SaveBoardPayload {
            board_id: None,
            name: "RateLimited".to_string(),
            members: vec!["AAPL".to_string(), "MSFT".to_string(), "META".to_string()],
            composition_algorithm: "equal_weight_v1".to_string(),
        })
        .expect("board save should succeed");

    board_build::run(database.clone(), runtime, &response.board_id, None)
        .await
        .expect("board build should finish with failure state");

    let conn = Connection::open(database.path()).expect("db should open");
    let error_codes: Vec<String> = conn
        .prepare(
            "SELECT last_error_code
             FROM sync_state
             WHERE target_type = 'symbol'
             ORDER BY target_id ASC",
        )
        .expect("statement should prepare")
        .query_map([], |row| row.get::<_, Option<String>>(0))
        .expect("query should succeed")
        .collect::<Result<Vec<_>, _>>()
        .expect("rows should load")
        .into_iter()
        .flatten()
        .collect();

    assert_eq!(
        error_codes,
        vec![
            "rate_limited".to_string(),
            "rate_limited".to_string(),
            "rate_limited".to_string()
        ]
    );
}

#[tokio::test]
async fn step5_chart_watch_emits_start_and_stop_logs() {
    let _guard = telemetry_test_guard();
    telemetry::clear_captured_lines();

    let database = Database::at(temp_db_path("watch-logs"));
    database.bootstrap().expect("bootstrap should succeed");
    insert_symbol_stub(&database, "AAPL", "symbol");

    let runtime = AppRuntime::for_tests(Arc::new(StaticQuoteProvider)).with_market_status_override(
        ProviderMarketStatus {
            market: "US".to_string(),
            trade_date: "2026-03-19".to_string(),
            market_state: "open".to_string(),
        },
    );

    let status = new_stock_lib::jobs::chart_watch::start(
        database,
        runtime.clone(),
        StartChartWatchPayload {
            target_type: "symbol".to_string(),
            target_id: "AAPL".to_string(),
            granularity: Some("day".to_string()),
            board_algorithm: None,
        },
        Arc::new(|_| {}),
    )
    .await
    .expect("watch should start");
    assert!(status.started);
    sleep(TokioDuration::from_millis(30)).await;

    new_stock_lib::jobs::chart_watch::stop(runtime)
        .await
        .expect("stop should succeed");

    let captured = telemetry::drain_captured_lines().join("\n");
    assert!(captured.contains("event=chart_watch_start_requested"));
    assert!(captured.contains("event=chart_watch_started"));
    assert!(captured.contains("event=chart_watch_stopped"));
}
