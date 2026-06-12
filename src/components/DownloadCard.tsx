// SPDX-FileCopyrightText: 2025 MisplacedOrange
// SPDX-License-Identifier: GPL-3.0-only

import { memo, useEffect, useState } from "react";
import { clsx } from "clsx";
import type { ChecksumResult, Download, UpdateDownloadOptionsRequest } from "../lib/types";
import { formatBytes, formatEta, formatSpeed, progressValue, statusLabel } from "../lib/format";

interface DownloadCardProps {
  download: Download;
  onPause: (id: string) => void;
  onResume: (id: string) => void;
  onCancel: (id: string) => void;
  onDelete: (id: string) => void;
  onUpdateOptions?: (request: UpdateDownloadOptionsRequest) => Promise<void>;
  onVerifyChecksum?: (id: string, expectedSha256?: string | null) => Promise<ChecksumResult>;
  onOpenFile?: (id: string) => void;
  onRevealInExplorer?: (id: string) => void;
  draggable?: boolean;
  dragOver?: boolean;
  onDragStart?: () => void;
  onDragEnter?: () => void;
  onDragEnd?: () => void;
  onDrop?: () => void;
}

export const DownloadCard = memo(function DownloadCard({
  download,
  onPause,
  onResume,
  onCancel,
  onDelete,
  onUpdateOptions,
  onVerifyChecksum,
  onOpenFile,
  onRevealInExplorer,
  draggable = false,
  dragOver = false,
  onDragStart,
  onDragEnter,
  onDragEnd,
  onDrop,
}: DownloadCardProps) {
  const [expanded, setExpanded] = useState(false);
  const [copied, setCopied] = useState(false);
  const [copiedError, setCopiedError] = useState(false);
  const [speedDraft, setSpeedDraft] = useState(
    download.speedLimitBps ? String(download.speedLimitBps / 1024 / 1024) : "",
  );
  const [priorityDraft, setPriorityDraft] = useState(download.priority);
  const [checksumDraft, setChecksumDraft] = useState(download.checksumSha256 ?? "");
  const [checksumResult, setChecksumResult] = useState<ChecksumResult | null>(null);
  const [savingOptions, setSavingOptions] = useState(false);
  const [verifying, setVerifying] = useState(false);

  const progress = progressValue(download);
  const canPause = download.status === "downloading" || download.status === "queued";
  const canResume = download.status === "paused" || download.status === "failed";
  const canCancel = !["completed", "cancelled", "failed"].includes(download.status);
  const canDelete = ["completed", "cancelled", "failed"].includes(download.status);
  const isRunning = download.status === "downloading";
  const isComplete = download.status === "completed";
  const priority = priorityCopy(download.priority);

  useEffect(() => {
    setSpeedDraft(download.speedLimitBps ? String(download.speedLimitBps / 1024 / 1024) : "");
    setPriorityDraft(download.priority);
    setChecksumDraft(download.checksumSha256 ?? "");
  }, [download.checksumSha256, download.priority, download.speedLimitBps]);

  function copyUrl() {
    void navigator.clipboard.writeText(download.url).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    });
  }

  function copyError() {
    if (!download.error) return;
    void navigator.clipboard.writeText(download.error).then(() => {
      setCopiedError(true);
      setTimeout(() => setCopiedError(false), 1500);
    });
  }

  async function saveOptions() {
    if (!onUpdateOptions) return;
    const numericLimit = Number(speedDraft);
    const clearSpeedLimit = !speedDraft.trim();
    if (!clearSpeedLimit && (!Number.isFinite(numericLimit) || numericLimit <= 0)) return;

    setSavingOptions(true);
    try {
      await onUpdateOptions({
        id: download.id,
        speedLimitBps: clearSpeedLimit ? null : Math.round(numericLimit * 1024 * 1024),
        clearSpeedLimit,
        priority: priorityDraft,
      });
    } finally {
      setSavingOptions(false);
    }
  }

  async function verifyChecksum() {
    if (!onVerifyChecksum) return;
    setVerifying(true);
    try {
      const result = await onVerifyChecksum(download.id, checksumDraft.trim() || null);
      setChecksumResult(result);
    } finally {
      setVerifying(false);
    }
  }

  const etaLabel = download.etaSeconds != null && download.etaSeconds > 0
    ? formatEta(download.etaSeconds)
    : null;

  return (
    <article
      className={clsx(
        "download-row",
        isRunning && "is-active",
        expanded && "is-expanded",
        draggable && "is-draggable",
        dragOver && "is-drag-over",
      )}
      draggable={draggable}
      onDragStart={onDragStart}
      onDragEnter={onDragEnter}
      onDragOver={(event) => {
        if (draggable) event.preventDefault();
      }}
      onDragEnd={onDragEnd}
      onDrop={(event) => {
        if (draggable) {
          event.preventDefault();
          onDrop?.();
        }
      }}
    >
      {/* Main row — click to expand. A div with button semantics rather than
          a real <button> because the action buttons inside it would otherwise
          be nested interactive elements (invalid HTML). */}
      <div
        role="button"
        tabIndex={0}
        className="download-row-summary"
        onClick={() => setExpanded((v) => !v)}
        onKeyDown={(event) => {
          if (event.target !== event.currentTarget) return;
          if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            setExpanded((v) => !v);
          }
        }}
        aria-expanded={expanded}
        aria-label={`${download.fileName}, ${statusLabel(download.status)}, ${Math.round(progress)} percent`}
      >
        <div className="download-name-cell">
          {draggable ? <span className="queue-grip" aria-hidden="true">::</span> : null}
          <span className={clsx("file-kind", isComplete && "is-complete")} />
          <div className="download-name-text">
            <div className="download-title-line">
              <h3 className="download-title truncate">{download.fileName}</h3>
              <span className={clsx("priority-badge", priority.className)}>{priority.label}</span>
            </div>
            {!expanded && download.error ? (
              <p className="download-error">{download.error}</p>
            ) : null}
          </div>
        </div>

        <div className="cell-strong tabular">
          {download.totalBytes ? formatBytes(download.totalBytes) : "Unknown"}
        </div>

        <div className="download-status-cell">
          {isComplete ? (
            <span className="cell-muted">Completed</span>
          ) : (
            <>
              <div
                className="row-progress"
                role="progressbar"
                aria-valuenow={Math.round(progress)}
                aria-valuemin={0}
                aria-valuemax={100}
                aria-label={`${download.fileName} progress`}
              >
                <span style={{ width: `${progress}%` }} />
              </div>
              <div className="download-status-copy">
                <span>{statusLabel(download.status)}</span>
                <span className="tabular">{Math.round(progress)}%</span>
              </div>
            </>
          )}
        </div>

        <div className="cell-strong tabular">
          {isRunning ? formatSpeed(download.speedBps) : "0 B/s"}
        </div>

        <div className="cell-muted">{formatAdded(download.createdAt)}</div>

        <div className="row-actions" onClick={(e) => e.stopPropagation()}>
          {canPause ? (
            <button type="button" className="row-action primary" onClick={() => onPause(download.id)} aria-label={`Pause ${download.fileName}`}>
              Pause
            </button>
          ) : null}
          {canResume ? (
            <button type="button" className="row-action primary" onClick={() => onResume(download.id)} aria-label={`Resume ${download.fileName}`}>
              Resume
            </button>
          ) : null}
          {canCancel ? (
            <button type="button" className="row-action danger" onClick={() => onCancel(download.id)} aria-label={`Cancel ${download.fileName}`}>
              Cancel
            </button>
          ) : null}
          {canDelete ? (
            <button type="button" className="row-action" onClick={() => onDelete(download.id)} aria-label={`Remove ${download.fileName}`}>
              Remove
            </button>
          ) : null}
        </div>
      </div>

      {/* Expanded detail panel */}
      {expanded && (
        <div className="download-detail">
          <dl className="download-detail-grid">
            <div>
              <dt>URL</dt>
              <dd className="truncate" title={download.url}>{download.url}</dd>
            </div>
            <div>
              <dt>Destination</dt>
              <dd className="truncate" title={download.destination}>{download.destination}</dd>
            </div>
            {etaLabel && (
              <div>
                <dt>ETA</dt>
                <dd>{etaLabel}</dd>
              </div>
            )}
            {download.retryCount > 0 && (
              <div>
                <dt>Retries</dt>
                <dd>{download.retryCount}/{download.maxRetries}</dd>
              </div>
            )}
            {download.error && (
              <div>
                <dt>Error</dt>
                <dd className="text-danger">{download.error}</dd>
              </div>
            )}
            {download.totalBytes && download.downloadedBytes > 0 && (
              <div>
                <dt>Downloaded</dt>
                <dd>{formatBytes(download.downloadedBytes)} / {formatBytes(download.totalBytes)}</dd>
              </div>
            )}
            {!download.totalBytes && (
              <div>
                <dt>Size</dt>
                <dd>Unknown content length</dd>
              </div>
            )}
          </dl>

          {onUpdateOptions ? (
            <div className="download-quick-edit" onClick={(event) => event.stopPropagation()}>
              <label>
                <span>Limit MB/s</span>
                <input
                  value={speedDraft}
                  onChange={(event) => setSpeedDraft(event.target.value)}
                  inputMode="decimal"
                  placeholder="Unlimited"
                  aria-label={`Speed limit for ${download.fileName}`}
                />
              </label>
              <label>
                <span>Priority</span>
                <select
                  value={priorityDraft}
                  onChange={(event) => setPriorityDraft(Number(event.target.value))}
                  aria-label={`Priority for ${download.fileName}`}
                >
                  <option value={-5}>Low</option>
                  <option value={0}>Normal</option>
                  <option value={5}>High</option>
                </select>
              </label>
              <button type="button" className="detail-action" onClick={saveOptions} disabled={savingOptions}>
                {savingOptions ? "Saving" : "Save"}
              </button>
            </div>
          ) : null}

          {isComplete && onVerifyChecksum ? (
            <div className="checksum-panel" onClick={(event) => event.stopPropagation()}>
              <label>
                <span>Expected SHA256</span>
                <input
                  value={checksumDraft}
                  onChange={(event) => setChecksumDraft(event.target.value)}
                  placeholder="Paste 64-character hash"
                  aria-label={`Expected SHA256 for ${download.fileName}`}
                />
              </label>
              <button type="button" className="detail-action" onClick={verifyChecksum} disabled={verifying}>
                {verifying ? "Checking" : "Verify"}
              </button>
              <span
                className={clsx(
                  "checksum-result",
                  checksumResult?.matched === true && "is-ok",
                  checksumResult?.matched === false && "is-bad",
                )}
              >
                {checksumResult
                  ? checksumResult.matched == null
                    ? `SHA256 ${checksumResult.computedSha256.slice(0, 12)}...`
                    : checksumResult.matched
                      ? "Verified"
                      : "Mismatch"
                  : download.checksumSha256
                    ? "Stored"
                    : "Not checked"}
              </span>
            </div>
          ) : null}

          <div className="download-detail-actions">
            {isComplete && onOpenFile && (
              <button
                type="button"
                className="detail-action"
                onClick={() => onOpenFile(download.id)}
              >
                Open file
              </button>
            )}
            {isComplete && onRevealInExplorer && (
              <button
                type="button"
                className="detail-action"
                onClick={() => onRevealInExplorer(download.id)}
              >
                Reveal in folder
              </button>
            )}
            <button type="button" className="detail-action" onClick={copyUrl}>
              {copied ? "Copied!" : "Copy URL"}
            </button>
            {download.error ? (
              <button type="button" className="detail-action" onClick={copyError}>
                {copiedError ? "Copied!" : "Copy error"}
              </button>
            ) : null}
          </div>
        </div>
      )}
    </article>
  );
});

function priorityCopy(priority: number): { label: string; className: string } {
  if (priority >= 5) return { label: "High", className: "priority-high" };
  if (priority <= -5) return { label: "Low", className: "priority-low" };
  return { label: "Normal", className: "priority-normal" };
}

function formatAdded(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "";

  const today = new Date();
  const sameDay =
    date.getFullYear() === today.getFullYear() &&
    date.getMonth() === today.getMonth() &&
    date.getDate() === today.getDate();

  if (sameDay) return `Today ${date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}`;
  return date.toLocaleDateString([], { month: "short", day: "numeric" });
}

