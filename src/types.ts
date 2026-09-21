export type OverallState = "stopped" | "starting" | "running" | "stopping" | "failed";

export interface DelegateStatus {
  overall: OverallState;
  proxy_ready: boolean;
  mcp_ready: boolean;
  tunnel_ready: boolean;
  mcp_pid: number | null;
  tunnel_pid: number | null;
  credential_configured: boolean;
  message: string;
}

export interface AppSettings {
  proxy_host: string;
  proxy_port: number;
  mcp_host: string;
  mcp_port: number;
  health_port: number;
  profile_name: string;
  mcp_executable: string;
  tunnel_executable: string;
  output_directory: string;
}

export type ViewId = "overview" | "logs" | "settings";
export type LogSource = "mcp" | "tunnel";
