use uuid::Uuid;

use chrono::Utc;
use std::time::Instant;

use crate::errors::{AppError, AppResult};
use crate::models::{
    BoardBuildStatusPayload, BoardMemberSummariesPayload, BoardRecord, SaveBoardPayload,
    SaveBoardResponse,
};
use crate::repositories::Database;
use crate::telemetry;

use super::chart::compose_board_bars;
use super::AppRuntime;

#[derive(Debug, Clone)]
pub struct BoardService {
    database: Database,
}

impl BoardService {
    pub fn new(database: Database) -> Self {
        Self { database }
    }

    pub fn save(&self, payload: SaveBoardPayload) -> AppResult<SaveBoardResponse> {
        let started_at = Instant::now();
        self.database.bootstrap()?;

        let name = payload.name.trim();
        if name.is_empty() {
            return Err(AppError::Message("board name is required".into()));
        }

        let members = normalize_members(payload.members);
        if members.is_empty() {
            return Err(AppError::Message("board members are required".into()));
        }

        validate_board_algorithm(&payload.composition_algorithm)?;

        let symbols = self.database.list_symbols_by_ids(&members)?;

        let existing = match payload.board_id.as_deref() {
            Some(board_id) => self.database.get_board(board_id)?,
            None => None,
        };
        let board_id = resolve_board_id(
            &self.database,
            existing.as_ref(),
            payload.board_id.as_deref(),
            name,
        )?;
        let sort_order = existing
            .as_ref()
            .map(|record| record.sort_order)
            .unwrap_or(self.database.next_board_sort_order()?);

        let mut estimated_bars = 0usize;
        let mut all_have_history = true;
        for member in &members {
            let count = self.database.count_bars_for_target(member)?;
            estimated_bars += count;
            if count == 0 {
                all_have_history = false;
            }
        }

        let should_background = !all_have_history || members.len() > 20 || estimated_bars > 10_000;
        telemetry::emit(
            "save_board_path_selected",
            &[
                (
                    "path",
                    if should_background {
                        "background".to_string()
                    } else {
                        "fast".to_string()
                    },
                ),
                ("localSymbolCount", symbols.len().to_string()),
                ("memberCount", members.len().to_string()),
                ("estimatedBars", estimated_bars.to_string()),
            ],
        );
        let now = Utc::now().to_rfc3339();
        let board = if should_background {
            BoardRecord {
                board_id: board_id.clone(),
                name: name.to_string(),
                sort_order,
                composition_algorithm: payload.composition_algorithm.clone(),
                build_status: "queued".into(),
                build_phase: "queued".into(),
                build_total: members.len(),
                build_completed: 0,
                build_failed: 0,
                build_job_id: Some(format!("step2-placeholder-{}", Uuid::new_v4().simple())),
                build_message: Some("等待 Step 3 后台构建".into()),
                build_started_at: Some(now.clone()),
                build_finished_at: None,
                created_at: existing
                    .as_ref()
                    .map(|record| record.created_at.clone())
                    .unwrap_or_else(|| now.clone()),
                updated_at: now.clone(),
            }
        } else {
            BoardRecord {
                board_id: board_id.clone(),
                name: name.to_string(),
                sort_order,
                composition_algorithm: payload.composition_algorithm.clone(),
                build_status: "succeeded".into(),
                build_phase: "completed".into(),
                build_total: members.len(),
                build_completed: members.len(),
                build_failed: 0,
                build_job_id: None,
                build_message: None,
                build_started_at: Some(now.clone()),
                build_finished_at: Some(now.clone()),
                created_at: existing
                    .as_ref()
                    .map(|record| record.created_at.clone())
                    .unwrap_or_else(|| now.clone()),
                updated_at: now.clone(),
            }
        };

        self.database.save_board_definition(&board, &members)?;

        if !should_background {
            let bars =
                compose_board_bars(&self.database, &members, &payload.composition_algorithm)?;
            self.database
                .save_board_chart(&board_id, &payload.composition_algorithm, &bars)?;
        }

        telemetry::emit(
            "save_board_completed",
            &[
                ("boardId", board_id.clone()),
                (
                    "path",
                    if should_background {
                        "background".to_string()
                    } else {
                        "fast".to_string()
                    },
                ),
                ("elapsedMs", started_at.elapsed().as_millis().to_string()),
            ],
        );

        Ok(SaveBoardResponse {
            board_id,
            rebuild_started: true,
            background_sync_started: should_background,
            build_status: board.build_status,
            build_phase: board.build_phase,
            build_job_id: board.build_job_id,
            composition_algorithm: board.composition_algorithm,
        })
    }

