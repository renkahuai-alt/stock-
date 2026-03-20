use std::sync::Arc;

use tauri::{Emitter, Manager};

pub mod app_shell;
mod board_weights;
pub mod chart_engine;
pub mod commands;
pub mod errors;
pub mod events;
pub mod jobs;
pub mod live_quote;
pub mod models;
pub mod repositories;
pub mod secret_store;
pub mod services;
pub mod telemetry;

pub fn run() {
    tauri::Builder::default()
        .menu(app_shell::windowing::build_app_menu)
        .on_menu_event(|app, event| {
            if event.id().as_ref() == app_shell::windowing::SETTINGS_MENU_ID {
                if let Err(error) = app_shell::windowing::open_settings_window(app.clone()) {
                    eprintln!("[windowing] settings-menu-open-failed error={error}");
                }
            }
        })
        .manage(services::AppState::default())
        .setup(|app| {
            let database = repositories::Database::new();
            database.bootstrap()?;
            jobs::board_build::recover_stale(&database)?;
            let runtime = app.state::<services::AppState>().runtime.clone();
            runtime.spawn_provider_prewarm();
            let app_handle = app.handle().clone();
            app_shell::windowing::ensure_main_window(app_handle.clone())?;
            let notifier: jobs::startup_sync::StartupSyncNotifier = Arc::new(move |payload| {
                if let Err(error) = app_handle.emit(events::SYNC_STATUS, payload) {
                    eprintln!("[startup_sync] status emit failed error={error}");
                }
            });
            jobs::startup_sync::spawn(database.clone(), runtime, Some(notifier));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::bootstrap,
            commands::save_credentials,
            commands::get_sync_status,
            commands::run_sync,
            commands::get_chart,
            commands::save_board,
            commands::delete_board,
            commands::get_board_build_status,
            commands::get_board_member_summaries,
            commands::get_target_note,
            commands::save_target_note,
            commands::open_settings_window,
            commands::close_settings_window,
            commands::start_chart_watch,
            commands::stop_chart_watch,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run new_stock tauri app");
}
