// SPDX-FileCopyrightText: 2025 MisplacedOrange
// SPDX-License-Identifier: GPL-3.0-only

import { useCallback, useEffect, useMemo, useState } from "react";
import type { ToastKind } from "./useToasts";
import {
  onDeepLinkUrl,
  onDownloadFinished,
  onDownloadProgress,
  onDownloadStatus,
  onTrayAddDownload,
  orangeApi,
} from "../lib/tauri";
import type {
  AppSettings,
  ChecksumResult,
  Download,
  DownloadSummary,
  StartDownloadRequest,
  StartVideoDownloadRequest,
  UpdateDownloadOptionsRequest,
  UpdateSettingsRequest,
} from "../lib/types";

type PushToast = (toast: { title: string; message?: string; kind: ToastKind }) => void;

// --- Helpers -----------------------------------------------------------------

function sortDownloads(downloads: Download[]) {
  return [...downloads].sort((a, b) => {
    const activeA = a.status === "downloading" || a.status === "queued";
    const activeB = b.status === "downloading" || b.status === "queued";

    if (activeA !== activeB) {
      return activeA ? -1 : 1;
    }

    return Date.parse(b.createdAt) - Date.parse(a.createdAt);
  });
}

const DEFAULT_SETTINGS: AppSettings = {
  defaultDownloadDirectory: "",
  defaultSpeedLimitBps: null,
  globalSpeedLimitBps: null,
  maxConcurrentDownloads: 3,
  autoResumeInterruptedDownloads: false,
  closeToTray: true,
  notificationsEnabled: true,
  notificationSound: false,
  backgroundUpdateNotifications: false,
  autoOpenFolderOnCompletion: false,
  historyRetentionDays: null,
  historyMaxRows: null,
  firstRunCompleted: false,
  theme: "creamsicle",
};

// --- Hook --------------------------------------------------------------------

