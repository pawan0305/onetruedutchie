mod anthropic;
mod audio;
mod commands;
mod deepgram;
mod settings;
mod state;
mod storage;

use std::sync::Arc;

use tauri::Manager;
use tracing_subscriber::EnvFilter;

use crate::state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,onetruedutchie_lib=debug")),
        )
        .try_init();

    tauri::Builder::default()
        .setup(|app| {
            let app_handle = app.handle().clone();
            let data_dir = app
                .path()
                .app_data_dir()
                .expect("could not resolve app data dir");
            std::fs::create_dir_all(&data_dir).ok();

            let state = AppState::new(app_handle, data_dir);
            app.manage(Arc::new(state));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::set_api_keys,
            commands::start_meeting,
            commands::stop_meeting,
            commands::current_meeting,
            commands::list_meetings,
            commands::load_meeting,
            commands::delete_meeting,
            commands::ask_question,
            commands::regenerate_summary,
            commands::set_meeting_title,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
