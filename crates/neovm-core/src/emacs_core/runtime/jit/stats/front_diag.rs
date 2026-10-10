//! Entry-front samples buffered until the eval thread's final report, never in a policy charge.
use super::CompileOrigin;
use crate::{emacs_core::jit::bg::JobClass, tls_scope::TlsScope};
use std::cell::{Cell, RefCell};
use std::io::{self, Write};
use std::sync::{
    Mutex, OnceLock,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant};

const CAP: usize = 65_536;
static THREADS: AtomicU64 = AtomicU64::new(1);
static FLUSH_LOCK: Mutex<()> = Mutex::new(());

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Route {
    #[default]
    Synchronous,
    WorkerPushOk,
    WorkerFallback,
    Inline,
}
impl Route {
    fn name(self) -> &'static str {
        match self {
            Self::Synchronous => "synchronous",
            Self::WorkerPushOk => "worker_push_ok",
            Self::WorkerFallback => "worker_fallback",
            Self::Inline => "inline",
        }
    }
}
#[derive(Clone, Copy, Default)]
struct Handoff {
    route: Route,
    seq: Option<u64>,
    at: Option<Instant>,
}
struct Sample {
    id: u64,
    attempt: u64,
    origin: CompileOrigin,
    route: Route,
    seq: Option<u64>,
    start: Instant,
    elapsed: Duration,
    pre_push: Option<Duration>,
    ok: bool,
}
struct Anchor {
    instant: Instant,
    before_ns: i128,
    after_ns: i128,
}
#[cfg(target_os = "linux")]
fn monotonic_ns() -> Option<i128> {
    let mut raw = std::mem::MaybeUninit::<libc::timespec>::uninit();
    // SAFETY: clock_gettime initializes the caller-owned timespec on success.
    if unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, raw.as_mut_ptr()) } != 0 {
        return None;
    }
    // SAFETY: the successful call above initialized both scalar fields.
    let raw = unsafe { raw.assume_init() };
    Some(i128::from(raw.tv_sec) * 1_000_000_000 + i128::from(raw.tv_nsec))
}
#[cfg(not(target_os = "linux"))]
fn monotonic_ns() -> Option<i128> {
    None
}
impl Anchor {
    fn capture() -> Option<Self> {
        let before_ns = monotonic_ns()?;
        let instant = Instant::now();
        let after_ns = monotonic_ns()?;
        (before_ns <= after_ns).then_some(Self {
            instant,
            before_ns,
            after_ns,
        })
    }
}
#[derive(Default)]
struct Buffer {
    anchor: Option<Anchor>,
    thread: u64,
    started: u64,
    finished: u64,
    aborted: u64,
    lost: u64,
    samples: Vec<Sample>,
}
thread_local! {
    static ACTIVE: Cell<Option<Handoff>> = const { Cell::new(None) };
    static BUFFER: RefCell<Buffer> = RefCell::new(Buffer::default());
}
fn path() -> Option<&'static std::ffi::OsStr> {
    static PATH: OnceLock<Option<std::ffi::OsString>> = OnceLock::new();
    PATH.get_or_init(|| std::env::var_os("NEOVM_JIT_FRONT_DIAGNOSTIC"))
        .as_deref()
}

pub(crate) struct Attempt {
    scope: Option<TlsScope<Option<Handoff>, Cell<Option<Handoff>>>>,
    id: u64,
    ordinal: u64,
    origin: CompileOrigin,
    entry: bool,
    done: bool,
}

static_assertions::assert_not_impl_any!(Attempt: Send, Sync);

