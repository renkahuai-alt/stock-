use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};

use async_trait::async_trait;
use chrono::{Datelike, Duration, NaiveDate};
use new_stock_lib::models::{GetChartPayload, SaveBoardPayload};
use new_stock_lib::repositories::Database;
use new_stock_lib::services::market_data::{
    MarketDataProvider, MarketDataTarget, ProviderBar, ProviderSecurity,
};
use new_stock_lib::services::{
    AppRuntime, BoardService, BootstrapService, ChartService, SyncService,
};
use rusqlite::{params, Connection};
use uuid::Uuid;

fn temp_db_path(test_name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "new-stock-step6-{test_name}-{}.sqlite3",
        Uuid::new_v4()
    ));
    if path.exists() {
        let _ = fs::remove_file(&path);
    }
    path
}

fn env_guard() -> MutexGuard<'static, ()> {
    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    ENV_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

struct ScopedEnvVar {
    key: &'static str,
    previous: Option<String>,
}

impl ScopedEnvVar {
    fn set(key: &'static str, value: &str) -> Self {
        let previous = std::env::var(key).ok();
        std::env::set_var(key, value);
        Self { key, previous }
    }
}

impl Drop for ScopedEnvVar {
    fn drop(&mut self) {
        match self.previous.as_deref() {
            Some(value) => std::env::set_var(self.key, value),
            None => std::env::remove_var(self.key),
        }
    }
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

fn insert_symbol_stub(database: &Database, target_id: &str, target_type: &str, name: &str) {
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
            name,
            chrono::Utc::now().to_rfc3339()
        ],
    )
    .expect("symbol stub should insert");
}

fn insert_daily_bars(database: &Database, target_id: &str, dates: &[&str], source: &str) {
    let conn = Connection::open(database.path()).expect("db should open");
    for (index, trade_date) in dates.iter().enumerate() {
        let base = 100.0 + index as f64;
        conn.execute(
            "INSERT INTO daily_bars (
                target_id, trade_date, open, high, low, close, volume, source, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(target_id, trade_date) DO UPDATE SET
                open = excluded.open,
                high = excluded.high,
                low = excluded.low,
                close = excluded.close,
                volume = excluded.volume,
                source = excluded.source,
                updated_at = excluded.updated_at",
            params![
                target_id,
                trade_date,
                base,
                base + 2.0,
                base - 2.0,
                base + 1.0,
                10_000.0 + index as f64,
                source,
                chrono::Utc::now().to_rfc3339()
            ],
        )
        .expect("daily bar should insert");
    }
}

fn board_daily_bar_count(database: &Database) -> i64 {
    let conn = Connection::open(database.path()).expect("db should open");
    conn.query_row("SELECT COUNT(*) FROM board_daily_bars", [], |row| {
        row.get(0)
    })
    .expect("board_daily_bars count should load")
}

fn fixture_bar_count(database: &Database) -> i64 {
    let conn = Connection::open(database.path()).expect("db should open");
    conn.query_row(
        "SELECT COUNT(*) FROM daily_bars WHERE source LIKE 'fixture_%'",
        [],
        |row| row.get(0),
    )
    .expect("fixture count should load")
}

fn emulate_legacy_fixture_purge(database: &Database) {
    let conn = Connection::open(database.path()).expect("db should open");
    conn.execute("DELETE FROM daily_bars WHERE source LIKE 'fixture_%'", [])
        .expect("fixture bars should delete");
    conn.execute("DELETE FROM board_daily_bars", [])
        .expect("board materialization should delete");
    conn.execute(
        "DELETE FROM sync_state
         WHERE NOT EXISTS (
            SELECT 1
            FROM daily_bars
            WHERE daily_bars.target_id = sync_state.target_id
         )",
        [],
    )
    .expect("orphan sync_state should delete");
}

#[derive(Debug, Clone)]
struct CutoverProvider;

