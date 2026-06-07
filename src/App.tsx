import { useCallback, useState } from "react";
import { FirstRunSetup } from "./components/FirstRunSetup";
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
    executorSummary,
    loading,
    loadError,
    settings,
    startDownloads,
    pauseDownload,
    resumeDownload,
    cancelDownload,
    deleteDownload,
    openFile,
    revealInExplorer,
    pauseAll,
    resumeAll,
    retryFailed,
    clearCompleted,
    clearCancelled,
    clearFailed,
    updateSettings,
    reorderDownload,
    updateDownloadOptions,
    verifyDownloadChecksum,
    cleanupHistory,
    openAddModal,
    addModalInitialUrl,
    clearOpenAddModal,
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
  const handlePauseAll = useCallback(() => void runCommand(pauseAll), [pauseAll, runCommand]);
  const handleResumeAll = useCallback(() => void runCommand(resumeAll), [resumeAll, runCommand]);
  const handleRetryFailed = useCallback(() => void runCommand(retryFailed), [retryFailed, runCommand]);
  const handleClearCompleted = useCallback(() => void runCommand(clearCompleted), [clearCompleted, runCommand]);
  const handleClearCancelled = useCallback(() => void runCommand(clearCancelled), [clearCancelled, runCommand]);
  const handleClearFailed = useCallback(() => void runCommand(clearFailed), [clearFailed, runCommand]);
  const handleSaveSettings = useCallback(
    (request: Parameters<typeof updateSettings>[0]) => runCommand(() => updateSettings(request)),
    [runCommand, updateSettings],
  );
  const handleReorderDownload = useCallback(
    (id: string, position: number) => void runCommand(() => reorderDownload(id, position)),
    [reorderDownload, runCommand],
  );
  const handleCleanupHistory = useCallback(
    () => void runCommand(cleanupHistory),
    [cleanupHistory, runCommand],
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
            aria-keyshortcuts="Control+,"
          >
            Settings
          </button>
        </nav>

        <main className="min-h-0 flex-1">
          {activePage === "downloads" ? (
            <DownloadsPage
              downloads={downloads}
              summary={summary}
              executorSummary={executorSummary}
              loading={loading}
              loadError={loadError}
              settings={settings}
              onStartDownloads={startDownloads}
              onUpdateSettings={handleSaveSettings}
              onPauseDownload={handlePauseDownload}
              onResumeDownload={handleResumeDownload}
              onCancelDownload={handleCancelDownload}
              onDeleteDownload={handleDeleteDownload}
              onReorderDownload={handleReorderDownload}
              onUpdateDownloadOptions={updateDownloadOptions}
              onVerifyDownloadChecksum={verifyDownloadChecksum}
              onOpenFile={openFile}
              onRevealInExplorer={revealInExplorer}
              onPauseAll={handlePauseAll}
              onResumeAll={handleResumeAll}
              onRetryFailed={handleRetryFailed}
              onClearCompleted={handleClearCompleted}
              onClearCancelled={handleClearCancelled}
              onClearFailed={handleClearFailed}
              onCleanupHistory={handleCleanupHistory}
              onNavigateToSettings={() => setActivePage("settings")}
              openAddModal={openAddModal}
              onAddModalOpened={clearOpenAddModal}
            />
          ) : (
            <SettingsPage
              settings={settings}
              onSave={handleSaveSettings}
              onCleanupHistory={handleCleanupHistory}
            />
          )}
        </main>
      </div>
      {!loading && !settings.firstRunCompleted ? (
        <FirstRunSetup settings={settings} onSave={handleSaveSettings} />
      ) : null}
      <ToastViewport toasts={toasts} onDismiss={removeToast} />
    </div>
  );
}
