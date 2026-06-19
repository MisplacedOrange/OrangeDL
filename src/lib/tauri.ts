// SPDX-FileCopyrightText: 2025 MisplacedOrange
// SPDX-License-Identifier: GPL-3.0-only

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppSettings,
  ChecksumResult,
  Download,
  PreflightResult,
  StartDownloadRequest,
  StartVideoDownloadRequest,
  UpdateCheckResult,
  UpdateDownloadOptionsRequest,
  UpdateSettingsRequest,
  VideoInfo,
  YtDlpStatus,
} from "./types";

// --- API object --------------------------------------------------------------

export const orangeApi = {
  // Downloads
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
  updateDownloadOptions(request: UpdateDownloadOptionsRequest) {
    return invoke<Download>("update_download_options", { request });
  },
  preflightCheck(url: string) {
    return invoke<PreflightResult>("preflight_check", { url });
  },
  verifyDownloadChecksum(id: string, expectedSha256?: string | null) {
    return invoke<ChecksumResult>("verify_download_checksum", { id, expectedSha256 });
  },

  // Bulk download actions
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
  cleanupHistory() {
    return invoke<string[]>("cleanup_history");
  },

  moveDownload(id: string, directory: string) {
    return invoke<Download>("move_download", { id, directory });
  },
  renameDownload(id: string, newName: string) {
    return invoke<Download>("rename_download", { id, newName });
  },

  // File operations
  openFile(id: string) {
    return invoke<void>("open_file", { id });
  },
  revealInExplorer(id: string) {
    return invoke<void>("reveal_in_explorer", { id });
  },
  pickDirectory() {
    return invoke<string | null>("pick_directory");
  },

  // Settings
  getSettings() {
    return invoke<AppSettings>("get_settings");
  },
  updateSettings(request: UpdateSettingsRequest) {
    return invoke<AppSettings>("update_settings", { request });
  },

  // Updates
  checkForUpdates() {
    return invoke<UpdateCheckResult>("check_for_updates");
  },

  // Video (yt-dlp)
  checkYtdlp() {
    return invoke<YtDlpStatus>("check_ytdlp");
  },
  fetchVideoInfo(url: string) {
    return invoke<VideoInfo>("fetch_video_info", { url });
  },
  startVideoDownload(request: StartVideoDownloadRequest) {
    return invoke<Download>("start_video_download", { request });
  },
  downloadYtdlp() {
    return invoke<void>("download_ytdlp");
  },
};

// --- Event listeners ---------------------------------------------------------

export function onDownloadProgress(handler: (download: Download) => void): Promise<UnlistenFn> {
  return listen<Download>("download-progress", (event) => handler(event.payload));
}

export function onDownloadFinished(handler: (download: Download) => void): Promise<UnlistenFn> {
  return listen<Download>("download-finished", (event) => handler(event.payload));
}

export function onDownloadStatus(handler: (download: Download) => void): Promise<UnlistenFn> {
  return listen<Download>("download-status", (event) => handler(event.payload));
}

export function onTrayAddDownload(handler: () => void): Promise<UnlistenFn> {
  return listen("tray-add-download", () => handler());
}

export function onDeepLinkUrl(handler: (url: string) => void): Promise<UnlistenFn> {
  return listen<string>("deep-link-url", (event) => handler(event.payload));
}