#[derive(Debug, Default)]
struct TrackingProviderState {
    static_targets: Mutex<Vec<String>>,
    bar_targets: Mutex<Vec<String>>,
}

#[derive(Debug, Clone)]
struct TrackingProvider {
    state: Arc<TrackingProviderState>,
}

impl TrackingProvider {
    fn new() -> Self {
        Self {
            state: Arc::new(TrackingProviderState::default()),
        }
    }
}

#[async_trait]
impl MarketDataProvider for CutoverProvider {
    fn provider_name(&self) -> &'static str {
        "cutover-provider"
    }

    async fn latest_trade_date(&self, _market: &str) -> new_stock_lib::errors::AppResult<String> {
        Ok("2026-03-18".to_string())
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
        target: &MarketDataTarget,
        _start_date: Option<&str>,
        end_date: &str,
    ) -> new_stock_lib::errors::AppResult<Vec<ProviderBar>> {
        Ok(generate_provider_bars(&target.target_id, end_date))
    }
}

#[async_trait]
impl MarketDataProvider for TrackingProvider {
    fn provider_name(&self) -> &'static str {
        "tracking-provider"
    }

    async fn latest_trade_date(&self, _market: &str) -> new_stock_lib::errors::AppResult<String> {
        Ok("2026-03-18".to_string())
    }

    async fn fetch_static_info(
        &self,
        targets: &[MarketDataTarget],
    ) -> new_stock_lib::errors::AppResult<Vec<ProviderSecurity>> {
        self.state
            .static_targets
            .lock()
            .expect("lock should succeed")
            .extend(targets.iter().map(|target| target.target_id.clone()));
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
        target: &MarketDataTarget,
        _start_date: Option<&str>,
        end_date: &str,
    ) -> new_stock_lib::errors::AppResult<Vec<ProviderBar>> {
        self.state
            .bar_targets
            .lock()
            .expect("lock should succeed")
            .push(target.target_id.clone());
        Ok(generate_provider_bars(&target.target_id, end_date))
    }
}

