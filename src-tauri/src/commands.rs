use crate::{config::{migrate_project_key_in_settings, save_settings as persist_settings, AppSettings}, credentials, AppState};
use std::path::Path;
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
pub fn migrate_project_key(old_project_id: String, new_project_id: String, state: State<'_, AppState>) -> Result<AppSettings, String> {
    let old_project_id = old_project_id.trim();
    let new_project_id = new_project_id.trim();
    let current = state.settings.lock().map_err(|_| "设置状态已损坏。".to_string())?.clone();
    let migrated = migrate_project_key_in_settings(&current, old_project_id, new_project_id)?;
    let log_root = {
        let mut manager = state.manager.lock().map_err(|_| "进程状态已损坏。".to_string())?;
        if !manager.is_stopped() {
            return Err("迁移 Project Key 前必须先停止全部项目连接。".to_string());
        }
        manager.log_dir().to_path_buf()
    };

    let logs_moved = migrate_project_log_directory(&log_root, old_project_id, new_project_id)?;
    if let Err(error) = persist_settings(&state.settings_path, &migrated) {
        if logs_moved {
            if let Err(rollback_error) = rollback_project_log_directory(&log_root, old_project_id, new_project_id) {
                return Err(format!("{error}；日志目录回滚失败：{rollback_error}"));
            }
        }
        return Err(error);
    }
    *state.settings.lock().map_err(|_| "设置状态已损坏。".to_string())? = migrated.clone();
    Ok(migrated)
}

fn migrate_project_log_directory(log_root: &Path, old_id: &str, new_id: &str) -> Result<bool, String> {
    let old_path = log_root.join(old_id);
    let new_path = log_root.join(new_id);
    if new_path.exists() {
        return Err(format!("新 Project Key 的日志目录已存在：{}。请先处理目录冲突。", new_path.display()));
    }
    if !old_path.exists() {
        return Ok(false);
    }
    std::fs::rename(&old_path, &new_path).map_err(|error| format!("无法迁移项目日志目录：{error}"))?;
    Ok(true)
}

fn rollback_project_log_directory(log_root: &Path, old_id: &str, new_id: &str) -> Result<(), String> {
    let old_path = log_root.join(old_id);
    let new_path = log_root.join(new_id);
    if !new_path.exists() {
        return Ok(());
    }
    if old_path.exists() {
        return Err(format!("旧日志目录已重新出现，无法安全回滚：{}。", old_path.display()));
    }
    std::fs::rename(&new_path, &old_path).map_err(|error| format!("无法恢复旧项目日志目录：{error}"))
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

#[cfg(test)]
mod tests {
    use super::{migrate_project_log_directory, rollback_project_log_directory};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_log_root(label: &str) -> std::path::PathBuf {
        let suffix = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        std::env::temp_dir().join(format!("dcfc-{label}-{suffix}"))
    }

    #[test]
    fn log_directory_migration_moves_existing_project_logs() {
        let root = temporary_log_root("move-logs");
        let old = root.join("new-project");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("mcp.stderr.log"), "log").unwrap();

        let moved = migrate_project_log_directory(&root, "new-project", "realize").unwrap();

        assert!(moved);
        assert!(!old.exists());
        assert_eq!(std::fs::read_to_string(root.join("realize").join("mcp.stderr.log")).unwrap(), "log");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn log_directory_migration_rejects_existing_destination() {
        let root = temporary_log_root("log-conflict");
        std::fs::create_dir_all(root.join("new-project")).unwrap();
        std::fs::create_dir_all(root.join("realize")).unwrap();

        let error = migrate_project_log_directory(&root, "new-project", "realize").unwrap_err();

        assert!(error.contains("已存在"));
        assert!(root.join("new-project").exists());
        assert!(root.join("realize").exists());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn log_directory_migration_can_be_rolled_back_without_losing_logs() {
        let root = temporary_log_root("rollback-logs");
        let old = root.join("new-project-2");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("router.stderr.log"), "preserved").unwrap();

        assert!(migrate_project_log_directory(&root, "new-project-2", "dcfc").unwrap());
        rollback_project_log_directory(&root, "new-project-2", "dcfc").unwrap();

        assert!(!root.join("dcfc").exists());
        assert_eq!(std::fs::read_to_string(old.join("router.stderr.log")).unwrap(), "preserved");
        std::fs::remove_dir_all(root).unwrap();
    }
}
