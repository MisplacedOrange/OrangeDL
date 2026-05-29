import type { Download, DownloadStatus } from "./types";

const BYTE_UNITS = ["B", "KB", "MB", "GB", "TB"];

export function formatBytes(value?: number | null): string {
  if (!value || value <= 0) {
    return "0 B";
  }

  const index = Math.min(Math.floor(Math.log(value) / Math.log(1024)), BYTE_UNITS.length - 1);
  const scaled = value / 1024 ** index;
  return `${scaled >= 10 || index === 0 ? scaled.toFixed(0) : scaled.toFixed(1)} ${BYTE_UNITS[index]}`;
}

export function formatSpeed(value?: number | null): string {
  if (!value || value <= 1) {
    return "0 B/s";
  }

  return `${formatBytes(value)}/s`;
}

export function formatEta(value?: number | null): string {
  if (!value || value <= 0) {
    return "--";
  }

  const hours = Math.floor(value / 3600);
  const minutes = Math.floor((value % 3600) / 60);
  const seconds = value % 60;

  if (hours > 0) {
    return `${hours}h ${minutes}m`;
  }

  if (minutes > 0) {
    return `${minutes}m ${seconds}s`;
  }

  return `${seconds}s`;
}

export function statusLabel(status: DownloadStatus): string {
  const labels: Record<DownloadStatus, string> = {
    queued: "Queued",
    downloading: "Downloading",
    paused: "Paused",
    completed: "Complete",
    failed: "Failed",
    cancelled: "Cancelled",
  };

  return labels[status];
}

export function progressValue(download: Download): number {
  if (download.status === "completed") {
    return 100;
  }

  return Math.max(0, Math.min(100, download.progress || 0));
}