fn generate_provider_bars(target_id: &str, latest_trade_date: &str) -> Vec<ProviderBar> {
    let end = NaiveDate::parse_from_str(latest_trade_date, "%Y-%m-%d")
        .expect("latest_trade_date should parse");
    let mut bars = Vec::new();

    for offset in 0..5 {
        let day = end - Duration::days((4 - offset) as i64);
        if day.weekday().number_from_monday() > 5 {
            continue;
        }
        let open = 200.0 + offset as f64;
        let close = open + 1.5;
        bars.push(ProviderBar {
            target_id: target_id.to_string(),
            trade_date: day.format("%Y-%m-%d").to_string(),
            open,
            high: close + 0.5,
            low: open - 0.5,
            close,
            volume: Some(50_000.0 + offset as f64),
            source: "cutover-provider".to_string(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        });
    }

    bars
}

#[test]
fn step6_bootstrap_without_fixture_still_returns_core_indexes() {
    let _lock = env_guard();
    let _fixture_guard = ScopedEnvVar::set("NEW_STOCK_DISABLE_DEV_FIXTURE", "1");
    let database = Database::at(temp_db_path("bootstrap-no-fixture"));
    database.bootstrap().expect("bootstrap should succeed");

    let payload = BootstrapService::new(database)
        .load()
        .expect("bootstrap payload");
    let ids = payload
        .indexes
        .into_iter()
        .map(|item| item.id)
        .collect::<Vec<_>>();

    assert_eq!(ids, vec!["DJI", "IXIC", "GSPC", "RUT"]);
}

#[test]
fn step6_purge_fixture_data_removes_fixture_bars_and_board_materialization() {
    let _lock = env_guard();
    let database = Database::at(temp_db_path("purge-fixture"));
    database.bootstrap().expect("bootstrap should succeed");

    let response = BoardService::new(database.clone())
        .save(SaveBoardPayload {
            board_id: None,
            name: "Fixture Board".to_string(),
            members: vec!["NVDA".to_string(), "AMD".to_string()],
            composition_algorithm: "equal_weight_v1".to_string(),
        })
        .expect("fixture board should save");
    assert!(!response.background_sync_started);
    assert!(fixture_bar_count(&database) > 0);
    assert!(board_daily_bar_count(&database) > 0);

    database
        .purge_fixture_data()
        .expect("fixture data should purge cleanly");

    assert_eq!(fixture_bar_count(&database), 0);
    assert_eq!(board_daily_bar_count(&database), 0);
}

#[tokio::test]
async fn step6_purge_fixture_data_removes_orphan_fixture_symbols_before_real_sync() {
    let _lock = env_guard();
    let database = Database::at(temp_db_path("purge-fixture-orphans"));
    database.bootstrap().expect("bootstrap should succeed");

    assert!(database
        .get_symbol("SIM01", "symbol")
        .expect("query should succeed")
        .is_some());
    assert!(database
        .get_symbol("EMPTY1", "symbol")
        .expect("query should succeed")
        .is_some());

    let provider = TrackingProvider::new();
    let state = provider.state.clone();
    let runtime = AppRuntime::for_tests(Arc::new(provider));
    let payload = SyncService::new(database.clone(), runtime)
        .run("manual")
        .await
        .expect("manual sync should succeed");

    assert_eq!(payload.status, "ready");
    assert!(database
        .get_symbol("SIM01", "symbol")
        .expect("query should succeed")
        .is_none());
    assert!(database
        .get_symbol("EMPTY1", "symbol")
        .expect("query should succeed")
        .is_none());

    let static_targets = state
        .static_targets
        .lock()
        .expect("lock should succeed")
        .clone();
    let bar_targets = state
        .bar_targets
        .lock()
        .expect("lock should succeed")
        .clone();
    assert!(
        !static_targets
            .iter()
            .any(|target| target.starts_with("SIM") || target.starts_with("EMPTY")),
        "fixture-only orphan symbols should not request static info: {static_targets:?}"
    );
    assert!(
        !bar_targets
            .iter()
            .any(|target| target.starts_with("SIM") || target.starts_with("EMPTY")),
        "fixture-only orphan symbols should not request history: {bar_targets:?}"
    );
}

#[tokio::test]
async fn step6_manual_sync_self_heals_legacy_fixture_orphan_symbols() {
    let _lock = env_guard();
    let database = Database::at(temp_db_path("legacy-fixture-orphans"));
    database.bootstrap().expect("bootstrap should succeed");
    emulate_legacy_fixture_purge(&database);

    assert_eq!(fixture_bar_count(&database), 0);
    assert!(database
        .get_symbol("SIM01", "symbol")
        .expect("query should succeed")
        .is_some());

    let provider = TrackingProvider::new();
    let state = provider.state.clone();
    let runtime = AppRuntime::for_tests(Arc::new(provider));
    let payload = SyncService::new(database.clone(), runtime)
        .run("manual")
        .await
        .expect("manual sync should succeed");

    assert_eq!(payload.status, "ready");
    assert!(database
        .get_symbol("SIM01", "symbol")
        .expect("query should succeed")
        .is_none());
    assert!(database
        .get_symbol("EMPTY1", "symbol")
        .expect("query should succeed")
        .is_none());

    let static_targets = state
        .static_targets
        .lock()
        .expect("lock should succeed")
        .clone();
    let bar_targets = state
        .bar_targets
        .lock()
        .expect("lock should succeed")
        .clone();
    assert!(
        !static_targets
            .iter()
            .any(|target| target.starts_with("SIM") || target.starts_with("EMPTY")),
        "legacy orphan symbols should not request static info: {static_targets:?}"
    );
    assert!(
        !bar_targets
            .iter()
            .any(|target| target.starts_with("SIM") || target.starts_with("EMPTY")),
        "legacy orphan symbols should not request history: {bar_targets:?}"
    );
}

#[tokio::test]
async fn step6_sync_replaces_legacy_real_bars_when_adjustment_policy_changes() {
    let _lock = env_guard();
    let _fixture_guard = ScopedEnvVar::set("NEW_STOCK_DISABLE_DEV_FIXTURE", "1");
    let database = Database::at(temp_db_path("adjustment-cutover"));
    database.bootstrap().expect("bootstrap should succeed");
    insert_symbol_stub(&database, "AAPL", "symbol", "Apple Inc");
    insert_daily_bars(
        &database,
        "AAPL",
        &["2026-03-14", "2026-03-17", "2026-03-18"],
        "legacy_no_adjust",
    );

    let runtime = AppRuntime::for_tests(Arc::new(CutoverProvider));
    SyncService::new(database.clone(), runtime)
        .run("manual")
        .await
        .expect("manual sync should succeed");

    let bars = database
        .list_daily_bars("AAPL")
        .expect("AAPL bars should remain readable");
    assert!(
        bars.iter().all(|bar| bar.source == "cutover-provider"),
        "legacy real bars should be fully replaced by refreshed history"
    );
    assert!(
        bars.len() >= 3,
        "refreshed history should still contain persisted bars"
    );
}

#[test]
fn step6_index_chart_meta_drops_fixture_provider_semantics() {
    let _lock = env_guard();
    let _fixture_guard = ScopedEnvVar::set("NEW_STOCK_DISABLE_DEV_FIXTURE", "1");
    let database = Database::at(temp_db_path("index-chart-meta"));
    database.bootstrap().expect("bootstrap should succeed");
    insert_daily_bars(
        &database,
        "DJI",
        &["2026-03-14", "2026-03-17", "2026-03-18"],
        "longbridge",
    );

    let chart = ChartService::new(database)
        .get_chart(chart_request("index", "DJI"))
        .expect("index chart should load");

    assert_eq!(chart.meta.provider_kind.as_deref(), Some("longbridge"));
    assert_eq!(chart.meta.value_mode, None);
}

#[tokio::test]
async fn step6_delete_board_removes_rows_and_invalidates_cached_chart() {
    let _lock = env_guard();
    let database = Database::at(temp_db_path("delete-board"));
    database.bootstrap().expect("bootstrap should succeed");
    let runtime = AppRuntime::default();

    let response = BoardService::new(database.clone())
        .save(SaveBoardPayload {
            board_id: None,
            name: "Delete Me".to_string(),
            members: vec!["NVDA".to_string(), "AMD".to_string()],
            composition_algorithm: "equal_weight_v1".to_string(),
        })
        .expect("board should save");
    assert!(!response.background_sync_started);

    let chart_service = ChartService::with_runtime(database.clone(), runtime.clone());
    let initial_chart = chart_service
        .get_chart(chart_request("board", &response.board_id))
        .expect("board chart should load");
    assert!(!initial_chart.bars.is_empty());

    BoardService::new(database.clone())
        .delete(&runtime, &response.board_id)
        .await
        .expect("board should delete");

    let conn = Connection::open(database.path()).expect("db should open");
    let board_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM boards WHERE board_id = ?1",
            params![response.board_id],
            |row| row.get(0),
        )
        .expect("board count should load");
    let member_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM board_members WHERE board_id = ?1",
            params![response.board_id],
            |row| row.get(0),
        )
        .expect("member count should load");
    let bar_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM board_daily_bars WHERE board_id = ?1",
            params![response.board_id],
            |row| row.get(0),
        )
        .expect("board bar count should load");

    assert_eq!(board_count, 0);
    assert_eq!(member_count, 0);
    assert_eq!(bar_count, 0);
    assert!(
        chart_service
            .get_chart(chart_request("board", &response.board_id))
            .is_err(),
        "cached board chart should be invalidated after delete"
    );
}
