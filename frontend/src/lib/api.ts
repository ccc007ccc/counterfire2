import { invoke } from "@tauri-apps/api/core";
import type { AppConfig, RuntimeStatus } from "./types";

export const api = {
  getConfig: (): Promise<AppConfig> => invoke("get_config"),
  updateConfig: (config: AppConfig): Promise<AppConfig> =>
    invoke("update_config", { config }),
  startService: (): Promise<void> => invoke("start_service"),
  stopService: (): Promise<void> => invoke("stop_service"),
  runtimeStatus: (): Promise<RuntimeStatus> => invoke("runtime_status"),
  demoOnce: (event: string): Promise<void> => invoke("demo_once", { event }),
};
