use crate::{config::{save_settings as persist_settings, AppSettings}, credentials, AppState};
use tauri::State;

#[tauri::command]
pub async fn get_status(state: State<'_, AppState>) -> Result<crate::process_manager::DelegateStatus, String> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let settings = app_state.settings.lock().map_err(|_| "设置状态已损坏。".to_string())?.clone();
        let mut manager = app_state.manager.lock().map_err(|_| "进程状态已损坏。".to_string())?;
        Ok(manager.status(&settings))
    }).await.map_err(|error| format!("状态读取任务异常：{error}"))?
}

#[tauri::command]
pub async fn start_delegate(state: State<'_, AppState>) -> Result<crate::process_manager::DelegateStatus, String> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || app_state.start()).await.map_err(|error| format!("启动任务异常：{error}"))?
}

#[tauri::command]
pub async fn stop_delegate(state: State<'_, AppState>) -> Result<crate::process_manager::DelegateStatus, String> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || app_state.stop()).await.map_err(|error| format!("停止任务异常：{error}"))?
}

#[tauri::command]
pub async fn start_all_projects(state: State<'_, AppState>) -> Result<crate::process_manager::DelegateStatus, String> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || app_state.start()).await.map_err(|error| format!("启动任务异常：{error}"))?
}

#[tauri::command]
pub async fn stop_all_projects(state: State<'_, AppState>) -> Result<crate::process_manager::DelegateStatus, String> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || app_state.stop()).await.map_err(|error| format!("停止任务异常：{error}"))?
}

#[tauri::command]
pub async fn start_project(project_id: String, state: State<'_, AppState>) -> Result<crate::process_manager::DelegateStatus, String> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || app_state.start_project(&project_id)).await.map_err(|error| format!("启动项目任务异常：{error}"))?
}

#[tauri::command]
pub async fn stop_project(project_id: String, state: State<'_, AppState>) -> Result<crate::process_manager::DelegateStatus, String> {
    let app_state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || app_state.stop_project(&project_id)).await.map_err(|error| format!("停止项目任务异常：{error}"))?
}

#[tauri::command]
pub fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    state.settings.lock().map(|settings| settings.clone()).map_err(|_| "设置状态已损坏。".to_string())
}

#[tauri::command]
pub fn save_settings(settings: AppSettings, state: State<'_, AppState>) -> Result<AppSettings, String> {
    persist_settings(&state.settings_path, &settings)?;
    *state.settings.lock().map_err(|_| "设置状态已损坏。".to_string())? = settings.clone();
    Ok(settings)
}

#[tauri::command]
pub fn save_runtime_key(key: String) -> Result<(), String> {
    credentials::write_runtime_key(&key)
}

#[tauri::command]
pub fn read_logs(source: String, project_id: Option<String>, state: State<'_, AppState>) -> Result<String, String> {
    state.manager.lock().map_err(|_| "进程状态已损坏。".to_string())?.read_logs(&source, project_id.as_deref())
}

#[tauri::command]
pub fn open_log_directory(state: State<'_, AppState>) -> Result<(), String> {
    let path = state.manager.lock().map_err(|_| "进程状态已损坏。".to_string())?.log_dir().to_path_buf();
    std::fs::create_dir_all(&path).map_err(|error| format!("无法创建日志目录：{error}"))?;
    std::process::Command::new("explorer.exe").arg(path).spawn().map_err(|error| format!("无法打开日志目录：{error}"))?;
    Ok(())
}
