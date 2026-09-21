import { invoke } from "@tauri-apps/api/core";
import type { AppSettings, DelegateStatus, LogSource } from "./types";

export const api = {
  getStatus: () => invoke<DelegateStatus>("get_status"),
  start: () => invoke<DelegateStatus>("start_delegate"),
  stop: () => invoke<DelegateStatus>("stop_delegate"),
  getSettings: () => invoke<AppSettings>("get_settings"),
  saveSettings: (settings: AppSettings) => invoke<AppSettings>("save_settings", { settings }),
  saveRuntimeKey: (key: string) => invoke<void>("save_runtime_key", { key }),
  readLogs: (source: LogSource) => invoke<string>("read_logs", { source }),
  openLogDirectory: () => invoke<void>("open_log_directory"),
};
