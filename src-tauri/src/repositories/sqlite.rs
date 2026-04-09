use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration as StdDuration;

use chrono::{Datelike, Duration, NaiveDate, Utc};
use rusqlite::{params, params_from_iter, Connection, OptionalExtension, Transaction};
use serde::Deserialize;

use crate::board_weights::resolve_snapshot_weights;
use crate::errors::{AppError, AppResult};
use crate::models::{
    BarPoint, BoardDailyBarRecord, BoardRecord, DailyBarRecord, IndexItem, MemberSummary,
    NoteRecord, SymbolRecord, SyncStatusPayload,
};

const FIXTURE_VERSION_KEY: &str = "dev_fixture_version";
const FIXTURE_VERSION_VALUE: &str = "step2_v1";
const BAR_ADJUSTMENT_POLICY_KEY: &str = "daily_bar_adjustment_policy";
const APP_DATA_DIR_NAME: &str = "new_stock";
const DATABASE_FILE_NAME: &str = "new_stock.sqlite3";

const CORE_INDEXES: [(&str, &str, &str); 4] = [
    ("DJI", "DJI", "Dow Jones Industrial Average"),
    ("IXIC", "IXIC", "NASDAQ Composite"),
    ("GSPC", "GSPC", "S&P 500"),
    ("RUT", "RUT", "Russell 2000"),
];

