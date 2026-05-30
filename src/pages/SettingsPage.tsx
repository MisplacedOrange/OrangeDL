import { FormEvent, useEffect, useState } from "react";
import { formatSpeed } from "../lib/format";
import type { AppSettings, UpdateSettingsRequest } from "../lib/types";

interface SettingsPageProps {
  settings: AppSettings;
  onSave: (request: UpdateSettingsRequest) => Promise<void>;
}

const runtimeCards = [
  {
    label: "SQLite ledger",
    value: "Persistent",
    detail: "Downloads are restored from local state.",
  },
  {
    label: "Auto retry",
    value: "3 attempts",
    detail: "Transient network failures back off and retry.",
  },
  {
    label: "Speed limits",
    value: "Defaults + per item",
    detail: "Set a global default or override when adding a URL.",
  },
  {
    label: "Temporary files",
    value: ".part",
    detail: "Partial data stays resumable until complete.",
  },
  {
    label: "Close behavior",
    value: "Tray resident",
    detail: "Closing the window keeps transfers alive.",
  },
  {
    label: "Queue",
    value: "Concurrent",
    detail: "Multiple Tokio tasks can run together.",
  },
];

export function SettingsPage({ settings, onSave }: SettingsPageProps) {
  const [directory, setDirectory] = useState(settings.defaultDownloadDirectory);
  const [defaultSpeedLimit, setDefaultSpeedLimit] = useState(
    settings.defaultSpeedLimitBps ? String(settings.defaultSpeedLimitBps / 1024 / 1024) : "",
  );
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    setDirectory(settings.defaultDownloadDirectory);
    setDefaultSpeedLimit(settings.defaultSpeedLimitBps ? String(settings.defaultSpeedLimitBps / 1024 / 1024) : "");
  }, [settings]);

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError("");

    const trimmedDirectory = directory.trim();
    const numericLimit = Number(defaultSpeedLimit);
    const defaultSpeedLimitBps =
      defaultSpeedLimit.trim() && Number.isFinite(numericLimit) && numericLimit > 0
        ? Math.round(numericLimit * 1024 * 1024)
        : null;

    if (defaultSpeedLimit.trim() && (!Number.isFinite(numericLimit) || numericLimit <= 0)) {
      setError("Default speed limit must be a positive number, or blank for unlimited.");
      return;
    }

    setSaving(true);
    try {
      await onSave({
        defaultDownloadDirectory: trimmedDirectory || null,
        defaultSpeedLimitBps,
      });
    } catch (saveError) {
      setError(String(saveError));
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="settings-page">
      <div className="settings-header">
        <div>
          <p className="text-xs font-semibold uppercase tracking-wide text-orange-400">Preferences</p>
          <h2 className="mt-1 text-2xl font-bold text-orange-50">Download settings</h2>
        </div>
        <span className="settings-badge">Saved locally</span>
      </div>

      <form onSubmit={handleSubmit} className="settings-card">
        <div className="settings-section-title">
          <span className="settings-section-mark">SET</span>
          <div>
            <h3>Defaults for new downloads</h3>
            <p>These values are applied when the add dialog leaves a field blank.</p>
          </div>
        </div>

        <label className="settings-field">
          <span>Default directory</span>
          <input
            value={directory}
            onChange={(event) => setDirectory(event.target.value)}
            placeholder="Leave blank to use the system Downloads folder"
          />
          <small>The folder is created on save if it does not already exist.</small>
        </label>

        <label className="settings-field">
          <span>Default speed limit MB/s</span>
          <input
            value={defaultSpeedLimit}
            onChange={(event) => setDefaultSpeedLimit(event.target.value)}
            inputMode="decimal"
            placeholder="Unlimited"
          />
          <small>Current default: {settings.defaultSpeedLimitBps ? formatSpeed(settings.defaultSpeedLimitBps) : "Unlimited"}</small>
        </label>

        {error ? <div className="settings-error">{error}</div> : null}

        <div className="settings-actions">
          <button
            type="button"
            onClick={() => {
              setDirectory("");
              setDefaultSpeedLimit("");
            }}
            className="secondary-button"
          >
            Reset defaults
          </button>
          <button type="submit" disabled={saving} className="primary-button">
            {saving ? "Saving" : "Save settings"}
          </button>
        </div>
      </form>

      <section className="settings-metrics" aria-label="Runtime settings">
        {runtimeCards.map((card, index) => (
          <article key={card.label} className="settings-metric-card">
            <div className="metric-icon">{String(index + 1).padStart(2, "0")}</div>
            <div>
              <p className="metric-label">{card.label}</p>
              <h3>{card.value}</h3>
              <p>{card.detail}</p>
            </div>
          </article>
        ))}
      </section>

      <section className="settings-note">
        <h3>History cleanup</h3>
        <p>
          Completed, cancelled, and failed downloads can be removed from the Downloads page. Completed files remain on
          disk; only the app history row is cleared.
        </p>
      </section>
    </div>
  );
}
