use serde::{Deserialize, Serialize};
use std::{fs, path::{Path, PathBuf}};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub proxy_host: String,
    pub proxy_port: u16,
    pub mcp_host: String,
    pub mcp_port: u16,
    pub health_port: u16,
    pub profile_name: String,
    pub mcp_executable: String,
    pub tunnel_executable: String,
    pub output_directory: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        let home = std::env::var("USERPROFILE").unwrap_or_else(|_| "C:\\Users\\Redmi".to_string());
        Self {
            proxy_host: "127.0.0.1".to_string(),
            proxy_port: 7897,
            mcp_host: "127.0.0.1".to_string(),
            mcp_port: 8000,
            health_port: 8080,
            profile_name: "chatgpt-delegate".to_string(),
            mcp_executable: format!("{home}\\.local\\bin\\chatgpt-delegate.exe"),
            tunnel_executable: "D:\\tunnel-client\\install\\tunnel-client.exe".to_string(),
            output_directory: "D:\\claudecode\\cchaha\\Project\\咨询\\artifacts\\chatgpt-delegation".to_string(),
        }
    }
}

impl AppSettings {
    pub fn validate(&self) -> Result<(), String> {
        if self.proxy_host.trim().is_empty() || self.mcp_host.trim().is_empty() {
            return Err("主机地址不能为空。".to_string());
        }
        if self.proxy_port == 0 || self.mcp_port == 0 || self.health_port == 0 {
            return Err("端口必须在 1 到 65535 之间。".to_string());
        }
        if self.mcp_port == self.health_port {
            return Err("MCP 端口与健康端口不能相同。".to_string());
        }
        Ok(())
    }
}

pub fn load_settings(path: &Path) -> AppSettings {
    fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
        .unwrap_or_default()
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
        assert_eq!(settings.proxy_port, 7897);
        assert_eq!(settings.mcp_port, 8000);
        assert_eq!(settings.health_port, 8080);
        assert!(settings.validate().is_ok());
    }

    #[test]
    fn rejects_port_collision() {
        let mut settings = AppSettings::default();
        settings.health_port = settings.mcp_port;
        assert!(settings.validate().is_err());
    }
}
