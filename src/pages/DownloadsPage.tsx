// SPDX-FileCopyrightText: 2025 MisplacedOrange
// SPDX-License-Identifier: GPL-3.0-only

import { useDeferredValue, useEffect, useMemo, useRef, useState } from "react";
import { clsx } from "clsx";
import { AddDownloadModal } from "../components/AddDownloadModal";
import { VideoDownloadModal } from "../components/VideoDownloadModal";
import { DownloadCard } from "../components/DownloadCard";
import type {
  AppSettings,
  ChecksumResult,
  Download,
  DownloadSummary,
  StartDownloadRequest,
  StartVideoDownloadRequest,
  UpdateDownloadOptionsRequest,
} from "../lib/types";
import { formatBytes, formatSpeed } from "../lib/format";

interface DownloadsPageProps {
  downloads: Download[];
  summary: DownloadSummary;
  loading: boolean;
  loadError: string;
  settings: AppSettings;
  onRefresh: () => Promise<void>;
  onStartDownloads: (requests: StartDownloadRequest[]) => Promise<void>;
  onStartVideoDownload: (request: StartVideoDownloadRequest) => Promise<void>;
  onPauseDownload: (id: string) => void;
  onResumeDownload: (id: string) => void;
  onCancelDownload: (id: string) => void;
  onDeleteDownload: (id: string) => void;
  onUpdateDownloadOptions: (request: UpdateDownloadOptionsRequest) => Promise<void>;
  onVerifyDownloadChecksum: (id: string, expectedSha256?: string | null) => Promise<ChecksumResult>;
  onOpenFile: (id: string) => void;
  onRevealInExplorer: (id: string) => void;
  onMoveDownload: (id: string) => void;
  onRenameDownload: (id: string, newName: string) => Promise<void>;
  onPauseAll: () => void;
  onResumeAll: () => void;
  onRetryFailed: () => void;
  onClearCompleted: () => void;
  onClearCancelled: () => void;
  onClearFailed: () => void;
  onCleanupHistory: () => void;
  onNavigateToSettings: () => void;
  openAddModal?: boolean;
  addModalInitialUrl?: string;
  onAddModalOpened?: () => void;
}

// --- Constants & utilities ---------------------------------------------------

type FilterId = "all" | "active" | "completed" | "failed";
type SortId = "newest" | "oldest" | "name" | "size" | "progress";

const filters: Array<{ id: FilterId; label: string }> = [
  { id: "all", label: "All" },
  { id: "active", label: "Active" },
  { id: "completed", label: "Completed" },
  { id: "failed", label: "Failed" },
];

const sortOptions: Array<{ id: SortId; label: string }> = [
  { id: "newest", label: "Newest first" },
  { id: "oldest", label: "Oldest first" },
  { id: "name", label: "Name A–Z" },
  { id: "size", label: "Largest first" },
  { id: "progress", label: "Progress" },
];

function readStoredValue(key: string): string | null {
  try {
    return window.localStorage.getItem(key);
  } catch {
    return null;
  }
}

function storeValue(key: string, value: string): void {
  try {
    window.localStorage.setItem(key, value);
  } catch {
    // Persistence is optional; keep the queue usable for this session.
  }
}

function storedSort(): SortId {
  const value = readStoredValue("orangedl.downloadSort");
  return sortOptions.some((option) => option.id === value) ? value as SortId : "newest";
}

function storedCompactMode(): boolean {
  return readStoredValue("orangedl.compactDownloads") === "true";
}

function storedPinnedIds(): Set<string> {
  try {
    const value = JSON.parse(readStoredValue("orangedl.pinnedDownloads") ?? "[]");
    return new Set(Array.isArray(value) ? value.filter((id): id is string => typeof id === "string") : []);
  } catch {
    return new Set();
  }
}

