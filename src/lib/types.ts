// SPDX-FileCopyrightText: 2025 MisplacedOrange
// SPDX-License-Identifier: GPL-3.0-only

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
  priority: number;
  queuePosition: number | null;
  retryCount: number;
  maxRetries: number;
  checksumSha256: string | null;
}

export interface StartDownloadRequest {
  url: string;
  fileName?: string | null;
  directory?: string | null;
  speedLimitBps?: number | null;
  priority?: number | null;
  queuePosition?: number | null;
}

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

export interface UpdateSettingsRequest {
  defaultDownloadDirectory?: string | null;
  defaultSpeedLimitBps?: number | null;
  globalSpeedLimitBps?: number | null;
  maxConcurrentDownloads?: number | null;
  autoResumeInterruptedDownloads?: boolean | null;
  closeToTray?: boolean | null;
  notificationsEnabled?: boolean | null;
  notificationSound?: boolean | null;
  backgroundUpdateNotifications?: boolean | null;
  autoOpenFolderOnCompletion?: boolean | null;
  historyRetentionDays?: number | null;
  historyMaxRows?: number | null;
  firstRunCompleted?: boolean | null;
  theme?: string | null;
}

export interface UpdateDownloadOptionsRequest {
  id: string;
  speedLimitBps?: number | null;
  clearSpeedLimit?: boolean;
  priority?: number | null;
}

export interface ExecutorSummary {
  active: number;
  queued: number;
  paused: number;
  completed: number;
  failed: number;
  cancelled: number;
  maxConcurrent: number;
  totalSpeedBps: number;
}

export interface DownloadSummary {
  total: number;
  active: number;
  queued: number;
  completed: number;
  failed: number;
}

export type PageId = "downloads" | "settings";

export interface PreflightResult {
  url: string;
  fileName: string | null;
  contentLength: number | null;
  contentType: string | null;
  supportsRange: boolean;
  etag: string | null;
  lastModified: string | null;
}

export interface ChecksumResult {
  id: string;
  computedSha256: string;
  expectedSha256: string | null;
  matched: boolean | null;
}

export interface UpdateCheckResult {
  currentVersion: string;
  latestVersion: string | null;
  releaseUrl: string | null;
  updateAvailable: boolean;
  message: string;
}
