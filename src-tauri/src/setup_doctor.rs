use crate::{
    config::{normalize_public_base_url, AppSettings},
    process_manager::{DelegateStatus, OverallState},
};
use serde::Serialize;
use std::{net::{IpAddr, ToSocketAddrs}, os::windows::process::CommandExt, path::Path, process::Command};

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

#[derive(Default)]
struct PublicProbe {
    base_status: Option<u16>,
    endpoint_status: Option<u16>,
    endpoint_valid: bool,
    error: Option<String>,
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
    let probe = endpoint.as_deref().map(probe_public_endpoint);
    evaluate(settings, status, endpoint, probe.as_ref())
}

fn probe_public_endpoint(endpoint: &str) -> PublicProbe {
    let mut probe = PublicProbe::default();
    let parsed = match url::Url::parse(endpoint) {
        Ok(parsed) => parsed,
        Err(_) => { probe.error = Some("Public URL 无效。".to_string()); return probe; }
    };
    let host = parsed.host_str().unwrap_or_default();
    let port = parsed.port_or_known_default().unwrap_or(443);
    let addresses = match (host, port).to_socket_addrs() {
        Ok(addresses) => addresses.collect::<Vec<_>>(),
        Err(_) => { probe.error = Some("Public URL 域名无法解析。".to_string()); return probe; }
    };
    if addresses.is_empty() || addresses.iter().any(|address| !public_ip(address.ip())) {
        probe.error = Some("Public URL 解析到本机、私网或保留地址，已停止公网探测。".to_string());
        return probe;
    }
    let pinned_ip = match addresses[0].ip() {
        IpAddr::V4(ip) => ip.to_string(),
        IpAddr::V6(ip) => format!("[{ip}]"),
    };
    let resolve = format!("{host}:{port}:{pinned_ip}");
    let curl = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".to_string())
        + r"\System32\curl.exe";
    let base = Command::new(&curl)
        .args(["-q", "--silent", "--show-error", "--noproxy", "*", "--connect-timeout", "3", "--max-time", "5", "--proto", "=https",
            "--resolve", &resolve, "--head", "--output", "NUL", "--write-out", "%{http_code}", endpoint])
        .creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW)
        .output();
    match base {
        Ok(output) if output.status.success() => {
            probe.base_status = String::from_utf8_lossy(&output.stdout).parse::<u16>().ok();
        }
        _ => probe.error = Some("HEAD 检查未完成；继续检测 MCP 根路由。".to_string()),
    }
    let body = r#"{"jsonrpc":"2.0","id":"dcfc-doctor","method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"dcfc-setup-doctor","version":"0.1.0"}}}"#;
    let response = Command::new(&curl)
        .args(["-q", "--silent", "--show-error", "--noproxy", "*", "--connect-timeout", "3", "--max-time", "5", "--max-filesize", "16384",
            "--proto", "=https", "--resolve", &resolve, "--header", "Content-Type: application/json",
            "--header", "Accept: application/json, text/event-stream",
            "--header", "MCP-Protocol-Version: 2025-06-18",
            "--data-raw", body, "--write-out", "\nDCFC_HTTP_STATUS:%{http_code}", endpoint])
        .creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW)
        .output();
    match response {
        Ok(output) => {
            let text = String::from_utf8_lossy(&output.stdout);
            if let Some((content, status)) = text.rsplit_once("\nDCFC_HTTP_STATUS:") {
                probe.endpoint_status = status.parse::<u16>().ok();
                probe.endpoint_valid = probe.endpoint_status.is_some_and(|code| (200..300).contains(&code))
                    && is_mcp_initialize_response(content);
                if probe.base_status.is_none() { probe.base_status = probe.endpoint_status; }
            }
            if probe.base_status.is_none() { probe.error = Some("从本机无法访问 Public URL；检查 TLS 与 Tunnel 公网映射。".to_string()); }
        }
        _ => probe.error = Some("公网地址有响应，但 MCP 初始化检查失败。".to_string()),
    }
    probe
}

fn is_mcp_initialize_response(content: &str) -> bool {
    let valid = |value: &str| serde_json::from_str::<serde_json::Value>(value).ok()
        .is_some_and(|json| json.get("jsonrpc").and_then(|value| value.as_str()) == Some("2.0")
            && json.pointer("/result/protocolVersion").and_then(|value| value.as_str()).is_some()
            && json.pointer("/result/serverInfo/name").and_then(|value| value.as_str()).is_some());
    valid(content) || content.lines().filter_map(|line| line.strip_prefix("data:")).any(|value| valid(value.trim()))
}

fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let octets = ip.octets();
            !ip.is_private() && !ip.is_loopback() && !ip.is_link_local()
                && !ip.is_broadcast() && !ip.is_documentation() && !ip.is_multicast()
                && !ip.is_unspecified() && octets[0] != 0 && octets[0] < 224
                && !(octets[0] == 100 && (64..=127).contains(&octets[1]))
                && !(octets[0] == 198 && (18..=19).contains(&octets[1]))
        }
        IpAddr::V6(ip) => {
            if let Some(mapped) = ip.to_ipv4_mapped() { return public_ip(IpAddr::V4(mapped)); }
            !ip.is_loopback() && !ip.is_unspecified() && !ip.is_unique_local()
                && !ip.is_unicast_link_local() && !ip.is_multicast()
                && (ip.segments()[0] & 0xe000) == 0x2000
                && !(ip.segments()[0] == 0x2001 && ip.segments()[1] == 0x0db8)
        }
    }
}

fn evaluate(settings: &AppSettings, status: &DelegateStatus, endpoint: Option<String>, probe: Option<&PublicProbe>) -> DoctorReport {
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
    checks.push(check("dependencies", "外部运行依赖", "shared", None,
        if proxy_exists && tunnel_exists { Ready } else { Action },
        if proxy_exists && tunnel_exists { "MCP Proxy 与 Tunnel Client 路径可用。" } else { "MCP Proxy 或 Tunnel Client 不存在。" },
        "在连接设置中指定由用户提供的 MCP Proxy 与 Tunnel Client。"));

    let magic_config_valid = settings.proxy_host == "127.0.0.1" && settings.proxy_port > 0;
    checks.push(check("magic", "Magic 本机代理", "shared", None,
        if !magic_config_valid { Action } else if status.proxy_ready { Ready } else { Action },
        if !magic_config_valid { "Magic 必须配置为 127.0.0.1 和有效端口。" } else if status.proxy_ready { "Magic 监听已就绪。" } else { "配置的 Magic 代理未就绪。" },
        "启动 Magic，并确认连接设置中的本机监听端口。"));

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

    let url_state = if settings.public_base_url.trim().is_empty() { Pending } else if endpoint.is_some() { Ready } else { Action };
    checks.push(check("public_url", "Public URL 地址格式", "registration", None, url_state,
        if url_state == Ready { "HTTPS 公网域名有效，已归一化为根路由地址。" } else if url_state == Pending { "Public URL 尚未填写；本地运行不受影响。" } else { "Public URL 不是有效的 HTTPS 公网基础地址。" },
        "在连接设置中填写 HTTPS 公网基础地址，不添加 /mcp、查询参数或本机地址。"));
    let reachability = probe.map(|probe| probe.base_status.is_some()).unwrap_or(false);
    checks.push(check("public_reachability", "公网地址可达性", "registration", None,
        if reachability { Ready } else if probe.is_some() { Action } else { Pending },
        if reachability { "本机 HTTPS 请求收到公网地址响应；这不证明 ChatGPT 可达。" } else if let Some(probe) = probe { probe.error.as_deref().unwrap_or("公网地址无响应。") } else { "等待有效 Public URL。" },
        "检查域名、TLS 证书与 Tunnel 映射；必要时从外部网络再验证。"));
    let endpoint_valid = probe.is_some_and(|probe| probe.endpoint_valid);
    let endpoint_detail = match probe {
        Some(probe) if probe.endpoint_valid => "派生的 Proxy 根路由返回 MCP initialize 响应。",
        Some(probe) if matches!(probe.endpoint_status, Some(401 | 403)) => "端点要求认证；未传送凭据，无法确认 MCP 协议。",
        Some(probe) if probe.endpoint_status.is_some() => "公网地址有响应，但派生根路由未返回有效 MCP initialize。",
        Some(_) => "未能验证 MCP 根路由。",
        None => "等待有效 Public URL。",
    };
    checks.push(check("endpoint", "DCFC MCP 端点", "registration", None,
        if endpoint_valid { Ready } else if probe.is_some() { Action } else { Pending }, endpoint_detail,
        "确认 Tunnel 指向当前 MCP Proxy 根路由，不要使用项目后端的 /mcp 地址。"));

    let local_ready = checks.iter().filter(|item| item.scope == "shared").all(|item| item.state == Ready) && running_projects > 0;
    let registration_ready = local_ready && endpoint_valid;
    checks.push(check("registration", "ChatGPT 注册准备", "registration", None,
        if registration_ready { Ready } else { Pending },
        if registration_ready { "本机与端点检查通过；ChatGPT 中的注册仍需用户完成。" } else { "先处理上述未就绪项；DCFC 不会代替 ChatGPT 注册或刷新工具。" },
        "在 ChatGPT 中使用下方端点创建应用；工具变化时在 ChatGPT 中按其界面重新同步。"));
    DoctorReport { checks, registration_endpoint: endpoint, local_ready, registration_ready }
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
        settings.proxy_executable = settings.mcp_executable.clone();
        settings.tunnel_executable = settings.mcp_executable.clone();
        let probe = PublicProbe { base_status: Some(405), endpoint_status: Some(200), endpoint_valid: true, error: None };
        let report = evaluate(&settings, &status, Some("https://dcfc.example.com/".to_string()), Some(&probe));
        assert!(report.local_ready && report.registration_ready);
        assert_eq!(report.checks.iter().filter(|check| check.scope == "project").count(), 12);
        assert_eq!(report.registration_endpoint.as_deref(), Some("https://dcfc.example.com/"));
    }

    #[test]
    fn public_url_failures_do_not_mark_healthy_local_runtime_failed() {
        let (mut settings, mut status) = fixture();
        let path = std::env::current_exe().unwrap().to_string_lossy().to_string();
        settings.mcp_executable = path.clone();
        settings.proxy_executable = path.clone();
        settings.tunnel_executable = path;
        settings.public_base_url.clear();
        let report = evaluate(&settings, &status, None, None);
        assert!(report.local_ready);
        assert!(!report.registration_ready);
        assert_eq!(state(&report, "public_url"), CheckState::Pending);
        settings.public_base_url = "https://localhost".to_string();
        let report = evaluate(&settings, &status, None, None);
        assert!(report.local_ready);
        assert_eq!(state(&report, "public_url"), CheckState::Action);

        settings.public_base_url = "https://dcfc.example.com".to_string();
        status.projects[3].mcp_ready = false;
        status.projects[3].overall = OverallState::Failed;
        status.projects[3].mcp_pid = None;
        let report = evaluate(&settings, &status, None, None);
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
        let report = evaluate(&settings, &status, None, None);
        for id in ["managed_runtime", "dependencies", "magic", "tunnel_configuration", "router"] {
            assert_eq!(state(&report, id), CheckState::Action, "{id}");
        }
        assert!(!report.local_ready);
    }

    #[test]
    fn reachable_public_host_is_not_enough_for_mcp_registration() {
        let (mut settings, status) = fixture();
        let path = std::env::current_exe().unwrap().to_string_lossy().to_string();
        settings.mcp_executable = path.clone();
        settings.proxy_executable = path.clone();
        settings.tunnel_executable = path;
        let probe = PublicProbe { base_status: Some(405), endpoint_status: Some(404), endpoint_valid: false, error: None };
        let report = evaluate(&settings, &status, Some("https://dcfc.example.com/".to_string()), Some(&probe));
        assert!(report.local_ready);
        assert_eq!(state(&report, "public_reachability"), CheckState::Ready);
        assert_eq!(state(&report, "endpoint"), CheckState::Action);
        assert!(!report.registration_ready);
    }

    #[test]
    fn public_probe_does_not_target_private_or_reserved_ips() {
        for address in ["127.0.0.1", "192.168.1.1", "10.0.0.2", "169.254.1.2",
            "100.64.0.1", "198.18.0.1", "2001:db8::1", "fc00::1", "::1"] {
            assert!(!public_ip(address.parse().unwrap()), "{address}");
        }
        assert!(public_ip("8.8.8.8".parse().unwrap()));
        assert!(public_ip("2606:4700:4700::1111".parse().unwrap()));
    }

    #[test]
    fn mcp_endpoint_requires_structured_initialize_result() {
        let response = r#"{"jsonrpc":"2.0","id":"dcfc-doctor","result":{"protocolVersion":"2025-06-18","serverInfo":{"name":"delegate-control-router","version":"1.0"},"capabilities":{}}}"#;
        assert!(is_mcp_initialize_response(response));
        assert!(is_mcp_initialize_response(&format!("event: message\ndata: {response}\n\n")));
        assert!(!is_mcp_initialize_response("<html>protocolVersion serverInfo</html>"));
        assert!(!is_mcp_initialize_response(r#"{"jsonrpc":"2.0","error":{"message":"denied"}}"#));
    }
}
