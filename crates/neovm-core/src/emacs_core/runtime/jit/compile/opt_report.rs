//! Gate-local opt construction evidence without runtime observation.
//!
//! Threading: a process mutex owns scalar counts and a fixed-size row array;
//! compiler guards contain only route/row indices. No Lisp values, per-mutator
//! caches, native fields or generated counters are added. The exit writer uses
//! the same mutex. A compilation after sealing appends an invalidation record.

use std::io::Write;
use std::sync::{Mutex, MutexGuard};

use super::CompileRequest;
use crate::emacs_core::jit::{bg, stats::CompileOrigin};

const ROW_CAP: usize = 128;

/// Compiler route, copied into report-owned counts. Threading: scalar only.
#[derive(Clone, Copy)]
enum Route {
    Normal,
    Osr,
}

impl Route {
    #[inline]
    fn name(self) -> &'static str {
        match self {
            Self::Normal => "normal",
            Self::Osr => "osr",
        }
    }
}

/// Construction outcome; Ready does not promise later cache retention.
/// Threading: diagnostic scalar only, never stored in a compiled leaf.
#[derive(Clone, Copy)]
enum Outcome {
    Active,
    Refused,
    Ready,
    Deferred,
}

impl Outcome {
    #[inline]
    fn name(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Refused => "refused",
            Self::Ready => "ready",
            Self::Deferred => "deferred",
        }
    }
}

/// Process-owned compilation totals. Threading: accessed under LEDGER only.
#[derive(Clone, Copy)]
struct Totals {
    attempts: u64,
    refused: u64,
    constructed: u64,
    ready: u64,
    deferred: u64,
    active: u64,
}

impl Totals {
    const ZERO: Self = Self {
        attempts: 0,
        refused: 0,
        constructed: 0,
        ready: 0,
        deferred: 0,
        active: 0,
    };
}

/// First ROW_CAP attempts, including refused/transient construction.
/// Threading: owned scalar metadata; origin is absent for old unrequested
/// entry routes. No source names or source identity assignments are performed.
#[derive(Clone, Copy)]
struct Row {
    seq: u64,
    route: Route,
    origin: Option<CompileOrigin>,
    ops: usize,
    osr_pc: Option<usize>,
    outcome: Outcome,
}

/// Process report state. Threading: one mutex serializes every compiler and
/// all output; the fixed row budget cannot truncate aggregate totals. Seal
/// completeness assumes all mutators have stopped compiling. Late events
/// invalidate the certificate; threaded backends are conservatively incomplete.
#[derive(Clone)]
struct Ledger {
    totals: [Totals; 2],
    rows: [Option<Row>; ROW_CAP],
    row_count: usize,
    sealed: bool,
    late: u64,
    poisoned: bool,
}

impl Ledger {
    const fn new() -> Self {
        Self {
            totals: [Totals::ZERO; 2],
            rows: [None; ROW_CAP],
            row_count: 0,
            sealed: false,
            late: 0,
            poisoned: false,
        }
    }

    #[cold]
    #[inline(never)]
    fn late_event(&mut self, kind: &'static str, path: &std::path::Path) {
        if !self.sealed {
            return;
        }
        self.late += 1;
        // The owned diagnostic capture must fail if invalidation cannot be
        // persisted: otherwise its old complete seal would remain believable.
        // Only an explicitly enabled report path plus post-seal compilation
        // reaches this; default/ordinary stats behavior remains unchanged.
        let invalidated = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .and_then(|mut file| {
                writeln!(
                    file,
                    "opt-report-late\tv=1\tpid={}\tkind={kind}",
                    std::process::id()
                )
            });
        if invalidated.is_err() {
            std::process::abort();
        }
    }
}

static LEDGER: Mutex<Ledger> = Mutex::new(Ledger::new());

#[cold]
#[inline(never)]
fn lock(ledger: &Mutex<Ledger>) -> MutexGuard<'_, Ledger> {
    match ledger.lock() {
        Ok(guard) => guard,
        Err(error) => {
            let mut guard = error.into_inner();
            guard.poisoned = true;
            guard
        }
    }
}

/// One actual opt construction attempt at the shared normal/OSR lowering
/// seam. Threading: invocation-owned scalar indices; completion publishes
/// counters under the process mutex, without observing Lisp or native entry.
#[must_use = "dropping an unfinished attempt records refusal"]
pub(super) struct Attempt {
    route: Route,
    slot: Option<usize>,
    active: bool,
    #[cfg(test)]
    local: Option<std::sync::Arc<test_support::Reporter>>,
}

impl Attempt {
    #[cold]
    #[inline(never)]
    pub(super) fn begin(
        opt_selected: bool,
        osr_pc: Option<usize>,
        request: Option<CompileRequest>,
        ops: usize,
    ) -> Option<Self> {
        if !opt_selected {
            return None;
        }
        #[cfg(test)]
        let local = test_support::current();
        #[cfg(test)]
        let path = match local.as_ref() {
            Some(reporter) => reporter.path.as_deref(),
            None => super::jit_opt_report_path(),
        };
        #[cfg(not(test))]
        let path = super::jit_opt_report_path();
        let path = path?;
        #[cfg(test)]
        let ledger = match local.as_ref() {
            Some(reporter) => &reporter.ledger,
            None => &LEDGER,
        };
        #[cfg(not(test))]
        let ledger = &LEDGER;
        let route = if osr_pc.is_some() {
            Route::Osr
        } else {
            Route::Normal
        };
        let mut state = lock(ledger);
        state.late_event("begin", path);
        let totals = &mut state.totals[route as usize];
        totals.attempts += 1;
        totals.active += 1;
        let seq = state.totals.iter().map(|totals| totals.attempts).sum();
        let slot = (state.row_count < ROW_CAP).then_some(state.row_count);
        if let Some(slot) = slot {
            state.rows[slot] = Some(Row {
                seq,
                route,
                ops,
                osr_pc,
                origin: request
                    .map(|request| request.origin)
                    .or(osr_pc.map(|_| CompileOrigin::Osr)),
                outcome: Outcome::Active,
            });
            state.row_count += 1;
        }
        drop(state);
        Some(Self {
            route,
            slot,
            active: true,
            #[cfg(test)]
            local,
        })
    }

