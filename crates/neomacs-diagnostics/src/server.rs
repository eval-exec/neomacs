//! The axum HTTP server: routes, handlers, and the dedicated-thread entry point.

use std::convert::Infallible;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use axum::Router;
use axum::extract::{Query, State};
use axum::http::{StatusCode, header};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Json, Response};
use axum::routing::get;
use tokio_stream::wrappers::IntervalStream;
use tokio_stream::{Stream, StreamExt};

use crate::metrics::MetricsSnapshot;

/// Source of metrics for the server.
///
/// Implemented for any `Fn` returning a snapshot, so the host binary can supply
/// a closure over its producers without this crate depending on the VM.
pub trait MetricsProvider: Send + Sync + 'static {
    fn snapshot(&self) -> MetricsSnapshot;
}

impl<F> MetricsProvider for F
where
    F: Fn() -> MetricsSnapshot + Send + Sync + 'static,
{
    fn snapshot(&self) -> MetricsSnapshot {
        self()
    }
}

/// Drives an on-demand Lisp CPU profile capture on the Lisp thread.
///
/// The host binary implements this by sending tasks over the eval-thread
/// channel and waking the Lisp thread. The methods are synchronous (they block
/// on cross-thread channels), so handlers call them via `spawn_blocking`.
pub trait ProfileController: Send + Sync + 'static {
    /// Whether a capture can actually run: true only when there is a live Lisp
    /// thread. Batch/headless returns false.
    fn is_live(&self) -> bool;
    /// Begin a CPU capture. `Ok(true)` = started a fresh session; `Ok(false)` =
    /// a CPU profiler session is already running (must not be hijacked, so do
    /// not proceed); `Err` = failure (dead thread / timeout).
    fn start(&self, interval_ns: u64) -> Result<bool, String>;
    /// Stop the capture and return folded stacks, clearing the log so the next
    /// capture starts clean.
    fn stop_and_fold(&self) -> Result<String, String>;
    /// Fire-and-forget stop + discard, to clean up a capture whose request was
    /// cancelled before its clean stop ran.
    fn abort(&self);
}

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) provider: Arc<dyn MetricsProvider>,
    pub(crate) profiler: Option<Arc<dyn ProfileController>>,
    /// Serializes captures so two concurrent `/profile` requests can't race the
    /// single shared profiler (double-start / stop-after-stop).
    pub(crate) capture_lock: Arc<tokio::sync::Mutex<()>>,
    /// Ring of recent captures, so `/diff` can compare two.
    pub(crate) captures: Arc<std::sync::Mutex<crate::capture_store::CaptureStore>>,
    /// Serializes native (pprof-rs) captures — only one SIGPROF profiler can be
    /// installed at a time. Independent of `capture_lock` (the Lisp poll-sampler
    /// and native SIGPROF sampler do not conflict). A `std` mutex (not tokio) so
    /// it can be held *inside* the blocking capture task, tracking the profiler
    /// guard's true lifetime even if the request future is cancelled.
    pub(crate) native_capture_lock: Arc<std::sync::Mutex<()>>,
}

/// Build the diagnostics HTTP router.
pub fn router(
    provider: Arc<dyn MetricsProvider>,
    profiler: Option<Arc<dyn ProfileController>>,
) -> Router {
    let state = AppState {
        provider,
        profiler,
        capture_lock: Arc::new(tokio::sync::Mutex::new(())),
        captures: Arc::new(std::sync::Mutex::new(
            crate::capture_store::CaptureStore::new(16),
        )),
        native_capture_lock: Arc::new(std::sync::Mutex::new(())),
    };
    Router::new()
        .route("/", get(index))
        .route("/metrics", get(metrics))
        .route("/live", get(live))
        .route("/profile/lisp.folded", get(profile_folded))
        .route("/profile/lisp.svg", get(profile_svg))
        .route("/profile/lisp.pprof", get(profile_pprof))
        .route("/profile/lisp/callers", get(callers))
        .route("/profile/native.svg", get(profile_native_svg))
        .route("/profile/native.pprof", get(profile_native_pprof))
        .route("/report", get(report))
        .route("/captures", get(captures_list))
        .route("/diff", get(diff))
        .with_state(state)
}

