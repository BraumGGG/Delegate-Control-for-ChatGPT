import { invoke } from "@tauri-apps/api/core";
import type { AppSettings, DelegateStatus, LogSource } from "./types";

export const api = {
  getStatus: () => invoke<DelegateStatus>("get_status"),
  start: () => invoke<DelegateStatus>("start_delegate"),
  stop: () => invoke<DelegateStatus>("stop_delegate"),
  startAllProjects: () => invoke<DelegateStatus>("start_all_projects"),
  stopAllProjects: () => invoke<DelegateStatus>("stop_all_projects"),
  startProject: (projectId: string) => invoke<DelegateStatus>("start_project", { projectId }),
  stopProject: (projectId: string) => invoke<DelegateStatus>("stop_project", { projectId }),
  getSettings: () => invoke<AppSettings>("get_settings"),
  saveSettings: (settings: AppSettings) => invoke<AppSettings>("save_settings", { settings }),
  migrateProjectKey: (oldProjectId: string, newProjectId: string) => invoke<AppSettings>("migrate_project_key", { oldProjectId, newProjectId }),
  saveRuntimeKey: (key: string) => invoke<void>("save_runtime_key", { key }),
  readLogs: (source: LogSource, projectId?: string) => invoke<string>("read_logs", { source, projectId: projectId ?? null }),
  openLogDirectory: () => invoke<void>("open_log_directory"),
};
