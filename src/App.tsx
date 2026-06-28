// SPDX-FileCopyrightText: 2025 MisplacedOrange
// SPDX-License-Identifier: GPL-3.0-only

import { useCallback, useEffect, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { FirstRunSetup } from "./components/FirstRunSetup";
import { ToastViewport } from "./components/ToastViewport";
import { useDownloads } from "./hooks/useDownloads";
import { useToasts } from "./hooks/useToasts";
import { DownloadsPage } from "./pages/DownloadsPage";
import { SettingsPage } from "./pages/SettingsPage";
import { applyTheme } from "./lib/themes";
import type { PageId } from "./lib/types";

export default function App() {
  const [activePage, setActivePage] = useState<PageId>("downloads");
  const { toasts, pushToast, removeToast } = useToasts();
  const {
    downloads,
    summary,
    loading,
    loadError,
    settings,
    startDownloads,
    startVideoDownload,
    pauseDownload,
    resumeDownload,
    cancelDownload,
    deleteDownload,
    openFile,
    revealInExplorer,
    moveDownload,
    renameDownload,
    pauseAll,
    resumeAll,
    retryFailed,
    clearCompleted,
    clearCancelled,
    clearFailed,
    updateSettings,
    updateDownloadOptions,
    verifyDownloadChecksum,
    cleanupHistory,
    openAddModal,
    addModalInitialUrl,
    clearOpenAddModal,
  } = useDownloads(pushToast);

  useEffect(() => {
    applyTheme(settings.theme);
  }, [settings.theme]);

  // --- Command runner --------------------------------------------------------

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

  // --- Download handlers -----------------------------------------------------

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
  const handleMoveDownload = useCallback(
    (id: string) => void runCommand(() => moveDownload(id)),
    [moveDownload, runCommand],
  );
  const handleRenameDownload = useCallback(
    (id: string, newName: string) => runCommand(() => renameDownload(id, newName)),
    [renameDownload, runCommand],
  );

  // --- Settings handlers -----------------------------------------------------

  const handleSaveSettings = useCallback(
    (request: Parameters<typeof updateSettings>[0]) => runCommand(() => updateSettings(request)),
    [runCommand, updateSettings],
  );
  const handleCleanupHistory = useCallback(
    () => void runCommand(cleanupHistory),
    [cleanupHistory, runCommand],
  );
  
  // --- Window handlers -------------------------------------------------------

  const handleMinimizeWindow = useCallback(
    () => void getCurrentWindow().minimize(),
    [],
  );
  const handleToggleMaximizeWindow = useCallback(
    () => void getCurrentWindow().toggleMaximize(),
    [],
  );
  const handleCloseWindow = useCallback(
    () => void getCurrentWindow().close(),
    [],
  );

  return (
    <div className="app-shell min-h-screen overflow-hidden bg-client">
      <div className="client-window">
        <nav className="client-tabs" aria-label="Primary navigation">
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
          <div className="window-drag-region" data-tauri-drag-region />
          <div className="window-controls" aria-label="Window controls">
            <button
              type="button"
              className="window-control window-control--minimize"
              onClick={handleMinimizeWindow}
              aria-label="Minimize window"
            >
              <span aria-hidden="true" />
            </button>
            <button
              type="button"
              className="window-control window-control--maximize"
              onClick={handleToggleMaximizeWindow}
              aria-label="Maximize window"
            >
              <span aria-hidden="true" />
            </button>
            <button
              type="button"
              className="window-control window-control--close"
              onClick={handleCloseWindow}
              aria-label="Close window"
            >
              <span aria-hidden="true" />
            </button>
          </div>
        </nav>

        <main className="min-h-0 flex-1">
          {activePage === "downloads" ? (
            <DownloadsPage
              downloads={downloads}
              summary={summary}
              loading={loading}
              loadError={loadError}
              settings={settings}
              onStartDownloads={startDownloads}
              onStartVideoDownload={startVideoDownload}
              onPauseDownload={handlePauseDownload}
              onResumeDownload={handleResumeDownload}
              onCancelDownload={handleCancelDownload}
              onDeleteDownload={handleDeleteDownload}
              onUpdateDownloadOptions={updateDownloadOptions}
              onVerifyDownloadChecksum={verifyDownloadChecksum}
              onOpenFile={openFile}
              onRevealInExplorer={revealInExplorer}
              onMoveDownload={handleMoveDownload}
              onRenameDownload={handleRenameDownload}
              onPauseAll={handlePauseAll}
              onResumeAll={handleResumeAll}
              onRetryFailed={handleRetryFailed}
              onClearCompleted={handleClearCompleted}
              onClearCancelled={handleClearCancelled}
              onClearFailed={handleClearFailed}
              onCleanupHistory={handleCleanupHistory}
              onNavigateToSettings={() => setActivePage("settings")}
              openAddModal={openAddModal}
              addModalInitialUrl={addModalInitialUrl}
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
