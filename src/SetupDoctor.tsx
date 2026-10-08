import { Check, CircleAlert, Copy, LoaderCircle, RefreshCw, Stethoscope } from "lucide-react";
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

  return <div className="view doctor-view">
    <div className="view-heading">
      <div><span className="eyebrow">SETUP DOCTOR</span><h1>安装诊断</h1><p className="view-subtitle">分别检查本机连接与每个项目；外部 Connector 注册不在本机诊断范围内。</p></div>
      <button className="secondary-button" onClick={onRefresh} disabled={loading}>
        {loading ? <LoaderCircle size={17} className="spin" /> : <RefreshCw size={17} />}重新检查
      </button>
    </div>
    {error && <p className="doctor-error" role="alert">{error}</p>}
    {report && <>
      <div className="doctor-summary">
        <Stethoscope size={22} />
        <div><strong>{report.local_ready ? "本机连接已就绪" : "本机连接仍有待处理项"}</strong><span>本页只检查本机运行依赖、共享链路和项目状态；外部 Connector 注册不在本机诊断范围内。</span></div>
      </div>
      <section className="doctor-section"><div className="doctor-section-head"><h2>共享连接</h2><span>本机配置与进程</span></div><CheckRows checks={shared} /></section>
      <section className="doctor-section"><div className="doctor-section-head"><h2>项目运行</h2><span>{projects.length} 个项目，逐项诊断</span></div><CheckRows checks={projects} /></section>
      {report.registration_endpoint && <section className="doctor-registration"><div className="doctor-section-head"><h2>Public URL</h2><span>仅供外部 Connector 使用</span></div><p>已保存的公网基础地址不会影响本机启动。外部 Connector 注册和真实调用请在 ChatGPT 中单独完成。</p><div className="doctor-endpoint"><code>{report.registration_endpoint}</code><button className="icon-button" onClick={() => onCopy(report.registration_endpoint!)} aria-label="复制 Public URL" title="复制 Public URL"><Copy size={16} /></button></div></section>}
    </>}
  </div>;
}
