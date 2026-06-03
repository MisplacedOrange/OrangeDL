import { useCallback, useState } from "react";
import { ToastViewport } from "./components/ToastViewport";
import { useDownloads } from "./hooks/useDownloads";
import { useToasts } from "./hooks/useToasts";
import orangeDlLogo from "./images/OrangeDL.svg";
import { DownloadsPage } from "./pages/DownloadsPage";
import { SettingsPage } from "./pages/SettingsPage";
import type { PageId } from "./lib/types";

export default function App() {
  const [activePage, setActivePage] = useState<PageId>("downloads");
  const { toasts, pushToast, removeToast } = useToasts();
  const {
    downloads,
    summary,
    loading,
    settings,
    startDownload,
    pauseDownload,
    resumeDownload,
    cancelDownload,
    deleteDownload,
    updateSettings,
  } = useDownloads(pushToast);

  const runCommand = useCallback(
    async (action: () => Promise<void>) => {
      try {
        await action();
      } catch (error) {
        pushToast({
          title: "Command failed",
          message: String(error),
          kind: "error",
        });
      }
    },
    [pushToast],
  );
  const handlePauseDownload = useCallback(
    (id: string) => void runCommand(() => pauseDownload(id)),
    [pauseDownload, runCommand],
  );
  const handleResumeDownload = useCallback(
    (id: string) => void runCommand(() => resumeDownload(id)),
    [resumeDownload, runCommand],
  );
  const handleCancelDownload = useCallback(
    (id: string) => void runCommand(() => cancelDownload(id)),
    [cancelDownload, runCommand],
  );
  const handleDeleteDownload = useCallback(
    (id: string) => void runCommand(() => deleteDownload(id)),
    [deleteDownload, runCommand],
  );
  const handleSaveSettings = useCallback(
    (request: Parameters<typeof updateSettings>[0]) => runCommand(() => updateSettings(request)),
    [runCommand, updateSettings],
  );

  return (
    <div className="app-shell min-h-screen overflow-hidden bg-client text-zinc-100">
      <div className="client-window">
        <nav className="client-tabs" aria-label="Primary navigation">
          <img className="app-mark" src={orangeDlLogo} alt="OrangeDL" />
          <button
            type="button"
            onClick={() => setActivePage("downloads")}
            className={activePage === "downloads" ? "active" : ""}
          >
            Downloads
          </button>
          <button
            type="button"
            onClick={() => setActivePage("settings")}
            className={activePage === "settings" ? "active" : ""}
          >
            Settings
          </button>
        </nav>

        <main className="min-h-0 flex-1">
          {activePage === "downloads" ? (
            <DownloadsPage
              downloads={downloads}
              summary={summary}
              loading={loading}
              defaultSpeedLimitBps={settings.defaultSpeedLimitBps}
              onStartDownload={startDownload}
              onPauseDownload={handlePauseDownload}
              onResumeDownload={handleResumeDownload}
              onCancelDownload={handleCancelDownload}
              onDeleteDownload={handleDeleteDownload}
            />
          ) : (
            <SettingsPage settings={settings} onSave={handleSaveSettings} />
          )}
        </main>
      </div>
      <ToastViewport toasts={toasts} onDismiss={removeToast} />
    </div>
  );
}
