use crate::config::{AppSettings, ProjectConfig};
use serde::Serialize;
use std::{fs, path::Path};

#[derive(Debug, Serialize)]
struct ProxyFile {
    proxy: ProxySettings,
    backends: Vec<ProxyBackend>,
}

#[derive(Debug, Serialize)]
struct ProxySettings {
    name: String,
    separator: String,
    hot_reload: bool,
    instructions: String,
    listen: ProxyListen,
}

#[derive(Debug, Serialize)]
struct ProxyListen {
    host: String,
    port: u16,
}

#[derive(Debug, Serialize)]
struct ProxyBackend {
    name: String,
    transport: String,
    url: String,
}

pub fn build_proxy_toml(settings: &AppSettings, projects: &[ProjectConfig]) -> Result<String, String> {
    if projects.is_empty() {
        return Err("至少需要一个在线项目才能启动 MCP Proxy。".to_string());
    }
    let backends = vec![ProxyBackend {
        name: String::new(),
        transport: "http".to_string(),
        url: format!("http://{}:{}/mcp", settings.mcp_proxy_host, settings.router_port),
    }];
    let file = ProxyFile {
        proxy: ProxySettings {
            name: "delegate-control".to_string(),
            separator: "".to_string(),
            hot_reload: true,
            instructions: "工具目录固定不随项目数量变化。项目选择通过工具的可选 project_id 参数完成；未指定时使用 DCFC 当前项目。请求未明确项目时必须先澄清，禁止猜测输出目录。".to_string(),
            listen: ProxyListen { host: settings.mcp_proxy_host.clone(), port: settings.mcp_proxy_port },
        },
        backends,
    };
    toml::to_string_pretty(&file).map_err(|error| format!("无法生成 MCP Proxy 配置：{error}"))
}

#[derive(Debug, Serialize)]
struct RouterRegistry<'a> {
    active_project_id: Option<&'a str>,
    projects: Vec<RouterProject<'a>>,
}

#[derive(Debug, Serialize)]
struct RouterProject<'a> {
    id: &'a str,
    name: &'a str,
    output_directory: &'a str,
    enabled: bool,
}

pub fn build_router_registry(settings: &AppSettings, projects: &[ProjectConfig]) -> Result<String, String> {
    if projects.is_empty() {
        return Err("至少需要一个在线项目才能启动 Router MCP。".to_string());
    }
    let active = settings.active_project_id.as_deref().filter(|id| projects.iter().any(|project| project.id == *id));
    let registry = RouterRegistry {
        active_project_id: active,
        projects: projects.iter().map(|project| RouterProject { id: &project.id, name: &project.name, output_directory: &project.output_directory, enabled: true }).collect(),
    };
    serde_json::to_string_pretty(&registry).map_err(|error| format!("无法生成 Router 项目注册表：{error}"))
}

pub fn write_proxy_config(path: &Path, content: &str) -> Result<(), String> {
    let parent = path.parent().ok_or_else(|| "MCP Proxy 配置路径无效。".to_string())?;
    fs::create_dir_all(parent).map_err(|error| format!("无法创建 MCP Proxy 配置目录：{error}"))?;
    let temporary = path.with_extension("toml.tmp");
    fs::write(&temporary, content).map_err(|error| format!("无法写入 MCP Proxy 临时配置：{error}"))?;
    fs::rename(&temporary, path).map_err(|error| format!("无法替换 MCP Proxy 配置：{error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppSettings;

    #[test]
    fn projects_use_one_fixed_router_backend() {
        let settings = AppSettings::default();
        let mut second = settings.projects[0].clone();
        second.id = "Research B".to_string();
        second.name = "研究 B".to_string();
        second.mcp_port = 8001;
        second.output_directory = "C:\\projects\\research-b".to_string();
        let toml = build_proxy_toml(&settings, &[settings.projects[0].clone(), second]).unwrap();
        assert!(toml.contains("name = \"\""));
        assert!(toml.contains("separator = \"\""));
        assert!(!toml.contains("research-b"));
        assert!(toml.contains("url = \"http://127.0.0.1:8101/mcp\""));
        assert!(toml.contains("hot_reload = true"));
    }

    #[test]
    fn router_registry_contains_stable_project_keys_and_directories() {
        let mut settings = AppSettings::default();
        settings.projects[0].id = "screencast".to_string();
        settings.projects[0].name = "ScreenCast".to_string();
        settings.active_project_id = Some("screencast".to_string());
        let registry = build_router_registry(&settings, &[settings.projects[0].clone()]).unwrap();
        assert!(registry.contains("\"active_project_id\": \"screencast\""));
        assert!(registry.contains("\"id\": \"screencast\""));
        let payload: serde_json::Value = serde_json::from_str(&registry).unwrap();
        assert_eq!(payload["projects"][0]["output_directory"].as_str(), Some(settings.projects[0].output_directory.as_str()));
    }

    #[test]
    fn rejects_empty_backend_list() {
        assert!(build_proxy_toml(&AppSettings::default(), &[]).is_err());
    }
}
