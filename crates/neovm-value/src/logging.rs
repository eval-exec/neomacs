//! Process-wide tracing/logging initialization.
//!
//! Two entry points:
//!
//! - [`init`] — for binary entry points. Takes a [`LogTarget`] that
//!   selects the default writer. See [`LogTarget`] for the per-variant
//!   policy and the `NEOMACS_LOG_FILE` override.  Returns a
//!   [`LoggingGuard`] that must be kept alive until process exit so
//!   any file appender can flush its queue.
//!
//! - [`init_for_tests`] — thin wrapper around [`init`] with
//!   [`LogTarget::Test`] for unit/integration tests. Uses
//!   `with_test_writer` so output is captured per-test by the test
//!   harness and only appears on failure.
//!
//! Both honor `RUST_LOG` and bridge the `log` facade into `tracing`
//! (via `tracing_subscriber`'s built-in `LogTracer` install inside
//! `try_init`), so events from crates using the `log` facade
//! (e.g. `cosmic-text`, `wgpu`) flow into the tracing subscriber.

use std::collections::VecDeque;
use std::fmt;
use std::io::IsTerminal;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use parking_lot::Mutex;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::Layer;
use tracing_subscriber::Registry;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

/// Tracing target owned by the cursor-geometry telemetry (`neomacs::cursor_geometry`).
/// Consumed verbatim by the display debug bundle without re-deriving cursor fields.
pub const CURSOR_GEOMETRY_TARGET: &str = "neomacs::cursor_geometry";

const CURSOR_GEOMETRY_RING_CAP: usize = 64;

static CURSOR_GEOMETRY_CAPTURE: AtomicBool = AtomicBool::new(false);
static CURSOR_GEOMETRY_RING: OnceLock<Mutex<VecDeque<String>>> = OnceLock::new();

fn cursor_geometry_ring() -> &'static Mutex<VecDeque<String>> {
    CURSOR_GEOMETRY_RING
        .get_or_init(|| Mutex::new(VecDeque::with_capacity(CURSOR_GEOMETRY_RING_CAP)))
}

/// Enable or disable in-process capture of `neomacs::cursor_geometry` DEBUG
/// events into the bounded ring buffer used by `neomacs-dump-display-state`.
///
/// Rebuilds tracing's callsite interest cache so `tracing::enabled!(target:
/// "neomacs::cursor_geometry", Level::DEBUG)` observes the change immediately
/// while remaining zero-cost when off. Returns the previous state.
pub fn set_cursor_geometry_capture(enabled: bool) -> bool {
    let previous = CURSOR_GEOMETRY_CAPTURE.swap(enabled, Ordering::SeqCst);
    if previous != enabled {
        tracing::callsite::rebuild_interest_cache();
    }
    previous
}

/// Whether in-process `neomacs::cursor_geometry` capture is currently enabled.
pub fn cursor_geometry_capture_enabled() -> bool {
    CURSOR_GEOMETRY_CAPTURE.load(Ordering::Relaxed)
}

/// Return a snapshot of the most recent `neomacs::cursor_geometry` log lines
/// (oldest first, bounded to the last 64 events).
pub fn snapshot_cursor_geometry_lines() -> Vec<String> {
    cursor_geometry_ring().lock().iter().cloned().collect()
}

/// Clear buffered `neomacs::cursor_geometry` lines (used by tests).
pub fn clear_cursor_geometry_lines() {
    cursor_geometry_ring().lock().clear();
}

#[derive(Default)]
struct CursorGeometryMessageVisitor {
    message: Option<String>,
    extras: Vec<String>,
}

impl tracing::field::Visit for CursorGeometryMessageVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn fmt::Debug) {
        if field.name() == "message" {
            self.message = Some(format!("{value:?}"));
        } else {
            self.extras.push(format!("{}={value:?}", field.name()));
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = Some(value.to_string());
        } else {
            self.extras.push(format!("{}={value}", field.name()));
        }
    }
}

struct CursorGeometryCaptureFilter;

impl<S: tracing::Subscriber> tracing_subscriber::layer::Filter<S> for CursorGeometryCaptureFilter {
    fn enabled(
        &self,
        metadata: &tracing::Metadata<'_>,
        _ctx: &tracing_subscriber::layer::Context<'_, S>,
    ) -> bool {
        // `tracing::enabled!` builds `Kind::HINT` metadata (neither `is_event`
        // nor `is_span`), so exclude spans rather than requiring `is_event()`.
        !metadata.is_span()
            && metadata.target() == CURSOR_GEOMETRY_TARGET
            && CURSOR_GEOMETRY_CAPTURE.load(Ordering::Relaxed)
    }

