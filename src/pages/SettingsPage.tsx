// SPDX-FileCopyrightText: 2025 MisplacedOrange
// SPDX-License-Identifier: GPL-3.0-only

import { ChangeEvent, FormEvent, useCallback, useEffect, useRef, useState } from "react";
import { clsx } from "clsx";
import { formatSpeed } from "../lib/format";
import { orangeApi } from "../lib/tauri";
import { applyTheme, THEMES } from "../lib/themes";
import type { AppSettings, UpdateCheckResult, UpdateSettingsRequest } from "../lib/types";

interface SettingsPageProps {
  settings: AppSettings;
  onSave: (request: UpdateSettingsRequest) => Promise<void>;
  onCleanupHistory: () => void;
}

function mbps(value: number | null): string {
  return value ? String(value / 1024 / 1024) : "";
}

function parseMbps(value: string): number | null {
  const numeric = Number(value);
  if (!value.trim()) return null;
  if (!Number.isFinite(numeric) || numeric <= 0) {
    throw new Error("Speed limits must be positive numbers, or blank for unlimited.");
  }
  return Math.round(numeric * 1024 * 1024);
}

function parsePositiveInt(value: string, label: string): number | null {
  if (!value.trim()) return null;
  const numeric = Number(value);
  if (!Number.isInteger(numeric) || numeric < 1) {
    throw new Error(`${label} must be a positive integer.`);
  }
  return numeric;
}

function exportSettings(settings: AppSettings) {
  const blob = new Blob([JSON.stringify(settings, null, 2)], { type: "application/json" });
  const href = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = href;
  link.download = "orangedl-settings.json";
  link.click();
  URL.revokeObjectURL(href);
}

