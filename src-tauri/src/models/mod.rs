use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct IndexItem {
    pub id: String,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BoardSummary {
    pub board_id: String,
    pub name: String,
    pub composition_algorithm: String,
    pub build_status: String,
    pub build_phase: String,
    pub build_total: usize,
    pub build_completed: usize,
    pub build_failed: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_job_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_message: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MemberSummary {
    pub symbol: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight_percent: Option<f64>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NoteRecord {
    pub target_type: String,
    pub target_id: String,
    pub content: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatusPayload {
    pub status: String,
    pub message: String,
    pub last_sync_at: Option<String>,
    pub latest_trade_date: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapPayload {
    pub indexes: Vec<IndexItem>,
    pub boards: Vec<BoardSummary>,
    pub members_by_board: HashMap<String, Vec<MemberSummary>>,
    pub active_target_note: NoteRecord,
    pub sync_status: SyncStatusPayload,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BoardMemberSummariesPayload {
    pub board_id: String,
    pub composition_algorithm: String,
    pub members: Vec<MemberSummary>,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BarPoint {
    pub time: String,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume: Option<f64>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ChartMeta {
    pub target_type: String,
    pub target_id: String,
    pub title: String,
    pub source_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_symbol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight_snapshot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight_snapshot_trade_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub granularity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ActiveOverlayPayload {
    pub kind: String,
    pub bar: BarPoint,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ChartPayload {
    pub meta: ChartMeta,
    pub bars: Vec<BarPoint>,
    pub latest_trade_date: Option<String>,
    pub source_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_overlay: Option<ActiveOverlayPayload>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SaveBoardResponse {
    pub board_id: String,
    pub rebuild_started: bool,
    pub background_sync_started: bool,
    pub build_status: String,
    pub build_phase: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_job_id: Option<String>,
    pub composition_algorithm: String,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BoardBuildStatusPayload {
    pub board_id: String,
    pub name: String,
    pub build_status: String,
    pub build_phase: String,
    pub build_total: usize,
    pub build_completed: usize,
    pub build_failed: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_job_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_message: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LiveQuoteOverlayPayload {
    pub watch_id: String,
    pub target_type: String,
    pub target_id: String,
    pub granularity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub board_algorithm: Option<String>,
    pub updated_at: String,
    pub market_state: String,
    pub source_status: String,
    pub overlay: LiveOverlayBar,
    pub meta: LiveQuoteOverlayMeta,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LiveOverlayBar {
    pub trade_date: String,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume: Option<f64>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LiveQuoteOverlayMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_symbol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight_snapshot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weight_snapshot_trade_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SimpleStatusPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub saved: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub opened: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stopped: Option<bool>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ChartWatchStatusPayload {
    pub watch_id: String,
    pub started: bool,
    pub target_type: String,
    pub target_id: String,
    pub granularity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub board_algorithm: Option<String>,
    pub interval_sec: u64,
    pub market_state: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StopChartWatchStatusPayload {
    pub stopped: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub watch_id: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SaveCredentialsPayload {
    pub app_key: String,
    pub app_secret: String,
    pub access_token: String,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetChartPayload {
    pub target_type: String,
    pub target_id: String,
    #[serde(default)]
    pub granularity: Option<String>,
    #[serde(default)]
    pub range: Option<String>,
    #[serde(default)]
    pub board_algorithm: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetBoardMemberSummariesPayload {
    pub board_id: String,
    pub composition_algorithm: String,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct StartChartWatchPayload {
    pub target_type: String,
    pub target_id: String,
    #[serde(default)]
    pub granularity: Option<String>,
    #[serde(default)]
    pub board_algorithm: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SaveBoardPayload {
    pub board_id: Option<String>,
    pub name: String,
    pub members: Vec<String>,
    pub composition_algorithm: String,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TargetNotePayload {
    pub target_type: String,
    pub target_id: String,
    pub content: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SymbolRecord {
    pub target_id: String,
    pub target_type: String,
    pub display_code: String,
    pub name: String,
    pub market: Option<String>,
    pub security_type: String,
    pub currency: Option<String>,
    pub total_shares: Option<f64>,
    pub circulating_shares: Option<f64>,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct DailyBarRecord {
    pub target_id: String,
    pub trade_date: String,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: Option<f64>,
    pub source: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct BoardRecord {
    pub board_id: String,
    pub name: String,
    pub sort_order: i64,
    pub composition_algorithm: String,
    pub build_status: String,
    pub build_phase: String,
    pub build_total: usize,
    pub build_completed: usize,
    pub build_failed: usize,
    pub build_job_id: Option<String>,
    pub build_message: Option<String>,
    pub build_started_at: Option<String>,
    pub build_finished_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct BoardDailyBarRecord {
    pub board_id: String,
    pub composition_algorithm: String,
    pub trade_date: String,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: Option<f64>,
    pub updated_at: String,
}

impl SimpleStatusPayload {
    pub fn saved() -> Self {
        Self {
            saved: Some(true),
            opened: None,
            closed: None,
            stopped: None,
        }
    }

    pub fn opened() -> Self {
        Self {
            saved: None,
            opened: Some(true),
            closed: None,
            stopped: None,
        }
    }

    pub fn closed() -> Self {
        Self {
            saved: None,
            opened: None,
            closed: Some(true),
            stopped: None,
        }
    }

    pub fn stopped() -> Self {
        Self {
            saved: None,
            opened: None,
            closed: None,
            stopped: Some(true),
        }
    }
}

impl BoardRecord {
    pub fn to_summary(&self) -> BoardSummary {
        BoardSummary {
            board_id: self.board_id.clone(),
            name: self.name.clone(),
            composition_algorithm: self.composition_algorithm.clone(),
            build_status: self.build_status.clone(),
            build_phase: self.build_phase.clone(),
            build_total: self.build_total,
            build_completed: self.build_completed,
            build_failed: self.build_failed,
            build_job_id: self.build_job_id.clone(),
            build_message: self.build_message.clone(),
            updated_at: self.updated_at.clone(),
        }
    }

    pub fn to_build_status(&self) -> BoardBuildStatusPayload {
        BoardBuildStatusPayload {
            board_id: self.board_id.clone(),
            name: self.name.clone(),
            build_status: self.build_status.clone(),
            build_phase: self.build_phase.clone(),
            build_total: self.build_total,
            build_completed: self.build_completed,
            build_failed: self.build_failed,
            build_job_id: self.build_job_id.clone(),
            build_message: self.build_message.clone(),
            updated_at: self.updated_at.clone(),
        }
    }
}