    fn callsite_enabled(
        &self,
        metadata: &'static tracing::Metadata<'static>,
    ) -> tracing::subscriber::Interest {
        if !metadata.is_span()
            && metadata.target() == CURSOR_GEOMETRY_TARGET
            && CURSOR_GEOMETRY_CAPTURE.load(Ordering::Relaxed)
        {
            tracing::subscriber::Interest::always()
        } else {
            tracing::subscriber::Interest::never()
        }
    }

    fn max_level_hint(&self) -> Option<tracing::metadata::LevelFilter> {
        Some(tracing::metadata::LevelFilter::DEBUG)
    }
}

struct CursorGeometryCaptureLayer;

impl<S: tracing::Subscriber> Layer<S> for CursorGeometryCaptureLayer {
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        if event.metadata().target() != CURSOR_GEOMETRY_TARGET {
            return;
        }
        let mut visitor = CursorGeometryMessageVisitor::default();
        event.record(&mut visitor);
        let line = match (visitor.message, visitor.extras.is_empty()) {
            (Some(msg), true) => msg,
            (Some(msg), false) => format!("{msg} {}", visitor.extras.join(" ")),
            (None, false) => visitor.extras.join(" "),
            (None, true) => return,
        };
        let mut ring = cursor_geometry_ring().lock();
        if ring.len() >= CURSOR_GEOMETRY_RING_CAP {
            ring.pop_front();
        }
        ring.push_back(line);
    }
}

/// Where the default writer sends tracing output for a given binary.
///
/// The variants describe *writers*, not runtime kinds: any binary can
/// pick whichever variant is appropriate. The user-facing runtime
/// classification (GUI vs TUI vs build utility) is an argument to this
/// choice, not the choice itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogTarget {
    /// Write to stdout.
    ///
    /// Used by:
    /// - the GUI binary (`neomacs` under `Gui` frontend), where stdout
    ///   is captured to a log file by convention (`> /tmp/neomacs.log 2>&1`);
    /// - build-time utilities (`neomacs-temacs`, `bootstrap-neomacs`),
    ///   whose stdout is captured by the `xtask` driver and surfaced
    ///   in build logs.
    ///
    /// With `NEOMACS_LOG_FILE=<path>`: stdout **and** file.
    Stdout,
    /// Write to a file, never to stdout/stderr.
    ///
    /// Used by the TUI binary (`neomacs -nw` / `--batch` for user-
    /// interactive runs) where writing to stdout or stderr would
    /// corrupt the alt-screen the redisplay engine is drawing into.
    ///
    /// When `NEOMACS_LOG_FILE` is not set, the TUI runs silently (no
    /// file output). Set `NEOMACS_LOG_FILE=<path>` to opt in to file
    /// logging.
    File,
    /// Unit or integration test. Default writer: captured test writer
    /// (`tracing_subscriber::fmt::TestWriter`, visible only on test
    /// failure).
    ///
    /// With `NEOMACS_LOG_FILE=<path>`: test writer **and** file.
    Test,
}

/// Held by the binary's `main` function for the duration of the process so
/// the non-blocking file appender's background worker can flush on
/// shutdown.
///
/// Drop this only when the process is about to exit; dropping it earlier
/// will cause subsequent log lines to be lost from the file output.
#[must_use = "drop the LoggingGuard only at process exit; dropping early loses file log lines"]
pub struct LoggingGuard {
    _file: Option<tracing_appender::non_blocking::WorkerGuard>,
}

type BoxedLayer = Box<dyn Layer<Registry> + Send + Sync + 'static>;

