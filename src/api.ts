import { invoke } from "@tauri-apps/api/core";
import type { AppSettings, DelegateStatus, DoctorReport, LogSource, MagicPortProbe, RuntimeReadiness } from "./types";

export const api = {
  getStatus: () => invoke<DelegateStatus>("get_status"),
  getRuntimeReadiness: () => invoke<RuntimeReadiness>("get_runtime_readiness"),
  checkRuntimeReadiness: (settings: AppSettings) => invoke<RuntimeReadiness>("check_runtime_readiness", { settings }),
  detectMagicPort: (settings: AppSettings) => invoke<MagicPortProbe>("detect_magic_port", { settings }),
  start: () => invoke<DelegateStatus>("start_delegate"),
  stop: () => invoke<DelegateStatus>("stop_delegate"),
  startAllProjects: () => invoke<DelegateStatus>("start_all_projects"),
  stopAllProjects: () => invoke<DelegateStatus>("stop_all_projects"),
  startProject: (projectId: string) => invoke<DelegateStatus>("start_project", { projectId }),
  stopProject: (projectId: string) => invoke<DelegateStatus>("stop_project", { projectId }),
  getSettings: () => invoke<AppSettings>("get_settings"),
  saveSettings: (settings: AppSettings) => invoke<AppSettings>("save_settings", { settings }),
  savePublicBaseUrl: (publicBaseUrl: string) => invoke<string>("save_public_base_url", { publicBaseUrl }),
  runSetupDoctor: () => invoke<DoctorReport>("run_setup_doctor"),
  migrateProjectKey: (oldProjectId: string, newProjectId: string) => invoke<AppSettings>("migrate_project_key", { oldProjectId, newProjectId }),
  saveRuntimeKey: (key: string) => invoke<void>("save_runtime_key", { key }),
  readLogs: (source: LogSource, projectId?: string) => invoke<string>("read_logs", { source, projectId: projectId ?? null }),
  openLogDirectory: () => invoke<void>("open_log_directory"),
};
