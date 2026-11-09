//! The bleNIMA app: the window, and the commands the UI calls in Rust.
//!
//! The real work lives in `blenima-core`. This file only connects it to the UI.

// No console window next to the app in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bluetooth;
mod window;

use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

use blenima_core::settings::Settings;
use blenima_core::workspace::Workspace;
use serde::Serialize;
use tauri::{Manager, State, WebviewWindow, WindowEvent};

/// What the status bar shows about the app and the computer it runs on.
#[derive(Serialize)]
struct AppInfo {
    version: &'static str,
    os: &'static str,
    arch: &'static str,
}

/// A value kept in memory and in a file in the config folder.
struct Saved<T> {
    path: PathBuf,
    current: Mutex<T>,
}

impl<T: Clone> Saved<T> {
    fn get(&self) -> T {
        self.current
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    fn set(&self, value: T) {
        *self.current.lock().unwrap_or_else(PoisonError::into_inner) = value;
    }

    fn save_problem(&self, error: std::io::Error) -> String {
        format!("could not save {}: {error}", self.path.display())
    }
}

#[tauri::command]
fn app_info() -> AppInfo {
    AppInfo {
        version: blenima_core::VERSION,
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
    }
}

#[tauri::command]
fn get_settings(file: State<Saved<Settings>>) -> Settings {
    file.get()
}

/// Saves new settings at once, and returns them as kept, with every value in range.
#[tauri::command]
fn set_settings(
    settings: Settings,
    file: State<Saved<Settings>>,
    main: WebviewWindow,
) -> Result<Settings, String> {
    let settings = settings.checked();
    settings
        .save(&file.path)
        .map_err(|error| file.save_problem(error))?;
    if settings.menu_font_size != file.get().menu_font_size {
        window::set_menu_font_size(&main, settings.menu_font_size);
    }
    file.set(settings.clone());
    Ok(settings)
}

#[tauri::command]
fn get_workspace(file: State<Saved<Workspace>>) -> Workspace {
    file.get()
}

/// Saves the open tabs. Not async, so saves happen in the order they were asked for.
#[tauri::command]
fn set_workspace(workspace: Workspace, file: State<Saved<Workspace>>) -> Result<(), String> {
    let mut current = file.current.lock().unwrap_or_else(PoisonError::into_inner);
    // The window's place and size come from the window, not from the UI.
    let workspace = Workspace {
        window: current.window,
        ..workspace
    }
    .checked();
    workspace
        .save(&file.path)
        .map_err(|error| file.save_problem(error))?;
    *current = workspace;
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let dir = app.path().app_config_dir()?;
            let settings = dir.join("settings.toml");
            let workspace = dir.join("workspace.toml");
            let settings_now = Settings::load(&settings);
            let restore_window = settings_now.startup.restore_window;
            let menu_font_size = settings_now.menu_font_size;
            app.manage(Saved {
                current: Mutex::new(settings_now),
                path: settings,
            });
            let workspace_now = Workspace::load(&workspace);
            // Without "open the window where it was", it opens in the middle.
            let window_before = workspace_now.window.filter(|_| restore_window);
            app.manage(Saved {
                current: Mutex::new(workspace_now),
                path: workspace,
            });
            app.manage(window::Unsaved::default());
            app.manage(bluetooth::Bluetooth::default());
            if let Some(main) = app.get_webview_window("main") {
                // Before the window shows, so it never changes size in sight.
                window::set_menu_font_size(&main, menu_font_size);
                window::restore(&main, window_before);
            }
            window::keep_saving(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::Moved(_) | WindowEvent::Resized(_) => window::changed(window),
            WindowEvent::CloseRequested { .. } => window::keep(window),
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            get_settings,
            set_settings,
            get_workspace,
            set_workspace,
            bluetooth::radio_info,
            bluetooth::scan_open,
            bluetooth::scan_run,
            bluetooth::scan_close,
            bluetooth::scan_clear,
            bluetooth::scan_details,
            bluetooth::scan_csv,
            bluetooth::device_open,
            bluetooth::device_command,
            bluetooth::device_close,
            bluetooth::save_text,
            bluetooth::read_file,
            bluetooth::device_pair,
            bluetooth::device_unpair,
            bluetooth::device_paired,
            bluetooth::bonds,
            bluetooth::advertise_start,
            bluetooth::advertise_status,
            bluetooth::advertise_stop,
            bluetooth::server_start,
            bluetooth::server_value,
            bluetooth::server_stop,
            bluetooth::standard_names,
        ])
        .run(tauri::generate_context!())
        .expect("bleNIMA could not start");
}
