use crate::database;
use crate::downloader::DownloadManager;
use crate::models::ExecutorSummary;
use sqlx::SqlitePool;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use tokio::sync::Notify;

pub struct DownloadExecutor {
    app: AppHandle,
    pool: SqlitePool,
    manager: Arc<DownloadManager>,
    notify: Arc<Notify>,
}

impl DownloadExecutor {
    pub fn new(
        app: AppHandle,
        pool: SqlitePool,
        manager: Arc<DownloadManager>,
        notify: Arc<Notify>,
    ) -> Self {
        Self {
            app,
            pool,
            manager,
            notify,
        }
    }

    pub fn start(self) {
        tauri::async_runtime::spawn(async move {
            self.run().await;
        });
    }

    async fn run(self) {
        loop {
            self.tick().await;

            tokio::select! {
                _ = self.notify.notified() => {}
                _ = tokio::time::sleep(Duration::from_secs(5)) => {}
            }
        }
    }

    async fn tick(&self) {
        let max_concurrent = database::get_max_concurrent_downloads(&self.pool)
            .await
            .unwrap_or(3) as usize;

        let active_count = self.manager.active_count();

        if active_count < max_concurrent {
            let slots = (max_concurrent - active_count) as u32;
            match database::get_next_queued(&self.pool, slots).await {
                Ok(queued) => {
                    for download in queued {
                        self.manager.spawn_queued_download(download.id);
                    }
                }
                Err(e) => eprintln!("OrangeDL executor queue error: {e}"),
            }
        }

        self.emit_summary().await;
    }

    async fn emit_summary(&self) {
        match database::get_executor_summary(&self.pool).await {
            Ok(summary) => {
                let tooltip = build_tray_tooltip(&summary);
                let _ = self.app.emit("executor-summary", &summary);
                if let Some(tray) = self.app.tray_by_id("main") {
                    let _ = tray.set_tooltip(Some(tooltip.as_str()));
                }
            }
            Err(e) => eprintln!("OrangeDL executor summary error: {e}"),
        }
    }
}

fn build_tray_tooltip(summary: &ExecutorSummary) -> String {
    if summary.active == 0 && summary.queued == 0 {
        return "OrangeDL".to_string();
    }
    let mut parts: Vec<String> = Vec::new();
    if summary.active > 0 {
        parts.push(format!("{} active", summary.active));
    }
    if summary.queued > 0 {
        parts.push(format!("{} queued", summary.queued));
    }
    let speed = format_speed_bps(summary.total_speed_bps);
    format!("OrangeDL \u{2014} {} \u{2014} {}", parts.join(", "), speed)
}

fn format_speed_bps(bps: f64) -> String {
    if bps < 1024.0 {
        format!("{:.0} B/s", bps)
    } else if bps < 1_048_576.0 {
        format!("{:.1} KB/s", bps / 1024.0)
    } else {
        format!("{:.1} MB/s", bps / 1_048_576.0)
    }
}
