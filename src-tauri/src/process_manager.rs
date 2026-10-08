use crate::{config::{AppSettings, NetworkMode, ProjectConfig}, credentials, proxy_config::{build_proxy_toml, build_router_registry, write_proxy_config}};
use serde::Serialize;
use std::{collections::HashMap, fs::{self, File, OpenOptions}, io::{Read, Write}, net::{TcpStream, ToSocketAddrs}, os::windows::{io::AsRawHandle, process::CommandExt}, path::{Path, PathBuf}, process::{Child, Command, Stdio}, thread, time::{Duration, Instant, SystemTime, UNIX_EPOCH}};
use windows_sys::Win32::{Foundation::{CloseHandle, HANDLE}, System::{JobObjects::{AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE}, Threading::CREATE_NO_WINDOW}};

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OverallState { Stopped, Starting, Running, Stopping, Failed }

#[derive(Debug, Clone, Serialize)]
pub struct ProjectRuntimeStatus {
    pub project_id: String,
    pub name: String,
    pub output_directory: String,
    pub overall: OverallState,
    pub mcp_ready: bool,
    pub mcp_pid: Option<u32>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DelegateStatus {
    pub overall: OverallState,
    pub proxy_ready: bool,
    pub router_ready: bool,
    pub mcp_proxy_ready: bool,
    pub mcp_ready: bool,
    pub tunnel_ready: bool,
    pub proxy_pid: Option<u32>,
    pub router_pid: Option<u32>,
    pub mcp_pid: Option<u32>,
    pub tunnel_pid: Option<u32>,
    pub credential_configured: bool,
    pub text_editing_available: bool,
    pub connector_capability_message: String,
    pub message: String,
    pub projects: Vec<ProjectRuntimeStatus>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeReadiness {
    pub mcp_available: bool,
    pub proxy_available: bool,
    pub tunnel_available: bool,
    pub credential_configured: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct MagicPortProbe {
    pub host: String,
    pub configured_port: u16,
    pub listening: bool,
    pub detected_port: Option<u16>,
    pub detail: String,
}

struct ManagedProject { child: Child }

#[derive(Debug, Serialize)]
struct StartupDiagnostic {
    timestamp_ms: u128,
    stage: String,
    state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    executable: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    config_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    config_exists: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    config_readable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    endpoint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tcp_ready: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    http_status: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    http_status_category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    log_directory: Option<String>,
    detail: String,
}

impl StartupDiagnostic {
    fn new(stage: &str, state: &str, detail: impl Into<String>) -> Self {
        let timestamp_ms = SystemTime::now().duration_since(UNIX_EPOCH).map(|value| value.as_millis()).unwrap_or_default();
        Self {
            timestamp_ms,
            stage: stage.to_string(),
            state: state.to_string(),
            executable: None,
            config_path: None,
            config_exists: None,
            config_readable: None,
            endpoint: None,
            tcp_ready: None,
            http_status: None,
            http_status_category: None,
            session_id: None,
            log_directory: None,
            detail: detail.into(),
        }
    }
}

pub struct ProcessManager {
    projects: HashMap<String, ManagedProject>,
    project_failures: HashMap<String, String>,
    proxy: Option<Child>,
    router: Option<Child>,
    tunnel: Option<Child>,
    job: Option<HANDLE>,
    overall: OverallState,
    message: String,
    log_dir: PathBuf,
    connector_capability_path: Option<String>,
    text_editing_available: bool,
    connector_capability_message: String,
    session_id: Option<String>,
}

unsafe impl Send for ProcessManager {}

impl ProcessManager {
    pub fn new(log_dir: PathBuf) -> Self {
        Self { projects: HashMap::new(), project_failures: HashMap::new(), proxy: None, router: None, tunnel: None, job: None, overall: OverallState::Stopped, message: "连接仅在需要时启动。".to_string(), log_dir, connector_capability_path: None, text_editing_available: false, connector_capability_message: "尚未检测 Connector 文件编辑能力。".to_string(), session_id: None }
    }

    pub fn status(&mut self, settings: &AppSettings) -> DelegateStatus {
        self.refresh_process_state();
        self.refresh_connector_capability(&settings.mcp_executable);
        let proxy_ready = settings.network_mode == NetworkMode::Direct || detect_clash_port(settings).is_ok();
        let router_ready = self.router.is_some() && tcp_ready(&settings.mcp_proxy_host, settings.router_port, 350);
        let mcp_proxy_ready = self.proxy.is_some() && tcp_ready(&settings.mcp_proxy_host, settings.mcp_proxy_port, 350);
        let tunnel_ready = self.tunnel.is_some() && http_ready(&settings.health_host, settings.health_port, 500);
        let projects = settings.projects.iter().map(|project| self.project_status(project)).collect::<Vec<_>>();
        let enabled = projects.iter().filter(|project| settings.projects.iter().any(|configured| configured.id == project.project_id && configured.enabled)).collect::<Vec<_>>();
        let mcp_ready = !enabled.is_empty() && enabled.iter().all(|project| project.mcp_ready);
        let selected = settings.active_project_id.as_ref().and_then(|id| self.projects.get(id)).or_else(|| self.projects.values().next());
        DelegateStatus { overall: self.overall, proxy_ready, router_ready, mcp_proxy_ready, mcp_ready, tunnel_ready, proxy_pid: self.proxy.as_ref().map(Child::id), router_pid: self.router.as_ref().map(Child::id), mcp_pid: selected.map(|managed| managed.child.id()), tunnel_pid: self.tunnel.as_ref().map(Child::id), credential_configured: credentials::credential_exists(), text_editing_available: self.text_editing_available, connector_capability_message: self.connector_capability_message.clone(), message: self.message.clone(), projects }
    }

    pub fn start(&mut self, settings: &AppSettings) -> Result<DelegateStatus, String> {
        self.start_all(settings)
    }

    pub fn start_all(&mut self, settings: &AppSettings) -> Result<DelegateStatus, String> {
        self.refresh_process_state();
        settings.validate()?;
        if settings.projects.iter().all(|project| !project.enabled) { return self.fail("至少启用一个项目后才能启动连接。".to_string()); }
        if self.all_enabled_running(settings) && self.router.is_some() && self.proxy.is_some() && self.tunnel.is_some() { return Ok(self.status(settings)); }
        self.stop_internal();
        self.begin_log_session()?;
        self.overall = OverallState::Starting;
        self.message = "正在启动多个项目的本地 MCP。".to_string();
        let key = credentials::read_runtime_key()?;
        require_file(&settings.mcp_executable, "MCP 程序")?;
        require_file(&settings.proxy_executable, "MCP Proxy 程序")?;
        require_file(&settings.tunnel_executable, "Tunnel 程序")?;
        self.job = Some(create_job()?);
        for project in settings.projects.iter().filter(|project| project.enabled) {
            if let Err(error) = self.start_project_mcp(settings, project) {
                self.stop_internal();
                return self.fail(error);
            }
        }
        if let Err(error) = self.start_shared(settings, &key) {
            self.stop_internal();
            return self.fail(error);
        }
        self.overall = OverallState::Running;
        self.message = "多个项目的 ChatGPT Delegate 连接已建立。".to_string();
        Ok(self.status(settings))
    }

    pub fn start_project(&mut self, settings: &AppSettings, project_id: &str) -> Result<DelegateStatus, String> {
        self.refresh_process_state();
        settings.validate()?;
        let project = settings.projects.iter().find(|project| project.id == project_id).ok_or_else(|| "找不到指定项目。".to_string())?;
        if !project.enabled {
            return self.fail(format!("项目“{}”尚未启用，请先在设置中启用后再启动。", project.name));
        }
        if self.projects.contains_key(project_id) { return Ok(self.status(settings)); }
        if self.projects.is_empty() && !self.shared_chain_ready() {
            self.begin_log_session()?;
        }
        if self.job.is_none() { self.job = Some(create_job()?); }
        self.overall = OverallState::Starting;
        if let Err(error) = self.start_project_mcp(settings, project) {
            if self.projects.is_empty() { self.close_job(); }
            return self.fail_preserving_projects(error);
        }
        if self.shared_chain_ready() {
            if let Err(error) = self.sync_router_registry(settings) {
                self.remove_project_process(project_id);
                if self.projects.is_empty() { self.close_job(); }
                return self.fail_preserving_projects(error);
            }
        } else {
            let key = match credentials::read_runtime_key() {
                Ok(key) => key,
                Err(error) => {
                    self.remove_project_process(project_id);
                    if self.projects.is_empty() { self.close_job(); }
                    return self.fail_preserving_projects(error);
                }
            };
            if self.router.is_some() || self.proxy.is_some() || self.tunnel.is_some() {
                self.stop_shared();
            }
            if let Err(error) = self.start_shared(settings, &key) {
                self.remove_project_process(project_id);
                if self.projects.is_empty() { self.close_job(); }
                return self.fail_preserving_projects(error);
            }
        }
        self.overall = OverallState::Running;
        self.message = format!("项目“{}”已在线。", project.name);
        Ok(self.status(settings))
    }

    pub fn stop(&mut self, settings: &AppSettings) -> DelegateStatus { self.stop_all(settings) }

    pub fn stop_all(&mut self, settings: &AppSettings) -> DelegateStatus {
        self.overall = OverallState::Stopping;
        self.message = "正在安全停止所有项目连接。".to_string();
        self.stop_internal();
        self.overall = OverallState::Stopped;
        self.message = "连接已停止，没有后台进程残留。".to_string();
        self.status(settings)
    }

    pub fn stop_project(&mut self, settings: &AppSettings, project_id: &str) -> Result<DelegateStatus, String> {
        self.refresh_process_state();
        let project = settings.projects.iter().find(|project| project.id == project_id).ok_or_else(|| "找不到指定项目。".to_string())?;
        let remaining = settings.projects.iter()
            .filter(|candidate| candidate.enabled && candidate.id != project_id && self.projects.contains_key(&candidate.id))
            .cloned()
            .collect::<Vec<_>>();
        if !remaining.is_empty() && self.shared_chain_ready() {
            if let Err(error) = self.write_router_registry(settings, &remaining) {
                return self.fail_preserving_projects(error);
            }
        }
        if let Some(mut managed) = self.projects.remove(project_id) { terminate_child(&mut managed.child); }
        self.project_failures.remove(project_id);
        if remaining.is_empty() {
            self.stop_shared();
            self.close_job();
            self.overall = OverallState::Stopped;
            self.message = format!("项目“{}”已停止。", project.name);
            return Ok(self.status(settings));
        }
        if !self.shared_chain_ready() {
            let key = match credentials::read_runtime_key() {
                Ok(key) => key,
                Err(error) => return self.fail_preserving_projects(error),
            };
            if self.router.is_some() || self.proxy.is_some() || self.tunnel.is_some() {
                self.stop_shared();
            }
            if let Err(error) = self.start_shared(settings, &key) {
                return self.fail_preserving_projects(error);
            }
        }
        self.overall = OverallState::Running;
        self.message = format!("项目“{}”已停止，其他项目仍在线。", project.name);
        Ok(self.status(settings))
    }

    pub fn read_logs(&self, source: &str, project_id: Option<&str>) -> Result<String, String> {
        let (names, directory) = match source {
            "mcp" => (vec!["mcp.stderr.log", "mcp.stdout.log"], project_id.map(|id| self.log_dir.join(id)).unwrap_or_else(|| self.log_dir.clone())),
            "proxy" => (vec!["proxy.stderr.log", "proxy.stdout.log"], self.log_dir.clone()),
            "router" => (vec!["router.stderr.log", "router.stdout.log"], self.log_dir.clone()),
            "tunnel" => (vec!["tunnel.stderr.log", "tunnel.log", "tunnel.stdout.log"], self.log_dir.clone()),
            "startup" => (vec!["startup-diagnostics.jsonl"], self.log_dir.clone()),
            _ => return Err("未知日志类型。".to_string()),
        };
        let mut combined = String::new();
        for name in names {
            if let Ok(content) = read_tail(&directory.join(name), 80_000) {
                if !content.trim().is_empty() { combined.push_str(&format!("===== {name} =====\n{content}\n")); }
            }
        }
        if combined.is_empty() { Ok("暂无日志。启动连接后将在这里显示运行信息。".to_string()) } else { Ok(combined) }
    }

    pub fn log_dir(&self) -> &Path { &self.log_dir }

    pub fn runtime_readiness(settings: &AppSettings) -> RuntimeReadiness {
        RuntimeReadiness {
            mcp_available: Path::new(&settings.mcp_executable).is_file(),
            proxy_available: Path::new(&settings.proxy_executable).is_file(),
            tunnel_available: Path::new(&settings.tunnel_executable).is_file(),
            credential_configured: credentials::credential_exists(),
        }
    }

    pub fn detect_magic_port(settings: &AppSettings) -> MagicPortProbe {
        let compatible_port = if settings.proxy_port == 7897 { 7877 } else { 7897 };
        if tcp_ready(&settings.proxy_host, settings.proxy_port, 700) {
            return MagicPortProbe {
                host: settings.proxy_host.clone(),
                configured_port: settings.proxy_port,
                listening: true,
                detected_port: Some(settings.proxy_port),
                detail: format!("已检测到本机代理监听 {}:{}。这只证明端口可连接，不代表代理出网或 Tunnel 已就绪。", settings.proxy_host, settings.proxy_port),
            };
        }
        if settings.proxy_port != compatible_port && tcp_ready(&settings.proxy_host, compatible_port, 700) {
            return MagicPortProbe {
                host: settings.proxy_host.clone(),
                configured_port: settings.proxy_port,
                listening: true,
                detected_port: Some(compatible_port),
                detail: format!("配置端口未监听，但检测到兼容端口 {}:{}。请把 Magic 端口改为 {} 后重新检测。", settings.proxy_host, compatible_port, compatible_port),
            };
        }
        MagicPortProbe {
            host: settings.proxy_host.clone(),
            configured_port: settings.proxy_port,
            listening: false,
            detected_port: None,
            detail: format!("未检测到 {}:{} 的 Magic 监听；DCFC 不会扫描所有端口。请在 Magic 的 HTTP/混合代理设置中确认端口后填写。", settings.proxy_host, settings.proxy_port),
        }
    }

    pub fn is_stopped(&self) -> bool {
        self.projects.is_empty() && self.proxy.is_none() && self.router.is_none() && self.tunnel.is_none()
    }

    fn refresh_connector_capability(&mut self, executable: &str) {
        if self.connector_capability_path.as_deref() == Some(executable) {
            return;
        }
        self.connector_capability_path = Some(executable.to_string());
        self.text_editing_available = false;
        let path = Path::new(executable);
        if !path.is_file() {
            self.connector_capability_message = "当前 MCP 程序不存在，暂时无法检测文件编辑能力。".to_string();
            return;
        }
        let output = Command::new(path)
            .args(["capabilities", "--json"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        let Ok(output) = output else {
            self.connector_capability_message = "当前 MCP 程序不支持能力检测，文件编辑工具不可用。".to_string();
            return;
        };
        if !output.status.success() {
            self.connector_capability_message = "当前 MCP 程序未提供文件编辑能力；现有报告工具仍可用。".to_string();
            return;
        }
        let Some(available) = parse_text_editing_capability(&output.stdout) else {
            self.connector_capability_message = "MCP 能力检测返回格式无效，文件编辑工具不可用。".to_string();
            return;
        };
        self.text_editing_available = available;
        self.connector_capability_message = if self.text_editing_available {
            "当前 MCP 支持项目内文本文件编辑。".to_string()
        } else {
            "当前 MCP 未提供文件编辑能力；现有报告工具仍可用。".to_string()
        };
    }

    fn project_status(&self, project: &ProjectConfig) -> ProjectRuntimeStatus {
        if let Some(managed) = self.projects.get(&project.id) {
            let ready = tcp_ready(&project.mcp_host, project.mcp_port, 350);
            return ProjectRuntimeStatus { project_id: project.id.clone(), name: project.name.clone(), output_directory: project.output_directory.clone(), overall: if ready { OverallState::Running } else { OverallState::Starting }, mcp_ready: ready, mcp_pid: Some(managed.child.id()), message: if ready { "项目 MCP 已就绪。".to_string() } else { "正在等待项目 MCP。".to_string() } };
        }
        let message = self.project_failures.get(&project.id).cloned().unwrap_or_else(|| if project.enabled { "项目未启动。".to_string() } else { "项目已停用。".to_string() });
        ProjectRuntimeStatus { project_id: project.id.clone(), name: project.name.clone(), output_directory: project.output_directory.clone(), overall: if self.project_failures.contains_key(&project.id) { OverallState::Failed } else { OverallState::Stopped }, mcp_ready: false, mcp_pid: None, message }
    }

    fn all_enabled_running(&self, settings: &AppSettings) -> bool { settings.projects.iter().filter(|project| project.enabled).all(|project| self.projects.contains_key(&project.id)) }

    fn shared_chain_ready(&self) -> bool {
        self.router.is_some() && self.proxy.is_some() && self.tunnel.is_some()
    }

    fn sync_router_registry(&self, settings: &AppSettings) -> Result<(), String> {
        let projects = settings.projects.iter()
            .filter(|project| project.enabled && self.projects.contains_key(&project.id))
            .cloned()
            .collect::<Vec<_>>();
        self.write_router_registry(settings, &projects)
    }

    fn write_router_registry(&self, settings: &AppSettings, projects: &[ProjectConfig]) -> Result<(), String> {
        let registry = build_router_registry(settings, projects)?;
        write_proxy_config(Path::new(&settings.router_config_path), &registry)
    }

    fn start_project_mcp(&mut self, settings: &AppSettings, project: &ProjectConfig) -> Result<(), String> {
        fs::create_dir_all(&project.output_directory).map_err(|error| format!("无法创建项目“{}”的输出目录：{error}", project.name))?;
        if tcp_ready(&project.mcp_host, project.mcp_port, 250) { return Err(format!("项目“{}”的 MCP 端口 {} 已被其他程序占用。", project.name, project.mcp_port)); }
        let project_log_dir = self.log_dir.join(&project.id);
        fs::create_dir_all(&project_log_dir).map_err(|error| format!("无法创建项目日志目录：{error}"))?;
        let stderr_path = project_log_dir.join("mcp.stderr.log");
        append_session_marker(&stderr_path, self.session_id.as_deref(), "project-mcp")?;
        append_session_marker(&project_log_dir.join("mcp.stdout.log"), self.session_id.as_deref(), "project-mcp")?;
        let stderr_offset = log_offset(&stderr_path);
        let stdout = log_file(&project_log_dir.join("mcp.stdout.log"))?;
        let stderr = log_file(&stderr_path)?;
        let mut command = Command::new(&settings.mcp_executable);
        command.args(["--output-dir", &project.output_directory, "serve", "--host", &project.mcp_host, "--port", &project.mcp_port.to_string()]).stdin(Stdio::null()).stdout(Stdio::from(stdout)).stderr(Stdio::from(stderr)).creation_flags(CREATE_NO_WINDOW);
        let mut child = command.spawn().map_err(|error| format!("无法启动项目“{}”的 MCP：{error}", project.name))?;
        if let Err(error) = assign_to_job(self.job.ok_or_else(|| "Windows Job Object 尚未创建。".to_string())?, &child) {
            terminate_child(&mut child);
            return Err(error);
        }
        let wait_result = wait_for_log_marker(&stderr_path, stderr_offset, b"Application startup complete.", Duration::from_secs(15), &mut child);
        if !matches!(wait_result, WaitResult::Ready) {
            terminate_child(&mut child);
            let reason = match wait_result { WaitResult::Exited => "MCP 进程已退出", WaitResult::TimedOut => "15 秒内未出现 MCP 启动完成标记", WaitResult::Ready => unreachable!() };
            return Err(format!("项目“{}”的 MCP 未能就绪：{}。请查看项目日志。当前 MCP 程序：{}", project.name, reason, settings.mcp_executable));
        }
        self.project_failures.remove(&project.id);
        self.projects.insert(project.id.clone(), ManagedProject { child });
        Ok(())
    }

    fn start_shared(&mut self, settings: &AppSettings, key: &str) -> Result<(), String> {
        let clash_port = resolve_clash_port(settings)?;
        if tcp_ready(&settings.mcp_proxy_host, settings.router_port, 250) { return Err(format!("Router 端口 {} 已被其他程序占用。", settings.router_port)); }
        if tcp_ready(&settings.mcp_proxy_host, settings.mcp_proxy_port, 250) { return Err(format!("MCP Proxy 端口 {} 已被其他程序占用。", settings.mcp_proxy_port)); }
        if tcp_ready(&settings.health_host, settings.health_port, 250) { return Err(format!("健康端口 {} 已被其他程序占用。", settings.health_port)); }
        let projects = settings.projects.iter().filter(|project| self.projects.contains_key(&project.id)).cloned().collect::<Vec<_>>();
        let router_endpoint = format!("http://{}:{}/mcp", settings.mcp_proxy_host, settings.router_port);
        let mut router_config_diag = StartupDiagnostic::new("router_registry", "writing", "正在生成 Router 项目注册表。");
        router_config_diag.config_path = Some(settings.router_config_path.clone());
        record_startup_diagnostic(&self.log_dir, router_config_diag);
        if let Err(error) = self.write_router_registry(settings, &projects) {
            let mut diagnostic = StartupDiagnostic::new("router_registry", "failed", error.clone());
            diagnostic.config_path = Some(settings.router_config_path.clone());
            record_startup_diagnostic(&self.log_dir, diagnostic);
            return Err(error);
        }
        let (router_registry_exists, router_registry_readable) = file_access_state(&settings.router_config_path);
        let mut router_registry_diag = StartupDiagnostic::new("router_registry", if router_registry_exists && router_registry_readable { "ready" } else { "failed" }, "Router 注册表已生成。");
        router_registry_diag.config_path = Some(settings.router_config_path.clone());
        router_registry_diag.config_exists = Some(router_registry_exists);
        router_registry_diag.config_readable = Some(router_registry_readable);
        record_startup_diagnostic(&self.log_dir, router_registry_diag);
        if !router_registry_exists || !router_registry_readable {
            return Err(format!("Router 注册表不可读：{}。请查看启动诊断。", settings.router_config_path));
        }
        let toml = build_proxy_toml(settings, &projects)?;
        let mut proxy_config_diag = StartupDiagnostic::new("proxy_config", "writing", "正在生成 MCP Proxy 配置。");
        proxy_config_diag.config_path = Some(settings.proxy_config_path.clone());
        record_startup_diagnostic(&self.log_dir, proxy_config_diag);
        if let Err(error) = write_proxy_config(Path::new(&settings.proxy_config_path), &toml) {
            let mut diagnostic = StartupDiagnostic::new("proxy_config", "failed", error.clone());
            diagnostic.config_path = Some(settings.proxy_config_path.clone());
            record_startup_diagnostic(&self.log_dir, diagnostic);
            return Err(error);
        }
        let (proxy_config_exists, proxy_config_readable) = file_access_state(&settings.proxy_config_path);
        let mut proxy_config_ready = StartupDiagnostic::new("proxy_config", if proxy_config_exists && proxy_config_readable { "ready" } else { "failed" }, "MCP Proxy 配置已生成。");
        proxy_config_ready.config_path = Some(settings.proxy_config_path.clone());
        proxy_config_ready.config_exists = Some(proxy_config_exists);
        proxy_config_ready.config_readable = Some(proxy_config_readable);
        record_startup_diagnostic(&self.log_dir, proxy_config_ready);
        if !proxy_config_exists || !proxy_config_readable {
            return Err(format!("MCP Proxy 配置不可读：{}。请查看启动诊断。", settings.proxy_config_path));
        }
        let (proxy_exists, proxy_readable) = file_access_state(&settings.proxy_executable);
        let mut proxy_executable_diag = StartupDiagnostic::new("proxy_executable", if proxy_exists && proxy_readable { "ready" } else { "failed" }, "已核对 MCP Proxy 实际执行文件。");
        proxy_executable_diag.executable = Some(settings.proxy_executable.clone());
        proxy_executable_diag.config_exists = Some(proxy_exists);
        proxy_executable_diag.config_readable = Some(proxy_readable);
        record_startup_diagnostic(&self.log_dir, proxy_executable_diag);
        if !proxy_exists || !proxy_readable {
            return Err(format!("MCP Proxy 程序不可读：{}。请查看启动诊断。", settings.proxy_executable));
        }
        fs::create_dir_all(&self.log_dir).map_err(|error| format!("无法创建日志目录：{error}"))?;
        let router_stderr_path = self.log_dir.join("router.stderr.log");
        let router_stderr_offset = log_offset(&router_stderr_path);
        let router_stdout = log_file(&self.log_dir.join("router.stdout.log"))?;
        let router_stderr = log_file(&router_stderr_path)?;
        let mut router_command = Command::new(&settings.mcp_executable);
        router_command.args(["router", "--config", &settings.router_config_path, "--host", &settings.mcp_proxy_host, "--port", &settings.router_port.to_string()]).stdin(Stdio::null()).stdout(Stdio::from(router_stdout)).stderr(Stdio::from(router_stderr)).creation_flags(CREATE_NO_WINDOW);
        let mut router = router_command.spawn().map_err(|error| format!("无法启动 Router MCP：{error}"))?;
        let mut router_process_diag = StartupDiagnostic::new("router_process", "started", "Router MCP 进程已创建。");
        router_process_diag.executable = Some(settings.mcp_executable.clone());
        router_process_diag.config_path = Some(settings.router_config_path.clone());
        router_process_diag.endpoint = Some(router_endpoint.clone());
        record_startup_diagnostic(&self.log_dir, router_process_diag);
        if let Err(error) = assign_to_job(self.job.ok_or_else(|| "Windows Job Object 尚未创建。".to_string())?, &router) {
            terminate_child(&mut router);
            return Err(error);
        }
        let wait_result = wait_for_log_marker(&router_stderr_path, router_stderr_offset, b"Application startup complete.", Duration::from_secs(15), &mut router);
        if !matches!(wait_result, WaitResult::Ready) {
            terminate_child(&mut router);
            let reason = match wait_result { WaitResult::Exited => "进程已退出", WaitResult::TimedOut => "15 秒内未出现 Router 启动完成标记", WaitResult::Ready => unreachable!() };
            let mut diagnostic = StartupDiagnostic::new("router_process", "failed", format!("Router 启动失败：{reason}。"));
            diagnostic.executable = Some(settings.mcp_executable.clone());
            diagnostic.config_path = Some(settings.router_config_path.clone());
            diagnostic.endpoint = Some(router_endpoint.clone());
            record_startup_diagnostic(&self.log_dir, diagnostic);
            return Err(format!("Router MCP 未能就绪：{}。请查看 Router 日志。当前程序：{}", reason, settings.mcp_executable));
        }
        // FastMCP writes its startup marker just before the Uvicorn socket is
        // observable on Windows. Do not let the proxy race the listener and
        // turn a transient 502 into a misleading "Proxy 未能就绪" failure.
        if !wait_for_tcp(&settings.mcp_proxy_host, settings.router_port, Duration::from_secs(10), &mut router) {
            terminate_child(&mut router);
            let mut diagnostic = StartupDiagnostic::new("router_tcp_readiness", "failed", "Router 启动日志已出现，但 TCP 端口不可连接。");
            diagnostic.config_path = Some(settings.router_config_path.clone());
            diagnostic.endpoint = Some(router_endpoint.clone());
            diagnostic.tcp_ready = Some(false);
            record_startup_diagnostic(&self.log_dir, diagnostic);
            return Err(format!("Router MCP 已报告启动但端口 {}:{} 仍不可连接。请查看 Router 日志。", settings.mcp_proxy_host, settings.router_port));
        }
        let mut router_tcp_diag = StartupDiagnostic::new("router_tcp_readiness", "ready", "Router TCP 端口可连接。");
        router_tcp_diag.config_path = Some(settings.router_config_path.clone());
        router_tcp_diag.endpoint = Some(router_endpoint.clone());
        router_tcp_diag.tcp_ready = Some(true);
        record_startup_diagnostic(&self.log_dir, router_tcp_diag);
        let (router_http_ready, router_http_status) = wait_for_mcp_http(&settings.mcp_proxy_host, settings.router_port, Duration::from_secs(10), &mut router);
        let mut router_http_diag = StartupDiagnostic::new("router_mcp_http_readiness", if router_http_ready { "ready" } else { "failed" }, if router_http_ready { "Router MCP initialize 预检成功。" } else { "Router MCP initialize 预检未返回 2xx。" });
        router_http_diag.config_path = Some(settings.router_config_path.clone());
        router_http_diag.endpoint = Some(router_endpoint.clone());
        router_http_diag.tcp_ready = Some(true);
        router_http_diag.http_status = router_http_status;
        router_http_diag.http_status_category = Some(http_status_category(router_http_status));
        record_startup_diagnostic(&self.log_dir, router_http_diag);
        if !router_http_ready {
            terminate_child(&mut router);
            let status = router_http_status.map(|code| code.to_string()).unwrap_or_else(|| "无响应".to_string());
            return Err(format!("Router MCP HTTP 预检失败：状态 {status}，端点 {router_endpoint}。请查看启动诊断和 Router 日志。"));
        }
        self.router = Some(router);
        let proxy_stderr_path = self.log_dir.join("proxy.stderr.log");
        let proxy_stderr_offset = log_offset(&proxy_stderr_path);
        let proxy_stdout = log_file(&self.log_dir.join("proxy.stdout.log"))?;
        let proxy_stderr = log_file(&proxy_stderr_path)?;
        let mut proxy_command = Command::new(&settings.proxy_executable);
        proxy_command.args(["--config", &settings.proxy_config_path]);
        apply_loopback_proxy_bypass(&mut proxy_command);
        proxy_command.stdin(Stdio::null()).stdout(Stdio::from(proxy_stdout)).stderr(Stdio::from(proxy_stderr)).creation_flags(CREATE_NO_WINDOW);
        let mut proxy_spawn_diag = StartupDiagnostic::new("proxy_process", "starting", "使用显式 --config 参数启动 MCP Proxy，并为本机 Router 设置 loopback 代理绕行。");
        proxy_spawn_diag.executable = Some(settings.proxy_executable.clone());
        proxy_spawn_diag.config_path = Some(settings.proxy_config_path.clone());
        proxy_spawn_diag.endpoint = Some(router_endpoint.clone());
        record_startup_diagnostic(&self.log_dir, proxy_spawn_diag);
        let mut proxy = proxy_command.spawn().map_err(|error| { self.stop_shared(); format!("无法启动 MCP Proxy：{error}") })?;
        if let Err(error) = assign_to_job(self.job.ok_or_else(|| "Windows Job Object 尚未创建。".to_string())?, &proxy) {
            terminate_child(&mut proxy);
            self.stop_shared();
            return Err(error);
        }
        // mcp-proxy styles the `listen=` field with ANSI escapes when launched
        // from the desktop app, so only match the stable unstyled prefix.
        let wait_result = wait_for_log_marker(&proxy_stderr_path, proxy_stderr_offset, b"Proxy ready ", Duration::from_secs(15), &mut proxy);
        if !matches!(wait_result, WaitResult::Ready) {
            terminate_child(&mut proxy);
            self.stop_shared();
            let reason = match wait_result { WaitResult::Exited => "进程已退出", WaitResult::TimedOut => "15 秒内未出现 Proxy 就绪标记", WaitResult::Ready => unreachable!() };
            let mut diagnostic = StartupDiagnostic::new("proxy_backend_initialization", "failed", format!("MCP Proxy backend 初始化失败：{reason}。"));
            diagnostic.executable = Some(settings.proxy_executable.clone());
            diagnostic.config_path = Some(settings.proxy_config_path.clone());
            diagnostic.endpoint = Some(router_endpoint.clone());
            record_startup_diagnostic(&self.log_dir, diagnostic);
            return Err(format!("MCP Proxy 未能就绪：{}。请查看 proxy 日志（目录：{}）。当前程序：{}", reason, self.log_dir.display(), settings.proxy_executable));
        }
        let mut proxy_ready_diag = StartupDiagnostic::new("proxy_backend_initialization", "ready", "MCP Proxy backend 已初始化并报告 Proxy ready。");
        proxy_ready_diag.executable = Some(settings.proxy_executable.clone());
        proxy_ready_diag.config_path = Some(settings.proxy_config_path.clone());
        proxy_ready_diag.endpoint = Some(router_endpoint);
        record_startup_diagnostic(&self.log_dir, proxy_ready_diag);
        self.proxy = Some(proxy);

        let tunnel_log = self.log_dir.join("tunnel.log");
        let tunnel_stdout = log_file(&self.log_dir.join("tunnel.stdout.log"))?;
        let tunnel_stderr = log_file(&self.log_dir.join("tunnel.stderr.log"))?;
        let mut tunnel_command = Command::new(&settings.tunnel_executable);
        // mcp-proxy exposes its Streamable HTTP router at the root path. The
        // project backends behind it still use /mcp, but the shared Tunnel
        // must target the proxy root so initialize requests are not 404.
        let tunnel_args = ["--mcp.server-url", &format!("url=http://{}:{},channel=main", settings.mcp_proxy_host, settings.mcp_proxy_port), "--open-web-ui=false", "--log.file", &tunnel_log.to_string_lossy()];
        tunnel_command.args(tunnel_identity_args(settings));
        if let Some(clash_port) = clash_port {
            tunnel_command.args(["--control-plane.http-proxy", &format!("http://{}:{}", settings.proxy_host, clash_port)]);
        }
        tunnel_command.args(tunnel_args)
            .env("CONTROL_PLANE_API_KEY", key).stdin(Stdio::null()).stdout(Stdio::from(tunnel_stdout)).stderr(Stdio::from(tunnel_stderr)).creation_flags(CREATE_NO_WINDOW);
        let mut tunnel = tunnel_command.spawn().map_err(|error| { self.stop_shared(); format!("无法启动 Tunnel：{error}") })?;
        if let Err(error) = assign_to_job(self.job.ok_or_else(|| "Windows Job Object 尚未创建。".to_string())?, &tunnel) {
            terminate_child(&mut tunnel);
            self.stop_shared();
            return Err(error);
        }
        if !wait_for_http(&settings.health_host, settings.health_port, Duration::from_secs(30), &mut tunnel) { terminate_child(&mut tunnel); self.stop_shared(); return Err("Tunnel 未能在 30 秒内就绪，请检查密钥和 Tunnel 日志。".to_string()); }
        self.tunnel = Some(tunnel);
        Ok(())
    }

    fn stop_shared(&mut self) { if let Some(mut child) = self.tunnel.take() { terminate_child(&mut child); } if let Some(mut child) = self.proxy.take() { terminate_child(&mut child); } if let Some(mut child) = self.router.take() { terminate_child(&mut child); } }

    fn begin_log_session(&mut self) -> Result<(), String> {
        let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|error| format!("无法生成日志会话时间戳：{error}"))?.as_millis();
        let session_id = format!("session-{timestamp}");
        archive_and_clear_runtime_logs(&self.log_dir, &session_id)?;
        self.session_id = Some(session_id.clone());
        for name in ["proxy.stderr.log", "proxy.stdout.log", "router.stderr.log", "router.stdout.log", "tunnel.stderr.log", "tunnel.stdout.log", "tunnel.log"] {
            append_session_marker(&self.log_dir.join(name), Some(&session_id), "shared")?;
        }
        let mut diagnostic = StartupDiagnostic::new("session", "started", "已创建新的运行日志会话；上一会话日志已归档，当前日志文件仅包含本次启动内容。");
        diagnostic.session_id = Some(session_id);
        record_startup_diagnostic(&self.log_dir, diagnostic);
        Ok(())
    }

    fn refresh_process_state(&mut self) {
        let dead = self.projects.iter_mut().filter_map(|(id, managed)| managed.child.try_wait().ok().flatten().map(|_| id.clone())).collect::<Vec<_>>();
        for id in dead { self.projects.remove(&id); self.project_failures.insert(id.clone(), "项目 MCP 进程意外退出，请查看项目日志。".to_string()); self.overall = OverallState::Failed; self.message = format!("项目 {id} 的 MCP 进程意外退出。"); }
        let proxy_dead = self.proxy.as_mut().and_then(|child| child.try_wait().ok()).flatten().is_some();
        let router_dead = self.router.as_mut().and_then(|child| child.try_wait().ok()).flatten().is_some();
        let tunnel_dead = self.tunnel.as_mut().and_then(|child| child.try_wait().ok()).flatten().is_some();
        if router_dead || proxy_dead || tunnel_dead {
            self.stop_shared();
            self.overall = OverallState::Failed;
            self.message = if router_dead { "Router MCP 进程意外退出，请查看日志。" } else if proxy_dead { "MCP Proxy 进程意外退出，请查看日志。" } else { "Tunnel 进程意外退出，请查看日志。" }.to_string();
        }
    }

    fn stop_internal(&mut self) {
        if let Some(job) = self.job.take() { unsafe { TerminateJobObject(job, 0); CloseHandle(job); } }
        for (_, mut managed) in self.projects.drain() { let _ = managed.child.wait(); }
        if let Some(mut child) = self.router.take() { let _ = child.wait(); }
        if let Some(mut child) = self.proxy.take() { let _ = child.wait(); }
        if let Some(mut child) = self.tunnel.take() { let _ = child.wait(); }
    }

    fn close_job(&mut self) {
        if let Some(job) = self.job.take() { unsafe { CloseHandle(job); } }
    }

    fn remove_project_process(&mut self, project_id: &str) {
        if let Some(mut managed) = self.projects.remove(project_id) { terminate_child(&mut managed.child); }
        self.project_failures.remove(project_id);
    }

    fn fail_preserving_projects<T>(&mut self, message: String) -> Result<T, String> {
        self.overall = if self.projects.is_empty() { OverallState::Failed } else { OverallState::Running };
        self.message = message.clone();
        Err(message)
    }

    fn fail<T>(&mut self, message: String) -> Result<T, String> { self.overall = OverallState::Failed; self.message = message.clone(); Err(message) }
}

fn tunnel_identity_args(settings: &AppSettings) -> Vec<&str> {
    let mut args = vec!["run"];
    if !settings.profile_name.trim().is_empty() {
        args.extend(["--profile", settings.profile_name.as_str()]);
    }
    if !settings.tunnel_id.trim().is_empty() {
        args.extend(["--control-plane.tunnel-id", settings.tunnel_id.as_str()]);
    }
    args
}

impl Drop for ProcessManager { fn drop(&mut self) { self.stop_internal(); } }

fn create_job() -> Result<HANDLE, String> {
    let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    if job.is_null() { return Err(format!("无法创建 Windows Job Object：{}", std::io::Error::last_os_error())); }
    let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
    info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    let ok = unsafe { SetInformationJobObject(job, JobObjectExtendedLimitInformation, &info as *const _ as *const _, std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32) };
    if ok == 0 { unsafe { CloseHandle(job) }; return Err(format!("无法配置 Windows Job Object：{}", std::io::Error::last_os_error())); }
    Ok(job)
}

fn assign_to_job(job: HANDLE, child: &Child) -> Result<(), String> { if unsafe { AssignProcessToJobObject(job, child.as_raw_handle() as HANDLE) } == 0 { Err(format!("无法将进程加入 Job Object：{}", std::io::Error::last_os_error())) } else { Ok(()) } }
fn terminate_child(child: &mut Child) { let _ = child.kill(); let _ = child.wait(); }
fn require_file(path: &str, label: &str) -> Result<(), String> { if Path::new(path).is_file() { Ok(()) } else { Err(format!("找不到{label}：{path}")) } }
fn loopback_bypass_value(existing: Option<&str>) -> String {
    let mut values = existing.unwrap_or_default().split(',').map(str::trim).filter(|value| !value.is_empty()).map(str::to_ascii_lowercase).collect::<Vec<_>>();
    for value in ["localhost", "127.0.0.1", "::1"] {
        if !values.iter().any(|current| current == value) { values.push(value.to_string()); }
    }
    values.join(",")
}
fn apply_loopback_proxy_bypass(command: &mut Command) {
    let existing_upper = std::env::var("NO_PROXY").ok();
    let existing_lower = std::env::var("no_proxy").ok();
    let bypass = loopback_bypass_value(existing_upper.as_deref().or(existing_lower.as_deref()));
    command.env("NO_PROXY", &bypass).env("no_proxy", bypass);
}
fn log_file(path: &Path) -> Result<File, String> { if let Some(parent) = path.parent() { fs::create_dir_all(parent).map_err(|error| format!("无法创建日志目录：{error}"))?; } OpenOptions::new().create(true).append(true).open(path).map_err(|error| format!("无法打开日志 {}：{error}", path.display())) }
fn log_offset(path: &Path) -> u64 { fs::metadata(path).map(|metadata| metadata.len()).unwrap_or(0) }
fn append_session_marker(path: &Path, session_id: Option<&str>, component: &str) -> Result<(), String> {
    let Some(session_id) = session_id else { return Ok(()); };
    if let Some(parent) = path.parent() { fs::create_dir_all(parent).map_err(|error| format!("无法创建日志目录：{error}"))?; }
    let mut file = OpenOptions::new().create(true).append(true).open(path).map_err(|error| format!("无法写入日志会话标记 {}：{error}", path.display()))?;
    writeln!(file, "===== DCFC {component} log session {session_id} =====").map_err(|error| format!("无法写入日志会话标记 {}：{error}", path.display()))
}
fn collect_runtime_log_files(root: &Path, output: &mut Vec<PathBuf>) -> Result<(), String> {
    if !root.is_dir() { return Ok(()); }
    for entry in fs::read_dir(root).map_err(|error| format!("无法读取日志目录 {}：{error}", root.display()))? {
        let entry = entry.map_err(|error| format!("无法读取日志目录项：{error}"))?;
        let path = entry.path();
        if path.file_name().is_some_and(|name| name == "archive") { continue; }
        if path.is_dir() {
            collect_runtime_log_files(&path, output)?;
        } else if path.extension().is_some_and(|extension| extension == "log") || path.file_name().is_some_and(|name| name == "startup-diagnostics.jsonl") {
            output.push(path);
        }
    }
    Ok(())
}
fn archive_and_clear_runtime_logs(log_dir: &Path, session_id: &str) -> Result<(), String> {
    fs::create_dir_all(log_dir).map_err(|error| format!("无法创建日志目录：{error}"))?;
    let mut files = Vec::new();
    collect_runtime_log_files(log_dir, &mut files)?;
    if files.is_empty() { return Ok(()); }
    let archive_root = log_dir.join("archive").join(session_id);
    for path in files {
        let relative = path.strip_prefix(log_dir).map_err(|error| format!("无法计算日志归档路径：{error}"))?;
        let destination = archive_root.join(relative);
        if let Some(parent) = destination.parent() { fs::create_dir_all(parent).map_err(|error| format!("无法创建日志归档目录：{error}"))?; }
        fs::copy(&path, &destination).map_err(|error| format!("无法归档日志 {}：{error}", path.display()))?;
        OpenOptions::new().write(true).truncate(true).open(&path).map_err(|error| format!("无法清空当前日志 {}：{error}", path.display()))?;
    }
    Ok(())
}
fn record_startup_diagnostic(log_dir: &Path, mut diagnostic: StartupDiagnostic) {
    let path = log_dir.join("startup-diagnostics.jsonl");
    diagnostic.log_directory = Some(log_dir.to_string_lossy().to_string());
    let Ok(mut file) = log_file(&path) else { return; };
    let Ok(line) = serde_json::to_string(&diagnostic) else { return; };
    let _ = writeln!(file, "{line}");
}
fn file_access_state(path: &str) -> (bool, bool) {
    let exists = Path::new(path).is_file();
    let readable = exists && File::open(path).is_ok();
    (exists, readable)
}
fn http_status_category(status: Option<u16>) -> String {
    match status {
        Some(code) if (200..300).contains(&code) => "2xx_ready".to_string(),
        Some(400..500) => "4xx_client_or_protocol".to_string(),
        Some(500..600) => "5xx_server_or_intermediary".to_string(),
        Some(_) => "other_http_status".to_string(),
        None => "no_http_response".to_string(),
    }
}
fn log_contains_marker(path: &Path, offset: u64, marker: &[u8]) -> bool {
    if marker.is_empty() { return false; }
    let Ok(bytes) = fs::read(path) else { return false; };
    let start = usize::try_from(offset).unwrap_or(usize::MAX).min(bytes.len());
    bytes[start..].windows(marker.len()).any(|window| window == marker)
}
fn detect_clash_port(settings: &AppSettings) -> Result<u16, String> {
    let compatible_port = if settings.proxy_port == 7897 { 7877 } else { 7897 };
    detect_clash_port_with_legacy(settings, compatible_port)
}
fn resolve_clash_port(settings: &AppSettings) -> Result<Option<u16>, String> {
    match settings.network_mode {
        NetworkMode::Direct => Ok(None),
        NetworkMode::Magic => detect_clash_port(settings).map(Some),
    }
}
fn detect_clash_port_with_legacy(settings: &AppSettings, legacy_port: u16) -> Result<u16, String> {
    if tcp_ready(&settings.proxy_host, settings.proxy_port, 700) {
        return Ok(settings.proxy_port);
    }
    if settings.proxy_port != legacy_port && tcp_ready(&settings.proxy_host, legacy_port, 700) {
        return Ok(legacy_port);
    }
    if settings.proxy_port != legacy_port {
        return Err(format!("Magic 代理未就绪：已检查 {}:{} 和兼容旧端口 {}:{}。请先启动本机代理或在连接设置中确认端口。", settings.proxy_host, settings.proxy_port, settings.proxy_host, legacy_port));
    }
    Err(format!("Magic 代理未就绪：{}:{}。请先启动本机代理后重试。", settings.proxy_host, settings.proxy_port))
}
fn tcp_ready(host: &str, port: u16, timeout_ms: u64) -> bool { let address = format!("{host}:{port}"); address.to_socket_addrs().ok().and_then(|mut addresses| addresses.next()).is_some_and(|socket| TcpStream::connect_timeout(&socket, Duration::from_millis(timeout_ms)).is_ok()) }
fn wait_for_tcp(host: &str, port: u16, timeout: Duration, child: &mut Child) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if tcp_ready(host, port, 250) { return true; }
        if child.try_wait().ok().flatten().is_some() { return false; }
        thread::sleep(Duration::from_millis(100));
    }
    tcp_ready(host, port, 250)
}
fn mcp_initialize_probe(host: &str, port: u16, timeout_ms: u64) -> Option<u16> {
    let address = format!("{host}:{port}");
    let Some(socket) = address.to_socket_addrs().ok().and_then(|mut addresses| addresses.next()) else { return None; };
    let Ok(mut stream) = TcpStream::connect_timeout(&socket, Duration::from_millis(timeout_ms)) else { return None; };
    let timeout = Duration::from_millis(timeout_ms);
    let _ = stream.set_read_timeout(Some(timeout));
    let body = br#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"dcfc-readiness","version":"1"}}}"#;
    let request = format!(
        "POST /mcp HTTP/1.1\r\nHost: {host}:{port}\r\nAccept: application/json, text/event-stream\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    if stream.write_all(request.as_bytes()).is_err() || stream.write_all(body).is_err() { return None; }
    let mut response = Vec::with_capacity(512);
    let mut buffer = [0_u8; 256];
    while response.len() < 8 * 1024 {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                response.extend_from_slice(&buffer[..count]);
                if response.windows(4).any(|window| window == b"\r\n\r\n") { break; }
            }
            Err(_) => return None,
        }
    }
    parse_http_status(&response)
}
fn wait_for_mcp_http(host: &str, port: u16, timeout: Duration, child: &mut Child) -> (bool, Option<u16>) {
    let deadline = Instant::now() + timeout;
    let mut last_status = None;
    while Instant::now() < deadline {
        if child.try_wait().ok().flatten().is_some() { return (false, last_status); }
        if let Some(status) = mcp_initialize_probe(host, port, 500) {
            last_status = Some(status);
            if (200..300).contains(&status) { return (true, last_status); }
        }
        thread::sleep(Duration::from_millis(250));
    }
    (false, last_status.or_else(|| mcp_initialize_probe(host, port, 500)))
}
fn http_ready(host: &str, port: u16, timeout_ms: u64) -> bool {
    http_probe(host, port, "/readyz", timeout_ms).is_some_and(|code| (200..300).contains(&code))
        || http_probe(host, port, "/healthz", timeout_ms).is_some_and(|code| (200..300).contains(&code))
}

fn http_probe(host: &str, port: u16, path: &str, timeout_ms: u64) -> Option<u16> {
    let address = format!("{host}:{port}");
    let Some(socket) = address.to_socket_addrs().ok().and_then(|mut addresses| addresses.next()) else { return None; };
    let Ok(mut stream) = TcpStream::connect_timeout(&socket, Duration::from_millis(timeout_ms)) else { return None; };
    let timeout = Duration::from_millis(timeout_ms);
    let _ = stream.set_read_timeout(Some(timeout));
    let request = format!("GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: close\r\n\r\n");
    if stream.write_all(request.as_bytes()).is_err() { return None; }

    // A single read is not guaranteed to contain the complete status line on
    // Windows. Read through the end of the headers before parsing the code.
    let mut response = Vec::with_capacity(512);
    let mut buffer = [0_u8; 256];
    while response.len() < 8 * 1024 {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => {
                response.extend_from_slice(&buffer[..count]);
                if response.windows(4).any(|window| window == b"\r\n\r\n") { break; }
            }
            Err(_) => return None,
        }
    }
    parse_http_status(&response)
}

