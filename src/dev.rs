use anyhow::{Context, Result};
use axum::Router;
use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{DebounceEventResult, Debouncer, RecommendedCache, new_debouncer};
use std::{sync::mpsc, time::Duration};
use tokio::signal;
use tower_http::services::ServeDir;

use crate::{build::Builder, path::AbsPath};

const HOST: &str = "127.0.0.1";
const PORT: u16 = 3000;

/// Build once, then serve `out` and rebuild on every change under `base`
pub async fn serve_and_watch(
    base: AbsPath,
    out: AbsPath,
    builder: impl Builder + Send + 'static,
) -> Result<()> {
    builder.build()?;

    // The watcher must stay in scope while the server runs
    let (_watcher, events) = watch(&base)?;

    tokio::select! {
        result = serve(out) => result?,
        result = rebuild(events, builder) => result?,
        result = signal::ctrl_c() => {
            result?;
            log::info!("Shutting down");
        }
    }
    Ok(())
}

/// Serve the built site from `out`
async fn serve(out: AbsPath) -> Result<()> {
    let app = Router::new().fallback_service(ServeDir::new(out.as_ref()));
    let listener = tokio::net::TcpListener::bind((HOST, PORT))
        .await
        .with_context(|| format!("Could not bind {HOST}:{PORT}"))?;
    log::info!("Serving files at http://{HOST}:{PORT}");
    axum::serve(listener, app).await?;
    Ok(())
}

/// Watch `base` recursively and forward debounced events
fn watch(
    base: &AbsPath,
) -> Result<(
    Debouncer<RecommendedWatcher, RecommendedCache>,
    mpsc::Receiver<DebounceEventResult>,
)> {
    let (tx, rx) = mpsc::channel();
    let mut debouncer = new_debouncer(Duration::from_millis(150), None, move |result| {
        // The rebuild loop is gone; drop the event
        let _ = tx.send(result);
    })
    .context("Could not create the file watcher")?;
    debouncer
        .watch(base.as_ref(), RecursiveMode::Recursive)
        .with_context(|| format!("Could not watch {}", base.inner().display()))?;
    log::info!("Watching {} for changes", base.inner().display());

    Ok((debouncer, rx))
}

/// Rebuild the site for every debounced batch of file events
async fn rebuild(
    events: mpsc::Receiver<DebounceEventResult>,
    builder: impl Builder + Send + 'static,
) -> Result<()> {
    // The std receiver blocks, so keep it off the Tokio runtime
    tokio::task::spawn_blocking(move || {
        while let Ok(result) = events.recv() {
            let events = match result {
                Ok(events) => events,
                Err(errors) => {
                    for error in errors {
                        log::warn!("File watcher error: {error}");
                    }
                    continue;
                }
            };
            if events.is_empty() {
                continue;
            }

            let changed = events
                .iter()
                .map(|event| event.event.paths.len())
                .sum::<usize>();
            log::info!("Detected changes in {changed} file(s), rebuilding");

            match builder.build() {
                Ok(()) => log::info!("Build succeeded"),
                Err(error) => {
                    // Keep serving the previous build: the author can fix the
                    // source and trigger another rebuild
                    log::error!("Build failed: {error:#}");
                }
            }
        }
    })
    .await?;
    Ok(())
}
