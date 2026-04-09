use std::sync::Arc;

use tauri::{AppHandle, Emitter, State};

use crate::errors::AppError;
use crate::events;
use crate::jobs;
use crate::models::{
    BoardBuildStatusPayload, BoardMemberSummariesPayload, BootstrapPayload, ChartPayload,
    ChartWatchStatusPayload, GetBoardMemberSummariesPayload, GetChartPayload, NoteRecord,
    SaveBoardPayload, SaveBoardResponse, SaveCredentialsPayload, SimpleStatusPayload,
    StartChartWatchPayload, StopChartWatchStatusPayload, SyncStatusPayload, TargetNotePayload,
};
use crate::repositories::Database;
use crate::secret_store;
use crate::services::{
    AppState, BoardService, BootstrapService, ChartService, NoteService, SyncService,
};

#[tauri::command]
pub fn bootstrap(state: State<'_, AppState>) -> Result<BootstrapPayload, AppError> {
    state.runtime.spawn_provider_prewarm();
    BootstrapService::new(Database::new()).load()
}

#[tauri::command]
pub fn save_credentials(
    app: AppHandle,
    state: State<'_, AppState>,
    payload: SaveCredentialsPayload,
) -> Result<SimpleStatusPayload, AppError> {
    secret_store::save_credentials(&payload)?;
    let runtime = state.runtime.clone();
    runtime.reset_provider();
    runtime.spawn_provider_prewarm();
    app.emit(events::SETTINGS_SAVED, ())
        .map_err(AppError::from)?;
    let sync_app = app.clone();
    tauri::async_runtime::spawn(async move {
        match SyncService::new(Database::new(), runtime).run("manual").await {
            Ok(payload) => {
                let _ = sync_app.emit(events::SYNC_STATUS, payload);
            }
            Err(error) => {
                eprintln!("[save_credentials] background sync failed error={error}");
            }
        }
    });
    Ok(SimpleStatusPayload::saved())
}

#[tauri::command]
pub fn get_sync_status() -> Result<SyncStatusPayload, AppError> {
    BootstrapService::new(Database::new()).sync_status()
}

#[tauri::command]
pub async fn run_sync(
    app: AppHandle,
    state: State<'_, AppState>,
    mode: String,
) -> Result<SyncStatusPayload, AppError> {
    let payload = SyncService::new(Database::new(), state.runtime.clone())
        .run(&mode)
        .await?;
    app.emit(events::SYNC_STATUS, payload.clone())
        .map_err(AppError::from)?;
    Ok(payload)
}

#[tauri::command]
pub fn get_chart(
    state: State<'_, AppState>,
    payload: GetChartPayload,
) -> Result<ChartPayload, AppError> {
    ChartService::with_runtime(Database::new(), state.runtime.clone()).get_chart(payload)
}

#[tauri::command]
pub async fn save_board(
    app: AppHandle,
    state: State<'_, AppState>,
    payload: SaveBoardPayload,
) -> Result<SaveBoardResponse, AppError> {
    let service = BoardService::new(Database::new());
    let response = service.save(payload)?;
    let status = service.get_build_status(&response.board_id)?;
    app.emit(events::SETTINGS_SAVED, ())
        .map_err(AppError::from)?;
    app.emit(events::BOARD_BUILD_STATUS, status)
        .map_err(AppError::from)?;
    if response.background_sync_started {
        let board_id = response.board_id.clone();
        let runtime = state.runtime.clone();
        let database = Database::new();
        let app_handle = app.clone();
        tauri::async_runtime::spawn(async move {
            let notifier: jobs::board_build::BoardBuildNotifier = Arc::new(move |payload| {
                let _ = app_handle.emit(events::BOARD_BUILD_STATUS, payload);
            });
            let _ = jobs::board_build::run(database, runtime, &board_id, Some(notifier)).await;
        });
    }
    Ok(response)
}

#[tauri::command]
pub async fn delete_board(
    app: AppHandle,
    state: State<'_, AppState>,
    board_id: String,
) -> Result<SimpleStatusPayload, AppError> {
    BoardService::new(Database::new())
        .delete(&state.runtime, &board_id)
        .await?;
    app.emit(events::SETTINGS_SAVED, ())
        .map_err(AppError::from)?;
    Ok(SimpleStatusPayload::saved())
}

#[tauri::command]
pub fn get_board_build_status(board_id: String) -> Result<BoardBuildStatusPayload, AppError> {
    BoardService::new(Database::new()).get_build_status(&board_id)
}

#[tauri::command]
pub fn get_board_member_summaries(
    payload: GetBoardMemberSummariesPayload,
) -> Result<BoardMemberSummariesPayload, AppError> {
    BoardService::new(Database::new())
        .get_member_summaries(&payload.board_id, &payload.composition_algorithm)
}

#[tauri::command]
pub fn get_target_note(payload: TargetNotePayload) -> Result<NoteRecord, AppError> {
    NoteService::new(Database::new()).get(&payload.target_type, &payload.target_id)
}

#[tauri::command]
pub fn save_target_note(payload: TargetNotePayload) -> Result<NoteRecord, AppError> {
    NoteService::new(Database::new()).save(payload)
}

#[tauri::command]
pub fn open_settings_window(app: AppHandle) -> Result<SimpleStatusPayload, AppError> {
    crate::app_shell::windowing::open_settings_window(app)?;
    Ok(SimpleStatusPayload::opened())
}

#[tauri::command]
pub fn close_settings_window(app: AppHandle) -> Result<SimpleStatusPayload, AppError> {
    crate::app_shell::windowing::close_settings_window(app)?;
    Ok(SimpleStatusPayload::closed())
}

#[tauri::command]
pub async fn start_chart_watch(
    app: AppHandle,
    state: State<'_, AppState>,
    payload: StartChartWatchPayload,
) -> Result<ChartWatchStatusPayload, AppError> {
    state.runtime.spawn_provider_prewarm();
    jobs::chart_watch::start(
        Database::new(),
        state.runtime.clone(),
        payload,
        Arc::new(move |overlay| {
            let _ = app.emit(events::CHART_LIVE_UPDATE, overlay);
        }),
    )
    .await
}

#[tauri::command]
pub async fn stop_chart_watch(
    state: State<'_, AppState>,
) -> Result<StopChartWatchStatusPayload, AppError> {
    jobs::chart_watch::stop(state.runtime.clone()).await
}
