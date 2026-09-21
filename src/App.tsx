import { useCallback, useEffect, useMemo, useState } from "react";
import {
  Activity,
  ArrowUpRight,
  Check,
  ChevronRight,
  CircleAlert,
  Copy,
  Eye,
  EyeOff,
  FileText,
  FolderOpen,
  KeyRound,
  Link2,
  LoaderCircle,
  MonitorCog,
  Network,
  Power,
  RefreshCw,
  RotateCw,
  Save,
  Server,
  Settings2,
  ShieldCheck,
  SquareTerminal,
  Unplug,
} from "lucide-react";
import { api } from "./api";
import type { AppSettings, DelegateStatus, LogSource, ViewId } from "./types";

const EMPTY_STATUS: DelegateStatus = {
  overall: "stopped",
  proxy_ready: false,
  mcp_ready: false,
  tunnel_ready: false,
  mcp_pid: null,
  tunnel_pid: null,
  credential_configured: false,
  message: "正在读取本机状态",
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
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [logSource, setLogSource] = useState<LogSource>("mcp");
  const [logs, setLogs] = useState("等待日志数据...");
  const [runtimeKey, setRuntimeKey] = useState("");
  const [showKey, setShowKey] = useState(false);

  const refreshStatus = useCallback(async () => {
    try {
      setStatus(await api.getStatus());
    } catch (cause) {
      setError(String(cause));
    }
  }, []);

  const refreshLogs = useCallback(async () => {
    try {
      setLogs(await api.readLogs(logSource));
    } catch (cause) {
      setLogs(`无法读取日志\n${String(cause)}`);
    }
  }, [logSource]);

  useEffect(() => {
    void refreshStatus();
    void api.getSettings().then(setSettings).catch((cause) => setError(String(cause)));
    const timer = window.setInterval(refreshStatus, 1800);
    return () => {
      window.clearInterval(timer);
    };
  }, [refreshStatus]);

  useEffect(() => {
    if (view !== "logs") return;
    void refreshLogs();
    const timer = window.setInterval(refreshLogs, 2500);
    return () => window.clearInterval(timer);
  }, [view, refreshLogs]);

  const copy = useMemo(() => stateCopy(status), [status]);
  const isRunning = status.overall === "running" || status.overall === "starting";

  async function handlePower(action: "start" | "stop" | "restart") {
    if (busy) return;
    setBusy(true);
    setError("");
    setNotice("");
    try {
      if (action === "restart") {
        await api.stop();
        setStatus(await api.start());
      } else {
        setStatus(action === "start" ? await api.start() : await api.stop());
      }
    } catch (cause) {
      setError(String(cause));
      await refreshStatus();
    } finally {
      setBusy(false);
    }
  }

  async function handleSaveSettings() {
    if (!settings) return;
    setBusy(true);
    setError("");
    try {
      setSettings(await api.saveSettings(settings));
      setNotice("设置已保存，将在下次启动连接时生效。");
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  }

  async function handleSaveKey() {
    if (!runtimeKey.trim()) return;
    setBusy(true);
    setError("");
    try {
      await api.saveRuntimeKey(runtimeKey.trim());
      setRuntimeKey("");
      setNotice("Runtime API Key 已写入 Windows 安全存储。");
      await refreshStatus();
    } catch (cause) {
      setError(String(cause));
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className="app-shell">
      <aside className="sidebar">
        <div className="brand-mark" aria-label="Delegate Control for ChatGPT">
          <span className="brand-rail brand-rail-a" />
          <span className="brand-node" />
          <span className="brand-rail brand-rail-b" />
        </div>
        <nav className="nav-list" aria-label="主导航">
          {NAV_ITEMS.map((item) => {
            const Icon = item.icon;
            return (
              <button
                key={item.id}
                className={`nav-button ${view === item.id ? "is-active" : ""}`}
                onClick={() => setView(item.id)}
                title={item.label}
                aria-label={item.label}
              >
                <Icon size={19} strokeWidth={1.8} />
              </button>
            );
          })}
        </nav>
        <div className="sidebar-status" title={status.overall === "running" ? "连接正常" : "当前未连接"}>
          <StatusDot ready={status.overall === "running"} working={status.overall === "starting"} />
        </div>
      </aside>

      <section className="workspace">
        <header className="topbar">
          <div>
            <div className="product-name">Delegate Control for ChatGPT</div>
            <div className="product-context">CHATGPT CONNECTOR CONSOLE</div>
          </div>
          <div className="topbar-actions">
            <button className="icon-button" onClick={refreshStatus} disabled={busy} title="刷新状态" aria-label="刷新状态">
              <RefreshCw size={18} className={busy ? "spin" : ""} />
            </button>
            <div className={`connection-pill ${status.overall}`}>
              <StatusDot ready={status.overall === "running"} working={status.overall === "starting"} />
              {status.overall === "running" ? "ONLINE" : status.overall.toUpperCase()}
            </div>
          </div>
        </header>

        {error && (
          <div className="alert-band error-band" role="alert">
            <CircleAlert size={18} />
            <span>{error}</span>
            <button onClick={() => setError("")} aria-label="关闭错误提示">×</button>
          </div>
        )}
        {notice && (
          <div className="alert-band success-band">
            <Check size={18} />
            <span>{notice}</span>
            <button onClick={() => setNotice("")} aria-label="关闭成功提示">×</button>
          </div>
        )}

        {view === "overview" && (
          <div className="view overview-view">
            <section className={`signal-hero ${copy.tone}`}>
              <div className="signal-copy">
                <div className="eyebrow">{copy.eyebrow}</div>
                <h1>{copy.title}</h1>
                <p>{status.message || "连接仅在你需要时建立，退出程序会自动清理后台进程。"}</p>
                <div className="hero-actions">
                  <button
                    className={`power-button ${isRunning ? "stop" : "start"}`}
                    onClick={() => handlePower(isRunning ? "stop" : "start")}
                    disabled={busy || status.overall === "stopping"}
                  >
                    {busy ? <LoaderCircle className="spin" size={20} /> : isRunning ? <Unplug size={20} /> : <Power size={20} />}
                    {busy ? "处理中" : isRunning ? "停止连接" : "启动连接"}
                  </button>
                  {status.overall === "running" && (
                    <button className="secondary-button" onClick={() => handlePower("restart")} disabled={busy}>
                      <RotateCw size={18} />重新连接
                    </button>
                  )}
                </div>
              </div>
              <div className="signal-visual" aria-hidden>
                <div className="signal-orbit orbit-one" />
                <div className="signal-orbit orbit-two" />
                <div className="signal-core">
                  <Link2 size={34} strokeWidth={1.5} />
                </div>
                <span className="pulse pulse-a" />
                <span className="pulse pulse-b" />
                <span className="pulse pulse-c" />
              </div>
            </section>

            <section className="chain-section">
              <div className="section-heading">
                <div>
                  <span className="eyebrow">CONNECTION PATH</span>
                  <h2>本机链路</h2>
                </div>
                <span className="section-note">127.0.0.1 · PRIVATE LOOPBACK</span>
              </div>
              <div className="chain-grid">
                <ChainItem icon={Network} index="01" title="Clash Proxy" detail={settings ? `${settings.proxy_host}:${settings.proxy_port}` : "127.0.0.1:7897"} ready={status.proxy_ready} />
                <ChevronRight className="chain-arrow" size={18} />
                <ChainItem icon={Server} index="02" title="Local MCP" detail={`PID ${status.mcp_pid ?? "—"}`} ready={status.mcp_ready} />
                <ChevronRight className="chain-arrow" size={18} />
                <ChainItem icon={ShieldCheck} index="03" title="Secure Tunnel" detail={`PID ${status.tunnel_pid ?? "—"}`} ready={status.tunnel_ready} />
              </div>
            </section>

            <section className="detail-strip">
              <div><span>启动模式</span><strong>手动按需</strong></div>
              <div><span>密钥存储</span><strong>{status.credential_configured ? "Windows Credential Manager" : "尚未配置"}</strong></div>
              <div><span>关闭窗口</span><strong>隐藏到系统托盘</strong></div>
            </section>
          </div>
        )}

        {view === "logs" && (
          <div className="view logs-view">
            <div className="view-heading">
              <div><span className="eyebrow">RUNTIME TRACE</span><h1>运行日志</h1></div>
              <button className="secondary-button" onClick={() => void api.openLogDirectory()}><FolderOpen size={18} />打开目录</button>
            </div>
            <div className="segmented-control" role="tablist">
              <button className={logSource === "mcp" ? "is-active" : ""} onClick={() => setLogSource("mcp")}><Server size={16} />MCP Server</button>
              <button className={logSource === "tunnel" ? "is-active" : ""} onClick={() => setLogSource("tunnel")}><ShieldCheck size={16} />Secure Tunnel</button>
            </div>
            <pre className="log-console">{logs}</pre>
            <div className="log-footer"><StatusDot ready={status.overall === "running"} />每 2.5 秒自动刷新<button className="text-button" onClick={refreshLogs}><RefreshCw size={15} />立即刷新</button></div>
          </div>
        )}

        {view === "settings" && settings && (
          <div className="view settings-view">
            <div className="view-heading">
              <div><span className="eyebrow">LOCAL CONFIGURATION</span><h1>连接设置</h1></div>
              <button className="primary-compact" onClick={handleSaveSettings} disabled={busy}><Save size={17} />保存设置</button>
            </div>

            <section className="settings-band">
              <div className="settings-label"><KeyRound size={20} /><div><h2>Runtime API Key</h2><p>凭据由当前 Windows 账户加密保管。</p></div></div>
              <div className="key-editor">
                <div className="input-with-icon">
                  <input type={showKey ? "text" : "password"} value={runtimeKey} onChange={(event) => setRuntimeKey(event.target.value)} placeholder={status.credential_configured ? "已安全保存，输入新密钥可覆盖" : "粘贴 Runtime API Key"} />
                  <button onClick={() => setShowKey((value) => !value)} title={showKey ? "隐藏密钥" : "显示密钥"} aria-label={showKey ? "隐藏密钥" : "显示密钥"}>{showKey ? <EyeOff size={17} /> : <Eye size={17} />}</button>
                </div>
                <button className="secondary-button" onClick={handleSaveKey} disabled={!runtimeKey.trim() || busy}><ShieldCheck size={17} />安全保存</button>
              </div>
            </section>

            <section className="settings-band settings-grid-band">
              <div className="settings-label"><Network size={20} /><div><h2>网络与端口</h2><p>修改前请确认没有其他程序占用端口。</p></div></div>
              <div className="form-grid">
                <Field label="代理主机"><input value={settings.proxy_host} onChange={(e) => setSettings({ ...settings, proxy_host: e.target.value })} /></Field>
                <Field label="代理端口"><input type="number" value={settings.proxy_port} onChange={(e) => setSettings({ ...settings, proxy_port: Number(e.target.value) })} /></Field>
                <Field label="MCP 端口"><input type="number" value={settings.mcp_port} onChange={(e) => setSettings({ ...settings, mcp_port: Number(e.target.value) })} /></Field>
                <Field label="健康端口"><input type="number" value={settings.health_port} onChange={(e) => setSettings({ ...settings, health_port: Number(e.target.value) })} /></Field>
              </div>
            </section>

            <section className="settings-band paths-band">
              <div className="settings-label"><MonitorCog size={20} /><div><h2>本机程序</h2><p>客户端直接管理已安装的 MCP 与 Tunnel。</p></div></div>
              <div className="path-list">
                <PathRow label="MCP" value={settings.mcp_executable} />
                <PathRow label="Tunnel" value={settings.tunnel_executable} />
                <PathRow label="结果目录" value={settings.output_directory} />
              </div>
            </section>
          </div>
        )}
      </section>
    </main>
  );
}

function ChainItem({ icon: Icon, index, title, detail, ready }: { icon: typeof Network; index: string; title: string; detail: string; ready: boolean }) {
  return (
    <div className={`chain-item ${ready ? "is-ready" : ""}`}>
      <span className="chain-index">{index}</span>
      <Icon size={22} strokeWidth={1.6} />
      <div><strong>{title}</strong><span>{detail}</span></div>
      <StatusDot ready={ready} />
    </div>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return <label className="field"><span>{label}</span>{children}</label>;
}

function PathRow({ label, value }: { label: string; value: string }) {
  async function copyValue() {
    await navigator.clipboard.writeText(value);
  }
  return <div className="path-row"><span>{label}</span><code>{value}</code><button onClick={copyValue} title="复制路径" aria-label={`复制${label}路径`}><Copy size={15} /></button></div>;
}

export default App;
