import { useCallback, useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { Activity, Check, ChevronRight, CircleAlert, Copy, Eye, EyeOff, Folder, FolderOpen, Grid2X2, KeyRound, Link2, LoaderCircle, MonitorCog, MoreHorizontal, Network, Play, Plus, Power, RefreshCw, RotateCw, Save, Search, Server, Settings2, ShieldCheck, Square, SquareTerminal, Trash2, Unplug } from "lucide-react";
import { api } from "./api";
import { getLegacyProjectKeyProposal, PROJECT_KEY_PATTERN, suggestProjectKey, validateProjectKeyCandidate } from "./projectIdentity";
import type { AppSettings, DelegateStatus, LogSource, ProjectConfig, ProjectRuntimeStatus, ViewId } from "./types";

const EMPTY_STATUS: DelegateStatus = {
  overall: "stopped", proxy_ready: false, router_ready: false, mcp_proxy_ready: false, mcp_ready: false, tunnel_ready: false, proxy_pid: null, router_pid: null, mcp_pid: null, tunnel_pid: null, credential_configured: false, text_editing_available: false, connector_capability_message: "正在检测 Connector 文件编辑能力", message: "正在读取本机状态", projects: [],
};

const NAV_ITEMS = [
  { id: "overview" as const, label: "首页", icon: Activity },
  { id: "projects" as const, label: "项目管理", icon: Grid2X2 },
  { id: "logs" as const, label: "运行日志", icon: SquareTerminal },
  { id: "settings" as const, label: "连接设置", icon: Settings2 },
];

type DraftProjectKey = { value: string; automatic: boolean };

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
  const [draftProjectKeys, setDraftProjectKeys] = useState<Record<string, DraftProjectKey>>({});
  const [migrationProjectId, setMigrationProjectId] = useState<string | null>(null);
  const [migrationKey, setMigrationKey] = useState("");
  const [dismissedLegacyProjectIds, setDismissedLegacyProjectIds] = useState<string[]>([]);
  const [projectQuery, setProjectQuery] = useState("");

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
      if (loaded.recovery_notice) setError(loaded.recovery_notice);
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
  const pageTitle = view === "overview" ? "首页" : view === "projects" ? "项目管理" : view === "logs" ? "运行日志" : "连接设置";
  const pageEyebrow = view === "overview" ? "CONNECTION CONSOLE" : view === "projects" ? "PROJECT MANAGEMENT" : view === "logs" ? "RUNTIME TRACE" : "LOCAL CONFIGURATION";
  const globalTone = status.overall === "running" ? "running" : status.overall === "failed" || status.overall === "starting" || status.overall === "stopping" ? "degraded" : "offline";
  const globalLabel = globalTone === "running" ? "ONLINE" : globalTone === "degraded" ? "DEGRADED" : "OFFLINE";
  const activeProjectId = selectedProjectId ?? settings?.active_project_id ?? null;
  const configuredProjects = (settings?.projects.map((configured) => {
    const runtime = status.projects.find((project) => project.project_id === configured.id);
    return runtime ?? {
      project_id: configured.id,
      name: configured.name,
      output_directory: configured.output_directory,
      overall: "stopped",
      mcp_ready: false,
      mcp_pid: null,
      message: configured.enabled ? "项目未启动。" : "项目已停用。",
    } satisfies ProjectRuntimeStatus;
  }) ?? status.projects).sort((left, right) => Number(right.project_id === activeProjectId) - Number(left.project_id === activeProjectId));
  const visibleProjects = configuredProjects.filter((project) => {
    const query = projectQuery.trim().toLowerCase();
    return !query || `${project.name} ${project.project_id} ${project.output_directory}`.toLowerCase().includes(query);
  });
  const isRunning = status.overall === "running" || status.overall === "starting";
  const selectedProject = settings?.projects.find((project) => project.id === selectedProjectId) ?? settings?.projects[0];
  const settingsDirty = settings !== null && savedSettings !== null && (JSON.stringify(settings) !== JSON.stringify(savedSettings) || Object.keys(draftProjectKeys).length > 0);
  const migrationValidation = settings && migrationProjectId ? validateProjectKeyCandidate(migrationProjectId, migrationKey.trim(), settings.projects) : null;

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
      if (!Number.isInteger(settings.proxy_port) || settings.proxy_port < 1 || settings.proxy_port > 65535) {
        throw new Error("Magic 端口必须是 1 到 65535 之间的整数。");
      }
      const idMapping = new Map<string, string>();
      const usedIds = new Set(settings.projects.filter((project) => !draftProjectKeys[project.id]).map((project) => project.id));
      const projects = settings.projects.map((project) => {
        const draft = draftProjectKeys[project.id];
        if (!draft) return project;
        const projectKey = draft.value.trim();
        if (!PROJECT_KEY_PATTERN.test(projectKey)) throw new Error(`项目“${project.name || "未命名项目"}”的 Project Key 无效。请使用小写字母、数字、短横线或下划线，长度不超过 64。`);
        if (usedIds.has(projectKey)) throw new Error(`Project Key 已存在：${projectKey}`);
        usedIds.add(projectKey);
        idMapping.set(project.id, projectKey);
        return { ...project, id: projectKey };
      });
      const committedSelection = selectedProjectId ? idMapping.get(selectedProjectId) ?? selectedProjectId : null;
      const saved = await api.saveSettings({ ...settings, projects, active_project_id: committedSelection });
      setSettings(saved);
      setSavedSettings(saved);
      setSelectedProjectId(committedSelection);
      setLogProjectId((current) => current ? idMapping.get(current) ?? current : saved.projects[0]?.id ?? null);
      setDraftProjectKeys({});
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

  function updateProjectName(id: string, name: string) {
    if (!settings) return;
    updateProject(id, { name });
    const draft = draftProjectKeys[id];
    if (!draft?.automatic) return;
    const usedIds = [
      ...settings.projects.filter((project) => project.id !== id && !draftProjectKeys[project.id]).map((project) => project.id),
      ...Object.entries(draftProjectKeys).filter(([projectId]) => projectId !== id).map(([, value]) => value.value),
    ];
    setDraftProjectKeys((current) => ({ ...current, [id]: { value: suggestProjectKey(name, usedIds), automatic: true } }));
  }

  function addProject() {
    if (!settings) return;
    const id = `draft-${Date.now()}`;
    const proposedKey = suggestProjectKey("project", [...settings.projects.map((project) => project.id), ...Object.values(draftProjectKeys).map((value) => value.value)]);
    const usedPorts = new Set(settings.projects.map((project) => project.mcp_port));
    let port = 8000;
    while (usedPorts.has(port) || port === settings.mcp_proxy_port || port === settings.router_port || port === settings.health_port) port += 1;
    const project: ProjectConfig = { id, name: "新项目", output_directory: `C:\\Projects\\${proposedKey}`, mcp_host: "127.0.0.1", mcp_port: port, enabled: false };
    setSettings({ ...settings, projects: [...settings.projects, project], active_project_id: id });
    setDraftProjectKeys((current) => ({ ...current, [id]: { value: proposedKey, automatic: true } }));
    setSelectedProjectId(id);
    setNotice("已添加项目。请确认项目名称、Project Key 和输出目录后保存。");
  }

  function removeProject(id: string) {
    if (!settings) return;
    const runtime = status.projects.find((project) => project.project_id === id);
    if (runtime?.overall === "running" || runtime?.overall === "starting") { setError("请先停止该项目，再删除项目配置。"); return; }
    const projects = settings.projects.filter((project) => project.id !== id);
    const next = projects[0]?.id ?? null;
    setSettings({ ...settings, projects, active_project_id: next });
    setSelectedProjectId(next);
    setDraftProjectKeys((current) => { const nextDrafts = { ...current }; delete nextDrafts[id]; return nextDrafts; });
    setDismissedLegacyProjectIds((current) => current.filter((projectId) => projectId !== id));
    if (migrationProjectId === id) { setMigrationProjectId(null); setMigrationKey(""); }
  }

  function openProjectKeyMigration(projectId: string, proposedKey = projectId) {
    if (status.overall !== "stopped") { setError("迁移 Project Key 前必须先停止全部项目连接。"); return; }
    if (settingsDirty) { setError("当前设置尚未保存。请先保存其他修改，再迁移 Project Key。"); return; }
    setError(""); setNotice("");
    setMigrationProjectId(projectId);
    setMigrationKey(proposedKey);
  }

  async function handleMigrateProjectKey(projectId: string) {
    if (!settings) return;
    if (status.overall !== "stopped") { setError("迁移 Project Key 前必须先停止全部项目连接。"); return; }
    if (settingsDirty) { setError("当前设置尚未保存。请先保存其他修改，再迁移 Project Key。"); return; }
    const newKey = migrationKey.trim();
    const validation = validateProjectKeyCandidate(projectId, newKey, settings.projects);
    if (validation) { setError(validation); return; }
    setProjectBusy(projectId); setError(""); setNotice("");
    try {
      const saved = await api.migrateProjectKey(projectId, newKey);
      setSettings(saved); setSavedSettings(saved);
      setSelectedProjectId((current) => current === projectId ? newKey : current);
      setLogProjectId((current) => current === projectId ? newKey : current);
      setDismissedLegacyProjectIds((current) => current.filter((id) => id !== projectId));
      setMigrationProjectId(null); setMigrationKey("");
      setNotice(`Project Key 已从 ${projectId} 迁移为 ${newKey}。ChatGPT 历史调用中的旧 key 需要手动更新。`);
      await refreshStatus();
    } catch (cause) { setError(String(cause)); } finally { setProjectBusy(null); }
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
        <div className="brand-lockup" aria-label="Delegate Control for ChatGPT"><div className="brand-mark"><Activity size={18} strokeWidth={2.2} /></div><div className="brand-copy"><strong>Delegate Control</strong><span>for ChatGPT</span></div></div>
        <nav className="nav-list" aria-label="主导航">
          {NAV_ITEMS.map((item) => { const Icon = item.icon; const ariaLabel = item.id === "settings" ? "设置" : item.label; return <button key={item.id} className={`nav-button ${view === item.id ? "is-active" : ""}`} onClick={() => setView(item.id)} title={item.label} aria-label={ariaLabel}><Icon size={19} strokeWidth={1.8} /><span className="nav-label">{item.label}</span></button>; })}
        </nav>
        <div className="sidebar-status" title={status.overall === "running" ? "连接正常" : "当前未连接"}><div className="sidebar-foot-row"><StatusDot ready={status.overall === "running"} working={status.overall === "starting"} /><span className="sidebar-foot-text">系统运行中</span></div><div className="sidebar-version">v1.0.0 · local control plane</div></div>
      </aside>

      <section className="workspace">
        <header className="topbar">
          <div className="header-title"><div className="product-context">{pageEyebrow}</div><div className="product-name">{pageTitle}</div></div>
          <div className="topbar-actions"><button className="icon-button" onClick={refreshStatus} disabled={busy} title="刷新状态" aria-label="刷新状态"><RefreshCw size={18} className={busy ? "spin" : ""} /></button><div className={`connection-pill ${globalTone}`}><StatusDot ready={globalTone === "running"} working={globalTone === "degraded"} />{globalLabel}</div></div>
        </header>

        {error && <div className="alert-band error-band" role="alert"><CircleAlert size={18} /><span>{error}</span><button onClick={() => setError("")} aria-label="关闭错误提示">×</button></div>}
        {notice && <div className="alert-band success-band"><Check size={18} /><span>{notice}</span><button onClick={() => setNotice("")} aria-label="关闭成功提示">×</button></div>}

        {view === "overview" && <div className="view overview-view prototype-dashboard">
          <section className="conn-summary">
            <div className="conn-left">
              <div className="conn-status-row"><div className="conn-icon"><Link2 size={22} /></div><div><div className="eyebrow">CONNECTION STATUS</div><div className="conn-title">{copy.title}</div></div></div>
              <p className="conn-desc">{status.message || "连接仅在需要时启动。"}{selectedProject && <> 当前活跃项目 <b>{selectedProject.name}</b>。</>}</p>
              <div className="conn-actions"><button className="btn btn-ghost-danger" onClick={() => handlePower(isRunning ? "stop" : "start")} disabled={busy || status.overall === "stopping"}>{busy ? <LoaderCircle className="spin" size={17} /> : isRunning ? <Square size={17} /> : <Power size={17} />}{busy ? "处理中" : isRunning ? "停止全部项目" : "启动全部项目"}</button>{status.overall === "running" && <button className="btn btn-secondary" onClick={() => handlePower("restart")} disabled={busy}><RotateCw size={17} />重新连接</button>}</div>
            </div>
            <div className="conn-infra">
              <InfraCell icon={Network} title="Magic Proxy" detail={settings ? `${settings.proxy_host}:${settings.proxy_port}` : "127.0.0.1"} ready={status.proxy_ready} />
              <InfraCell icon={Server} title="Router MCP" detail={`PID ${status.router_pid ?? "—"}`} ready={status.router_ready} />
              <InfraCell icon={Server} title="MCP Proxy" detail={`PID ${status.proxy_pid ?? "—"}`} ready={status.mcp_proxy_ready} />
              <InfraCell icon={ShieldCheck} title="Secure Tunnel" detail={`PID ${status.tunnel_pid ?? "—"}`} ready={status.tunnel_ready} />
            </div>
          </section>

          <section className="section project-backends-section"><div className="proj-toolbar"><div><div className="eyebrow">PROJECT BACKENDS</div><div className="section-title">项目 <span className="proj-count">{configuredProjects.length}</span></div></div><div className="proj-tools"><label className="search-input-wrap"><Search size={16} /><input className="search-input" aria-label="搜索项目" value={projectQuery} onChange={(event) => setProjectQuery(event.target.value)} placeholder="搜索名称 / key / 路径…" /></label><button className="btn btn-primary btn-sm" onClick={addProject}><Plus size={17} />新增项目</button></div></div><div className="proj-scroll"><div className="proj-list">{visibleProjects.map((project) => <ProjectListRow key={project.project_id} project={project} busy={projectBusy === project.project_id} active={project.project_id === selectedProjectId} onSelect={() => setSelectedProjectId(project.project_id)} onStart={() => void handleProjectPower(project.project_id, "start")} onStop={() => void handleProjectPower(project.project_id, "stop")} />)}</div></div></section>

          <section className="section connection-path-section"><div className="section-head"><div><div className="eyebrow">CONNECTION PATH</div><div className="section-title">共享链路</div></div><span className="section-note">127.0.0.1 · Private Loopback</span></div><div className="path-flow"><PathStep icon={Network} number="01" title="Magic Proxy" detail={settings ? `${settings.proxy_host}:${settings.proxy_port}` : "127.0.0.1"} ready={status.proxy_ready} /><ChevronRight className="path-arrow" size={18} /><PathStep icon={Server} number="02" title="Router MCP" detail={`PID ${status.router_pid ?? "—"}`} ready={status.router_ready} /><ChevronRight className="path-arrow" size={18} /><PathStep icon={Server} number="03" title="MCP Proxy" detail={`PID ${status.proxy_pid ?? "—"}`} ready={status.mcp_proxy_ready} /><ChevronRight className="path-arrow" size={18} /><PathStep icon={ShieldCheck} number="04" title="Secure Tunnel" detail={`PID ${status.tunnel_pid ?? "—"}`} ready={status.tunnel_ready} /></div></section>

          <section className="section operational-meta-section"><div className="meta-row"><div className="meta-cell"><div className="mlabel">启动模式</div><div className="mval">手动按需</div></div><div className="meta-cell"><div className="mlabel">密钥存储</div><div className="mval">{status.credential_configured ? "Windows Credential Manager" : "尚未配置"}</div></div><div className="meta-cell"><div className="mlabel">文件编辑</div><div className={`mval ${status.text_editing_available ? "ok" : ""}`}>{status.text_editing_available ? "已支持" : "旧 Connector"}</div></div><div className="meta-cell"><div className="mlabel">关闭窗口</div><div className="mval">隐藏到系统托盘</div></div></div></section>
        </div>}

        {view === "logs" && <div className="view logs-view"><div className="view-heading"><div><span className="eyebrow">RUNTIME TRACE</span><h1>运行日志</h1></div><button className="secondary-button" onClick={() => void api.openLogDirectory()}><FolderOpen size={18} />打开目录</button></div><div className="segmented-control" role="tablist"><button className={logSource === "mcp" ? "is-active" : ""} onClick={() => setLogSource("mcp")}><Server size={16} />项目 MCP</button><button className={logSource === "router" ? "is-active" : ""} onClick={() => setLogSource("router")}><Server size={16} />Router MCP</button><button className={logSource === "proxy" ? "is-active" : ""} onClick={() => setLogSource("proxy")}><Network size={16} />MCP Proxy</button><button className={logSource === "tunnel" ? "is-active" : ""} onClick={() => setLogSource("tunnel")}><ShieldCheck size={16} />Secure Tunnel</button></div>{logSource === "mcp" && <label className="log-project-select"><span>项目</span><select value={logProjectId ?? ""} onChange={(event) => setLogProjectId(event.target.value || null)}>{settings?.projects.map((project) => <option key={project.id} value={project.id}>{project.name}</option>)}</select></label>}<pre className="log-console">{logs}</pre><div className="log-footer"><StatusDot ready={status.overall === "running"} />每 2.5 秒自动刷新<button className="text-button" onClick={refreshLogs}><RefreshCw size={15} />立即刷新</button></div></div>}

        {(view === "settings" || view === "projects") && settings && <div className="view settings-view"><div className="view-heading"><div><span className="eyebrow">LOCAL CONFIGURATION</span><h1>{view === "projects" ? "项目管理" : "连接设置"}</h1><p className="view-subtitle">管理项目身份、输出目录、连接参数与本机运行组件。</p></div><button className="primary-compact" onClick={handleSaveSettings} disabled={busy}><Save size={17} />保存设置</button></div>
          {view === "projects" && <section className="settings-band project-management-band">
            <div className="settings-label"><MonitorCog size={20} /><div><h2>项目目录</h2><p>项目名称用于识别，Project Key / project_id 用于 ChatGPT 和 Router 路由。</p></div></div>
            <div className="project-editor">
              <div className="project-toolbar"><strong>{settings.projects.length} 个项目</strong><button className="secondary-button" onClick={addProject}><Plus size={17} />新增项目</button></div>
              {settings.projects.map((project) => {
                const runtime = status.projects.find((item) => item.project_id === project.id);
                const running = runtime?.overall === "running" || runtime?.overall === "starting";
                const draftKey = draftProjectKeys[project.id];
                const migrating = migrationProjectId === project.id;
                const legacyProposal = draftKey ? null : getLegacyProjectKeyProposal(project, settings.projects);
                const showLegacyGuide = legacyProposal && !dismissedLegacyProjectIds.includes(project.id);
                return <div className={`project-editor-row ${selectedProjectId === project.id ? "is-selected" : ""}`} key={project.id} onClick={() => setSelectedProjectId(project.id)}>
                  <div className="project-editor-main">
                    <label className="project-identity-field"><span>项目名称</span><input aria-label="项目名称" value={project.name} onChange={(event) => updateProjectName(project.id, event.target.value)} /></label>
                    <div className="project-key-field"><span>Project Key / project_id</span>{draftKey ? <input aria-label="Project Key" value={draftKey.value} onChange={(event) => setDraftProjectKeys((current) => ({ ...current, [project.id]: { value: event.target.value, automatic: false } }))} /> : <div className="project-key-display"><code>{project.id}</code><button className="text-button" onClick={(event) => { event.stopPropagation(); openProjectKeyMigration(project.id); }} disabled={status.overall !== "stopped" || settingsDirty}>更改 key</button></div>}<small>{draftKey ? "首次保存后成为稳定路由标识" : "稳定标识；改名不会自动改变"}</small></div>
                    <div className="project-path-editor"><code>{project.output_directory}</code><button className="icon-button" onClick={(event) => { event.stopPropagation(); void chooseDirectory(project.id); }} title="选择输出目录" aria-label="选择输出目录"><FolderOpen size={16} /></button></div>
                    {showLegacyGuide && <div className="legacy-key-guide" onClick={(event) => event.stopPropagation()}>
                      <div className="legacy-key-copy"><CircleAlert size={17} /><div><strong>建议清理旧 Project Key</strong><p>这是旧版本生成的通用标识。迁移后 ChatGPT 和 Router 将使用更清晰的项目身份。</p></div></div>
                      <div className="key-transition" aria-label={`${project.id} 迁移为 ${legacyProposal.proposedKey}`}><code>{project.id}</code><ChevronRight size={15} /><code>{legacyProposal.proposedKey}</code></div>
                      {legacyProposal.conflict && <p className="migration-validation">建议 key 已被其他项目使用，开始迁移后请修改目标 key。</p>}
                      <div className="legacy-key-actions"><button className="secondary-button" aria-label={`稍后处理 ${project.name}`} onClick={() => setDismissedLegacyProjectIds((current) => [...current, project.id])}>稍后处理</button><button className="primary-compact" aria-label={`迁移 ${project.name} 的 Project Key`} onClick={() => openProjectKeyMigration(project.id, legacyProposal.proposedKey)} disabled={status.overall !== "stopped" || settingsDirty}><KeyRound size={15} />开始迁移</button></div>
                    </div>}
                    {migrating && <div className="project-key-migration" onClick={(event) => event.stopPropagation()}>
                      <div><strong>{legacyProposal ? "迁移旧 Project Key" : "更改 Project Key"}</strong><p>DCFC 和 Router 将改用新 key。历史 ChatGPT 调用及外部文档中的旧 key 不会自动更新；迁移仅在全部连接停止时执行。</p></div>
                      <div className="key-transition migration-transition"><code>{project.id}</code><ChevronRight size={15} /><code>{migrationKey.trim() || "新 key"}</code></div>
                      <input aria-label="新的 Project Key" value={migrationKey} onChange={(event) => setMigrationKey(event.target.value)} />
                      {migrationValidation && <p className="migration-validation" role="alert">{migrationValidation}</p>}
                      <div className="migration-actions"><button className="secondary-button" onClick={() => { setMigrationProjectId(null); setMigrationKey(""); }}>取消</button><button className="primary-compact" onClick={() => void handleMigrateProjectKey(project.id)} disabled={projectBusy === project.id || migrationValidation !== null}>{projectBusy === project.id ? <LoaderCircle className="spin" size={16} /> : <KeyRound size={16} />}确认迁移</button></div>
                    </div>}
                  </div>
                  <label className="project-enabled"><input type="checkbox" checked={project.enabled} onChange={(event) => updateProject(project.id, { enabled: event.target.checked })} />启用</label>
                  <button className="icon-button" onClick={(event) => { event.stopPropagation(); void handleProjectPower(project.id, running ? "stop" : "start"); }} disabled={projectBusy !== null && projectBusy !== project.id} title={running ? "停止项目" : "启动项目"} aria-label={running ? "停止项目" : "启动项目"}>{projectBusy === project.id ? <LoaderCircle className="spin" size={16} /> : running ? <Square size={16} /> : <Play size={16} />}</button>
                  <button className="icon-button danger-icon" onClick={(event) => { event.stopPropagation(); removeProject(project.id); }} title="删除项目" aria-label="删除项目"><Trash2 size={16} /></button>
                </div>;
              })}
            </div>
          </section>}

          {view === "settings" && <><section className="settings-band"><div className="settings-label"><KeyRound size={20} /><div><h2>Runtime API Key</h2><p>凭据由当前 Windows 账户加密保管。</p></div></div><div className="key-editor"><div className="input-with-icon"><input type={showKey ? "text" : "password"} value={runtimeKey} onChange={(event) => setRuntimeKey(event.target.value)} placeholder={status.credential_configured ? "已安全保存，输入新密钥可覆盖" : "粘贴 Runtime API Key"} /><button onClick={() => setShowKey((value) => !value)} title={showKey ? "隐藏密钥" : "显示密钥"} aria-label={showKey ? "隐藏密钥" : "显示密钥"}>{showKey ? <EyeOff size={17} /> : <Eye size={17} />}</button></div><button className="secondary-button" onClick={handleSaveKey} disabled={!runtimeKey.trim() || busy}><ShieldCheck size={17} />安全保存</button></div></section>

          <section className="settings-band settings-grid-band"><div className="settings-label"><Network size={20} /><div><h2>共享网络与端口</h2><p>固定 Router 工具目录，项目通过 project_id 参数路由。</p></div></div><div className="form-grid"><Field label="Magic 主机（固定）"><input value={settings.proxy_host} readOnly /></Field><Field label="Magic 端口"><input type="number" min={1} max={65535} step={1} value={settings.proxy_port} onChange={(event) => setSettings({ ...settings, proxy_port: Number(event.target.value) })} /></Field><Field label="Router 端口"><input type="number" value={settings.router_port} onChange={(event) => setSettings({ ...settings, router_port: Number(event.target.value) })} /></Field><Field label="MCP Proxy 端口"><input type="number" value={settings.mcp_proxy_port} onChange={(event) => setSettings({ ...settings, mcp_proxy_port: Number(event.target.value) })} /></Field><Field label="健康端口"><input type="number" value={settings.health_port} onChange={(event) => setSettings({ ...settings, health_port: Number(event.target.value) })} /></Field></div></section>

          <section className="settings-band paths-band"><div className="settings-label"><MonitorCog size={20} /><div><h2>本机程序</h2><p>客户端管理项目 MCP、固定 Router、Proxy 与 Tunnel。</p></div></div><div className="path-list"><div className="path-edit-row"><span>MCP</span><input value={settings.mcp_executable} onChange={(event) => setSettings({ ...settings, mcp_executable: event.target.value })} /><button className="icon-button" onClick={() => void chooseMcpExecutable()} title="选择 chatgpt-delegate Connector" aria-label="选择 MCP Connector"><FolderOpen size={16} /></button></div><div className="capability-note"><StatusDot ready={status.text_editing_available} /><span>{status.connector_capability_message}</span></div><div className="path-edit-row"><span>MCP Proxy</span><input value={settings.proxy_executable} onChange={(event) => setSettings({ ...settings, proxy_executable: event.target.value })} /><button className="icon-button" onClick={() => void chooseProxyExecutable()} title="选择 MCP Proxy" aria-label="选择 MCP Proxy"><FolderOpen size={16} /></button></div><PathRow label="Tunnel" value={settings.tunnel_executable} /><PathRow label="Proxy 配置" value={settings.proxy_config_path} /><PathRow label="Router 注册表" value={settings.router_config_path} /></div></section></>}
        </div>}
      </section>
    </main>
  );
}