export function useDownloads(pushToast: PushToast) {
  // --- State -----------------------------------------------------------------

  const [openAddModal, setOpenAddModal] = useState(false);
  const [addModalInitialUrl, setAddModalInitialUrl] = useState("");
  const [downloads, setDownloads] = useState<Download[]>([]);
  const [settings, setSettings] = useState<AppSettings>(DEFAULT_SETTINGS);
  const [loading, setLoading] = useState(true);
  const [loadError, setLoadError] = useState("");

  // --- Core helpers ----------------------------------------------------------

  const upsertDownload = useCallback((download: Download) => {
    setDownloads((current) => {
      const index = current.findIndex((item) => item.id === download.id);

      if (index < 0) {
        return sortDownloads([...current, download]);
      }

      const previous = current[index];
      const next = [...current];
      next[index] = download;

      if (
        previous.status !== download.status ||
        previous.createdAt !== download.createdAt
      ) {
        return sortDownloads(next);
      }

      return next;
    });
  }, []);

  const removeDownloads = useCallback((ids: string[]) => {
    const idSet = new Set(ids);
    setDownloads((current) => current.filter((d) => !idSet.has(d.id)));
  }, []);

  const refresh = useCallback(async () => {
    setLoading(true);
    setLoadError("");
    try {
      const [items, appSettings] = await Promise.all([
        orangeApi.listDownloads(),
        orangeApi.getSettings(),
      ]);
      setDownloads(sortDownloads(items));
      setSettings(appSettings);
    } catch (error) {
      setLoadError(String(error));
      pushToast({
        title: "Unable to load downloads",
        message: String(error),
        kind: "error",
      });
    } finally {
      setLoading(false);
    }
  }, [pushToast]);

  // --- Effects ---------------------------------------------------------------

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    let unlistenProgress: (() => void) | undefined;
    let unlistenFinished: (() => void) | undefined;
    let unlistenStatus: (() => void) | undefined;
    let disposed = false;

    void onDownloadProgress((download) => {
      upsertDownload(download);
    }).then((unlisten) => {
      if (disposed) unlisten();
      else unlistenProgress = unlisten;
    });

    void onDownloadFinished((download) => {
      upsertDownload(download);
      pushToast({
        title: "Download complete",
        message: download.fileName,
        kind: "success",
      });
    }).then((unlisten) => {
      if (disposed) unlisten();
      else unlistenFinished = unlisten;
    });

    void onDownloadStatus((download) => {
      upsertDownload(download);
    }).then((unlisten) => {
      if (disposed) unlisten();
      else unlistenStatus = unlisten;
    });

    let unlistenTray: (() => void) | undefined;
    let unlistenDeepLink: (() => void) | undefined;

    void onTrayAddDownload(() => {
      setAddModalInitialUrl("");
      setOpenAddModal(true);
    }).then((unlisten) => {
      if (disposed) unlisten();
      else unlistenTray = unlisten;
    });

    void onDeepLinkUrl((url) => {
      try {
        const parsed = new URL(url);
        if (parsed.protocol === "orangedl:") {
          const target = parsed.searchParams.get("url");
          if (target) {
            setAddModalInitialUrl(target);
            setOpenAddModal(true);
          }
        }
      } catch {
        // ignore malformed deep links
      }
    }).then((unlisten) => {
      if (disposed) unlisten();
      else unlistenDeepLink = unlisten;
    });

    return () => {
      disposed = true;
      unlistenProgress?.();
      unlistenFinished?.();
      unlistenStatus?.();
      unlistenTray?.();
      unlistenDeepLink?.();
    };
  }, [pushToast, upsertDownload]);

  // --- Derived state ---------------------------------------------------------

  const summary = useMemo<DownloadSummary>(() => {
    return downloads.reduce(
      (total, download) => {
        total.total += 1;
        if (download.status === "downloading" || download.status === "queued") {
          total.active += 1;
        }
        if (download.status === "completed") total.completed += 1;
        if (download.status === "failed") total.failed += 1;
        return total;
      },
      { total: 0, active: 0, completed: 0, failed: 0 },
    );
  }, [downloads]);

  // --- Download actions ------------------------------------------------------

  const startDownloads = useCallback(
    async (requests: StartDownloadRequest[]) => {
      const results = await Promise.allSettled(requests.map((r) => orangeApi.startDownload(r)));
      let added = 0;
      let failed = 0;
      for (const result of results) {
        if (result.status === "fulfilled") {
          upsertDownload(result.value);
          added++;
        } else {
          failed++;
        }
      }
      if (added > 0) {
        pushToast({
          title: added === 1 ? "Download added" : `${added} downloads added`,
          kind: "info",
        });
      }
      if (failed > 0) {
        pushToast({ title: `${failed} download${failed === 1 ? "" : "s"} failed to add`, kind: "error" });
      }
    },
    [pushToast, upsertDownload],
  );

  const startVideoDownload = useCallback(
    async (request: StartVideoDownloadRequest) => {
      const download = await orangeApi.startVideoDownload(request);
      upsertDownload(download);
      pushToast({ title: "Video download queued", message: download.fileName, kind: "info" });
    },
    [pushToast, upsertDownload],
  );

  const pauseDownload = useCallback(
    async (id: string) => {
      const download = await orangeApi.pauseDownload(id);
      upsertDownload(download);
      pushToast({ title: "Paused", message: download.fileName, kind: "info" });
    },
    [pushToast, upsertDownload],
  );

  const resumeDownload = useCallback(
    async (id: string) => {
      const download = await orangeApi.resumeDownload(id);
      upsertDownload(download);
      pushToast({ title: "Resuming", message: download.fileName, kind: "info" });
    },
    [pushToast, upsertDownload],
  );

  const cancelDownload = useCallback(
    async (id: string) => {
      const download = await orangeApi.cancelDownload(id);
      upsertDownload(download);
      pushToast({ title: "Cancelled", message: download.fileName, kind: "info" });
    },
    [pushToast, upsertDownload],
  );

  const deleteDownload = useCallback(
    async (id: string) => {
      await orangeApi.deleteDownload(id);
      setDownloads((current) => current.filter((download) => download.id !== id));
      pushToast({ title: "Removed", kind: "success" });
    },
    [pushToast],
  );

  const pauseAll = useCallback(async () => {
    await orangeApi.pauseAllDownloads();
    await refresh();
    pushToast({ title: "All downloads paused", kind: "info" });
  }, [pushToast, refresh]);

  const resumeAll = useCallback(async () => {
    await orangeApi.resumeAllDownloads();
    await refresh();
    pushToast({ title: "Resuming all downloads", kind: "info" });
  }, [pushToast, refresh]);

  const retryFailed = useCallback(async () => {
    const ids = await orangeApi.retryFailedDownloads();
    if (ids.length === 0) {
      pushToast({ title: "No failed downloads to retry", kind: "info" });
      return;
    }
    await refresh();
    pushToast({ title: `Retrying ${ids.length} download${ids.length === 1 ? "" : "s"}`, kind: "info" });
  }, [pushToast, refresh]);

  const clearCompleted = useCallback(async () => {
    const ids = await orangeApi.clearCompletedDownloads();
    if (ids.length > 0) removeDownloads(ids);
    pushToast({ title: `Cleared ${ids.length} completed`, kind: "success" });
  }, [pushToast, removeDownloads]);

  const clearCancelled = useCallback(async () => {
    const ids = await orangeApi.clearCancelledDownloads();
    if (ids.length > 0) removeDownloads(ids);
    pushToast({ title: `Cleared ${ids.length} cancelled`, kind: "success" });
  }, [pushToast, removeDownloads]);

  const clearFailed = useCallback(async () => {
    const ids = await orangeApi.clearFailedDownloads();
    if (ids.length > 0) removeDownloads(ids);
    pushToast({ title: `Cleared ${ids.length} failed`, kind: "success" });
  }, [pushToast, removeDownloads]);

  const updateDownloadOptions = useCallback(
    async (request: UpdateDownloadOptionsRequest) => {
      const download = await orangeApi.updateDownloadOptions(request);
      upsertDownload(download);
      pushToast({ title: "Download options saved", message: download.fileName, kind: "success" });
    },
    [pushToast, upsertDownload],
  );

  const verifyDownloadChecksum = useCallback(
    async (id: string, expectedSha256?: string | null): Promise<ChecksumResult> => {
      const result = await orangeApi.verifyDownloadChecksum(id, expectedSha256 ?? null);
      await refresh();
      if (result.matched === true) {
        pushToast({ title: "Checksum verified", kind: "success" });
      } else if (result.matched === false) {
        pushToast({ title: "Checksum mismatch", kind: "error" });
      } else {
        pushToast({ title: "Checksum computed", kind: "info" });
      }
      return result;
    },
    [pushToast, refresh],
  );

  const cleanupHistory = useCallback(async () => {
    const ids = await orangeApi.cleanupHistory();
    if (ids.length > 0) removeDownloads(ids);
    pushToast({ title: `History cleanup removed ${ids.length}`, kind: "success" });
  }, [pushToast, removeDownloads]);

  const openFile = useCallback(
    async (id: string) => {
      try {
        await orangeApi.openFile(id);
      } catch (error) {
        pushToast({ title: "Could not open file", message: String(error), kind: "error" });
      }
    },
    [pushToast],
  );

  const revealInExplorer = useCallback(
    async (id: string) => {
      try {
        await orangeApi.revealInExplorer(id);
      } catch (error) {
        pushToast({ title: "Could not reveal file", message: String(error), kind: "error" });
      }
    },
    [pushToast],
  );

  const moveDownload = useCallback(
    async (id: string) => {
      const directory = await orangeApi.pickDirectory();
      if (!directory) return;
      const download = await orangeApi.moveDownload(id, directory);
      upsertDownload(download);
      pushToast({ title: "Moved", message: download.fileName, kind: "success" });
    },
    [pushToast, upsertDownload],
  );

  const renameDownload = useCallback(
    async (id: string, newName: string) => {
      const download = await orangeApi.renameDownload(id, newName);
      upsertDownload(download);
      pushToast({ title: "Renamed", message: download.fileName, kind: "success" });
    },
    [pushToast, upsertDownload],
  );

  // --- Settings actions ------------------------------------------------------

  const updateSettings = useCallback(
    async (request: UpdateSettingsRequest) => {
      const appSettings = await orangeApi.updateSettings(completeSettingsRequest(settings, request));
      setSettings(appSettings);
      pushToast({ title: "Settings saved", kind: "success" });
    },
    [pushToast, settings],
  );

  // --- Return ----------------------------------------------------------------

  return {
    downloads,
    summary,
    loading,
    loadError,
    settings,
    refresh,
    startDownloads,
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
    startVideoDownload,
    openAddModal,
    addModalInitialUrl,
    clearOpenAddModal: () => setOpenAddModal(false),
  };
}

