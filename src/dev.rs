use axum::Router;
use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{DebounceEventResult, Debouncer, RecommendedCache, new_debouncer};
use snafu::{Report, prelude::*};
use std::{path::Path, sync::mpsc, time::Duration};
use tokio::signal;
use tower_http::services::ServeDir;

use crate::{build::Builder, path::Roots};

const HOST: &str = "127.0.0.1";
const PORT: u16 = 3000;

/// Build once, then serve `out` and rebuild on every change under `base`
pub async fn serve_and_watch(
    roots: Roots,
    builder: impl Builder + Send + 'static,
) -> Result<(), snafu::Whatever> {
    builder.build()?;

    // The watcher must stay in scope while the server runs
    let (_watcher, events) = watch(roots.base())?;

    tokio::select! {
        result = serve(roots.out()) => result?,
        result = rebuild(events, builder) => result?,
        result = signal::ctrl_c() => {
            result.whatever_context("Unknown error")?;
            log::info!("Shutting down");
        }
    }
    Ok(())
}

/// Serve the built site from `out`
async fn serve(out: &Path) -> Result<(), snafu::Whatever> {
    let app = Router::new().fallback_service(ServeDir::new(out));
    let listener = tokio::net::TcpListener::bind((HOST, PORT))
        .await
        .whatever_context(format!("Could not bind {HOST}:{PORT}"))?;
    log::info!("Serving files at http://{HOST}:{PORT}");
    axum::serve(listener, app)
        .await
        .whatever_context("Error while serving the app")?;
    Ok(())
}

/// Watch `base` recursively and forward debounced events
fn watch(
    base: &Path,
) -> Result<
    (
        Debouncer<RecommendedWatcher, RecommendedCache>,
        mpsc::Receiver<DebounceEventResult>,
    ),
    snafu::Whatever,
> {
    let (tx, rx) = mpsc::channel();
    let mut debouncer = new_debouncer(Duration::from_millis(150), None, move |result| {
        // The rebuild loop is gone; drop the event
        let _ = tx.send(result);
    })
    .whatever_context("Could not create the file watcher")?;
    debouncer
        .watch(base, RecursiveMode::Recursive)
        .whatever_context(format!("Could not watch {}", base.display()))?;
    log::info!("Watching {} for changes", base.display());

    Ok((debouncer, rx))
}

/// Rebuild the site for every debounced batch of file events
async fn rebuild(
    events: mpsc::Receiver<DebounceEventResult>,
    builder: impl Builder + Send + 'static,
) -> Result<(), snafu::Whatever> {
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
                    log::error!("Build failed: {}", Report::from_error(error));
                }
            }
        }
    })
    .await
    .whatever_context("Error rebuilding the site")?;

    Ok(())
}
