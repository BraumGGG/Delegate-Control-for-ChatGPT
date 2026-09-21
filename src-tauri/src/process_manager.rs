use crate::{config::AppSettings, credentials};
use serde::Serialize;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    net::{TcpStream, ToSocketAddrs},
    os::windows::{io::AsRawHandle, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE},
    System::{
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
            SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        },
        Threading::CREATE_NO_WINDOW,
    },
};

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OverallState {
    Stopped,
    Starting,
    Running,
    Stopping,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct DelegateStatus {
    pub overall: OverallState,
    pub proxy_ready: bool,
    pub mcp_ready: bool,
    pub tunnel_ready: bool,
    pub mcp_pid: Option<u32>,
    pub tunnel_pid: Option<u32>,
    pub credential_configured: bool,
    pub message: String,
}

pub struct ProcessManager {
    mcp: Option<Child>,
    tunnel: Option<Child>,
    job: Option<HANDLE>,
    overall: OverallState,
    message: String,
    log_dir: PathBuf,
}

unsafe impl Send for ProcessManager {}

impl ProcessManager {
    pub fn new(log_dir: PathBuf) -> Self {
        Self {
            mcp: None,
            tunnel: None,
            job: None,
            overall: OverallState::Stopped,
            message: "连接仅在需要时启动。".to_string(),
            log_dir,
        }
    }

    pub fn status(&mut self, settings: &AppSettings) -> DelegateStatus {
        self.refresh_process_state();
        let proxy_ready = tcp_ready(&settings.proxy_host, settings.proxy_port, 350);
        let mcp_ready = tcp_ready(&settings.mcp_host, settings.mcp_port, 350);
        let tunnel_ready = http_ready(&settings.mcp_host, settings.health_port, 500);
        DelegateStatus {
            overall: self.overall,
            proxy_ready,
            mcp_ready,
            tunnel_ready,
            mcp_pid: self.mcp.as_ref().map(Child::id),
            tunnel_pid: self.tunnel.as_ref().map(Child::id),
            credential_configured: credentials::credential_exists(),
            message: self.message.clone(),
        }
    }

    pub fn start(&mut self, settings: &AppSettings) -> Result<DelegateStatus, String> {
        self.refresh_process_state();
        if self.overall == OverallState::Running {
            return Ok(self.status(settings));
        }
        self.stop_internal();
        settings.validate()?;

        let key = credentials::read_runtime_key()?;
        require_file(&settings.mcp_executable, "MCP 程序")?;
        require_file(&settings.tunnel_executable, "Tunnel 程序")?;
        fs::create_dir_all(&settings.output_directory).map_err(|error| format!("无法创建结果目录：{error}"))?;
        fs::create_dir_all(&self.log_dir).map_err(|error| format!("无法创建日志目录：{error}"))?;

        if !tcp_ready(&settings.proxy_host, settings.proxy_port, 700) {
            return self.fail("Clash 代理未就绪，请先启动 Clash。".to_string());
        }
        if tcp_ready(&settings.mcp_host, settings.mcp_port, 250) {
            return self.fail(format!("端口 {} 已被其他程序占用。", settings.mcp_port));
        }
        if tcp_ready(&settings.mcp_host, settings.health_port, 250) {
            return self.fail(format!("端口 {} 已被其他程序占用。", settings.health_port));
        }

        let mcp_stdout = log_file(&self.log_dir.join("mcp.stdout.log"))?;
        let mcp_stderr = log_file(&self.log_dir.join("mcp.stderr.log"))?;
        let tunnel_stdout = log_file(&self.log_dir.join("tunnel.stdout.log"))?;
        let tunnel_stderr = log_file(&self.log_dir.join("tunnel.stderr.log"))?;

        self.overall = OverallState::Starting;
        self.message = "正在启动本地 MCP Server。".to_string();
        let job = create_job()?;
        self.job = Some(job);

        let mut mcp_command = Command::new(&settings.mcp_executable);
        mcp_command
            .args([
                "--output-dir",
                &settings.output_directory,
                "serve",
                "--host",
                &settings.mcp_host,
                "--port",
                &settings.mcp_port.to_string(),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::from(mcp_stdout))
            .stderr(Stdio::from(mcp_stderr))
            .creation_flags(CREATE_NO_WINDOW);
        let mut mcp = match mcp_command.spawn() {
            Ok(child) => child,
            Err(error) => {
                self.stop_internal();
                return self.fail(format!("无法启动 MCP：{error}"));
            }
        };
        if let Err(error) = assign_to_job(job, &mcp) {
            self.mcp = Some(mcp);
            self.stop_internal();
            return self.fail(error);
        }

        if !wait_for_tcp(&settings.mcp_host, settings.mcp_port, Duration::from_secs(15), &mut mcp) {
            self.mcp = Some(mcp);
            self.stop_internal();
            return self.fail("MCP 未能在 15 秒内就绪，请查看日志。".to_string());
        }
        self.mcp = Some(mcp);
        self.message = "本地 MCP 已就绪，正在建立安全 Tunnel。".to_string();

        let tunnel_log = self.log_dir.join("tunnel.log");
        let mut tunnel_command = Command::new(&settings.tunnel_executable);
        tunnel_command
            .args([
                "run",
                "--profile",
                &settings.profile_name,
                "--control-plane.http-proxy",
                &format!("http://{}:{}", settings.proxy_host, settings.proxy_port),
                "--open-web-ui=false",
                "--log.file",
                &tunnel_log.to_string_lossy(),
            ])
            .env("CONTROL_PLANE_API_KEY", key)
            .stdin(Stdio::null())
            .stdout(Stdio::from(tunnel_stdout))
            .stderr(Stdio::from(tunnel_stderr))
            .creation_flags(CREATE_NO_WINDOW);
        let mut tunnel = match tunnel_command.spawn() {
            Ok(child) => child,
            Err(error) => {
                self.stop_internal();
                return self.fail(format!("无法启动 Tunnel：{error}"));
            }
        };
        if let Err(error) = assign_to_job(job, &tunnel) {
            self.tunnel = Some(tunnel);
            self.stop_internal();
            return self.fail(error);
        }

        if !wait_for_http(&settings.mcp_host, settings.health_port, Duration::from_secs(30), &mut tunnel) {
            self.tunnel = Some(tunnel);
            self.stop_internal();
            return self.fail("Tunnel 未能在 30 秒内就绪，请检查密钥和 Tunnel 日志。".to_string());
        }
        self.tunnel = Some(tunnel);
        self.overall = OverallState::Running;
        self.message = "ChatGPT Delegate 连接已建立。".to_string();
        Ok(self.status(settings))
    }

    pub fn stop(&mut self, settings: &AppSettings) -> DelegateStatus {
        self.overall = OverallState::Stopping;
        self.message = "正在安全停止连接。".to_string();
        self.stop_internal();
        self.overall = OverallState::Stopped;
        self.message = "连接已停止，没有后台进程残留。".to_string();
        self.status(settings)
    }

    pub fn read_logs(&self, source: &str) -> Result<String, String> {
        let names: &[&str] = match source {
            "mcp" => &["mcp.stderr.log", "mcp.stdout.log"],
            "tunnel" => &["tunnel.stderr.log", "tunnel.log", "tunnel.stdout.log"],
            _ => return Err("未知日志类型。".to_string()),
        };
        let mut combined = String::new();
        for name in names {
            let path = self.log_dir.join(name);
            if let Ok(content) = read_tail(&path, 80_000) {
                if !content.trim().is_empty() {
                    combined.push_str(&format!("===== {name} =====\n{content}\n"));
                }
            }
        }
        if combined.is_empty() {
            Ok("暂无日志。启动连接后将在这里显示运行信息。".to_string())
        } else {
            Ok(combined)
        }
    }

    pub fn log_dir(&self) -> &Path {
        &self.log_dir
    }

    fn refresh_process_state(&mut self) {
        if matches!(self.overall, OverallState::Running | OverallState::Starting) {
            let mcp_dead = self.mcp.as_mut().and_then(|child| child.try_wait().ok()).flatten().is_some();
            let tunnel_dead = self.tunnel.as_mut().and_then(|child| child.try_wait().ok()).flatten().is_some();
            if mcp_dead || tunnel_dead {
                self.stop_internal();
                self.overall = OverallState::Failed;
                self.message = if mcp_dead { "MCP 进程意外退出，请查看日志。" } else { "Tunnel 进程意外退出，请查看日志。" }.to_string();
            }
        }
    }

    fn stop_internal(&mut self) {
        if let Some(job) = self.job.take() {
            unsafe {
                TerminateJobObject(job, 0);
                CloseHandle(job);
            }
        }
        if let Some(mut child) = self.tunnel.take() {
            let _ = child.wait();
        }
        if let Some(mut child) = self.mcp.take() {
            let _ = child.wait();
        }
    }

    fn fail<T>(&mut self, message: String) -> Result<T, String> {
        self.overall = OverallState::Failed;
        self.message = message.clone();
        Err(message)
    }
}

impl Drop for ProcessManager {
    fn drop(&mut self) {
        self.stop_internal();
    }
}

fn create_job() -> Result<HANDLE, String> {
    let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    if job.is_null() {
        return Err(format!("无法创建 Windows Job Object：{}", std::io::Error::last_os_error()));
    }
    let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
    info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    let ok = unsafe {
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const _,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    };
    if ok == 0 {
        unsafe { CloseHandle(job) };
        return Err(format!("无法配置 Windows Job Object：{}", std::io::Error::last_os_error()));
    }
    Ok(job)
}

fn assign_to_job(job: HANDLE, child: &Child) -> Result<(), String> {
    let process = child.as_raw_handle() as HANDLE;
    if unsafe { AssignProcessToJobObject(job, process) } == 0 {
        return Err(format!("无法将进程加入 Job Object：{}", std::io::Error::last_os_error()));
    }
    Ok(())
}

fn require_file(path: &str, label: &str) -> Result<(), String> {
    if Path::new(path).is_file() { Ok(()) } else { Err(format!("找不到{label}：{path}")) }
}

fn log_file(path: &Path) -> Result<File, String> {
    OpenOptions::new().create(true).append(true).open(path).map_err(|error| format!("无法打开日志 {}：{error}", path.display()))
}

fn tcp_ready(host: &str, port: u16, timeout_ms: u64) -> bool {
    let address = format!("{host}:{port}");
    address.to_socket_addrs().ok().and_then(|mut addresses| addresses.next()).is_some_and(|socket| TcpStream::connect_timeout(&socket, Duration::from_millis(timeout_ms)).is_ok())
}

fn http_ready(host: &str, port: u16, timeout_ms: u64) -> bool {
    let address = format!("{host}:{port}");
    let Some(socket) = address.to_socket_addrs().ok().and_then(|mut addresses| addresses.next()) else { return false; };
    let Ok(mut stream) = TcpStream::connect_timeout(&socket, Duration::from_millis(timeout_ms)) else { return false; };
    let _ = stream.set_read_timeout(Some(Duration::from_millis(timeout_ms)));
    let request = format!("GET /readyz HTTP/1.1\r\nHost: {host}:{port}\r\nConnection: close\r\n\r\n");
    if stream.write_all(request.as_bytes()).is_err() { return false; }
    let mut buffer = [0_u8; 64];
    stream.read(&mut buffer).is_ok_and(|count| String::from_utf8_lossy(&buffer[..count]).contains(" 200 "))
}

fn wait_for_tcp(host: &str, port: u16, timeout: Duration, child: &mut Child) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if child.try_wait().ok().flatten().is_some() { return false; }
        if tcp_ready(host, port, 300) { return true; }
        thread::sleep(Duration::from_millis(250));
    }
    false
}

