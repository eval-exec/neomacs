//! Native (Rust) CPU profiling via `pprof-rs`.
//!
//! Complements the Lisp CPU profiler: it samples where *native* time goes (GC,
//! layout, rendering, bytecode dispatch) — exactly the code the Lisp
//! poll-sampler cannot attribute. SIGPROF-based, but the Lisp profiler is
//! cooperative (thread-CPU-time poll), so the two never conflict. Runs entirely
//! in this crate — no Lisp thread needed, so it works even in batch/headless.
//!
//! These functions block for `secs` (real time) while the SIGPROF timer
//! samples; call them from `spawn_blocking`.

#[cfg(unix)]
use std::time::Duration;

/// Default sampling frequency (Hz). 997 is a prime near 1 kHz to avoid aliasing
/// with periodic activity.
pub const DEFAULT_FREQ_HZ: i32 = 997;

#[cfg(unix)]
fn capture_report(secs: u64, freq: i32) -> Result<pprof::Report, String> {
    let guard = pprof::ProfilerGuardBuilder::default()
        .frequency(freq)
        .blocklist(&["libc", "libgcc", "pthread", "vdso"])
        .build()
        .map_err(|e| format!("native profiler failed to start: {e}"))?;
    std::thread::sleep(Duration::from_secs(secs));
    guard
        .report()
        .build()
        .map_err(|e| format!("native profiler report failed: {e}"))
}

/// Capture `secs` of native CPU and render an SVG flamegraph.
#[cfg(unix)]
pub fn capture_native_svg(secs: u64, freq: i32) -> Result<Vec<u8>, String> {
    let report = capture_report(secs, freq)?;
    let mut svg = Vec::new();
    report
        .flamegraph(&mut svg)
        .map_err(|e| format!("native flamegraph render failed: {e}"))?;
    Ok(svg)
}

/// Capture `secs` of native CPU and return pprof protobuf bytes for
/// `go tool pprof`.
#[cfg(unix)]
pub fn capture_native_pprof(secs: u64, freq: i32) -> Result<Vec<u8>, String> {
    use pprof::protos::Message;
    let report = capture_report(secs, freq)?;
    let profile = report
        .pprof()
        .map_err(|e| format!("native pprof conversion failed: {e}"))?;
    profile
        .write_to_bytes()
        .map_err(|e| format!("native pprof encode failed: {e}"))
}

#[cfg(not(unix))]
pub fn capture_native_svg(_secs: u64, _freq: i32) -> Result<Vec<u8>, String> {
    Err("native CPU profiling is only supported on Unix".to_string())
}

#[cfg(not(unix))]
pub fn capture_native_pprof(_secs: u64, _freq: i32) -> Result<Vec<u8>, String> {
    Err("native CPU profiling is only supported on Unix".to_string())
}
