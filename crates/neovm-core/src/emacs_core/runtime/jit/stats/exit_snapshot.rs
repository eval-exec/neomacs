//! Compile-only exit evidence, independent of ordinary JIT observation knobs.
//!
//! Threading: process-global scalar atomics cover all opt constructors; guards
//! own no Lisp state, cache, native counter or per-mutator storage. Timing is the
//! reporting mutator's existing P0.1 aggregate, not a process-wide mutator sum.
//! A complete seal assumes stopped mutators and a synchronous backend. Any
//! constructor that overlaps or follows sealing invalidates the file. Nothing
//! is recorded or written without the immutable `NEOVM_JIT_EXIT_FILE` path.

use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use super::CompileStats;
use crate::emacs_core::jit::bg;
use crate::emacs_core::jit::compile::opt_report::{ConstructedState, ConstructionSite};

/// Threading: immutable compiler-route tag; no Lisp or mutator identity.
#[derive(Clone, Copy, Debug)]
enum Route {
    Normal,
    Osr,
}

impl Route {
    fn index(self) -> usize {
        match self {
            Self::Normal => 0,
            Self::Osr => 1,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Osr => "osr",
        }
    }
}

/// Threading: immutable scalar outcome, published through process atomics.
#[derive(Clone, Copy, Debug)]
enum Outcome {
    Refused,
    Ready,
    Deferred,
}

/// Per-route process counters. Threading: concurrent compilers update only
/// SeqCst atomics; outcomes publish before the active reservation is released.
/// Component loads are checked for conservation when sealing.
#[derive(Debug)]
struct RouteCounters {
    attempts: AtomicU64,
    refused: AtomicU64,
    ready: AtomicU64,
    deferred: AtomicU64,
    active: AtomicU64,
}

impl RouteCounters {
    const fn new() -> Self {
        Self {
            attempts: AtomicU64::new(0),
            refused: AtomicU64::new(0),
            ready: AtomicU64::new(0),
            deferred: AtomicU64::new(0),
            active: AtomicU64::new(0),
        }
    }

    fn snapshot(&self) -> RouteSnapshot {
        RouteSnapshot {
            attempts: self.attempts.load(Ordering::SeqCst),
            refused: self.refused.load(Ordering::SeqCst),
            ready: self.ready.load(Ordering::SeqCst),
            deferred: self.deferred.load(Ordering::SeqCst),
            active: self.active.load(Ordering::SeqCst),
        }
    }
}

/// Scalar process evidence. SeqCst reservation/seal order ensures that a begin
/// cannot miss both the exit snapshot and the post-seal invalidation check.
/// Outcome publication precedes releasing the active reservation. Concurrent
/// snapshots are conservative: arithmetic disagreement prevents completeness.
#[derive(Debug)]
struct Counters {
    routes: [RouteCounters; 2],
    sealed: AtomicBool,
    late: AtomicU64,
}

impl Counters {
    const fn new() -> Self {
        Self {
            routes: [RouteCounters::new(), RouteCounters::new()],
            sealed: AtomicBool::new(false),
            late: AtomicU64::new(0),
        }
    }

    fn reserve(&self, route: Route) {
        let totals = &self.routes[route.index()];
        // Reserve first, then test the seal in Attempt::begin_with. If sealing
        // wins before reservation, that later seal load sees it; if reservation
        // wins, the snapshot sees active work or its published completion.
        totals.active.fetch_add(1, Ordering::SeqCst);
        totals.attempts.fetch_add(1, Ordering::SeqCst);
    }

    fn finish(&self, route: Route, outcome: Outcome) {
        let totals = &self.routes[route.index()];
        match outcome {
            Outcome::Refused => &totals.refused,
            Outcome::Ready => &totals.ready,
            Outcome::Deferred => &totals.deferred,
        }
        .fetch_add(1, Ordering::SeqCst);
        totals.active.fetch_sub(1, Ordering::SeqCst);
    }

    fn late_record(&self, pid: u32, kind: &'static str) -> Option<String> {
        if !self.sealed.load(Ordering::SeqCst) {
            return None;
        }
        let late = self.late.fetch_add(1, Ordering::SeqCst) + 1;
        Some(format!(
            "[neovm-jit-exit-invalidated] v=1 pid={pid} sealed=1 late={late} kind={kind}\n"
        ))
    }

    fn seal(&self) -> Option<Snapshot> {
        if self.sealed.swap(true, Ordering::SeqCst) {
            return None;
        }
        Some(Snapshot {
            routes: [self.routes[0].snapshot(), self.routes[1].snapshot()],
            late: self.late.load(Ordering::SeqCst),
        })
    }
}

/// Owned scalar counter sample. Threading: component loads need not form an
/// atomic snapshot; active/conservation checks and seal invalidation govern
/// completeness. It retains no Lisp or mutator state.
#[derive(Clone, Copy, Debug, Default)]
struct RouteSnapshot {
    attempts: u64,
    refused: u64,
    ready: u64,
    deferred: u64,
    active: u64,
}

impl RouteSnapshot {
    fn constructed(self) -> Option<u64> {
        self.ready.checked_add(self.deferred)
    }

    fn conserved(self) -> bool {
        self.constructed()
            .and_then(|n| n.checked_add(self.refused))
            .and_then(|n| n.checked_add(self.active))
            == Some(self.attempts)
    }
}

