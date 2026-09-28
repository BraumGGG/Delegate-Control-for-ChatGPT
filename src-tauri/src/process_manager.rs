use crate::{config::{AppSettings, ProjectConfig}, credentials, proxy_config::{build_proxy_toml, build_router_registry, write_proxy_config}};
use serde::Serialize;
use std::{collections::HashMap, fs::{self, File, OpenOptions}, io::{Read, Write}, net::{TcpStream, ToSocketAddrs}, os::windows::{io::AsRawHandle, process::CommandExt}, path::{Path, PathBuf}, process::{Child, Command, Stdio}, thread, time::{Duration, Instant}};
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

struct ManagedProject { child: Child }

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
}

unsafe impl Send for ProcessManager {}

impl ProcessManager {
    pub fn new(log_dir: PathBuf) -> Self {
        Self { projects: HashMap::new(), project_failures: HashMap::new(), proxy: None, router: None, tunnel: None, job: None, overall: OverallState::Stopped, message: "连接仅在需要时启动。".to_string(), log_dir, connector_capability_path: None, text_editing_available: false, connector_capability_message: "尚未检测 Connector 文件编辑能力。".to_string() }
    }

    pub fn status(&mut self, settings: &AppSettings) -> DelegateStatus {
        self.refresh_process_state();
        self.refresh_connector_capability(&settings.mcp_executable);
        let proxy_ready = detect_clash_port(settings).is_ok();
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
        let clash_port = detect_clash_port(settings)?;
        if tcp_ready(&settings.mcp_proxy_host, settings.router_port, 250) { return Err(format!("Router 端口 {} 已被其他程序占用。", settings.router_port)); }
        if tcp_ready(&settings.mcp_proxy_host, settings.mcp_proxy_port, 250) { return Err(format!("MCP Proxy 端口 {} 已被其他程序占用。", settings.mcp_proxy_port)); }
        if tcp_ready(&settings.health_host, settings.health_port, 250) { return Err(format!("健康端口 {} 已被其他程序占用。", settings.health_port)); }
        let projects = settings.projects.iter().filter(|project| self.projects.contains_key(&project.id)).cloned().collect::<Vec<_>>();
        self.write_router_registry(settings, &projects)?;
        let toml = build_proxy_toml(settings, &projects)?;
        write_proxy_config(Path::new(&settings.proxy_config_path), &toml)?;
        fs::create_dir_all(&self.log_dir).map_err(|error| format!("无法创建日志目录：{error}"))?;
        let router_stderr_path = self.log_dir.join("router.stderr.log");
        let router_stderr_offset = log_offset(&router_stderr_path);
        let router_stdout = log_file(&self.log_dir.join("router.stdout.log"))?;
        let router_stderr = log_file(&router_stderr_path)?;
        let mut router_command = Command::new(&settings.mcp_executable);
        router_command.args(["router", "--config", &settings.router_config_path, "--host", &settings.mcp_proxy_host, "--port", &settings.router_port.to_string()]).stdin(Stdio::null()).stdout(Stdio::from(router_stdout)).stderr(Stdio::from(router_stderr)).creation_flags(CREATE_NO_WINDOW);
        let mut router = router_command.spawn().map_err(|error| format!("无法启动 Router MCP：{error}"))?;
        if let Err(error) = assign_to_job(self.job.ok_or_else(|| "Windows Job Object 尚未创建。".to_string())?, &router) {
            terminate_child(&mut router);
            return Err(error);
        }
        let wait_result = wait_for_log_marker(&router_stderr_path, router_stderr_offset, b"Application startup complete.", Duration::from_secs(15), &mut router);
        if !matches!(wait_result, WaitResult::Ready) {
            terminate_child(&mut router);
            let reason = match wait_result { WaitResult::Exited => "进程已退出", WaitResult::TimedOut => "15 秒内未出现 Router 启动完成标记", WaitResult::Ready => unreachable!() };
            return Err(format!("Router MCP 未能就绪：{}。请查看 Router 日志。当前程序：{}", reason, settings.mcp_executable));
        }
        self.router = Some(router);
        let proxy_stderr_path = self.log_dir.join("proxy.stderr.log");
        let proxy_stderr_offset = log_offset(&proxy_stderr_path);
        let proxy_stdout = log_file(&self.log_dir.join("proxy.stdout.log"))?;
        let proxy_stderr = log_file(&proxy_stderr_path)?;
        let mut proxy_command = Command::new(&settings.proxy_executable);
        proxy_command.args(["--config", &settings.proxy_config_path]).stdin(Stdio::null()).stdout(Stdio::from(proxy_stdout)).stderr(Stdio::from(proxy_stderr)).creation_flags(CREATE_NO_WINDOW);
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
            return Err(format!("MCP Proxy 未能就绪：{}。请查看 proxy 日志。当前程序：{}", reason, settings.proxy_executable));
        }
        self.proxy = Some(proxy);

        let tunnel_log = self.log_dir.join("tunnel.log");
        let tunnel_stdout = log_file(&self.log_dir.join("tunnel.stdout.log"))?;
        let tunnel_stderr = log_file(&self.log_dir.join("tunnel.stderr.log"))?;
        let mut tunnel_command = Command::new(&settings.tunnel_executable);
        // mcp-proxy exposes its Streamable HTTP router at the root path. The
        // project backends behind it still use /mcp, but the shared Tunnel
        // must target the proxy root so initialize requests are not 404.
        tunnel_command.args(["run", "--profile", &settings.profile_name, "--control-plane.http-proxy", &format!("http://{}:{}", settings.proxy_host, clash_port), "--mcp.server-url", &format!("url=http://{}:{},channel=main", settings.mcp_proxy_host, settings.mcp_proxy_port), "--open-web-ui=false", "--log.file", &tunnel_log.to_string_lossy()]).env("CONTROL_PLANE_API_KEY", key).stdin(Stdio::null()).stdout(Stdio::from(tunnel_stdout)).stderr(Stdio::from(tunnel_stderr)).creation_flags(CREATE_NO_WINDOW);
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
fn log_file(path: &Path) -> Result<File, String> { if let Some(parent) = path.parent() { fs::create_dir_all(parent).map_err(|error| format!("无法创建日志目录：{error}"))?; } OpenOptions::new().create(true).append(true).open(path).map_err(|error| format!("无法打开日志 {}：{error}", path.display())) }
fn log_offset(path: &Path) -> u64 { fs::metadata(path).map(|metadata| metadata.len()).unwrap_or(0) }
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
    fn parses_connector_text_editing_capability_without_exposing_payload() {
        assert_eq!(parse_text_editing_capability(br#"{"text_editing":true}"#), Some(true));
        assert_eq!(parse_text_editing_capability(br#"{"text_editing":false}"#), Some(false));
        assert_eq!(parse_text_editing_capability(br#"{"status":"ok"}"#), None);
        assert_eq!(parse_text_editing_capability(b"not-json"), None);
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
