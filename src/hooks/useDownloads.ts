import { useCallback, useEffect, useMemo, useState } from "react";
import type { ToastKind } from "./useToasts";
import { onDownloadFinished, onDownloadProgress, orangeApi } from "../lib/tauri";
import type {
  AppSettings,
  Download,
  DownloadSummary,
  StartDownloadRequest,
  UpdateSettingsRequest,
} from "../lib/types";

type PushToast = (toast: { title: string; message?: string; kind: ToastKind }) => void;

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

export function useDownloads(pushToast: PushToast) {
  const [downloads, setDownloads] = useState<Download[]>([]);
  const [settings, setSettings] = useState<AppSettings>({
    defaultDownloadDirectory: "",
    defaultSpeedLimitBps: null,
  });
  const [loading, setLoading] = useState(true);

  const upsertDownload = useCallback((download: Download) => {
    setDownloads((current) => {
      const index = current.findIndex((item) => item.id === download.id);

      if (index < 0) {
        return sortDownloads([...current, download]);
      }

      const previous = current[index];
      const next = [...current];
      next[index] = download;

      if (previous.status !== download.status || previous.createdAt !== download.createdAt) {
        return sortDownloads(next);
      }

      return next;
    });
  }, []);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const [items, appSettings] = await Promise.all([
        orangeApi.listDownloads(),
        orangeApi.getSettings(),
      ]);
      setDownloads(sortDownloads(items));
      setSettings(appSettings);
    } catch (error) {
      pushToast({
        title: "Unable to load downloads",
        message: String(error),
        kind: "error",
      });
    } finally {
      setLoading(false);
    }
  }, [pushToast]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    let unlistenProgress: (() => void) | undefined;
    let unlistenFinished: (() => void) | undefined;
    let disposed = false;

    void onDownloadProgress((download) => {
      upsertDownload(download);
    }).then((unlisten) => {
      if (disposed) {
        unlisten();
      } else {
        unlistenProgress = unlisten;
      }
    });

    void onDownloadFinished((download) => {
      upsertDownload(download);
      pushToast({
        title: "Download complete",
        message: download.fileName,
        kind: "success",
      });
    }).then((unlisten) => {
      if (disposed) {
        unlisten();
      } else {
        unlistenFinished = unlisten;
      }
    });

    return () => {
      disposed = true;
      unlistenProgress?.();
      unlistenFinished?.();
    };
  }, [pushToast, upsertDownload]);

  const summary = useMemo<DownloadSummary>(() => {
    return downloads.reduce(
      (total, download) => {
        total.total += 1;

        if (download.status === "downloading") {
          total.active += 1;
        }

        if (download.status === "queued") {
          total.queued += 1;
        }

        if (download.status === "completed") {
          total.completed += 1;
        }

        if (download.status === "failed") {
          total.failed += 1;
        }

        return total;
      },
      { total: 0, active: 0, queued: 0, completed: 0, failed: 0 },
    );
  }, [downloads]);

  const startDownload = useCallback(
    async (request: StartDownloadRequest) => {
      const download = await orangeApi.startDownload(request);
      upsertDownload(download);
      pushToast({
        title: "Download queued",
        message: download.fileName,
        kind: "info",
      });
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
      pushToast({ title: "Resumed", message: download.fileName, kind: "info" });
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

  const updateSettings = useCallback(
    async (request: UpdateSettingsRequest) => {
      const appSettings = await orangeApi.updateSettings(request);
      setSettings(appSettings);
      pushToast({ title: "Settings saved", kind: "success" });
    },
    [pushToast],
  );

  return {
    downloads,
    summary,
    loading,
    settings,
    refresh,
    startDownload,
    pauseDownload,
    resumeDownload,
    cancelDownload,
    deleteDownload,
    updateSettings,
  };
}
