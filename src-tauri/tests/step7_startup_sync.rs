use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use new_stock_lib::jobs::startup_sync;
use new_stock_lib::models::SaveBoardPayload;
use new_stock_lib::repositories::Database;
use new_stock_lib::services::market_data::{
    MarketDataProvider, MarketDataTarget, ProviderBar, ProviderSecurity,
};
use new_stock_lib::services::{AppRuntime, BoardService, ChartService};
use rusqlite::{params, Connection};
use tokio::sync::oneshot;
use uuid::Uuid;

fn temp_db_path(test_name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "new-stock-step7-{test_name}-{}.sqlite3",
        Uuid::new_v4()
    ));
    if path.exists() {
        let _ = fs::remove_file(&path);
    }
    path
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

#[test]
fn step7_app_setup_keeps_startup_sync_off_the_blocking_path() {
    let source = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/lib.rs"))
        .expect("lib.rs source should load");

    let window_index = source
        .find("app_shell::windowing::ensure_main_window")
        .expect("main window setup should exist");
    let startup_sync_index = source
        .find("jobs::startup_sync::spawn(")
        .expect("startup sync should be launched in the background");

    assert!(
        window_index < startup_sync_index,
        "main window should be ensured before startup sync is launched"
    );
    assert!(
        !source.contains("block_on(jobs::startup_sync::run"),
        "startup sync should not block Tauri setup"
    );
}

#[derive(Clone, Debug)]
struct StaticProvider;

#[async_trait]
impl MarketDataProvider for StaticProvider {
    fn provider_name(&self) -> &'static str {
        "static-provider"
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
        target: &MarketDataTarget,
        _start_date: Option<&str>,
        _end_date: &str,
    ) -> new_stock_lib::errors::AppResult<Vec<ProviderBar>> {
        Ok(vec![ProviderBar {
            target_id: target.target_id.clone(),
            trade_date: "2026-03-19".to_string(),
            open: 100.0,
            high: 101.0,
            low: 99.0,
            close: 100.5,
            volume: Some(12_345.0),
            source: "static-provider".to_string(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        }])
    }
}

#[tokio::test]
async fn step7_startup_sync_runs_and_notifies() {
    let database = Database::at(temp_db_path("startup-sync"));
    database.bootstrap().expect("bootstrap should succeed");
    insert_symbol_stub(&database, "AAPL", "symbol");

    let runtime = AppRuntime::for_tests(Arc::new(StaticProvider));
    let (tx, rx) = oneshot::channel();
    let tx = Arc::new(Mutex::new(Some(tx)));
    let _payload = startup_sync::run(
        database.clone(),
        runtime.clone(),
        Some(Arc::new(move |payload| {
            if let Some(tx) = tx.lock().expect("tx lock should succeed").take() {
                let _ = tx.send(payload);
            }
        })),
    )
    .await
    .expect("startup sync should succeed");

    let notified = rx.await.expect("notifier should fire");
    assert_eq!(notified.status, "ready");
    assert_eq!(notified.latest_trade_date.as_deref(), Some("2026-03-19"));

    let chart = ChartService::with_runtime(database.clone(), runtime)
        .get_chart(new_stock_lib::models::GetChartPayload {
            target_type: "symbol".to_string(),
            target_id: "AAPL".to_string(),
            granularity: Some("day".to_string()),
            range: Some("1y".to_string()),
            board_algorithm: None,
        })
        .expect("symbol chart should load");
    assert_eq!(chart.latest_trade_date.as_deref(), Some("2026-03-19"));

    let board = BoardService::new(database.clone())
        .save(SaveBoardPayload {
            board_id: None,
            name: "B".to_string(),
            members: vec!["AAPL".to_string()],
            composition_algorithm: "equal_weight_v1".to_string(),
        })
        .expect("board should save");
    assert!(!board.background_sync_started);
}