// --- Settings merge ----------------------------------------------------------

function completeSettingsRequest(
  current: AppSettings,
  request: UpdateSettingsRequest,
): UpdateSettingsRequest {
  const has = (key: keyof UpdateSettingsRequest) =>
    Object.prototype.hasOwnProperty.call(request, key);

  return {
    defaultDownloadDirectory: has("defaultDownloadDirectory")
      ? request.defaultDownloadDirectory ?? ""
      : current.defaultDownloadDirectory,
    defaultSpeedLimitBps: has("defaultSpeedLimitBps")
      ? request.defaultSpeedLimitBps ?? null
      : current.defaultSpeedLimitBps,
    globalSpeedLimitBps: has("globalSpeedLimitBps")
      ? request.globalSpeedLimitBps ?? null
      : current.globalSpeedLimitBps,
    maxConcurrentDownloads: has("maxConcurrentDownloads")
      ? request.maxConcurrentDownloads ?? current.maxConcurrentDownloads
      : current.maxConcurrentDownloads,
    autoResumeInterruptedDownloads: has("autoResumeInterruptedDownloads")
      ? Boolean(request.autoResumeInterruptedDownloads)
      : current.autoResumeInterruptedDownloads,
    closeToTray: has("closeToTray") ? Boolean(request.closeToTray) : current.closeToTray,
    notificationsEnabled: has("notificationsEnabled")
      ? Boolean(request.notificationsEnabled)
      : current.notificationsEnabled,
    notificationSound: has("notificationSound")
      ? Boolean(request.notificationSound)
      : current.notificationSound,
    backgroundUpdateNotifications: has("backgroundUpdateNotifications")
      ? Boolean(request.backgroundUpdateNotifications)
      : current.backgroundUpdateNotifications,
    autoOpenFolderOnCompletion: has("autoOpenFolderOnCompletion")
      ? Boolean(request.autoOpenFolderOnCompletion)
      : current.autoOpenFolderOnCompletion,
    historyRetentionDays: has("historyRetentionDays")
      ? request.historyRetentionDays ?? null
      : current.historyRetentionDays,
    historyMaxRows: has("historyMaxRows")
      ? request.historyMaxRows ?? null
      : current.historyMaxRows,
    firstRunCompleted: has("firstRunCompleted")
      ? Boolean(request.firstRunCompleted)
      : current.firstRunCompleted,
    theme: has("theme") ? request.theme ?? current.theme : current.theme,
  };
}
