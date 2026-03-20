use std::sync::Arc;

use crate::errors::{AppError, AppResult};
use crate::models::SyncStatusPayload;
use crate::repositories::Database;
use crate::services::{AppRuntime, SyncService};

pub type StartupSyncNotifier = Arc<dyn Fn(SyncStatusPayload) + Send + Sync>;

pub async fn run(
    database: Database,
    runtime: AppRuntime,
    notifier: Option<StartupSyncNotifier>,
) -> AppResult<SyncStatusPayload> {
    let payload = SyncService::new(database, runtime).run("startup").await?;
    if let Some(notifier) = notifier {
        notifier(payload.clone());
    }
    Ok(payload)
}

pub fn spawn(database: Database, runtime: AppRuntime, notifier: Option<StartupSyncNotifier>) {
    tauri::async_runtime::spawn(async move {
        let _ = run(database, runtime, notifier).await.map_err(|error| {
            eprintln!("[startup_sync] run failed error={}", error_message(&error));
        });
    });
}

fn error_message(error: &AppError) -> String {
    match error {
        AppError::Message(message) => message.clone(),
    }
}