/// Self-describing index so an agent can navigate with no prior knowledge.
async fn index() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "name": "neomacs-diagnostics",
        "version": env!("CARGO_PKG_VERSION"),
        "endpoints": {
            "/": "this index",
            "/metrics": "current performance metrics snapshot (JSON)",
            "/live": "server-sent events stream of metrics (~1 Hz)",
            "/profile/lisp.folded?secs=N": "capture N s of Lisp CPU as folded stacks (text)",
            "/profile/lisp.svg?secs=N": "the same capture rendered as an SVG flamegraph",
            "/profile/lisp.pprof?secs=N": "the same capture as pprof protobuf (go tool pprof)",
            "/profile/lisp/callers?fn=NAME&secs=N": "callers/callees of NAME (JSON)",
            "/profile/native.svg?secs=N": "native (Rust) CPU flamegraph — GC/layout/render/dispatch",
            "/profile/native.pprof?secs=N": "native CPU as pprof protobuf (go tool pprof)",
            "/report?secs=N&top=K&sort=self|total": "ranked top-K CPU hotspots (JSON)",
            "/captures": "list stored captures (id, samples, age) for /diff",
            "/diff?before=A&after=B&top=K": "ranked self% change between two captures (JSON)"
        }
    }))
}

/// Current metrics snapshot as JSON.
async fn metrics(State(state): State<AppState>) -> Json<MetricsSnapshot> {
    Json(state.provider.snapshot())
}

/// Server-sent events stream: one JSON snapshot per event at ~1 Hz.
///
/// `tokio::time::interval` yields its first tick immediately, so a subscriber
/// receives a frame right away rather than after the first interval.
async fn live(State(state): State<AppState>) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let provider = state.provider.clone();
    let interval = tokio::time::interval(Duration::from_millis(1000));
    let stream = IntervalStream::new(interval).map(move |_| {
        let snap = provider.snapshot();
        // `json_data` only fails for non-serializable values; ours always is.
        Ok(Event::default()
            .json_data(snap)
            .expect("MetricsSnapshot is serializable"))
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Query parameters shared by the capture endpoints.
#[derive(serde::Deserialize)]
struct CaptureParams {
    secs: Option<u64>,
    top: Option<usize>,
    sort: Option<String>,
    #[serde(rename = "fn")]
    function: Option<String>,
    before: Option<u64>,
    after: Option<u64>,
}

/// Capture folded stacks and store them in the ring, returning the assigned id
/// (so a client can `/diff` against it later) alongside the folded text.
async fn capture_and_store(
    state: &AppState,
    secs: u64,
) -> Result<(u64, String), (StatusCode, String)> {
    let folded = do_capture(state, secs).await?;
    let id = state.captures.lock().unwrap().store(folded.clone());
    Ok((id, folded))
}

/// Ensures a started capture is stopped even if the request future is dropped
/// (client disconnect) or an error unwinds before the clean stop runs. Without
/// this, the profiler would run orphaned and contaminate the next capture.
struct CaptureCleanup {
    ctrl: Option<Arc<dyn ProfileController>>,
}

impl CaptureCleanup {
    fn disarm(&mut self) {
        self.ctrl = None;
    }
}

impl Drop for CaptureCleanup {
    fn drop(&mut self) {
        if let Some(ctrl) = self.ctrl.take() {
            ctrl.abort();
        }
    }
}

/// Run one serialized capture: gate on liveness, acquire the capture lock, start
/// sampling (without hijacking a running session), wait `secs`, then stop and
/// return folded stacks. `503` when no Lisp thread, `409` when a session is
/// already running.
async fn do_capture(state: &AppState, secs: u64) -> Result<String, (StatusCode, String)> {
    let Some(ctrl) = state.profiler.clone() else {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "diagnostics profiling is not available".to_string(),
        ));
    };
    // Real liveness gate: batch/headless has a controller but no Lisp thread, so
    // reject before enqueuing a start task into a never-drained channel.
    if !ctrl.is_live() {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "no live Lisp thread (batch mode or editor not started)".to_string(),
        ));
    }
    let secs = secs.clamp(1, 60);
    let interval_ns = 1_000_000; // 1 ms sampling

    let _guard = state.capture_lock.lock().await;

    // start / stop block on crossbeam channels; keep them off the async runtime
    // thread so /metrics and /live stay responsive.
    let c = ctrl.clone();
    let started = tokio::task::spawn_blocking(move || c.start(interval_ns))
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map_err(|e| (StatusCode::SERVICE_UNAVAILABLE, e))?;
    if !started {
        return Err((
            StatusCode::CONFLICT,
            "a CPU profiler session is already running (interactive profiler-start?)".to_string(),
        ));
    }

    // We own the session now — guarantee it is stopped even if this request is
    // cancelled or an error unwinds below.
    let mut cleanup = CaptureCleanup {
        ctrl: Some(ctrl.clone()),
    };

    tokio::time::sleep(Duration::from_secs(secs)).await;

    let c = ctrl.clone();
    let folded = tokio::task::spawn_blocking(move || c.stop_and_fold())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .map_err(|e| (StatusCode::SERVICE_UNAVAILABLE, e))?;
    cleanup.disarm();
    Ok(folded)
}

