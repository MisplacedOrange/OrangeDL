import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppSettings,
  ChecksumResult,
  Download,
  ExecutorSummary,
  PreflightResult,
  StartDownloadRequest,
  UpdateCheckResult,
  UpdateDownloadOptionsRequest,
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
  pauseAllDownloads() {
    return invoke<string[]>("pause_all_downloads");
  },
  resumeAllDownloads() {
    return invoke<string[]>("resume_all_downloads");
  },
  retryFailedDownloads() {
    return invoke<string[]>("retry_failed_downloads");
  },
  clearCompletedDownloads() {
    return invoke<string[]>("clear_completed_downloads");
  },
  clearCancelledDownloads() {
    return invoke<string[]>("clear_cancelled_downloads");
  },
  clearFailedDownloads() {
    return invoke<string[]>("clear_failed_downloads");
  },
  reorderDownload(id: string, position: number) {
    return invoke<Download>("reorder_download", { id, position });
  },
  updateDownloadOptions(request: UpdateDownloadOptionsRequest) {
    return invoke<Download>("update_download_options", { request });
  },
  getExecutorSummary() {
    return invoke<ExecutorSummary>("get_executor_summary");
  },
  preflightCheck(url: string) {
    return invoke<PreflightResult>("preflight_check", { url });
  },
  openFile(id: string) {
    return invoke<void>("open_file", { id });
  },
  revealInExplorer(id: string) {
    return invoke<void>("reveal_in_explorer", { id });
  },
  verifyDownloadChecksum(id: string, expectedSha256?: string | null) {
    return invoke<ChecksumResult>("verify_download_checksum", { id, expectedSha256 });
  },
  checkForUpdates() {
    return invoke<UpdateCheckResult>("check_for_updates");
  },
  cleanupHistory() {
    return invoke<string[]>("cleanup_history");
  },
  pickDirectory() {
    return invoke<string | null>("pick_directory");
  },
};

export function onDownloadProgress(handler: (download: Download) => void): Promise<UnlistenFn> {
  return listen<Download>("download-progress", (event) => handler(event.payload));
}

export function onDownloadFinished(handler: (download: Download) => void): Promise<UnlistenFn> {
  return listen<Download>("download-finished", (event) => handler(event.payload));
}

export function onDownloadStatus(handler: (download: Download) => void): Promise<UnlistenFn> {
  return listen<Download>("download-status", (event) => handler(event.payload));
}

export function onExecutorSummary(
  handler: (summary: ExecutorSummary) => void,
): Promise<UnlistenFn> {
  return listen<ExecutorSummary>("executor-summary", (event) => handler(event.payload));
}

export function onTrayAddDownload(handler: () => void): Promise<UnlistenFn> {
  return listen("tray-add-download", () => handler());
}

export function onDeepLinkUrl(handler: (url: string) => void): Promise<UnlistenFn> {
  return listen<string>("deep-link-url", (event) => handler(event.payload));
}
