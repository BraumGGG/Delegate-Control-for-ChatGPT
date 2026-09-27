export type OverallState = "stopped" | "starting" | "running" | "stopping" | "failed";

export interface ProjectConfig {
  id: string;
  name: string;
  output_directory: string;
  mcp_host: string;
  mcp_port: number;
  enabled: boolean;
}

export interface AppSettings {
  proxy_host: string;
  proxy_port: number;
  mcp_proxy_host: string;
  mcp_proxy_port: number;
  router_port: number;
  health_host: string;
  health_port: number;
  profile_name: string;
  mcp_executable: string;
  proxy_executable: string;
  proxy_config_path: string;
  router_config_path: string;
  tunnel_executable: string;
  projects: ProjectConfig[];
  active_project_id: string | null;
}

export interface ProjectRuntimeStatus {
  project_id: string;
  name: string;
  output_directory: string;
  overall: OverallState;
  mcp_ready: boolean;
  mcp_pid: number | null;
  message: string;
}

export interface DelegateStatus {
  overall: OverallState;
  proxy_ready: boolean;
  router_ready: boolean;
  mcp_proxy_ready: boolean;
  mcp_ready: boolean;
  tunnel_ready: boolean;
  proxy_pid: number | null;
  router_pid: number | null;
  mcp_pid: number | null;
  tunnel_pid: number | null;
  credential_configured: boolean;
  text_editing_available: boolean;
  connector_capability_message: string;
  message: string;
  projects: ProjectRuntimeStatus[];
}

export type ViewId = "overview" | "projects" | "logs" | "settings";
export type LogSource = "mcp" | "router" | "proxy" | "tunnel";
