use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fs, path::{Path, PathBuf}, time::{SystemTime, UNIX_EPOCH}};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectConfig {
    pub id: String,
    pub name: String,
    pub output_directory: String,
    pub mcp_host: String,
    pub mcp_port: u16,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub proxy_host: String,
    pub proxy_port: u16,
    #[serde(default = "default_localhost")]
    pub mcp_proxy_host: String,
    #[serde(default = "default_mcp_proxy_port")]
    pub mcp_proxy_port: u16,
    #[serde(default = "default_router_port")]
    pub router_port: u16,
    #[serde(default = "default_localhost")]
    pub health_host: String,
    #[serde(default = "default_health_port")]
    pub health_port: u16,
    pub profile_name: String,
    #[serde(default)]
    pub tunnel_id: String,
    pub mcp_executable: String,
    pub proxy_executable: String,
    pub proxy_config_path: String,
    #[serde(default = "default_router_config_path")]
    pub router_config_path: String,
    pub tunnel_executable: String,
    #[serde(default)]
    pub projects: Vec<ProjectConfig>,
    #[serde(default)]
    pub active_project_id: Option<String>,
}

fn default_true() -> bool { true }
fn default_localhost() -> String { "127.0.0.1".to_string() }
fn default_mcp_proxy_port() -> u16 { 8100 }
fn default_router_port() -> u16 { 8101 }
fn default_health_port() -> u16 { 8080 }

fn default_proxy_config_path() -> String {
    app_data_root().join("mcp-proxy.toml").to_string_lossy().to_string()
}

fn default_router_config_path() -> String {
    app_data_root().join("router-projects.json").to_string_lossy().to_string()
}

fn bundled_mcp_executable() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
        .unwrap_or_default()
        .join("runtime")
        .join("chatgpt-delegate-edit")
        .join("chatgpt-delegate-edit.exe")
}

fn empty_settings() -> AppSettings {
    let mut settings = AppSettings::default();
    settings.projects.clear();
    settings.active_project_id = None;
    settings.profile_name.clear();
    settings.mcp_executable = bundled_mcp_executable().to_string_lossy().to_string();
    settings
}

impl Default for AppSettings {
    fn default() -> Self {
        let home = std::env::var("USERPROFILE").map(PathBuf::from).unwrap_or_else(|_| std::env::temp_dir());
        let default_project = ProjectConfig {
            id: "default".to_string(),
            name: "默认项目".to_string(),
            output_directory: home.join(".delegate-control").join("projects").join("default").to_string_lossy().to_string(),
            mcp_host: "127.0.0.1".to_string(),
            mcp_port: 8000,
            enabled: true,
        };
        Self {
            proxy_host: "127.0.0.1".to_string(),
            proxy_port: 7877,
            mcp_proxy_host: "127.0.0.1".to_string(),
            mcp_proxy_port: 8100,
            router_port: 8101,
            health_host: "127.0.0.1".to_string(),
            health_port: 8080,
            profile_name: "chatgpt-delegate".to_string(),
            tunnel_id: String::new(),
            mcp_executable: home.join(".local").join("bin").join("chatgpt-delegate.exe").to_string_lossy().to_string(),
            proxy_executable: home.join(".local").join("bin").join("mcp-proxy.exe").to_string_lossy().to_string(),
            proxy_config_path: default_proxy_config_path(),
            router_config_path: default_router_config_path(),
            tunnel_executable: home.join(".local").join("bin").join("tunnel-client.exe").to_string_lossy().to_string(),
            projects: vec![default_project],
            active_project_id: Some("default".to_string()),
        }
    }
}

