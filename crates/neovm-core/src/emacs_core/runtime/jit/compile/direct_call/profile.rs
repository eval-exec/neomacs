//! Per-emitted-site diagnostics for `NEOVM_JIT_DIRECT_PROFILE=on`.
//!
//! Attempts count executions before the direct site's guards; hits count
//! executions that pass every guard and enter the selected native entry.
//! Their difference is the number of calls handed to the reference shim.
//! The ordinal identifies the emitted direct site within its leaf, rather
//! than a bytecode pc (MIR and fusing can change that coordinate system).
//! Every emission also gets a process-unique id, so recompiled sites remain
//! distinguishable even when they share a source and ordinal.
//!
//! Threading: a process-wide mutex protects only the diagnostic registry.
//! It retains an Arc for every site until process exit, keeping the counter
//! addresses baked into generated code alive after leaf retirement. Site
//! metadata is immutable and contains only copied names and integer ids,
//! never Lisp values or mutator caches. Generated increments use atomic
//! RMWs, preserving counts across mutators; relaxed report loads publish no
//! runtime state and a live snapshot need not describe one instant. The
//! knob is read only at compile time. Off, no registration or CLIF is added.
//! An optional `NEOVM_JIT_DIRECT_PROFILE_FILE` exports immutable counter
//! addresses before code publication. An ancestor profiler can snapshot
//! these atomics around an edit-loop barrier. Registry locking serializes
//! complete TSV rows across compiler threads; cells remain alive at exit.

use super::super::knobs::jit_direct_profile_on;
use crate::emacs_core::intern::SymId;
use cranelift_codegen::ir::{AtomicRmwOp, InstBuilder, MemFlagsData, Type, types};
use cranelift_frontend::FunctionBuilder;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

/// One emitted site's immutable identity and concurrently readable counts.
/// Threading: retained by the process registry; metadata never changes and
/// only atomic operations access counters from generated code or reports.
pub(crate) struct SiteProfile {
    id: u64,
    owner: Box<str>,
    source_id: Box<str>,
    ordinal: usize,
    callee: Box<str>,
    attempts: AtomicU64,
    hits: AtomicU64,
}

static NEXT_SITE_ID: AtomicU64 = AtomicU64::new(1);
static ARMING_ATTEMPTS: AtomicU64 = AtomicU64::new(0);
static SITES: OnceLock<Mutex<Vec<Arc<SiteProfile>>>> = OnceLock::new();

/// Count the cold eligibility calls, including those that never publish an
/// entry. Threading: relaxed diagnostic increments publish no Lisp state.
pub(crate) fn note_arming() {
    if jit_direct_profile_on() {
        ARMING_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
    }
}

/// Register one emission when diagnostics are compile-enabled. Callers may
/// omit `callee` when lowering has only its SSA symbol word; self sites are
/// still named by their owning source. No callee is materialized here.
pub(crate) fn register_site(ordinal: usize, callee: Option<SymId>) -> Option<Arc<SiteProfile>> {
    jit_direct_profile_on().then(|| register_site_enabled(ordinal, callee))
}

