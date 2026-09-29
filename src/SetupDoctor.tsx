import { Check, CircleAlert, Copy, ExternalLink, LoaderCircle, RefreshCw, Stethoscope } from "lucide-react";
import type { DoctorCheck, DoctorReport } from "./types";

const stateText = { ready: "就绪", action: "需处理", pending: "待检查" };

function CheckRows({ checks }: { checks: DoctorCheck[] }) {
  return <div className="doctor-checks">{checks.map((item) =>
    <div className="doctor-row" key={item.id}>
      <span className={`doctor-indicator ${item.state}`} aria-label={stateText[item.state]}>
        {item.state === "ready" ? <Check size={16} /> : <CircleAlert size={16} />}
      </span>
      <div className="doctor-row-copy">
        <div className="doctor-row-head"><strong>{item.label}</strong>{item.project_id && <code>{item.project_id}</code>}</div>
        <p>{item.detail}</p>
        {item.state !== "ready" && <small>{item.action}</small>}
      </div>
      <span className={`doctor-state ${item.state}`}>{stateText[item.state]}</span>
    </div>
  )}</div>;
}

export function SetupDoctor({
  report, loading, error, onRefresh, onCopy, onSettings,
}: {
  report: DoctorReport | null;
  loading: boolean;
  error: string;
  onRefresh: () => void;
  onCopy: (value: string) => void;
  onSettings: () => void;
}) {
  const shared = report?.checks.filter((item) => item.scope === "shared") ?? [];
  const projects = report?.checks.filter((item) => item.scope === "project") ?? [];
  const registration = report?.checks.filter((item) => item.scope === "registration") ?? [];

  return <div className="view doctor-view">
    <div className="view-heading">
      <div><span className="eyebrow">SETUP DOCTOR</span><h1>安装诊断</h1><p className="view-subtitle">分别检查本机连接、每个项目与 ChatGPT 注册准备。</p></div>
      <button className="secondary-button" onClick={onRefresh} disabled={loading}>
        {loading ? <LoaderCircle size={17} className="spin" /> : <RefreshCw size={17} />}重新检查
      </button>
    </div>
    {error && <p className="doctor-error" role="alert">{error}</p>}
    {report && <>
      <div className="doctor-summary">
        <Stethoscope size={22} />
        <div><strong>{report.local_ready ? "本机连接已就绪" : "本机连接仍有待处理项"}</strong><span>{report.registration_ready ? "公网 MCP 端点检查通过；仍需在 ChatGPT 中注册。" : "公网地址与 ChatGPT 注册单独检查，不影响本机连接结论。"}</span></div>
      </div>
      <section className="doctor-section"><div className="doctor-section-head"><h2>共享连接</h2><span>本机配置与进程</span></div><CheckRows checks={shared} /></section>
      <section className="doctor-section"><div className="doctor-section-head"><h2>项目运行</h2><span>{projects.length} 个项目，逐项诊断</span></div><CheckRows checks={projects} /></section>
      <section className="doctor-section"><div className="doctor-section-head"><h2>ChatGPT 注册准备</h2><span>公网地址与协议检查</span></div><CheckRows checks={registration} /></section>
      <section className="doctor-registration">
        <div className="doctor-section-head"><h2>在 ChatGPT 中注册</h2><span>由用户在 ChatGPT 完成</span></div>
        <p>DCFC 本地运行组件不是 ChatGPT 中的外部应用。请先配置 Public URL，再使用下方派生的 MCP 根路由地址注册。</p>
        {report.registration_endpoint ? <div className="doctor-endpoint"><code>{report.registration_endpoint}</code><button className="icon-button" onClick={() => onCopy(report.registration_endpoint!)} aria-label="复制注册地址" title="复制注册地址"><Copy size={16} /></button></div> : <button className="secondary-button" onClick={onSettings}><ExternalLink size={16} />设置 Public URL</button>}
        <ol><li>确认本机连接与目标项目已启动，诊断中没有共享链路故障。</li><li>在 ChatGPT 的应用/连接器设置中新增自定义 MCP 应用，填写上方地址并完成外部授权。</li><li>在 ChatGPT 内核对可用工具；工具清单变更后按 ChatGPT 当前界面重新同步，DCFC 无法强制刷新外部 Schema。</li></ol>
        <p className="doctor-caveat">本机 HTTPS 检查不等于从 ChatGPT 网络完成端到端调用；注册结果需要在 ChatGPT 中验证。</p>
      </section>
    </>}
  </div>;
}