impl AppSettings {
    pub fn validate(&self) -> Result<(), String> {
        if self.proxy_host.trim().is_empty() || self.mcp_proxy_host.trim().is_empty() || self.health_host.trim().is_empty() {
            return Err("代理、MCP Proxy 和健康检查主机地址不能为空。".to_string());
        }
        if self.proxy_port == 0 || self.router_port == 0 || self.health_port == 0 {
            return Err("代理、Router 和健康检查端口必须在 1 到 65535 之间。".to_string());
        }
        if self.mcp_proxy_port == self.router_port || self.mcp_proxy_port == self.health_port || self.router_port == self.health_port {
            return Err("MCP Proxy、Router 和健康端口不能相同。".to_string());
        }

        let mut ids = std::collections::HashSet::new();
        let mut slugs = std::collections::HashSet::new();
        let mut dirs = std::collections::HashSet::new();
        let mut ports = std::collections::HashSet::new();
        for project in &self.projects {
            if project.id.trim().is_empty() {
                return Err("项目 ID 不能为空。".to_string());
            }
            validate_project_key(&project.id)?;
            if !ids.insert(project.id.trim().to_string()) {
                return Err(format!("项目 ID 重复：{}。", project.id));
            }
        }
        for project in self.projects.iter().filter(|project| project.enabled) {
            if project.id.trim().is_empty() || project.name.trim().is_empty() {
                return Err("启用项目的名称和 ID 不能为空。".to_string());
            }
            if project.mcp_host.trim().is_empty() {
                return Err(format!("项目“{}”的 MCP 主机地址不能为空。", project.name));
            }
            let slug = backend_slug(&project.id)?;
            if !slugs.insert(slug) {
                return Err(format!("项目 namespace 重复：{}。", project.id));
            }
            let directory = project.output_directory.trim();
            if directory.is_empty() || !Path::new(directory).is_absolute() {
                return Err(format!("项目“{}”的输出目录必须是绝对路径。", project.name));
            }
            if !dirs.insert(normalized_path(directory)) {
                return Err(format!("项目“{}”的输出目录与其他项目重复。", project.name));
            }
            if project.mcp_port == 0 || project.mcp_port == self.mcp_proxy_port || project.mcp_port == self.router_port || project.mcp_port == self.health_port {
                return Err(format!("项目“{}”的 MCP 端口与共享端口冲突。", project.name));
            }
            if !ports.insert(project.mcp_port) {
                return Err(format!("启用项目的 MCP 端口重复：{}。", project.mcp_port));
            }
        }
        if let Some(active) = &self.active_project_id {
            if !self.projects.iter().any(|project| &project.id == active) {
                return Err("当前项目不存在。".to_string());
            }
        }
        Ok(())
    }
}

pub fn backend_slug(id: &str) -> Result<String, String> {
    let slug: String = id.chars().map(|character| {
        if character.is_ascii_alphanumeric() || character == '-' || character == '_' { character.to_ascii_lowercase() } else { '-' }
    }).collect();
    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() { Err(format!("项目 ID 无法生成有效 namespace：{id}。")) } else { Ok(slug) }
}

pub fn validate_project_key(id: &str) -> Result<(), String> {
    let value = id.trim();
    if value.is_empty() || value.len() > 64 {
        return Err(format!("项目 key 无效：{id}。必须是 1 到 64 个字符。"));
    }
    let valid = value.chars().enumerate().all(|(index, character)| {
        character.is_ascii_lowercase() || character.is_ascii_digit() || (index > 0 && (character == '-' || character == '_'))
    });
    if !valid {
        return Err(format!("项目 key 无效：{id}。只能使用小写字母、数字、短横线和下划线。"));
    }
    Ok(())
}

pub fn migrate_project_key_in_settings(settings: &AppSettings, old_id: &str, new_id: &str) -> Result<AppSettings, String> {
    let old_id = old_id.trim();
    let new_id = new_id.trim();
    if old_id == new_id {
        return Err("新的 Project Key 不能与当前 key 相同。".to_string());
    }
    validate_project_key(new_id)?;
    if settings.projects.iter().any(|project| project.id == new_id) {
        return Err(format!("Project Key 已存在：{new_id}。"));
    }
    if !settings.projects.iter().any(|project| project.id == old_id) {
        return Err(format!("找不到需要迁移的项目：{old_id}。"));
    }

    let mut migrated = settings.clone();
    let project = migrated.projects.iter_mut().find(|project| project.id == old_id).expect("project existence checked");
    project.id = new_id.to_string();
    if migrated.active_project_id.as_deref() == Some(old_id) {
        migrated.active_project_id = Some(new_id.to_string());
    }
    migrated.validate()?;
    Ok(migrated)
}

