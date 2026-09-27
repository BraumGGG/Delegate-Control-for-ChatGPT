use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fs, path::{Path, PathBuf}};

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
    let base = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(base).join("Delegate Control").join("mcp-proxy.toml").to_string_lossy().to_string()
}

fn default_router_config_path() -> String {
    let base = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(base).join("Delegate Control").join("router-projects.json").to_string_lossy().to_string()
}

fn empty_settings() -> AppSettings {
    let mut settings = AppSettings::default();
    settings.projects.clear();
    settings.active_project_id = None;
    settings
}

impl Default for AppSettings {
    fn default() -> Self {
        let home = std::env::var("USERPROFILE").unwrap_or_else(|_| "C:\\Users\\Redmi".to_string());
        let default_project = ProjectConfig {
            id: "default".to_string(),
            name: "默认项目".to_string(),
            output_directory: "D:\\claudecode\\cchaha\\Project\\咨询\\artifacts\\chatgpt-delegation".to_string(),
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
            mcp_executable: format!("{home}\\.local\\bin\\chatgpt-delegate.exe"),
            proxy_executable: format!("{home}\\.local\\bin\\mcp-proxy.exe"),
            proxy_config_path: default_proxy_config_path(),
            router_config_path: default_router_config_path(),
            tunnel_executable: "D:\\tunnel-client\\install\\tunnel-client.exe".to_string(),
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

pub fn load_settings(path: &Path) -> AppSettings {
    match fs::read_to_string(path) {
        Ok(content) => match serde_json::from_str::<Value>(&content) {
            Ok(value) => load_settings_from_value(value),
            Err(_) => load_settings_backup(path).unwrap_or_else(empty_settings),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => AppSettings::default(),
        Err(_) => load_settings_backup(path).unwrap_or_else(empty_settings),
    }
}

fn load_settings_backup(path: &Path) -> Option<AppSettings> {
    let parent = path.parent()?;
    let stem = path.file_name()?.to_string_lossy();
    let mut candidates = fs::read_dir(parent).ok()?.filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with(&format!("{stem}.backup-")))
        .filter_map(|entry| entry.metadata().ok().map(|metadata| (metadata.modified().ok(), entry.path())))
        .collect::<Vec<_>>();
    candidates.sort_by_key(|(modified, _)| *modified);
    candidates.into_iter().rev().find_map(|(_, candidate)| {
        let content = fs::read_to_string(candidate).ok()?;
        let value = serde_json::from_str::<Value>(&content).ok()?;
        let projects = value.get("projects")?.as_array()?;
        if projects.is_empty() { return None; }
        let settings = load_settings_from_value(value);
        settings.validate().ok().map(|_| settings)
    })
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
    fs::write(path, json).map_err(|error| format!("无法保存设置：{error}"))
}

pub fn app_data_root() -> PathBuf {
    let base = std::env::var("APPDATA").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(base).join("Delegate Control")
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
}