/// Owned exit sample. Threading: completeness assumes stopped mutators and a
/// synchronous backend; overlapping or later construction invalidates evidence.
struct Snapshot {
    routes: [RouteSnapshot; 2],
    late: u64,
}

impl Snapshot {
    fn complete(&self, mode: bg::BgMode, pending_local: usize) -> bool {
        self.late == 0
            && mode != bg::BgMode::Threaded
            && pending_local == 0
            && self.routes.iter().all(|r| r.active == 0 && r.conserved())
    }
}

static COUNTERS: Counters = Counters::new();

fn path() -> Option<&'static Path> {
    super::super::compile::jit_exit_path()
}

fn append(path: &Path, record: &str) -> io::Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    file.write_all(record.as_bytes())
}

#[cold]
#[inline(never)]
fn append_or_abort(path: &Path, record: &str) {
    if let Err(error) = append(path, record) {
        tracing::error!(target: "neovm_jit", path = %path.display(), %error,
                        "NEOVM_JIT_EXIT_FILE evidence could not be written");
        // Explicit diagnostic mode must never exit cleanly leaving an old
        // complete certificate after a failed invalidation write.
        std::process::abort();
    }
}

/// Invocation-owned compiler reservation. Threading: references only scalar
/// process evidence and an immutable diagnostic path; no Lisp/native state.
#[must_use = "dropping an unfinished opt attempt records refusal"]
#[derive(Debug)]
pub(crate) struct Attempt<'a> {
    counters: &'a Counters,
    route: Route,
    path: &'a Path,
    active: bool,
}

static_assertions::assert_impl_all!(Attempt<'static>: Send, Sync);

impl Attempt<'static> {
    #[cold]
    #[inline(never)]
    pub(crate) fn begin(site: ConstructionSite) -> Option<Self> {
        let route = match site {
            ConstructionSite::Unselected => return None,
            ConstructionSite::Normal => Route::Normal,
            ConstructionSite::Osr { .. } => Route::Osr,
        };
        let path = path()?;
        Some(Self::begin_with(&COUNTERS, route, path))
    }
}

impl<'a> Attempt<'a> {
    fn begin_with(counters: &'a Counters, route: Route, path: &'a Path) -> Self {
        counters.reserve(route);
        if let Some(record) = counters.late_record(std::process::id(), "begin") {
            append_or_abort(path, &record);
        }
        Self {
            counters,
            route,
            path,
            active: true,
        }
    }

    #[cold]
    #[inline(never)]
    pub(crate) fn constructed(mut self, state: ConstructedState) {
        self.finish(match state {
            ConstructedState::Ready => Outcome::Ready,
            ConstructedState::Deferred => Outcome::Deferred,
        });
    }

    fn finish(&mut self, outcome: Outcome) {
        self.counters.finish(self.route, outcome);
        if let Some(record) = self.counters.late_record(std::process::id(), "finish") {
            append_or_abort(self.path, &record);
        }
        self.active = false;
    }
}

impl Drop for Attempt<'_> {
    fn drop(&mut self) {
        if self.active {
            self.finish(Outcome::Refused);
        }
    }
}

fn write_final(
    output: &mut impl Write,
    pid: u32,
    snapshot: &Snapshot,
    current: &CompileStats,
    mode: bg::BgMode,
    pending_local: usize,
) -> io::Result<()> {
    let compile_us = current
        .origins
        .iter()
        .try_fold(0u64, |sum, row| sum.checked_add(row.us));
    let complete = snapshot.complete(mode, pending_local) && compile_us.is_some();
    let mode = match mode {
        bg::BgMode::Legacy => "legacy",
        bg::BgMode::Sync => "sync",
        bg::BgMode::Threaded => "threaded",
    };
    write!(
        output,
        "[neovm-jit-exit] v=1 pid={pid} sealed=1 complete={} bg={mode} bg_pending_local={pending_local} late={} compile_time_scope=reporting_mutator",
        u8::from(complete),
        snapshot.late
    )?;
    for route in [Route::Normal, Route::Osr] {
        let row = snapshot.routes[route.index()];
        let name = route.name();
        write!(
            output,
            " {name}_attempts={} {name}_refused={} {name}_constructed={} {name}_ready={} {name}_deferred={} {name}_active={}",
            row.attempts,
            row.refused,
            row.constructed().unwrap_or(u64::MAX),
            row.ready,
            row.deferred,
            row.active
        )?;
    }
    writeln!(
        output,
        " origin[{}] compile_total_us={}",
        super::phases::render_origins(&current.origins),
        compile_us.unwrap_or(u64::MAX)
    )
}

/// Called before the ordinary exit-report gate. No summary, phase clock,
/// naming, native-entry counter, cache walk or feedback observation is enabled.
#[cold]
#[inline(never)]
pub(crate) fn report_at_exit(current: &CompileStats) {
    let Some(path) = path() else { return };
    let Some(snapshot) = COUNTERS.seal() else {
        return;
    };
    let mut record = Vec::with_capacity(1024);
    if write_final(
        &mut record,
        std::process::id(),
        &snapshot,
        current,
        bg::mode(),
        bg::pending_count(),
    )
    .is_err()
    {
        std::process::abort();
    }
    let record = String::from_utf8(record).expect("exit record contains only ASCII fields");
    append_or_abort(path, &record);
}

#[cfg(test)]
#[path = "tests/exit_snapshot_test.rs"]
mod tests;