#[derive(Debug, Clone)]
pub struct Database {
    path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct SyncStateRecord {
    pub target_type: String,
    pub target_id: String,
    pub latest_trade_date: Option<String>,
    pub last_sync_at: Option<String>,
    pub last_sync_status: String,
    pub last_error_code: Option<String>,
    pub last_error_message: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SyncJobRecord {
    pub job_id: String,
    pub mode: String,
    pub status: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub summary_json: Option<String>,
    pub error_json: Option<String>,
}

impl Default for Database {
    fn default() -> Self {
        Self::new()
    }
}

impl Database {
    pub fn new() -> Self {
        if let Ok(path) = std::env::var("NEW_STOCK_DB_PATH") {
            return Self {
                path: PathBuf::from(path),
            };
        }

        Self {
            path: default_database_path(),
        }
    }

    pub fn at(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn bootstrap(&self) -> AppResult<()> {
        self.migrate_legacy_database_if_needed()?;

        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut conn = Connection::open(&self.path)?;
        Self::configure_connection(&conn)?;
        self.create_schema(&conn)?;
        self.seed_core_indexes(&conn)?;

        if self.should_import_fixture(&conn)? {
            self.import_dev_fixture(&mut conn)?;
        }

        Ok(())
    }

    pub fn open_connection(&self) -> AppResult<Connection> {
        self.migrate_legacy_database_if_needed()?;

        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        let conn = Connection::open(&self.path)?;
        Self::configure_connection(&conn)?;
        Ok(conn)
    }

    fn migrate_legacy_database_if_needed(&self) -> AppResult<()> {
        let legacy = legacy_database_path();
        if self.path == legacy || self.path.exists() || !legacy.exists() {
            return Ok(());
        }

        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::copy(&legacy, &self.path)?;

        for suffix in ["-wal", "-shm"] {
            let legacy_sidecar = sqlite_sidecar_path(&legacy, suffix);
            if legacy_sidecar.exists() {
                fs::copy(&legacy_sidecar, sqlite_sidecar_path(&self.path, suffix))?;
            }
        }

        Ok(())
    }

    pub fn list_indexes(&self) -> AppResult<Vec<IndexItem>> {
        let conn = self.open_connection()?;
        let mut statement = conn.prepare(
            "SELECT target_id, name
             FROM symbols
             WHERE target_type = 'index'
             ORDER BY CASE target_id
               WHEN 'DJI' THEN 1
               WHEN 'IXIC' THEN 2
               WHEN 'GSPC' THEN 3
               WHEN 'RUT' THEN 4
               ELSE 99
             END",
        )?;

        let rows = statement.query_map([], |row| {
            Ok(IndexItem {
                id: row.get(0)?,
                label: row.get(1)?,
                disabled: None,
            })
        })?;

        rows.collect::<Result<Vec<_>, _>>().map_err(AppError::from)
    }

    pub fn list_boards(&self) -> AppResult<Vec<BoardRecord>> {
        let conn = self.open_connection()?;
        let mut statement = conn.prepare(
            "SELECT board_id, name, sort_order, composition_algorithm,
                    build_status, build_phase, build_total, build_completed, build_failed,
                    build_job_id, build_message, build_started_at, build_finished_at,
                    created_at, updated_at
             FROM boards
             ORDER BY sort_order ASC, created_at ASC",
        )?;
        let rows = statement.query_map([], map_board_record)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(AppError::from)
    }

    pub fn get_board(&self, board_id: &str) -> AppResult<Option<BoardRecord>> {
        let conn = self.open_connection()?;
        conn.query_row(
            "SELECT board_id, name, sort_order, composition_algorithm,
                    build_status, build_phase, build_total, build_completed, build_failed,
                    build_job_id, build_message, build_started_at, build_finished_at,
                    created_at, updated_at
             FROM boards
             WHERE board_id = ?1",
            params![board_id],
            map_board_record,
        )
        .optional()
        .map_err(AppError::from)
    }

    pub fn board_exists(&self, board_id: &str) -> AppResult<bool> {
        let conn = self.open_connection()?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM boards WHERE board_id = ?1",
            params![board_id],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn list_board_members(&self, board_id: &str) -> AppResult<Vec<String>> {
        let conn = self.open_connection()?;
        let mut statement = conn.prepare(
            "SELECT target_id
             FROM board_members
             WHERE board_id = ?1
             ORDER BY sort_order ASC, target_id ASC",
        )?;
        let rows = statement.query_map(params![board_id], |row| row.get::<_, String>(0))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(AppError::from)
    }

    pub fn list_board_ids_by_member_targets(
        &self,
        target_ids: &[String],
    ) -> AppResult<Vec<String>> {
        if target_ids.is_empty() {
            return Ok(Vec::new());
        }

        let conn = self.open_connection()?;
        let placeholders = vec!["?"; target_ids.len()].join(", ");
        let sql = format!(
            "SELECT DISTINCT board_id
             FROM board_members
             WHERE target_id IN ({placeholders})
             ORDER BY board_id ASC"
        );
        let mut statement = conn.prepare(&sql)?;
        let rows = statement.query_map(params_from_iter(target_ids.iter()), |row| {
            row.get::<_, String>(0)
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(AppError::from)
    }

    pub fn delete_board_daily_bars_for_boards(&self, board_ids: &[String]) -> AppResult<()> {
        if board_ids.is_empty() {
            return Ok(());
        }

        let mut conn = self.open_connection()?;
        let transaction = conn.transaction()?;
        for board_id in board_ids {
            transaction.execute(
                "DELETE FROM board_daily_bars WHERE board_id = ?1",
                params![board_id],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn list_member_summaries(
        &self,
        board_id: &str,
        algorithm: &str,
    ) -> AppResult<Vec<MemberSummary>> {
        let members = self.list_board_members(board_id)?;
        let symbols = self.list_symbols_by_ids(&members)?;
        let latest_closes = self.list_latest_closes_by_ids(&members)?;

        Ok(match algorithm {
            "equal_weight_v1" => assign_equal_weight(&members),
            "market_cap_weight_v1" => assign_market_cap_weight(&members, &symbols, &latest_closes)?,
            other => {
                return Err(AppError::Message(format!(
                    "unsupported board algorithm: {other}"
                )))
            }
        })
    }

    pub fn list_symbols_by_ids(&self, target_ids: &[String]) -> AppResult<Vec<SymbolRecord>> {
        let conn = self.open_connection()?;
        let mut results = Vec::new();
        for target_id in target_ids {
            if let Some(record) = conn
                .query_row(
                    "SELECT target_id, target_type, display_code, name, market, security_type,
                            currency, total_shares, circulating_shares, updated_at
                     FROM symbols WHERE target_id = ?1",
                    params![target_id],
                    map_symbol_record,
                )
                .optional()?
            {
                results.push(record);
            }
        }
        Ok(results)
    }

    pub fn get_symbol(
        &self,
        target_id: &str,
        target_type: &str,
    ) -> AppResult<Option<SymbolRecord>> {
        let conn = self.open_connection()?;
        conn.query_row(
            "SELECT target_id, target_type, display_code, name, market, security_type,
                    currency, total_shares, circulating_shares, updated_at
             FROM symbols
             WHERE target_id = ?1 AND target_type = ?2",
            params![target_id, target_type],
            map_symbol_record,
        )
        .optional()
        .map_err(AppError::from)
    }

    pub fn count_bars_for_target(&self, target_id: &str) -> AppResult<usize> {
        let conn = self.open_connection()?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM daily_bars WHERE target_id = ?1",
            params![target_id],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    pub fn has_any_daily_bars(&self) -> AppResult<bool> {
        let conn = self.open_connection()?;
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM daily_bars", [], |row| row.get(0))?;
        Ok(count > 0)
    }

    pub fn has_fixture_bars(&self) -> AppResult<bool> {
        let conn = self.open_connection()?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM daily_bars WHERE source LIKE 'fixture_%'",
            [],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn latest_bar_date(&self, target_id: &str) -> AppResult<Option<String>> {
        let conn = self.open_connection()?;
        conn.query_row(
            "SELECT MAX(trade_date) FROM daily_bars WHERE target_id = ?1",
            params![target_id],
            |row| row.get(0),
        )
        .map_err(AppError::from)
    }

    pub fn earliest_bar_date(&self, target_id: &str) -> AppResult<Option<String>> {
        let conn = self.open_connection()?;
        conn.query_row(
            "SELECT MIN(trade_date) FROM daily_bars WHERE target_id = ?1",
            params![target_id],
            |row| row.get(0),
        )
        .map_err(AppError::from)
    }

    pub fn target_uses_only_fixture_bars(&self, target_id: &str) -> AppResult<bool> {
        let conn = self.open_connection()?;
        let (total_count, non_fixture_count): (i64, i64) = conn.query_row(
            "SELECT
                COUNT(*),
                SUM(CASE WHEN source NOT LIKE 'fixture_%' THEN 1 ELSE 0 END)
             FROM daily_bars
             WHERE target_id = ?1",
            params![target_id],
            |row| Ok((row.get(0)?, row.get::<_, Option<i64>>(1)?.unwrap_or(0))),
        )?;
        Ok(total_count > 0 && non_fixture_count == 0)
    }

    pub fn list_symbols_for_sync(&self) -> AppResult<Vec<SymbolRecord>> {
        let conn = self.open_connection()?;
        let mut statement = conn.prepare(
            "SELECT target_id, target_type, display_code, name, market, security_type,
                    currency, total_shares, circulating_shares, updated_at
             FROM symbols
             ORDER BY CASE target_type WHEN 'index' THEN 0 ELSE 1 END, target_id ASC",
        )?;
        let rows = statement.query_map([], map_symbol_record)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(AppError::from)
    }

    pub fn save_symbols(&self, symbols: &[SymbolRecord]) -> AppResult<()> {
        if symbols.is_empty() {
            return Ok(());
        }

        let mut conn = self.open_connection()?;
        let transaction = conn.transaction()?;
        for symbol in symbols {
            transaction.execute(
                "INSERT INTO symbols (
                    target_id, target_type, display_code, name, market, security_type,
                    currency, total_shares, circulating_shares, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
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
                    symbol.target_id,
                    symbol.target_type,
                    symbol.display_code,
                    symbol.name,
                    symbol.market,
                    symbol.security_type,
                    symbol.currency,
                    symbol.total_shares,
                    symbol.circulating_shares,
                    symbol.updated_at
                ],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn save_daily_bars(&self, bars: &[DailyBarRecord]) -> AppResult<()> {
        if bars.is_empty() {
            return Ok(());
        }

        let mut conn = self.open_connection()?;
        let transaction = conn.transaction()?;
        for bar in bars {
            insert_daily_bar(&transaction, bar)?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn save_sync_batch(
        &self,
        bars: &[DailyBarRecord],
        rows: &[SyncStateRecord],
    ) -> AppResult<()> {
        if bars.is_empty() && rows.is_empty() {
            return Ok(());
        }

        let mut conn = self.open_connection()?;
        let transaction = conn.transaction()?;
        for bar in bars {
            insert_daily_bar(&transaction, bar)?;
        }
        for row in rows {
            upsert_sync_state_row(&transaction, row)?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn delete_daily_bars_for_targets(&self, target_ids: &[String]) -> AppResult<()> {
        if target_ids.is_empty() {
            return Ok(());
        }

        let mut conn = self.open_connection()?;
        let transaction = conn.transaction()?;
        for target_id in target_ids {
            transaction.execute(
                "DELETE FROM daily_bars WHERE target_id = ?1",
                params![target_id],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn list_daily_bars(&self, target_id: &str) -> AppResult<Vec<DailyBarRecord>> {
        let conn = self.open_connection()?;
        let mut statement = conn.prepare(
            "SELECT target_id, trade_date, open, high, low, close, volume, source, updated_at
             FROM daily_bars
             WHERE target_id = ?1
             ORDER BY trade_date ASC",
        )?;
        let rows = statement.query_map(params![target_id], map_daily_bar_record)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(AppError::from)
    }

    pub fn list_latest_closes_by_ids(
        &self,
        target_ids: &[String],
    ) -> AppResult<HashMap<String, f64>> {
        if target_ids.is_empty() {
            return Ok(HashMap::new());
        }

        let conn = self.open_connection()?;
        let placeholders = vec!["?"; target_ids.len()].join(", ");
        let sql = format!(
            "SELECT bars.target_id, bars.close
             FROM daily_bars bars
             INNER JOIN (
                SELECT target_id, MAX(trade_date) AS latest_trade_date
                FROM daily_bars
                WHERE target_id IN ({placeholders})
                GROUP BY target_id
             ) latest
               ON latest.target_id = bars.target_id
              AND latest.latest_trade_date = bars.trade_date
             ORDER BY bars.target_id ASC"
        );
        let mut statement = conn.prepare(&sql)?;
        let rows = statement.query_map(params_from_iter(target_ids.iter()), |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
        })?;

        rows.collect::<Result<HashMap<_, _>, _>>()
            .map_err(AppError::from)
    }

    pub fn list_board_daily_bars(
        &self,
        board_id: &str,
        algorithm: &str,
    ) -> AppResult<Vec<BoardDailyBarRecord>> {
        let conn = self.open_connection()?;
        let mut statement = conn.prepare(
            "SELECT board_id, composition_algorithm, trade_date, open, high, low, close, volume, updated_at
             FROM board_daily_bars
             WHERE board_id = ?1 AND composition_algorithm = ?2
             ORDER BY trade_date ASC",
        )?;
        let rows = statement.query_map(params![board_id, algorithm], |row| {
            Ok(BoardDailyBarRecord {
                board_id: row.get(0)?,
                composition_algorithm: row.get(1)?,
                trade_date: row.get(2)?,
                open: row.get(3)?,
                high: row.get(4)?,
                low: row.get(5)?,
                close: row.get(6)?,
                volume: row.get(7)?,
                updated_at: row.get(8)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(AppError::from)
    }

    pub fn save_board_chart(
        &self,
        board_id: &str,
        algorithm: &str,
        bars: &[BarPoint],
    ) -> AppResult<()> {
        let mut conn = self.open_connection()?;
        let exists: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM boards WHERE board_id = ?1)",
            params![board_id],
            |row| row.get(0),
        )?;
        if !exists {
            return Err(AppError::Message(format!("board not found: {board_id}")));
        }
        let transaction = conn.transaction()?;
        transaction.execute(
            "DELETE FROM board_daily_bars WHERE board_id = ?1 AND composition_algorithm = ?2",
            params![board_id, algorithm],
        )?;

        for bar in bars {
            transaction.execute(
                "INSERT INTO board_daily_bars (
                    board_id, composition_algorithm, trade_date,
                    open, high, low, close, volume, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    board_id,
                    algorithm,
                    bar.time,
                    bar.open,
                    bar.high,
                    bar.low,
                    bar.close,
                    bar.volume,
                    now_string()
                ],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn save_board_definition(&self, board: &BoardRecord, members: &[String]) -> AppResult<()> {
        let mut conn = self.open_connection()?;
        let transaction = conn.transaction()?;

        transaction.execute(
            "INSERT INTO boards (
                board_id, name, sort_order, composition_algorithm,
                build_status, build_phase, build_total, build_completed, build_failed,
                build_job_id, build_message, build_started_at, build_finished_at,
                created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
             ON CONFLICT(board_id) DO UPDATE SET
                name = excluded.name,
                sort_order = excluded.sort_order,
                composition_algorithm = excluded.composition_algorithm,
                build_status = excluded.build_status,
                build_phase = excluded.build_phase,
                build_total = excluded.build_total,
                build_completed = excluded.build_completed,
                build_failed = excluded.build_failed,
                build_job_id = excluded.build_job_id,
                build_message = excluded.build_message,
                build_started_at = excluded.build_started_at,
                build_finished_at = excluded.build_finished_at,
                updated_at = excluded.updated_at",
            params![
                board.board_id,
                board.name,
                board.sort_order,
                board.composition_algorithm,
                board.build_status,
                board.build_phase,
                board.build_total as i64,
                board.build_completed as i64,
                board.build_failed as i64,
                board.build_job_id,
                board.build_message,
                board.build_started_at,
                board.build_finished_at,
                board.created_at,
                board.updated_at
            ],
        )?;

        transaction.execute(
            "DELETE FROM board_members WHERE board_id = ?1",
            params![board.board_id],
        )?;
        transaction.execute(
            "DELETE FROM board_daily_bars WHERE board_id = ?1",
            params![board.board_id],
        )?;

        for (index, member) in members.iter().enumerate() {
            transaction.execute(
                "INSERT INTO board_members (board_id, target_id, sort_order)
                 VALUES (?1, ?2, ?3)",
                params![board.board_id, member, index as i64],
            )?;
        }

        transaction.commit()?;
        Ok(())
    }

    pub fn update_board_build_state(&self, board: &BoardRecord) -> AppResult<()> {
        let conn = self.open_connection()?;
        let updated = conn.execute(
            "UPDATE boards
             SET name = ?2,
                 sort_order = ?3,
                 composition_algorithm = ?4,
                 build_status = ?5,
                 build_phase = ?6,
                 build_total = ?7,
                 build_completed = ?8,
                 build_failed = ?9,
                 build_job_id = ?10,
                 build_message = ?11,
                 build_started_at = ?12,
                 build_finished_at = ?13,
                 updated_at = ?14
             WHERE board_id = ?1",
            params![
                board.board_id,
                board.name,
                board.sort_order,
                board.composition_algorithm,
                board.build_status,
                board.build_phase,
                board.build_total as i64,
                board.build_completed as i64,
                board.build_failed as i64,
                board.build_job_id,
                board.build_message,
                board.build_started_at,
                board.build_finished_at,
                board.updated_at
            ],
        )?;
        if updated == 0 {
            return Err(AppError::Message(format!(
                "board not found: {}",
                board.board_id
            )));
        }
        Ok(())
    }

    pub fn delete_board(&self, board_id: &str) -> AppResult<()> {
        let mut conn = self.open_connection()?;
        let transaction = conn.transaction()?;
        transaction.execute(
            "DELETE FROM target_notes WHERE target_type = 'board' AND target_id = ?1",
            params![board_id],
        )?;
        transaction.execute(
            "DELETE FROM board_daily_bars WHERE board_id = ?1",
            params![board_id],
        )?;
        transaction.execute(
            "DELETE FROM board_members WHERE board_id = ?1",
            params![board_id],
        )?;
        let deleted =
            transaction.execute("DELETE FROM boards WHERE board_id = ?1", params![board_id])?;
        if deleted == 0 {
            return Err(AppError::Message(format!("board not found: {board_id}")));
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn delete_all_board_daily_bars(&self) -> AppResult<()> {
        let conn = self.open_connection()?;
        conn.execute("DELETE FROM board_daily_bars", [])?;
        Ok(())
    }

    pub fn purge_fixture_data(&self) -> AppResult<()> {
        let fixture = load_dev_fixture()?;
        let mut conn = self.open_connection()?;
        let transaction = conn.transaction()?;
        transaction.execute("DELETE FROM daily_bars WHERE source LIKE 'fixture_%'", [])?;
        transaction.execute("DELETE FROM board_daily_bars", [])?;
        transaction.execute(
            "DELETE FROM sync_state
             WHERE NOT EXISTS (
                SELECT 1
                FROM daily_bars
                WHERE daily_bars.target_id = sync_state.target_id
             )",
            [],
        )?;
        delete_orphan_fixture_symbols(
            &transaction,
            &fixture
                .symbols
                .iter()
                .map(|seed| seed.target_id.clone())
                .collect::<Vec<_>>(),
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn cleanup_fixture_orphan_symbols(&self) -> AppResult<()> {
        let fixture = load_dev_fixture()?;
        let mut conn = self.open_connection()?;
        let transaction = conn.transaction()?;
        transaction.execute(
            "DELETE FROM sync_state
             WHERE NOT EXISTS (
                SELECT 1
                FROM daily_bars
                WHERE daily_bars.target_id = sync_state.target_id
             )",
            [],
        )?;
        delete_orphan_fixture_symbols(
            &transaction,
            &fixture
                .symbols
                .iter()
                .map(|seed| seed.target_id.clone())
                .collect::<Vec<_>>(),
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn current_bar_adjustment_policy(&self) -> AppResult<Option<String>> {
        let conn = self.open_connection()?;
        conn.query_row(
            "SELECT value FROM app_settings WHERE key = ?1",
            params![BAR_ADJUSTMENT_POLICY_KEY],
            |row| row.get(0),
        )
        .optional()
        .map_err(AppError::from)
    }

    pub fn set_bar_adjustment_policy(&self, value: &str) -> AppResult<()> {
        let conn = self.open_connection()?;
        conn.execute(
            "INSERT INTO app_settings (key, value, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET
                value = excluded.value,
                updated_at = excluded.updated_at",
            params![BAR_ADJUSTMENT_POLICY_KEY, value, now_string()],
        )?;
        Ok(())
    }

    pub fn recover_stale_board_builds(&self) -> AppResult<usize> {
        let conn = self.open_connection()?;
        let updated_at = now_string();
        conn.execute(
            "UPDATE boards
             SET build_status = 'failed',
                 build_phase = 'failed',
                 build_message = '应用中断，任务未完成',
                 build_finished_at = ?1,
                 updated_at = ?1
             WHERE build_status IN ('queued', 'running')",
            params![updated_at],
        )
        .map_err(AppError::from)
    }

    pub fn next_board_sort_order(&self) -> AppResult<i64> {
        let conn = self.open_connection()?;
        let value: Option<i64> =
            conn.query_row("SELECT MAX(sort_order) FROM boards", [], |row| row.get(0))?;
        Ok(value.unwrap_or(0) + 1)
    }

    pub fn get_note(&self, target_type: &str, target_id: &str) -> AppResult<Option<NoteRecord>> {
        let conn = self.open_connection()?;
        conn.query_row(
            "SELECT target_type, target_id, content, updated_at
             FROM target_notes
             WHERE target_type = ?1 AND target_id = ?2",
            params![target_type, target_id],
            |row| {
                Ok(NoteRecord {
                    target_type: row.get(0)?,
                    target_id: row.get(1)?,
                    content: row.get(2)?,
                    updated_at: row.get(3)?,
                })
            },
        )
        .optional()
        .map_err(AppError::from)
    }

    pub fn get_first_note(&self) -> AppResult<Option<NoteRecord>> {
        let conn = self.open_connection()?;
        conn.query_row(
            "SELECT target_type, target_id, content, updated_at
             FROM target_notes
             ORDER BY updated_at DESC, created_at DESC
             LIMIT 1",
            [],
            |row| {
                Ok(NoteRecord {
                    target_type: row.get(0)?,
                    target_id: row.get(1)?,
                    content: row.get(2)?,
                    updated_at: row.get(3)?,
                })
            },
        )
        .optional()
        .map_err(AppError::from)
    }

    pub fn save_note(
        &self,
        target_type: &str,
        target_id: &str,
        content: &str,
    ) -> AppResult<NoteRecord> {
        let mut conn = self.open_connection()?;
        let now = now_string();
        let transaction = conn.transaction()?;
        transaction.execute(
            "INSERT INTO target_notes (target_type, target_id, content, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(target_type, target_id) DO UPDATE SET
                content = excluded.content,
                updated_at = excluded.updated_at",
            params![target_type, target_id, content, now, now],
        )?;
        transaction.commit()?;

        Ok(NoteRecord {
            target_type: target_type.to_string(),
            target_id: target_id.to_string(),
            content: content.to_string(),
            updated_at: now,
        })
    }

    pub fn save_sync_states(&self, rows: &[SyncStateRecord]) -> AppResult<()> {
        if rows.is_empty() {
            return Ok(());
        }

        let mut conn = self.open_connection()?;
        let transaction = conn.transaction()?;
        for row in rows {
            upsert_sync_state_row(&transaction, row)?;
        }
        transaction.commit()?;
        Ok(())
    }

    pub fn insert_sync_job(&self, job: &SyncJobRecord) -> AppResult<()> {
        let conn = self.open_connection()?;
        conn.execute(
            "INSERT INTO sync_jobs (
                job_id, mode, status, started_at, finished_at, summary_json, error_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(job_id) DO UPDATE SET
                mode = excluded.mode,
                status = excluded.status,
                started_at = excluded.started_at,
                finished_at = excluded.finished_at,
                summary_json = excluded.summary_json,
                error_json = excluded.error_json",
            params![
                job.job_id,
                job.mode,
                job.status,
                job.started_at,
                job.finished_at,
                job.summary_json,
                job.error_json
            ],
        )?;
        Ok(())
    }

    pub fn finish_sync_job(
        &self,
        job_id: &str,
        status: &str,
        finished_at: Option<String>,
        summary_json: Option<String>,
        error_json: Option<String>,
    ) -> AppResult<()> {
        let conn = self.open_connection()?;
        conn.execute(
            "UPDATE sync_jobs
             SET status = ?2,
                 finished_at = ?3,
                 summary_json = ?4,
                 error_json = ?5
             WHERE job_id = ?1",
            params![job_id, status, finished_at, summary_json, error_json],
        )?;
        Ok(())
    }

    pub fn latest_sync_job(&self) -> AppResult<Option<SyncJobRecord>> {
        let conn = self.open_connection()?;
        conn.query_row(
            "SELECT job_id, mode, status, started_at, finished_at, summary_json, error_json
             FROM sync_jobs
             ORDER BY started_at DESC
             LIMIT 1",
            [],
            |row| {
                Ok(SyncJobRecord {
                    job_id: row.get(0)?,
                    mode: row.get(1)?,
                    status: row.get(2)?,
                    started_at: row.get(3)?,
                    finished_at: row.get(4)?,
                    summary_json: row.get(5)?,
                    error_json: row.get(6)?,
                })
            },
        )
        .optional()
        .map_err(AppError::from)
    }

    pub fn get_sync_status_summary(&self) -> AppResult<SyncStatusPayload> {
        let conn = self.open_connection()?;
        let summary = conn.query_row(
            "SELECT MAX(last_sync_at), MAX(latest_trade_date)
             FROM sync_state",
            [],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                ))
            },
        )?;

        Ok(SyncStatusPayload {
            status: "ready".into(),
            message: "同步状态可用".into(),
            last_sync_at: summary.0,
            latest_trade_date: summary.1,
        })
    }

    fn should_import_fixture(&self, conn: &Connection) -> AppResult<bool> {
        if !cfg!(debug_assertions) {
            return Ok(false);
        }
        if matches!(
            std::env::var("NEW_STOCK_DISABLE_DEV_FIXTURE")
                .ok()
                .as_deref(),
            Some("1") | Some("true") | Some("TRUE")
        ) {
            return Ok(false);
        }
        if crate::secret_store::load_credentials()
            .ok()
            .flatten()
            .is_some()
        {
            return Ok(false);
        }

        let value: Option<String> = conn
            .query_row(
                "SELECT value FROM app_settings WHERE key = ?1",
                params![FIXTURE_VERSION_KEY],
                |row| row.get(0),
            )
            .optional()?;

        Ok(value.as_deref() != Some(FIXTURE_VERSION_VALUE))
    }

    fn seed_core_indexes(&self, conn: &Connection) -> AppResult<()> {
        let now = now_string();
        for (target_id, display_code, name) in CORE_INDEXES {
            conn.execute(
                "INSERT INTO symbols (
                    target_id, target_type, display_code, name, market, security_type,
                    currency, total_shares, circulating_shares, updated_at
                 ) VALUES (?1, 'index', ?2, ?3, 'US', 'index', NULL, NULL, NULL, ?4)
                 ON CONFLICT(target_id) DO UPDATE SET
                    target_type = excluded.target_type,
                    display_code = excluded.display_code,
                    name = excluded.name,
                    market = excluded.market,
                    security_type = excluded.security_type,
                    updated_at = excluded.updated_at",
                params![target_id, display_code, name, now],
            )?;
        }
        Ok(())
    }

    fn create_schema(&self, conn: &Connection) -> AppResult<()> {
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS app_settings (
              key TEXT PRIMARY KEY,
              value TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS symbols (
              target_id TEXT PRIMARY KEY,
              target_type TEXT NOT NULL,
              display_code TEXT NOT NULL,
              name TEXT NOT NULL,
              market TEXT,
              security_type TEXT NOT NULL,
              currency TEXT,
              total_shares REAL,
              circulating_shares REAL,
              updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS daily_bars (
              target_id TEXT NOT NULL,
              trade_date TEXT NOT NULL,
              open REAL NOT NULL,
              high REAL NOT NULL,
              low REAL NOT NULL,
              close REAL NOT NULL,
              volume REAL,
              source TEXT NOT NULL,
              updated_at TEXT NOT NULL,
              PRIMARY KEY (target_id, trade_date)
            );

            CREATE TABLE IF NOT EXISTS boards (
              board_id TEXT PRIMARY KEY,
              name TEXT NOT NULL UNIQUE,
              sort_order INTEGER NOT NULL,
              composition_algorithm TEXT NOT NULL,
              build_status TEXT NOT NULL,
              build_phase TEXT NOT NULL,
              build_total INTEGER NOT NULL,
              build_completed INTEGER NOT NULL,
              build_failed INTEGER NOT NULL,
              build_job_id TEXT,
              build_message TEXT,
              build_started_at TEXT,
              build_finished_at TEXT,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS board_members (
              board_id TEXT NOT NULL,
              target_id TEXT NOT NULL,
              sort_order INTEGER NOT NULL,
              PRIMARY KEY (board_id, target_id)
            );

            CREATE TABLE IF NOT EXISTS board_daily_bars (
              board_id TEXT NOT NULL,
              composition_algorithm TEXT NOT NULL,
              trade_date TEXT NOT NULL,
              open REAL NOT NULL,
              high REAL NOT NULL,
              low REAL NOT NULL,
              close REAL NOT NULL,
              volume REAL,
              updated_at TEXT NOT NULL,
              PRIMARY KEY (board_id, composition_algorithm, trade_date)
            );

            CREATE TABLE IF NOT EXISTS target_notes (
              target_type TEXT NOT NULL,
              target_id TEXT NOT NULL,
              content TEXT NOT NULL,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL,
              PRIMARY KEY (target_type, target_id)
            );

            CREATE TABLE IF NOT EXISTS sync_state (
              target_type TEXT NOT NULL,
              target_id TEXT NOT NULL,
              latest_trade_date TEXT,
              last_sync_at TEXT,
              last_sync_status TEXT,
              last_error_code TEXT,
              last_error_message TEXT,
              PRIMARY KEY (target_type, target_id)
            );

            CREATE TABLE IF NOT EXISTS sync_jobs (
              job_id TEXT PRIMARY KEY,
              mode TEXT NOT NULL,
              status TEXT NOT NULL,
              started_at TEXT NOT NULL,
              finished_at TEXT,
              summary_json TEXT,
              error_json TEXT
            );
            ",
        )?;
        Ok(())
    }

    fn import_dev_fixture(&self, conn: &mut Connection) -> AppResult<()> {
        let fixture = load_dev_fixture()?;
        let dates = build_trading_dates(&fixture.latest_trade_date, fixture.bar_count)?;
        let transaction = conn.transaction()?;

        for seed in &fixture.indexes {
            insert_symbol(&transaction, seed, "index")?;
            for bar in generate_bars(seed, &dates, "fixture_index") {
                insert_daily_bar(&transaction, &bar)?;
            }
            insert_sync_state(
                &transaction,
                "index",
                &seed.target_id,
                Some(fixture.latest_trade_date.as_str()),
                Some(fixture.last_sync_at.as_str()),
                "ready",
            )?;
        }

        for seed in &fixture.symbols {
            insert_symbol(&transaction, seed, "symbol")?;
            if !seed.skip_bars.unwrap_or(false) {
                for bar in generate_bars(seed, &dates, "fixture_symbol") {
                    insert_daily_bar(&transaction, &bar)?;
                }
                insert_sync_state(
                    &transaction,
                    "symbol",
                    &seed.target_id,
                    Some(fixture.latest_trade_date.as_str()),
                    Some(fixture.last_sync_at.as_str()),
                    "ready",
                )?;
            } else {
                insert_sync_state(
                    &transaction,
                    "symbol",
                    &seed.target_id,
                    None,
                    Some(fixture.last_sync_at.as_str()),
                    "ready",
                )?;
            }
        }

        for board in &fixture.boards {
            transaction.execute(
                "INSERT INTO boards (
                    board_id, name, sort_order, composition_algorithm,
                    build_status, build_phase, build_total, build_completed, build_failed,
                    build_job_id, build_message, build_started_at, build_finished_at,
                    created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, 'succeeded', 'completed', ?5, ?5, 0, NULL, NULL, NULL, NULL, ?6, ?6)
                 ON CONFLICT(board_id) DO UPDATE SET
                    name = excluded.name,
                    sort_order = excluded.sort_order,
                    composition_algorithm = excluded.composition_algorithm,
                    build_status = excluded.build_status,
                    build_phase = excluded.build_phase,
                    build_total = excluded.build_total,
                    build_completed = excluded.build_completed,
                    build_failed = excluded.build_failed,
                    updated_at = excluded.updated_at",
                params![
                    board.board_id,
                    board.name,
                    board.sort_order,
                    board.composition_algorithm,
                    board.members.len() as i64,
                    fixture.last_sync_at.as_str()
                ],
            )?;

            for (index, member) in board.members.iter().enumerate() {
                transaction.execute(
                    "INSERT INTO board_members (board_id, target_id, sort_order)
                     VALUES (?1, ?2, ?3)
                     ON CONFLICT(board_id, target_id) DO UPDATE SET
                        sort_order = excluded.sort_order",
                    params![board.board_id, member, index as i64],
                )?;
            }
        }

        for note in &fixture.notes {
            transaction.execute(
                "INSERT INTO target_notes (target_type, target_id, content, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?4)
                 ON CONFLICT(target_type, target_id) DO UPDATE SET
                    content = excluded.content,
                    updated_at = excluded.updated_at",
                params![
                    note.target_type,
                    note.target_id,
                    note.content,
                    fixture.last_sync_at.as_str()
                ],
            )?;
        }

        transaction.execute(
            "INSERT INTO app_settings (key, value, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET
                value = excluded.value,
                updated_at = excluded.updated_at",
            params![
                FIXTURE_VERSION_KEY,
                fixture.fixture_version.as_str(),
                fixture.last_sync_at.as_str()
            ],
        )?;

        transaction.commit()?;
        Ok(())
    }

    fn configure_connection(conn: &Connection) -> AppResult<()> {
        conn.busy_timeout(StdDuration::from_secs(2))?;
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;
            PRAGMA synchronous = NORMAL;
            ",
        )?;
        Ok(())
    }
}

fn default_database_path() -> PathBuf {
    let base = dirs::data_local_dir()
        .or_else(dirs::data_dir)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));

    base.join(APP_DATA_DIR_NAME).join(DATABASE_FILE_NAME)
}

fn legacy_database_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("dev-data")
        .join(DATABASE_FILE_NAME)
}

fn sqlite_sidecar_path(path: &Path, suffix: &str) -> PathBuf {
    let file_name = path
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| DATABASE_FILE_NAME.to_string());
    path.with_file_name(format!("{file_name}{suffix}"))
}

fn dev_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("dev-fixtures")
        .join("step2")
        .join("seed.json")
}

fn load_dev_fixture() -> AppResult<SeedFixture> {
    Ok(serde_json::from_str(&fs::read_to_string(
        dev_fixture_path(),
    )?)?)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SeedFixture {
    fixture_version: String,
    latest_trade_date: String,
    last_sync_at: String,
    bar_count: usize,
    indexes: Vec<SeedTarget>,
    symbols: Vec<SeedTarget>,
    boards: Vec<SeedBoard>,
    notes: Vec<SeedNote>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SeedTarget {
    target_id: String,
    display_code: String,
    name: String,
    market: Option<String>,
    security_type: String,
    currency: Option<String>,
    total_shares: Option<f64>,
    circulating_shares: Option<f64>,
    price_seed: f64,
    trend: f64,
    volatility: f64,
    #[serde(default)]
    skip_bars: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SeedBoard {
    board_id: String,
    name: String,
    sort_order: i64,
    composition_algorithm: String,
    members: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SeedNote {
    target_type: String,
    target_id: String,
    content: String,
}

fn insert_symbol(
    transaction: &Transaction<'_>,
    seed: &SeedTarget,
    target_type: &str,
) -> AppResult<()> {
    transaction.execute(
        "INSERT INTO symbols (
            target_id, target_type, display_code, name, market, security_type,
            currency, total_shares, circulating_shares, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
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
            seed.target_id,
            target_type,
            seed.display_code,
            seed.name,
            seed.market,
            seed.security_type,
            seed.currency,
            seed.total_shares,
            seed.circulating_shares,
            now_string()
        ],
    )?;
    Ok(())
}

fn delete_orphan_fixture_symbols(
    transaction: &Transaction<'_>,
    target_ids: &[String],
) -> AppResult<()> {
    if target_ids.is_empty() {
        return Ok(());
    }

    let placeholders = vec!["?"; target_ids.len()].join(", ");
    let sql = format!(
        "DELETE FROM symbols
         WHERE target_type = 'symbol'
           AND target_id IN ({placeholders})
           AND NOT EXISTS (
                SELECT 1
                FROM daily_bars
                WHERE daily_bars.target_id = symbols.target_id
           )
           AND NOT EXISTS (
                SELECT 1
                FROM board_members
                WHERE board_members.target_id = symbols.target_id
           )"
    );
    transaction.execute(&sql, params_from_iter(target_ids.iter()))?;
    Ok(())
}

fn insert_daily_bar(transaction: &Transaction<'_>, bar: &DailyBarRecord) -> AppResult<()> {
    transaction.execute(
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
            bar.target_id,
            bar.trade_date,
            bar.open,
            bar.high,
            bar.low,
            bar.close,
            bar.volume,
            bar.source,
            bar.updated_at
        ],
    )?;
    Ok(())
}

fn insert_sync_state(
    transaction: &Transaction<'_>,
    target_type: &str,
    target_id: &str,
    latest_trade_date: Option<&str>,
    last_sync_at: Option<&str>,
    status: &str,
) -> AppResult<()> {
    transaction.execute(
        "INSERT INTO sync_state (
            target_type, target_id, latest_trade_date, last_sync_at,
            last_sync_status, last_error_code, last_error_message
         ) VALUES (?1, ?2, ?3, ?4, ?5, NULL, NULL)
         ON CONFLICT(target_type, target_id) DO UPDATE SET
            latest_trade_date = excluded.latest_trade_date,
            last_sync_at = excluded.last_sync_at,
            last_sync_status = excluded.last_sync_status,
            last_error_code = excluded.last_error_code,
            last_error_message = excluded.last_error_message",
        params![
            target_type,
            target_id,
            latest_trade_date,
            last_sync_at,
            status
        ],
    )?;
    Ok(())
}

fn upsert_sync_state_row(transaction: &Transaction<'_>, row: &SyncStateRecord) -> AppResult<()> {
    transaction.execute(
        "INSERT INTO sync_state (
            target_type, target_id, latest_trade_date, last_sync_at,
            last_sync_status, last_error_code, last_error_message
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(target_type, target_id) DO UPDATE SET
            latest_trade_date = excluded.latest_trade_date,
            last_sync_at = excluded.last_sync_at,
            last_sync_status = excluded.last_sync_status,
            last_error_code = excluded.last_error_code,
            last_error_message = excluded.last_error_message",
        params![
            row.target_type,
            row.target_id,
            row.latest_trade_date,
            row.last_sync_at,
            row.last_sync_status,
            row.last_error_code,
            row.last_error_message
        ],
    )?;
    Ok(())
}

fn generate_bars(seed: &SeedTarget, dates: &[String], source: &str) -> Vec<DailyBarRecord> {
    let mut previous_close = seed.price_seed;

    dates
        .iter()
        .enumerate()
        .map(|(index, trade_date)| {
            let wave =
                ((index as f64 / 11.0).sin() + ((index + 3) as f64 / 17.0).cos()) * seed.volatility;
            let drift = seed.trend * (index as f64 / dates.len() as f64);
            let open = round_to((previous_close + wave * 0.08).max(1.0));
            let close = round_to((open + drift + wave * 0.12).max(1.0));
            let high = round_to(open.max(close) + wave.abs() * 0.05 + 0.8);
            let low = round_to((open.min(close) - wave.abs() * 0.05 - 0.8).max(0.1));
            let volume = Some(round_to(
                250_000.0 + index as f64 * 320.0 + seed.price_seed * 90.0,
            ));
            previous_close = close;

            DailyBarRecord {
                target_id: seed.target_id.clone(),
                trade_date: trade_date.clone(),
                open,
                high,
                low,
                close,
                volume,
                source: source.to_string(),
                updated_at: now_string(),
            }
        })
        .collect()
}

fn build_trading_dates(latest_trade_date: &str, count: usize) -> AppResult<Vec<String>> {
    let mut dates = Vec::with_capacity(count);
    let mut current = NaiveDate::parse_from_str(latest_trade_date, "%Y-%m-%d")
        .map_err(|error| AppError::Message(error.to_string()))?;

    while dates.len() < count {
        if current.weekday().number_from_monday() <= 5 {
            dates.push(current.format("%Y-%m-%d").to_string());
        }
        current -= Duration::days(1);
    }

    dates.reverse();
    Ok(dates)
}

fn assign_equal_weight(symbols: &[String]) -> Vec<MemberSummary> {
    if symbols.is_empty() {
        return Vec::new();
    }

    let weight = round_to(100.0 / symbols.len() as f64);
    let mut allocated = 0.0;
    symbols
        .iter()
        .enumerate()
        .map(|(index, symbol)| {
            let weight_percent = if index + 1 == symbols.len() {
                round_to(100.0 - allocated)
            } else {
                allocated += weight;
                weight
            };
            MemberSummary {
                symbol: symbol.clone(),
                weight_percent: Some(weight_percent),
            }
        })
        .collect()
}

fn assign_market_cap_weight(
    symbols: &[String],
    symbol_rows: &[SymbolRecord],
    latest_closes: &HashMap<String, f64>,
) -> AppResult<Vec<MemberSummary>> {
    let weights =
        resolve_snapshot_weights(symbols, symbol_rows, latest_closes, "market_cap_weight_v1")?;

    let mut allocated = 0.0;
    Ok(weights
        .into_iter()
        .enumerate()
        .map(|(index, (symbol, weight_ratio))| {
            let weight_percent = if index + 1 == symbols.len() {
                round_to(100.0 - allocated)
            } else {
                let value = round_to(weight_ratio * 100.0);
                allocated += value;
                value
            };
            MemberSummary {
                symbol,
                weight_percent: Some(weight_percent),
            }
        })
        .collect())
}

fn map_board_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<BoardRecord> {
    Ok(BoardRecord {
        board_id: row.get(0)?,
        name: row.get(1)?,
        sort_order: row.get(2)?,
        composition_algorithm: row.get(3)?,
        build_status: row.get(4)?,
        build_phase: row.get(5)?,
        build_total: row.get::<_, i64>(6)? as usize,
        build_completed: row.get::<_, i64>(7)? as usize,
        build_failed: row.get::<_, i64>(8)? as usize,
        build_job_id: row.get(9)?,
        build_message: row.get(10)?,
        build_started_at: row.get(11)?,
        build_finished_at: row.get(12)?,
        created_at: row.get(13)?,
        updated_at: row.get(14)?,
    })
}

fn map_symbol_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<SymbolRecord> {
    Ok(SymbolRecord {
        target_id: row.get(0)?,
        target_type: row.get(1)?,
        display_code: row.get(2)?,
        name: row.get(3)?,
        market: row.get(4)?,
        security_type: row.get(5)?,
        currency: row.get(6)?,
        total_shares: row.get(7)?,
        circulating_shares: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

fn map_daily_bar_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<DailyBarRecord> {
    Ok(DailyBarRecord {
        target_id: row.get(0)?,
        trade_date: row.get(1)?,
        open: row.get(2)?,
        high: row.get(3)?,
        low: row.get(4)?,
        close: row.get(5)?,
        volume: row.get(6)?,
        source: row.get(7)?,
        updated_at: row.get(8)?,
    })
}

pub fn now_string() -> String {
    Utc::now().to_rfc3339()
}

fn round_to(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_database_path_uses_user_data_directory_shape() {
        let path = default_database_path();
        let path_str = path.to_string_lossy();

        assert!(
            path_str.ends_with("new_stock/new_stock.sqlite3")
                || path_str.ends_with("new_stock\\new_stock.sqlite3"),
            "default path should live under a user data directory: {path_str}"
        );
        assert!(
            !path_str.contains("src-tauri/dev-data"),
            "release default path must not point at the source tree: {path_str}"
        );
    }

    #[test]
    fn sqlite_sidecar_path_appends_suffix_to_database_name() {
        let path = PathBuf::from("/tmp/new_stock.sqlite3");
        assert_eq!(
            sqlite_sidecar_path(&path, "-wal"),
            PathBuf::from("/tmp/new_stock.sqlite3-wal")
        );
    }
}