fn stable_project_key(name: &str, used: &std::collections::HashSet<String>) -> String {
    let mut slug: String = name
        .chars()
        .map(|character| if character.is_ascii_alphanumeric() { character.to_ascii_lowercase() } else { '-' })
        .collect();
    slug = slug.trim_matches('-').to_string();
    if slug.is_empty() { slug = "project".to_string(); }
    let base = slug.clone();
    let mut index = 2;
    while used.contains(&slug) {
        slug = format!("{base}-{index}");
        index += 1;
    }
    slug
}

fn migrate_project_keys(mut settings: AppSettings) -> AppSettings {
    let mut used = std::collections::HashSet::new();
    let mut mapping = std::collections::HashMap::new();
    for project in &mut settings.projects {
        let old = project.id.clone();
        let timestamp_key = old.strip_prefix("project-").is_some_and(|suffix| !suffix.is_empty() && suffix.chars().all(|character| character.is_ascii_digit()));
        if timestamp_key {
            project.id = stable_project_key(&project.name, &used);
        }
        used.insert(project.id.clone());
        mapping.insert(old, project.id.clone());
    }
    if let Some(active) = settings.active_project_id.as_ref().and_then(|id| mapping.get(id).cloned()) {
        settings.active_project_id = Some(active);
    }
    settings
}

fn normalized_path(path: &str) -> String {
    path.trim().trim_end_matches(['\\', '/']).replace('/', "\\").to_ascii_lowercase()
}

#[cfg(test)]
pub fn load_settings(path: &Path) -> AppSettings {
    load_settings_from_sources(path, &[])
}

pub fn load_application_settings(path: &Path) -> AppSettings {
    let sources = legacy_settings_paths();
    let mut settings = load_settings_from_sources(path, &sources);
    let stable_root = path.parent().unwrap_or_else(|| Path::new("."));
    let stable_proxy = stable_root.join("mcp-proxy.toml").to_string_lossy().to_string();
    let stable_router = stable_root.join("router-projects.json").to_string_lossy().to_string();
    let mut changed = false;
    if is_legacy_runtime_path(&settings.proxy_config_path, "mcp-proxy.toml") && settings.proxy_config_path != stable_proxy {
        settings.proxy_config_path = stable_proxy;
        changed = true;
    }
    if is_legacy_runtime_path(&settings.router_config_path, "router-projects.json") && settings.router_config_path != stable_router {
        settings.router_config_path = stable_router;
        changed = true;
    }
    if changed && !settings.projects.is_empty() {
        let _ = save_settings(path, &settings);
    }
    settings
}

fn load_settings_from_sources(path: &Path, additional_sources: &[PathBuf]) -> AppSettings {
    let current = read_candidate(path, None);
    let mut candidates = Vec::new();
    if let Some(candidate) = current.clone() {
        candidates.push(candidate);
    }
    candidates.extend(backup_paths(path).into_iter().filter_map(|candidate_path| read_candidate(&candidate_path, None)));
    for source in additional_sources {
        if same_path(source, path) { continue; }
        if let Some(candidate) = read_candidate(source, None) {
            candidates.push(candidate);
        }
        candidates.extend(backup_paths(source).into_iter().filter_map(|candidate_path| read_candidate(&candidate_path, None)));
    }

    if candidates.is_empty() {
        return empty_settings();
    }

    candidates.sort_by(|left, right| {
        right.project_count.cmp(&left.project_count)
            .then_with(|| right.modified.cmp(&left.modified))
            .then_with(|| right.modern.cmp(&left.modern))
    });
    let selected = candidates.into_iter().next().expect("candidate list is not empty");
    let selected_is_current = current.as_ref().is_some_and(|candidate| candidate.path == selected.path);
    if !selected_is_current {
        if path.exists() {
            let _ = preserve_recovery_copy(path);
        }
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::copy(&selected.path, path);
    }
    selected.settings
}