function sourceName(url: string): string {
  try {
    return new URL(url).hostname.replace(/^www\./, "").toLowerCase() || "Unknown source";
  } catch {
    return "Unknown source";
  }
}

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
  document.body.appendChild(link);
  link.click();
  link.remove();
  window.setTimeout(() => URL.revokeObjectURL(href), 0);
}

function parseImportDefinitions(text: string): StartDownloadRequest[] {
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    return [];
  }
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
      };
    })
    .filter((request): request is StartDownloadRequest => Boolean(request));
}

// --- Component ---------------------------------------------------------------

export function DownloadsPage({
  downloads,
  summary,
  loading,
  loadError,
  settings,
  onRefresh,
  onStartDownloads,
  onStartVideoDownload,
  onPauseDownload,
  onResumeDownload,
  onCancelDownload,
  onDeleteDownload,
  onUpdateDownloadOptions,
  onVerifyDownloadChecksum,
  onOpenFile,
  onRevealInExplorer,
  onMoveDownload,
  onRenameDownload,
  onPauseAll,
  onResumeAll,
  onRetryFailed,
  onClearCompleted,
  onClearCancelled,
  onClearFailed,
  onCleanupHistory,
  onNavigateToSettings,
  openAddModal,
  addModalInitialUrl,
  onAddModalOpened,
}: DownloadsPageProps) {
  // --- State -----------------------------------------------------------------

  const [modalOpen, setModalOpen] = useState(false);
  const [draftUrl, setDraftUrl] = useState("");
  const [videoModalOpen, setVideoModalOpen] = useState(false);
  const [videoModalUrl, setVideoModalUrl] = useState("");
  const [search, setSearch] = useState("");
  const deferredSearch = useDeferredValue(search);
  const [filter, setFilter] = useState<FilterId>("all");
  const [sort, setSort] = useState<SortId>(storedSort);
  const [source, setSource] = useState("all");
  const [compact, setCompact] = useState(storedCompactMode);
  const [pinnedIds, setPinnedIds] = useState<Set<string>>(storedPinnedIds);
  const [dragActive, setDragActive] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const [online, setOnline] = useState(() => navigator.onLine);
  const searchRef = useRef<HTMLInputElement>(null);
  const importInputRef = useRef<HTMLInputElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  // --- Derived state ---------------------------------------------------------

  const totalSpeed = useMemo(
    () =>
      downloads
        .filter((d) => d.status === "downloading")
        .reduce((total, d) => total + d.speedBps, 0),
    [downloads],
  );

  const aggregateProgress = useMemo(() => {
    let downloadedBytes = 0;
    let totalBytes = 0;
    let unknownSizeCount = 0;
    for (const download of downloads) {
      if (!["queued", "downloading", "paused"].includes(download.status)) continue;
      if (download.totalBytes && download.totalBytes > 0) {
        totalBytes += download.totalBytes;
        downloadedBytes += Math.min(download.downloadedBytes, download.totalBytes);
      } else {
        unknownSizeCount += 1;
      }
    }
    return {
      downloadedBytes,
      totalBytes,
      unknownSizeCount,
      percent: totalBytes > 0 ? Math.round((downloadedBytes / totalBytes) * 100) : null,
    };
  }, [downloads]);

  const sourceOptions = useMemo(() => {
    const counts = new Map<string, number>();
    for (const download of downloads) {
      const name = sourceName(download.url);
      counts.set(name, (counts.get(name) ?? 0) + 1);
    }
    return [...counts]
      .map(([name, count]) => ({ name, count }))
      .sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: "base" }));
  }, [downloads]);
  const activeSource =
    source === "all" || sourceOptions.some((option) => option.name === source)
      ? source
      : "all";

  const visibleDownloads = useMemo(() => {
    const needle = deferredSearch.trim().toLowerCase();
    const matches = downloads.filter((download) => {
      const matchesSearch =
        !needle ||
        download.fileName.toLowerCase().includes(needle) ||
        download.url.toLowerCase().includes(needle);
      const matchesFilter =
        filter === "all" ||
        (filter === "active" && ["queued", "downloading", "paused"].includes(download.status)) ||
        download.status === filter;
      const matchesSource = activeSource === "all" || sourceName(download.url) === activeSource;
      return matchesSearch && matchesFilter && matchesSource;
    });
    return [...matches].sort((a, b) => {
      const pinnedOrder = Number(pinnedIds.has(b.id)) - Number(pinnedIds.has(a.id));
      if (pinnedOrder !== 0) return pinnedOrder;
      if (sort === "oldest") return Date.parse(a.createdAt) - Date.parse(b.createdAt);
      if (sort === "name") return a.fileName.localeCompare(b.fileName, undefined, { sensitivity: "base" });
      if (sort === "size") return (b.totalBytes ?? -1) - (a.totalBytes ?? -1);
      if (sort === "progress") return b.progress - a.progress;
      return Date.parse(b.createdAt) - Date.parse(a.createdAt);
    });
  }, [activeSource, downloads, filter, deferredSearch, pinnedIds, sort]);

  const filterCounts = useMemo(() => {
    let active = 0;
    let completed = 0;
    let failed = 0;
    for (const download of downloads) {
      if (["queued", "downloading", "paused"].includes(download.status)) active += 1;
      if (download.status === "completed") completed += 1;
      if (download.status === "failed") failed += 1;
    }
    return { all: downloads.length, active, completed, failed };
  }, [downloads]);

  // --- Effects ---------------------------------------------------------------

  useEffect(() => {
    storeValue("orangedl.downloadSort", sort);
  }, [sort]);

  useEffect(() => {
    storeValue("orangedl.compactDownloads", String(compact));
  }, [compact]);

  useEffect(() => {
    storeValue("orangedl.pinnedDownloads", JSON.stringify([...pinnedIds]));
  }, [pinnedIds]);

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

  // Close the More menu on outside click or Escape
  useEffect(() => {
    if (!menuOpen) return;
    function handlePointerDown(event: MouseEvent) {
      if (menuRef.current && !menuRef.current.contains(event.target as Node)) {
        setMenuOpen(false);
      }
    }
    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") setMenuOpen(false);
    }
    window.addEventListener("mousedown", handlePointerDown);
    window.addEventListener("keydown", handleKeyDown);
    return () => {
      window.removeEventListener("mousedown", handlePointerDown);
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, [menuOpen]);

  // Open modal when triggered externally (tray / deep link)
  useEffect(() => {
    if (openAddModal) {
      setDraftUrl(addModalInitialUrl ?? "");
      setModalOpen(true);
      onAddModalOpened?.();
    }
  }, [openAddModal, addModalInitialUrl, onAddModalOpened]);

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

  // --- Handlers --------------------------------------------------------------

  function openWithUrl(url: string) {
    setDraftUrl(url);
    setModalOpen(true);
  }

  function togglePinned(id: string) {
    setPinnedIds((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
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

  // --- Render ----------------------------------------------------------------

  const hasActive = downloads.some((d) => d.status === "downloading" || d.status === "queued");
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
      <header className="downloads-heading">
        <div>
          <p className="page-eyebrow">Transfer workspace</p>
          <h1>Downloads</h1>
        </div>
        <div className="queue-snapshot" aria-label="Queue summary">
          <span><strong className="tabular">{summary.active}</strong> active</span>
          <span><strong className="tabular">{summary.completed}</strong> complete</span>
          <span><strong className="tabular">{summary.failed}</strong> failed</span>
        </div>
      </header>
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
              <span className="chip-count">{filterCounts[item.id]}</span>
            </button>
          ))}
        </div>

        <label className="download-search">
          <span aria-hidden="true">
            <svg width="14" height="14" viewBox="0 0 14 14" fill="none" aria-hidden="true">
              <circle cx="5.5" cy="5.5" r="4" stroke="currentColor" strokeWidth="1.5"/>
              <line x1="8.7" y1="8.7" x2="13" y2="13" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"/>
            </svg>
          </span>
          <input
            ref={searchRef}
            value={search}
            onChange={(event) => setSearch(event.target.value)}
            placeholder="Name or URL"
            aria-label="Search downloads"
            aria-keyshortcuts="Control+f"
          />
        </label>

        <label className="download-sort">
          <span className="sr-only">Sort downloads</span>
          <select
            value={sort}
            onChange={(event) => setSort(event.target.value as SortId)}
            aria-label="Sort downloads"
          >
            {sortOptions.map((option) => (
              <option key={option.id} value={option.id}>{option.label}</option>
            ))}
          </select>
        </label>

        {sourceOptions.length > 1 ? (
          <label className="download-sort download-source-filter">
            <span className="sr-only">Filter downloads by source</span>
            <select
              value={activeSource}
              onChange={(event) => setSource(event.target.value)}
              aria-label="Filter downloads by source"
            >
              <option value="all">All sources ({downloads.length})</option>
              {sourceOptions.map((option) => (
                <option key={option.name} value={option.name}>
                  {option.name} ({option.count})
                </option>
              ))}
            </select>
          </label>
        ) : null}

        <div className="toolbar-cluster">
          {hasActive && (
            <button type="button" onClick={onPauseAll} className="toolbar-btn">
              Pause all
            </button>
          )}
          {hasPaused && (
            <button type="button" onClick={onResumeAll} className="toolbar-btn">
              Resume all
            </button>
          )}
          {hasFailed && (
            <button type="button" onClick={onRetryFailed} className="toolbar-btn">
              Retry failed
            </button>
          )}

          <div className="more-menu" ref={menuRef}>
            <button
              type="button"
              className="toolbar-btn"
              onClick={() => setMenuOpen((open) => !open)}
              aria-haspopup="menu"
              aria-expanded={menuOpen}
              aria-label="More actions"
            >
              More
              <span aria-hidden="true" className="more-caret">▾</span>
            </button>
            {menuOpen && (
              <div className="more-menu-panel" role="menu">
                <button type="button" role="menuitem" onClick={() => { setMenuOpen(false); importInputRef.current?.click(); }}>
                  Import URLs…
                </button>
                <button
                  type="button"
                  role="menuitem"
                  onClick={() => {
                    setMenuOpen(false);
                    exportFile("orangedl-history.json", "application/json", JSON.stringify(downloads, null, 2));
                  }}
                >
                  Export JSON
                </button>
                <button
                  type="button"
                  role="menuitem"
                  onClick={() => {
                    setMenuOpen(false);
                    exportFile("orangedl-history.csv", "text/csv", historyCsv(downloads));
                  }}
                >
                  Export CSV
                </button>
                <button
                  type="button"
                  role="menuitemcheckbox"
                  aria-checked={compact}
                  onClick={() => setCompact((value) => !value)}
                >
                  {compact ? "Use comfortable rows" : "Use compact rows"}
                </button>
                <div className="more-menu-separator" role="separator" />
                {hasCompleted && (
                  <button type="button" role="menuitem" onClick={() => { setMenuOpen(false); onClearCompleted(); }}>
                    Clear completed
                  </button>
                )}
                {hasCancelled && (
                  <button type="button" role="menuitem" onClick={() => { setMenuOpen(false); onClearCancelled(); }}>
                    Clear cancelled
                  </button>
                )}
                {hasFailed && (
                  <button type="button" role="menuitem" onClick={() => { setMenuOpen(false); onClearFailed(); }}>
                    Clear failed
                  </button>
                )}
                <button type="button" role="menuitem" onClick={() => { setMenuOpen(false); onCleanupHistory(); }}>
                  Cleanup history
                </button>
              </div>
            )}
          </div>

          <button
            type="button"
            onClick={() => { setDraftUrl(""); setModalOpen(true); }}
            className="add-download-button"
            aria-keyshortcuts="Control+n"
          >
            <span aria-hidden="true">+</span>
            Add download
          </button>
        </div>

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

      {!online || loadError ? (
        <div className="download-state-strip" role="status">
          {!online ? <span>Network unavailable. Active transfers will resume when the connection returns.</span> : null}
          {loadError ? (
            <span className="load-error-state">
              Could not load downloads. {loadError}
              <button type="button" onClick={() => void onRefresh()}>Retry</button>
            </span>
          ) : null}
        </div>
      ) : null}

      <div className={clsx("download-table min-h-0 flex-1 overflow-auto", compact && "is-compact")}>
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
          <div className="download-table-body">
            {visibleDownloads.map((download) => (
              <DownloadCard
                key={download.id}
                download={download}
                pinned={pinnedIds.has(download.id)}
                onTogglePinned={togglePinned}
                onDownloadAgain={openWithUrl}
                onPause={onPauseDownload}
                onResume={onResumeDownload}
                onCancel={onCancelDownload}
                onDelete={onDeleteDownload}
                onUpdateOptions={onUpdateDownloadOptions}
                onVerifyChecksum={onVerifyDownloadChecksum}
                onOpenFile={onOpenFile}
                onRevealInExplorer={onRevealInExplorer}
                onMove={onMoveDownload}
                onRename={onRenameDownload}
              />
            ))}
          </div>
        ) : (
          <button type="button" onClick={() => setModalOpen(true)} className="empty-download-state">
            <div>
              <span className="empty-state-icon">+</span>
              <p className="empty-state-title">
                {downloads.length === 0 ? "Your queue is ready" : "No downloads match this view"}
              </p>
              <p className="empty-state-sub">
                {downloads.length === 0
                  ? "Add a direct link, paste a media URL, or drop a text file here."
                  : "Try another filter or clear the search."}
              </p>
            </div>
          </button>
        )}
      </div>

      <footer className="download-statusbar">
        <div className="flex items-center gap-2">
          <span className="statusbar-label">Total speed</span>
          <span className="speed-pill">{formatSpeed(totalSpeed)}</span>
        </div>
        {summary.active > 0 && aggregateProgress.percent != null ? (
          <div
            className="queue-progress-summary"
            aria-label={`Queue progress ${aggregateProgress.percent} percent, ${formatBytes(aggregateProgress.downloadedBytes)} of ${formatBytes(aggregateProgress.totalBytes)}`}
          >
            <span
              className="queue-progress-track"
              role="progressbar"
              aria-valuemin={0}
              aria-valuemax={100}
              aria-valuenow={aggregateProgress.percent}
            >
              <span style={{ width: `${aggregateProgress.percent}%` }} />
            </span>
            <span className="statusbar-label truncate">
              {aggregateProgress.percent}% · {formatBytes(aggregateProgress.downloadedBytes)} of {formatBytes(aggregateProgress.totalBytes)}
              {aggregateProgress.unknownSizeCount > 0 ? ` · ${aggregateProgress.unknownSizeCount} unknown` : ""}
            </span>
          </div>
        ) : (
          <div className="statusbar-label truncate">
            {summary.active > 0 ? `${summary.active} active · sizes pending` : `${summary.completed} completed`}
          </div>
        )}
      </footer>

      {dragActive ? (
        <div className="drag-overlay-backdrop pointer-events-none absolute inset-0 z-20 grid place-items-center">
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
        onVideoUrl={(url) => {
          setVideoModalUrl(url);
          setVideoModalOpen(true);
        }}
      />
      <VideoDownloadModal
        open={videoModalOpen}
        url={videoModalUrl}
        defaultDirectory={settings.defaultDownloadDirectory}
        onClose={() => setVideoModalOpen(false)}
        onSubmit={onStartVideoDownload}
      />
    </div>
  );
}
