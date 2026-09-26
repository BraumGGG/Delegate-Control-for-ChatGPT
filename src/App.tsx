import { useCallback, useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { Activity, Check, ChevronRight, CircleAlert, Copy, Eye, EyeOff, FolderOpen, KeyRound, Link2, LoaderCircle, MonitorCog, Network, Play, Plus, Power, RefreshCw, RotateCw, Save, Server, Settings2, ShieldCheck, Square, SquareTerminal, Trash2, Unplug } from "lucide-react";
import { api } from "./api";
import type { AppSettings, DelegateStatus, LogSource, ProjectConfig, ViewId } from "./types";

const EMPTY_STATUS: DelegateStatus = {
  overall: "stopped", proxy_ready: false, router_ready: false, mcp_proxy_ready: false, mcp_ready: false, tunnel_ready: false, proxy_pid: null, router_pid: null, mcp_pid: null, tunnel_pid: null, credential_configured: false, text_editing_available: false, connector_capability_message: "正在检测 Connector 文件编辑能力", message: "正在读取本机状态", projects: [],
};

const NAV_ITEMS = [
  { id: "overview" as const, label: "总览", icon: Activity },
  { id: "logs" as const, label: "日志", icon: SquareTerminal },
  { id: "settings" as const, label: "设置", icon: Settings2 },
];

function stateCopy(status: DelegateStatus) {
  if (status.overall === "running") return { eyebrow: "LINK ESTABLISHED", title: "连接已建立", tone: "online" };
  if (status.overall === "starting") return { eyebrow: "ESTABLISHING LINK", title: "正在建立连接", tone: "working" };
  if (status.overall === "stopping") return { eyebrow: "CLOSING LINK", title: "正在安全停止", tone: "working" };
  if (status.overall === "failed") return { eyebrow: "ATTENTION REQUIRED", title: "连接需要处理", tone: "error" };
  return { eyebrow: "READY ON DEMAND", title: "等待启动", tone: "idle" };
}

function StatusDot({ ready, working = false }: { ready: boolean; working?: boolean }) {
  return <span className={`status-dot ${ready ? "is-ready" : working ? "is-working" : ""}`} aria-hidden />;
}

function App() {
  const [view, setView] = useState<ViewId>("overview");
  const [status, setStatus] = useState<DelegateStatus>(EMPTY_STATUS);
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [savedSettings, setSavedSettings] = useState<AppSettings | null>(null);
  const [busy, setBusy] = useState(false);
  const [projectBusy, setProjectBusy] = useState<string | null>(null);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [selectedProjectId, setSelectedProjectId] = useState<string | null>(null);
  const [logSource, setLogSource] = useState<LogSource>("mcp");
  const [logProjectId, setLogProjectId] = useState<string | null>(null);
  const [logs, setLogs] = useState("等待日志数据...");
  const [runtimeKey, setRuntimeKey] = useState("");
  const [showKey, setShowKey] = useState(false);

  const refreshStatus = useCallback(async () => {
    try { setStatus(await api.getStatus()); } catch (cause) { setError(String(cause)); }
  }, []);

  const refreshLogs = useCallback(async () => {
    try { setLogs(await api.readLogs(logSource, logSource === "mcp" ? logProjectId ?? undefined : undefined)); } catch (cause) { setLogs(`无法读取日志\n${String(cause)}`); }
  }, [logProjectId, logSource]);

  useEffect(() => {
    void refreshStatus();
    void api.getSettings().then((loaded) => {
      setSettings(loaded);
      setSavedSettings(loaded);
      setSelectedProjectId(loaded.active_project_id ?? loaded.projects[0]?.id ?? null);
      setLogProjectId(loaded.projects[0]?.id ?? null);
    }).catch((cause) => setError(String(cause)));
    const timer = window.setInterval(refreshStatus, 1800);
    return () => window.clearInterval(timer);
  }, [refreshStatus]);

  useEffect(() => {
    if (view !== "logs") return;
    void refreshLogs();
    const timer = window.setInterval(refreshLogs, 2500);
    return () => window.clearInterval(timer);
  }, [view, refreshLogs]);

  const copy = useMemo(() => stateCopy(status), [status]);
  const isRunning = status.overall === "running" || status.overall === "starting";
  const selectedProject = settings?.projects.find((project) => project.id === selectedProjectId) ?? settings?.projects[0];
  const settingsDirty = settings !== null && savedSettings !== null && JSON.stringify(settings) !== JSON.stringify(savedSettings);

  async function handlePower(action: "start" | "stop" | "restart") {
    if (busy) return;
    if (settingsDirty && action !== "stop") {
      setError("项目配置尚未保存，请先点击“保存设置”后再启动连接。");
      return;
    }
    setBusy(true); setError(""); setNotice("");
    try {
      if (action === "restart") { await api.stopAllProjects(); setStatus(await api.startAllProjects()); }
      else setStatus(action === "start" ? await api.startAllProjects() : await api.stopAllProjects());
    } catch (cause) { setError(String(cause)); await refreshStatus(); } finally { setBusy(false); }
  }

  async function handleProjectPower(projectId: string, action: "start" | "stop") {
    if (projectBusy) return;
    if (settingsDirty && action === "start") {
      setError("项目配置尚未保存，请先点击“保存设置”后再启动项目。");
      return;
    }
    setProjectBusy(projectId); setError(""); setNotice("");
    try {
      setStatus(action === "start" ? await api.startProject(projectId) : await api.stopProject(projectId));
    } catch (cause) { setError(String(cause)); await refreshStatus(); } finally { setProjectBusy(null); }
  }

  async function handleSaveSettings() {
    if (!settings) return;
    setBusy(true); setError("");
    try {
      const saved = await api.saveSettings({ ...settings, active_project_id: selectedProjectId });
      setSettings(saved);
      setSavedSettings(saved);
      setNotice(isRunning ? "设置已保存。项目目录、端口或启用状态将在重启连接后生效。" : "设置已保存。启用项目后可以同时启动。");
    } catch (cause) { setError(String(cause)); } finally { setBusy(false); }
  }

  async function handleSaveKey() {
    if (!runtimeKey.trim()) return;
    setBusy(true); setError("");
    try { await api.saveRuntimeKey(runtimeKey.trim()); setRuntimeKey(""); setNotice("Runtime API Key 已写入 Windows 安全存储。"); await refreshStatus(); } catch (cause) { setError(String(cause)); } finally { setBusy(false); }
  }

  function updateProject(id: string, patch: Partial<ProjectConfig>) {
    if (!settings) return;
    setSettings({ ...settings, projects: settings.projects.map((project) => project.id === id ? { ...project, ...patch } : project) });
  }

  function addProject() {
    if (!settings) return;
    const base = "new-project";
    const usedIds = new Set(settings.projects.map((project) => project.id));
    let id = base;
    let suffix = 2;
    while (usedIds.has(id)) { id = `${base}-${suffix}`; suffix += 1; }
    const usedPorts = new Set(settings.projects.map((project) => project.mcp_port));
    let port = 8000;
    while (usedPorts.has(port) || port === settings.mcp_proxy_port || port === settings.router_port || port === settings.health_port) port += 1;
    const project: ProjectConfig = { id, name: "新项目", output_directory: "C:\\Projects\\new-project", mcp_host: "127.0.0.1", mcp_port: port, enabled: false };
    setSettings({ ...settings, projects: [...settings.projects, project], active_project_id: id });
    setSelectedProjectId(id);
    setNotice("已添加项目，请选择输出目录并保存设置。");
  }

  function removeProject(id: string) {
    if (!settings) return;
    const runtime = status.projects.find((project) => project.project_id === id);
    if (runtime?.overall === "running" || runtime?.overall === "starting") { setError("请先停止该项目，再删除项目配置。"); return; }
    const projects = settings.projects.filter((project) => project.id !== id);
    const next = projects[0]?.id ?? null;
    setSettings({ ...settings, projects, active_project_id: next });
    setSelectedProjectId(next);
  }

  async function chooseDirectory(id: string) {
    const selected = await open({ directory: true, multiple: false, title: "选择项目文档输出目录" });
    if (typeof selected === "string") updateProject(id, { output_directory: selected });
  }

  async function chooseProxyExecutable() {
    const selected = await open({ directory: false, multiple: false, title: "选择 mcp-proxy.exe" });
    if (typeof selected === "string" && settings) setSettings({ ...settings, proxy_executable: selected });
  }

  async function chooseMcpExecutable() {
    const selected = await open({ directory: false, multiple: false, title: "选择 chatgpt-delegate Connector" });
    if (typeof selected === "string" && settings) setSettings({ ...settings, mcp_executable: selected });
  }

  return (
    <main className="app-shell">
      <aside className="sidebar">
        <div className="brand-mark" aria-label="Delegate Control for ChatGPT"><span className="brand-rail brand-rail-a" /><span className="brand-node" /><span className="brand-rail brand-rail-b" /></div>
        <nav className="nav-list" aria-label="主导航">
          {NAV_ITEMS.map((item) => { const Icon = item.icon; return <button key={item.id} className={`nav-button ${view === item.id ? "is-active" : ""}`} onClick={() => setView(item.id)} title={item.label} aria-label={item.label}><Icon size={19} strokeWidth={1.8} /></button>; })}
        </nav>
        <div className="sidebar-status" title={status.overall === "running" ? "连接正常" : "当前未连接"}><StatusDot ready={status.overall === "running"} working={status.overall === "starting"} /></div>
      </aside>

      <section className="workspace">
        <header className="topbar">
          <div><div className="product-name">Delegate Control for ChatGPT</div><div className="product-context">CHATGPT CONNECTOR CONSOLE</div></div>
          <div className="topbar-actions"><button className="icon-button" onClick={refreshStatus} disabled={busy} title="刷新状态" aria-label="刷新状态"><RefreshCw size={18} className={busy ? "spin" : ""} /></button><div className={`connection-pill ${status.overall}`}><StatusDot ready={status.overall === "running"} working={status.overall === "starting"} />{status.overall === "running" ? "ONLINE" : status.overall.toUpperCase()}</div></div>
        </header>

        {error && <div className="alert-band error-band" role="alert"><CircleAlert size={18} /><span>{error}</span><button onClick={() => setError("")} aria-label="关闭错误提示">×</button></div>}
        {notice && <div className="alert-band success-band"><Check size={18} /><span>{notice}</span><button onClick={() => setNotice("")} aria-label="关闭成功提示">×</button></div>}

        {view === "overview" && <div className="view overview-view">
          <section className="active-project-banner" aria-label="当前项目"><div><span className="eyebrow">ACTIVE PROJECT</span><strong>{selectedProject?.name ?? "未选择项目"}</strong></div><code>{selectedProject?.output_directory ?? "请在设置中选择项目目录"}</code></section>
          <section className={`signal-hero ${copy.tone}`}><div className="signal-copy"><div className="eyebrow">{copy.eyebrow}</div><h1>{copy.title}</h1><p>{status.message || "连接仅在你需要时建立，退出程序会自动清理后台进程。"}</p><div className="hero-actions"><button className={`power-button ${isRunning ? "stop" : "start"}`} onClick={() => handlePower(isRunning ? "stop" : "start")} disabled={busy || status.overall === "stopping"}>{busy ? <LoaderCircle className="spin" size={20} /> : isRunning ? <Unplug size={20} /> : <Power size={20} />}{busy ? "处理中" : isRunning ? "停止全部项目" : "启动全部项目"}</button>{status.overall === "running" && <button className="secondary-button" onClick={() => handlePower("restart")} disabled={busy}><RotateCw size={18} />重新连接</button>}</div></div><div className="signal-visual" aria-hidden><div className="signal-orbit orbit-one" /><div className="signal-orbit orbit-two" /><div className="signal-core"><Link2 size={34} strokeWidth={1.5} /></div><span className="pulse pulse-a" /><span className="pulse pulse-b" /><span className="pulse pulse-c" /></div></section>

          <section className="chain-section"><div className="section-heading"><div><span className="eyebrow">CONNECTION PATH</span><h2>共享链路</h2></div><span className="section-note">127.0.0.1 · PRIVATE LOOPBACK</span></div><div className="chain-grid"><ChainItem icon={Network} index="01" title="Clash Proxy" detail={settings ? `${settings.proxy_host}:${settings.proxy_port}` : "127.0.0.1:7897"} ready={status.proxy_ready} /><ChevronRight className="chain-arrow" size={18} /><ChainItem icon={Server} index="02" title="Router MCP" detail={`PID ${status.router_pid ?? "—"}`} ready={status.router_ready} /><ChevronRight className="chain-arrow" size={18} /><ChainItem icon={Server} index="03" title="MCP Proxy" detail={`PID ${status.proxy_pid ?? "—"}`} ready={status.mcp_proxy_ready} /><ChevronRight className="chain-arrow" size={18} /><ChainItem icon={ShieldCheck} index="04" title="Secure Tunnel" detail={`PID ${status.tunnel_pid ?? "—"}`} ready={status.tunnel_ready} /></div></section>

          <section className="project-status-section"><div className="section-heading"><div><span className="eyebrow">PROJECT BACKENDS</span><h2>在线项目</h2></div><span className="section-note">{status.projects.filter((project) => project.overall === "running").length} ONLINE</span></div><div className="project-status-grid">{status.projects.map((project) => <ProjectStatusRow key={project.project_id} project={project} busy={projectBusy === project.project_id} onStart={() => void handleProjectPower(project.project_id, "start")} onStop={() => void handleProjectPower(project.project_id, "stop")} />)}</div></section>

          <section className="detail-strip"><div><span>启动模式</span><strong>手动按需</strong></div><div><span>密钥存储</span><strong>{status.credential_configured ? "Windows Credential Manager" : "尚未配置"}</strong></div><div><span>文件编辑</span><strong className={status.text_editing_available ? "capability-ready" : "capability-muted"}>{status.text_editing_available ? "已支持" : "旧 Connector"}</strong></div><div><span>关闭窗口</span><strong>隐藏到系统托盘</strong></div></section>
        </div>}

        {view === "logs" && <div className="view logs-view"><div className="view-heading"><div><span className="eyebrow">RUNTIME TRACE</span><h1>运行日志</h1></div><button className="secondary-button" onClick={() => void api.openLogDirectory()}><FolderOpen size={18} />打开目录</button></div><div className="segmented-control" role="tablist"><button className={logSource === "mcp" ? "is-active" : ""} onClick={() => setLogSource("mcp")}><Server size={16} />项目 MCP</button><button className={logSource === "router" ? "is-active" : ""} onClick={() => setLogSource("router")}><Server size={16} />Router MCP</button><button className={logSource === "proxy" ? "is-active" : ""} onClick={() => setLogSource("proxy")}><Network size={16} />MCP Proxy</button><button className={logSource === "tunnel" ? "is-active" : ""} onClick={() => setLogSource("tunnel")}><ShieldCheck size={16} />Secure Tunnel</button></div>{logSource === "mcp" && <label className="log-project-select"><span>项目</span><select value={logProjectId ?? ""} onChange={(event) => setLogProjectId(event.target.value || null)}>{settings?.projects.map((project) => <option key={project.id} value={project.id}>{project.name}</option>)}</select></label>}<pre className="log-console">{logs}</pre><div className="log-footer"><StatusDot ready={status.overall === "running"} />每 2.5 秒自动刷新<button className="text-button" onClick={refreshLogs}><RefreshCw size={15} />立即刷新</button></div></div>}

        {view === "settings" && settings && <div className="view settings-view"><div className="view-heading"><div><span className="eyebrow">LOCAL CONFIGURATION</span><h1>连接设置</h1></div><button className="primary-compact" onClick={handleSaveSettings} disabled={busy}><Save size={17} />保存设置</button></div>
          <section className="settings-band project-management-band"><div className="settings-label"><MonitorCog size={20} /><div><h2>项目目录</h2><p>每个项目拥有独立文档目录；在线项目通过共享 MCP Proxy 暴露。</p></div></div><div className="project-editor"><div className="project-toolbar"><strong>{settings.projects.length} 个项目</strong><button className="secondary-button" onClick={addProject}><Plus size={17} />新增项目</button></div>{settings.projects.map((project) => { const runtime = status.projects.find((item) => item.project_id === project.id); const running = runtime?.overall === "running" || runtime?.overall === "starting"; return <div className={`project-editor-row ${selectedProjectId === project.id ? "is-selected" : ""}`} key={project.id} onClick={() => setSelectedProjectId(project.id)}><div className="project-editor-main"><input aria-label="项目名称" value={project.name} onChange={(event) => updateProject(project.id, { name: event.target.value })} /><code>{project.id}</code><div className="project-path-editor"><code>{project.output_directory}</code><button className="icon-button" onClick={(event) => { event.stopPropagation(); void chooseDirectory(project.id); }} title="选择输出目录" aria-label="选择输出目录"><FolderOpen size={16} /></button></div></div><label className="project-enabled"><input type="checkbox" checked={project.enabled} onChange={(event) => updateProject(project.id, { enabled: event.target.checked })} />启用</label><button className="icon-button" onClick={(event) => { event.stopPropagation(); void handleProjectPower(project.id, running ? "stop" : "start"); }} disabled={projectBusy !== null && projectBusy !== project.id} title={running ? "停止项目" : "启动项目"} aria-label={running ? "停止项目" : "启动项目"}>{projectBusy === project.id ? <LoaderCircle className="spin" size={16} /> : running ? <Square size={16} /> : <Play size={16} />}</button><button className="icon-button danger-icon" onClick={(event) => { event.stopPropagation(); removeProject(project.id); }} title="删除项目" aria-label="删除项目"><Trash2 size={16} /></button></div>; })}</div></section>

          <section className="settings-band"><div className="settings-label"><KeyRound size={20} /><div><h2>Runtime API Key</h2><p>凭据由当前 Windows 账户加密保管。</p></div></div><div className="key-editor"><div className="input-with-icon"><input type={showKey ? "text" : "password"} value={runtimeKey} onChange={(event) => setRuntimeKey(event.target.value)} placeholder={status.credential_configured ? "已安全保存，输入新密钥可覆盖" : "粘贴 Runtime API Key"} /><button onClick={() => setShowKey((value) => !value)} title={showKey ? "隐藏密钥" : "显示密钥"} aria-label={showKey ? "隐藏密钥" : "显示密钥"}>{showKey ? <EyeOff size={17} /> : <Eye size={17} />}</button></div><button className="secondary-button" onClick={handleSaveKey} disabled={!runtimeKey.trim() || busy}><ShieldCheck size={17} />安全保存</button></div></section>

          <section className="settings-band settings-grid-band"><div className="settings-label"><Network size={20} /><div><h2>共享网络与端口</h2><p>固定 Router 工具目录，项目通过 project_id 参数路由。</p></div></div><div className="form-grid"><Field label="Clash 主机"><input value={settings.proxy_host} onChange={(event) => setSettings({ ...settings, proxy_host: event.target.value })} /></Field><Field label="Clash 端口"><input type="number" value={settings.proxy_port} onChange={(event) => setSettings({ ...settings, proxy_port: Number(event.target.value) })} /></Field><Field label="Router 端口"><input type="number" value={settings.router_port} onChange={(event) => setSettings({ ...settings, router_port: Number(event.target.value) })} /></Field><Field label="MCP Proxy 端口"><input type="number" value={settings.mcp_proxy_port} onChange={(event) => setSettings({ ...settings, mcp_proxy_port: Number(event.target.value) })} /></Field><Field label="健康端口"><input type="number" value={settings.health_port} onChange={(event) => setSettings({ ...settings, health_port: Number(event.target.value) })} /></Field></div></section>

          <section className="settings-band paths-band"><div className="settings-label"><MonitorCog size={20} /><div><h2>本机程序</h2><p>客户端管理项目 MCP、固定 Router、Proxy 与 Tunnel。</p></div></div><div className="path-list"><div className="path-edit-row"><span>MCP</span><input value={settings.mcp_executable} onChange={(event) => setSettings({ ...settings, mcp_executable: event.target.value })} /><button className="icon-button" onClick={() => void chooseMcpExecutable()} title="选择 chatgpt-delegate Connector" aria-label="选择 MCP Connector"><FolderOpen size={16} /></button></div><div className="capability-note"><StatusDot ready={status.text_editing_available} /><span>{status.connector_capability_message}</span></div><div className="path-edit-row"><span>MCP Proxy</span><input value={settings.proxy_executable} onChange={(event) => setSettings({ ...settings, proxy_executable: event.target.value })} /><button className="icon-button" onClick={() => void chooseProxyExecutable()} title="选择 MCP Proxy" aria-label="选择 MCP Proxy"><FolderOpen size={16} /></button></div><PathRow label="Tunnel" value={settings.tunnel_executable} /><PathRow label="Proxy 配置" value={settings.proxy_config_path} /><PathRow label="Router 注册表" value={settings.router_config_path} /></div></section>
        </div>}
      </section>
    </main>
  );
}

function ChainItem({ icon: Icon, index, title, detail, ready }: { icon: typeof Network; index: string; title: string; detail: string; ready: boolean }) { return <div className={`chain-item ${ready ? "is-ready" : ""}`}><span className="chain-index">{index}</span><Icon size={22} strokeWidth={1.6} /><div><strong>{title}</strong><span>{detail}</span></div><StatusDot ready={ready} /></div>; }
function ProjectStatusRow({ project, busy, onStart, onStop }: { project: DelegateStatus["projects"][number]; busy: boolean; onStart: () => void; onStop: () => void }) { const running = project.overall === "running" || project.overall === "starting"; return <div className={`project-status-row ${project.overall}`}><div><strong>{project.name}</strong><span>{project.output_directory}</span></div><code>{project.project_id}</code><StatusDot ready={project.mcp_ready} working={project.overall === "starting"} /><button className="icon-button" onClick={running ? onStop : onStart} disabled={busy} title={running ? "停止项目" : "启动项目"} aria-label={running ? `停止${project.name}` : `启动${project.name}`}>{busy ? <LoaderCircle className="spin" size={16} /> : running ? <Square size={16} /> : <Play size={16} />}</button></div>; }
function Field({ label, children }: { label: string; children: React.ReactNode }) { return <label className="field"><span>{label}</span>{children}</label>; }
function PathRow({ label, value }: { label: string; value: string }) { async function copyValue() { await navigator.clipboard.writeText(value); } return <div className="path-row"><span>{label}</span><code>{value}</code><button onClick={copyValue} title="复制路径" aria-label={`复制${label}路径`}><Copy size={15} /></button></div>; }

export default App;