fn parse_http_status(response: &[u8]) -> Option<u16> {
    let Some(status_line) = response.split(|byte| *byte == b'\n').next() else { return None; };
    let status_line = status_line.strip_suffix(b"\r").unwrap_or(status_line);
    let mut parts = status_line.split(|byte| *byte == b' ');
    let _version = parts.next();
    let Some(code) = parts.next() else { return None; };
    std::str::from_utf8(code).ok().and_then(|value| value.parse::<u16>().ok())
}
enum WaitResult { Ready, Exited, TimedOut }

fn wait_for_log_marker(path: &Path, offset: u64, marker: &[u8], timeout: Duration, child: &mut Child) -> WaitResult {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if child.try_wait().ok().flatten().is_some() { return WaitResult::Exited; }
        if log_contains_marker(path, offset, marker) { return WaitResult::Ready; }
        thread::sleep(Duration::from_millis(250));
    }
    WaitResult::TimedOut
}
fn wait_for_http(host: &str, port: u16, timeout: Duration, child: &mut Child) -> bool { let deadline = Instant::now() + timeout; while Instant::now() < deadline { if child.try_wait().ok().flatten().is_some() { return false; } if http_ready(host, port, 500) { return true; } thread::sleep(Duration::from_millis(350)); } false }
fn read_tail(path: &Path, max_bytes: usize) -> Result<String, std::io::Error> { let bytes = fs::read(path)?; let start = bytes.len().saturating_sub(max_bytes); Ok(String::from_utf8_lossy(&bytes[start..]).to_string()) }
fn parse_text_editing_capability(output: &[u8]) -> Option<bool> { serde_json::from_slice::<serde_json::Value>(output).ok()?.get("text_editing").and_then(serde_json::Value::as_bool) }

