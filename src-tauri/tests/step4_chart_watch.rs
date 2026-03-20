use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::Utc;
use new_stock_lib::jobs::chart_watch;
use new_stock_lib::models::{
    ChartWatchStatusPayload, DailyBarRecord, GetChartPayload, LiveQuoteOverlayPayload,
    SaveBoardPayload, StartChartWatchPayload, SymbolRecord,
};
use new_stock_lib::repositories::Database;
use new_stock_lib::services::market_data::{
    MarketDataProvider, MarketDataTarget, ProviderBar, ProviderMarketStatus, ProviderQuote,
    ProviderSecurity,
};
use new_stock_lib::services::{AppRuntime, BoardService, ChartService};
use tokio::time::{sleep, timeout, Duration as TokioDuration};
use uuid::Uuid;

fn temp_db_path(test_name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "new-stock-step4-{test_name}-{}.sqlite3",
        Uuid::new_v4()
    ));
    if path.exists() {
        let _ = fs::remove_file(&path);
    }
    path
}

fn chart_request(
    target_type: &str,
    target_id: &str,
    granularity: &str,
    board_algorithm: Option<&str>,
) -> GetChartPayload {
    GetChartPayload {
        target_type: target_type.to_string(),
        target_id: target_id.to_string(),
        granularity: Some(granularity.to_string()),
        range: Some("1y".to_string()),
        board_algorithm: board_algorithm.map(str::to_string),
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
    let bars = rows
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
        .collect::<Vec<_>>();
    database
        .save_sync_batch(&bars, &[])
        .expect("daily bars should save");
}

#[derive(Debug, Clone)]
struct QuoteSnapshot {
    prev_close: f64,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
}

#[derive(Debug)]
struct Step4ProviderState {
    market_status: Mutex<ProviderMarketStatus>,
    quotes: Mutex<HashMap<String, QuoteSnapshot>>,
    quote_batch_calls: AtomicUsize,
}

impl Step4ProviderState {
    fn open(trade_date: &str) -> Self {
        Self {
            market_status: Mutex::new(ProviderMarketStatus {
                market: "US".to_string(),
                trade_date: trade_date.to_string(),
                market_state: "open".to_string(),
            }),
            quotes: Mutex::new(HashMap::new()),
            quote_batch_calls: AtomicUsize::new(0),
        }
    }
}

#[derive(Clone, Debug)]
struct Step4Provider {
    state: Arc<Step4ProviderState>,
}

impl Step4Provider {
    fn open(trade_date: &str) -> Self {
        Self {
            state: Arc::new(Step4ProviderState::open(trade_date)),
        }
    }

    fn closed(trade_date: &str) -> Self {
        let provider = Self::open(trade_date);
        provider
            .state
            .market_status
            .lock()
            .expect("lock should succeed")
            .market_state = "closed".to_string();
        provider
    }

    fn with_quote(self, target_id: &str, snapshot: QuoteSnapshot) -> Self {
        self.state
            .quotes
            .lock()
            .expect("lock should succeed")
            .insert(target_id.to_string(), snapshot);
        self
    }
}

#[async_trait]
impl MarketDataProvider for Step4Provider {
    fn provider_name(&self) -> &'static str {
        "step4-provider"
    }

    async fn latest_trade_date(&self, _market: &str) -> new_stock_lib::errors::AppResult<String> {
        Ok(self
            .state
            .market_status
            .lock()
            .expect("lock should succeed")
            .trade_date
            .clone())
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

    async fn market_status(
        &self,
        _market: &str,
    ) -> new_stock_lib::errors::AppResult<ProviderMarketStatus> {
        Ok(self
            .state
            .market_status
            .lock()
            .expect("lock should succeed")
            .clone())
    }

    async fn fetch_realtime_quotes(
        &self,
        targets: &[MarketDataTarget],
    ) -> new_stock_lib::errors::AppResult<Vec<ProviderQuote>> {
        self.state.quote_batch_calls.fetch_add(1, Ordering::SeqCst);
        let quotes = self.state.quotes.lock().expect("lock should succeed");
        let now = chrono::Utc::now().to_rfc3339();
        targets
            .iter()
            .map(|target| {
                let snapshot = quotes.get(&target.target_id).ok_or_else(|| {
                    new_stock_lib::errors::AppError::Message(format!(
                        "missing quote snapshot for {}",
                        target.target_id
                    ))
                })?;

                Ok(ProviderQuote {
                    target_id: target.target_id.clone(),
                    target_type: target.target_type.clone(),
                    provider_symbol: target.provider_symbol.clone(),
                    prev_close: snapshot.prev_close,
                    open: snapshot.open,
                    high: snapshot.high,
                    low: snapshot.low,
                    close: snapshot.close,
                    volume: Some(snapshot.volume),
                    updated_at: now.clone(),
                    source_status: "live".to_string(),
                })
            })
            .collect()
    }
}

#[derive(Debug)]
struct SlowMarketStatusProviderState {
    quote: QuoteSnapshot,
    market_status_calls: AtomicUsize,
    quote_batch_calls: AtomicUsize,
}

#[derive(Clone, Debug)]
struct SlowMarketStatusProvider {
    state: Arc<SlowMarketStatusProviderState>,
}

impl SlowMarketStatusProvider {
    fn new(quote: QuoteSnapshot) -> Self {
        Self {
            state: Arc::new(SlowMarketStatusProviderState {
                quote,
                market_status_calls: AtomicUsize::new(0),
                quote_batch_calls: AtomicUsize::new(0),
            }),
        }
    }
}

#[async_trait]
impl MarketDataProvider for SlowMarketStatusProvider {
    fn provider_name(&self) -> &'static str {
        "slow-market-status-provider"
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

    async fn market_status(
        &self,
        _market: &str,
    ) -> new_stock_lib::errors::AppResult<ProviderMarketStatus> {
        self.state
            .market_status_calls
            .fetch_add(1, Ordering::SeqCst);
        sleep(TokioDuration::from_secs(1)).await;
        Ok(fixed_market_status("2026-03-19", "open"))
    }

    async fn fetch_realtime_quotes(
        &self,
        targets: &[MarketDataTarget],
    ) -> new_stock_lib::errors::AppResult<Vec<ProviderQuote>> {
        self.state.quote_batch_calls.fetch_add(1, Ordering::SeqCst);
        let now = chrono::Utc::now().to_rfc3339();
        Ok(targets
            .iter()
            .map(|target| ProviderQuote {
                target_id: target.target_id.clone(),
                target_type: target.target_type.clone(),
                provider_symbol: target.provider_symbol.clone(),
                prev_close: self.state.quote.prev_close,
                open: self.state.quote.open,
                high: self.state.quote.high,
                low: self.state.quote.low,
                close: self.state.quote.close,
                volume: Some(self.state.quote.volume),
                updated_at: now.clone(),
                source_status: "live".to_string(),
            })
            .collect())
    }
}

#[derive(Debug)]
struct WarmableQuoteProviderState {
    quote: QuoteSnapshot,
    warmed: AtomicBool,
    prewarm_calls: AtomicUsize,
    quote_batch_calls: AtomicUsize,
}

#[derive(Clone, Debug)]
struct WarmableQuoteProvider {
    state: Arc<WarmableQuoteProviderState>,
}

impl WarmableQuoteProvider {
    fn new(quote: QuoteSnapshot) -> Self {
        Self {
            state: Arc::new(WarmableQuoteProviderState {
                quote,
                warmed: AtomicBool::new(false),
                prewarm_calls: AtomicUsize::new(0),
                quote_batch_calls: AtomicUsize::new(0),
            }),
        }
    }
}

#[async_trait]
impl MarketDataProvider for WarmableQuoteProvider {
    fn provider_name(&self) -> &'static str {
        "warmable-quote-provider"
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
        sleep(TokioDuration::from_millis(150)).await;
        self.state.warmed.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn market_status(
        &self,
        _market: &str,
    ) -> new_stock_lib::errors::AppResult<ProviderMarketStatus> {
        Ok(fixed_market_status("2026-03-19", "open"))
    }

    async fn fetch_realtime_quotes(
        &self,
        targets: &[MarketDataTarget],
    ) -> new_stock_lib::errors::AppResult<Vec<ProviderQuote>> {
        self.state.quote_batch_calls.fetch_add(1, Ordering::SeqCst);
        if !self.state.warmed.load(Ordering::SeqCst) {
            sleep(TokioDuration::from_millis(250)).await;
        }
        let now = chrono::Utc::now().to_rfc3339();
        Ok(targets
            .iter()
            .map(|target| ProviderQuote {
                target_id: target.target_id.clone(),
                target_type: target.target_type.clone(),
                provider_symbol: target.provider_symbol.clone(),
                prev_close: self.state.quote.prev_close,
                open: self.state.quote.open,
                high: self.state.quote.high,
                low: self.state.quote.low,
                close: self.state.quote.close,
                volume: Some(self.state.quote.volume),
                updated_at: now.clone(),
                source_status: "live".to_string(),
            })
            .collect())
    }
}

#[tokio::test]
async fn step4_rejects_non_day_watch_requests() {
    let database = Database::at(temp_db_path("reject-week"));
    database.bootstrap().expect("bootstrap should succeed");
    let runtime = AppRuntime::for_tests(Arc::new(Step4Provider::open("2026-03-19")))
        .with_market_status_override(fixed_market_status("2026-03-19", "open"));

    let error = chart_watch::start(
        database,
        runtime,
        StartChartWatchPayload {
            target_type: "symbol".to_string(),
            target_id: "NVDA".to_string(),
            granularity: Some("week".to_string()),
            board_algorithm: None,
        },
        Arc::new(|_payload| {}),
    )
    .await
    .expect_err("week watch should be rejected");

    assert!(error.to_string().contains("WATCH_UNSUPPORTED_GRANULARITY"));
}

#[tokio::test]
async fn step4_returns_closed_state_without_starting_task_when_market_is_closed() {
    let database = Database::at(temp_db_path("market-closed"));
    database.bootstrap().expect("bootstrap should succeed");
    let runtime = AppRuntime::for_tests(Arc::new(Step4Provider::closed("2026-03-19").with_quote(
        "NVDA",
        QuoteSnapshot {
            prev_close: 100.0,
            open: 101.0,
            high: 102.0,
            low: 99.5,
            close: 101.5,
            volume: 10_000.0,
        },
    )))
    .with_market_status_override(fixed_market_status("2026-03-19", "closed"));
    let events = Arc::new(Mutex::new(Vec::<LiveQuoteOverlayPayload>::new()));

    let status = chart_watch::start(
        database,
        runtime.clone(),
        StartChartWatchPayload {
            target_type: "symbol".to_string(),
            target_id: "NVDA".to_string(),
            granularity: Some("day".to_string()),
            board_algorithm: None,
        },
        Arc::new({
            let events = events.clone();
            move |payload| {
                events.lock().expect("lock should succeed").push(payload);
            }
        }),
    )
    .await
    .expect("closed market should return graceful status");

    assert!(!status.started);
    assert_eq!(status.market_state, "closed");
    assert!(status
        .message
        .as_deref()
        .unwrap_or_default()
        .contains("盘中"));
    sleep(TokioDuration::from_millis(50)).await;
    assert!(events.lock().expect("lock should succeed").is_empty());

    let stop_status = chart_watch::stop(runtime)
        .await
        .expect("stop should remain idempotent");
    assert!(stop_status.stopped);
    assert!(stop_status.watch_id.is_none());
}

#[tokio::test]
async fn step4_reuses_same_active_watch_and_merges_symbol_overlay_into_day_chart() {
    let database = Database::at(temp_db_path("symbol-overlay"));
    database.bootstrap().expect("bootstrap should succeed");
    let provider = Step4Provider::open("2026-03-19").with_quote(
        "NVDA",
        QuoteSnapshot {
            prev_close: 125.0,
            open: 126.0,
            high: 129.0,
            low: 124.5,
            close: 128.5,
            volume: 21_000.0,
        },
    );
    let provider_state = provider.state.clone();
    let runtime = AppRuntime::for_tests(Arc::new(provider))
        .with_market_status_override(fixed_market_status("2026-03-19", "open"));
    let events = Arc::new(Mutex::new(Vec::<LiveQuoteOverlayPayload>::new()));

    let first = start_symbol_watch(database.clone(), runtime.clone(), events.clone()).await;
    let second = start_symbol_watch(database.clone(), runtime.clone(), events.clone()).await;
    assert_eq!(first.watch_id, second.watch_id);

    sleep(TokioDuration::from_millis(50)).await;
    assert_eq!(provider_state.quote_batch_calls.load(Ordering::SeqCst), 1);

    let day_chart = ChartService::with_runtime(database.clone(), runtime.clone())
        .get_chart(chart_request("symbol", "NVDA", "day", None))
        .expect("day chart should load");
    assert_eq!(
        day_chart
            .active_overlay
            .as_ref()
            .map(|item| item.kind.as_str()),
        Some("live_quote")
    );
    assert_eq!(day_chart.bars.last().map(|bar| bar.close), Some(128.5));

    let week_chart = ChartService::with_runtime(database.clone(), runtime.clone())
        .get_chart(chart_request("symbol", "NVDA", "week", None))
        .expect("week chart should load");
    assert!(week_chart.active_overlay.is_none());

    let stop_status = chart_watch::stop(runtime.clone())
        .await
        .expect("stop should succeed");
    assert_eq!(
        stop_status.watch_id.as_deref(),
        Some(first.watch_id.as_str())
    );

    let day_chart_after_stop = ChartService::with_runtime(database, runtime)
        .get_chart(chart_request("symbol", "NVDA", "day", None))
        .expect("day chart should load after stop");
    assert!(day_chart_after_stop.active_overlay.is_none());
}

#[tokio::test]
async fn step4_emits_board_overlay_and_deduplicates_member_quote_fetch() {
    let database = Database::at(temp_db_path("board-overlay"));
    database.bootstrap().expect("bootstrap should succeed");
    let provider = Step4Provider::open("2026-03-19")
        .with_quote(
            "NVDA",
            QuoteSnapshot {
                prev_close: 100.0,
                open: 110.0,
                high: 112.0,
                low: 109.0,
                close: 111.0,
                volume: 1_000.0,
            },
        )
        .with_quote(
            "AMD",
            QuoteSnapshot {
                prev_close: 50.0,
                open: 52.0,
                high: 53.0,
                low: 51.0,
                close: 52.5,
                volume: 800.0,
            },
        )
        .with_quote(
            "AVGO",
            QuoteSnapshot {
                prev_close: 200.0,
                open: 204.0,
                high: 208.0,
                low: 203.0,
                close: 206.0,
                volume: 600.0,
            },
        );
    let provider_state = provider.state.clone();
    let runtime = AppRuntime::for_tests(Arc::new(provider))
        .with_market_status_override(fixed_market_status("2026-03-19", "open"));
    let events = Arc::new(Mutex::new(Vec::<LiveQuoteOverlayPayload>::new()));

    let historical_board_chart = ChartService::with_runtime(database.clone(), runtime.clone())
        .get_chart(chart_request(
            "board",
            "board-ai",
            "day",
            Some("equal_weight_v1"),
        ))
        .expect("board chart should load");
    let previous_close = historical_board_chart
        .bars
        .last()
        .map(|bar| bar.close)
        .expect("board should have historical close");

    let status = chart_watch::start(
        database.clone(),
        runtime,
        StartChartWatchPayload {
            target_type: "board".to_string(),
            target_id: "board-ai".to_string(),
            granularity: Some("day".to_string()),
            board_algorithm: None,
        },
        Arc::new({
            let events = events.clone();
            move |payload| {
                events.lock().expect("lock should succeed").push(payload);
            }
        }),
    )
    .await
    .expect("board watch should start");

    assert!(status.started);
    sleep(TokioDuration::from_millis(50)).await;

    let emitted = events.lock().expect("lock should succeed");
    let payload = emitted.last().expect("board event should emit");
    let expected_close =
        round2(previous_close * ((111.0 / 100.0 + 52.5 / 50.0 + 206.0 / 200.0) / 3.0));
    assert_eq!(payload.overlay.trade_date, "2026-03-19");
    assert_eq!(payload.overlay.close, expected_close);
    assert_eq!(provider_state.quote_batch_calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn step4_market_cap_board_overlay_uses_same_static_weight_snapshot_as_history() {
    let database = Database::at(temp_db_path("board-overlay-market-cap-snapshot"));
    database.bootstrap().expect("bootstrap should succeed");
    database
        .purge_fixture_data()
        .expect("fixture data should be removable for this regression test");

    insert_symbol(&database, "ALPHA", Some(1.0), Some(1.0));
    insert_symbol(&database, "BETA", Some(1.0), Some(1.0));

    insert_daily_bars(
        &database,
        "ALPHA",
        &[
            ("2026-03-17", 100.0, 100.0, 100.0, 100.0),
            ("2026-03-18", 10.0, 10.0, 10.0, 10.0),
        ],
    );
    insert_daily_bars(
        &database,
        "BETA",
        &[
            ("2026-03-17", 100.0, 100.0, 100.0, 100.0),
            ("2026-03-18", 90.0, 90.0, 90.0, 90.0),
        ],
    );

    let board = BoardService::new(database.clone())
        .save(SaveBoardPayload {
            board_id: None,
            name: "Snapshot Board".to_string(),
            members: vec!["ALPHA".to_string(), "BETA".to_string()],
            composition_algorithm: "market_cap_weight_v1".to_string(),
        })
        .expect("board should save through fast path");
    assert!(!board.background_sync_started);

    let provider = Step4Provider::open("2026-03-19")
        .with_quote(
            "ALPHA",
            QuoteSnapshot {
                prev_close: 10.0,
                open: 100.0,
                high: 100.0,
                low: 100.0,
                close: 100.0,
                volume: 1_000.0,
            },
        )
        .with_quote(
            "BETA",
            QuoteSnapshot {
                prev_close: 90.0,
                open: 90.0,
                high: 90.0,
                low: 90.0,
                close: 90.0,
                volume: 1_000.0,
            },
        );
    let runtime = AppRuntime::for_tests(Arc::new(provider))
        .with_market_status_override(fixed_market_status("2026-03-19", "open"));
    let events = Arc::new(Mutex::new(Vec::<LiveQuoteOverlayPayload>::new()));

    let historical_board_chart = ChartService::with_runtime(database.clone(), runtime.clone())
        .get_chart(chart_request(
            "board",
            &board.board_id,
            "day",
            Some("market_cap_weight_v1"),
        ))
        .expect("board chart should load");
    let previous_close = historical_board_chart
        .bars
        .last()
        .map(|bar| bar.close)
        .expect("board should have previous close");
    assert_eq!(previous_close, 82.0);

    let status = chart_watch::start(
        database,
        runtime,
        StartChartWatchPayload {
            target_type: "board".to_string(),
            target_id: board.board_id,
            granularity: Some("day".to_string()),
            board_algorithm: Some("market_cap_weight_v1".to_string()),
        },
        Arc::new({
            let events = events.clone();
            move |payload| {
                events.lock().expect("lock should succeed").push(payload);
            }
        }),
    )
    .await
    .expect("board watch should start");

    assert!(status.started);
    sleep(TokioDuration::from_millis(50)).await;

    let emitted = events.lock().expect("lock should succeed");
    let payload = emitted.last().expect("board event should emit");
    let expected_close = round2(previous_close * ((0.1 * (100.0 / 10.0)) + (0.9 * (90.0 / 90.0))));
    assert_eq!(payload.overlay.trade_date, "2026-03-19");
    assert_eq!(payload.overlay.close, expected_close);
}

#[tokio::test]
async fn step4_market_cap_board_overlay_remains_continuous_without_gap() {
    let database = Database::at(temp_db_path("board-overlay-market-cap-continuous"));
    database.bootstrap().expect("bootstrap should succeed");
    database
        .purge_fixture_data()
        .expect("fixture data should be removable for this regression test");

    insert_symbol(&database, "ALPHA", Some(1.0), Some(1.0));
    insert_symbol(&database, "BETA", Some(1.0), Some(1.0));

    insert_daily_bars(
        &database,
        "ALPHA",
        &[
            ("2026-03-17", 100.0, 100.0, 100.0, 100.0),
            ("2026-03-18", 10.0, 10.0, 10.0, 10.0),
        ],
    );
    insert_daily_bars(
        &database,
        "BETA",
        &[
            ("2026-03-17", 100.0, 100.0, 100.0, 100.0),
            ("2026-03-18", 90.0, 90.0, 90.0, 90.0),
        ],
    );

    let board = BoardService::new(database.clone())
        .save(SaveBoardPayload {
            board_id: None,
            name: "Continuous Snapshot Board".to_string(),
            members: vec!["ALPHA".to_string(), "BETA".to_string()],
            composition_algorithm: "market_cap_weight_v1".to_string(),
        })
        .expect("board should save through fast path");

    let provider = Step4Provider::open("2026-03-19")
        .with_quote(
            "ALPHA",
            QuoteSnapshot {
                prev_close: 10.0,
                open: 10.0,
                high: 10.0,
                low: 10.0,
                close: 10.0,
                volume: 1_000.0,
            },
        )
        .with_quote(
            "BETA",
            QuoteSnapshot {
                prev_close: 90.0,
                open: 90.0,
                high: 90.0,
                low: 90.0,
                close: 90.0,
                volume: 1_000.0,
            },
        );
    let runtime = AppRuntime::for_tests(Arc::new(provider))
        .with_market_status_override(fixed_market_status("2026-03-19", "open"));
    let events = Arc::new(Mutex::new(Vec::<LiveQuoteOverlayPayload>::new()));

    let historical_board_chart = ChartService::with_runtime(database.clone(), runtime.clone())
        .get_chart(chart_request(
            "board",
            &board.board_id,
            "day",
            Some("market_cap_weight_v1"),
        ))
        .expect("board chart should load");
    let previous_close = historical_board_chart
        .bars
        .last()
        .map(|bar| bar.close)
        .expect("board should have previous close");

    let status = chart_watch::start(
        database,
        runtime,
        StartChartWatchPayload {
            target_type: "board".to_string(),
            target_id: board.board_id,
            granularity: Some("day".to_string()),
            board_algorithm: Some("market_cap_weight_v1".to_string()),
        },
        Arc::new({
            let events = events.clone();
            move |payload| {
                events.lock().expect("lock should succeed").push(payload);
            }
        }),
    )
    .await
    .expect("board watch should start");

    assert!(status.started);
    sleep(TokioDuration::from_millis(50)).await;

    let emitted = events.lock().expect("lock should succeed");
    let payload = emitted.last().expect("board event should emit");
    assert_eq!(payload.overlay.trade_date, "2026-03-19");
    assert_eq!(payload.overlay.open, previous_close);
    assert_eq!(payload.overlay.high, previous_close);
    assert_eq!(payload.overlay.low, previous_close);
    assert_eq!(payload.overlay.close, previous_close);
}

#[tokio::test]
async fn step4_market_cap_board_chart_and_overlay_share_snapshot_metadata() {
    let database = Database::at(temp_db_path("board-overlay-market-cap-metadata"));
    database.bootstrap().expect("bootstrap should succeed");
    database
        .purge_fixture_data()
        .expect("fixture data should be removable for this regression test");

    insert_symbol(&database, "ALPHA", Some(1.0), Some(1.0));
    insert_symbol(&database, "BETA", Some(1.0), Some(1.0));

    insert_daily_bars(
        &database,
        "ALPHA",
        &[
            ("2026-03-17", 100.0, 100.0, 100.0, 100.0),
            ("2026-03-18", 10.0, 10.0, 10.0, 10.0),
        ],
    );
    insert_daily_bars(
        &database,
        "BETA",
        &[
            ("2026-03-17", 100.0, 100.0, 100.0, 100.0),
            ("2026-03-18", 90.0, 90.0, 90.0, 90.0),
        ],
    );

    let board = BoardService::new(database.clone())
        .save(SaveBoardPayload {
            board_id: None,
            name: "Metadata Snapshot Board".to_string(),
            members: vec!["ALPHA".to_string(), "BETA".to_string()],
            composition_algorithm: "market_cap_weight_v1".to_string(),
        })
        .expect("board should save through fast path");

    let provider = Step4Provider::open("2026-03-19")
        .with_quote(
            "ALPHA",
            QuoteSnapshot {
                prev_close: 10.0,
                open: 10.0,
                high: 10.0,
                low: 10.0,
                close: 10.0,
                volume: 1_000.0,
            },
        )
        .with_quote(
            "BETA",
            QuoteSnapshot {
                prev_close: 90.0,
                open: 90.0,
                high: 90.0,
                low: 90.0,
                close: 90.0,
                volume: 1_000.0,
            },
        );
    let runtime = AppRuntime::for_tests(Arc::new(provider))
        .with_market_status_override(fixed_market_status("2026-03-19", "open"));
    let events = Arc::new(Mutex::new(Vec::<LiveQuoteOverlayPayload>::new()));

    let historical_board_chart = ChartService::with_runtime(database.clone(), runtime.clone())
        .get_chart(chart_request(
            "board",
            &board.board_id,
            "day",
            Some("market_cap_weight_v1"),
        ))
        .expect("board chart should load");
    assert_eq!(
        historical_board_chart.meta.value_mode.as_deref(),
        Some("synthetic_board_points")
    );
    assert_eq!(
        historical_board_chart.meta.weight_snapshot.as_deref(),
        Some("previous_close_x_shares")
    );
    assert_eq!(
        historical_board_chart
            .meta
            .weight_snapshot_trade_date
            .as_deref(),
        Some("2026-03-18")
    );

    let status = chart_watch::start(
        database.clone(),
        runtime.clone(),
        StartChartWatchPayload {
            target_type: "board".to_string(),
            target_id: board.board_id.clone(),
            granularity: Some("day".to_string()),
            board_algorithm: Some("market_cap_weight_v1".to_string()),
        },
        Arc::new({
            let events = events.clone();
            move |payload| {
                events.lock().expect("lock should succeed").push(payload);
            }
        }),
    )
    .await
    .expect("board watch should start");

    assert!(status.started);
    sleep(TokioDuration::from_millis(50)).await;

    let payload = events
        .lock()
        .expect("lock should succeed")
        .last()
        .cloned()
        .expect("board event should emit");
    assert_eq!(
        payload.board_algorithm.as_deref(),
        Some("market_cap_weight_v1")
    );
    assert_eq!(
        payload.meta.value_mode.as_deref(),
        Some("synthetic_board_points")
    );
    assert_eq!(
        payload.meta.weight_snapshot.as_deref(),
        Some("previous_close_x_shares")
    );
    assert_eq!(
        payload.meta.weight_snapshot_trade_date.as_deref(),
        Some("2026-03-18")
    );

    let merged_chart = ChartService::with_runtime(database, runtime)
        .get_chart(chart_request(
            "board",
            &board.board_id,
            "day",
            Some("market_cap_weight_v1"),
        ))
        .expect("merged board chart should load");
    assert_eq!(merged_chart.source_status, payload.source_status);
    assert_eq!(merged_chart.meta.source_status, payload.source_status);
    assert_eq!(merged_chart.meta.value_mode, payload.meta.value_mode);
    assert_eq!(
        merged_chart.meta.weight_snapshot,
        payload.meta.weight_snapshot
    );
    assert_eq!(
        merged_chart.meta.weight_snapshot_trade_date,
        payload.meta.weight_snapshot_trade_date
    );
    assert_eq!(
        merged_chart.bars.last().map(|bar| bar.close),
        Some(payload.overlay.close)
    );
}

#[tokio::test]
async fn step4_start_returns_without_waiting_for_provider_market_status() {
    let database = Database::at(temp_db_path("fast-start"));
    database.bootstrap().expect("bootstrap should succeed");
    let provider = SlowMarketStatusProvider::new(QuoteSnapshot {
        prev_close: 125.0,
        open: 126.0,
        high: 129.0,
        low: 124.5,
        close: 128.5,
        volume: 21_000.0,
    });
    let provider_state = provider.state.clone();
    let runtime = AppRuntime::for_tests(Arc::new(provider))
        .with_market_status_override(fixed_market_status("2026-03-19", "open"));
    let events = Arc::new(Mutex::new(Vec::<LiveQuoteOverlayPayload>::new()));

    let status = timeout(
        TokioDuration::from_millis(100),
        chart_watch::start(
            database,
            runtime.clone(),
            StartChartWatchPayload {
                target_type: "symbol".to_string(),
                target_id: "NVDA".to_string(),
                granularity: Some("day".to_string()),
                board_algorithm: None,
            },
            Arc::new({
                let events = events.clone();
                move |payload| {
                    events.lock().expect("lock should succeed").push(payload);
                }
            }),
        ),
    )
    .await
    .expect("start should return quickly")
    .expect("watch should start");

    assert!(status.started);
    sleep(TokioDuration::from_millis(50)).await;
    assert_eq!(provider_state.market_status_calls.load(Ordering::SeqCst), 0);
    assert_eq!(provider_state.quote_batch_calls.load(Ordering::SeqCst), 1);
    assert!(!events.lock().expect("lock should succeed").is_empty());

    let stop_status = chart_watch::stop(runtime)
        .await
        .expect("stop should succeed");
    assert_eq!(
        stop_status.watch_id.as_deref(),
        Some(status.watch_id.as_str())
    );
}

#[tokio::test]
async fn step4_prewarm_provider_reduces_first_live_event_cold_start() {
    let database = Database::at(temp_db_path("prewarm-live-event"));
    database.bootstrap().expect("bootstrap should succeed");
    let provider = WarmableQuoteProvider::new(QuoteSnapshot {
        prev_close: 125.0,
        open: 126.0,
        high: 129.0,
        low: 124.5,
        close: 128.5,
        volume: 21_000.0,
    });
    let provider_state = provider.state.clone();
    let runtime = AppRuntime::for_tests(Arc::new(provider))
        .with_market_status_override(fixed_market_status("2026-03-19", "open"));
    runtime
        .prewarm_provider()
        .await
        .expect("prewarm should succeed");

    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<LiveQuoteOverlayPayload>();
    let status = chart_watch::start(
        database,
        runtime.clone(),
        StartChartWatchPayload {
            target_type: "symbol".to_string(),
            target_id: "NVDA".to_string(),
            granularity: Some("day".to_string()),
            board_algorithm: None,
        },
        Arc::new(move |payload| {
            let _ = tx.send(payload);
        }),
    )
    .await
    .expect("watch should start");

    let event = timeout(TokioDuration::from_millis(120), rx.recv())
        .await
        .expect("first live event should arrive after prewarm")
        .expect("event should exist");
    assert_eq!(event.watch_id, status.watch_id);
    assert_eq!(provider_state.prewarm_calls.load(Ordering::SeqCst), 1);
    assert_eq!(provider_state.quote_batch_calls.load(Ordering::SeqCst), 1);

    let stop_status = chart_watch::stop(runtime)
        .await
        .expect("stop should succeed");
    assert_eq!(
        stop_status.watch_id.as_deref(),
        Some(status.watch_id.as_str())
    );
}

async fn start_symbol_watch(
    database: Database,
    runtime: AppRuntime,
    events: Arc<Mutex<Vec<LiveQuoteOverlayPayload>>>,
) -> ChartWatchStatusPayload {
    chart_watch::start(
        database,
        runtime,
        StartChartWatchPayload {
            target_type: "symbol".to_string(),
            target_id: "NVDA".to_string(),
            granularity: Some("day".to_string()),
            board_algorithm: None,
        },
        Arc::new(move |payload| {
            events.lock().expect("lock should succeed").push(payload);
        }),
    )
    .await
    .expect("watch should start")
}

fn round2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

fn fixed_market_status(trade_date: &str, market_state: &str) -> ProviderMarketStatus {
    ProviderMarketStatus {
        market: "US".to_string(),
        trade_date: trade_date.to_string(),
        market_state: market_state.to_string(),
    }
}
