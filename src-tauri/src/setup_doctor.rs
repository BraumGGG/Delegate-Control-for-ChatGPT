use crate::{
    config::{is_managed_mcp_proxy_path, normalize_public_base_url, AppSettings},
    process_manager::{DelegateStatus, OverallState},
};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckState { Ready, Action, Pending }

#[derive(Debug, Clone, Serialize)]
pub struct DoctorCheck {
    pub id: String,
    pub label: String,
    pub scope: String,
    pub project_id: Option<String>,
    pub state: CheckState,
    pub detail: String,
    pub action: String,
}

#[derive(Debug, Serialize)]
pub struct DoctorReport {
    pub checks: Vec<DoctorCheck>,
    pub registration_endpoint: Option<String>,
    pub local_ready: bool,
    pub registration_ready: bool,
}

fn check(id: &str, label: &str, scope: &str, project_id: Option<&str>, state: CheckState, detail: &str, action: &str) -> DoctorCheck {
    DoctorCheck {
        id: id.to_string(), label: label.to_string(), scope: scope.to_string(),
        project_id: project_id.map(str::to_string), state, detail: detail.to_string(), action: action.to_string(),
    }
}

pub fn diagnose(settings: &AppSettings, status: &DelegateStatus) -> DoctorReport {
    let endpoint = normalize_public_base_url(&settings.public_base_url).ok()
        .filter(|_| !settings.public_base_url.trim().is_empty())
        .map(|base| format!("{base}/"));
    evaluate(settings, status, endpoint)
}

