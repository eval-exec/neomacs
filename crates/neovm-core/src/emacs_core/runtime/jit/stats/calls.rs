//! The call-target census (`[neovm-jit-final-calls]`, P2.1 C3 and F-1): at
//! exit, under `NEOVM_JIT_FEEDBACK` and a report knob, one summary line of
//! every recording call site of every live source, then the most executed
//! sites. The summary carries F-1's stability premise (R3): of the sites
//! compiled code executed at least `STABLE_WINDOW` times, how many changed
//! their target after that point.

use super::ReportTag;
use crate::emacs_core::jit::RuntimeState;
use crate::emacs_core::jit::feedback::{CallTarget, STABLE_WINDOW};

/// How many sites the ranked section prints.
pub(crate) const SITE_ROWS: usize = 24;

/// One recording site in the census (plain data).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SiteRow {
    /// The function owning the site (its current binding's name, else
    /// `anon`).
    pub(crate) owner: String,
    pub(crate) pc: u32,
    pub(crate) shape: &'static str,
    /// The lattice, rendered: `uninit`, `sym:NAME`, `source:NAME`,
    /// `poly:N` or `mega`.
    pub(crate) state: String,
    pub(crate) kind: StateKind,
    pub(crate) count: u32,
    pub(crate) late: u32,
}

/// The lattice word's kind, for the summary's buckets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StateKind {
    Uninit,
    Sym,
    Source,
    Poly,
    Mega,
}

impl StateKind {
    fn index(self) -> usize {
        self as usize
    }
}

/// Render the census: the summary line, then the most executed sites.
pub(crate) fn render(sources: usize, mut rows: Vec<SiteRow>) -> Vec<(ReportTag, String)> {
    let mut by_state = [0u64; 5];
    let mut exec_by_state = [0u64; 5];
    let mut by_shape = [0u64; 3];
    let (mut counted, mut window, mut late_sites, mut late_transitions) = (0u64, 0u64, 0u64, 0u64);
    let (mut window_mono_poly, mut late_mono_poly) = (0u64, 0u64);
    for row in &rows {
        by_state[row.kind.index()] += 1;
        exec_by_state[row.kind.index()] += u64::from(row.count);
        by_shape[match row.shape {
            "callee" => 0,
            "apply" => 1,
            _ => 2,
        }] += 1;
        if row.count > 0 {
            counted += 1;
        }
        late_transitions += u64::from(row.late);
        if row.count >= STABLE_WINDOW {
            window += 1;
            let mono_poly = matches!(
                row.kind,
                StateKind::Sym | StateKind::Source | StateKind::Poly
            );
            // A site that went megamorphic late was mono/poly at the window.
            let was_mono_poly = mono_poly || row.late > 0;
            if row.late > 0 {
                late_sites += 1;
            }
            if was_mono_poly {
                window_mono_poly += 1;
                if row.late > 0 {
                    late_mono_poly += 1;
                }
            }
        }
    }
    let mut lines = vec![(
        ReportTag::FinalCalls,
        format!(
            "sources={sources} sites={} shape[callee={} apply={} callback={}] \
             state[uninit={} sym={} source={} poly={} mega={}] counted={counted} \
             exec[uninit={} sym={} source={} poly={} mega={}] window={} window_sites={window} \
             late_sites={late_sites} late_transitions={late_transitions} \
             window_mono_poly={window_mono_poly} late_mono_poly={late_mono_poly}",
            rows.len(),
            by_shape[0],
            by_shape[1],
            by_shape[2],
            by_state[0],
            by_state[1],
            by_state[2],
            by_state[3],
            by_state[4],
            exec_by_state[0],
            exec_by_state[1],
            exec_by_state[2],
            exec_by_state[3],
            exec_by_state[4],
            STABLE_WINDOW,
        ),
    )];
    rows.sort_by(|a, b| b.count.cmp(&a.count).then(a.owner.cmp(&b.owner)));
    for row in rows.iter().filter(|r| r.count > 0).take(SITE_ROWS) {
        lines.push((
            ReportTag::FinalCallSite,
            format!(
                "fn={} pc={} shape={} state={} count={} late={}",
                row.owner, row.pc, row.shape, row.state, row.count, row.late
            ),
        ));
    }
    lines
}

/// Gather the census rows of every registered live source. Read-only on
/// `ctx` (one obarray walk for names; no interning, no allocation on the
/// Lisp heap).
pub(crate) fn collect(ctx: &crate::emacs_core::eval::Context) -> (usize, Vec<SiteRow>) {
    use std::sync::Arc;
    let sources = crate::emacs_core::jit::feedback::census::live_sources();
    if sources.is_empty() {
        return (0, Vec::new());
    }
    // Source state -> the name of a function cell holding it.
    let mut names: rustc_hash::FxHashMap<*const RuntimeState, String> = Default::default();
    for (name, f) in ctx.obarray.interned_function_cells_with_names() {
        let Some(bc) = f.bytecode_data_if_materialized() else {
            continue;
        };
        let Some(runtime) = bc.runtime.as_ref() else {
            continue;
        };
        names.entry(runtime.state_ptr()).or_insert_with(|| {
            super::epoch::report_token(
                crate::emacs_core::intern::resolve_name_lisp_string(name)
                    .as_utf8_str()
                    .unwrap_or("<non-utf8>"),
            )
        });
    }
    let name_of = |state: &Arc<RuntimeState>| {
        names
            .get(&Arc::as_ptr(state))
            .cloned()
            .unwrap_or_else(|| "anon".to_string())
    };
    let mut rows = Vec::new();
    for source in &sources {
        let Some(sites) = source.call_sites() else {
            continue;
        };
        let owner = name_of(source);
        for site in sites.sites() {
            let (kind, state) = match site.target() {
                CallTarget::Uninit => (StateKind::Uninit, "uninit".to_string()),
                CallTarget::Sym(sym) => (
                    StateKind::Sym,
                    format!(
                        "sym:{}",
                        super::epoch::report_token(crate::emacs_core::intern::resolve_sym(sym))
                    ),
                ),
                CallTarget::Sources(targets) if targets.len() == 1 => (
                    StateKind::Source,
                    format!("source:{}", name_of(&targets[0])),
                ),
                CallTarget::Sources(targets) => {
                    (StateKind::Poly, format!("poly:{}", targets.len()))
                }
                CallTarget::Mega => (StateKind::Mega, "mega".to_string()),
            };
            rows.push(SiteRow {
                owner: owner.clone(),
                pc: site.pc(),
                shape: site.shape().name(),
                state,
                kind,
                count: site.count(),
                late: site.late(),
            });
        }
    }
    (sources.len(), rows)
}