fn wait_for_http(host: &str, port: u16, timeout: Duration, child: &mut Child) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if child.try_wait().ok().flatten().is_some() { return false; }
        if http_ready(host, port, 500) { return true; }
        thread::sleep(Duration::from_millis(350));
    }
    false
}

fn read_tail(path: &Path, max_bytes: usize) -> Result<String, std::io::Error> {
    let bytes = fs::read(path)?;
    let start = bytes.len().saturating_sub(max_bytes);
    Ok(String::from_utf8_lossy(&bytes[start..]).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stopped_manager_has_no_pids() {
        let mut manager = ProcessManager::new(PathBuf::from("target/test-logs"));
        let status = manager.status(&AppSettings::default());
        assert_eq!(status.overall, OverallState::Stopped);
        assert!(status.mcp_pid.is_none());
        assert!(status.tunnel_pid.is_none());
    }

    #[test]
    #[ignore = "requires installed Delegate tools, Clash, and a Runtime API Key"]
    fn live_start_is_idempotent_and_stop_cleans_up() {
        let settings = AppSettings::default();
        let mut manager = ProcessManager::new(PathBuf::from("target/live-test-logs"));

        let first = manager.start(&settings).expect("live start should succeed");
        assert_eq!(first.overall, OverallState::Running);
        assert!(first.mcp_ready && first.tunnel_ready);
        let first_pids = (first.mcp_pid, first.tunnel_pid);

        let second = manager.start(&settings).expect("second start should be idempotent");
        assert_eq!((second.mcp_pid, second.tunnel_pid), first_pids);

        let stopped = manager.stop(&settings);
        assert_eq!(stopped.overall, OverallState::Stopped);
        assert!(!stopped.mcp_ready && !stopped.tunnel_ready);
    }
}
