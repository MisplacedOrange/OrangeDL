import { useDeferredValue, useEffect, useMemo, useRef, useState } from "react";
import { clsx } from "clsx";
import { AddDownloadModal } from "../components/AddDownloadModal";
import { DownloadCard } from "../components/DownloadCard";
import type {
  AppSettings,
  ChecksumResult,
  Download,
  DownloadSummary,
  ExecutorSummary,
  StartDownloadRequest,
  UpdateDownloadOptionsRequest,
  UpdateSettingsRequest,
} from "../lib/types";
import { formatSpeed } from "../lib/format";

interface DownloadsPageProps {
  downloads: Download[];
  summary: DownloadSummary;
  executorSummary: ExecutorSummary;
  loading: boolean;
  loadError: string;
  settings: AppSettings;
  onStartDownloads: (requests: StartDownloadRequest[]) => Promise<void>;
  onUpdateSettings: (request: UpdateSettingsRequest) => Promise<void>;
  onPauseDownload: (id: string) => void;
  onResumeDownload: (id: string) => void;
  onCancelDownload: (id: string) => void;
  onDeleteDownload: (id: string) => void;
  onReorderDownload: (id: string, position: number) => void;
  onUpdateDownloadOptions: (request: UpdateDownloadOptionsRequest) => Promise<void>;
  onVerifyDownloadChecksum: (id: string, expectedSha256?: string | null) => Promise<ChecksumResult>;
  onOpenFile: (id: string) => void;
  onRevealInExplorer: (id: string) => void;
  onPauseAll: () => void;
  onResumeAll: () => void;
  onRetryFailed: () => void;
  onClearCompleted: () => void;
  onClearCancelled: () => void;
  onClearFailed: () => void;
  onCleanupHistory: () => void;
  onNavigateToSettings: () => void;
  openAddModal?: boolean;
  onAddModalOpened?: () => void;
}

type FilterId = "all" | "active" | "history";

const filters: Array<{ id: FilterId; label: string }> = [
  { id: "all", label: "All" },
  { id: "active", label: "Active" },
  { id: "history", label: "History" },
];

const VIRTUALIZE_AFTER = 300;
const VIRTUAL_ROW_HEIGHT = 76;
const VIRTUAL_OVERSCAN = 8;

