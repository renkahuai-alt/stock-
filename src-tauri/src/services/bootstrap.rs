use std::collections::HashMap;

use chrono::Utc;

use crate::errors::AppResult;
use crate::models::{BootstrapPayload, NoteRecord};
use crate::repositories::Database;

use super::{AppRuntime, SyncService};

#[derive(Debug, Clone)]
pub struct BootstrapService {
    database: Database,
}

impl BootstrapService {
    pub fn new(database: Database) -> Self {
        Self { database }
    }

    pub fn load(&self) -> AppResult<BootstrapPayload> {
        self.database.bootstrap()?;

        let indexes = self.database.list_indexes()?;
        let boards = self.database.list_boards()?;
        let mut members_by_board = HashMap::new();

        for board in &boards {
            members_by_board.insert(
                board.board_id.clone(),
                self.database
                    .list_member_summaries(&board.board_id, &board.composition_algorithm)?,
            );
        }

        let sync_status =
            SyncService::new(self.database.clone(), AppRuntime::default()).current_status()?;
        let default_timestamp = sync_status
            .last_sync_at
            .clone()
            .unwrap_or_else(|| Utc::now().to_rfc3339());

        let active_target_note = self
            .database
            .get_first_note()?
            .or_else(|| default_note(&boards, &indexes, &default_timestamp))
            .unwrap_or(NoteRecord {
                target_type: "index".into(),
                target_id: "DJI".into(),
                content: String::new(),
                updated_at: default_timestamp.clone(),
            });

        Ok(BootstrapPayload {
            indexes,
            boards: boards
                .into_iter()
                .map(|record| record.to_summary())
                .collect(),
            members_by_board,
            active_target_note,
            sync_status,
        })
    }

    pub fn sync_status(&self) -> AppResult<crate::models::SyncStatusPayload> {
        self.database.bootstrap()?;
        SyncService::new(self.database.clone(), AppRuntime::default()).current_status()
    }
}

fn default_note(
    boards: &[crate::models::BoardRecord],
    indexes: &[crate::models::IndexItem],
    default_timestamp: &str,
) -> Option<NoteRecord> {
    if let Some(board) = boards.first() {
        return Some(NoteRecord {
            target_type: "board".into(),
            target_id: board.board_id.clone(),
            content: String::new(),
            updated_at: board.updated_at.clone(),
        });
    }

    indexes.first().map(|index| NoteRecord {
        target_type: "index".into(),
        target_id: index.id.clone(),
        content: String::new(),
        updated_at: default_timestamp.to_string(),
    })
}
