use std::fs;
use std::path::PathBuf;

use chrono::Utc;
use new_stock_lib::models::{
    DailyBarRecord, GetChartPayload, SaveBoardPayload, SymbolRecord, TargetNotePayload,
};
use new_stock_lib::repositories::Database;
use new_stock_lib::services::{BoardService, BootstrapService, ChartService, NoteService};
use rusqlite::{params, Connection};
use uuid::Uuid;

fn temp_db_path(test_name: &str) -> PathBuf {
    let path =
        std::env::temp_dir().join(format!("new_stock-{test_name}-{}.sqlite3", Uuid::new_v4()));
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

fn insert_symbol(
    database: &Database,
    symbol: &str,
    total_shares: Option<f64>,
    circulating_shares: Option<f64>,
) {
    database
        .save_symbols(&[SymbolRecord {
            target_id: symbol.to_string(),
            target_type: "symbol".to_string(),
            display_code: symbol.to_string(),
            name: format!("{symbol} Corp"),
            market: Some("US".to_string()),
            security_type: "equity".to_string(),
            currency: Some("USD".to_string()),
            total_shares,
            circulating_shares,
            updated_at: Utc::now().to_rfc3339(),
        }])
        .expect("symbol should save");
}

fn insert_daily_bars(database: &Database, symbol: &str, rows: &[(&str, f64, f64, f64, f64)]) {
    let bars: Vec<DailyBarRecord> = rows
        .iter()
        .map(|(trade_date, open, high, low, close)| DailyBarRecord {
            target_id: symbol.to_string(),
            trade_date: (*trade_date).to_string(),
            open: *open,
            high: *high,
            low: *low,
            close: *close,
            volume: Some(1_000_000.0),
            source: "test".to_string(),
            updated_at: Utc::now().to_rfc3339(),
        })
        .collect();
    database
        .save_sync_batch(&bars, &[])
        .expect("daily bars should save");
}

#[test]
fn step2_bootstrap_initializes_schema_and_fixture_once() {
    let db_path = temp_db_path("bootstrap");
    let database = Database::at(&db_path);

    database
        .bootstrap()
        .expect("first bootstrap should succeed");

    let conn = Connection::open(&db_path).expect("db should open");
    let journal_mode: String = conn
        .query_row("PRAGMA journal_mode;", [], |row| row.get(0))
        .expect("journal mode should be readable");
    assert_eq!(journal_mode.to_lowercase(), "wal");

    let initial_symbols: i64 = conn
        .query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0))
        .expect("symbols count should be readable");
    let initial_bars: i64 = conn
        .query_row("SELECT COUNT(*) FROM daily_bars", [], |row| row.get(0))
        .expect("bars count should be readable");
    let fixture_version: String = conn
        .query_row(
            "SELECT value FROM app_settings WHERE key = 'dev_fixture_version'",
            [],
            |row| row.get(0),
        )
        .expect("fixture version should exist");

    drop(conn);

    database
        .bootstrap()
        .expect("second bootstrap should stay idempotent");

    let conn = Connection::open(&db_path).expect("db should open again");
    let symbols_after: i64 = conn
        .query_row("SELECT COUNT(*) FROM symbols", [], |row| row.get(0))
        .expect("symbols count should be readable");
    let bars_after: i64 = conn
        .query_row("SELECT COUNT(*) FROM daily_bars", [], |row| row.get(0))
        .expect("bars count should be readable");

    assert_eq!(fixture_version, "step2_v1");
    assert!(
        initial_symbols >= 29,
        "fixture should seed indexes and symbols"
    );
    assert!(initial_bars >= 780 * 10, "fixture should seed enough bars");
    assert_eq!(
        initial_symbols, symbols_after,
        "fixture import must be idempotent"
    );
    assert_eq!(initial_bars, bars_after, "fixture bars must not duplicate");
}