#[cfg(test)]
mod tests {
    use super::*;

    fn sleeping_child() -> Child {
        Command::new("cmd.exe")
            .args(["/C", "ping -n 4 127.0.0.1 > nul"])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .expect("test child should start")
    }

    #[test]
    fn stopped_manager_has_no_pids() {
        let mut manager = ProcessManager::new(PathBuf::from("target/test-logs"));
        let status = manager.status(&AppSettings::default());
        assert_eq!(status.overall, OverallState::Stopped);
        assert!(status.mcp_pid.is_none());
        assert!(status.proxy_pid.is_none());
        assert!(status.tunnel_pid.is_none());
    }

    #[test]
    fn disabled_project_cannot_be_started() {
        let mut settings = AppSettings::default();
        settings.projects[0].enabled = false;
        let mut manager = ProcessManager::new(PathBuf::from("target/test-logs-disabled"));
        let error = manager.start_project(&settings, "default").expect_err("disabled project must be rejected");
        assert!(error.contains("尚未启用"));
        assert!(manager.projects.is_empty());
        assert!(manager.proxy.is_none());
        assert!(manager.tunnel.is_none());
    }

    #[test]
    fn http_success_accepts_all_2xx_statuses_and_split_headers() {
        assert_eq!(parse_http_status(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n"), Some(200));
        assert_eq!(parse_http_status(b"HTTP/1.1 204 No Content\r\n\r\n"), Some(204));
        assert_eq!(parse_http_status(b"HTTP/1.1 201 OK\r\n\r\n"), Some(201));
        assert_eq!(parse_http_status(b"HTTP/1.1 503 Service Unavailable\r\n\r\n"), Some(503));
        assert_eq!(parse_http_status(b"HTTP/1.1 OK\r\n\r\n"), None);
    }

    #[test]
    fn startup_diagnostic_writes_paths_and_http_category_without_secrets() {
        let suffix = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("dcfc-startup-diagnostic-{suffix}"));
        let mut diagnostic = StartupDiagnostic::new("router_mcp_http_readiness", "failed", "Router MCP initialize 预检未返回 2xx。");
        diagnostic.config_path = Some("C:\\AppData\\Delegate Control\\router-projects.json".to_string());
        diagnostic.endpoint = Some("http://127.0.0.1:8101/mcp".to_string());
        diagnostic.http_status = Some(502);
        diagnostic.http_status_category = Some(http_status_category(Some(502)));
        record_startup_diagnostic(&root, diagnostic);

        let line = fs::read_to_string(root.join("startup-diagnostics.jsonl")).unwrap();
        let value: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(value["stage"], "router_mcp_http_readiness");
        assert_eq!(value["http_status"], 502);
        assert_eq!(value["http_status_category"], "5xx_server_or_intermediary");
        assert!(!line.contains("CONTROL_PLANE_API_KEY"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn http_status_categories_distinguish_missing_response_and_5xx() {
        assert_eq!(http_status_category(None), "no_http_response");
        assert_eq!(http_status_category(Some(200)), "2xx_ready");
        assert_eq!(http_status_category(Some(502)), "5xx_server_or_intermediary");
        assert_eq!(http_status_category(Some(404)), "4xx_client_or_protocol");
    }

    #[test]
    fn readiness_marker_only_matches_content_written_after_launch_offset() {
        let suffix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("dcfc-readiness-{suffix}.log"));
        fs::write(&path, b"Application startup complete.\n").unwrap();
        let offset = log_offset(&path);

        assert!(!log_contains_marker(&path, offset, b"Application startup complete."));
        OpenOptions::new().append(true).open(&path).unwrap().write_all(b"Application startup complete.\n").unwrap();
        assert!(log_contains_marker(&path, offset, b"Application startup complete."));

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn proxy_readiness_marker_accepts_ansi_styled_fields() {
        let suffix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let path = std::env::temp_dir().join(format!("dcfc-proxy-readiness-{suffix}.log"));
        fs::write(&path, b"Proxy ready \x1b[3mlisten\x1b[0m\x1b[2m=\x1b[0m127.0.0.1:8100\n").unwrap();

        assert!(log_contains_marker(&path, 0, b"Proxy ready "));
        assert!(!log_contains_marker(&path, 0, b"Proxy ready listen="));

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn tcp_readiness_wait_stops_when_router_exits_before_binding() {
        let mut child = Command::new("cmd.exe")
            .args(["/C", "exit 1"])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .expect("test child should start");
        assert!(!wait_for_tcp("127.0.0.1", 59999, Duration::from_secs(2), &mut child));
    }

    #[test]
    fn parses_connector_text_editing_capability_without_exposing_payload() {
        assert_eq!(parse_text_editing_capability(br#"{"text_editing":true}"#), Some(true));
        assert_eq!(parse_text_editing_capability(br#"{"text_editing":false}"#), Some(false));
        assert_eq!(parse_text_editing_capability(br#"{"status":"ok"}"#), None);
        assert_eq!(parse_text_editing_capability(b"not-json"), None);
    }

    #[test]
    fn tunnel_id_is_passed_as_runtime_identity_separately_from_profile() {
        let mut settings = AppSettings::default();
        settings.profile_name = String::new();
        settings.tunnel_id = "tunnel_demo_01".to_string();
        assert_eq!(tunnel_identity_args(&settings), vec!["run", "--control-plane.tunnel-id", "tunnel_demo_01"]);

        settings.profile_name = "legacy-profile".to_string();
        assert_eq!(tunnel_identity_args(&settings), vec![
            "run", "--profile", "legacy-profile", "--control-plane.tunnel-id", "tunnel_demo_01",
        ]);
    }

    #[test]
    fn stop_one_preserves_owned_shared_chain_and_updates_registry() {
        let suffix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("dcfc-stop-one-{suffix}"));
        fs::create_dir_all(&root).unwrap();
        let mut settings = AppSettings::default();
        settings.router_config_path = root.join("router-projects.json").to_string_lossy().to_string();
        settings.projects[0].id = "screencast".to_string();
        settings.projects[0].name = "ScreenCast".to_string();
        settings.projects[0].output_directory = root.join("screencast").to_string_lossy().to_string();
        settings.projects[0].mcp_port = 42001;
        let mut second = settings.projects[0].clone();
        second.id = "dcfc".to_string();
        second.name = "DCFC".to_string();
        second.output_directory = root.join("dcfc").to_string_lossy().to_string();
        second.mcp_port = 42002;
        settings.projects.push(second);
        settings.active_project_id = Some("dcfc".to_string());

        let mut manager = ProcessManager::new(root.join("logs"));
        manager.projects.insert("screencast".to_string(), ManagedProject { child: sleeping_child() });
        manager.projects.insert("dcfc".to_string(), ManagedProject { child: sleeping_child() });
        manager.router = Some(sleeping_child());
        manager.proxy = Some(sleeping_child());
        manager.tunnel = Some(sleeping_child());
        let shared_pids = (
            manager.router.as_ref().unwrap().id(),
            manager.proxy.as_ref().unwrap().id(),
            manager.tunnel.as_ref().unwrap().id(),
        );

        manager.stop_project(&settings, "screencast").unwrap();

        assert!(!manager.projects.contains_key("screencast"));
        assert!(manager.projects.contains_key("dcfc"));
        assert_eq!(manager.router.as_ref().unwrap().id(), shared_pids.0);
        assert_eq!(manager.proxy.as_ref().unwrap().id(), shared_pids.1);
        assert_eq!(manager.tunnel.as_ref().unwrap().id(), shared_pids.2);
        let registry = fs::read_to_string(&settings.router_config_path).unwrap();
        assert!(!registry.contains("screencast"));
        assert!(registry.contains("dcfc"));

        manager.stop_shared();
        manager.remove_project_process("dcfc");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unowned_router_listener_is_still_rejected_as_foreign() {
        let clash = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let mut settings = AppSettings::default();
        settings.proxy_port = clash.local_addr().unwrap().port();
        settings.router_port = listener.local_addr().unwrap().port();
        let mut manager = ProcessManager::new(PathBuf::from("target/test-logs-foreign-router"));

        let error = manager.start_shared(&settings, "unused-test-key").unwrap_err();

        assert!(error.contains("Router 端口"));
        assert!(error.contains("已被其他程序占用"));
        assert!(manager.router.is_none());
    }

    #[test]
    fn shared_start_rejects_unavailable_clash_before_launching_children() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let mut settings = AppSettings::default();
        settings.proxy_port = port;
        let mut manager = ProcessManager::new(PathBuf::from("target/test-logs-clash-preflight"));

        let error = manager.start_shared(&settings, "unused-test-key").unwrap_err();

        assert!(error.contains("Magic 代理未就绪"));
        assert!(manager.router.is_none());
        assert!(manager.proxy.is_none());
        assert!(manager.tunnel.is_none());
    }

    #[test]
    fn direct_mode_skips_magic_preflight() {
        let mut settings = AppSettings::default();
        settings.network_mode = NetworkMode::Direct;
        settings.proxy_host = "invalid-host-for-direct-mode".to_string();
        settings.proxy_port = 0;
        assert_eq!(resolve_clash_port(&settings).unwrap(), None);

        let mut magic = settings.clone();
        magic.network_mode = NetworkMode::Magic;
        let error = resolve_clash_port(&magic).unwrap_err();
        assert!(error.contains("Magic 代理未就绪"));
    }

    #[test]
    fn clash_preflight_uses_legacy_listener_when_configured_port_is_stale() {
        let configured = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let configured_port = configured.local_addr().unwrap().port();
        drop(configured);
        let legacy = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let legacy_port = legacy.local_addr().unwrap().port();
        let mut settings = AppSettings::default();
        settings.proxy_port = configured_port;
        assert_eq!(detect_clash_port_with_legacy(&settings, legacy_port).unwrap(), legacy_port);
    }

    #[test]
    fn clash_preflight_migrates_from_legacy_7897_to_current_7877_listener() {
        let current = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let current_port = current.local_addr().unwrap().port();
        let mut settings = AppSettings::default();
        settings.proxy_port = 7897;
        assert_eq!(detect_clash_port_with_legacy(&settings, current_port).unwrap(), current_port);
    }

    #[test]
    fn clash_preflight_error_names_configured_and_legacy_ports() {
        let first = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let configured_port = first.local_addr().unwrap().port();
        drop(first);
        let second = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let legacy_port = second.local_addr().unwrap().port();
        drop(second);
        let mut settings = AppSettings::default();
        settings.proxy_port = configured_port;
        let error = detect_clash_port_with_legacy(&settings, legacy_port).unwrap_err();
        assert!(error.contains(&configured_port.to_string()));
        assert!(error.contains(&legacy_port.to_string()));
    }

    #[test]
    fn magic_port_probe_reports_configured_listener_without_claiming_network_readiness() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let mut settings = AppSettings::default();
        settings.proxy_port = port;

        let probe = ProcessManager::detect_magic_port(&settings);

        assert!(probe.listening);
        assert_eq!(probe.detected_port, Some(port));
        assert!(probe.detail.contains("不代表代理出网"));
    }

    #[test]
    fn magic_port_probe_reports_missing_listener_and_does_not_mutate_settings() {
        let configured = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let configured_port = configured.local_addr().unwrap().port();
        drop(configured);
        let mut settings = AppSettings::default();
        settings.proxy_port = configured_port;

        let probe = ProcessManager::detect_magic_port(&settings);

        assert!(!probe.listening);
        assert_eq!(probe.detected_port, None);
        assert_eq!(settings.proxy_port, configured_port);
    }

    fn connector_call(python: &str, project_id: &str) {
        let code = r#"
import asyncio
import sys
import httpx
from mcp import ClientSession
from mcp.client.streamable_http import streamable_http_client

async def main():
    async with httpx.AsyncClient(trust_env=False) as client:
        async with streamable_http_client('http://127.0.0.1:8100', http_client=client) as (read, write, _):
            async with ClientSession(read, write) as session:
                await session.initialize()
                result = await session.call_tool('connector_status', {'project_id': sys.argv[1]})
                if result.isError or result.structuredContent.get('status') != 'ready':
                    raise RuntimeError(result)

asyncio.run(main())
"#;
        let output = Command::new(python).args(["-c", code, project_id]).creation_flags(CREATE_NO_WINDOW).output().expect("MCP client should run");
        assert!(output.status.success(), "connector_status failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    fn connector_sha_append(python: &str) {
        let code = r#"
import asyncio
import httpx
from mcp import ClientSession
from mcp.client.streamable_http import streamable_http_client

async def main():
    async with httpx.AsyncClient(trust_env=False) as client:
        async with streamable_http_client('http://127.0.0.1:8100', http_client=client) as (read, write, _):
            async with ClientSession(read, write) as session:
                await session.initialize()
                before = await session.call_tool('read_text_file', {
                    'filename': 'PRODUCT_DESIGNER_DEVELOPER_HANDOFF.md',
                    'project_id': 'dcfc',
                })
                if before.isError:
                    raise RuntimeError(before)
                result = await session.call_tool('append_text_file', {
                    'filename': 'PRODUCT_DESIGNER_DEVELOPER_HANDOFF.md',
                    'content': '<!-- PDH-20260927-009 real Connector SHA-256 append verification passed. -->',
                    'expected_sha256': before.structuredContent['sha256'],
                    'project_id': 'dcfc',
                })
                if result.isError or result.structuredContent.get('operation') != 'append':
                    raise RuntimeError(result)

asyncio.run(main())
"#;
        let output = Command::new(python).args(["-c", code]).creation_flags(CREATE_NO_WINDOW).output().expect("MCP append client should run");
        assert!(output.status.success(), "SHA-256 append failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    #[test]
    #[ignore = "requires the installed DCFC runtime, Magic proxy, Credential Manager key, MCP Proxy, and Tunnel"]
    fn live_multi_project_lifecycle_matrix() {
        let settings_path = PathBuf::from(std::env::var("DCFC_SETTINGS_PATH").expect("DCFC_SETTINGS_PATH is required"));
        let python = std::env::var("DCFC_MCP_CLIENT_PYTHON").expect("DCFC_MCP_CLIENT_PYTHON is required");
        let settings = crate::config::load_application_settings(&settings_path);
        let settings_before = fs::read(&settings_path).expect("migrated settings should be readable");
        let identities = settings.projects.iter().map(|project| (project.name.as_str(), project.id.as_str())).collect::<Vec<_>>();
        assert_eq!(identities, vec![("ScreenCast", "screencast"), ("Realize", "realize"), ("DCFC", "dcfc")]);
        assert_eq!(settings.active_project_id.as_deref(), Some("dcfc"));

        let mut manager = ProcessManager::new(settings_path.parent().unwrap().join("logs"));
        let started = manager.start_all(&settings).expect("Start All should succeed");
        assert_eq!(started.overall, OverallState::Running);
        connector_call(&python, "screencast");
        connector_call(&python, "realize");
        connector_call(&python, "dcfc");
        let shared_pids = (started.router_pid, started.proxy_pid, started.tunnel_pid);

        for project_id in ["screencast", "realize", "dcfc"] {
            let stopped = manager.stop_project(&settings, project_id).expect("Stop One should succeed");
            assert_eq!((stopped.router_pid, stopped.proxy_pid, stopped.tunnel_pid), shared_pids);
            for remaining in ["screencast", "realize", "dcfc"].into_iter().filter(|id| *id != project_id) {
                connector_call(&python, remaining);
            }
            let restarted = manager.start_project(&settings, project_id).expect("Start One should succeed");
            assert_eq!((restarted.router_pid, restarted.proxy_pid, restarted.tunnel_pid), shared_pids);
            connector_call(&python, project_id);
        }

        let stopped_all = manager.stop_all(&settings);
        assert_eq!(stopped_all.overall, OverallState::Stopped);

        let started_one = manager.start_project(&settings, "dcfc").expect("Start One from fully stopped state should succeed");
        assert_eq!(started_one.overall, OverallState::Running);
        connector_call(&python, "dcfc");
        manager.stop_all(&settings);

        let restarted_all = manager.start_all(&settings).expect("second Start All should succeed");
        assert_eq!(restarted_all.overall, OverallState::Running);
        connector_call(&python, "dcfc");
        connector_sha_append(&python);
        manager.stop_all(&settings);

        let settings_after = fs::read(&settings_path).expect("settings should remain readable");
        assert_eq!(settings_after, settings_before, "lifecycle operations must not mutate settings.json");
    }

    #[test]
    #[ignore = "requires installed Delegate tools, mcp-proxy, Magic proxy, and a Runtime API Key"]
    fn live_start_is_idempotent_and_stop_cleans_up() {
        let settings = AppSettings::default();
        let mut manager = ProcessManager::new(PathBuf::from("target/live-test-logs"));
        let first = manager.start(&settings).expect("live start should succeed");
        assert_eq!(first.overall, OverallState::Running);
        assert!(first.mcp_ready && first.mcp_proxy_ready && first.tunnel_ready);
        let first_pids = (first.mcp_pid, first.proxy_pid, first.tunnel_pid);
        let second = manager.start(&settings).expect("second start should be idempotent");
        assert_eq!((second.mcp_pid, second.proxy_pid, second.tunnel_pid), first_pids);
        let stopped = manager.stop(&settings);
        assert_eq!(stopped.overall, OverallState::Stopped);
        assert!(!stopped.mcp_ready && !stopped.mcp_proxy_ready && !stopped.tunnel_ready);
    }
}
