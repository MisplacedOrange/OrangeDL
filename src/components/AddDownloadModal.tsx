// SPDX-FileCopyrightText: 2025 MisplacedOrange
// SPDX-License-Identifier: GPL-3.0-only

import { FormEvent, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { clsx } from "clsx";
import type { PreflightResult, StartDownloadRequest } from "../lib/types";
import { orangeApi } from "../lib/tauri";
import { formatBytes } from "../lib/format";

const VIDEO_HOSTS = new Set([
  "youtube.com", "youtu.be", "bilibili.com", "b23.tv", "twitch.tv",
  "clips.twitch.tv", "vimeo.com", "dailymotion.com", "tiktok.com",
  "vm.tiktok.com", "twitter.com", "x.com", "instagram.com", "reddit.com",
  "v.redd.it", "facebook.com", "fb.watch", "nicovideo.jp", "rumble.com",
  "odysee.com", "streamable.com", "gfycat.com",
]);

function isVideoUrl(url: string): boolean {
  try {
    const parsed = new URL(url.trim());
    let host = parsed.hostname.toLowerCase();
    host = host.replace(/^(www\.|m\.|music\.)/, "");
    return VIDEO_HOSTS.has(host);
  } catch {
    return false;
  }
}

interface AddDownloadModalProps {
  open: boolean;
  initialUrl?: string;
  defaultDirectory?: string;
  defaultSpeedLimitBps?: number | null;
  onClose: () => void;
  onSubmit: (requests: StartDownloadRequest[]) => Promise<void>;
  onVideoUrl?: (url: string) => void;
}

function extractUrls(text: string): string[] {
  const matches = text.match(/https?:\/\/[^\s"'<>]+/gi);
  if (matches) return matches.map((url) => url.trim());
  return text.split(/[\n\r]+/).map((l) => l.trim()).filter(Boolean);
}

export function AddDownloadModal({
  open,
  initialUrl = "",
  defaultDirectory = "",
  defaultSpeedLimitBps = null,
  onClose,
  onSubmit,
  onVideoUrl,
}: AddDownloadModalProps) {
  const [urlText, setUrlText] = useState(initialUrl);
  const [fileName, setFileName] = useState("");
  const [directory, setDirectory] = useState(defaultDirectory);
  const [speedLimit, setSpeedLimit] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState("");
  const [preflight, setPreflight] = useState<PreflightResult | null>(null);
  const [preflighting, setPreflighting] = useState(false);
  const preflightTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const urls = useMemo(() => extractUrls(urlText), [urlText]);
  const isMulti = urls.length > 1;
  const singleUrl = urls.length === 1 ? urls[0] : null;

  // Auto-detect clipboard URL when modal opens
  useEffect(() => {
    if (open && !initialUrl) {
      navigator.clipboard.readText().catch(() => "").then((text) => {
        const trimmed = text.trim();
        if (/^https?:\/\//i.test(trimmed)) {
          setUrlText(trimmed);
        }
      });
    }
  }, [open, initialUrl]);

  useEffect(() => {
    if (open) {
      setUrlText(initialUrl);
      setFileName("");
      setDirectory(defaultDirectory);
      setSpeedLimit("");
      setError("");
      setPreflight(null);
    }
  }, [initialUrl, open, defaultDirectory]);

  // Preflight with debounce for single URL
  useEffect(() => {
    if (preflightTimer.current) clearTimeout(preflightTimer.current);
    if (!singleUrl || !/^https?:\/\//i.test(singleUrl)) {
      setPreflight(null);
      return;
    }
    preflightTimer.current = setTimeout(async () => {
      setPreflighting(true);
      try {
        const result = await orangeApi.preflightCheck(singleUrl);
        setPreflight(result);
        if (result.fileName && !fileName) {
          setFileName(result.fileName);
        }
      } catch {
        setPreflight(null);
      } finally {
        setPreflighting(false);
      }
    }, 800);
    return () => {
      if (preflightTimer.current) clearTimeout(preflightTimer.current);
    };
  }, [singleUrl]); // eslint-disable-line react-hooks/exhaustive-deps

  const parsedName = useMemo(() => {
    if (!singleUrl) return "";
    try {
      const parsed = new URL(singleUrl);
      const segment = parsed.pathname.split("/").filter(Boolean).pop();
      return segment ? decodeURIComponent(segment) : "";
    } catch {
      return "";
    }
  }, [singleUrl]);

  const handlePickDirectory = useCallback(async () => {
    try {
      const picked = await orangeApi.pickDirectory();
      if (picked) setDirectory(picked);
    } catch {
      // user cancelled or not supported
    }
  }, []);

  if (!open) return null;

  function validateUrl(url: string): string | null {
    try {
      const parsed = new URL(url.trim());
      if (!["http:", "https:"].includes(parsed.protocol)) return "Only HTTP and HTTPS links are supported.";
      if (parsed.username || parsed.password) return "URLs with embedded credentials are not supported.";
    } catch {
      return "Enter a valid download URL.";
    }
    return null;
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError("");

    if (urls.length === 0) {
      setError("Enter at least one URL.");
      return;
    }

    for (const url of urls) {
      const err = validateUrl(url);
      if (err) { setError(`${url}: ${err}`); return; }
    }

    const numericLimit = Number(speedLimit);
    const speedLimitBps =
      Number.isFinite(numericLimit) && numericLimit > 0 ? Math.round(numericLimit * 1024 * 1024) : null;

    if (speedLimit.trim() && (!Number.isFinite(numericLimit) || numericLimit <= 0)) {
      setError("Speed limit must be a positive number, or blank for unlimited.");
      return;
    }

    setSubmitting(true);
    try {
      const requests: StartDownloadRequest[] = urls.map((url) => ({
        url: url.trim(),
        fileName: !isMulti && fileName.trim() ? fileName.trim() : null,
        directory: directory.trim() || null,
        speedLimitBps,
      }));
      await onSubmit(requests);
      setUrlText("");
      setFileName("");
      setDirectory(defaultDirectory);
      setSpeedLimit("");
      onClose();
    } catch (submitError) {
      setError(String(submitError));
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <div className="modal-backdrop fixed inset-0 z-40 grid place-items-center px-4">
      <form
        onSubmit={handleSubmit}
        className="modal-panel w-full max-w-xl animate-panel-in"
        role="dialog"
        aria-modal="true"
        aria-labelledby="add-download-title"
      >
        <div className="mb-5 flex items-start justify-between gap-4">
          <div>
            <p className="modal-eyebrow">New transfer</p>
            <h2 id="add-download-title" className="modal-title">Add download</h2>
            <p className="modal-subtitle mt-1">Paste one or more HTTP/HTTPS links</p>
          </div>
        </div>

        <label className="block">
          <span className="field-label">URL{isMulti ? `S (${urls.length})` : ""}</span>
          <textarea
            value={urlText}
            onChange={(event) => setUrlText(event.target.value)}
            autoFocus
            rows={isMulti ? Math.min(urls.length + 1, 6) : 1}
            className={clsx("field-input text-sm", isMulti && "font-mono text-xs")}
            placeholder="https://example.com/file.zip"
            aria-label="Download URLs"
          />
        </label>

        {/* Video URL banner */}
        {singleUrl && isVideoUrl(singleUrl) && onVideoUrl && (
          <div className="mt-2 flex items-center gap-3 rounded-lg border border-accent/30 bg-accent/10 px-3 py-2">
            <span className="text-sm">Video URL detected</span>
            <button
              type="button"
              onClick={() => { onClose(); onVideoUrl(singleUrl); }}
              className="button-cta h-8 px-3 text-xs ml-auto"
            >
              Pick quality &amp; download
            </button>
          </div>
        )}

        {/* Preflight hint */}
        {singleUrl && !isVideoUrl(singleUrl) && (
          <div className="modal-hint mt-2 min-h-[18px]">
            {preflighting && <span>Checking server…</span>}
            {!preflighting && preflight && (
              <span className="flex flex-wrap gap-x-3 gap-y-0.5">
                {preflight.contentLength != null && (
                  <span>{formatBytes(preflight.contentLength)}</span>
                )}
                {preflight.contentLength == null && (
                  <span className="text-warn">Unknown size</span>
                )}
                {preflight.contentType && <span>{preflight.contentType}</span>}
                {preflight.supportsRange ? (
                  <span className="text-success">✓ Resumable</span>
                ) : (
                  <span className="text-warn">⚠ Resume not supported</span>
                )}
              </span>
            )}
          </div>
        )}

        {!isMulti && (
          <div className="mt-4 grid grid-cols-2 gap-4">
            <label className="block">
              <span className="field-label">Filename</span>
              <input
                value={fileName}
                onChange={(event) => setFileName(event.target.value)}
                className="field-input h-12 text-sm"
                placeholder={preflight?.fileName || parsedName || "Auto"}
                aria-label="Filename"
              />
            </label>

            <label className="block">
              <span className="field-label">Limit MB/s</span>
              <input
                value={speedLimit}
                onChange={(event) => setSpeedLimit(event.target.value)}
                inputMode="decimal"
                className="field-input h-12 text-sm"
                placeholder={defaultSpeedLimitBps ? `Default ${(defaultSpeedLimitBps / 1024 / 1024).toFixed(1)}` : "Unlimited"}
                aria-label="Speed limit in megabytes per second"
              />
            </label>
          </div>
        )}

        {/* Save-to folder */}
        <div className="mt-4">
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

        {isMulti && (
          <div className="mt-4">
            <span className="field-label">Limit MB/s</span>
            <input
              value={speedLimit}
              onChange={(event) => setSpeedLimit(event.target.value)}
              inputMode="decimal"
              className="field-input h-12 text-sm"
              placeholder="Unlimited"
              aria-label="Speed limit in megabytes per second"
            />
          </div>
        )}

        {error ? <div className="form-error">{error}</div> : null}

        <div className="mt-6 flex justify-end gap-3">
          <button
            type="button"
            onClick={onClose}
            className="button-ghost h-11 px-4"
            aria-label="Cancel add download"
          >
            Cancel
          </button>
          <button
            type="submit"
            disabled={submitting || urls.length === 0}
            className="button-cta h-11 px-4"
            aria-label={isMulti ? `Add ${urls.length} downloads` : "Add download"}
          >
            {submitting ? "Adding…" : isMulti ? `Add ${urls.length} downloads` : "Add download"}
          </button>
        </div>
      </form>
    </div>
  );
}
