//! Shared e2e test infrastructure for browser-driving Playwright tests.
//!
//! Provides three things in one place:
//!
//! 1. A single shared `Playwright` + `Browser` initialised lazily on
//!    first use, reused across every test in the binary. Cuts ~1s of
//!    cold-start overhead per test.
//! 2. A per-test `BrowserContext` for storage isolation, opened on
//!    every call to [`with_traced_page`]. Cookies / localStorage / etc.
//!    don't leak between tests.
//! 3. Trace recording: each test records a Playwright trace from the
//!    moment the context opens. On panic, the trace is saved to
//!    `target/playwright-traces/<name>.zip`; on success, it's
//!    discarded. Drag a saved trace into `npx playwright show-trace
//!    target/playwright-traces/<name>.zip` (or the online viewer at
//!    https://trace.playwright.dev) to step through what happened.
//!
//! `tracing_subscriber` is initialised once on first use so
//! playwright-rs's `#[tracing::instrument]` spans surface in test
//! output. Filter via `RUST_LOG`; defaults to `info`.

use std::future::Future;
use std::path::PathBuf;
use std::sync::OnceLock;

use futures::FutureExt as _;
use playwright_rs::protocol::{TracingStartOptions, TracingStopOptions};
use playwright_rs::{Page, Playwright};
use tracing_subscriber::EnvFilter;

static TRACING_INIT: OnceLock<()> = OnceLock::new();

fn init_tracing() {
    TRACING_INIT.get_or_init(|| {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(
                EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
            )
            .with_target(true)
            .with_test_writer()
            .compact()
            .try_init();
    });
}

fn trace_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("playwright-traces")
}

/// Run an async test body with a fresh `BrowserContext` + `Page`,
/// recording a Playwright trace. On panic, the trace is saved to
/// `target/playwright-traces/<name>.zip`. On success, the trace is
/// discarded.
///
/// `name` should be the test function's own name (used as the trace
/// filename). Each test in the binary contributes one trace per
/// failed run, zero traces per successful run.
pub async fn with_traced_page<F, Fut>(name: &str, body: F)
where
    F: FnOnce(Page) -> Fut,
    Fut: Future<Output = ()>,
{
    init_tracing();
    // Per-test Playwright + Browser. Sharing across tests via a static
    // `OnceCell<Browser>` deadlocks because each `#[tokio::test]` has
    // its own runtime; the Browser's internal channels die when the
    // first test's runtime ends and subsequent tests block forever
    // waiting for responses on those dead channels.
    let pw = Playwright::launch().await.expect("launch playwright");
    let browser = pw.chromium().launch().await.expect("launch chromium");
    let context = browser.new_context().await.expect("new browser context");

    let tracing_handle = context.tracing().await.expect("tracing handle");
    tracing_handle
        .start(Some(
            TracingStartOptions::default()
                .name(name)
                .screenshots(true)
                .snapshots(true),
        ))
        .await
        .expect("tracing start");

    let page = context.new_page().await.expect("new page");

    // Run the body, catching panics so we can save the trace on
    // failure and re-raise the panic afterwards. `AssertUnwindSafe` is
    // safe here because the body owns its `Page` and we don't observe
    // any partially-mutated borrowed state across the unwind.
    let body_result = std::panic::AssertUnwindSafe(body(page))
        .catch_unwind()
        .await;

    let trace_path = if body_result.is_err() {
        let dir = trace_dir();
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join(format!("{name}.zip"));
        eprintln!(
            "e2e test `{name}` failed; saving Playwright trace to {}",
            path.display()
        );
        Some(path)
    } else {
        None
    };

    let stop_opts = trace_path
        .as_ref()
        .map(|p| TracingStopOptions::default().path(p.to_string_lossy()));

    let _ = tracing_handle.stop(stop_opts).await;
    let _ = context.close().await;
    let _ = browser.close().await;

    // Dogfood `playwright-rs-trace`: parse the saved trace and print any
    // actions that recorded an error, so failure diagnostics include the
    // recorded action context inline (no need to drop into the trace
    // viewer for a quick triage). The trace viewer remains the right
    // tool for deep inspection — this is just the "what failed" preview.
    if let Some(path) = &trace_path
        && let Err(e) = print_failed_actions(path)
    {
        eprintln!("(trace parse failed: {e:?})");
    }

    if let Err(panic) = body_result {
        std::panic::resume_unwind(panic);
    }
}

fn print_failed_actions(trace_path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut reader = playwright_rs_trace::open(trace_path)?;
    let mut found = false;
    for action in reader.actions()? {
        let action = action?;
        if let Some(error) = &action.error {
            if !found {
                eprintln!("  trace summary — failed actions:");
                found = true;
            }
            eprintln!(
                "    {}.{}{}: {} — {}",
                action.class,
                action.method,
                action
                    .title
                    .as_deref()
                    .map(|t| format!(" ({t})"))
                    .unwrap_or_default(),
                error.name,
                error.message,
            );
        }
    }
    if !found {
        eprintln!("  trace summary — no recorded actions errored (failure was on the Rust side)");
    }
    Ok(())
}

/// Convenience wrapper around [`with_traced_page`] that navigates to a
/// rendered chapter HTML file under `book/build/html/<slug>.html`
/// before handing the page to the test body. Every e2e test in
/// `tests/e2e_callouts.rs` targets the same chapter, so this saves
/// per-test boilerplate.
pub async fn with_traced_chapter<F, Fut>(name: &str, chapter_slug: &str, body: F)
where
    F: FnOnce(Page) -> Fut + Send + 'static,
    Fut: Future<Output = ()> + Send,
{
    let chapter_html = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("book")
        .join("build")
        .join("html")
        .join(format!("{chapter_slug}.html"));
    let url = format!("file://{}", chapter_html.display());
    with_traced_page(name, move |page| async move {
        page.goto(&url, None).await.expect("goto chapter");
        body(page).await;
    })
    .await;
}