#[test]
fn step2_bootstrap_service_reads_sqlite_state() {
    let database = Database::at(temp_db_path("bootstrap-service"));
    database.bootstrap().expect("bootstrap should succeed");

    let payload = BootstrapService::new(database)
        .load()
        .expect("bootstrap service should load data");

    assert_eq!(payload.indexes.len(), 4);
    assert!(payload.indexes.iter().any(|item| item.id == "DJI"));
    assert!(payload
        .boards
        .iter()
        .any(|board| board.board_id == "board-ai"));
    let board = payload
        .boards
        .iter()
        .find(|board| board.board_id == "board-ai")
        .expect("fixture board should exist");
    assert_eq!(board.build_status, "succeeded");
    assert_eq!(board.build_phase, "completed");
    assert!(!board.updated_at.is_empty());
    assert_eq!(payload.active_target_note.target_type, "board");
    assert_eq!(payload.active_target_note.target_id, "board-ai");
    assert!(!payload.active_target_note.updated_at.is_empty());
    assert!(!payload.members_by_board["board-ai"].is_empty());
    assert!(
        payload.sync_status.status == "offline_readable"
            || payload.sync_status.status == "ready",
        "step3 sync status should reflect either configured credentials or offline-readable local data"
    );
}

#[test]
fn step2_note_service_persists_and_restores() {
    let database = Database::at(temp_db_path("note-service"));
    database.bootstrap().expect("bootstrap should succeed");

    let note_service = NoteService::new(database.clone());
    let saved = note_service
        .save(TargetNotePayload {
            target_type: "symbol".to_string(),
            target_id: "NVDA".to_string(),
            content: Some("Watch gross margin trend".to_string()),
        })
        .expect("note should save");

    assert_eq!(saved.content, "Watch gross margin trend");
    assert!(!saved.updated_at.is_empty());

    let restored = NoteService::new(database)
        .get("symbol", "NVDA")
        .expect("note should restore");
    assert_eq!(restored.content, "Watch gross margin trend");
    assert_eq!(restored.updated_at, saved.updated_at);

    let fallback_database = Database::at(temp_db_path("note-fallback"));
    fallback_database
        .bootstrap()
        .expect("fallback db bootstrap should succeed");
    let fallback = NoteService::new(fallback_database)
        .get("symbol", "NVDA")
        .expect("missing note should still return contract fields");
    assert_eq!(fallback.content, "");
    assert!(!fallback.updated_at.is_empty());
}

#[test]
fn step2_save_board_fast_path_persists_and_generates_chart() {
    let database = Database::at(temp_db_path("save-board-fast"));
    database.bootstrap().expect("bootstrap should succeed");

    let response = BoardService::new(database.clone())
        .save(SaveBoardPayload {
            board_id: None,
            name: "核心AI".to_string(),
            members: vec!["NVDA".to_string(), "AMD".to_string(), "AVGO".to_string()],
            composition_algorithm: "equal_weight_v1".to_string(),
        })
        .expect("board should save on fast path");

    assert!(!response.background_sync_started);
    assert_eq!(response.build_status, "succeeded");
    assert_eq!(response.build_phase, "completed");

    let bootstrap = BootstrapService::new(database.clone())
        .load()
        .expect("bootstrap should read new board");
    let saved_board = bootstrap
        .boards
        .iter()
        .find(|board| board.board_id == response.board_id)
        .expect("saved board should be present after bootstrap");
    assert_eq!(saved_board.build_status, "succeeded");
    assert_eq!(saved_board.build_phase, "completed");
    assert!(!saved_board.updated_at.is_empty());

    let status = BoardService::new(database.clone())
        .get_build_status(&response.board_id)
        .expect("build status should load from sqlite");
    assert_eq!(status.build_status, "succeeded");
    assert_eq!(status.build_phase, "completed");
    assert!(!status.updated_at.is_empty());

    let chart = ChartService::new(database)
        .get_chart(GetChartPayload {
            target_type: "board".to_string(),
            target_id: response.board_id,
            granularity: Some("day".to_string()),
            range: Some("1y".to_string()),
            board_algorithm: Some("equal_weight_v1".to_string()),
        })
        .expect("new board should chart immediately");

    assert!(!chart.bars.is_empty());
    assert_eq!(chart.source_status, "local_cache");
}