fn same_path(left: &Path, right: &Path) -> bool {
    left.to_string_lossy().replace('/', "\\").eq_ignore_ascii_case(&right.to_string_lossy().replace('/', "\\"))
}

fn is_legacy_runtime_path(path: &str, file_name: &str) -> bool {
    let normalized = path.replace('/', "\\").to_ascii_lowercase();
    normalized.ends_with(&format!("\\delegate control\\{}", file_name.to_ascii_lowercase()))
}

#[derive(Debug, Clone)]
struct SettingsCandidate {
    path: PathBuf,
    settings: AppSettings,
    project_count: usize,
    modified: SystemTime,
    modern: bool,
}

fn backup_paths(path: &Path) -> Vec<PathBuf> {
    let Some(parent) = path.parent() else { return Vec::new(); };
    let Some(file_name) = path.file_name() else { return Vec::new(); };
    let stem = file_name.to_string_lossy();
    let mut candidates = fs::read_dir(parent).ok().into_iter().flatten().filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with(&format!("{stem}.backup-")))
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    candidates.sort();
    candidates
}

fn read_candidate(path: &Path, modified: Option<SystemTime>) -> Option<SettingsCandidate> {
    let content = fs::read_to_string(path).ok()?;
    let value = serde_json::from_str::<Value>(&content).ok()?;
    let modern = value.get("projects").is_some();
    let settings = load_settings_from_value(value.clone());
    if settings.projects.is_empty() || settings.validate().is_err() {
        return None;
    }
    if modern && is_injected_default_payload(&value, &settings) {
        return None;
    }
    let modified = modified.or_else(|| fs::metadata(path).ok()?.modified().ok()).unwrap_or(UNIX_EPOCH);
    Some(SettingsCandidate { path: path.to_path_buf(), project_count: settings.projects.len(), settings, modified, modern })
}

fn is_injected_default_payload(value: &Value, settings: &AppSettings) -> bool {
    value.get("projects").and_then(Value::as_array).is_some()
        && settings.projects.iter().any(|project| project.id == "default" && project.name == "默认项目")
}

fn preserve_recovery_copy(path: &Path) -> Option<PathBuf> {
    let parent = path.parent()?;
    let file_name = path.file_name()?.to_string_lossy();
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_millis();
    let target = parent.join(format!("{file_name}.recovery-{stamp}"));
    fs::copy(path, &target).ok().map(|_| target)
}