/// Initialize optional scalar bookkeeping before the existing compile timer starts.
/// Non-entry attempts mask an enclosing marker but are not emitted.
pub(crate) fn begin(
    id: u64,
    origin: CompileOrigin,
    class: Option<JobClass>,
) -> Option<Box<Attempt>> {
    path()?;
    Some(Box::new(owned(id, origin, class == Some(JobClass::Entry))))
}
fn owned(id: u64, origin: CompileOrigin, entry: bool) -> Attempt {
    let ordinal = BUFFER.with(|buffer| {
        let mut b = buffer.borrow_mut();
        if b.thread == 0 {
            b.thread = THREADS.fetch_add(1, Ordering::Relaxed);
            b.anchor = Anchor::capture();
        }
        if entry {
            b.started += 1;
        }
        b.started
    });
    Attempt {
        scope: Some(TlsScope::new(&ACTIVE, Some(Handoff::default()))),
        id,
        ordinal,
        origin,
        entry,
        done: false,
    }
}
impl Attempt {
    /// Called only after existing policy CPU and record_compile endpoints.
    pub(crate) fn finish(mut self, start: Instant, elapsed: Duration, ok: bool) {
        let handoff = self
            .scope
            .take()
            .and_then(TlsScope::finish)
            .flatten()
            .unwrap_or_default();
        self.done = true;
        if self.entry {
            BUFFER.with(|buffer| {
                let mut b = buffer.borrow_mut();
                b.finished += 1;
                if b.samples.len() == CAP {
                    b.lost += 1;
                    return;
                }
                b.samples.push(Sample {
                    id: self.id,
                    attempt: self.ordinal,
                    origin: self.origin,
                    route: handoff.route,
                    seq: handoff.seq,
                    start,
                    elapsed,
                    pre_push: handoff.at.map(|at| at.saturating_duration_since(start)),
                    ok,
                });
            });
        }
    }
}
impl Drop for Attempt {
    fn drop(&mut self) {
        if self.entry && !self.done {
            let _ = BUFFER.try_with(|b| {
                if let Ok(mut b) = b.try_borrow_mut() {
                    b.aborted += 1;
                }
            });
        }
    }
}
pub(crate) fn handoff(route: Route, seq: u64, at: Instant) {
    // No environment/config check or worker logging on this path.
    let _ = ACTIVE.try_with(|slot| {
        if slot.get().is_some() {
            slot.set(Some(Handoff {
                route,
                seq: Some(seq),
                at: Some(at),
            }));
        }
    });
}
fn offset(at: Instant, epoch: Instant) -> i128 {
    at.checked_duration_since(epoch).map_or_else(
        || -(epoch.duration_since(at).as_nanos() as i128),
        |d| d.as_nanos() as i128,
    )
}
fn render(b: &Buffer, out: &mut impl Write) -> io::Result<()> {
    let epoch = b
        .anchor
        .as_ref()
        .map(|a| a.instant)
        .or_else(|| b.samples.first().map(|s| s.start));
    for s in &b.samples {
        let origin: &'static str = s.origin.into();
        let start = offset(s.start, epoch.expect("sample epoch"));
        writeln!(
            out,
            "front_v2 pid={} thread={} attempt={} id={} origin={} class=entry route={} seq={} ok={} start_ns={} return_ns={} full_return_ns={} pre_push_ns={}",
            std::process::id(),
            b.thread,
            s.attempt,
            s.id,
            origin,
            s.route.name(),
            s.seq.map_or_else(|| "-".into(), |n| n.to_string()),
            u8::from(s.ok),
            start,
            start + s.elapsed.as_nanos() as i128,
            s.elapsed.as_nanos(),
            s.pre_push
                .map_or_else(|| "-".into(), |d| d.as_nanos().to_string())
        )?;
    }
    writeln!(
        out,
        "front_end_v2 pid={} thread={} started={} finished={} aborted={} lost={} records={} anchor_before_ns={} anchor_after_ns={}",
        std::process::id(),
        b.thread,
        b.started,
        b.finished,
        b.aborted,
        b.lost,
        b.samples.len(),
        b.anchor
            .as_ref()
            .map_or_else(|| "-".into(), |a| a.before_ns.to_string()),
        b.anchor
            .as_ref()
            .map_or_else(|| "-".into(), |a| a.after_ns.to_string())
    )
}

/// Explicit final-report flush; caller must be outside the timed workload/active compile.
/// Each independent mutator flushes its own buffer. No reporting knobs are enabled.
pub(crate) fn flush_at_exit() {
    let Some(path) = path() else {
        return;
    };
    let _writer = FLUSH_LOCK.lock().unwrap_or_else(|p| p.into_inner());
    BUFFER.with(|buffer| {
        let b = buffer.borrow();
        let result = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .and_then(|mut file| render(&b, &mut file));
        if let Err(error) = result {
            tracing::warn!(target: "neovm_jit", %error, "NEOVM_JIT_FRONT_DIAGNOSTIC incomplete");
        }
    });
}
#[cfg(test)]
#[path = "front_diag/tests/front_diag_test.rs"]
mod tests;