fn evaluate(settings: &AppSettings, status: &DelegateStatus, endpoint: Option<String>) -> DoctorReport {
    use CheckState::{Action, Pending, Ready};
    let mut checks = Vec::new();
    let mut local_settings = settings.clone();
    local_settings.public_base_url.clear();
    let config_error = local_settings.validate().err();
    checks.push(check("configuration", "本地项目配置", "shared", None,
        if config_error.is_none() && !settings.projects.is_empty() { Ready } else { Action },
        config_error.as_deref().unwrap_or(if settings.projects.is_empty() { "尚未创建真实项目。" } else { "项目与端口配置有效。" }),
        "检查项目 key、输出目录与端口，保存有效配置。"));

    let executable = Path::new(&settings.mcp_executable);
    let managed_layout = settings.mcp_executable.to_ascii_lowercase().replace('/', "\\")
        .ends_with("\\runtime\\chatgpt-delegate-edit\\chatgpt-delegate-edit.exe");
    let runtime_ok = executable.is_file() && (!managed_layout || executable.parent().is_some_and(|dir| dir.join("_internal").is_dir()));
    checks.push(check("managed_runtime", "DCFC 本地运行组件", "shared", None,
        if runtime_ok { Ready } else { Action },
        if runtime_ok { "本地 MCP/Router 运行组件可用。" } else { "内置运行组件缺失或 onedir 依赖不完整。" },
        "重新安装 DCFC；不要手工选择内部组件。"));

    let proxy_exists = Path::new(&settings.proxy_executable).is_file();
    let tunnel_exists = Path::new(&settings.tunnel_executable).is_file();
    let managed_proxy = is_managed_mcp_proxy_path(&settings.proxy_executable);
    checks.push(check("managed_mcp_proxy", "DCFC 内置 MCP Proxy", "shared", None,
        if proxy_exists && managed_proxy { Ready } else { Action },
        if proxy_exists && managed_proxy { "安装包内置 MCP Proxy 可用。" } else if !proxy_exists && managed_proxy { "安装包内置 MCP Proxy 缺失或损坏。" } else { "当前不是安装包内置 MCP Proxy；请重新安装 DCFC。" },
        "重新安装 DCFC 以恢复内置 MCP Proxy；普通用户不需要配置 Proxy 路径。"));
    checks.push(check("dependencies", "外部运行依赖", "shared", None,
        if proxy_exists && tunnel_exists { Ready } else { Action },
        if proxy_exists && tunnel_exists && managed_proxy { "内置 MCP Proxy 与 Tunnel Client 均可用。" }
        else if !proxy_exists && managed_proxy { "内置 MCP Proxy 不存在。" }
        else if !managed_proxy { "MCP Proxy 不是安装包内置组件。" }
        else if !proxy_exists { "MCP Proxy 不存在。" }
        else { "Tunnel Client 不存在。" },
        "重新安装 DCFC 或选择实际存在的 Tunnel Client。"));

    let magic_config_valid = settings.network_mode == crate::config::NetworkMode::Direct || (settings.proxy_host == "127.0.0.1" && settings.proxy_port > 0);
    checks.push(check("magic", "Magic 本机代理", "shared", None,
        if settings.network_mode == crate::config::NetworkMode::Direct { Ready } else if !magic_config_valid { Action } else if status.proxy_ready { Ready } else { Action },
        if settings.network_mode == crate::config::NetworkMode::Direct { "Direct 模式已启用，不依赖 Magic 端口。" } else if !magic_config_valid { "Magic 必须配置为 127.0.0.1 和有效端口。" } else if status.proxy_ready { "Magic 监听已就绪。" } else { "配置的 Magic 代理未就绪。" },
        if settings.network_mode == crate::config::NetworkMode::Direct { "无需配置 Magic。" } else { "启动 Magic，并确认连接设置中的本机监听端口。" }));

    let tunnel_configured = status.credential_configured && (!settings.tunnel_id.trim().is_empty() || !settings.profile_name.trim().is_empty());
    checks.push(check("tunnel_configuration", "Tunnel 配置", "shared", None,
        if tunnel_configured { Ready } else { Action },
        if !status.credential_configured { "Runtime API Key 尚未配置。" } else if !tunnel_configured { "Tunnel ID 尚未配置。" } else if settings.tunnel_id.trim().is_empty() { "旧版 profile 配置可兼容运行；建议确认 Tunnel ID。" } else { "Tunnel ID 与密钥存储状态已配置。" },
        "在连接设置中配置 Tunnel ID；Runtime API Key 只保存到 Windows Credential Manager。"));

    let shared_started = status.router_pid.is_some() || status.proxy_pid.is_some() || status.tunnel_pid.is_some();
    let infrastructure_ready = status.router_ready && status.mcp_proxy_ready;
    checks.push(check("router", "共享 Router 与 MCP Proxy", "shared", None,
        if infrastructure_ready { Ready } else if shared_started { Action } else { Pending },
        if infrastructure_ready { "共享 Router 与 MCP Proxy 已就绪。" } else if shared_started { "共享进程已启动，但 Router 或 MCP Proxy 未就绪。" } else { "共享链路尚未启动。" },
        "启动项目后重试；若共享进程已启动仍失败，请查看 Router/MCP Proxy 日志。"));
    checks.push(check("tunnel_runtime", "Secure Tunnel 运行状态", "shared", None,
        if status.tunnel_ready { Ready } else if status.tunnel_pid.is_some() { Action } else { Pending },
        if status.tunnel_ready { "Tunnel 本地健康检查已通过。" } else if status.tunnel_pid.is_some() { "Tunnel 已启动但健康检查未通过。" } else { "Tunnel 尚未启动。" },
        "先修复共享链路，再查看 Tunnel 日志；不要自动重置配置。"));

    let mut running_projects = 0;
    for project in &settings.projects {
        let runtime = status.projects.iter().find(|item| item.project_id == project.id);
        let ready = project.enabled && runtime.is_some_and(|item| item.mcp_ready);
        if ready { running_projects += 1; }
        let failed = runtime.is_some_and(|item| item.overall == OverallState::Failed || (item.mcp_pid.is_some() && !item.mcp_ready));
        checks.push(check(&format!("project:{}", project.id), &project.name, "project", Some(&project.id),
            if ready { Ready } else if failed { Action } else { Pending },
            if ready { "项目 MCP 已就绪。" } else if !project.enabled { "项目已停用。" } else if failed { "该项目 MCP 启动或运行失败，其他项目不受此结论影响。" } else { "该项目尚未启动。" },
            "只检查该项目的进程、端口与项目 MCP 日志；不停止其他项目。"));
    }

    let local_ready = checks.iter().filter(|item| item.scope == "shared").all(|item| item.state == Ready) && running_projects > 0;
    DoctorReport { checks, registration_endpoint: endpoint, local_ready, registration_ready: false }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::{AppSettings, ProjectConfig}, process_manager::ProjectRuntimeStatus};

    fn fixture() -> (AppSettings, DelegateStatus) {
        let mut settings = AppSettings::default();
        settings.projects = (0..12).map(|i| ProjectConfig {
            id: format!("project-{i}"), name: format!("Project {i}"), output_directory: format!(r"C:\projects\{i}"),
            mcp_host: "127.0.0.1".to_string(), mcp_port: 8000 + i, enabled: true,
        }).collect();
        settings.active_project_id = Some("project-0".to_string());
        settings.public_base_url = "https://dcfc.example.com/".to_string();
        settings.tunnel_id = "tunnel-1".to_string();
        let status = DelegateStatus {
            overall: OverallState::Running, proxy_ready: true, router_ready: true, mcp_proxy_ready: true,
            mcp_ready: true, tunnel_ready: true, proxy_pid: Some(1), router_pid: Some(2),
            mcp_pid: Some(3), tunnel_pid: Some(4), credential_configured: true, text_editing_available: true,
            connector_capability_message: String::new(), message: String::new(),
            projects: settings.projects.iter().map(|p| ProjectRuntimeStatus {
                project_id: p.id.clone(), name: p.name.clone(), output_directory: p.output_directory.clone(),
                overall: OverallState::Running, mcp_ready: true, mcp_pid: Some(10),
                message: String::new(),
            }).collect(),
        };
        (settings, status)
    }

    fn state<'a>(report: &'a DoctorReport, id: &str) -> CheckState {
        report.checks.iter().find(|check| check.id == id).unwrap().state
    }

    #[test]
    fn healthy_local_and_public_endpoint_keep_arbitrary_projects() {
        let (mut settings, status) = fixture();
        let path = std::env::current_exe().unwrap();
        settings.mcp_executable = path.to_string_lossy().to_string();
        settings.proxy_executable = crate::config::bundled_mcp_proxy_executable().to_string_lossy().to_string();
        settings.tunnel_executable = settings.mcp_executable.clone();
        let report = evaluate(&settings, &status, Some("https://dcfc.example.com/".to_string()));
        assert_eq!(report.checks.iter().filter(|check| check.scope == "project").count(), 12);
        assert_eq!(report.registration_endpoint.as_deref(), Some("https://dcfc.example.com/"));
    }

    #[test]
    fn public_url_failures_do_not_mark_healthy_local_runtime_failed() {
        let (mut settings, mut status) = fixture();
        let path = std::env::current_exe().unwrap().to_string_lossy().to_string();
        settings.mcp_executable = path.clone();
        settings.proxy_executable = crate::config::bundled_mcp_proxy_executable().to_string_lossy().to_string();
        settings.tunnel_executable = path;
        settings.public_base_url.clear();
        let report = evaluate(&settings, &status, None);
        assert!(!report.registration_ready);
        settings.public_base_url = "https://localhost".to_string();
        let _report = evaluate(&settings, &status, None);
        settings.public_base_url = "https://dcfc.example.com".to_string();
        status.projects[3].mcp_ready = false;
        status.projects[3].overall = OverallState::Failed;
        status.projects[3].mcp_pid = None;
        let report = evaluate(&settings, &status, None);
        assert_eq!(state(&report, "project:project-3"), CheckState::Action);
        assert_eq!(state(&report, "project:project-4"), CheckState::Ready);
    }

    #[test]
    fn failure_matrix_separates_shared_and_project_checks() {
        let (mut settings, mut status) = fixture();
        settings.mcp_executable = "C:\\missing\\runtime.exe".to_string();
        settings.proxy_executable = "C:\\missing\\proxy.exe".to_string();
        settings.tunnel_executable = "C:\\missing\\tunnel.exe".to_string();
        settings.proxy_host = "0.0.0.0".to_string();
        settings.tunnel_id.clear();
        settings.profile_name.clear();
        status.proxy_ready = false;
        status.router_ready = false;
        status.router_pid = Some(2);
        let report = evaluate(&settings, &status, None);
        for id in ["managed_runtime", "managed_mcp_proxy", "dependencies", "magic", "tunnel_configuration", "router"] {
            assert_eq!(state(&report, id), CheckState::Action, "{id}");
        }
        assert!(!report.local_ready);
    }

    #[test]
    fn reachable_public_host_is_not_enough_for_mcp_registration() {
        let (mut settings, status) = fixture();
        let path = std::env::current_exe().unwrap().to_string_lossy().to_string();
        settings.mcp_executable = path.clone();
        settings.proxy_executable = crate::config::bundled_mcp_proxy_executable().to_string_lossy().to_string();
        settings.tunnel_executable = path;
        let report = evaluate(&settings, &status, Some("https://dcfc.example.com/".to_string()));
        assert!(!report.registration_ready);
    }
}