function extractUrls(text: string): string[] {
  return Array.from(new Set(text.match(/https?:\/\/[^\s"'<>]+/gi) ?? []));
}

function csvValue(value: unknown): string {
  const text = value == null ? "" : String(value);
  return `"${text.replace(/"/g, '""')}"`;
}

function historyCsv(downloads: Download[]): string {
  const headers = [
    "id",
    "url",
    "fileName",
    "destination",
    "status",
    "downloadedBytes",
    "totalBytes",
    "priority",
    "createdAt",
    "updatedAt",
    "error",
  ];
  const rows = downloads.map((download) =>
    [
      download.id,
      download.url,
      download.fileName,
      download.destination,
      download.status,
      download.downloadedBytes,
      download.totalBytes,
      download.priority,
      download.createdAt,
      download.updatedAt,
      download.error,
    ]
      .map(csvValue)
      .join(","),
  );
  return [headers.join(","), ...rows].join("\n");
}

function exportFile(fileName: string, mimeType: string, body: string) {
  const blob = new Blob([body], { type: mimeType });
  const href = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = href;
  link.download = fileName;
  link.click();
  URL.revokeObjectURL(href);
}

function parseImportDefinitions(text: string): StartDownloadRequest[] {
  const parsed = JSON.parse(text) as unknown;
  const values = Array.isArray(parsed) ? parsed : [parsed];
  return values
    .map((value): StartDownloadRequest | null => {
      if (typeof value === "string") return { url: value };
      if (!value || typeof value !== "object") return null;
      const record = value as Record<string, unknown>;
      if (typeof record.url !== "string") return null;
      return {
        url: record.url,
        fileName: typeof record.fileName === "string" ? record.fileName : null,
        directory: typeof record.directory === "string" ? record.directory : null,
        speedLimitBps:
          typeof record.speedLimitBps === "number" && record.speedLimitBps > 0
            ? record.speedLimitBps
            : null,
        priority: typeof record.priority === "number" ? record.priority : 0,
        queuePosition:
          typeof record.queuePosition === "number" ? record.queuePosition : null,
      };
    })
    .filter((request): request is StartDownloadRequest => Boolean(request));
}

export function DownloadsPage({
  downloads,
  summary,
  executorSummary,
  loading,
  loadError,
  settings,
  onStartDownloads,
  onUpdateSettings,
  onPauseDownload,
  onResumeDownload,
  onCancelDownload,
  onDeleteDownload,
  onReorderDownload,
  onUpdateDownloadOptions,
  onVerifyDownloadChecksum,
  onOpenFile,
  onRevealInExplorer,
  onPauseAll,
  onResumeAll,
  onRetryFailed,
  onClearCompleted,
  onClearCancelled,
  onClearFailed,
  onCleanupHistory,
  onNavigateToSettings,
  openAddModal,
  onAddModalOpened,
}: DownloadsPageProps) {
  const [modalOpen, setModalOpen] = useState(false);
  const [draftUrl, setDraftUrl] = useState("");
  const [search, setSearch] = useState("");
  const deferredSearch = useDeferredValue(search);
  const [filter, setFilter] = useState<FilterId>("all");
  const [dragActive, setDragActive] = useState(false);
  const [draggedQueueId, setDraggedQueueId] = useState<string | null>(null);
  const [dragOverQueueId, setDragOverQueueId] = useState<string | null>(null);
  const [globalSpeedDraft, setGlobalSpeedDraft] = useState("");
  const [online, setOnline] = useState(() => navigator.onLine);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewportHeight, setViewportHeight] = useState(0);
  const searchRef = useRef<HTMLInputElement>(null);
  const importInputRef = useRef<HTMLInputElement>(null);
  const tableRef = useRef<HTMLDivElement>(null);

  const totalSpeed = useMemo(
    () =>
      downloads
        .filter((d) => d.status === "downloading")
        .reduce((total, d) => total + d.speedBps, 0),
    [downloads],
  );

  const visibleDownloads = useMemo(() => {
    const needle = deferredSearch.trim().toLowerCase();
    return downloads.filter((download) => {
      const matchesSearch =
        !needle ||
        download.fileName.toLowerCase().includes(needle) ||
        download.url.toLowerCase().includes(needle);
      const matchesFilter =
        filter === "all" ||
        (filter === "active" && ["queued", "downloading", "paused"].includes(download.status)) ||
        (filter === "history" && ["completed", "failed", "cancelled"].includes(download.status));
      return matchesSearch && matchesFilter;
    });
  }, [downloads, filter, deferredSearch]);

  const queuedVisibleDownloads = useMemo(
    () => visibleDownloads.filter((download) => download.status === "queued"),
    [visibleDownloads],
  );

  const useVirtualList = visibleDownloads.length > VIRTUALIZE_AFTER;
  const virtualRange = useMemo(() => {
    if (!useVirtualList) return { start: 0, end: visibleDownloads.length };
    const start = Math.max(0, Math.floor(scrollTop / VIRTUAL_ROW_HEIGHT) - VIRTUAL_OVERSCAN);
    const visibleCount = Math.ceil((viewportHeight || 600) / VIRTUAL_ROW_HEIGHT);
    const end = Math.min(visibleDownloads.length, start + visibleCount + VIRTUAL_OVERSCAN * 2);
    return { start, end };
  }, [scrollTop, useVirtualList, viewportHeight, visibleDownloads.length]);

  const renderedDownloads = useMemo(
    () => visibleDownloads.slice(virtualRange.start, virtualRange.end),
    [visibleDownloads, virtualRange],
  );

  const topSpacer = useVirtualList ? virtualRange.start * VIRTUAL_ROW_HEIGHT : 0;
  const bottomSpacer = useVirtualList
    ? Math.max(0, (visibleDownloads.length - virtualRange.end) * VIRTUAL_ROW_HEIGHT)
    : 0;

  useEffect(() => {
    const value = settings.globalSpeedLimitBps
      ? String(settings.globalSpeedLimitBps / 1024 / 1024)
      : "";
    setGlobalSpeedDraft(value);
  }, [settings.globalSpeedLimitBps]);

  useEffect(() => {
    function handleOnline() {
      setOnline(true);
    }
    function handleOffline() {
      setOnline(false);
    }
    window.addEventListener("online", handleOnline);
    window.addEventListener("offline", handleOffline);
    return () => {
      window.removeEventListener("online", handleOnline);
      window.removeEventListener("offline", handleOffline);
    };
  }, []);

  useEffect(() => {
    const node = tableRef.current;
    if (!node) return;
    setViewportHeight(node.clientHeight);
    const observer = new ResizeObserver(() => setViewportHeight(node.clientHeight));
    observer.observe(node);
    return () => observer.disconnect();
  }, []);

  // Open modal when triggered externally (tray / deep link)
  useEffect(() => {
    if (openAddModal) {
      setDraftUrl("");
      setModalOpen(true);
      onAddModalOpened?.();
    }
  }, [openAddModal, onAddModalOpened]);

  // Keyboard shortcuts
  useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      const tag = (event.target as HTMLElement).tagName;
      const isInput = tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT";

      if (event.ctrlKey || event.metaKey) {
        if (event.key === "n") {
          event.preventDefault();
          setDraftUrl("");
          setModalOpen(true);
        } else if (event.key === "f") {
          event.preventDefault();
          searchRef.current?.focus();
        } else if (event.key === ",") {
          event.preventDefault();
          onNavigateToSettings();
        }
        return;
      }

      if (isInput) return;
    }

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [onNavigateToSettings]);

  function openWithUrl(url: string) {
    setDraftUrl(url);
    setModalOpen(true);
  }

  async function handleDrop(event: React.DragEvent<HTMLDivElement>) {
    event.preventDefault();
    setDragActive(false);
    if (event.dataTransfer.files.length > 0) {
      const texts = await Promise.all(
        Array.from(event.dataTransfer.files)
          .filter((file) => file.type.startsWith("text/") || /\.(txt|url|json)$/i.test(file.name))
          .map((file) => file.text()),
      );
      const urls = extractUrls(texts.join("\n"));
      if (urls.length > 0) openWithUrl(urls.join("\n"));
      return;
    }
    const text =
      event.dataTransfer.getData("text/uri-list") ||
      event.dataTransfer.getData("text/plain");
    const match = text.match(/https?:\/\/\S+/i);
    if (match) openWithUrl(match[0].trim());
  }

  async function handleImportFile(file: File) {
    const text = await file.text();
    if (/\.json$/i.test(file.name)) {
      const requests = parseImportDefinitions(text);
      if (requests.length > 0) await onStartDownloads(requests);
      return;
    }

    const requests = extractUrls(text).map((url) => ({ url }));
    if (requests.length > 0) await onStartDownloads(requests);
  }

  function handleQueueDrop(targetId: string) {
    if (!draggedQueueId || draggedQueueId === targetId) return;
    const position = queuedVisibleDownloads.findIndex((download) => download.id === targetId);
    if (position >= 0) onReorderDownload(draggedQueueId, position);
    setDraggedQueueId(null);
    setDragOverQueueId(null);
  }

  async function saveGlobalSpeedLimit() {
    const numeric = Number(globalSpeedDraft);
    if (!globalSpeedDraft.trim()) {
      await onUpdateSettings({ globalSpeedLimitBps: null });
      return;
    }
    if (Number.isFinite(numeric) && numeric > 0) {
      await onUpdateSettings({ globalSpeedLimitBps: Math.round(numeric * 1024 * 1024) });
    }
  }

  const hasActive = summary.active > 0 || summary.queued > 0;
  const hasPaused = downloads.some((d) => d.status === "paused");
  const hasFailed = summary.failed > 0;
  const hasCompleted = summary.completed > 0;
  const hasCancelled = downloads.some((d) => d.status === "cancelled");

  return (
    <div
      className={clsx("relative flex h-full flex-col bg-client", dragActive && "drag-active")}
      onDragOver={(event) => {
        event.preventDefault();
        setDragActive(true);
      }}
      onDragLeave={() => setDragActive(false)}
      onDrop={handleDrop}
    >
      <div className="download-toolbar">
        <div className="filter-chip" aria-label="Download filters">
          {filters.map((item) => (
            <button
              key={item.id}
              type="button"
              onClick={() => setFilter(item.id)}
              className={filter === item.id ? "active" : ""}
            >
              {item.label}
            </button>
          ))}
        </div>

        <label className="download-search">
          <span>Search</span>
          <input
            ref={searchRef}
            value={search}
            onChange={(event) => setSearch(event.target.value)}
            placeholder="Name or URL"
            aria-label="Search downloads"
            aria-keyshortcuts="Control+f"
          />
        </label>

        <button
          type="button"
          onClick={() => { setDraftUrl(""); setModalOpen(true); }}
          className="add-download-button"
          aria-keyshortcuts="Control+n"
        >
          <span aria-hidden="true">+</span>
          Add download
        </button>

        <input
          ref={importInputRef}
          type="file"
          accept=".txt,.json,text/plain,application/json"
          className="sr-only"
          onChange={(event) => {
            const file = event.target.files?.[0];
            if (file) void handleImportFile(file);
            event.target.value = "";
          }}
        />
      </div>

      {/* Queue controls */}
      <div className="queue-controls" aria-label="Queue controls">
        {hasActive && (
          <button type="button" onClick={onPauseAll} className="queue-btn queue-btn--pause">
            Pause all
          </button>
        )}
        {hasPaused && (
          <button type="button" onClick={onResumeAll} className="queue-btn queue-btn--resume">
            Resume all
          </button>
        )}
        {hasFailed && (
          <button type="button" onClick={onRetryFailed} className="queue-btn queue-btn--retry">
            Retry failed
          </button>
        )}
        {hasCompleted && (
          <button
            type="button"
            onClick={onClearCompleted}
            className="queue-btn queue-btn--clear"
          >
            Clear completed
          </button>
        )}
        {hasCancelled && (
          <button
            type="button"
            onClick={onClearCancelled}
            className="queue-btn queue-btn--clear"
          >
            Clear cancelled
          </button>
        )}
        {hasFailed && (
          <button
            type="button"
            onClick={onClearFailed}
            className="queue-btn queue-btn--clear"
          >
            Clear failed
          </button>
        )}
        <button type="button" onClick={() => importInputRef.current?.click()} className="queue-btn queue-btn--retry">
          Import
        </button>
        <button
          type="button"
          onClick={() => exportFile("orangedl-history.json", "application/json", JSON.stringify(downloads, null, 2))}
          className="queue-btn queue-btn--clear"
        >
          Export JSON
        </button>
        <button
          type="button"
          onClick={() => exportFile("orangedl-history.csv", "text/csv", historyCsv(downloads))}
          className="queue-btn queue-btn--clear"
        >
          Export CSV
        </button>
        <button type="button" onClick={onCleanupHistory} className="queue-btn queue-btn--clear">
          Cleanup
        </button>
      </div>

      {!online || loadError || useVirtualList ? (
        <div className="download-state-strip" role="status">
          {!online ? <span>Offline</span> : null}
          {loadError ? <span>Load error: {loadError}</span> : null}
          {useVirtualList ? (
            <span>
              Large history mode: showing {virtualRange.end - virtualRange.start} of{" "}
              {visibleDownloads.length}
            </span>
          ) : null}
        </div>
      ) : null}

      <section className="queue-strip" aria-label="Queue slots">
        <div>
          <span>Active slots</span>
          <strong>
            {executorSummary.active}/{executorSummary.maxConcurrent}
          </strong>
        </div>
        <div className="slot-rail" aria-hidden="true">
          {Array.from({ length: Math.max(1, executorSummary.maxConcurrent) }, (_, index) => (
            <span key={index} className={index < executorSummary.active ? "filled" : ""} />
          ))}
        </div>
        <div>
          <span>Queue</span>
          <strong>{executorSummary.queued}</strong>
        </div>
        <div>
          <span>Total speed</span>
          <strong>{formatSpeed(executorSummary.totalSpeedBps)}</strong>
        </div>
      </section>

      <section className="download-overview" aria-label="Download summary">
        <div className="download-overview-stat">
          <span>Total</span>
          <strong>{summary.total}</strong>
        </div>
        <div className="download-overview-stat">
          <span>Active</span>
          <strong>{summary.active}</strong>
        </div>
        <div className="download-overview-stat">
          <span>Queued</span>
          <strong>{summary.queued}</strong>
        </div>
        <div className="download-overview-stat">
          <span>Completed</span>
          <strong>{summary.completed}</strong>
        </div>
        <div className="download-overview-stat">
          <span>Slots</span>
          <strong>
            {executorSummary.active}/{executorSummary.maxConcurrent}
          </strong>
        </div>
      </section>

      <div
        ref={tableRef}
        className="download-table min-h-0 flex-1 overflow-auto"
        onScroll={(event) => setScrollTop(event.currentTarget.scrollTop)}
      >
        <div className="download-table-header">
          <span>Name</span>
          <span>Size</span>
          <span>Progress</span>
          <span>Speed</span>
          <span>Added</span>
          <span>Actions</span>
        </div>

        {loading ? (
          <div className="download-skeleton">
            {Array.from({ length: 8 }, (_, index) => (
              <div key={index} className="skeleton-row" />
            ))}
          </div>
        ) : visibleDownloads.length > 0 ? (
          <div className={clsx("download-table-body", useVirtualList && "is-virtual")}>
            {topSpacer > 0 ? <div style={{ height: topSpacer }} /> : null}
            {renderedDownloads.map((download) => (
              <DownloadCard
                key={download.id}
                download={download}
                onPause={onPauseDownload}
                onResume={onResumeDownload}
                onCancel={onCancelDownload}
                onDelete={onDeleteDownload}
                onUpdateOptions={onUpdateDownloadOptions}
                onVerifyChecksum={onVerifyDownloadChecksum}
                onOpenFile={onOpenFile}
                onRevealInExplorer={onRevealInExplorer}
                draggable={download.status === "queued"}
                dragOver={dragOverQueueId === download.id}
                onDragStart={() => setDraggedQueueId(download.id)}
                onDragEnter={() => {
                  if (draggedQueueId && download.status === "queued") {
                    setDragOverQueueId(download.id);
                  }
                }}
                onDragEnd={() => {
                  setDraggedQueueId(null);
                  setDragOverQueueId(null);
                }}
                onDrop={() => handleQueueDrop(download.id)}
              />
            ))}
            {bottomSpacer > 0 ? <div style={{ height: bottomSpacer }} /> : null}
          </div>
        ) : (
          <button type="button" onClick={() => setModalOpen(true)} className="empty-download-state">
            <div>
              <span className="empty-state-icon">+</span>
              <p className="mt-4 text-lg font-bold text-stone-300">No downloads match this view</p>
              <p className="mt-1 text-sm text-stone-500">
                Paste or drop an HTTP/HTTPS URL to start.
              </p>
            </div>
          </button>
        )}
      </div>

      <button type="button" onClick={() => setModalOpen(true)} className="drop-zone">
        <span className="drop-icon">+</span>
        Drop a URL here or add one manually
      </button>

      <footer className="download-statusbar">
        <div className="flex items-center gap-2">
          <span className="text-stone-500">Total speed</span>
          <span className="speed-pill">{formatSpeed(totalSpeed)}</span>
        </div>
        <label className="global-speed-control">
          <span>Global cap MB/s</span>
          <input
            value={globalSpeedDraft}
            onChange={(event) => setGlobalSpeedDraft(event.target.value)}
            onBlur={() => void saveGlobalSpeedLimit()}
            onKeyDown={(event) => {
              if (event.key === "Enter") void saveGlobalSpeedLimit();
            }}
            placeholder="Unlimited"
            inputMode="decimal"
            aria-label="Global speed limit in megabytes per second"
          />
        </label>
        <div className="truncate text-stone-500">
          {summary.active > 0
            ? `${summary.active} active, ${summary.queued} queued`
            : `${summary.completed} completed`}
        </div>
      </footer>

      {dragActive ? (
        <div className="pointer-events-none absolute inset-0 z-20 grid place-items-center bg-black/70">
          <div className="drag-overlay">Drop URL to add download</div>
        </div>
      ) : null}

      <AddDownloadModal
        open={modalOpen}
        initialUrl={draftUrl}
        defaultDirectory={settings.defaultDownloadDirectory}
        defaultSpeedLimitBps={settings.defaultSpeedLimitBps}
        onClose={() => setModalOpen(false)}
        onSubmit={onStartDownloads}
      />
    </div>
  );
}
