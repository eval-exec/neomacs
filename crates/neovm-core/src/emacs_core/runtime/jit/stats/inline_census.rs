//! P2.3 commit 3's measurement-only inline census.
//!
//! `NEOVM_JIT_INLINE_CENSUS=1` records the latest static verdict for each
//! source's call sites and counts bytecode callbacks entering
//! `Context::apply1_bytecode`. It changes neither admission nor generated
//! code. Run it separately from performance gates: counting a callback
//! deliberately pays a mutex acquisition.
//!
//! Threading: the process-wide mutex serializes Rust diagnostic data from
//! any number of mutators. Keys are immutable, process-unique bytecode
//! `source_id`s (shared by closure instances); no Lisp `Value`, heap pointer,
//! or runtime handle is retained. The relaxed knob word publishes only a
//! scalar flag, never accompanying state. Concurrent first reads compute
//! the same process environment setting. The report is one locked snapshot.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Mutex, OnceLock};

use crate::emacs_core::bytecode::ByteCodeFunction;
use crate::emacs_core::eval::Context;
use crate::emacs_core::jit::compile::{CompileError, CompiledLeaf, LeafTier};
use crate::emacs_core::jit::inline::{CensusSite, census_callee_verdict, census_sites};
use crate::emacs_core::symbol::Obarray;
use crate::emacs_core::value::Value;

use super::ReportTag;

/// Scalar process configuration. Relaxed loads/stores publish no payload,
/// so independent compiler/mutator readers need no acquire/release ordering.
#[repr(u8)]
enum CensusMode {
    Unread,
    Off,
    On,
}

static ENABLED: AtomicU8 = AtomicU8::new(CensusMode::Unread as u8);

/// Process configuration used when deriving each mutator's attention word.
/// Callback entry shares the existing quit guard; it reads this flag again
/// only on the cold path. The environment lookup stays out of line.
#[inline(always)]
pub(crate) fn enabled() -> bool {
    #[cfg(test)]
    if let Some(on) = FORCE_ENABLED.with(std::cell::Cell::get) {
        return on;
    }
    match ENABLED.load(Ordering::Relaxed) {
        value if value == CensusMode::Off as u8 => false,
        value if value == CensusMode::On as u8 => true,
        _ => read_enabled(),
    }
}

#[cold]
#[inline(never)]
fn read_enabled() -> bool {
    let on = std::env::var("NEOVM_JIT_INLINE_CENSUS").as_deref() == Ok("1");
    let mode = if on { CensusMode::On } else { CensusMode::Off };
    ENABLED.store(mode as u8, Ordering::Relaxed);
    on
}

/// Diagnostic metadata copied from a source. There are no Lisp roots or
/// shared runtime references; this data remains valid after source GC.
#[derive(Clone, Debug)]
struct Source {
    name: String,
    compiled_id: Option<u64>,
    sites: Vec<CensusSite>,
    compiles: u64,
    outcome: CompileOutcome,
}

/// Latest result of a source's compile attempt. All fields are copied
/// diagnostic data, and the containing census mutex owns every update.
#[derive(Clone, Copy, Debug)]
enum CompileOutcome {
    Pending,
    Compiled(LeafTier),
    NotProfitable,
    NotCompilable,
}

impl CompileOutcome {
    fn name(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Compiled(tier) => tier.name(),
            Self::NotProfitable => "not_profitable",
            Self::NotCompilable => "not_compilable",
        }
    }

    fn compiled(self) -> bool {
        matches!(self, Self::Compiled(_))
    }
}

/// Callback counts for one source, independent of individual closure
/// instances. Mutators update these only while holding the census mutex.
#[derive(Clone, Debug)]
struct Callback {
    name: String,
    count: u64,
    v2: Result<(), String>,
}

/// Process-wide diagnostic counters; all mutation and snapshots require
/// the containing mutex. No field contains Lisp state.
#[derive(Default)]
struct Census {
    sources: BTreeMap<u64, Source>,
    callbacks: BTreeMap<u64, Callback>,
    loop_base: Option<BTreeMap<u64, u64>>,
}

fn census() -> &'static Mutex<Census> {
    static CENSUS: OnceLock<Mutex<Census>> = OnceLock::new();
    CENSUS.get_or_init(|| Mutex::new(Census::default()))
}