#[test]
fn step2_save_board_background_placeholder_persists_status() {
    let database = Database::at(temp_db_path("save-board-background"));
    database.bootstrap().expect("bootstrap should succeed");

    let members: Vec<String> = (1..=21).map(|index| format!("SIM{:02}", index)).collect();
    let response = BoardService::new(database.clone())
        .save(SaveBoardPayload {
            board_id: None,
            name: "大样本板块".to_string(),
            members,
            composition_algorithm: "equal_weight_v1".to_string(),
        })
        .expect("board should save on background placeholder path");

    assert!(response.background_sync_started);
    assert_eq!(response.build_status, "queued");
    assert_eq!(response.build_phase, "queued");
    assert!(response.build_job_id.is_some());

    let status = BoardService::new(database.clone())
        .get_build_status(&response.board_id)
        .expect("build status should load from sqlite");
    assert_eq!(status.build_status, "queued");
    assert_eq!(status.build_phase, "queued");
    assert_eq!(status.build_total, 21);
    assert!(!status.updated_at.is_empty());

    let conn = Connection::open(database.path()).expect("db should open");
    let persisted_row: (String, String) = conn
        .query_row(
            "SELECT build_status, build_phase FROM boards WHERE board_id = ?1",
            params![response.board_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("persisted board status should exist");
    assert_eq!(persisted_row.0, "queued");
    assert_eq!(persisted_row.1, "queued");
}

#[test]
fn step2_market_cap_member_summary_and_chart_use_price_times_shares() {
    let database = Database::at(temp_db_path("market-cap-weight"));
    database.bootstrap().expect("bootstrap should succeed");

    insert_symbol(&database, "MCAPA", Some(10.0), Some(10.0));
    insert_symbol(&database, "MCAPB", Some(50.0), Some(50.0));
    insert_daily_bars(
        &database,
        "MCAPA",
        &[
            ("2026-03-17", 500.0, 500.0, 500.0, 500.0),
            ("2026-03-18", 600.0, 600.0, 600.0, 600.0),
        ],
    );
    insert_daily_bars(
        &database,
        "MCAPB",
        &[
            ("2026-03-17", 10.0, 10.0, 10.0, 10.0),
            ("2026-03-18", 8.0, 8.0, 8.0, 8.0),
        ],
    );

    let response = BoardService::new(database.clone())
        .save(SaveBoardPayload {
            board_id: None,
            name: "市值算法校验".to_string(),
            members: vec!["MCAPA".to_string(), "MCAPB".to_string()],
            composition_algorithm: "market_cap_weight_v1".to_string(),
        })
        .expect("board should save on fast path");

    let bootstrap = BootstrapService::new(database.clone())
        .load()
        .expect("bootstrap should load");
    let member_summaries = bootstrap
        .members_by_board
        .get(&response.board_id)
        .expect("member summaries should exist");
    let weight_a = member_summaries
        .iter()
        .find(|item| item.symbol == "MCAPA")
        .and_then(|item| item.weight_percent)
        .expect("MCAPA weight should exist");
    let weight_b = member_summaries
        .iter()
        .find(|item| item.symbol == "MCAPB")
        .and_then(|item| item.weight_percent)
        .expect("MCAPB weight should exist");
    assert!(
        weight_a > weight_b,
        "market-cap summary should favor higher price*shares member"
    );

    let chart = ChartService::new(database)
        .get_chart(GetChartPayload {
            target_type: "board".to_string(),
            target_id: response.board_id,
            granularity: Some("day".to_string()),
            range: Some("all".to_string()),
            board_algorithm: Some("market_cap_weight_v1".to_string()),
        })
        .expect("board chart should load");

    let latest_close = chart
        .bars
        .last()
        .map(|bar| bar.close)
        .expect("board chart should have bars");
    assert!(
        latest_close > 100.0,
        "market-cap board chart should reflect price*shares dominance, got {latest_close}"
    );
}

#[test]
fn step2_board_service_supports_algorithm_specific_member_summaries() {
    let database = Database::at(temp_db_path("board-member-summaries"));
    database.bootstrap().expect("bootstrap should succeed");

    insert_symbol(&database, "ALGOA", Some(10.0), Some(10.0));
    insert_symbol(&database, "ALGOB", Some(50.0), Some(50.0));
    insert_daily_bars(
        &database,
        "ALGOA",
        &[("2026-03-18", 400.0, 400.0, 400.0, 400.0)],
    );
    insert_daily_bars(
        &database,
        "ALGOB",
        &[("2026-03-18", 10.0, 10.0, 10.0, 10.0)],
    );

    let response = BoardService::new(database.clone())
        .save(SaveBoardPayload {
            board_id: None,
            name: "算法切换板块".to_string(),
            members: vec!["ALGOA".to_string(), "ALGOB".to_string()],
            composition_algorithm: "equal_weight_v1".to_string(),
        })
        .expect("board should save");

    let equal_payload = BoardService::new(database.clone())
        .get_member_summaries(&response.board_id, "equal_weight_v1")
        .expect("equal-weight member summaries should load");
    let market_payload = BoardService::new(database)
        .get_member_summaries(&response.board_id, "market_cap_weight_v1")
        .expect("market-cap member summaries should load");

    let equal_a = equal_payload
        .members
        .iter()
        .find(|item| item.symbol == "ALGOA")
        .and_then(|item| item.weight_percent)
        .expect("equal weight should exist");
    let market_a = market_payload
        .members
        .iter()
        .find(|item| item.symbol == "ALGOA")
        .and_then(|item| item.weight_percent)
        .expect("market weight should exist");

    assert_eq!(equal_payload.composition_algorithm, "equal_weight_v1");
    assert_eq!(market_payload.composition_algorithm, "market_cap_weight_v1");
    assert_eq!(equal_a, 50.0);
    assert!(market_a > equal_a);
}

#[test]
fn step2_get_chart_supports_day_week_and_reports_invalid_inputs() {
    let database = Database::at(temp_db_path("chart-service"));
    database.bootstrap().expect("bootstrap should succeed");
    let chart_service = ChartService::new(database.clone());

    let day_chart = chart_service
        .get_chart(GetChartPayload {
            range: Some("1m".to_string()),
            ..chart_request("index", "DJI")
        })
        .expect("day chart should load");
    assert_eq!(day_chart.meta.target_id, "DJI");
    assert_eq!(day_chart.meta.granularity.as_deref(), Some("day"));
    assert_eq!(day_chart.bars.len(), 22);

    let day_year_chart = chart_service
        .get_chart(chart_request("index", "DJI"))
        .expect("1y day chart should load");

    let week_chart = chart_service
        .get_chart(GetChartPayload {
            granularity: Some("week".to_string()),
            ..chart_request("index", "DJI")
        })
        .expect("week chart should load");
    assert!(week_chart.bars.len() < day_year_chart.bars.len());
    assert_eq!(week_chart.meta.granularity.as_deref(), Some("week"));

    let empty_chart = chart_service
        .get_chart(GetChartPayload {
            target_type: "symbol".to_string(),
            target_id: "EMPTY1".to_string(),
            granularity: Some("day".to_string()),
            range: Some("1m".to_string()),
            board_algorithm: None,
        })
        .expect("known target without bars should return empty payload");
    assert!(empty_chart.bars.is_empty());
    assert!(empty_chart.latest_trade_date.is_none());
    assert_eq!(empty_chart.source_status, "empty");

    let invalid_granularity = chart_service.get_chart(GetChartPayload {
        granularity: Some("month".to_string()),
        ..chart_request("index", "DJI")
    });
    assert!(invalid_granularity.is_err());

    let invalid_target = chart_service.get_chart(GetChartPayload {
        target_type: "symbol".to_string(),
        target_id: "DOES_NOT_EXIST".to_string(),
        granularity: Some("day".to_string()),
        range: Some("1m".to_string()),
        board_algorithm: None,
    });
    assert!(invalid_target.is_err());
}
