import { memo } from "react";
import { clsx } from "clsx";
import type { Download } from "../lib/types";
import { formatBytes, formatSpeed, progressValue, statusLabel } from "../lib/format";

interface DownloadCardProps {
  download: Download;
  onPause: (id: string) => void;
  onResume: (id: string) => void;
  onCancel: (id: string) => void;
  onDelete: (id: string) => void;
}

export const DownloadCard = memo(function DownloadCard({
  download,
  onPause,
  onResume,
  onCancel,
  onDelete,
}: DownloadCardProps) {
  const progress = progressValue(download);
  const canPause = download.status === "downloading" || download.status === "queued";
  const canResume = download.status === "paused" || download.status === "failed";
  const canCancel = !["completed", "cancelled", "failed"].includes(download.status);
  const canDelete = ["completed", "cancelled", "failed"].includes(download.status);
  const isRunning = download.status === "downloading";
  const isComplete = download.status === "completed";

  return (
    <article className={clsx("download-row", isRunning && "is-active")}>
      <div className="download-name-cell">
        <span className={clsx("file-kind", isComplete && "is-complete")} />
        <div className="download-name-text">
          <h3 className="truncate text-sm font-bold text-orange-50">{download.fileName}</h3>
          {download.error ? <p className="download-error">{download.error}</p> : null}
        </div>
      </div>

      <div className="tabular text-sm font-bold text-orange-100">
        {download.totalBytes ? formatBytes(download.totalBytes) : formatBytes(download.downloadedBytes)}
      </div>

      <div className="download-status-cell">
        {isComplete ? (
          <span className="text-sm font-bold text-stone-400">Completed</span>
        ) : (
          <>
            <div className="row-progress" aria-label={`${Math.round(progress)} percent complete`}>
              <span style={{ width: `${progress}%` }} />
            </div>
            <div className="download-status-copy">
              <span>{statusLabel(download.status)}</span>
              <span className="tabular">{Math.round(progress)}%</span>
            </div>
          </>
        )}
      </div>

      <div className="tabular text-sm font-bold text-orange-50">
        {isRunning ? formatSpeed(download.speedBps) : "0 B/s"}
      </div>

      <div className="text-sm font-bold text-stone-400">{formatAdded(download.createdAt)}</div>

      <div className="row-actions">
        {canPause ? (
          <button type="button" className="row-action primary" onClick={() => onPause(download.id)}>
            Pause
          </button>
        ) : null}
        {canResume ? (
          <button type="button" className="row-action primary" onClick={() => onResume(download.id)}>
            Resume
          </button>
        ) : null}
        {canCancel ? (
          <button type="button" className="row-action danger" onClick={() => onCancel(download.id)}>
            Cancel
          </button>
        ) : null}
        {canDelete ? (
          <button type="button" className="row-action" onClick={() => onDelete(download.id)}>
            Remove
          </button>
        ) : null}
        {!canPause && !canResume && !canCancel && !canDelete ? (
          <span className="row-terminal-status">{statusLabel(download.status)}</span>
        ) : null}
      </div>
    </article>
  );
});

function formatAdded(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return "";
  }

  const today = new Date();
  const sameDay =
    date.getFullYear() === today.getFullYear() &&
    date.getMonth() === today.getMonth() &&
    date.getDate() === today.getDate();

  if (sameDay) {
    return `Today ${date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}`;
  }

  return date.toLocaleDateString([], { month: "short", day: "numeric" });
}
