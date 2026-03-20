use crate::models::{LiveOverlayBar, LiveQuoteOverlayMeta, LiveQuoteOverlayPayload};

#[allow(clippy::too_many_arguments)]
pub fn overlay_payload(
    watch_id: &str,
    target_type: &str,
    target_id: &str,
    granularity: &str,
    updated_at: &str,
    board_algorithm: Option<String>,
    market_state: &str,
    source_status: &str,
    overlay: LiveOverlayBar,
    title: Option<String>,
    provider_symbol: Option<String>,
    provider_kind: Option<String>,
    value_mode: Option<String>,
    weight_snapshot: Option<String>,
    weight_snapshot_trade_date: Option<String>,
    message: Option<String>,
) -> LiveQuoteOverlayPayload {
    LiveQuoteOverlayPayload {
        watch_id: watch_id.to_string(),
        target_type: target_type.to_string(),
        target_id: target_id.to_string(),
        granularity: granularity.to_string(),
        board_algorithm,
        updated_at: updated_at.to_string(),
        market_state: market_state.to_string(),
        source_status: source_status.to_string(),
        overlay,
        meta: LiveQuoteOverlayMeta {
            title,
            source_status: Some(source_status.to_string()),
            provider_symbol,
            provider_kind,
            value_mode,
            weight_snapshot,
            weight_snapshot_trade_date,
            message,
        },
    }
}