async fn profile_folded(State(state): State<AppState>, Query(p): Query<CaptureParams>) -> Response {
    match capture_and_store(&state, p.secs.unwrap_or(5)).await {
        Ok((id, folded)) => (
            [
                ("content-type", "text/plain; charset=utf-8".to_string()),
                ("x-capture-id", id.to_string()),
            ],
            folded,
        )
            .into_response(),
        Err((code, msg)) => (code, msg).into_response(),
    }
}

async fn profile_svg(State(state): State<AppState>, Query(p): Query<CaptureParams>) -> Response {
    match do_capture(&state, p.secs.unwrap_or(5)).await {
        Ok(folded) => match crate::flamegraph::folded_to_svg(&folded, "Neomacs Lisp CPU") {
            Ok(svg) => ([(header::CONTENT_TYPE, "image/svg+xml")], svg).into_response(),
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e).into_response(),
        },
        Err((code, msg)) => (code, msg).into_response(),
    }
}

async fn profile_pprof(State(state): State<AppState>, Query(p): Query<CaptureParams>) -> Response {
    match do_capture(&state, p.secs.unwrap_or(5)).await {
        Ok(folded) => {
            let pb = crate::pprof::folded_to_pprof(&folded);
            ([(header::CONTENT_TYPE, "application/octet-stream")], pb).into_response()
        }
        Err((code, msg)) => (code, msg).into_response(),
    }
}

async fn callers(State(state): State<AppState>, Query(p): Query<CaptureParams>) -> Response {
    let Some(func) = p.function.clone() else {
        return (
            StatusCode::BAD_REQUEST,
            "missing required query parameter ?fn=FUNCTION\n",
        )
            .into_response();
    };
    match do_capture(&state, p.secs.unwrap_or(5)).await {
        Ok(folded) => {
            Json(crate::report::callers_report_from_folded(&folded, &func)).into_response()
        }
        Err((code, msg)) => (code, msg).into_response(),
    }
}

async fn report(State(state): State<AppState>, Query(p): Query<CaptureParams>) -> Response {
    let secs = p.secs.unwrap_or(5);
    match capture_and_store(&state, secs).await {
        Ok((id, folded)) => {
            let top = p.top.unwrap_or(20).clamp(1, 1000);
            let sort_by_self = p.sort.as_deref() != Some("total");
            let rep = crate::report::cpu_report_from_folded(&folded, top, sort_by_self);
            (
                [("x-capture-id", id.to_string())],
                Json(serde_json::json!({
                    "capture_id": id,
                    "window_secs": secs.clamp(1, 60),
                    "sort": if sort_by_self { "self" } else { "total" },
                    "report": rep,
                })),
            )
                .into_response()
        }
        Err((code, msg)) => (code, msg).into_response(),
    }
}

/// Run a native (pprof-rs) capture off the runtime thread, serialized so only
/// one SIGPROF profiler is installed at a time.
async fn capture_native(
    state: &AppState,
    secs: u64,
    svg: bool,
) -> Result<Vec<u8>, (StatusCode, String)> {
    let secs = secs.clamp(1, 60);
    let freq = crate::native::DEFAULT_FREQ_HZ;
    let lock = state.native_capture_lock.clone();
    tokio::task::spawn_blocking(move || {
        // Hold the serialization lock for the profiler's TRUE lifetime. Taking
        // it inside the blocking task (not around the await) makes it immune to
        // request cancellation — a dropped future can't cancel spawn_blocking,
        // so a concurrent capture correctly queues here instead of racing
        // pprof-rs's internal guard into a spurious error. Poison is recoverable
        // (the guarded unit has no invariant).
        let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
        if svg {
            crate::native::capture_native_svg(secs, freq)
        } else {
            crate::native::capture_native_pprof(secs, freq)
        }
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))
}