    /// Called after code construction, before cache retention/installation.
    /// A null entry is a successful deferred front, counted conservatively even
    /// if a later worker refuses or a stale install cancels the leaf.
    #[cold]
    #[inline(never)]
    pub(super) fn constructed(mut self, deferred: bool) {
        self.finish(if deferred {
            Outcome::Deferred
        } else {
            Outcome::Ready
        });
    }

    #[cold]
    #[inline(never)]
    fn finish(&mut self, outcome: Outcome) {
        #[cfg(test)]
        let ledger = match self.local.as_ref() {
            Some(reporter) => &reporter.ledger,
            None => &LEDGER,
        };
        #[cfg(not(test))]
        let ledger = &LEDGER;
        #[cfg(test)]
        let path = match self.local.as_ref() {
            Some(reporter) => reporter.path.as_deref(),
            None => super::jit_opt_report_path(),
        };
        #[cfg(not(test))]
        let path = super::jit_opt_report_path();
        let mut state = lock(ledger);
        state.late_event(
            outcome.name(),
            path.expect("active attempt has a configured path"),
        );
        let totals = &mut state.totals[self.route as usize];
        totals.active -= 1;
        match outcome {
            Outcome::Refused => totals.refused += 1,
            Outcome::Ready => {
                totals.constructed += 1;
                totals.ready += 1;
            }
            Outcome::Deferred => {
                totals.constructed += 1;
                totals.deferred += 1;
            }
            Outcome::Active => unreachable!("only completed attempts reach finish"),
        }
        if let Some(slot) = self.slot {
            state.rows[slot]
                .as_mut()
                .expect("attempt owns its row")
                .outcome = outcome;
        }
        drop(state);
        self.active = false;
    }
}

impl Drop for Attempt {
    #[cold]
    #[inline(never)]
    fn drop(&mut self) {
        if self.active {
            self.finish(Outcome::Refused);
        }
    }
}

#[cold]
#[inline(never)]
fn write_final(
    output: &mut impl Write,
    state: &Ledger,
    bg_mode: bg::BgMode,
    pending_local: usize,
) -> std::io::Result<()> {
    let pid = std::process::id();
    for row in state.rows[..state.row_count].iter().flatten() {
        let origin: &'static str = row.origin.map(Into::into).unwrap_or("unknown");
        let pc = row.osr_pc.map_or_else(|| "-".into(), |pc| pc.to_string());
        writeln!(
            output,
            "opt-report-row\tv=1\tpid={pid}\tseq={}\troute={}\torigin={origin}\tops={}\tosr_pc={pc}\toutcome={}",
            row.seq,
            row.route.name(),
            row.ops,
            row.outcome.name()
        )?;
    }
    let mode: &'static str = bg_mode.into();
    let total_attempts: u64 = state.totals.iter().map(|totals| totals.attempts).sum();
    let active: u64 = state.totals.iter().map(|totals| totals.active).sum();
    let complete = active == 0
        && !state.poisoned
        && state.late == 0
        && bg_mode != bg::BgMode::Threaded
        && pending_local == 0;
    write!(
        output,
        "opt-report-final\tv=1\tpid={pid}\tsealed=1\tcomplete={}\trows={}\trow_drops={}\tlate={}\tpoisoned={}\tbg={mode}\tbg_pending_local={pending_local}",
        u8::from(complete),
        state.row_count,
        total_attempts - state.row_count as u64,
        state.late,
        u8::from(state.poisoned)
    )?;
    for route in [Route::Normal, Route::Osr] {
        let totals = state.totals[route as usize];
        let name = route.name();
        write!(
            output,
            "\t{name}_attempts={}\t{name}_refused={}\t{name}_constructed={}\t{name}_ready={}\t{name}_deferred={}\t{name}_active={}",
            totals.attempts,
            totals.refused,
            totals.constructed,
            totals.ready,
            totals.deferred,
            totals.active
        )?;
    }
    writeln!(output)
}

/// The always-called exit seam invokes this before the ordinary report knob
/// early return. Threading: seals a process ledger under its mutex; no Context
/// or Lisp cache access. A complete certificate requires stopped mutators and
/// synchronous backends; any later compile invalidates it with a file record.
#[cold]
#[inline(never)]
pub(crate) fn report_at_exit() {
    #[cfg(test)]
    if let Some(reporter) = test_support::current() {
        if let Some(path) = reporter.path.as_deref() {
            seal(&reporter.ledger, path);
        }
        return;
    }
    let Some(path) = super::jit_opt_report_path() else {
        return;
    };
    seal(&LEDGER, path);
}

#[cold]
#[inline(never)]
fn seal(ledger: &Mutex<Ledger>, path: &std::path::Path) {
    let mut state = lock(ledger);
    if state.sealed {
        return;
    }
    state.sealed = true;
    if let Ok(file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let mut output = std::io::BufWriter::with_capacity(4096, file);
        let _ = write_final(&mut output, &state, bg::mode(), bg::pending_count())
            .and_then(|()| output.flush());
    }
}

#[cfg(test)]
#[path = "opt_report/tests/support.rs"]
mod test_support;

#[cfg(test)]
#[path = "opt_report/tests/report_test.rs"]
mod tests;
