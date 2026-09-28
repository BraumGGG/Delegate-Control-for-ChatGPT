import { useCallback, useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { Activity, Check, ChevronRight, CircleAlert, Copy, Eye, EyeOff, Folder, FolderOpen, Grid2X2, KeyRound, Link2, LoaderCircle, MonitorCog, MoreHorizontal, Network, Play, Plus, Power, RefreshCw, RotateCw, Save, Search, Server, Settings2, ShieldCheck, Square, SquareTerminal, Trash2, Unplug } from "lucide-react";
import { api } from "./api";
import { getLegacyProjectKeyProposal, PROJECT_KEY_PATTERN, suggestProjectKey, validateProjectKeyCandidate } from "./projectIdentity";
import type { AppSettings, DelegateStatus, LogSource, ProjectConfig, ProjectRuntimeStatus, RuntimeReadiness, ViewId } from "./types";

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
type WizardStep = 0 | 1 | 2 | 3 | 4;
type WizardDraft = {
  step: WizardStep;
  tunnelId: string;
  magicPort: number;
  projectName: string;
  outputDirectory: string;
  proxyExecutable: string;
  tunnelExecutable: string;
};

const WIZARD_DRAFT_KEY = "dcfc.first-run-wizard.v1";
const DEFAULT_WIZARD_DRAFT: WizardDraft = {
  step: 0,
  tunnelId: "",
  magicPort: 7877,
  projectName: "",
  outputDirectory: "",
  proxyExecutable: "",
  tunnelExecutable: "",
};

function readWizardDraft(): WizardDraft {
  try {
    const parsed = JSON.parse(window.localStorage.getItem(WIZARD_DRAFT_KEY) ?? "null") as Partial<WizardDraft> | null;
    if (!parsed) return DEFAULT_WIZARD_DRAFT;
    return {
      step: [0, 1, 2, 3, 4].includes(Number(parsed.step)) ? Number(parsed.step) as WizardStep : 0,
      tunnelId: typeof parsed.tunnelId === "string" ? parsed.tunnelId : "",
      magicPort: Number.isInteger(parsed.magicPort) ? Number(parsed.magicPort) : 7877,
      projectName: typeof parsed.projectName === "string" ? parsed.projectName : "",
      outputDirectory: typeof parsed.outputDirectory === "string" ? parsed.outputDirectory : "",
      proxyExecutable: typeof parsed.proxyExecutable === "string" ? parsed.proxyExecutable : "",
      tunnelExecutable: typeof parsed.tunnelExecutable === "string" ? parsed.tunnelExecutable : "",
    };
  } catch {
    return DEFAULT_WIZARD_DRAFT;
  }
}

function isAbsoluteWindowsPath(value: string) {
  return /^[A-Za-z]:[\\/]/.test(value.trim()) || /^\\\\[^\\]/.test(value.trim());
}

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
  const [runtimeReadiness, setRuntimeReadiness] = useState<RuntimeReadiness | null>(null);
  const [wizardDraft, setWizardDraft] = useState<WizardDraft>(DEFAULT_WIZARD_DRAFT);
  const [wizardVisible, setWizardVisible] = useState(false);
  const [wizardRuntimeKey, setWizardRuntimeKey] = useState("");
  const [wizardError, setWizardError] = useState("");

  const refreshStatus = useCallback(async () => {
    try { setStatus(await api.getStatus()); } catch (cause) { setError(String(cause)); }
  }, []);

  const refreshRuntimeReadiness = useCallback(async () => {
    try { setRuntimeReadiness(await api.getRuntimeReadiness()); } catch (cause) { setError(String(cause)); }
  }, []);

  const refreshLogs = useCallback(async () => {
    try { setLogs(await api.readLogs(logSource, logSource === "mcp" ? logProjectId ?? undefined : undefined)); } catch (cause) { setLogs(`无法读取日志\n${String(cause)}`); }
  }, [logProjectId, logSource]);

  useEffect(() => {
    void refreshStatus();
    void refreshRuntimeReadiness();
    void api.getSettings().then((loaded) => {
      setSettings(loaded);
      setSavedSettings(loaded);
      setSelectedProjectId(loaded.active_project_id ?? loaded.projects[0]?.id ?? null);
      setLogProjectId(loaded.projects[0]?.id ?? null);
      if (loaded.projects.length === 0) {
        const draft = readWizardDraft();
        const restored = {
          ...draft,
          magicPort: draft.magicPort || loaded.proxy_port || 7877,
          proxyExecutable: draft.proxyExecutable || loaded.proxy_executable,
          tunnelExecutable: draft.tunnelExecutable || loaded.tunnel_executable,
        };
        const current = { ...loaded, proxy_executable: restored.proxyExecutable, tunnel_executable: restored.tunnelExecutable };
        setSettings(current);
        setWizardDraft(restored);
        void api.checkRuntimeReadiness(current).then(setRuntimeReadiness).catch((cause) => setWizardError(String(cause)));
        setWizardVisible(true);
      }
    }).catch((cause) => setError(String(cause)));
    const timer = window.setInterval(refreshStatus, 1800);
    return () => window.clearInterval(timer);
  }, [refreshRuntimeReadiness, refreshStatus]);

  useEffect(() => {
    if (!settings || settings.projects.length > 0 || !wizardVisible) return;
    window.localStorage.setItem(WIZARD_DRAFT_KEY, JSON.stringify(wizardDraft));
  }, [settings, wizardDraft, wizardVisible]);

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
  const wizardProjectKey = suggestProjectKey(wizardDraft.projectName || "project", []);
  const wizardProjectPort = settings ? nextProjectPort(settings) : 8000;
  const wizardDependenciesReady = runtimeReadiness ? runtimeReadiness.mcp_available && runtimeReadiness.proxy_available && runtimeReadiness.tunnel_available : false;
  const wizardCredentialReady = Boolean(runtimeReadiness?.credential_configured || status.credential_configured || wizardRuntimeKey.trim());
  const wizardMagicValid = Number.isInteger(wizardDraft.magicPort) && wizardDraft.magicPort > 0 && wizardDraft.magicPort <= 65535;
  const wizardProjectValid = wizardDraft.projectName.trim().length > 0 && isAbsoluteWindowsPath(wizardDraft.outputDirectory);

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

  function updateWizardDraft(patch: Partial<WizardDraft>) {
    setWizardDraft((current) => ({ ...current, ...patch }));
    setWizardError("");
  }

  function updateWizardSettings(patch: Partial<AppSettings>) {
    if (!settings) return;
    const next = { ...settings, ...patch };
    setSettings(next);
    if (patch.proxy_executable !== undefined) updateWizardDraft({ proxyExecutable: patch.proxy_executable });
    if (patch.tunnel_executable !== undefined) updateWizardDraft({ tunnelExecutable: patch.tunnel_executable });
    void api.checkRuntimeReadiness(next).then(setRuntimeReadiness).catch((cause) => setWizardError(String(cause)));
  }

  async function chooseWizardDirectory() {
    const selected = await open({ directory: true, multiple: false, title: "选择首个项目的文档输出目录" });
    if (typeof selected === "string") updateWizardDraft({ outputDirectory: selected });
  }

  async function chooseWizardExecutable(kind: "proxy" | "tunnel") {
    const selected = await open({ directory: false, multiple: false, title: kind === "proxy" ? "选择 mcp-proxy.exe" : "选择 tunnel-client.exe" });
    if (typeof selected !== "string") return;
    updateWizardSettings(kind === "proxy" ? { proxy_executable: selected } : { tunnel_executable: selected });
  }

  async function advanceWizard() {
    if (!settings) return;
    setWizardError("");
    if (wizardDraft.step === 0 && !wizardDependenciesReady) {
      setWizardError("内部运行组件未安装完整，或外部 MCP Proxy / Tunnel Client 路径不可用。");
      return;
    }
    if (wizardDraft.step === 1) {
      if (!wizardCredentialReady) {
        setWizardError("请输入 Runtime API Key，或先确认已有密钥已安全保存。");
        return;
      }
      if (wizardRuntimeKey.trim()) {
        try {
          await api.saveRuntimeKey(wizardRuntimeKey.trim());
          setWizardRuntimeKey("");
          await refreshStatus();
          await refreshRuntimeReadiness();
        } catch (cause) {
          setWizardError(String(cause));
          return;
        }
      }
    }
    if (wizardDraft.step === 2 && (!wizardDraft.tunnelId.trim() || !wizardMagicValid)) {
      setWizardError("Tunnel ID 不能为空，Magic 端口必须是 1 到 65535 的整数。");
      return;
    }
    if (wizardDraft.step === 3 && !wizardProjectValid) {
      setWizardError("请输入项目名称，并选择 Windows 绝对路径作为文档输出目录。");
      return;
    }
    updateWizardDraft({ step: Math.min(4, wizardDraft.step + 1) as WizardStep });
  }

  function goBackWizard() {
    updateWizardDraft({ step: Math.max(0, wizardDraft.step - 1) as WizardStep });
  }

  async function finishWizard() {
    if (!settings) return;
    if (!wizardCredentialReady || !wizardDependenciesReady || !wizardMagicValid || !wizardProjectValid || !wizardDraft.tunnelId.trim()) {
      setWizardError("请完成所有本机配置检查后再完成向导。");
      return;
    }
    setBusy(true); setWizardError(""); setError("");
    try {
      const project: ProjectConfig = {
        id: wizardProjectKey,
        name: wizardDraft.projectName.trim(),
        output_directory: wizardDraft.outputDirectory.trim(),
        mcp_host: "127.0.0.1",
        mcp_port: wizardProjectPort,
        enabled: true,
      };
      const nextSettings: AppSettings = {
        ...settings,
        proxy_host: "127.0.0.1",
        proxy_port: wizardDraft.magicPort,
        mcp_proxy_host: "127.0.0.1",
        health_host: "127.0.0.1",
        profile_name: "",
        tunnel_id: wizardDraft.tunnelId.trim(),
        projects: [project],
        active_project_id: project.id,
      };
      const saved = await api.saveSettings(nextSettings);
      setSettings(saved);
      setSavedSettings(saved);
      setSelectedProjectId(project.id);
      setLogProjectId(project.id);
      setWizardVisible(false);
      window.localStorage.removeItem(WIZARD_DRAFT_KEY);
      setNotice("本机配置已保存。向导只确认了本地配置，尚未验证端到端连接；现在可以启动项目。");
      await refreshRuntimeReadiness();
    } catch (cause) {
      setWizardError(String(cause));
    } finally {
      setBusy(false);
    }
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
        {settings && settings.projects.length === 0 && wizardVisible && <OnboardingWizard
          draft={wizardDraft}
          settings={settings}
          readiness={runtimeReadiness}
          status={status}
          runtimeKey={wizardRuntimeKey}
          error={wizardError}
          busy={busy}
          projectKey={wizardProjectKey}
          projectPort={wizardProjectPort}
          onDraftChange={updateWizardDraft}
          onSettingsChange={updateWizardSettings}
          onRuntimeKeyChange={setWizardRuntimeKey}
          onNext={() => void advanceWizard()}
          onBack={goBackWizard}
          onFinish={() => void finishWizard()}
          onCancel={() => setWizardVisible(false)}
          onChooseDirectory={() => void chooseWizardDirectory()}
          onChooseExecutable={(kind) => void chooseWizardExecutable(kind)}
        />}
        {settings && settings.projects.length === 0 && !wizardVisible && <div className="onboarding-resume">
          <div className="onboarding-resume-card">
            <div className="wizard-kicker">FIRST RUN SETUP</div>
            <h1>完成本机配置后再开始连接</h1>
            <p>当前还没有项目配置。已有配置不会被覆盖，也不会自动生成“默认项目”。</p>
            <button className="btn btn-primary" onClick={() => setWizardVisible(true)}><RotateCw size={17} />继续配置</button>
          </div>
        </div>}

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

          <section className="settings-band settings-grid-band"><div className="settings-label"><Network size={20} /><div><h2>共享网络与端口</h2><p>固定 Router 工具目录，项目通过 project_id 参数路由。</p></div></div><div className="form-grid"><Field label="Magic 主机"><input value={settings.proxy_host} onChange={(event) => setSettings({ ...settings, proxy_host: event.target.value })} /></Field><Field label="Magic 端口"><input type="number" value={settings.proxy_port} onChange={(event) => setSettings({ ...settings, proxy_port: Number(event.target.value) })} /></Field><Field label="Router 端口"><input type="number" value={settings.router_port} onChange={(event) => setSettings({ ...settings, router_port: Number(event.target.value) })} /></Field><Field label="MCP Proxy 端口"><input type="number" value={settings.mcp_proxy_port} onChange={(event) => setSettings({ ...settings, mcp_proxy_port: Number(event.target.value) })} /></Field><Field label="健康端口"><input type="number" value={settings.health_port} onChange={(event) => setSettings({ ...settings, health_port: Number(event.target.value) })} /></Field></div></section>

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

function nextProjectPort(settings: AppSettings) {
  const used = new Set(settings.projects.map((project) => project.mcp_port));
  let port = 8000;
  while (used.has(port) || port === settings.mcp_proxy_port || port === settings.router_port || port === settings.health_port) port += 1;
  return port;
}

function OnboardingWizard({
  draft,
  settings,
  readiness,
  status,
  runtimeKey,
  error,
  busy,
  projectKey,
  projectPort,
  onDraftChange,
  onSettingsChange,
  onRuntimeKeyChange,
  onNext,
  onBack,
  onFinish,
  onCancel,
  onChooseDirectory,
  onChooseExecutable,
}: {
  draft: WizardDraft;
  settings: AppSettings;
  readiness: RuntimeReadiness | null;
  status: DelegateStatus;
  runtimeKey: string;
  error: string;
  busy: boolean;
  projectKey: string;
  projectPort: number;
  onDraftChange: (patch: Partial<WizardDraft>) => void;
  onSettingsChange: (patch: Partial<AppSettings>) => void;
  onRuntimeKeyChange: (value: string) => void;
  onNext: () => void;
  onBack: () => void;
  onFinish: () => void;
  onCancel: () => void;
  onChooseDirectory: () => void;
  onChooseExecutable: (kind: "proxy" | "tunnel") => void;
}) {
  const steps = [
    ["01", "运行依赖", "确认本机程序"],
    ["02", "OpenAI 连接", "安全配置凭据"],
    ["03", "Magic", "固定本地代理"],
    ["04", "首个项目", "创建真实项目"],
    ["05", "完成检查", "确认本地配置"],
  ];
  const dependencyItems = [
    ["MCP Proxy", "proxy", settings.proxy_executable, readiness?.proxy_available ?? false, (value: string) => onSettingsChange({ proxy_executable: value })],
    ["Tunnel Client", "tunnel", settings.tunnel_executable, readiness?.tunnel_available ?? false, (value: string) => onSettingsChange({ tunnel_executable: value })],
  ] as const;
  const credentialReady = Boolean(readiness?.credential_configured || status.credential_configured || runtimeKey.trim());
  const depsReady = Boolean(readiness?.mcp_available && readiness.proxy_available && readiness.tunnel_available);
  const projectValid = draft.projectName.trim().length > 0 && isAbsoluteWindowsPath(draft.outputDirectory);

  return <div className="wizard-overlay" role="dialog" aria-modal="true" aria-labelledby="wizard-title">
    <div className="wizard-shell">
      <header className="wizard-header">
        <div className="wizard-brand"><div className="brand-mark"><Activity size={18} /></div><div><div className="wizard-kicker">DELEGATE CONTROL</div><strong>首次运行配置</strong></div></div>
        <button className="icon-button" onClick={onCancel} title="稍后配置" aria-label="稍后配置"><span aria-hidden>×</span></button>
      </header>
      <div className="wizard-progress" aria-label="配置进度">{steps.map(([number, label, detail], index) => <div className={`wizard-step ${draft.step === index ? "is-current" : ""} ${draft.step > index ? "is-complete" : ""}`} key={number}><span>{draft.step > index ? "✓" : number}</span><div><strong>{label}</strong><small>{detail}</small></div></div>)}</div>
      <section className="wizard-body">
        <div className="wizard-copy"><div className="wizard-kicker">STEP {String(draft.step + 1).padStart(2, "0")} / 05</div><h1 id="wizard-title">{steps[draft.step][1]}</h1><p>{draft.step === 0 ? "DCFC 检查内置运行组件是否完整；MCP Proxy 与 Tunnel Client 由你提供。" : draft.step === 1 ? "Runtime API Key 只写入当前 Windows 账户的安全存储，向导不会显示或保存密钥内容。" : draft.step === 2 ? "Magic 仅作为本机回环代理使用。这里配置端口和用户可见的 Tunnel ID。" : draft.step === 3 ? "创建第一个真实项目。项目 key 与 MCP 端口由 DCFC 自动生成，后续可在项目管理中查看。" : "最后一步只确认本地配置是否完整，不代表已经完成端到端连接。"}</p></div>
        {draft.step === 0 && <div className="wizard-panel"><div className="wizard-panel-title"><MonitorCog size={19} /><span>运行依赖就绪</span><span className={`wizard-check ${depsReady ? "ready" : ""}`}>{depsReady ? "已就绪" : "待处理"}</span></div><div className="wizard-dependency-list"><div className="wizard-dependency"><div className="wizard-dependency-meta"><span>DCFC 内置运行组件</span><code>{readiness?.mcp_available ? "安装完整" : "未找到；请重新安装 DCFC"}</code></div><span className={`wizard-check ${readiness?.mcp_available ? "ready" : ""}`}>{readiness?.mcp_available ? "可用" : "缺失"}</span></div>{dependencyItems.map(([label, kind, value, ready, onChange]) => <div className="wizard-dependency" key={label}><div className="wizard-dependency-meta"><span>{label}</span><code>{value || "尚未指定"}</code></div><span className={`wizard-check ${ready ? "ready" : ""}`}>{ready ? "可用" : "未找到"}</span><div className="wizard-path-input"><input value={value} onChange={(event) => onChange(event.target.value)} aria-label={`${label}路径`} /><button className="icon-button" onClick={() => onChooseExecutable(kind)} title={`选择${label}`} aria-label={`选择${label}`}><FolderOpen size={16} /></button></div></div>)}</div><p className="wizard-hint">外部程序继续由用户提供。选择文件后会重新检查路径；DCFC 内置组件不需要手工配置。</p></div>}
        {draft.step === 1 && <div className="wizard-panel"><div className="wizard-panel-title"><KeyRound size={19} /><span>OpenAI 连接</span><span className={`wizard-check ${credentialReady ? "ready" : ""}`}>{credentialReady ? "已配置" : "待配置"}</span></div><label className="wizard-field"><span>Runtime API Key</span><input type="password" value={runtimeKey} onChange={(event) => onRuntimeKeyChange(event.target.value)} placeholder={credentialReady ? "已安全保存；如需更换可输入新密钥" : "粘贴 Runtime API Key"} autoComplete="off" /></label><div className="wizard-safe-note"><ShieldCheck size={17} /><span>密钥只发送到 Windows Credential Manager。向导草稿、本地设置和日志中都不会保存密钥。</span></div></div>}
        {draft.step === 2 && <div className="wizard-panel"><div className="wizard-panel-title"><Network size={19} /><span>Magic 与 Tunnel</span><span className={`wizard-check ${draft.tunnelId.trim() && Number.isInteger(draft.magicPort) && draft.magicPort > 0 && draft.magicPort <= 65535 ? "ready" : ""}`}>{draft.tunnelId.trim() && Number.isInteger(draft.magicPort) && draft.magicPort > 0 && draft.magicPort <= 65535 ? "可继续" : "待填写"}</span></div><div className="wizard-form-grid"><label className="wizard-field"><span>Tunnel ID</span><input value={draft.tunnelId} onChange={(event) => onDraftChange({ tunnelId: event.target.value })} placeholder="例如：my-team-tunnel" /></label><label className="wizard-field"><span>Magic 端口</span><input type="number" min={1} max={65535} value={draft.magicPort} onChange={(event) => onDraftChange({ magicPort: Number(event.target.value) })} /></label></div><div className="wizard-fixed-row"><span>Magic 主机</span><code>127.0.0.1</code><small>固定 loopback，不对外监听</small></div></div>}
        {draft.step === 3 && <div className="wizard-panel"><div className="wizard-panel-title"><Plus size={19} /><span>创建首个项目</span><span className={`wizard-check ${projectValid ? "ready" : ""}`}>{projectValid ? "可继续" : "待填写"}</span></div><div className="wizard-form-grid"><label className="wizard-field"><span>项目名称</span><input value={draft.projectName} onChange={(event) => onDraftChange({ projectName: event.target.value })} placeholder="例如：我的项目" /></label><label className="wizard-field"><span>自动生成的 Project Key</span><input value={projectKey} readOnly aria-readonly /></label></div><label className="wizard-field"><span>文档输出目录</span><div className="wizard-path-input"><input value={draft.outputDirectory} onChange={(event) => onDraftChange({ outputDirectory: event.target.value })} placeholder="选择 Windows 绝对路径" /><button className="icon-button" onClick={onChooseDirectory} title="选择输出目录" aria-label="选择输出目录"><FolderOpen size={17} /></button></div></label><div className="wizard-summary-line"><span>MCP 端口</span><code>127.0.0.1:{projectPort}</code><span>启用状态</span><b>创建后可启动</b></div></div>}
        {draft.step === 4 && <div className="wizard-panel"><div className="wizard-panel-title"><Check size={19} /><span>本地配置检查</span><span className="wizard-check ready">本地可保存</span></div><div className="wizard-review-list"><ReviewLine label="运行依赖" value={depsReady ? "内置组件及外部程序路径可用" : "仍有依赖未就绪"} ready={depsReady} /><ReviewLine label="Runtime API Key" value={credentialReady ? "已配置到安全存储" : "尚未配置"} ready={credentialReady} /><ReviewLine label="Magic" value={`127.0.0.1:${draft.magicPort}`} ready={draft.magicPort > 0 && draft.magicPort <= 65535} /><ReviewLine label="Tunnel ID" value={draft.tunnelId || "尚未填写"} ready={Boolean(draft.tunnelId.trim())} /><ReviewLine label="首个项目" value={`${draft.projectName || "尚未填写"} · ${projectKey}`} ready={projectValid} /></div><p className="wizard-hint">完成后会保存一个真实项目配置，不会创建“默认项目”。连接是否能端到端建立，请在返回控制台后手动启动并查看状态。</p></div>}
        {error && <div className="wizard-error" role="alert"><CircleAlert size={16} />{error}</div>}
      </section>
      <footer className="wizard-footer"><button className="secondary-button" onClick={onCancel}>稍后配置</button><div className="wizard-footer-actions">{draft.step > 0 && <button className="secondary-button" onClick={onBack} disabled={busy}>上一步</button>}{draft.step < 4 ? <button className="btn btn-primary" onClick={onNext} disabled={busy}>{busy ? <LoaderCircle size={16} className="spin" /> : <ChevronRight size={16} />}下一步</button> : <button className="btn btn-primary" onClick={onFinish} disabled={busy}><Save size={16} />保存本地配置</button>}</div></footer>
    </div>
  </div>;
}

function ReviewLine({ label, value, ready }: { label: string; value: string; ready: boolean }) {
  return <div className="wizard-review-line"><span>{label}</span><code>{value}</code><span className={`wizard-review-status ${ready ? "ready" : ""}`}>{ready ? "通过" : "待处理"}</span></div>;
}

export default App;
