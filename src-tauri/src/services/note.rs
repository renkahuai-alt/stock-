use crate::errors::AppResult;
use crate::models::{NoteRecord, TargetNotePayload};
use crate::repositories::Database;
use chrono::Utc;

#[derive(Debug, Clone)]
pub struct NoteService {
    database: Database,
}

impl NoteService {
    pub fn new(database: Database) -> Self {
        Self { database }
    }

    pub fn get(&self, target_type: &str, target_id: &str) -> AppResult<NoteRecord> {
        self.database.bootstrap()?;
        let default_timestamp = self
            .database
            .get_sync_status_summary()?
            .last_sync_at
            .unwrap_or_else(|| Utc::now().to_rfc3339());
        Ok(self
            .database
            .get_note(target_type, target_id)?
            .unwrap_or(NoteRecord {
                target_type: target_type.to_string(),
                target_id: target_id.to_string(),
                content: String::new(),
                updated_at: default_timestamp,
            }))
    }

    pub fn save(&self, payload: TargetNotePayload) -> AppResult<NoteRecord> {
        self.database.bootstrap()?;
        self.database.save_note(
            &payload.target_type,
            &payload.target_id,
            payload.content.as_deref().unwrap_or_default(),
        )
    }
}
