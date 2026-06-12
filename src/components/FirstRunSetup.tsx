// SPDX-FileCopyrightText: 2025 MisplacedOrange
// SPDX-License-Identifier: GPL-3.0-only

import { FormEvent, useCallback, useEffect, useState } from "react";
import { orangeApi } from "../lib/tauri";
import type { AppSettings, UpdateSettingsRequest } from "../lib/types";

interface FirstRunSetupProps {
  settings: AppSettings;
  onSave: (request: UpdateSettingsRequest) => Promise<void>;
}

export function FirstRunSetup({ settings, onSave }: FirstRunSetupProps) {
  const [directory, setDirectory] = useState(settings.defaultDownloadDirectory);
  const [maxConcurrent, setMaxConcurrent] = useState(String(settings.maxConcurrentDownloads ?? 3));
  const [autoResume, setAutoResume] = useState(settings.autoResumeInterruptedDownloads ?? false);
  const [closeToTray, setCloseToTray] = useState(settings.closeToTray ?? true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    setDirectory(settings.defaultDownloadDirectory);
    setMaxConcurrent(String(settings.maxConcurrentDownloads ?? 3));
    setAutoResume(settings.autoResumeInterruptedDownloads ?? false);
    setCloseToTray(settings.closeToTray ?? true);
  }, [settings]);

  const handlePickDirectory = useCallback(async () => {
    try {
      const picked = await orangeApi.pickDirectory();
      if (picked) setDirectory(picked);
    } catch {
      // user cancelled or not supported
    }
  }, []);

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError("");
    const concurrent = Number(maxConcurrent);
    if (!Number.isInteger(concurrent) || concurrent < 1) {
      setError("Max concurrency must be a positive integer.");
      return;
    }

    setSaving(true);
    try {
      await onSave({
        defaultDownloadDirectory: directory.trim(),
        maxConcurrentDownloads: concurrent,
        autoResumeInterruptedDownloads: autoResume,
        closeToTray,
        firstRunCompleted: true,
      });
    } catch (saveError) {
      setError(String(saveError));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="modal-backdrop fixed inset-0 z-50 grid place-items-center px-4">
      <form
        onSubmit={handleSubmit}
        className="first-run-panel animate-panel-in"
        role="dialog"
        aria-modal="true"
        aria-labelledby="first-run-title"
      >
        <div className="settings-section-title">
          <span className="settings-section-mark">01</span>
          <div>
            <h2 id="first-run-title">Set up OrangeDL</h2>
            <p>Choose the defaults used by the queue executor.</p>
          </div>
        </div>

        <div className="settings-field">
          <span>Default directory</span>
          <div className="settings-inline">
            <input
              value={directory}
              onChange={(event) => setDirectory(event.target.value)}
              placeholder="Use system Downloads folder"
              aria-label="Default download directory"
            />
            <button type="button" className="secondary-button" onClick={handlePickDirectory}>
              Browse
            </button>
          </div>
        </div>

        <label className="settings-field">
          <span>Max concurrent downloads</span>
          <input
            value={maxConcurrent}
            onChange={(event) => setMaxConcurrent(event.target.value)}
            inputMode="numeric"
            aria-label="Max concurrent downloads"
          />
        </label>

        <label className="settings-field settings-field--checkbox">
          <input
            type="checkbox"
            checked={autoResume}
            onChange={(event) => setAutoResume(event.target.checked)}
          />
          <span>Auto-resume interrupted downloads on launch</span>
        </label>

        <label className="settings-field settings-field--checkbox">
          <input
            type="checkbox"
            checked={closeToTray}
            onChange={(event) => setCloseToTray(event.target.checked)}
          />
          <span>Close to tray</span>
        </label>

        {error ? <div className="settings-error">{error}</div> : null}

        <div className="settings-actions">
          <button type="submit" className="primary-button" disabled={saving}>
            {saving ? "Saving" : "Save setup"}
          </button>
        </div>
      </form>
    </div>
  );
}