    pub fn get_build_status(&self, board_id: &str) -> AppResult<BoardBuildStatusPayload> {
        self.database.bootstrap()?;
        let board = self
            .database
            .get_board(board_id)?
            .ok_or_else(|| AppError::Message(format!("board not found: {board_id}")))?;
        Ok(board.to_build_status())
    }

    pub fn get_member_summaries(
        &self,
        board_id: &str,
        composition_algorithm: &str,
    ) -> AppResult<BoardMemberSummariesPayload> {
        self.database.bootstrap()?;
        validate_board_algorithm(composition_algorithm)?;
        let board = self
            .database
            .get_board(board_id)?
            .ok_or_else(|| AppError::Message(format!("board not found: {board_id}")))?;
        let members = self
            .database
            .list_member_summaries(board_id, composition_algorithm)?;

        Ok(BoardMemberSummariesPayload {
            board_id: board.board_id,
            composition_algorithm: composition_algorithm.to_string(),
            members,
            updated_at: Utc::now().to_rfc3339(),
        })
    }

    pub async fn delete(&self, runtime: &AppRuntime, board_id: &str) -> AppResult<()> {
        self.database.bootstrap()?;
        self.database.delete_board(board_id)?;

        if let Some(active_watch) = runtime.active_watch().await {
            if active_watch.target_type == "board" && active_watch.target_id == board_id {
                active_watch.abort_handle.abort();
                runtime.clear_live_overlay(&active_watch.overlay_key);
                let _ = runtime.clear_active_watch_if(&active_watch.watch_id).await;
            }
        }

        runtime.invalidate_targets(&[], &[board_id.to_string()]);
        Ok(())
    }
}

fn validate_board_algorithm(value: &str) -> AppResult<()> {
    match value {
        "equal_weight_v1" | "market_cap_weight_v1" => Ok(()),
        other => Err(AppError::Message(format!(
            "unsupported compositionAlgorithm: {other}"
        ))),
    }
}

fn normalize_members(members: Vec<String>) -> Vec<String> {
    let mut normalized = Vec::new();
    for member in members {
        let value = member.trim().to_uppercase();
        if !value.is_empty() && !normalized.contains(&value) {
            normalized.push(value);
        }
    }
    normalized
}

fn resolve_board_id(
    database: &Database,
    existing: Option<&BoardRecord>,
    requested_id: Option<&str>,
    name: &str,
) -> AppResult<String> {
    if let Some(record) = existing {
        return Ok(record.board_id.clone());
    }

    if let Some(board_id) = requested_id {
        return Ok(board_id.to_string());
    }

    let mut slug = slugify(name);
    if slug.is_empty() {
        slug = format!("board-{}", Uuid::new_v4().simple());
    }
    if !database.board_exists(&slug)? {
        return Ok(slug);
    }

    Ok(format!("{slug}-{}", Uuid::new_v4().simple()))
}

fn slugify(name: &str) -> String {
    let mut slug = String::from("board-");
    let mut last_dash = false;

    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if ('\u{4e00}'..='\u{9fa5}').contains(&ch) {
            slug.push(ch);
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
    }

    slug.trim_end_matches('-').to_string()
}
