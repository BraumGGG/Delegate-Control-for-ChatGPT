mod commands;
mod config;
mod credentials;
mod process_manager;
mod proxy_config;

use config::{app_data_root, load_application_settings, AppSettings};
use process_manager::{DelegateStatus, ProcessManager};
use std::{path::PathBuf, sync::{Arc, Mutex}};
use tauri::Manager;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

#[derive(Clone)]
pub struct AppState {
    manager: Arc<Mutex<ProcessManager>>,
    settings: Arc<Mutex<AppSettings>>,
    settings_path: PathBuf,
}

impl AppState {
    fn new() -> Self {
        let root = app_data_root();
        let settings_path = root.join("settings.json");
        let settings = load_application_settings(&settings_path);
        Self {
            manager: Arc::new(Mutex::new(ProcessManager::new(root.join("logs")))),
            settings: Arc::new(Mutex::new(settings)),
            settings_path,
        }
    }

    fn start(&self) -> Result<DelegateStatus, String> {
        let settings = self.settings.lock().map_err(|_| "设置状态已损坏。".to_string())?.clone();
        self.manager.lock().map_err(|_| "进程状态已损坏。".to_string())?.start(&settings)
    }

    fn stop(&self) -> Result<DelegateStatus, String> {
        let settings = self.settings.lock().map_err(|_| "设置状态已损坏。".to_string())?.clone();
        Ok(self.manager.lock().map_err(|_| "进程状态已损坏。".to_string())?.stop(&settings))
    }

    fn start_project(&self, project_id: &str) -> Result<DelegateStatus, String> {
        let settings = self.settings.lock().map_err(|_| "设置状态已损坏。".to_string())?.clone();
        self.manager.lock().map_err(|_| "进程状态已损坏。".to_string())?.start_project(&settings, project_id)
    }

    fn stop_project(&self, project_id: &str) -> Result<DelegateStatus, String> {
        let settings = self.settings.lock().map_err(|_| "设置状态已损坏。".to_string())?.clone();
        self.manager.lock().map_err(|_| "进程状态已损坏。".to_string())?.stop_project(&settings, project_id)
    }
}

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

pub fn run() {
    let app_state = AppState::new();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_main_window(app);
        }))
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::get_status,
            commands::get_runtime_readiness,
            commands::check_runtime_readiness,
            commands::start_delegate,
            commands::stop_delegate,
            commands::start_all_projects,
            commands::stop_all_projects,
            commands::start_project,
            commands::stop_project,
            commands::get_settings,
            commands::save_settings,
            commands::migrate_project_key,
            commands::save_runtime_key,
            commands::read_logs,
            commands::open_log_directory,
        ])
        .setup(|app| {
            let open = MenuItem::with_id(app, "open", "打开控制面板", true, None::<&str>)?;
            let start = MenuItem::with_id(app, "start", "启动连接", true, None::<&str>)?;
            let stop = MenuItem::with_id(app, "stop", "停止连接", true, None::<&str>)?;
            let exit = MenuItem::with_id(app, "exit", "退出程序", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &start, &stop, &exit])?;

            TrayIconBuilder::new()
                .icon(app.default_window_icon().expect("default app icon").clone())
                .tooltip("Delegate Control for ChatGPT")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => show_main_window(app),
                    "start" => {
                        let state = app.state::<AppState>().inner().clone();
                        tauri::async_runtime::spawn_blocking(move || { let _ = state.start(); });
                    }
                    "stop" => {
                        let state = app.state::<AppState>().inner().clone();
                        tauri::async_runtime::spawn_blocking(move || { let _ = state.stop(); });
                    }
                    "exit" => {
                        let state = app.state::<AppState>().inner().clone();
                        let _ = state.stop();
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                        show_main_window(tray.app_handle());
                    }
                })
                .build(app)?;
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running Delegate Control for ChatGPT");
}