fn register_site_enabled(ordinal: usize, callee: Option<SymId>) -> Arc<SiteProfile> {
    use crate::emacs_core::jit::stats::perf_map;

    let label = perf_map::active_label(perf_map::LabelTier::Baseline);
    let (owner, source_id) = label
        .as_deref()
        .and_then(perf_map::label_parts)
        .unwrap_or(("-", "0"));
    let callee = callee.map_or_else(
        || "-".into(),
        |sym| {
            let name = crate::emacs_core::intern::resolve_sym_lisp_string(sym)
                .as_utf8_str()
                .unwrap_or("<non-utf8>");
            perf_map::sanitize(name)
        },
    );
    let site = Arc::new(SiteProfile {
        id: NEXT_SITE_ID.fetch_add(1, Ordering::Relaxed),
        owner: owner.into(),
        source_id: source_id.into(),
        ordinal,
        callee,
        attempts: AtomicU64::new(0),
        hits: AtomicU64::new(0),
    });
    let mut sites = SITES
        .get_or_init(|| Mutex::new(Vec::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    sites.push(Arc::clone(&site));
    export_counter_addresses(&site);
    site
}

fn export_counter_addresses(site: &SiteProfile) {
    use std::io::Write;

    static PATH: OnceLock<Option<std::path::PathBuf>> = OnceLock::new();
    let Some(path) = PATH.get_or_init(|| {
        std::env::var_os("NEOVM_JIT_DIRECT_PROFILE_FILE")
            .filter(|value| !value.is_empty())
            .map(std::path::PathBuf::from)
    }) else {
        return;
    };
    let write = || -> std::io::Result<()> {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        if file.metadata()?.len() == 0 {
            writeln!(
                file,
                "pid\tid\tsource\tsite\towner\tattempts_addr\thits_addr"
            )?;
        }
        writeln!(file, "{}", site.counter_addresses())
    };
    if let Err(error) = write() {
        tracing::warn!(?path, %error, "cannot export direct-site diagnostic addresses");
    }
}

/// Count an execution immediately before the direct site's first guard.
/// Passing `None` emits nothing, preserving the off-mode CLIF verbatim.
pub(crate) fn emit_attempt(fb: &mut FunctionBuilder<'_>, ptr_ty: Type, site: Option<&SiteProfile>) {
    if let Some(site) = site {
        emit_increment(fb, ptr_ty, &site.attempts);
    }
}

/// Count a native-entry hit after every direct guard has passed, before
/// entering the callee. A hit includes calls whose callee later deopts or
/// signals; those are direct cold exits, not guard misses.
pub(crate) fn emit_hit(fb: &mut FunctionBuilder<'_>, ptr_ty: Type, site: Option<&SiteProfile>) {
    if let Some(site) = site {
        emit_increment(fb, ptr_ty, &site.hits);
    }
}

fn emit_increment(fb: &mut FunctionBuilder<'_>, ptr_ty: Type, counter: &AtomicU64) {
    let at = fb
        .ins()
        .iconst(ptr_ty, core::ptr::from_ref(counter) as usize as i64);
    let one = fb.ins().iconst(types::I64, 1);
    fb.ins().atomic_rmw(
        types::I64,
        MemFlagsData::trusted(),
        AtomicRmwOp::Add,
        at,
        one,
    );
}

impl SiteProfile {
    fn counter_addresses(&self) -> String {
        let owner = self.owner.replace(['\t', '\n', '\r'], "_");
        format!(
            "{}\t{}\t{}\t{}\t{}\t{:#x}\t{:#x}",
            std::process::id(),
            self.id,
            self.source_id,
            self.ordinal,
            owner,
            core::ptr::from_ref(&self.attempts) as usize,
            core::ptr::from_ref(&self.hits) as usize,
        )
    }

    fn render(&self) -> String {
        // Load hits first: every hit is sequenced after its attempt. Counts
        // can still advance during the snapshot, hence saturating subtraction.
        let hits = self.hits.load(Ordering::Relaxed);
        let attempts = self.attempts.load(Ordering::Relaxed);
        let misses = attempts.saturating_sub(hits);
        format!(
            "direct-profile:id={},owner={},source={},site={},callee={},attempts={attempts},hits={hits},misses={misses}",
            self.id, self.owner, self.source_id, self.ordinal, self.callee
        )
    }
}

/// Append every emitted site to the direct-call census, including sites
/// that were never executed or never reached the direct entry. No runtime
/// counters or Lisp state are consulted when no site was registered.
pub(crate) fn render_stats() -> Option<String> {
    if !jit_direct_profile_on() {
        return None;
    }
    let sites = SITES
        .get()
        .map(|sites| {
            sites
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone()
        })
        .unwrap_or_default();
    let rows = sites
        .iter()
        .map(|site| site.render())
        .collect::<Vec<_>>()
        .join(" ");
    Some(format!(
        "direct-profile:sites={},arming_attempts={} {rows}",
        sites.len(),
        ARMING_ATTEMPTS.load(Ordering::Relaxed)
    ))
}

#[cfg(test)]
#[path = "profile/tests/profile_test.rs"]
mod tests;
