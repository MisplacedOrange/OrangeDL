import { useMemo, useState } from "react";
import { clsx } from "clsx";
import { AddDownloadModal } from "../components/AddDownloadModal";
import { DownloadCard } from "../components/DownloadCard";
import type { Download, DownloadSummary, StartDownloadRequest } from "../lib/types";
import { formatSpeed } from "../lib/format";

interface DownloadsPageProps {
  downloads: Download[];
  summary: DownloadSummary;
  loading: boolean;
  defaultSpeedLimitBps: number | null;
  onStartDownload: (request: StartDownloadRequest) => Promise<void>;
  onPauseDownload: (id: string) => void;
  onResumeDownload: (id: string) => void;
  onCancelDownload: (id: string) => void;
  onDeleteDownload: (id: string) => void;
}

type FilterId = "all" | "active" | "history";

const filters: Array<{ id: FilterId; label: string }> = [
  { id: "all", label: "All" },
  { id: "active", label: "Active" },
  { id: "history", label: "History" },
];

export function DownloadsPage({
  downloads,
  summary,
  loading,
  defaultSpeedLimitBps,
  onStartDownload,
  onPauseDownload,
  onResumeDownload,
  onCancelDownload,
  onDeleteDownload,
}: DownloadsPageProps) {
  const [modalOpen, setModalOpen] = useState(false);
  const [draftUrl, setDraftUrl] = useState("");
  const [search, setSearch] = useState("");
  const [filter, setFilter] = useState<FilterId>("all");
  const [dragActive, setDragActive] = useState(false);

  const totalSpeed = useMemo(
    () =>
      downloads
        .filter((download) => download.status === "downloading")
        .reduce((total, download) => total + download.speedBps, 0),
    [downloads],
  );

  const visibleDownloads = useMemo(() => {
    const needle = search.trim().toLowerCase();

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
  }, [downloads, filter, search]);

  function openWithUrl(url: string) {
    setDraftUrl(url);
    setModalOpen(true);
  }

  function handleDrop(event: React.DragEvent<HTMLDivElement>) {
    event.preventDefault();
    setDragActive(false);

    const text = event.dataTransfer.getData("text/uri-list") || event.dataTransfer.getData("text/plain");
    const match = text.match(/https?:\/\/\S+/i);

    if (match) {
      openWithUrl(match[0].trim());
    }
  }

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
            value={search}
            onChange={(event) => setSearch(event.target.value)}
            placeholder="Name or URL"
          />
        </label>

        <button
          type="button"
          onClick={() => {
            setDraftUrl("");
            setModalOpen(true);
          }}
          className="add-download-button"
        >
          <span aria-hidden="true">+</span>
          Add download
        </button>
      </div>

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
      </section>

      <div className="download-table min-h-0 flex-1 overflow-auto">
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
                onPause={onPauseDownload}
                onResume={onResumeDownload}
                onCancel={onCancelDownload}
                onDelete={onDeleteDownload}
              />
            ))}
          </div>
        ) : (
          <button type="button" onClick={() => setModalOpen(true)} className="empty-download-state">
            <div>
              <span className="empty-state-icon">+</span>
              <p className="mt-4 text-lg font-bold text-stone-300">No downloads match this view</p>
              <p className="mt-1 text-sm text-stone-500">Paste or drop an HTTP/HTTPS URL to start.</p>
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
        <div className="truncate text-stone-500">
          {summary.active > 0 ? `${summary.active} active, ${summary.queued} queued` : `${summary.completed} completed`}
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
        defaultSpeedLimitBps={defaultSpeedLimitBps}
        onClose={() => setModalOpen(false)}
        onSubmit={onStartDownload}
      />
    </div>
  );
}
