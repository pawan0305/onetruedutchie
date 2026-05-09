mod anthropic;
mod audio;
mod commands;
mod deepgram;
mod settings;
mod state;
mod storage;

use std::sync::{Arc, Mutex};

use tauri::Manager;
use tracing_subscriber::EnvFilter;

use crate::state::AppState;

/// Log to ~/Library/Logs/com.onetruedutchie.app/onetrue.log so the installed
/// .app has somewhere to write tracing output (Tauri swallows stderr when
/// launched via Finder/`open`).
fn open_log_file() -> Option<std::fs::File> {
    let home = std::env::var("HOME").ok()?;
    let dir = std::path::PathBuf::from(home).join("Library/Logs/com.onetruedutchie.app");
    std::fs::create_dir_all(&dir).ok()?;
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("onetrue.log"))
        .ok()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,onetruedutchie_lib=debug"));
    if let Some(file) = open_log_file() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_writer(Mutex::new(file))
            .with_ansi(false)
            .try_init();
    } else {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .try_init();
    }
    tracing::info!("OneTrueDutchie starting");

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

            // Apply persisted overlay mode + lock. If the user had subtitles
            // on when they last quit, show the overlay window now and apply
            // the click-through state.
            let mode = settings::read_overlay_mode();
            let locked = settings::read_overlay_locked();
            if let Some(win) = app.get_webview_window("overlay") {
                if mode != "off" {
                    let _ = win.show();
                    let _ = win.set_always_on_top(true);
                    #[cfg(target_os = "macos")]
                    {
                        let _ = win.set_visible_on_all_workspaces(true);
                    }
                }
                let _ = win.set_ignore_cursor_events(locked);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_settings,
            commands::set_api_keys,
            commands::set_translate_enabled,
            commands::set_overlay_mode,
            commands::set_overlay_font_size,
            commands::set_overlay_locked,
            commands::start_meeting,
            commands::stop_meeting,
            commands::current_meeting,
            commands::list_meetings,
            commands::load_meeting,
            commands::delete_meeting,
            commands::rename_meeting,
            commands::export_english_transcript,
            commands::ask_question,
            commands::regenerate_summary,
            commands::set_meeting_title,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