/// Initialize tracing for a binary entry point.
///
/// The `target` argument selects the default writer:
///
/// | target | default writer | with `NEOMACS_LOG_FILE=<path>` |
/// |---|---|---|
/// | [`LogTarget::Stdout`] | stdout | stdout + file |
/// | [`LogTarget::File`] | silent (no file) | file at `<path>` |
/// | [`LogTarget::Test`] | captured test writer | test writer + file |
///
/// [`LogTarget::File`] produces no file output unless `NEOMACS_LOG_FILE`
/// is set, so TUI runs are silent by default.
///
/// Behavior shared across all targets:
///
/// - Filter comes from `RUST_LOG`; defaults to `warn`.
/// - Bridges crates using the `log` facade into tracing (via
///   `tracing_subscriber`'s own `LogTracer` install inside
///   `try_init`).
/// - Idempotent — safe to call multiple times. Only the first call
///   sets up the global subscriber; subsequent calls return an empty
///   guard.
/// - If the configured log file fails to open, a warning is printed to
///   stderr and the function continues with the default writer only
///   (for [`LogTarget::Stdout`] and [`LogTarget::Test`]) or silently
///   (for [`LogTarget::File`]).
///
/// Legacy: `NEOMACS_LOG_TO_FILE=1` is still accepted and is equivalent
/// to setting `NEOMACS_LOG_FILE=neomacs-{pid}.log` in the current
/// directory. New call sites should prefer `NEOMACS_LOG_FILE`.
pub fn init(target: LogTarget) -> LoggingGuard {
    static INIT: OnceLock<()> = OnceLock::new();
    let mut guard: Option<tracing_appender::non_blocking::WorkerGuard> = None;
    INIT.get_or_init(|| {
        install_first_panic_capture();
        guard = init_inner(target);
    });
    LoggingGuard { _file: guard }
}

/// Install a panic hook that records the FIRST panic's location and
/// backtrace to `/tmp/neomacs-first-panic.txt` before chaining to the
/// default hook. The default Rust panic hook can itself panic when
/// dropping objects that touch tracing machinery during unwind,
/// producing the "thread panicked while processing panic" abort
/// message that hides the original panic. Writing to a file
/// synchronously (no tracing, no allocator surprises) before chaining
/// to the default hook ensures we always have the first panic's site
/// on disk.
fn install_first_panic_capture() {
    use std::io::Write as _;
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("/tmp/neomacs-first-panic.txt")
        {
            let loc = info
                .location()
                .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
                .unwrap_or_else(|| "<unknown>".to_string());
            let payload = info
                .payload()
                .downcast_ref::<&'static str>()
                .map(|s| s.to_string())
                .or_else(|| info.payload().downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "<non-string payload>".to_string());
            let _ = writeln!(
                f,
                "=== PANIC ===\nAT: {}\nPAYLOAD: {}\nBACKTRACE:\n{}\n",
                loc,
                payload,
                std::backtrace::Backtrace::force_capture()
            );
            let _ = f.flush();
        }
        default_hook(info);
    }));
}

/// Initialize tracing for unit/integration tests.
///
/// Thin wrapper around [`init`] with [`LogTarget::Test`]. Idempotent — safe
/// to call from every `#[test]` function. The returned guard is discarded
/// because tests do not have a well-defined process shutdown boundary.
#[cfg(any(test, feature = "test-support"))]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn init_for_tests() {
    let _ = init(LogTarget::Test);
}

fn make_env_filter() -> EnvFilter {
    EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"))
}

/// Resolve the log file path from environment.
///
/// - `NEOMACS_LOG_FILE=<path>` is the canonical way to set it.
/// - `NEOMACS_LOG_TO_FILE=1` is a legacy alias that maps to
///   `neomacs-{pid}.log` in the current directory.
/// - Returns `None` when neither env var is set.
fn resolve_env_log_file() -> Option<std::path::PathBuf> {
    if let Some(path) = std::env::var_os("NEOMACS_LOG_FILE") {
        let path = std::path::PathBuf::from(path);
        if path.as_os_str().is_empty() {
            return None;
        }
        return Some(path);
    }
    let legacy = std::env::var("NEOMACS_LOG_TO_FILE")
        .map(|v| v == "1")
        .unwrap_or(false);
    if legacy {
        let pid = std::process::id();
        return Some(std::path::PathBuf::from(format!("neomacs-{pid}.log")));
    }
    None
}

fn default_layer(target: LogTarget) -> Option<BoxedLayer> {
    match target {
        // stdout is a captured log by convention (build drivers, GUI test
        // harnesses, `> file`), so colorize only a real terminal.  Pipelines
        // parse these fields (`scene_commits=`, `quality_mode=`); ANSI
        // between the name and its value silently defeats every probe.
        LogTarget::Stdout => Some(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stdout)
                .with_ansi(std::io::stdout().is_terminal())
                .with_filter(make_env_filter())
                .boxed(),
        ),
        LogTarget::File => None,
        LogTarget::Test => Some(
            tracing_subscriber::fmt::layer()
                .with_test_writer()
                .with_filter(make_env_filter())
                .boxed(),
        ),
    }
}