/// Inspect a compile's original body, including bodies legacy MIR will
/// accept before the old fuser runs. Repeated compiles replace site
/// verdicts, so warming feedback does not leave the first cold verdict.
pub(crate) fn note_compile(f: &ByteCodeFunction, obarray: Option<&Obarray>) {
    if !enabled() {
        return;
    }
    let sites = census_sites(f, obarray);
    let name = super::perf_map::active_label_name()
        .unwrap_or_else(|| super::perf_map::anon_name(f).into());
    let mut census = census().lock().unwrap_or_else(|p| p.into_inner());
    let compiles = census
        .sources
        .get(&f.source_id)
        .map_or(1, |s| s.compiles + 1);
    census.sources.insert(
        f.source_id,
        Source {
            name,
            compiled_id: f.jit_runtime().compiled_id(),
            sites,
            compiles,
            outcome: CompileOutcome::Pending,
        },
    );
}

/// Complete a compile probe with its tier or refusal. This distinguishes
/// candidates in compiled leaves from candidates in bodies that never
/// passed the profitability or capability gates.
pub(crate) fn note_compile_outcome(
    f: &ByteCodeFunction,
    result: &Result<CompiledLeaf, CompileError>,
) {
    if !enabled() {
        return;
    }
    let outcome = match result {
        Ok(leaf) => CompileOutcome::Compiled(leaf.tier()),
        Err(CompileError::NotProfitable) => CompileOutcome::NotProfitable,
        Err(_) => CompileOutcome::NotCompilable,
    };
    let mut census = census().lock().unwrap_or_else(|p| p.into_inner());
    if let Some(source) = census.sources.get_mut(&f.source_id) {
        source.outcome = outcome;
    }
}

/// Count every entry to the bytecode callback protocol, before quit,
/// depth, GC and debugger checks. This is an entry census, including
/// callbacks those checks prevent from executing. Callback entry reaches
/// this wrapper only through its cold attention path; all counting stays
/// out of line.
#[inline(always)]
pub(crate) fn note_callback(function: Value) {
    if enabled() {
        record_callback(function);
    }
}

#[cold]
#[inline(never)]
fn record_callback(function: Value) {
    // A census must not materialize a lazy pdump stub or assign a compiled
    // id. apply1's bytecode classification has normally materialized it
    // already; a remaining stub is outside this source-level census.
    let Some(f) = function.bytecode_data_if_materialized() else {
        return;
    };
    let mut census = census().lock().unwrap_or_else(|p| p.into_inner());
    let row = census
        .callbacks
        .entry(f.source_id)
        .or_insert_with(|| Callback {
            name: super::perf_map::anon_name(f).into(),
            count: 0,
            // The entry itself proves this source is executed; callback
            // eligibility is structural rather than a sample of pre-call heat.
            v2: census_callee_verdict(f, 1),
        });
    row.count = row.count.saturating_add(1);
}

/// Mark startup's callback counts so an editor row can report callbacks
/// per operation from its command-loop interval. The snapshot is process
/// wide, including other mutators' calls in that interval.
pub(crate) fn mark_command_loop_entry() {
    if !enabled() {
        return;
    }
    let mut census = census().lock().unwrap_or_else(|p| p.into_inner());
    census.loop_base = Some(
        census
            .callbacks
            .iter()
            .map(|(&id, r)| (id, r.count))
            .collect(),
    );
}

#[derive(Clone, Debug, Default)]
struct Snapshot {
    sources: BTreeMap<u64, Source>,
    callbacks: BTreeMap<u64, Callback>,
    loop_base: Option<BTreeMap<u64, u64>>,
}

fn snapshot() -> Snapshot {
    let census = census().lock().unwrap_or_else(|p| p.into_inner());
    Snapshot {
        sources: census.sources.clone(),
        callbacks: census.callbacks.clone(),
        loop_base: census.loop_base.clone(),
    }
}

fn verdict(result: &Result<(), String>) -> &str {
    match result {
        Ok(()) => "eligible",
        Err(reason) => reason,
    }
}