async fn profile_native_svg(
    State(state): State<AppState>,
    Query(p): Query<CaptureParams>,
) -> Response {
    match capture_native(&state, p.secs.unwrap_or(5), true).await {
        Ok(svg) => ([("content-type", "image/svg+xml")], svg).into_response(),
        Err((code, msg)) => (code, msg).into_response(),
    }
}

async fn profile_native_pprof(
    State(state): State<AppState>,
    Query(p): Query<CaptureParams>,
) -> Response {
    match capture_native(&state, p.secs.unwrap_or(5), false).await {
        Ok(pb) => ([("content-type", "application/octet-stream")], pb).into_response(),
        Err((code, msg)) => (code, msg).into_response(),
    }
}

/// List the stored captures available for `/diff`.
async fn captures_list(State(state): State<AppState>) -> Json<serde_json::Value> {
    let list = state.captures.lock().unwrap().list();
    let items: Vec<serde_json::Value> = list
        .into_iter()
        .map(|(id, total, secs_ago)| {
            serde_json::json!({ "id": id, "total_samples": total, "captured_secs_ago": secs_ago })
        })
        .collect();
    Json(serde_json::json!({ "captures": items }))
}

/// Compare two stored captures: `/diff?before=A&after=B&top=K`.
async fn diff(State(state): State<AppState>, Query(p): Query<CaptureParams>) -> Response {
    let (Some(before_id), Some(after_id)) = (p.before, p.after) else {
        return (
            StatusCode::BAD_REQUEST,
            "diff requires ?before=ID&after=ID (see /captures)\n",
        )
            .into_response();
    };
    // Clone the two captures out, then release the store lock before the
    // CPU-bound diff so it never blocks /metrics or /live on the runtime thread.
    let folded = {
        let store = state.captures.lock().unwrap_or_else(|e| e.into_inner());
        match (store.folded(before_id), store.folded(after_id)) {
            (Some(b), Some(a)) => Some((b.to_string(), a.to_string())),
            _ => None,
        }
    };
    let Some((before, after)) = folded else {
        return (
            StatusCode::NOT_FOUND,
            "unknown capture id (it may have been evicted; see /captures)\n",
        )
            .into_response();
    };
    let top = p.top.unwrap_or(20).clamp(1, 1000);
    let report = crate::report::diff_from_folded(&before, &after, top);
    Json(serde_json::json!({
        "before": before_id,
        "after": after_id,
        "diff": report,
    }))
    .into_response()
}

/// Configuration for the diagnostics server.
pub struct DiagnosticsConfig {
    /// TCP port to bind on `127.0.0.1`.
    pub port: u16,
}

/// Parse a TCP port from a string, rejecting empty, zero, and out-of-range
/// values. Used to interpret the `NEOMACS_DIAGNOSTICS_PORT` env var.
pub fn port_from_str(raw: &str) -> Option<u16> {
    match raw.trim().parse::<u16>() {
        Ok(port) if port != 0 => Some(port),
        _ => None,
    }
}

/// Spawn the diagnostics server on a dedicated OS thread running a
/// current-thread tokio runtime. Binds `127.0.0.1:<port>` only.
///
/// `profiler` is `None` in batch/headless (no Lisp thread) — the capture
/// endpoints then return `503`. Bind/serve errors are logged, not panicked, so
/// a diagnostics failure never brings down the editor.
pub fn spawn(
    config: DiagnosticsConfig,
    provider: Arc<dyn MetricsProvider>,
    profiler: Option<Arc<dyn ProfileController>>,
) -> std::io::Result<JoinHandle<()>> {
    thread::Builder::new()
        .name("neomacs-diagnostics".to_string())
        .spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    tracing::error!("diagnostics: failed to build tokio runtime: {e}");
                    return;
                }
            };
            rt.block_on(async move {
                let addr = std::net::SocketAddr::from(([127, 0, 0, 1], config.port));
                let listener = match tokio::net::TcpListener::bind(addr).await {
                    Ok(l) => l,
                    Err(e) => {
                        tracing::error!("diagnostics: bind {addr} failed: {e}");
                        return;
                    }
                };
                tracing::info!("neomacs diagnostics listening on http://{addr}");
                let app = router(provider, profiler);
                if let Err(e) = axum::serve(listener, app).await {
                    tracing::error!("diagnostics: server error: {e}");
                }
            });
        })
}
