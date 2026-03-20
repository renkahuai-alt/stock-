use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Instant;

use tokio::sync::{Mutex, Semaphore};
use tokio::task::AbortHandle;

use crate::errors::AppResult;
use crate::models::{BarPoint, ChartPayload, DailyBarRecord, LiveQuoteOverlayPayload};
use crate::telemetry;

use super::market_data::{
    local_market_status, provider_from_credentials, MarketDataProvider, ProviderMarketStatus,
};

#[derive(Clone)]
pub struct AppRuntime {
    provider_override: Option<Arc<dyn MarketDataProvider>>,
    provider_cache: Arc<RwLock<Option<Arc<dyn MarketDataProvider>>>>,
    provider_prewarm_inflight: Arc<AtomicBool>,
    market_status_override: Option<ProviderMarketStatus>,
    raw_daily_cache: Arc<RwLock<HashMap<String, Vec<DailyBarRecord>>>>,
    weekly_cache: Arc<RwLock<HashMap<String, Vec<BarPoint>>>>,
    chart_payload_cache: Arc<RwLock<HashMap<String, ChartPayload>>>,
    live_overlay_cache: Arc<RwLock<HashMap<String, LiveQuoteOverlayPayload>>>,
    board_build_gate: Arc<Semaphore>,
    active_watch: Arc<Mutex<Option<ActiveWatchHandle>>>,
}

impl Default for AppRuntime {
    fn default() -> Self {
        Self {
            provider_override: None,
            provider_cache: Arc::new(RwLock::new(None)),
            provider_prewarm_inflight: Arc::new(AtomicBool::new(false)),
            market_status_override: None,
            raw_daily_cache: Arc::new(RwLock::new(HashMap::new())),
            weekly_cache: Arc::new(RwLock::new(HashMap::new())),
            chart_payload_cache: Arc::new(RwLock::new(HashMap::new())),
            live_overlay_cache: Arc::new(RwLock::new(HashMap::new())),
            board_build_gate: Arc::new(Semaphore::new(1)),
            active_watch: Arc::new(Mutex::new(None)),
        }
    }
}

impl AppRuntime {
    pub fn for_tests(provider: Arc<dyn MarketDataProvider>) -> Self {
        Self {
            provider_override: Some(provider),
            ..Self::default()
        }
    }

    pub fn with_market_status_override(mut self, status: ProviderMarketStatus) -> Self {
        self.market_status_override = Some(status);
        self
    }

    pub fn provider(&self) -> AppResult<Arc<dyn MarketDataProvider>> {
        if let Some(provider) = &self.provider_override {
            return Ok(provider.clone());
        }

        if let Some(provider) = self
            .provider_cache
            .read()
            .ok()
            .and_then(|cache| cache.clone())
        {
            return Ok(provider);
        }

        let provider = provider_from_credentials()?;
        if let Ok(mut cache) = self.provider_cache.write() {
            *cache = Some(provider.clone());
        }

        Ok(provider)
    }

    pub fn reset_provider(&self) {
        if let Ok(mut cache) = self.provider_cache.write() {
            *cache = None;
        }
        self.provider_prewarm_inflight
            .store(false, Ordering::SeqCst);
    }

    pub fn market_status(&self, market: &str) -> AppResult<ProviderMarketStatus> {
        if let Some(status) = &self.market_status_override {
            let mut status = status.clone();
            status.market = market.to_string();
            return Ok(status);
        }

        local_market_status(market)
    }

    pub async fn prewarm_provider(&self) -> AppResult<()> {
        let provider = self.provider()?;
        if !provider.is_available() {
            telemetry::emit(
                "provider_prewarm_skipped",
                &[("reason", "provider_unavailable".to_string())],
            );
            return Ok(());
        }

        provider.prewarm().await
    }

    pub fn spawn_provider_prewarm(&self) {
        if self.provider_prewarm_inflight.swap(true, Ordering::SeqCst) {
            telemetry::emit(
                "provider_prewarm_skipped",
                &[("reason", "inflight".to_string())],
            );
            return;
        }

        telemetry::emit(
            "provider_prewarm_started",
            &[("source", "runtime.spawn_provider_prewarm".to_string())],
        );
        let runtime = self.clone();
        tauri::async_runtime::spawn(async move {
            let started_at = Instant::now();
            match runtime.prewarm_provider().await {
                Ok(()) => telemetry::emit(
                    "provider_prewarm_succeeded",
                    &[("elapsedMs", started_at.elapsed().as_millis().to_string())],
                ),
                Err(error) => telemetry::emit(
                    "provider_prewarm_failed",
                    &[
                        ("elapsedMs", started_at.elapsed().as_millis().to_string()),
                        ("error", error.to_string()),
                    ],
                ),
            }
            runtime
                .provider_prewarm_inflight
                .store(false, Ordering::SeqCst);
        });
    }

