import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppSettings,
  Download,
  StartDownloadRequest,
  UpdateSettingsRequest,
} from "./types";

export const orangeApi = {
  startDownload(request: StartDownloadRequest) {
    return invoke<Download>("start_download", { request });
  },
  pauseDownload(id: string) {
    return invoke<Download>("pause_download", { id });
  },
  resumeDownload(id: string) {
    return invoke<Download>("resume_download", { id });
  },
  cancelDownload(id: string) {
    return invoke<Download>("cancel_download", { id });
  },
  deleteDownload(id: string) {
    return invoke<string>("delete_download", { id });
  },
  listDownloads() {
    return invoke<Download[]>("list_downloads");
  },
  getSettings() {
    return invoke<AppSettings>("get_settings");
  },
  updateSettings(request: UpdateSettingsRequest) {
    return invoke<AppSettings>("update_settings", { request });
  },
};

export function onDownloadProgress(handler: (download: Download) => void): Promise<UnlistenFn> {
  return listen<Download>("download-progress", (event) => handler(event.payload));
}

export function onDownloadFinished(handler: (download: Download) => void): Promise<UnlistenFn> {
  return listen<Download>("download-finished", (event) => handler(event.payload));
}