function ChainItem({ icon: Icon, index, title, detail, ready }: { icon: typeof Network; index: string; title: string; detail: string; ready: boolean }) { return <div className={`chain-item ${ready ? "is-ready" : ""}`}><span className="chain-index">{index}</span><Icon size={22} strokeWidth={1.6} /><div><strong>{title}</strong><span>{detail}</span></div><StatusDot ready={ready} /></div>; }
function InfraItem({ icon: Icon, title, detail, ready, pid }: { icon: typeof Network; title: string; detail: string; ready: boolean; pid: number | null }) { return <div className="infra-item"><span className="infra-icon"><Icon size={17} /></span><div><strong>{title}<StatusDot ready={ready} /></strong><span>{pid ? `PID ${pid}` : detail}</span></div></div>; }
function InfraCell({ icon: Icon, title, detail, ready }: { icon: typeof Network; title: string; detail: string; ready: boolean }) { return <div className="ic"><div className="icbox"><Icon size={16} /></div><div className="icmeta"><div className="ic-name">{title}<span className={`sdot ${ready ? "running" : "offline"}`} /></div><div className="ic-sub">{detail}</div></div></div>; }
function PathStep({ icon: Icon, number, title, detail, ready }: { icon: typeof Network; number: string; title: string; detail: string; ready: boolean }) { return <div className="path-step"><span className="num">{number}</span><div className="picon"><Icon size={17} /></div><div><div className="pname">{title}</div><div className="psub">{detail}</div></div><span className={`sdot ${ready ? "running" : "offline"}`} /></div>; }
function ProjectListRow({ project, busy, active, onSelect, onStart, onStop }: { project: DelegateStatus["projects"][number]; busy: boolean; active: boolean; onSelect: () => void; onStart: () => void; onStop: () => void }) {
  const running = project.overall === "running" || project.overall === "starting";
  const stateLabel = project.overall === "running" ? "运行中" : project.overall === "starting" ? "启动中" : project.overall === "failed" ? "启动失败" : "已停止";
  const avatarTone = project.project_id === "screencast" ? "project-avatar-red" : project.project_id === "realize" ? "project-avatar-purple" : "";
  return <div className={`lrow ${active ? "active" : ""}`} onClick={onSelect}><div className={`lavatar ${avatarTone}`}>{project.name.slice(0, 1).toUpperCase()}</div><div className="lmain"><div className="lname">{project.name} <span className="lkey">{project.project_id}</span></div><div className="lpath" title={project.output_directory}>{project.output_directory}</div></div><span className="lstate"><span className={`sdot ${project.overall === "failed" ? "error" : running ? "running" : "offline"}`} />{stateLabel}</span><button className="mini-btn" title={running ? "停止" : "启动"} aria-label={running ? `停止${project.name}` : `启动${project.name}`} onClick={(event) => { event.stopPropagation(); running ? onStop() : onStart(); }} disabled={busy}>{busy ? <LoaderCircle className="spin" size={15} /> : running ? <Square size={15} /> : <Play size={15} />}</button><button className="mini-btn" title="更多操作" aria-label={`${project.name} 更多操作`} onClick={(event) => event.stopPropagation()}><MoreHorizontal size={15} /></button></div>;
}
function ProjectStatusRow({ project, busy, onStart, onStop }: { project: DelegateStatus["projects"][number]; busy: boolean; onStart: () => void; onStop: () => void }) {
  const running = project.overall === "running" || project.overall === "starting";
  const stateLabel = project.overall === "running" ? "运行中" : project.overall === "starting" ? "启动中" : project.overall === "failed" ? "启动失败" : "已停止";
  return <article className={`project-status-row ${project.overall}`}>
    <span className="project-avatar">{project.name.slice(0, 1).toUpperCase()}</span><div className="project-row-main"><strong>{project.name}</strong><code>{project.project_id}</code><span title={project.output_directory}>{project.output_directory}</span></div><span className={`state-chip ${project.overall}`}><StatusDot ready={project.mcp_ready} working={project.overall === "starting"} />{stateLabel}</span><button className="icon-button project-power" onClick={running ? onStop : onStart} disabled={busy} title={running ? "停止项目" : "启动项目"} aria-label={running ? `停止${project.name}` : `启动${project.name}`}>{busy ? <LoaderCircle className="spin" size={16} /> : running ? <Square size={16} /> : <Play size={16} />}</button><button className="project-menu-button" title="更多项目操作" aria-label={`${project.name} 更多项目操作`}><MoreHorizontal size={18} /></button>
  </article>;
}
function Field({ label, children }: { label: string; children: React.ReactNode }) { return <label className="field"><span>{label}</span>{children}</label>; }
function PathRow({ label, value }: { label: string; value: string }) { async function copyValue() { await navigator.clipboard.writeText(value); } return <div className="path-row"><span>{label}</span><code>{value}</code><button onClick={copyValue} title="复制路径" aria-label={`复制${label}路径`}><Copy size={15} /></button></div>; }

export default App;
