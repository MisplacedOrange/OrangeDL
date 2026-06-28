// SPDX-FileCopyrightText: 2025 MisplacedOrange
// SPDX-License-Identifier: GPL-3.0-only

// --- Download ----------------------------------------------------------------

export type DownloadStatus =
  | "queued"
  | "downloading"
  | "paused"
  | "completed"
  | "failed"
  | "cancelled";

export interface Download {
  id: string;
  url: string;
  fileName: string;
  destination: string;
  tempPath: string;
  totalBytes: number | null;
  downloadedBytes: number;
  progress: number;
  speedBps: number;
  etaSeconds: number | null;
  status: DownloadStatus;
  error: string | null;
  createdAt: string;
  updatedAt: string;
  speedLimitBps: number | null;
  retryCount: number;
  maxRetries: number;
  checksumSha256: string | null;
}

export interface DownloadSummary {
  total: number;
  active: number;
  completed: number;
  failed: number;
}

export interface StartDownloadRequest {
  url: string;
  fileName?: string | null;
  directory?: string | null;
  speedLimitBps?: number | null;
}

export interface UpdateDownloadOptionsRequest {
  id: string;
  speedLimitBps?: number | null;
  clearSpeedLimit?: boolean;
}

export interface ChecksumResult {
  id: string;
  computedSha256: string;
  expectedSha256: string | null;
  matched: boolean | null;
}

// --- Settings ----------------------------------------------------------------

export interface AppSettings {
  defaultDownloadDirectory: string;
  defaultSpeedLimitBps: number | null;
  globalSpeedLimitBps: number | null;
  maxConcurrentDownloads: number;
  autoResumeInterruptedDownloads: boolean;
  closeToTray: boolean;
  notificationsEnabled: boolean;
  notificationSound: boolean;
  backgroundUpdateNotifications: boolean;
  autoOpenFolderOnCompletion: boolean;
  historyRetentionDays: number | null;
  historyMaxRows: number | null;
  firstRunCompleted: boolean;
  theme: string;
}

// --- Preflight & updates -----------------------------------------------------

export interface PreflightResult {
  url: string;
  fileName: string | null;
  contentLength: number | null;
  contentType: string | null;
  supportsRange: boolean;
  etag: string | null;
  lastModified: string | null;
}

export interface UpdateCheckResult {
  currentVersion: string;
  latestVersion: string | null;
  releaseUrl: string | null;
  updateAvailable: boolean;
  message: string;
}

// --- Video -------------------------------------------------------------------

export interface YtDlpStatus {
  found: boolean;
  path: string | null;
  version: string | null;
}

export interface VideoInfo {
  title: string;
  thumbnailUrl: string | null;
  durationSecs: number | null;
  uploader: string | null;
  webpageUrl: string;
  extractor: string;
}

export interface StartVideoDownloadRequest {
  url: string;
  quality: string;
  title: string;
  directory?: string | null;
}

export const VIDEO_QUALITY_OPTIONS = [
  { value: "best", label: "Best available" },
  { value: "1080p", label: "1080p" },
  { value: "720p", label: "720p" },
  { value: "480p", label: "480p" },
  { value: "360p", label: "360p" },
  { value: "audio", label: "Audio only" },
] as const;

// --- App ---------------------------------------------------------------------

export type PageId = "downloads" | "settings";
