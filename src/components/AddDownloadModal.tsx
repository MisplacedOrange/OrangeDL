import { FormEvent, useEffect, useMemo, useState } from "react";
import { clsx } from "clsx";
import type { StartDownloadRequest } from "../lib/types";

interface AddDownloadModalProps {
  open: boolean;
  initialUrl?: string;
  defaultSpeedLimitBps?: number | null;
  onClose: () => void;
  onSubmit: (request: StartDownloadRequest) => Promise<void>;
}

export function AddDownloadModal({
  open,
  initialUrl = "",
  defaultSpeedLimitBps = null,
  onClose,
  onSubmit,
}: AddDownloadModalProps) {
  const [url, setUrl] = useState(initialUrl);
  const [fileName, setFileName] = useState("");
  const [speedLimit, setSpeedLimit] = useState("");
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    if (open) {
      setUrl(initialUrl);
      setError("");
    }
  }, [initialUrl, open]);

  const parsedName = useMemo(() => {
    try {
      const parsed = new URL(url);
      const segment = parsed.pathname.split("/").filter(Boolean).pop();
      return segment ? decodeURIComponent(segment) : "";
    } catch {
      return "";
    }
  }, [url]);

  if (!open) {
    return null;
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError("");

    try {
      const parsed = new URL(url.trim());
      if (!["http:", "https:"].includes(parsed.protocol)) {
        setError("Only HTTP and HTTPS links are supported.");
        return;
      }
      if (parsed.username || parsed.password) {
        setError("URLs with embedded credentials are not supported.");
        return;
      }
    } catch {
      setError("Enter a valid download URL.");
      return;
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
      await onSubmit({
        url: url.trim(),
        fileName: fileName.trim() || null,
        speedLimitBps,
      });
      setUrl("");
      setFileName("");
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
      >
        <div className="mb-5 flex items-start justify-between gap-4">
          <div>
            <p className="text-xs font-bold uppercase tracking-wide text-orange-400">New transfer</p>
            <h2 className="text-xl font-black text-orange-50">Add download</h2>
            <p className="mt-1 text-sm text-stone-500">Paste a direct HTTP/HTTPS link and optionally cap speed.</p>
          </div>
          <button
            type="button"
            onClick={onClose}
            className="rounded-lg border border-stone-800 px-3 py-2 text-xs font-black uppercase tracking-wide text-stone-400 transition hover:border-orange-600 hover:bg-stone-900 hover:text-orange-200"
          >
            Close
          </button>
        </div>

        <label className="block">
          <span className="mb-2 block text-xs font-black uppercase tracking-wide text-stone-500">URL</span>
          <input
            value={url}
            onChange={(event) => setUrl(event.target.value)}
            autoFocus
            className="h-12 w-full rounded-xl border border-stone-800 bg-stone-900 px-3 text-sm text-orange-50 outline-none transition placeholder:text-stone-500 focus:border-orange-500"
            placeholder="https://example.com/file.zip"
          />
        </label>

        <div className="mt-4 grid grid-cols-2 gap-4">
          <label className="block">
            <span className="mb-2 block text-xs font-black uppercase tracking-wide text-stone-500">Filename</span>
            <input
              value={fileName}
              onChange={(event) => setFileName(event.target.value)}
              className="h-12 w-full rounded-xl border border-stone-800 bg-stone-900 px-3 text-sm text-orange-50 outline-none transition placeholder:text-stone-500 focus:border-orange-500"
              placeholder={parsedName || "Auto"}
            />
          </label>

          <label className="block">
            <span className="mb-2 block text-xs font-black uppercase tracking-wide text-stone-500">Limit MB/s</span>
            <input
              value={speedLimit}
              onChange={(event) => setSpeedLimit(event.target.value)}
              inputMode="decimal"
              className="h-12 w-full rounded-xl border border-stone-800 bg-stone-900 px-3 text-sm text-orange-50 outline-none transition placeholder:text-stone-500 focus:border-orange-500"
              placeholder={defaultSpeedLimitBps ? `Default ${defaultSpeedLimitBps / 1024 / 1024} MB/s` : "Unlimited"}
            />
          </label>
        </div>

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
          >
            Cancel
          </button>
          <button
            type="submit"
            disabled={submitting}
            className={clsx(
              "h-11 rounded-xl border border-orange-500 bg-orange-500 px-4 text-sm font-black text-stone-950 transition",
              "hover:bg-orange-400 disabled:cursor-not-allowed disabled:opacity-60",
            )}
          >
            {submitting ? "Adding" : "Add download"}
          </button>
        </div>
      </form>
    </div>
  );
}
