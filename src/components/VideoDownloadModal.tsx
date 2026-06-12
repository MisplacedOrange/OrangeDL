// SPDX-FileCopyrightText: 2025 MisplacedOrange
// SPDX-License-Identifier: GPL-3.0-only

import { FormEvent, useCallback, useEffect, useState } from "react";
import { clsx } from "clsx";
import { orangeApi } from "../lib/tauri";
import type { VideoInfo, YtDlpStatus, StartVideoDownloadRequest } from "../lib/types";
import { VIDEO_QUALITY_OPTIONS } from "../lib/types";

interface VideoDownloadModalProps {
  open: boolean;
  url: string;
  defaultDirectory?: string;
  onClose: () => void;
  onSubmit: (request: StartVideoDownloadRequest) => Promise<void>;
}

function formatDuration(secs: number): string {
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = secs % 60;
  if (h > 0) return `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
  return `${m}:${String(s).padStart(2, "0")}`;
}

export function VideoDownloadModal({
  open,
  url,
  defaultDirectory = "",
  onClose,
  onSubmit,
}: VideoDownloadModalProps) {
  const [ytdlp, setYtdlp] = useState<YtDlpStatus | null>(null);
  const [installing, setInstalling] = useState(false);
  const [installError, setInstallError] = useState("");

  const [info, setInfo] = useState<VideoInfo | null>(null);
  const [fetching, setFetching] = useState(false);
  const [fetchError, setFetchError] = useState("");

  const [quality, setQuality] = useState("best");
  const [directory, setDirectory] = useState(defaultDirectory);
  const [submitting, setSubmitting] = useState(false);
  const [submitError, setSubmitError] = useState("");

  // Check yt-dlp and fetch video info when modal opens
  useEffect(() => {
    if (!open) return;
    setInfo(null);
    setFetchError("");
    setSubmitError("");
    setInstallError("");
    setQuality("best");
    setDirectory(defaultDirectory);

    orangeApi.checkYtdlp().then((status) => {
      setYtdlp(status);
      if (status.found && url) {
        setFetching(true);
        orangeApi
          .fetchVideoInfo(url)
          .then((i) => setInfo(i))
          .catch((e) => setFetchError(String(e)))
          .finally(() => setFetching(false));
      }
    });
  }, [open, url, defaultDirectory]);

  const handleInstall = useCallback(async () => {
    setInstalling(true);
    setInstallError("");
    try {
      await orangeApi.downloadYtdlp();
      const status = await orangeApi.checkYtdlp();
      setYtdlp(status);
      if (status.found && url) {
        setFetching(true);
        orangeApi
          .fetchVideoInfo(url)
          .then((i) => setInfo(i))
          .catch((e) => setFetchError(String(e)))
          .finally(() => setFetching(false));
      }
    } catch (e) {
      setInstallError(String(e));
    } finally {
      setInstalling(false);
    }
  }, [url]);

  const handlePickDirectory = useCallback(async () => {
    try {
      const picked = await orangeApi.pickDirectory();
      if (picked) setDirectory(picked);
    } catch {
      // user cancelled
    }
  }, []);

  async function handleSubmit(event: FormEvent) {
    event.preventDefault();
    if (!info) return;
    setSubmitting(true);
    setSubmitError("");
    try {
      await onSubmit({
        url,
        quality,
        title: info.title,
        directory: directory.trim() || null,
      });
      onClose();
    } catch (e) {
      setSubmitError(String(e));
    } finally {
      setSubmitting(false);
    }
  }

  if (!open) return null;

  return (
    <div className="modal-backdrop fixed inset-0 z-40 grid place-items-center px-4">
      <form
        onSubmit={handleSubmit}
        className="modal-panel w-full max-w-xl animate-panel-in"
        role="dialog"
        aria-modal="true"
        aria-labelledby="video-dl-title"
      >
        <div className="mb-5">
          <p className="modal-eyebrow">Video download</p>
          <h2 id="video-dl-title" className="modal-title">Download video</h2>
        </div>

        {/* yt-dlp not found */}
        {ytdlp && !ytdlp.found && (
          <div className="mb-4 rounded-lg border border-warn/40 bg-warn/10 p-4">
            <p className="text-sm font-medium text-warn mb-2">yt-dlp is required</p>
            <p className="text-xs text-secondary mb-3">
              yt-dlp is an open-source tool needed to download from YouTube, Bilibili, and other
              video platforms.
            </p>
            <button
              type="button"
              onClick={handleInstall}
              disabled={installing}
              className="button-cta h-9 px-4 text-sm"
            >
              {installing ? "Downloading yt-dlp…" : "Install yt-dlp automatically"}
            </button>
            {installError && (
              <p className="mt-2 text-xs text-error">{installError}</p>
            )}
          </div>
        )}

        {/* yt-dlp found — version badge */}
        {ytdlp?.found && (
          <div className="mb-3 flex items-center gap-2">
            <span className="text-xs text-success">yt-dlp {ytdlp.version}</span>
          </div>
        )}

        {/* Video info */}
        {ytdlp?.found && (
          <>
            <div className="mb-4 rounded-lg bg-surface-alt p-3 min-h-[72px]">
              {fetching && (
                <p className="text-sm text-secondary">Fetching video info…</p>
              )}
              {fetchError && (
                <p className="text-sm text-error">{fetchError}</p>
              )}
              {!fetching && !fetchError && info && (
                <div className="flex gap-3">
                  {info.thumbnailUrl && (
                    <img
                      src={info.thumbnailUrl}
                      alt=""
                      className="h-16 w-28 rounded object-cover flex-shrink-0"
                    />
                  )}
                  <div className="min-w-0">
                    <p className="text-sm font-medium truncate">{info.title}</p>
                    {info.uploader && (
                      <p className="text-xs text-secondary truncate">{info.uploader}</p>
                    )}
                    {info.durationSecs != null && (
                      <p className="text-xs text-secondary mt-0.5">
                        {formatDuration(info.durationSecs)}
                      </p>
                    )}
                  </div>
                </div>
              )}
              {!fetching && !fetchError && !info && (
                <p className="text-sm text-secondary truncate">{url}</p>
              )}
            </div>

            {/* Quality selector */}
            <label className="block mb-4">
              <span className="field-label">Quality</span>
              <select
                value={quality}
                onChange={(e) => setQuality(e.target.value)}
                className="field-input h-12 text-sm"
                aria-label="Download quality"
              >
                {VIDEO_QUALITY_OPTIONS.map((opt) => (
                  <option key={opt.value} value={opt.value}>
                    {opt.label}
                  </option>
                ))}
              </select>
            </label>

            {/* Save to folder */}
            <div className="mb-4">
              <span className="field-label">Save to</span>
              <div className="flex gap-2">
                <input
                  value={directory}
                  onChange={(e) => setDirectory(e.target.value)}
                  className="field-input h-12 min-w-0 flex-1 text-sm"
                  placeholder="Default download folder"
                  aria-label="Save to folder"
                />
                <button
                  type="button"
                  onClick={handlePickDirectory}
                  className="button-ghost h-12 px-3 text-sm"
                  aria-label="Browse for save folder"
                >
                  Browse
                </button>
              </div>
            </div>

            {quality === "1080p" || quality === "4k" ? (
              <p className="text-xs text-secondary mb-3">
                Note: High-quality downloads may require ffmpeg for merging. If the download fails,
                try a lower quality or install ffmpeg.
              </p>
            ) : null}
          </>
        )}

        {submitError && <div className="form-error mb-3">{submitError}</div>}

        <div className="flex justify-end gap-3">
          <button
            type="button"
            onClick={onClose}
            className="button-ghost h-11 px-4"
          >
            Cancel
          </button>
          <button
            type="submit"
            disabled={submitting || !ytdlp?.found || fetching || !info}
            className={clsx("button-cta h-11 px-4", (!ytdlp?.found || !info) && "opacity-50")}
          >
            {submitting ? "Starting…" : "Download"}
          </button>
        </div>
      </form>
    </div>
  );
}