fn open_file_layer(
    path: &std::path::Path,
) -> (
    Option<BoxedLayer>,
    Option<tracing_appender::non_blocking::WorkerGuard>,
) {
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        Ok(file) => {
            let (writer, worker_guard) = tracing_appender::non_blocking(file);
            let layer = tracing_subscriber::fmt::layer()
                .with_writer(writer)
                .with_ansi(false)
                .with_filter(make_env_filter())
                .boxed();
            (Some(layer), Some(worker_guard))
        }
        Err(e) => {
            eprintln!(
                "warning: could not open log file {}: {e}; continuing without file output",
                path.display(),
            );
            (None, None)
        }
    }
}

fn file_layer_for(
    _target: LogTarget,
) -> (
    Option<BoxedLayer>,
    Option<tracing_appender::non_blocking::WorkerGuard>,
) {
    let path = match resolve_env_log_file() {
        Some(path) => path,
        // No env override → no file layer for any target. TUI runs
        // silently by default; use NEOMACS_LOG_FILE to opt in.
        None => return (None, None),
    };
    open_file_layer(&path)
}

/// Whether a `RUST_LOG` directive string explicitly enables DEBUG/TRACE
/// logging for `neomacs::cursor_geometry` (or globally).
fn rust_log_enables_cursor_geometry(rust_log: &str) -> bool {
    rust_log.split(',').any(|raw| {
        let tok = raw.trim();
        if tok.eq_ignore_ascii_case("debug") || tok.eq_ignore_ascii_case("trace") {
            return true;
        }
        if let Some((target, level)) = tok.split_once('=') {
            target.trim() == CURSOR_GEOMETRY_TARGET
                && (level.trim().eq_ignore_ascii_case("debug")
                    || level.trim().eq_ignore_ascii_case("trace"))
        } else {
            false
        }
    })
}

fn init_inner(target: LogTarget) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    // Note: `tracing_subscriber::fmt::Subscriber::try_init` (which the
    // `.try_init()` call below ultimately runs) installs the log→tracing
    // bridge itself via `LogTracer::init`, so we do NOT call
    // `tracing_log::LogTracer::init()` ourselves. Doing so was the
    // source of a
    //   "warning: tracing subscriber init failed: attempted to set a
    //    logger after the logging system was already initialized"
    // line that fired on every neomacs / neomacs-temacs / bootstrap-neomacs
    // invocation: the manual call set a global log facade, then the
    // subscriber try_init tried to set it again and the second call
    // failed via `log::SetLoggerError`, which `tracing_subscriber`
    // surfaces as a `TryInitError::Logger`.

    let default = default_layer(target);
    let (file, file_guard) = file_layer_for(target);

    // Each layer carries its own `EnvFilter` (per-layer filter), so the
    // registry is not wrapped in a global filter. We combine the
    // (optional) default and file layers into a single `Vec<BoxedLayer>`:
    // `Vec<L>` implements `Layer<S>` when each `L: Layer<S>`, so a single
    // `.with(layers)` call stacks both atop the plain `Registry`. This
    // avoids the trait-bound explosion that occurs when chaining
    // `.with(Option<BoxedLayer>).with(Option<BoxedLayer>)`.
    let mut layers: Vec<BoxedLayer> = Vec::new();
    if let Some(layer) = default {
        layers.push(layer);
    }
    if let Some(layer) = file {
        layers.push(layer);
    }
    // Zero-cost-when-off per-layer filtered ring buffer for
    // `neomacs::cursor_geometry` events consumed verbatim by
    // `M-x neomacs-dump-display-state`. If the user launched with
    // `RUST_LOG=...,neomacs::cursor_geometry=debug`, pre-arm capture so
    // lines emitted before the dump command runs are already buffered.
    if std::env::var("RUST_LOG")
        .ok()
        .is_some_and(|val| rust_log_enables_cursor_geometry(&val))
    {
        CURSOR_GEOMETRY_CAPTURE.store(true, Ordering::SeqCst);
    }
    layers.push(
        CursorGeometryCaptureLayer
            .with_filter(CursorGeometryCaptureFilter)
            .boxed(),
    );
    // GC trace is an explicit stderr diagnostic even for silent batch/TUI
    // logging. Keep this separate from RUST_LOG and the normal writers so
    // TRACE=1 alone exposes the generation labels without changing Lisp output.
    if std::env::var("NEOVM_GC_TRACE").as_deref() == Ok("1") {
        let layer = tracing_subscriber::fmt::layer()
            .with_writer(std::io::stderr)
            .with_ansi(false)
            // Preserve the legacy message at column zero for pause parsers;
            // structured event fields follow it. This event-only layer has
            // no span prefixes, timestamps, levels or target labels.
            .without_time()
            .with_level(false)
            .with_target(false)
            .with_filter(tracing_subscriber::filter::filter_fn(|metadata| {
                metadata.is_event()
                    && metadata.target() == "neovm::gc"
                    && metadata.fields().field("kind").is_some()
                    && metadata.fields().field("cycle").is_some()
            }))
            .boxed();
        layers.push(layer);
    }
    let result = tracing_subscriber::registry().with(layers).try_init();
    if let Err(e) = result {
        eprintln!("warning: tracing subscriber init failed: {e}");
    }
    file_guard
}