pub fn load_settings_from_value(value: Value) -> AppSettings {
    if value.get("projects").is_some() {
        return serde_json::from_value(value).map(migrate_project_keys).unwrap_or_else(|_| empty_settings());
    }
    let has_legacy_project_fields = value.get("output_directory").is_some() || value.get("mcp_host").is_some() || value.get("mcp_port").is_some();
    if !has_legacy_project_fields {
        let defaults = AppSettings::default();
        return AppSettings {
            proxy_host: string_or_default(&value, "proxy_host", defaults.proxy_host),
            proxy_port: number_or_default(&value, "proxy_port", defaults.proxy_port),
            mcp_proxy_host: defaults.mcp_proxy_host,
            mcp_proxy_port: defaults.mcp_proxy_port,
            router_port: defaults.router_port,
            health_host: defaults.health_host,
            health_port: number_or_default(&value, "health_port", defaults.health_port),
            profile_name: string_or_default(&value, "profile_name", defaults.profile_name),
            tunnel_id: string_or_default(&value, "tunnel_id", defaults.tunnel_id),
            mcp_executable: string_or_default(&value, "mcp_executable", defaults.mcp_executable),
            proxy_executable: defaults.proxy_executable,
            proxy_config_path: defaults.proxy_config_path,
            router_config_path: defaults.router_config_path,
            tunnel_executable: string_or_default(&value, "tunnel_executable", defaults.tunnel_executable),
            projects: Vec::new(),
            active_project_id: None,
        };
    }
    let defaults = AppSettings::default();
    let string_value = |key: &str, fallback: String| value.get(key).and_then(Value::as_str).map(str::to_string).unwrap_or(fallback);
    let number_value = |key: &str, fallback: u16| value.get(key).and_then(Value::as_u64).and_then(|number| u16::try_from(number).ok()).unwrap_or(fallback);
    let project = ProjectConfig {
        id: "default".to_string(),
        name: "默认项目".to_string(),
        output_directory: string_value("output_directory", defaults.projects[0].output_directory.clone()),
        mcp_host: string_value("mcp_host", defaults.projects[0].mcp_host.clone()),
        mcp_port: number_value("mcp_port", defaults.projects[0].mcp_port),
        enabled: true,
    };
    AppSettings {
        proxy_host: string_value("proxy_host", defaults.proxy_host),
        proxy_port: number_value("proxy_port", defaults.proxy_port),
        mcp_proxy_host: "127.0.0.1".to_string(),
        mcp_proxy_port: defaults.mcp_proxy_port,
        router_port: defaults.router_port,
        health_host: "127.0.0.1".to_string(),
        health_port: number_value("health_port", defaults.health_port),
        profile_name: string_value("profile_name", defaults.profile_name),
        tunnel_id: string_value("tunnel_id", defaults.tunnel_id),
        mcp_executable: string_value("mcp_executable", defaults.mcp_executable),
        proxy_executable: defaults.proxy_executable,
        proxy_config_path: defaults.proxy_config_path,
        router_config_path: defaults.router_config_path,
        tunnel_executable: string_value("tunnel_executable", defaults.tunnel_executable),
        projects: vec![project],
        active_project_id: Some("default".to_string()),
    }
}

fn string_or_default(value: &Value, key: &str, fallback: String) -> String {
    value.get(key).and_then(Value::as_str).map(str::to_string).unwrap_or(fallback)
}

fn number_or_default(value: &Value, key: &str, fallback: u16) -> u16 {
    value.get(key).and_then(Value::as_u64).and_then(|number| u16::try_from(number).ok()).unwrap_or(fallback)
}

pub fn save_settings(path: &Path, settings: &AppSettings) -> Result<(), String> {
    settings.validate()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("无法创建设置目录：{error}"))?;
    }
    let json = serde_json::to_string_pretty(settings).map_err(|error| format!("无法序列化设置：{error}"))?;
    if path.exists() {
        preserve_settings_backup(path)?;
    }
    fs::write(path, json).map_err(|error| format!("无法保存设置：{error}"))
}

fn preserve_settings_backup(path: &Path) -> Result<PathBuf, String> {
    let parent = path.parent().ok_or_else(|| "设置文件缺少父目录。".to_string())?;
    let file_name = path.file_name().ok_or_else(|| "设置文件缺少文件名。".to_string())?.to_string_lossy();
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|error| format!("无法生成设置备份时间戳：{error}"))?.as_millis();
    let target = parent.join(format!("{file_name}.backup-{stamp}"));
    fs::copy(path, &target).map_err(|error| format!("无法备份现有设置：{error}"))?;
    Ok(target)
}

pub fn app_data_root() -> PathBuf {
    let home = std::env::var("USERPROFILE").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".delegate-control")
}

