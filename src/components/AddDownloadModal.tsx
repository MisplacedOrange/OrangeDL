import { FormEvent, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { clsx } from "clsx";
import type { PreflightResult, StartDownloadRequest } from "../lib/types";
import { orangeApi } from "../lib/tauri";
import { formatBytes } from "../lib/format";

interface AddDownloadModalProps {
  open: boolean;
  initialUrl?: string;
  defaultDirectory?: string;
  defaultSpeedLimitBps?: number | null;
  onClose: () => void;
  onSubmit: (requests: StartDownloadRequest[]) => Promise<void>;
}

type QueueMode = "now" | "top" | "bottom";

const priorityOptions = [
  { value: -5, label: "Low" },
  { value: 0, label: "Normal" },
  { value: 5, label: "High" },
];

function extractUrls(text: string): string[] {
  const matches = text.match(/https?:\/\/[^\s"'<>]+/gi);
  if (matches) return matches.map((url) => url.trim());
  return text.split(/[\n\r]+/).map((l) => l.trim()).filter(Boolean);
}

function queuePositionFor(mode: QueueMode, index: number): number | null {
  if (mode === "bottom") return null;
  return index;
}

export function AddDownloadModal({
  open,
  initialUrl = "",
  defaultDirectory = "",
  defaultSpeedLimitBps = null,
  onClose,
  onSubmit,
}: AddDownloadModalProps) {
  const [urlText, setUrlText] = useState(initialUrl);
  const [fileName, setFileName] = useState("");
  const [directory, setDirectory] = useState(defaultDirectory);
  const [speedLimit, setSpeedLimit] = useState("");
  const [queueMode, setQueueMode] = useState<QueueMode>("bottom");
  const [priority, setPriority] = useState(0);
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
      setQueueMode("bottom");
      setPriority(0);
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
      const requests: StartDownloadRequest[] = urls.map((url, i) => ({
        url: url.trim(),
        fileName: !isMulti && fileName.trim() ? fileName.trim() : null,
        directory: directory.trim() || null,
        speedLimitBps,
        priority: queueMode === "now" ? Math.max(priority, 10) : priority,
        queuePosition: queuePositionFor(queueMode, i),
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
    <div className="modal-backdrop fixed inset-0 z-40 grid place-items-center bg-black/70 px-4">
      <form
        onSubmit={handleSubmit}
        className="w-full max-w-xl animate-panel-in rounded-2xl border border-stone-800 bg-stone-950 p-5"
        role="dialog"
        aria-modal="true"
        aria-labelledby="add-download-title"
      >
        <div className="mb-5 flex items-start justify-between gap-4">
          <div>
            <p className="text-xs font-bold uppercase tracking-wide text-orange-400">New transfer</p>
            <h2 id="add-download-title" className="text-xl font-black text-orange-50">Add download</h2>
            <p className="mt-1 text-sm text-stone-500">Paste one or more HTTP/HTTPS links</p>
          </div>
        </div>

        <label className="block">
          <span className="mb-2 block text-xs font-black uppercase tracking-wide text-stone-500">
            URL{isMulti ? `S (${urls.length})` : ""}
          </span>
          <textarea
            value={urlText}
            onChange={(event) => setUrlText(event.target.value)}
            autoFocus
            rows={isMulti ? Math.min(urls.length + 1, 6) : 1}
            className={clsx(
              "w-full rounded-xl border border-stone-800 bg-stone-900 px-3 py-3 text-sm text-orange-50 outline-none transition placeholder:text-stone-500 focus:border-orange-500 resize-none leading-5",
              isMulti && "font-mono text-xs",
            )}
            placeholder="https://example.com/file.zip"
            aria-label="Download URLs"
          />
        </label>

        {/* Preflight hint */}
        {singleUrl && (
          <div className="mt-2 text-xs text-stone-500 min-h-[18px]">
            {preflighting && <span>Checking server…</span>}
            {!preflighting && preflight && (
              <span className="flex flex-wrap gap-x-3 gap-y-0.5">
                {preflight.contentLength != null && (
                  <span>{formatBytes(preflight.contentLength)}</span>
                )}
                {preflight.contentLength == null && (
                  <span className="text-amber-500">Unknown size</span>
                )}
                {preflight.contentType && <span>{preflight.contentType}</span>}
                {preflight.supportsRange ? (
                  <span className="text-green-500">✓ Resumable</span>
                ) : (
                  <span className="text-amber-500">⚠ Resume not supported</span>
                )}
              </span>
            )}
          </div>
        )}

        {!isMulti && (
          <div className="mt-4 grid grid-cols-2 gap-4">
            <label className="block">
              <span className="mb-2 block text-xs font-black uppercase tracking-wide text-stone-500">Filename</span>
              <input
                value={fileName}
                onChange={(event) => setFileName(event.target.value)}
                className="h-12 w-full rounded-xl border border-stone-800 bg-stone-900 px-3 text-sm text-orange-50 outline-none transition placeholder:text-stone-500 focus:border-orange-500"
                placeholder={preflight?.fileName || parsedName || "Auto"}
                aria-label="Filename"
              />
            </label>

            <label className="block">
              <span className="mb-2 block text-xs font-black uppercase tracking-wide text-stone-500">Limit MB/s</span>
              <input
                value={speedLimit}
                onChange={(event) => setSpeedLimit(event.target.value)}
                inputMode="decimal"
                className="h-12 w-full rounded-xl border border-stone-800 bg-stone-900 px-3 text-sm text-orange-50 outline-none transition placeholder:text-stone-500 focus:border-orange-500"
                placeholder={defaultSpeedLimitBps ? `Default ${(defaultSpeedLimitBps / 1024 / 1024).toFixed(1)}` : "Unlimited"}
                aria-label="Speed limit in megabytes per second"
              />
            </label>
          </div>
        )}

        <div className="mt-4 grid grid-cols-2 gap-4">
          <fieldset>
            <legend className="mb-2 block text-xs font-black uppercase tracking-wide text-stone-500">
              Queue position
            </legend>
            <div className="queue-mode">
              {[
                ["now", "Start now"],
                ["top", "Top"],
                ["bottom", "Bottom"],
              ].map(([value, label]) => (
                <button
                  key={value}
                  type="button"
                  className={queueMode === value ? "active" : ""}
                  onClick={() => setQueueMode(value as QueueMode)}
                  aria-pressed={queueMode === value}
                >
                  {label}
                </button>
              ))}
            </div>
          </fieldset>

          <label className="block">
            <span className="mb-2 block text-xs font-black uppercase tracking-wide text-stone-500">
              Priority
            </span>
            <select
              value={priority}
              onChange={(event) => setPriority(Number(event.target.value))}
              className="h-12 w-full rounded-xl border border-stone-800 bg-stone-900 px-3 text-sm text-orange-50 outline-none transition focus:border-orange-500"
              aria-label="Download priority"
            >
              {priorityOptions.map((option) => (
                <option key={option.value} value={option.value}>
                  {option.label}
                </option>
              ))}
            </select>
          </label>
        </div>

        {/* Save-to folder */}
        <div className="mt-4">
          <span className="mb-2 block text-xs font-black uppercase tracking-wide text-stone-500">Save to</span>
          <div className="flex gap-2">
            <input
              value={directory}
              onChange={(e) => setDirectory(e.target.value)}
              className="h-12 min-w-0 flex-1 rounded-xl border border-stone-800 bg-stone-900 px-3 text-sm text-orange-50 outline-none transition placeholder:text-stone-500 focus:border-orange-500"
              placeholder="Default download folder"
              aria-label="Save to folder"
            />
            <button
              type="button"
              onClick={handlePickDirectory}
              className="h-12 rounded-xl border border-stone-800 px-3 text-sm font-bold text-stone-400 transition hover:border-stone-700 hover:bg-stone-900 hover:text-orange-100"
              aria-label="Browse for save folder"
            >
              Browse
            </button>
          </div>
        </div>

        {isMulti && (
          <div className="mt-4">
            <span className="mb-2 block text-xs font-black uppercase tracking-wide text-stone-500">Limit MB/s</span>
            <input
              value={speedLimit}
              onChange={(event) => setSpeedLimit(event.target.value)}
              inputMode="decimal"
              className="h-12 w-full rounded-xl border border-stone-800 bg-stone-900 px-3 text-sm text-orange-50 outline-none transition placeholder:text-stone-500 focus:border-orange-500"
              placeholder="Unlimited"
              aria-label="Speed limit in megabytes per second"
            />
          </div>
        )}

        {error ? (
          <div className="mt-4 rounded-xl border border-red-900 bg-red-950 px-3 py-2 text-sm font-bold text-red-200">
            {error}
          </div>
        ) : null}

        <div className="mt-6 flex justify-end gap-3">
          <button
            type="button"
            onClick={onClose}
            className="h-11 rounded-xl border border-stone-800 px-4 text-sm font-bold text-stone-400 transition hover:border-stone-700 hover:bg-stone-900 hover:text-orange-100"
            aria-label="Cancel add download"
          >
            Cancel
          </button>
          <button
            type="submit"
            disabled={submitting || urls.length === 0}
            className={clsx(
              "h-11 rounded-xl border border-orange-500 bg-orange-500 px-4 text-sm font-black text-stone-950 transition",
              "hover:bg-orange-400 disabled:cursor-not-allowed disabled:opacity-60",
            )}
            aria-label={isMulti ? `Add ${urls.length} downloads` : "Add download"}
          >
            {submitting ? "Adding…" : isMulti ? `Add ${urls.length} downloads` : "Add download"}
          </button>
        </div>
      </form>
    </div>
  );
}