impl Snapshot {
    fn render(&self, names: &BTreeMap<u64, String>) -> Vec<(ReportTag, String)> {
        let mut reasons = BTreeMap::<String, u64>::new();
        let mut shapes = BTreeMap::<&str, u64>::new();
        let (mut sites, mut replay, mut v2, mut compiles) = (0, 0, 0, 0);
        let (mut compiled_sources, mut compiled_v2) = (0, 0);
        for source in self.sources.values() {
            compiles += source.compiles;
            compiled_sources += u64::from(source.outcome.compiled());
            for site in &source.sites {
                sites += 1;
                replay += u64::from(site.replay.is_ok());
                v2 += u64::from(site.v2.is_ok());
                compiled_v2 += u64::from(source.outcome.compiled() && site.v2.is_ok());
                *shapes.entry(site.shape.name()).or_default() += 1;
                *reasons.entry(verdict(&site.v2).to_string()).or_default() += 1;
            }
        }
        let callbacks: u64 = self.callbacks.values().map(|r| r.count).sum();
        let callback_v2: u64 = self
            .callbacks
            .values()
            .filter(|r| r.v2.is_ok())
            .map(|r| r.count)
            .sum();
        let since_loop: Option<u64> = self.loop_base.as_ref().map(|base| {
            self.callbacks
                .iter()
                .map(|(id, r)| r.count.saturating_sub(*base.get(id).unwrap_or(&0)))
                .sum()
        });
        let fields = |rows: &BTreeMap<String, u64>| {
            rows.iter()
                .map(|(k, n)| format!("{k}={n}"))
                .collect::<Vec<_>>()
                .join(",")
        };
        let shape_fields = shapes
            .iter()
            .map(|(k, n)| format!("{k}={n}"))
            .collect::<Vec<_>>()
            .join(",");
        let mut lines = vec![(
            ReportTag::FinalInlineCensus,
            format!(
                "sources={} compiles={compiles} sites={sites} replay_eligible={replay} v2_eligible={v2} compiled_sources={compiled_sources} compiled_v2_eligible={compiled_v2} shape[{shape_fields}] v2[{}] callbacks={callbacks} callback_targets={} callback_v2={callback_v2} callbacks_since_command_loop={}",
                self.sources.len(),
                fields(&reasons),
                self.callbacks.len(),
                since_loop.map_or_else(|| "-".to_string(), |n| n.to_string())
            ),
        )];
        // All sources/sites are printed, not a top-N sample: the census
        // must answer how many candidates compiled org leaves contain.
        for (&id, source) in &self.sources {
            let name = names.get(&id).unwrap_or(&source.name);
            for site in &source.sites {
                let target = site
                    .target_source
                    .map_or_else(|| "-".to_string(), |n| n.to_string());
                lines.push((ReportTag::FinalInlineSource, format!(
                    "source={id} id={} fn={name} compiles={} outcome={} pc={} shape={} target_source={target} replay={} v2={}",
                    source.compiled_id.map_or_else(|| "-".to_string(), |n| n.to_string()),
                    source.compiles, source.outcome.name(), site.pc, site.shape.name(), verdict(&site.replay), verdict(&site.v2)
                )));
            }
        }
        for (&id, callback) in &self.callbacks {
            let name = names.get(&id).unwrap_or(&callback.name);
            let delta = self.loop_base.as_ref().map_or_else(
                || "-".to_string(),
                |base| {
                    callback
                        .count
                        .saturating_sub(*base.get(&id).unwrap_or(&0))
                        .to_string()
                },
            );
            lines.push((
                ReportTag::FinalInlineCallback,
                format!(
                    "source={id} fn={name} callbacks={} since_command_loop={delta} v2={}",
                    callback.count,
                    verdict(&callback.v2)
                ),
            ));
        }
        lines
    }
}

/// Emit the census independently of other report knobs. Names are looked
/// up read-only in the reporting mutator's obarray; anonymous/dead sources
/// retain their copied fallback name. No Lisp allocation or safe point.
pub(crate) fn report_at_exit(ctx: &Context) {
    if !enabled() {
        return;
    }
    let snapshot = snapshot();
    let mut names = BTreeMap::new();
    for (name, function) in ctx.obarray.interned_function_cells_with_names() {
        if let Some(f) = function.bytecode_data_if_materialized() {
            names.entry(f.source_id).or_insert_with(|| {
                super::epoch::report_token(
                    crate::emacs_core::intern::resolve_name_lisp_string(name)
                        .as_utf8_str()
                        .unwrap_or("<non-utf8>"),
                )
            });
        }
    }
    for (tag, line) in snapshot.render(&names) {
        super::report_line(tag, &line);
    }
}

#[cfg(test)]
thread_local! {
    /// Scalar test configuration only; never Lisp or mutator runtime state.
    static FORCE_ENABLED: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
#[path = "tests/inline_census_test.rs"]
mod tests;