    pub fn board_build_gate(&self) -> Arc<Semaphore> {
        self.board_build_gate.clone()
    }

    pub fn get_raw_daily(&self, target_id: &str) -> Option<Vec<DailyBarRecord>> {
        self.raw_daily_cache
            .read()
            .ok()
            .and_then(|cache| cache.get(target_id).cloned())
    }

    pub fn put_raw_daily(&self, target_id: &str, bars: &[DailyBarRecord]) {
        if let Ok(mut cache) = self.raw_daily_cache.write() {
            cache.insert(target_id.to_string(), bars.to_vec());
        }
    }

    pub fn get_weekly(&self, key: &str) -> Option<Vec<BarPoint>> {
        self.weekly_cache
            .read()
            .ok()
            .and_then(|cache| cache.get(key).cloned())
    }

    pub fn put_weekly(&self, key: &str, bars: &[BarPoint]) {
        if let Ok(mut cache) = self.weekly_cache.write() {
            cache.insert(key.to_string(), bars.to_vec());
        }
    }

    pub fn get_chart_payload(&self, key: &str) -> Option<ChartPayload> {
        self.chart_payload_cache
            .read()
            .ok()
            .and_then(|cache| cache.get(key).cloned())
    }

    pub fn put_chart_payload(&self, key: &str, payload: &ChartPayload) {
        if let Ok(mut cache) = self.chart_payload_cache.write() {
            cache.insert(key.to_string(), payload.clone());
        }
    }

    pub fn get_live_overlay(&self, key: &str) -> Option<LiveQuoteOverlayPayload> {
        self.live_overlay_cache
            .read()
            .ok()
            .and_then(|cache| cache.get(key).cloned())
    }

    pub fn put_live_overlay(&self, key: &str, payload: &LiveQuoteOverlayPayload) {
        if let Ok(mut cache) = self.live_overlay_cache.write() {
            cache.insert(key.to_string(), payload.clone());
        }
    }

    pub fn clear_live_overlay(&self, key: &str) {
        if let Ok(mut cache) = self.live_overlay_cache.write() {
            cache.remove(key);
        }
    }

    pub async fn active_watch(&self) -> Option<ActiveWatchHandle> {
        self.active_watch.lock().await.clone()
    }

    pub async fn set_active_watch(
        &self,
        next: Option<ActiveWatchHandle>,
    ) -> Option<ActiveWatchHandle> {
        let mut guard = self.active_watch.lock().await;
        std::mem::replace(&mut *guard, next)
    }

    pub async fn clear_active_watch_if(&self, watch_id: &str) -> Option<ActiveWatchHandle> {
        let mut guard = self.active_watch.lock().await;
        match guard.as_ref() {
            Some(active) if active.watch_id == watch_id => guard.take(),
            _ => None,
        }
    }

    pub fn invalidate_targets(&self, target_ids: &[String], board_ids: &[String]) {
        telemetry::emit(
            "cache_invalidation",
            &[
                ("targetCount", target_ids.len().to_string()),
                ("boardCount", board_ids.len().to_string()),
            ],
        );
        if let Ok(mut raw_cache) = self.raw_daily_cache.write() {
            for target_id in target_ids {
                raw_cache.remove(target_id);
            }
        }

        if let Ok(mut weekly_cache) = self.weekly_cache.write() {
            weekly_cache.retain(|key, _| {
                !target_ids.iter().any(|target_id| key.contains(target_id))
                    && !board_ids.iter().any(|board_id| key.contains(board_id))
            });
        }

        if let Ok(mut chart_cache) = self.chart_payload_cache.write() {
            chart_cache.retain(|key, _| {
                !target_ids.iter().any(|target_id| key.contains(target_id))
                    && !board_ids.iter().any(|board_id| key.contains(board_id))
            });
        }

        if let Ok(mut overlay_cache) = self.live_overlay_cache.write() {
            overlay_cache.retain(|key, _| {
                !target_ids.iter().any(|target_id| key.contains(target_id))
                    && !board_ids.iter().any(|board_id| key.contains(board_id))
            });
        }
    }
}

#[derive(Clone)]
pub struct ActiveWatchHandle {
    pub watch_id: String,
    pub overlay_key: String,
    pub target_type: String,
    pub target_id: String,
    pub granularity: String,
    pub board_algorithm: Option<String>,
    pub interval_sec: u64,
    pub abort_handle: AbortHandle,
}

impl ActiveWatchHandle {
    pub fn matches(
        &self,
        target_type: &str,
        target_id: &str,
        granularity: &str,
        board_algorithm: Option<&str>,
    ) -> bool {
        self.target_type == target_type
            && self.target_id == target_id
            && self.granularity == granularity
            && self.board_algorithm.as_deref() == board_algorithm
    }
}