#[cfg(test)]
mod cursor_geometry_capture_tests {
    use super::*;

    #[test]
    fn cursor_geometry_capture_is_off_by_default_and_toggles_live() {
        // Build an isolated subscriber mirroring default `RUST_LOG=warn` so
        // ambient developer-shell `RUST_LOG=debug` does not mask the filter.
        let subscriber = tracing_subscriber::registry()
            .with(
                tracing_subscriber::fmt::layer()
                    .with_test_writer()
                    .with_filter(EnvFilter::new("warn")),
            )
            .with(CursorGeometryCaptureLayer.with_filter(CursorGeometryCaptureFilter));

        tracing::subscriber::with_default(subscriber, || {
            set_cursor_geometry_capture(false);
            clear_cursor_geometry_lines();
            assert!(!cursor_geometry_capture_enabled());
            assert!(
                !tracing::enabled!(target: CURSOR_GEOMETRY_TARGET, tracing::Level::DEBUG),
                "cursor_geometry target must stay disabled under default warn filter when capture is off"
            );

            tracing::debug!(
                target: CURSOR_GEOMETRY_TARGET,
                "frame=1 stage=draw presentation=0 skipped=true"
            );
            assert!(
                snapshot_cursor_geometry_lines().is_empty(),
                "events emitted while off must not be buffered"
            );

            let prev = set_cursor_geometry_capture(true);
            assert!(!prev);
            assert!(cursor_geometry_capture_enabled());
            assert!(
                tracing::enabled!(target: CURSOR_GEOMETRY_TARGET, tracing::Level::DEBUG),
                "set_cursor_geometry_capture(true) must enable neomacs::cursor_geometry at DEBUG"
            );

            tracing::debug!(
                target: CURSOR_GEOMETRY_TARGET,
                "frame=1 stage=layout presentation=7 window=3 hscroll=0 vscroll=0"
            );
            tracing::debug!(
                target: CURSOR_GEOMETRY_TARGET,
                "frame=1 stage=draw presentation=7 window=3 drawn_in_row=true"
            );

            let lines = snapshot_cursor_geometry_lines();
            assert_eq!(lines.len(), 2);
            assert!(lines[0].contains("stage=layout presentation=7"));
            assert!(lines[1].contains("stage=draw presentation=7"));

            let was_enabled = set_cursor_geometry_capture(false);
            assert!(was_enabled);
            assert!(!cursor_geometry_capture_enabled());
            assert!(
                !tracing::enabled!(target: CURSOR_GEOMETRY_TARGET, tracing::Level::DEBUG),
                "set_cursor_geometry_capture(false) must disable neomacs::cursor_geometry again"
            );
        });
    }

    #[test]
    fn rust_log_parser_detects_cursor_geometry_directives() {
        assert!(!rust_log_enables_cursor_geometry("warn"));
        assert!(!rust_log_enables_cursor_geometry(
            "info,neomacs::cursor_geometry=warn"
        ));
        assert!(rust_log_enables_cursor_geometry(
            "warn,neomacs::cursor_geometry=debug"
        ));
        assert!(rust_log_enables_cursor_geometry(
            "neomacs::cursor_geometry=trace"
        ));
        assert!(rust_log_enables_cursor_geometry("debug"));
    }
}