fn legacy_settings_paths() -> Vec<PathBuf> {
    let home = PathBuf::from(std::env::var("USERPROFILE").unwrap_or_else(|_| ".".to_string()));
    let mut paths = vec![home.join("AppData").join("Roaming").join("Delegate Control").join("settings.json")];
    if let Ok(appdata) = std::env::var("APPDATA") {
        paths.push(PathBuf::from(appdata).join("Delegate Control").join("settings.json"));
    }
    let packages = home.join("AppData").join("Local").join("Packages");
    if let Ok(entries) = fs::read_dir(packages) {
        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if name.starts_with("openai.codex_") {
                paths.push(entry.path().join("LocalCache").join("Roaming").join("Delegate Control").join("settings.json"));
            }
        }
    }
    paths.sort_by_key(|path| path.to_string_lossy().to_ascii_lowercase());
    paths.dedup_by(|left, right| same_path(left, right));
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_use_expected_ports() {
        let settings = AppSettings::default();
        assert_eq!(settings.proxy_port, 7877);
        assert_eq!(settings.mcp_proxy_port, 8100);
        assert_eq!(settings.router_port, 8101);
        assert_eq!(settings.projects[0].mcp_port, 8000);
        assert_eq!(settings.health_port, 8080);
        assert!(settings.validate().is_ok());
        assert!(Path::new(&settings.projects[0].output_directory).is_absolute());
        assert!(settings.mcp_executable.ends_with("chatgpt-delegate.exe"));
        assert!(settings.proxy_executable.ends_with("mcp-proxy.exe"));
        assert!(settings.tunnel_executable.ends_with("tunnel-client.exe"));
        let expected_home = std::env::var("USERPROFILE").map(PathBuf::from).unwrap_or_else(|_| std::env::temp_dir());
        for path in [&settings.projects[0].output_directory, &settings.mcp_executable, &settings.proxy_executable, &settings.tunnel_executable] {
            assert!(Path::new(path).starts_with(&expected_home));
            assert!(!path.contains("D:\\claudecode"));
        }
    }

    #[test]
    fn rejects_port_collision() {
        let mut settings = AppSettings::default();
        settings.projects[0].mcp_port = settings.health_port;
        assert!(settings.validate().is_err());
    }

    #[test]
    fn migrates_legacy_single_output_directory() {
        let legacy = serde_json::json!({
            "proxy_host": "127.0.0.1",
            "proxy_port": 7877,
            "mcp_host": "127.0.0.1",
            "mcp_port": 8000,
            "health_port": 8080,
            "profile_name": "chatgpt-delegate",
            "mcp_executable": "mcp.exe",
            "tunnel_executable": "tunnel.exe",
            "output_directory": "C:\\\\projects\\\\legacy"
        });
        let settings = load_settings_from_value(legacy);
        assert_eq!(settings.projects.len(), 1);
        assert_eq!(settings.projects[0].id, "default");
        assert_eq!(settings.projects[0].output_directory, "C:\\\\projects\\\\legacy");
        assert_eq!(settings.projects[0].mcp_port, 8000);
    }

    #[test]
    fn rejects_duplicate_enabled_project_directories_and_ports() {
        let mut settings = AppSettings::default();
        settings.projects.push(ProjectConfig { id: "other".to_string(), name: "其他".to_string(), output_directory: settings.projects[0].output_directory.clone(), mcp_host: "127.0.0.1".to_string(), mcp_port: 8000, enabled: true });
        assert!(settings.validate().is_err());
    }

    #[test]
    fn arbitrary_project_counts_remain_valid_for_onboarding_bypass_and_resume() {
        let root = std::env::temp_dir().join(format!("dcfc-onboarding-counts-{}", std::process::id()));
        for count in [0, 1, 3, 12] {
            let mut settings = settings_with_projects(count, &root.join(format!("count-{count}")));
            if count == 0 {
                settings.active_project_id = None;
            }
            assert!(settings.validate().is_ok(), "project count {count} should remain valid");
            assert_eq!(settings.projects.len(), count);
        }
    }

    #[test]
    fn creates_safe_backend_slug() {
        assert_eq!(backend_slug("Client A").unwrap(), "client-a");
        assert!(backend_slug("!!!").is_err());
    }

    #[test]
    fn validates_stable_project_keys() {
        assert!(validate_project_key("screencast").is_ok());
        assert!(validate_project_key("project-1790084958708").is_ok());
        assert!(validate_project_key("ScreenCast").is_err());
        assert!(validate_project_key("../escape").is_err());
    }

    #[test]
    fn migrates_timestamp_project_id_to_stable_name_key() {
        let mut value = serde_json::to_value(AppSettings::default()).unwrap();
        value["projects"] = serde_json::json!([{
            "id": "project-1790084958708",
            "name": "ScreenCast",
            "output_directory": "C:\\projects\\screencast",
            "mcp_host": "127.0.0.1",
            "mcp_port": 8002,
            "enabled": true
        }]);
        value["active_project_id"] = serde_json::json!("project-1790084958708");
        let settings = load_settings_from_value(value);
        assert_eq!(settings.projects[0].id, "screencast");
        assert_eq!(settings.active_project_id.as_deref(), Some("screencast"));
    }

    #[test]
    fn partial_settings_do_not_inject_default_project() {
        let settings = load_settings_from_value(serde_json::json!({"proxy_port": 7877}));
        assert!(settings.projects.is_empty());
        assert_eq!(settings.active_project_id, None);
        assert_eq!(settings.proxy_port, 7877);
    }

    #[test]
    fn malformed_project_payload_does_not_fall_back_to_default_project() {
        let settings = load_settings_from_value(serde_json::json!({"projects": "not-an-array"}));
        assert!(settings.projects.is_empty());
        assert_eq!(settings.active_project_id, None);
    }

    fn settings_with_projects(count: usize, prefix: &Path) -> AppSettings {
        let mut settings = AppSettings::default();
        settings.projects = (0..count).map(|index| ProjectConfig {
            id: format!("project-{index}"),
            name: format!("Project {index}"),
            output_directory: prefix.join(format!("project-{index}")).to_string_lossy().to_string(),
            mcp_host: "127.0.0.1".to_string(),
            mcp_port: 8000 + index as u16,
            enabled: true,
        }).collect();
        settings.active_project_id = settings.projects.last().map(|project| project.id.clone());
        settings
    }

    #[test]
    fn load_settings_prefers_largest_valid_backup_and_preserves_current_file() {
        let root = std::env::temp_dir().join(format!("dcfc-settings-recovery-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let path = root.join("settings.json");

        let mut current = settings_with_projects(2, &root.join("current"));
        current.projects[1].id = "default".to_string();
        current.projects[1].name = "默认项目".to_string();
        current.active_project_id = Some("default".to_string());
        fs::write(&path, serde_json::to_string_pretty(&current).unwrap()).unwrap();

        let backup = settings_with_projects(5, &root.join("backup"));
        let backup_path = root.join("settings.json.backup-20260928-largest");
        fs::write(&backup_path, serde_json::to_string_pretty(&backup).unwrap()).unwrap();

        let loaded = load_settings(&path);
        assert_eq!(loaded.projects.len(), 5);
        assert_eq!(loaded.active_project_id.as_deref(), Some("project-4"));
        assert!(root.join("settings.json.recovery-0").exists() || fs::read_dir(&root).unwrap().any(|entry| entry.unwrap().file_name().to_string_lossy().starts_with("settings.json.recovery-")));
        let persisted: AppSettings = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(persisted.projects.len(), 5);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn load_settings_keeps_current_valid_configuration_even_when_backup_is_smaller() {
        let root = std::env::temp_dir().join(format!("dcfc-settings-current-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let path = root.join("settings.json");
        let current = settings_with_projects(6, &root.join("current"));
        fs::write(&path, serde_json::to_string_pretty(&current).unwrap()).unwrap();
        let backup = settings_with_projects(3, &root.join("backup"));
        fs::write(root.join("settings.json.backup-small"), serde_json::to_string_pretty(&backup).unwrap()).unwrap();

        let loaded = load_settings(&path);
        assert_eq!(loaded.projects.len(), 6);
        assert_eq!(loaded.active_project_id.as_deref(), Some("project-5"));
        assert_eq!(fs::read_to_string(&path).unwrap(), serde_json::to_string_pretty(&current).unwrap());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn load_settings_does_not_restore_injected_default_without_a_valid_backup() {
        let root = std::env::temp_dir().join(format!("dcfc-settings-default-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let path = root.join("settings.json");
        let payload = serde_json::json!({
            "proxy_host": "127.0.0.1",
            "proxy_port": 7897,
            "projects": [{
                "id": "default",
                "name": "默认项目",
                "output_directory": root.join("default").to_string_lossy(),
                "mcp_host": "127.0.0.1",
                "mcp_port": 8000,
                "enabled": true
            }],
            "active_project_id": "default"
        });
        fs::write(&path, serde_json::to_string_pretty(&payload).unwrap()).unwrap();
        let loaded = load_settings(&path);
        assert!(loaded.projects.is_empty());
        assert_eq!(loaded.active_project_id, None);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn load_settings_rejects_injected_default_mixed_with_real_projects() {
        let root = std::env::temp_dir().join(format!("dcfc-settings-mixed-default-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let path = root.join("settings.json");
        let mut current = settings_with_projects(2, &root.join("current"));
        current.projects[1].id = "default".to_string();
        current.projects[1].name = "默认项目".to_string();
        current.active_project_id = Some("default".to_string());
        fs::write(&path, serde_json::to_string_pretty(&current).unwrap()).unwrap();

        let loaded = load_settings(&path);
        assert!(loaded.projects.is_empty());
        assert_eq!(loaded.active_project_id, None);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_settings_file_does_not_inject_default_project() {
        let root = std::env::temp_dir().join(format!("dcfc-settings-missing-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let loaded = load_settings(&root.join("settings.json"));
        assert!(loaded.projects.is_empty());
        assert_eq!(loaded.active_project_id, None);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn source_migration_selects_arbitrary_largest_collection_into_stable_root() {
        let root = std::env::temp_dir().join(format!("dcfc-settings-source-migration-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let stable = root.join("stable").join("settings.json");
        let source_a = root.join("roaming").join("settings.json");
        let source_b = root.join("package-cache").join("settings.json");
        fs::create_dir_all(source_a.parent().unwrap()).unwrap();
        fs::create_dir_all(source_b.parent().unwrap()).unwrap();
        fs::write(&source_a, serde_json::to_string_pretty(&settings_with_projects(4, &root.join("a"))).unwrap()).unwrap();
        fs::write(&source_b, serde_json::to_string_pretty(&settings_with_projects(8, &root.join("b"))).unwrap()).unwrap();

        let loaded = load_settings_from_sources(&stable, &[source_a, source_b]);
        assert_eq!(loaded.projects.len(), 8);
        assert_eq!(loaded.active_project_id.as_deref(), Some("project-7"));
        let persisted: AppSettings = serde_json::from_str(&fs::read_to_string(&stable).unwrap()).unwrap();
        assert_eq!(persisted.projects.len(), 8);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn save_settings_preserves_previous_arbitrary_project_collection() {
        let root = std::env::temp_dir().join(format!("dcfc-settings-save-backup-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let path = root.join("settings.json");
        let original = settings_with_projects(7, &root.join("original"));
        fs::write(&path, serde_json::to_string_pretty(&original).unwrap()).unwrap();
        let updated = settings_with_projects(9, &root.join("updated"));
        save_settings(&path, &updated).unwrap();

        let backup = fs::read_dir(&root).unwrap().filter_map(Result::ok)
            .find(|entry| entry.file_name().to_string_lossy().starts_with("settings.json.backup-"))
            .expect("save should preserve the previous settings file");
        let preserved: AppSettings = serde_json::from_str(&fs::read_to_string(backup.path()).unwrap()).unwrap();
        assert_eq!(preserved.projects.len(), 7);
        let current: AppSettings = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(current.projects.len(), 9);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn stable_root_is_outside_virtualized_appdata() {
        let root = app_data_root().to_string_lossy().replace('/', "\\").to_ascii_lowercase();
        assert!(root.ends_with("\\.delegate-control"));
        assert!(!root.contains("\\appdata\\"));
    }
}
