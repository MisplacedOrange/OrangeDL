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
}

export interface StartDownloadRequest {
  url: string;
  fileName?: string | null;
  directory?: string | null;
  speedLimitBps?: number | null;
}

export interface AppSettings {
  defaultDownloadDirectory: string;
  defaultSpeedLimitBps: number | null;
}

export interface UpdateSettingsRequest {
  defaultDownloadDirectory?: string | null;
  defaultSpeedLimitBps?: number | null;
}

export interface DownloadSummary {
  total: number;
  active: number;
  queued: number;
  completed: number;
  failed: number;
}

export type PageId = "downloads" | "settings";