export function SettingsPage({ settings, onSave, onCleanupHistory }: SettingsPageProps) {
  // --- State -----------------------------------------------------------------

  const [directory, setDirectory] = useState(settings.defaultDownloadDirectory);
  const [defaultSpeedLimit, setDefaultSpeedLimit] = useState(mbps(settings.defaultSpeedLimitBps));
  const [globalSpeedLimit, setGlobalSpeedLimit] = useState(mbps(settings.globalSpeedLimitBps));
  const [closeToTray, setCloseToTray] = useState(settings.closeToTray ?? true);
  const [notificationsEnabled, setNotificationsEnabled] = useState(
    settings.notificationsEnabled ?? true,
  );
  const [notificationSound, setNotificationSound] = useState(settings.notificationSound ?? false);
  const [backgroundUpdateNotifications, setBackgroundUpdateNotifications] = useState(
    settings.backgroundUpdateNotifications ?? false,
  );
  const [autoOpenFolder, setAutoOpenFolder] = useState(
    settings.autoOpenFolderOnCompletion ?? false,
  );
  const [historyRetentionDays, setHistoryRetentionDays] = useState(
    settings.historyRetentionDays ? String(settings.historyRetentionDays) : "",
  );
  const [historyMaxRows, setHistoryMaxRows] = useState(
    settings.historyMaxRows ? String(settings.historyMaxRows) : "",
  );
  const [theme, setTheme] = useState(settings.theme);
  const [saving, setSaving] = useState(false);
  const [checkingUpdates, setCheckingUpdates] = useState(false);
  const [updateResult, setUpdateResult] = useState<UpdateCheckResult | null>(null);
  const [error, setError] = useState("");
  const importInputRef = useRef<HTMLInputElement>(null);

  // --- Handlers --------------------------------------------------------------

  const handlePickDirectory = useCallback(async () => {
    try {
      const picked = await orangeApi.pickDirectory();
      if (picked) setDirectory(picked);
    } catch {
      // user cancelled or not supported
    }
  }, []);

  useEffect(() => {
    setDirectory(settings.defaultDownloadDirectory);
    setDefaultSpeedLimit(mbps(settings.defaultSpeedLimitBps));
    setGlobalSpeedLimit(mbps(settings.globalSpeedLimitBps));
    setCloseToTray(settings.closeToTray ?? true);
    setNotificationsEnabled(settings.notificationsEnabled ?? true);
    setNotificationSound(settings.notificationSound ?? false);
    setBackgroundUpdateNotifications(settings.backgroundUpdateNotifications ?? false);
    setAutoOpenFolder(settings.autoOpenFolderOnCompletion ?? false);
    setHistoryRetentionDays(settings.historyRetentionDays ? String(settings.historyRetentionDays) : "");
    setHistoryMaxRows(settings.historyMaxRows ? String(settings.historyMaxRows) : "");
    setTheme(settings.theme);
  }, [settings]);

  async function handleThemeSelect(id: string) {
    setTheme(id);
    applyTheme(id);
    await onSave({ theme: id });
  }

  function buildRequest(firstRunCompleted = settings.firstRunCompleted): UpdateSettingsRequest {
    return {
      defaultDownloadDirectory: directory.trim(),
      defaultSpeedLimitBps: parseMbps(defaultSpeedLimit),
      globalSpeedLimitBps: parseMbps(globalSpeedLimit),
      closeToTray,
      notificationsEnabled,
      notificationSound,
      backgroundUpdateNotifications,
      autoOpenFolderOnCompletion: autoOpenFolder,
      historyRetentionDays: parsePositiveInt(historyRetentionDays, "History retention days"),
      historyMaxRows: parsePositiveInt(historyMaxRows, "History max rows"),
      firstRunCompleted,
      theme,
    };
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError("");

    setSaving(true);
    try {
      await onSave(buildRequest());
    } catch (saveError) {
      setError(String(saveError));
    } finally {
      setSaving(false);
    }
  }

  async function handleImportSettings(event: ChangeEvent<HTMLInputElement>) {
    const file = event.target.files?.[0];
    event.target.value = "";
    if (!file) return;

    setError("");
    try {
      const parsed = JSON.parse(await file.text()) as Partial<AppSettings>;
      await onSave({
        defaultDownloadDirectory: parsed.defaultDownloadDirectory ?? settings.defaultDownloadDirectory,
        defaultSpeedLimitBps: parsed.defaultSpeedLimitBps ?? null,
        globalSpeedLimitBps: parsed.globalSpeedLimitBps ?? null,
        closeToTray: parsed.closeToTray ?? settings.closeToTray,
        notificationsEnabled: parsed.notificationsEnabled ?? settings.notificationsEnabled,
        notificationSound: parsed.notificationSound ?? settings.notificationSound,
        backgroundUpdateNotifications:
          parsed.backgroundUpdateNotifications ?? settings.backgroundUpdateNotifications,
        autoOpenFolderOnCompletion:
          parsed.autoOpenFolderOnCompletion ?? settings.autoOpenFolderOnCompletion,
        historyRetentionDays: parsed.historyRetentionDays ?? null,
        historyMaxRows: parsed.historyMaxRows ?? null,
        firstRunCompleted: parsed.firstRunCompleted ?? settings.firstRunCompleted,
        theme: parsed.theme ?? settings.theme,
      });
    } catch (importError) {
      setError(`Settings import failed: ${String(importError)}`);
    }
  }

  async function checkForUpdates() {
    setCheckingUpdates(true);
    setUpdateResult(null);
    setError("");
    try {
      setUpdateResult(await orangeApi.checkForUpdates());
    } catch (updateError) {
      setError(String(updateError));
    } finally {
      setCheckingUpdates(false);
    }
  }

  function resetDefaults() {
    if (!window.confirm("Reset download, notification, and history settings?")) return;
    setDirectory("");
    setDefaultSpeedLimit("");
    setGlobalSpeedLimit("");
    setCloseToTray(true);
    setNotificationsEnabled(true);
    setNotificationSound(false);
    setBackgroundUpdateNotifications(false);
    setAutoOpenFolder(false);
    setHistoryRetentionDays("");
    setHistoryMaxRows("");
  }

  // --- Render ----------------------------------------------------------------

  return (
    <div className="settings-page">
      <div className="settings-header">
        <div>
          <p className="page-eyebrow">Preferences</p>
          <h2 className="page-title">Download settings</h2>
        </div>
        <span className="settings-badge">Saved locally</span>
      </div>

      <form onSubmit={handleSubmit} className="settings-card">
        <section className="settings-group">
          <div className="settings-section-title">
            <span className="settings-section-mark">TH</span>
            <div>
              <h3>Appearance</h3>
              <p>Pick a theme — it applies instantly and is saved right away.</p>
            </div>
          </div>

          <div className="theme-grid" role="radiogroup" aria-label="Color theme">
            {THEMES.map((option) => (
              <button
                key={option.id}
                type="button"
                role="radio"
                aria-checked={theme === option.id}
                className={clsx("theme-option", theme === option.id && "active")}
                onClick={() => void handleThemeSelect(option.id)}
              >
                <span className="theme-swatch" aria-hidden="true">
                  {option.swatch.map((color, index) => (
                    <i key={index} style={{ background: color }} />
                  ))}
                </span>
                <span className="theme-name">
                  {option.label}
                  {theme === option.id ? <span className="theme-active-tag">Active</span> : null}
                </span>
                <span className="theme-desc">{option.description}</span>
              </button>
            ))}
          </div>
        </section>

        <section className="settings-group">
          <div className="settings-section-title">
            <span className="settings-section-mark">DL</span>
            <div>
              <h3>Downloads</h3>
              <p>Defaults used when new transfers leave fields blank.</p>
            </div>
          </div>

          <div className="settings-field">
            <span>Default directory</span>
            <div className="settings-inline">
              <input
                value={directory}
                onChange={(event) => setDirectory(event.target.value)}
                placeholder="Use system Downloads folder"
              />
              <button type="button" onClick={handlePickDirectory} className="secondary-button">
                Browse
              </button>
            </div>
            <small>The folder is created on save if it does not already exist.</small>
          </div>

          <label className="settings-field settings-field--checkbox">
            <input
              type="checkbox"
              checked={autoOpenFolder}
              onChange={(event) => setAutoOpenFolder(event.target.checked)}
            />
            <span>Open containing folder when a download completes</span>
          </label>

          <label className="settings-field settings-field--checkbox">
            <input
              type="checkbox"
              checked={closeToTray}
              onChange={(event) => setCloseToTray(event.target.checked)}
            />
            <span>Close to tray</span>
          </label>
        </section>

        <section className="settings-group">
          <div className="settings-section-title">
            <span className="settings-section-mark">SP</span>
            <div>
              <h3>Speed</h3>
              <p>Caps are stored in megabytes per second.</p>
            </div>
          </div>

          <label className="settings-field">
            <span>Default speed limit MB/s</span>
            <input
              value={defaultSpeedLimit}
              onChange={(event) => setDefaultSpeedLimit(event.target.value)}
              inputMode="decimal"
              placeholder="Unlimited"
            />
            <small>
              Current:{" "}
              {settings.defaultSpeedLimitBps
                ? formatSpeed(settings.defaultSpeedLimitBps)
                : "Unlimited"}
            </small>
          </label>

          <label className="settings-field">
            <span>Global speed limit MB/s</span>
            <input
              value={globalSpeedLimit}
              onChange={(event) => setGlobalSpeedLimit(event.target.value)}
              inputMode="decimal"
              placeholder="Unlimited"
            />
          </label>
        </section>

        <section className="settings-group">
          <div className="settings-section-title">
            <span className="settings-section-mark">NT</span>
            <div>
              <h3>Notifications</h3>
              <p>Completion and failure alerts.</p>
            </div>
          </div>

          <label className="settings-field settings-field--checkbox">
            <input
              type="checkbox"
              checked={notificationsEnabled}
              onChange={(event) => setNotificationsEnabled(event.target.checked)}
            />
            <span>Show native notifications</span>
          </label>

          <label className="settings-field settings-field--checkbox">
            <input
              type="checkbox"
              checked={notificationSound}
              onChange={(event) => setNotificationSound(event.target.checked)}
            />
            <span>Allow notification sound when supported</span>
          </label>
        </section>

        <section className="settings-group">
          <div className="settings-section-title">
            <span className="settings-section-mark">UP</span>
            <div>
              <h3>Installer/update</h3>
              <p>Update checks never install without confirmation.</p>
            </div>
          </div>

          <label className="settings-field settings-field--checkbox">
            <input
              type="checkbox"
              checked={backgroundUpdateNotifications}
              onChange={(event) => setBackgroundUpdateNotifications(event.target.checked)}
            />
            <span>Notify when a newer release is found</span>
          </label>

          <div className="settings-inline settings-inline--wrap">
            <button
              type="button"
              className="secondary-button"
              onClick={checkForUpdates}
              disabled={checkingUpdates}
            >
              {checkingUpdates ? "Checking" : "Check for updates"}
            </button>
            {updateResult ? (
              <span className="settings-result">
                {updateResult.latestVersion
                  ? `${updateResult.message} Latest: ${updateResult.latestVersion}`
                  : updateResult.message}
              </span>
            ) : null}
          </div>
        </section>

        <section className="settings-group">
          <div className="settings-section-title">
            <span className="settings-section-mark">AD</span>
            <div>
              <h3>Advanced</h3>
              <p>Settings files, cleanup policies, and project documents.</p>
            </div>
          </div>

          <div className="settings-grid-two">
            <label className="settings-field">
              <span>History retention days</span>
              <input
                value={historyRetentionDays}
                onChange={(event) => setHistoryRetentionDays(event.target.value)}
                inputMode="numeric"
                placeholder="Keep forever"
              />
            </label>

            <label className="settings-field">
              <span>History max rows</span>
              <input
                value={historyMaxRows}
                onChange={(event) => setHistoryMaxRows(event.target.value)}
                inputMode="numeric"
                placeholder="No cap"
              />
            </label>
          </div>

          <input
            ref={importInputRef}
            type="file"
            accept="application/json,.json"
            className="sr-only"
            onChange={handleImportSettings}
          />

          <div className="settings-inline settings-inline--wrap">
            <button
              type="button"
              className="secondary-button"
              onClick={() => exportSettings(settings)}
            >
              Export settings
            </button>
            <button
              type="button"
              className="secondary-button"
              onClick={() => importInputRef.current?.click()}
            >
              Import settings
            </button>
            <button type="button" className="secondary-button" onClick={onCleanupHistory}>
              Run cleanup
            </button>
            <a href="https://github.com/misplacedorange/OrangeDL/blob/main/docs/privacy-policy.md" target="_blank" rel="noreferrer">
              Privacy
            </a>
            <a href="https://github.com/misplacedorange/OrangeDL/blob/main/docs/terms-and-conditions.md" target="_blank" rel="noreferrer">
              Terms
            </a>
          </div>
        </section>

        {error ? <div className="settings-error">{error}</div> : null}

        <div className="settings-actions">
          <button type="button" onClick={resetDefaults} className="secondary-button">
            Reset defaults
          </button>
          <button type="submit" disabled={saving} className="primary-button">
            {saving ? "Saving" : "Save settings"}
          </button>
        </div>
      </form>
    </div>
  );
}
