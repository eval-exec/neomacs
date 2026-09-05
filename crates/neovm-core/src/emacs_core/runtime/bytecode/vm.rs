//! Bytecode virtual machine — stack-based interpreter.

use std::collections::HashSet;
use std::sync::OnceLock;

use smallvec::SmallVec;

use super::chunk::ByteCodeFunction;
use super::opcode::Op;
use crate::emacs_core::builtins;
use crate::emacs_core::error::*;
use crate::emacs_core::eval::{
    BytecodeBacktraceFrame, BytecodeStackCallDispatch, ConditionFrame, LispArgVec, ResumeTarget,
    SubrEntry, lookup_global_subr_entry, subr_entry_from_value,
};
use crate::emacs_core::intern::{SymId, intern, lookup_interned, resolve_sym};
// storage_char_len and storage_substring no longer needed here — using emacs_char + LispString
use crate::emacs_core::value::*;
use crate::tagged::header::{SubrDispatchKind, SubrFn, SubrObj};
use crate::tagged::value::TAG_MASK;
use crate::window::FrameId;

/// Dynamic, execution-weighted opcode histogram for the Tier-0 interpreter
/// dispatch loop. Compiled in ONLY under the `vm-profile` feature, so the
/// production loop's bump site vanishes entirely — zero cost when off (no env
/// check, no branch). This is the EXECUTION-weighted op-mix the deferred JIT
/// work (tier-0 ICs / quickening) needs to size itself; distinct from the
/// STATIC per-compiled-function op-mix behind `NEOVM_JIT_PROFILE`
/// (jit/compile.rs), which counts a function's ops once at compile time.
#[cfg(feature = "vm-profile")]
pub(crate) mod vm_profile {
    use super::Op;
    use crate::emacs_core::intern::SymId;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::fmt::Write as _;

    /// Entry-kind tags for the bytecode call-op that dispatched a builtin.
    /// Round-2 intrinsics enter the JIT through DIFFERENT lowerings, so the
    /// adjudication needs to know, per builtin, which op population it comes
    /// from: `Op::Call` (generic funcall — the `find_spec_sites` speculation
    /// path from round 1) vs `Op::CallBuiltinSym` (buffer/point ops, the
    /// `neovm_jit_named_builtin` lowering that would need a NEW spec-site
    /// extension) vs `Op::CallBuiltin` (name-based, override-aware).
    pub(crate) const ENTRY_CALL: u8 = 1;
    pub(crate) const ENTRY_CALLBUILTINSYM: u8 = 2;
    pub(crate) const ENTRY_CALLBUILTIN: u8 = 3;

    /// `Op::Call` callee resolution-kind classes (Task-4 counters): what the
    /// current dispatch path does with the callee value, classified BEFORE the
    /// call so the closure-vs-builtin split of the `Op::Call` population is
    /// measured directly instead of derived by subtraction.
    pub(crate) const CK_BUILTIN_SYM: u8 = 0; // symbol -> subr cell / global subr entry
    pub(crate) const CK_CLOSURE_SYM: u8 = 1; // symbol -> bytecode cell (re-resolved per call)
    pub(crate) const CK_OTHER_SYM: u8 = 2; // symbol -> lambda/advice/alias/autoload/void/overrides
    pub(crate) const CK_CLOSURE_VAL: u8 = 3; // bytecode object callee (no resolution to cache)
    pub(crate) const CK_SUBR_VAL: u8 = 4; // subr object callee
    pub(crate) const CK_OTHER_VAL: u8 = 5; // any other callee value
    pub(crate) const CK_COUNT: usize = 6;
    const CK_NAMES: [&str; CK_COUNT] = [
        "builtin-sym (symbol -> subr cell/global entry)",
        "closure-sym (symbol -> bytecode cell; re-resolved per call)",
        "other-sym   (lambda/advice/alias/autoload/void/overrides)",
        "closure-val (bytecode object callee; no resolution)",
        "subr-val    (subr object callee)",
        "other-val   (any other callee value)",
    ];

    /// Per-site callee keys: symbols carry their SymId (tag 1 in the low
    /// bits); non-symbol callees collapse into one bucket per value class —
    /// a symbol-keyed call IC cannot cache them, so per-value identity churn
    /// (fresh closures per iteration) must not masquerade as polymorphism.
    pub(crate) const SITE_KEY_CLOSURE_VAL: u64 = 2;
    pub(crate) const SITE_KEY_SUBR_VAL: u64 = 3;
    pub(crate) const SITE_KEY_OTHER_VAL: u64 = 4;
    pub(crate) fn site_key_for_symbol(id: SymId) -> u64 {
        ((id.0 as u64) << 3) | 1
    }
    fn site_key_name(key: u64) -> String {
        if key & 7 == 1 {
            crate::emacs_core::intern::resolve_sym(SymId((key >> 3) as u32)).to_string()
        } else {
            match key {
                SITE_KEY_CLOSURE_VAL => "#<closure-val>".to_string(),
                SITE_KEY_SUBR_VAL => "#<subr-val>".to_string(),
                _ => "#<other-val>".to_string(),
            }
        }
    }

    /// `Op::VarRef` resolution classes (Task-4 BLV sizing): which branch of
    /// `fast_path_var_ref`/`lookup_var_id` the read takes. `PLAIN_NIL` is
    /// interesting on its own: a nil-valued Plainval read still pays a
    /// buffer-local probe (the buffer-undo-list compat shim), win or lose.
    pub(crate) const VR_PLAIN: u8 = 0; // Plainval, non-nil, direct return
    pub(crate) const VR_PLAIN_NIL: u8 = 1; // Plainval nil; buffer-local probe MISSED
    pub(crate) const VR_PLAIN_NIL_BLV: u8 = 2; // Plainval nil; buffer-local probe HIT
    pub(crate) const VR_LOCALIZED: u8 = 3; // SYMBOL_LOCALIZED (true BLV machinery)
    pub(crate) const VR_FORWARDED: u8 = 4; // SYMBOL_FORWARDED (per-buffer/C slot)
    pub(crate) const VR_SLOW_OTHER: u8 = 5; // unbound/alias-to-plain/error paths
    pub(crate) const VR_COUNT: usize = 6;
    const VR_NAMES: [&str; VR_COUNT] = [
        "plain         (Plainval non-nil, direct)",
        "plain-nil     (Plainval nil; BLV probe MISS — probe still paid)",
        "plain-nil-blv (Plainval nil; BLV probe HIT — buffer-local value)",
        "localized     (SYMBOL_LOCALIZED — true BLV machinery)",
        "forwarded     (SYMBOL_FORWARDED — per-buffer/C slot)",
        "slow-other    (unbound / alias-to-plain / error paths)",
    ];

    /// (function identity, call-site pc) — one bytecode `Op::Call` site.
    type SiteId = (usize, u32);
    /// Per-site callee histogram rows: (callee key, execution count).
    type SiteRows = Vec<(u64, u64)>;

    thread_local! {
        static OP_COUNTS: RefCell<HashMap<String, u64>> = RefCell::new(HashMap::new());
        /// Adjacent executed-opcode PAIRS, and the previous op's name.
        ///
        /// Single-op frequencies say which arms are hot; they do not say which
        /// FUSIONS are worth writing. A superinstruction (the existing
        /// `Dup`/`StackRef`/`Lss`/`GotoIfNil` arm is one) removes a dispatch
        /// only for the pair it matches, so the pair distribution is what
        /// justifies adding another.
        static OP_PAIR_COUNTS: RefCell<HashMap<(String, String), u64>> =
            RefCell::new(HashMap::new());
        static PREV_OP: RefCell<Option<String>> = const { RefCell::new(None) };
        static SUBR_COUNTS: RefCell<HashMap<SymId, u64>> = RefCell::new(HashMap::new());
        /// (callee SymId, ENTRY_* tag) -> count of bytecode call-ops that
        /// dispatched that callee through that op. Populated at the run_loop
        /// call arms (not `subr_entry_from_value`), so each count attributes to
        /// the STATIC callee symbol of the exact op that issued the call — the
        /// Op::Call-vs-CallBuiltinSym entry split the round-2 report needs.
        static ENTRY_COUNTS: RefCell<HashMap<(u32, u8), u64>> = RefCell::new(HashMap::new());
        /// Execution counts per CK_* resolution kind at the Op::Call arm.
        static CALL_KIND_COUNTS: RefCell<[u64; CK_COUNT]> = const { RefCell::new([0; CK_COUNT]) };
        /// (function identity, call-site pc) -> per-site callee histogram.
        /// Identity is the `&ByteCodeFunction` address: stable while alive
        /// (non-moving GC); free-then-reuse ABA is acceptable measurement
        /// noise. Execution-WEIGHTED, unlike the JIT `FeedbackVec` (a 3-state
        /// lattice, per-instance, not enumerable without a heap walk) — this
        /// is the per-site polymorphism table the T1 report flagged missing.
        static CALL_SITES: RefCell<HashMap<SiteId, SiteRows>> = RefCell::new(HashMap::new());
        /// (symbol, VR_* class) -> Op::VarRef read count.
        static VARREF_COUNTS: RefCell<HashMap<(u32, u8), u64>> = RefCell::new(HashMap::new());
        /// Reads whose resolution crossed a variable alias (any class).
        static VARREF_ALIAS: RefCell<u64> = const { RefCell::new(0) };
    }

    /// Bump the per-builtin call histogram. Hooked at `subr_entry_from_value`
    /// (eval.rs), the single resolver every subr dispatch path funnels through
    /// (tree-walk eval, Op::Call funcall, and CallBuiltinSym via
    /// funcall_general), so this ranks WHICH builtins a workload actually
    /// calls — the input the JIT builtin-intrinsics work needs (the op
    /// histogram above strips the callee).
    pub(crate) fn bump_subr(id: SymId) {
        SUBR_COUNTS.with(|c| {
            *c.borrow_mut().entry(id).or_insert(0) += 1;
        });
    }

    /// Record a bytecode call-op targeting `sym`, split by the dispatching op
    /// (`ENTRY_*`). Hooked in `run_loop`'s `Op::Call`/`Op::CallBuiltin`/
    /// `Op::CallBuiltinSym` arms so the round-2 report can show, per builtin,
    /// the Op::Call-vs-CallBuiltinSym entry split (the two lowerings).
    ///
    /// This is a superset denominator of `bump_subr`: an `Op::Call` whose
    /// callee is a bytecode object (not a subr) also lands here, but such rows
    /// are filtered out of the ranking, which is keyed by the SUBR-MIX totals.
    /// Conversely, calls that never traverse `run_loop` (tree-walked eval,
    /// direct `funcall`/`apply`) are counted only in the SUBR-MIX total and
    /// show up as the report's "other" column.
    pub(crate) fn bump_entry(sym: SymId, kind: u8) {
        ENTRY_COUNTS.with(|c| {
            *c.borrow_mut().entry((sym.0, kind)).or_insert(0) += 1;
        });
    }

    /// Bump the executed-op histogram (once per dispatched op while profiling).
    /// Keyed by the variant name without operands ("StackRef(3)" -> "StackRef").
    pub(crate) fn bump(op: &Op) {
        let dbg = format!("{op:?}");
        let name = dbg.split(['(', ' ', '{']).next().unwrap_or(dbg.as_str());
        OP_COUNTS.with(|c| {
            let mut m = c.borrow_mut();
            if let Some(v) = m.get_mut(name) {
                *v += 1;
            } else {
                m.insert(name.to_string(), 1);
            }
        });
        PREV_OP.with(|prev| {
            let mut prev = prev.borrow_mut();
            if let Some(prev_name) = prev.as_deref() {
                OP_PAIR_COUNTS.with(|c| {
                    let mut m = c.borrow_mut();
                    if let Some(v) = m.get_mut(&(prev_name.to_string(), name.to_string())) {
                        *v += 1;
                    } else {
                        m.insert((prev_name.to_string(), name.to_string()), 1);
                    }
                });
            }
            *prev = Some(name.to_string());
        });
    }

    /// Record one `Op::Call` execution: its CK_* resolution kind plus the
    /// callee key under its call site (function identity, pc).
    pub(crate) fn bump_call_site(func_ident: usize, pc: u32, key: u64, kind: u8) {
        CALL_KIND_COUNTS.with(|c| c.borrow_mut()[kind as usize] += 1);
        CALL_SITES.with(|c| {
            let mut m = c.borrow_mut();
            let per_site = m.entry((func_ident, pc)).or_default();
            if let Some(row) = per_site.iter_mut().find(|row| row.0 == key) {
                row.1 += 1;
            } else {
                per_site.push((key, 1));
            }
        });
    }

    /// Record one `Op::VarRef` execution under its symbol + VR_* class.
    pub(crate) fn bump_varref(sym: SymId, class: u8, via_alias: bool) {
        VARREF_COUNTS.with(|c| {
            *c.borrow_mut().entry((sym.0, class)).or_insert(0) += 1;
        });
        if via_alias {
            VARREF_ALIAS.with(|c| *c.borrow_mut() += 1);
        }
    }

    /// Clear the histograms (call before a measured workload).
    pub(crate) fn reset() {
        OP_COUNTS.with(|c| c.borrow_mut().clear());
        OP_PAIR_COUNTS.with(|c| c.borrow_mut().clear());
        PREV_OP.with(|c| *c.borrow_mut() = None);
        SUBR_COUNTS.with(|c| c.borrow_mut().clear());
        ENTRY_COUNTS.with(|c| c.borrow_mut().clear());
        CALL_KIND_COUNTS.with(|c| *c.borrow_mut() = [0; CK_COUNT]);
        CALL_SITES.with(|c| c.borrow_mut().clear());
        VARREF_COUNTS.with(|c| c.borrow_mut().clear());
        VARREF_ALIAS.with(|c| *c.borrow_mut() = 0);
    }

    /// Format the OP-MIX + SUBR-MIX (with the per-builtin entry split) into a
    /// String. Shared by [`dump`] and the `neovm--vm-profile-dump` debug subr.
    pub(crate) fn report(label: &str) -> String {
        let mut out = String::new();
        let mut rows: Vec<(String, u64)> =
            OP_COUNTS.with(|c| c.borrow().iter().map(|(k, v)| (k.clone(), *v)).collect());
        rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let total: u64 = rows.iter().map(|r| r.1).sum();
        let _ = writeln!(
            out,
            "=== OP-MIX [{label}]: {total} ops executed, {} distinct ===",
            rows.len()
        );
        for (name, count) in &rows {
            let pct = 100.0 * *count as f64 / total.max(1) as f64;
            let _ = writeln!(out, "  {name:<16} {count:>12}  {pct:5.2}%");
        }

        // Adjacent pairs: which superinstruction would actually pay.
        let mut pair_rows: Vec<((String, String), u64)> =
            OP_PAIR_COUNTS.with(|c| c.borrow().iter().map(|(k, v)| (k.clone(), *v)).collect());
        pair_rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let pair_total: u64 = pair_rows.iter().map(|r| r.1).sum();
        let _ = writeln!(
            out,
            "=== OP-PAIRS [{label}]: {pair_total} transitions, {} distinct ===",
            pair_rows.len()
        );
        for ((a, b), count) in pair_rows.iter().take(25) {
            let pct = 100.0 * *count as f64 / pair_total.max(1) as f64;
            let _ = writeln!(out, "  {a:<16} -> {b:<16} {count:>12}  {pct:5.2}%");
        }

        let entry: HashMap<(u32, u8), u64> = ENTRY_COUNTS.with(|c| c.borrow().clone());
        let mut subr_rows: Vec<(SymId, u64)> =
            SUBR_COUNTS.with(|c| c.borrow().iter().map(|(k, v)| (*k, *v)).collect());
        subr_rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.0.cmp(&b.0.0)));
        let subr_total: u64 = subr_rows.iter().map(|r| r.1).sum();
        let _ = writeln!(
            out,
            "=== SUBR-MIX [{label}]: {subr_total} builtin calls, {} distinct ===",
            subr_rows.len()
        );
        let _ = writeln!(
            out,
            "  (entry split: Op::Call | CBSym=CallBuiltinSym | CBtin=CallBuiltin | other=tree-walk/funcall)"
        );
        let _ = writeln!(
            out,
            "  {:<28} {:>12} {:>6}  {:>11} {:>11} {:>8} {:>11}",
            "builtin", "calls", "%", "Op::Call", "CBSym", "CBtin", "other"
        );
        for (id, count) in subr_rows.iter().take(120) {
            let name = crate::emacs_core::intern::resolve_sym(*id);
            let pct = 100.0 * *count as f64 / subr_total.max(1) as f64;
            let opcall = entry.get(&(id.0, ENTRY_CALL)).copied().unwrap_or(0);
            let cbsym = entry
                .get(&(id.0, ENTRY_CALLBUILTINSYM))
                .copied()
                .unwrap_or(0);
            let cbtin = entry.get(&(id.0, ENTRY_CALLBUILTIN)).copied().unwrap_or(0);
            let other = count.saturating_sub(opcall + cbsym + cbtin);
            let _ = writeln!(
                out,
                "  {name:<28} {count:>12} {pct:5.2}%  {opcall:>11} {cbsym:>11} {cbtin:>8} {other:>11}"
            );
        }

        // --- CALL-KIND: closure-vs-builtin split of the Op::Call population ---
        let kinds = CALL_KIND_COUNTS.with(|c| *c.borrow());
        let kind_total: u64 = kinds.iter().sum();
        let _ = writeln!(
            out,
            "=== CALL-KIND [{label}]: {kind_total} Op::Call executions ==="
        );
        for (i, name) in CK_NAMES.iter().enumerate() {
            let count = kinds[i];
            let pct = 100.0 * count as f64 / kind_total.max(1) as f64;
            let _ = writeln!(out, "  {name:<60} {count:>12}  {pct:5.2}%");
        }

        // --- CALL-SITES: execution-weighted per-site polymorphism ---
        let sites: Vec<(SiteId, SiteRows)> =
            CALL_SITES.with(|c| c.borrow().iter().map(|(k, v)| (*k, v.clone())).collect());
        let site_total_execs: u64 = sites.iter().flat_map(|s| s.1.iter().map(|r| r.1)).sum();
        let _ = writeln!(
            out,
            "=== CALL-SITES [{label}]: {} sites, {site_total_execs} executions ===",
            sites.len()
        );
        let mut by_arity = [(0u64, 0u64); 3]; // [1, 2, >=3] -> (sites, execs)
        let mut nonsym = (0u64, 0u64); // sites with any non-symbol callee key
        for (_, rows) in &sites {
            let execs: u64 = rows.iter().map(|r| r.1).sum();
            let bucket = (rows.len().min(3)) - 1;
            by_arity[bucket].0 += 1;
            by_arity[bucket].1 += execs;
            if rows.iter().any(|r| r.0 & 7 != 1) {
                nonsym.0 += 1;
                nonsym.1 += rows
                    .iter()
                    .filter(|r| r.0 & 7 != 1)
                    .map(|r| r.1)
                    .sum::<u64>();
            }
        }
        for (i, label_txt) in ["1 callee (monomorphic)", "2 callees", ">=3 callees"]
            .iter()
            .enumerate()
        {
            let (s, e) = by_arity[i];
            let spct = 100.0 * s as f64 / (sites.len().max(1)) as f64;
            let epct = 100.0 * e as f64 / site_total_execs.max(1) as f64;
            let _ = writeln!(
                out,
                "  {label_txt:<24} {s:>8} sites {spct:5.2}%  |  {e:>12} execs {epct:5.2}%"
            );
        }
        let _ = writeln!(
            out,
            "  non-symbol-callee execs: {} (at {} sites) — not symbol-IC-cacheable",
            nonsym.1, nonsym.0
        );
        let mut poly: Vec<&(SiteId, SiteRows)> = sites.iter().filter(|s| s.1.len() > 1).collect();
        poly.sort_by_key(|s| std::cmp::Reverse(s.1.iter().map(|r| r.1).sum::<u64>()));
        for ((func_ident, pc), rows) in poly.iter().take(12) {
            let execs: u64 = rows.iter().map(|r| r.1).sum();
            let mut rows = rows.clone();
            rows.sort_by_key(|r| std::cmp::Reverse(r.1));
            let callees = rows
                .iter()
                .map(|(k, n)| format!("{}({n})", site_key_name(*k)))
                .collect::<Vec<_>>()
                .join(" ");
            let _ = writeln!(
                out,
                "  poly site fn@{func_ident:#x} pc={pc} execs={execs}: {callees}"
            );
        }

        // --- VARREF-MIX: per-class + per-symbol Op::VarRef breakdown ---
        let varrefs: Vec<((u32, u8), u64)> =
            VARREF_COUNTS.with(|c| c.borrow().iter().map(|(k, v)| (*k, *v)).collect());
        let vr_total: u64 = varrefs.iter().map(|r| r.1).sum();
        let mut vr_class = [0u64; VR_COUNT];
        let mut per_sym: HashMap<u32, [u64; VR_COUNT]> = HashMap::new();
        for ((sym, class), count) in &varrefs {
            vr_class[*class as usize] += count;
            per_sym.entry(*sym).or_default()[*class as usize] += count;
        }
        let _ = writeln!(
            out,
            "=== VARREF-MIX [{label}]: {vr_total} reads, {} distinct symbols ===",
            per_sym.len()
        );
        for (i, name) in VR_NAMES.iter().enumerate() {
            let count = vr_class[i];
            let pct = 100.0 * count as f64 / vr_total.max(1) as f64;
            let _ = writeln!(out, "  {name:<64} {count:>12}  {pct:5.2}%");
        }
        let alias = VARREF_ALIAS.with(|c| *c.borrow());
        let _ = writeln!(out, "  via-alias (any class): {alias}");
        let blv_value = vr_class[VR_PLAIN_NIL_BLV as usize] + vr_class[VR_LOCALIZED as usize];
        let buffer_consulting =
            blv_value + vr_class[VR_PLAIN_NIL as usize] + vr_class[VR_FORWARDED as usize];
        let _ = writeln!(
            out,
            "  buffer-local VALUE reads (plain-nil-blv+localized): {blv_value} ({:.2}%)",
            100.0 * blv_value as f64 / vr_total.max(1) as f64
        );
        let _ = writeln!(
            out,
            "  buffer-CONSULTING reads (+plain-nil probes+forwarded): {buffer_consulting} ({:.2}%)",
            100.0 * buffer_consulting as f64 / vr_total.max(1) as f64
        );
        let mut sym_rows: Vec<(u32, [u64; VR_COUNT], u64)> = per_sym
            .into_iter()
            .map(|(sym, classes)| (sym, classes, classes.iter().sum::<u64>()))
            .collect();
        sym_rows.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.0.cmp(&b.0)));
        let _ = writeln!(
            out,
            "  {:<36} {:>11} {:>6} | {:>9} {:>9} {:>9} {:>9} {:>9} {:>9}",
            "symbol", "reads", "%", "plain", "pl-nil", "nil-blv", "localizd", "forward", "slow"
        );
        for (sym, classes, total) in sym_rows.iter().take(40) {
            let name = crate::emacs_core::intern::resolve_sym(SymId(*sym));
            let pct = 100.0 * *total as f64 / vr_total.max(1) as f64;
            let _ = writeln!(
                out,
                "  {name:<36} {total:>11} {pct:5.2}% | {:>9} {:>9} {:>9} {:>9} {:>9} {:>9}",
                classes[0], classes[1], classes[2], classes[3], classes[4], classes[5]
            );
        }
        out
    }

    /// Print the OP-MIX + SUBR-MIX (with entry split) to stderr.
    pub(crate) fn dump(label: &str) {
        eprint!("{}", report(label));
    }
}

/// Local marker for catch/condition-case frames mirrored into the shared
/// condition runtime.
#[derive(Clone, Debug)]
enum Handler {
    /// Local marker corresponding to a catch/condition-case frame already
    /// stored in `Context.condition_stack`.
    Condition,
}

type HandlerStack = SmallVec<[Handler; 4]>;
type BindStack = SmallVec<[usize; 8]>;

#[cfg(test)]
thread_local! {
    static RUN_LOOP_DEPTH: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static RUN_LOOP_MAX_DEPTH: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static ITERATIVE_CONTEXT_BC_FRAMES_MAX: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static GENERIC_BYTECODE_CLEANUP_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static OPCODE_DISPATCH_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static MUTATING_WRITEBACK_CLASSIFICATION_COUNT: std::cell::Cell<usize> =
        const { std::cell::Cell::new(0) };
    static INLINE_BUILTIN_DIRECT_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
struct RunLoopDepthGuard;

#[cfg(test)]
impl RunLoopDepthGuard {
    fn enter() -> Self {
        RUN_LOOP_DEPTH.with(|depth| {
            let current = depth.get() + 1;
            depth.set(current);
            RUN_LOOP_MAX_DEPTH.with(|maximum| maximum.set(maximum.get().max(current)));
        });
        Self
    }
}

#[cfg(test)]
impl Drop for RunLoopDepthGuard {
    fn drop(&mut self) {
        RUN_LOOP_DEPTH.with(|depth| depth.set(depth.get() - 1));
    }
}

#[cfg(test)]
fn reset_run_loop_max_depth() {
    RUN_LOOP_DEPTH.with(|depth| depth.set(0));
    RUN_LOOP_MAX_DEPTH.with(|maximum| maximum.set(0));
}

#[cfg(test)]
fn run_loop_max_depth() -> usize {
    RUN_LOOP_MAX_DEPTH.with(std::cell::Cell::get)
}

#[cfg(test)]
fn reset_iterative_context_bc_frames_max() {
    ITERATIVE_CONTEXT_BC_FRAMES_MAX.with(|maximum| maximum.set(0));
}

#[cfg(test)]
fn observe_iterative_context_bc_frames_len(len: usize) {
    ITERATIVE_CONTEXT_BC_FRAMES_MAX.with(|maximum| maximum.set(maximum.get().max(len)));
}

#[cfg(test)]
fn iterative_context_bc_frames_max() -> usize {
    ITERATIVE_CONTEXT_BC_FRAMES_MAX.with(std::cell::Cell::get)
}

#[cfg(test)]
fn reset_generic_bytecode_cleanup_count() {
    GENERIC_BYTECODE_CLEANUP_COUNT.with(|count| count.set(0));
}

#[cfg(test)]
fn generic_bytecode_cleanup_count() -> usize {
    GENERIC_BYTECODE_CLEANUP_COUNT.with(std::cell::Cell::get)
}

#[cfg(test)]
fn reset_opcode_dispatch_count() {
    OPCODE_DISPATCH_COUNT.with(|count| count.set(0));
}

#[cfg(test)]
fn opcode_dispatch_count() -> usize {
    OPCODE_DISPATCH_COUNT.with(std::cell::Cell::get)
}

#[cfg(test)]
fn reset_mutating_writeback_classification_count() {
    MUTATING_WRITEBACK_CLASSIFICATION_COUNT.with(|count| count.set(0));
}

#[cfg(test)]
fn mutating_writeback_classification_count() -> usize {
    MUTATING_WRITEBACK_CLASSIFICATION_COUNT.with(std::cell::Cell::get)
}

#[cfg(test)]
fn reset_inline_builtin_direct_count() {
    INLINE_BUILTIN_DIRECT_COUNT.with(|count| count.set(0));
}

#[cfg(test)]
fn inline_builtin_direct_count() -> usize {
    INLINE_BUILTIN_DIRECT_COUNT.with(std::cell::Cell::get)
}

use crate::emacs_core::eval::SpecBinding;

#[cold]
#[inline(never)]
fn invalid_bytecode_flow() -> Flow {
    signal("error", vec![Value::string("Invalid byte-code")])
}

#[cold]
#[inline(never)]
fn trace_invalid_bytecode_site(
    func: &ByteCodeFunction,
    reason: &str,
    pc: usize,
    frame_base: usize,
    frame_limit: usize,
    stack_len: usize,
    op: Option<&Op>,
) {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    if !*ENABLED.get_or_init(|| std::env::var_os("NEOMACS_TRACE_INVALID_BYTECODE").is_some()) {
        return;
    }

    let ops = func.executable_ops();
    let gnu_byte_offset = func.executable_gnu_byte_offset_map().and_then(|map| {
        map.iter()
            .find_map(|entry| (entry.instruction_index == pc).then_some(entry.byte_offset))
    });
    let op_window_start = pc.saturating_sub(8);
    let op_window_end = (pc + 8).min(ops.len());
    let op_window = ops[op_window_start..op_window_end]
        .iter()
        .enumerate()
        .map(|(idx, op)| format!("{}:{:?}", op_window_start + idx, op))
        .collect::<Vec<_>>()
        .join(" ");
    let raw_bytes = func.gnu_bytecode_bytes.as_ref().map(|bytes| {
        let start = gnu_byte_offset.unwrap_or(0).saturating_sub(12);
        let end = (gnu_byte_offset.unwrap_or(0) + 24).min(bytes.len());
        bytes[start..end]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<Vec<_>>()
            .join(" ")
    });
    tracing::error!(
        reason,
        pc,
        gnu_byte_offset,
        ?op,
        op_window,
        raw_bytes,
        stack_len,
        frame_base,
        frame_limit,
        max_stack = func.max_stack,
        ops_len = ops.len(),
        constants_len = func.constants.len(),
        lexical = func.lexical,
        "Invalid byte-code"
    );
}

/// A6: register-resident operand-stack cursor for `run_loop`.
///
/// Holds the operand stack's base pointer and logical length in locals so
/// per-opcode stack traffic is plain pointer arithmetic instead of a
/// `self.ctx.bc_buf` pointer/len load chain plus a len store per op. GNU
/// Emacs keeps `top` and `pc` in registers the same way (bytecode.c, "The
/// interpreter can be compiled one of two ways" / exec_byte_code locals).
///
/// GNU can leave `top` unpublished across calls because its GC marks the
/// whole `maxdepth` region of each bytecode frame conservatively. Our GC is
/// precise — the roots are exactly `bc_buf[..len]` at a safe point — so this
/// cursor imposes a publication discipline instead:
///
/// - `publish` (which takes `self` BY VALUE) writes the logical length back
///   into `bc_buf` before any escape into `Context`/eval that could reach a
///   GC safe point, run Lisp, or push/truncate `bc_buf`. Because `publish`
///   moves the cursor, any stale use after an escape is a borrow-check error,
///   and `acquire` must re-derive base+len afterwards (the Vec may have
///   reallocated).
/// - The cursor itself never grows the Vec: pushes are bounded by
///   `frame_limit`, whose capacity `run_frame` reserved up front. Vec-growing
///   operations (e.g. Op::Apply's list spread) run published.
/// - In debug builds a thread-local flag turns a missed publication before GC
///   into a deterministic panic instead of silent heap corruption; GC entry
///   asserts it via `debug_assert_no_live_stack_cursor`.
pub(crate) struct StackCursor {
    base: *mut Value,
    len: usize,
}

#[cfg(debug_assertions)]
thread_local! {
    static STACK_CURSOR_LIVE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Debug hook for GC entry points: a collection must never observe a live
/// (unpublished) operand-stack cursor — it would mark a stale stack length.
#[cfg(debug_assertions)]
pub(crate) fn debug_assert_no_live_stack_cursor() {
    STACK_CURSOR_LIVE.with(|flag| {
        assert!(
            !flag.get(),
            "GC entered while a bytecode StackCursor held unpublished stack state"
        );
    });
}

impl StackCursor {
    #[inline(always)]
    fn acquire(ctx: &mut crate::emacs_core::eval::Context) -> Self {
        #[cfg(debug_assertions)]
        STACK_CURSOR_LIVE.with(|flag| {
            assert!(!flag.get(), "acquired a StackCursor while another is live");
            flag.set(true);
        });
        Self {
            base: ctx.bc_buf.as_mut_ptr(),
            len: ctx.bc_buf.len(),
        }
    }

    #[inline(always)]
    fn publish(self, ctx: &mut crate::emacs_core::eval::Context) {
        #[cfg(debug_assertions)]
        STACK_CURSOR_LIVE.with(|flag| flag.set(false));
        debug_assert!(self.len <= ctx.bc_buf.capacity());
        // SAFETY: every slot below `len` was either already initialized in
        // bc_buf or written through the cursor; Value is Copy with no Drop.
        unsafe { ctx.bc_buf.set_len(self.len) }
    }

    /// Debug-only: mirror the live length into the context so
    /// Context-side debug assertions (which cannot see the cursor) stay
    /// accurate while the cursor remains live across the iterative call
    /// transition. Release builds never observe the stale length on this
    /// path.
    #[cfg(debug_assertions)]
    fn debug_sync_len(&self, ctx: &mut crate::emacs_core::eval::Context) {
        debug_assert!(self.len <= ctx.bc_buf.capacity());
        // SAFETY: same initialization argument as `publish`.
        unsafe { ctx.bc_buf.set_len(self.len) }
    }

    #[inline(always)]
    fn pop(&mut self) -> Option<Value> {
        if self.len == 0 {
            return None;
        }
        self.len -= 1;
        // SAFETY: len was > 0, so slot len-1 is initialized.
        Some(unsafe { self.base.add(self.len).read() })
    }

    #[inline(always)]
    fn truncate(&mut self, new_len: usize) {
        if new_len < self.len {
            self.len = new_len;
        }
    }

    /// SAFETY: caller must have checked `self.len < frame_limit`, and
    /// `frame_limit <= bc_buf.capacity()` (reserved by run_frame).
    #[inline(always)]
    unsafe fn push_unchecked(&mut self, value: Value) {
        unsafe { self.base.add(self.len).write(value) };
        self.len += 1;
    }
}

impl std::ops::Deref for StackCursor {
    type Target = [Value];
    #[inline(always)]
    fn deref(&self) -> &[Value] {
        // SAFETY: base/len describe the initialized prefix of bc_buf, which
        // cannot move while the cursor is live (no Vec growth unpublished).
        unsafe { std::slice::from_raw_parts(self.base, self.len) }
    }
}

impl std::ops::DerefMut for StackCursor {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut [Value] {
        // SAFETY: as Deref, and the cursor has exclusive access to the frame.
        unsafe { std::slice::from_raw_parts_mut(self.base, self.len) }
    }
}

/// One-word proof that a call target resolved to an ordinary builtin subr.
///
/// A symbol value denotes the legacy static-table fallback; a subr value is
/// the exact live function-cell object read by Bcall. Constructors validate
/// the corresponding metadata before creating the token, so dispatch can
/// materialize the relatively large [`SubrEntry`] only after selecting the
/// builtin branch.
#[repr(transparent)]
#[derive(Clone, Copy)]
struct ResolvedBuiltinCallee(Value);

/// One-word proof that a value is the bytecode object read from a symbol's
/// live function cell.
///
/// The wrapper is deliberately distinct from an arbitrary [`Value`]: only a
/// successfully classified function-cell read can populate the symbol-call
/// cache, so a cache hit can skip both the paged symbol lookup and the heap
/// kind check without making an unchecked value callable.
#[repr(transparent)]
#[derive(Clone, Copy)]
struct ResolvedByteCodeCallee(Value);

/// Proof that a resolved bytecode callee may enter the current Tier-0 driver
/// directly for the exact argument count carried by its cache key.
///
/// Construction is private to [`Vm::resolve_interpreter_stack_call_target`]:
/// it requires both an interpreter-only execution policy and GNU
/// `setup_frame` eligibility.  Keeping this distinct from a merely resolved
/// bytecode object makes accidentally bypassing adaptive tier dispatch or
/// arity handling unrepresentable at the call site.
#[repr(transparent)]
#[derive(Clone, Copy)]
struct PreparedInterpreterCall(ResolvedByteCodeCallee);

/// Result of GNU `Bcall`'s single live function-cell read.
///
/// Keeping this closed prevents the bytecode call path from first probing one
/// target class and then re-resolving the same mutable symbol cell in a second
/// helper.  Every direct branch carries the exact live callee it classified;
/// aliases, autoloads, advice and compiler overrides remain on `Generic`.
#[derive(Clone, Copy)]
enum ResolvedStackCallTarget {
    Interpreter { call: PreparedInterpreterCall },
    ByteCode { callee: ResolvedByteCodeCallee },
    Builtin { callee: ResolvedBuiltinCallee },
    Generic,
}

#[derive(Clone, Copy)]
struct InterpreterFrameCleanup {
    condition_stack_base: usize,
    specpdl_base: usize,
}

/// Storage protocol for the Lisp value that keeps an executing bytecode
/// function alive.
///
/// Entry/recursive frames own a `Context::bc_frames` entry. Iterative children
/// instead reuse the caller's consumed function-designator operand. Keeping
/// release behind closed marker types makes popping the context stack for an
/// operand-rooted child impossible at the call site, with no runtime tag or
/// extra field in [`InterpreterFrame`].
trait BytecodeFrameRootStorage {
    fn release(ctx: &mut crate::emacs_core::eval::Context);
}

enum ContextBytecodeFrameRoot {}

impl BytecodeFrameRootStorage for ContextBytecodeFrameRoot {
    #[inline(always)]
    fn release(ctx: &mut crate::emacs_core::eval::Context) {
        ctx.bc_frames.pop();
    }
}

enum ConsumedCallOperandFrameRoot {}

impl BytecodeFrameRootStorage for ConsumedCallOperandFrameRoot {
    #[inline(always)]
    fn release(_ctx: &mut crate::emacs_core::eval::Context) {}
}

/// Suspended Tier-0 execution position, including the JIT OSR latch.
///
/// GNU's `bc_frame` saves one program-counter pointer. Neomacs also needs to
/// remember whether it already attempted OSR in that frame, but a live Rust
/// `Vec<Op>` index can never occupy usize's high bit: allocations are bounded
/// by `isize::MAX` bytes. Packing the latch there keeps the resume state one
/// word wide and makes frame copies match GNU's compact saved-register shape.
#[cfg(feature = "jit")]
#[repr(transparent)]
#[derive(Clone, Copy)]
struct InterpreterResumePoint(usize);

#[cfg(feature = "jit")]
impl InterpreterResumePoint {
    const OSR_TRIED_FLAG: usize = 1usize << (usize::BITS - 1);
    const PC_MASK: usize = !Self::OSR_TRIED_FLAG;

    #[inline(always)]
    fn new(pc: usize, osr_tried: bool) -> Self {
        debug_assert_eq!(
            pc & Self::OSR_TRIED_FLAG,
            0,
            "a live bytecode instruction index cannot occupy usize's high bit"
        );
        Self(pc | usize::from(osr_tried) * Self::OSR_TRIED_FLAG)
    }

    #[inline(always)]
    fn pc(self) -> usize {
        self.0 & Self::PC_MASK
    }

    #[inline(always)]
    fn osr_tried(self) -> bool {
        self.0 & Self::OSR_TRIED_FLAG != 0
    }
}

/// One-word immutable code handle for the function executed by a frame.
///
/// The entry pointer is borrowed for exactly the `run_loop` call. Every nested
/// frame's consumed caller operand owns the exact Lisp value and keeps its
/// pointer live. Bytecode arena slots are immovable, and published bytecode is
/// immutable in production, so the handle can retain the already checked
/// address instead of repeating tagged-object classification each time GNU's
/// `Breturn` resumes a caller. Whether a frame is the entry frame is represented
/// by the caller stack, not by a nullable code sentinel, so every frame has
/// valid code.
#[repr(transparent)]
#[derive(Clone, Copy)]
struct InterpreterFunction(std::ptr::NonNull<ByteCodeFunction>);

impl InterpreterFunction {
    fn new(code: &ByteCodeFunction) -> Self {
        Self(std::ptr::NonNull::from(code))
    }

    #[inline]
    fn code(&self) -> &ByteCodeFunction {
        // SAFETY: the entry borrow outlives `run_loop`; every nested frame's
        // consumed caller operand roots its immutable, immovable bytecode arena
        // slot.
        unsafe { self.0.as_ref() }
    }
}

/// All mutable Tier-0 state required to suspend and later resume one frame.
///
/// GNU stores the equivalent fields in `struct bc_frame` plus the register
/// locals saved by `Bcall`. Keeping them in one Rust value makes a recursive
/// interpreter entry unrepresentable on the iterative path: callers are moved
/// onto the driver stack and can only be resumed through `Breturn` handling.
#[derive(Clone, Copy)]
struct InterpreterFrame {
    function: InterpreterFunction,
    frame_base: usize,
    frame_limit: usize,
    #[cfg(feature = "jit")]
    resume: InterpreterResumePoint,
    #[cfg(not(feature = "jit"))]
    pc: usize,
    cleanup: InterpreterFrameCleanup,
    #[cfg(debug_assertions)]
    entry_lexenv: Value,
}

impl InterpreterFrame {
    #[inline(always)]
    fn pc(&self) -> usize {
        #[cfg(feature = "jit")]
        {
            self.resume.pc()
        }
        #[cfg(not(feature = "jit"))]
        {
            self.pc
        }
    }

    #[inline(always)]
    fn set_pc(&mut self, pc: usize) {
        #[cfg(feature = "jit")]
        {
            self.resume = InterpreterResumePoint::new(pc, self.resume.osr_tried());
        }
        #[cfg(not(feature = "jit"))]
        {
            self.pc = pc;
        }
    }

    #[inline(always)]
    fn save_execution_state(&mut self, pc: usize, osr_tried: bool) {
        #[cfg(feature = "jit")]
        {
            self.resume = InterpreterResumePoint::new(pc, osr_tried);
        }
        #[cfg(not(feature = "jit"))]
        {
            let _ = osr_tried;
            self.pc = pc;
        }
    }
}

/// Variable-sized state for the active frame at the matching driver depth.
///
/// Keeping this in a parallel stack lets suspended frames remain compact:
/// moving an `InterpreterFrame` at every Bcall/Breturn no longer copies the
/// inline storage of two `SmallVec`s. The auxiliary state stays at a stable
/// logical depth until that frame completes, like GNU's separate handler and
/// specpdl stacks.
struct InterpreterFrameAux {
    handlers: HandlerStack,
    bind_stack: BindStack,
}

impl InterpreterFrameAux {
    fn new(handlers: HandlerStack, bind_stack: BindStack) -> Self {
        Self {
            handlers,
            bind_stack,
        }
    }

    fn empty() -> Self {
        Self::new(HandlerStack::new(), BindStack::new())
    }

    fn is_empty(&self) -> bool {
        self.handlers.is_empty() && self.bind_stack.is_empty()
    }
}

/// Logical caller depth in the iterative interpreter driver.
///
/// A dedicated type prevents sparse auxiliary state from being restored with
/// an unrelated bytecode-buffer or specpdl index.  Depth zero denotes the
/// entry frame; a callee's current depth is the number of suspended callers.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct InterpreterDriverDepth(usize);

impl InterpreterDriverDepth {
    #[cfg(test)]
    const ROOT: Self = Self(0);

    fn from_suspended_callers(callers: usize) -> Self {
        Self(callers)
    }
}

struct SuspendedInterpreterFrameAux {
    depth: InterpreterDriverDepth,
    state: InterpreterFrameAux,
}

/// Variable-sized handler/bind state for the current driver frame.
///
/// GNU keeps handler/spec state outside `bc_frame`; ordinary calls therefore
/// do not copy an empty record.  Mirror that shape with one current state and
/// a sparse stack containing only suspended callers whose state is nonempty.
/// The common Bcall/Breturn path changes only `current`'s empty state and never
/// pushes the 96-byte pair of inline `SmallVec`s.
#[derive(Clone, Copy, Eq, PartialEq)]
enum InterpreterFrameAuxOccupancy {
    KnownEmpty,
    MayContainState,
}

struct InterpreterFrameAuxStack {
    current: InterpreterFrameAux,
    current_occupancy: InterpreterFrameAuxOccupancy,
    suspended: Vec<SuspendedInterpreterFrameAux>,
}

impl InterpreterFrameAuxStack {
    fn new(handlers: HandlerStack, bind_stack: BindStack) -> Self {
        let current = InterpreterFrameAux::new(handlers, bind_stack);
        let current_occupancy = if current.is_empty() {
            InterpreterFrameAuxOccupancy::KnownEmpty
        } else {
            InterpreterFrameAuxOccupancy::MayContainState
        };
        Self {
            current,
            current_occupancy,
            suspended: Vec::new(),
        }
    }

    fn current_mut(&mut self) -> &mut InterpreterFrameAux {
        // A mutable borrow can make either auxiliary stack nonempty.  Record
        // that possibility before lending it out; paths that subsequently
        // observe both stacks empty refine the state back to KnownEmpty.
        self.current_occupancy = InterpreterFrameAuxOccupancy::MayContainState;
        &mut self.current
    }

    fn suspend_current(&mut self, depth: InterpreterDriverDepth) {
        if self.current_occupancy == InterpreterFrameAuxOccupancy::KnownEmpty {
            return;
        }
        if self.current.is_empty() {
            self.current_occupancy = InterpreterFrameAuxOccupancy::KnownEmpty;
            return;
        }
        self.suspended.push(SuspendedInterpreterFrameAux {
            depth,
            state: std::mem::replace(&mut self.current, InterpreterFrameAux::empty()),
        });
        self.current_occupancy = InterpreterFrameAuxOccupancy::KnownEmpty;
    }

    fn restore_current(&mut self, depth: InterpreterDriverDepth) {
        if self
            .suspended
            .last()
            .is_some_and(|suspended| suspended.depth == depth)
        {
            self.current = self
                .suspended
                .pop()
                .expect("matching sparse auxiliary frame must exist")
                .state;
            self.current_occupancy = InterpreterFrameAuxOccupancy::MayContainState;
        } else {
            debug_assert!(
                self.suspended
                    .last()
                    .is_none_or(|suspended| suspended.depth < depth),
                "sparse auxiliary frames must remain ordered by driver depth"
            );
            // The ordinary Breturn path has neither current nor suspended
            // auxiliary state.  Keep that already-empty state in place: in
            // addition to avoiding two SmallVec resets on every return, this
            // preserves any reusable spilled storage left by an earlier
            // handler or dynamic binding in the same driver frame.
            if self.current_occupancy == InterpreterFrameAuxOccupancy::MayContainState {
                if !self.current.is_empty() {
                    self.current = InterpreterFrameAux::empty();
                }
                self.current_occupancy = InterpreterFrameAuxOccupancy::KnownEmpty;
            }
        }
    }

    fn take_entry(&mut self) -> (HandlerStack, BindStack) {
        debug_assert!(self.suspended.is_empty());
        let entry = std::mem::replace(&mut self.current, InterpreterFrameAux::empty());
        self.current_occupancy = InterpreterFrameAuxOccupancy::KnownEmpty;
        (entry.handlers, entry.bind_stack)
    }

    #[cfg(test)]
    fn materialized_suspended_len(&self) -> usize {
        self.suspended.len()
    }
}

/// A bytecode callee proven eligible for GNU's iterative `setup_frame` path.
///
/// `value` is the exact GC-visible Lisp identity installed in the consumed call
/// operand; `function` is the matching immutable code address already checked
/// by Bcall dispatch.
/// Keeping them inseparable prevents child-frame construction from decoding the
/// tagged value a second time or accidentally pairing code with another value.
#[derive(Clone, Copy)]
struct PreparedInterpreterCallee {
    value: Value,
    function: InterpreterFunction,
}

impl PreparedInterpreterCallee {
    fn new(value: Value, code: &ByteCodeFunction) -> Self {
        Self {
            value,
            function: InterpreterFunction::new(code),
        }
    }

    #[inline(always)]
    fn code(&self) -> &ByteCodeFunction {
        self.function.code()
    }
}

/// The caller stack slot consumed by one `Op::Call` as its function
/// designator.
///
/// `dispatch_interpreter_stack_call` first records GNU's user-visible
/// backtrace function, then an iterative entry replaces this dead operand with
/// the exact resolved bytecode object. The ordinary GC scan of `bc_buf` thereby
/// keeps the executing function alive even if Lisp redefines its symbol while
/// it is running. The private constructor prevents an argument slot from being
/// mistaken for the root slot.
#[repr(transparent)]
#[derive(Clone, Copy)]
struct ConsumedCallOperandRootSlot(usize);

impl ConsumedCallOperandRootSlot {
    #[inline(always)]
    fn from_args_start(args_start: usize) -> Self {
        Self(
            args_start
                .checked_sub(1)
                .expect("an iterative bytecode call must have a function operand"),
        )
    }

    #[inline(always)]
    fn args_start(self) -> usize {
        self.0 + 1
    }

    #[inline(always)]
    fn install_exact_callee(self, stack: &mut [Value], callee: Value) {
        stack[self.0] = callee;
    }
}

// These values are copied on every iterative Bcall/Breturn. Keep accidental
// enum/Option padding from silently turning frame transitions into bulk memory
// traffic again. The bounds include the debug-only lexenv invariant field.
const _: () = {
    assert!(std::mem::size_of::<InterpreterFunction>() == std::mem::size_of::<Value>());
    assert!(std::mem::size_of::<PreparedInterpreterCallee>() == 2 * std::mem::size_of::<Value>());
    assert!(std::mem::size_of::<InterpreterFrame>() <= 56);
    // A suspended caller costs a frame plus its continuation, in two parallel
    // stacks. Neither is ever COPIED any more -- a call writes the callee's
    // frame once and a return pops -- so these bound footprint, not per-call
    // traffic, which is the whole point of the split.
    assert!(std::mem::size_of::<BytecodeCallContinuation>() <= 16);
};

struct BytecodeCallContinuation {
    stack_after_call: usize,
    backtrace: BytecodeBacktraceFrame,
}

/// The interpreter's frame stack. **The ACTIVE frame is the last element** of
/// `frames`; everything below it is a suspended caller.
///
/// The active frame used to be a local in `run_loop`, with ONE slot reused for
/// every callee. That is why entering a call had to copy the caller out (48
/// bytes) and returning had to copy it back: the slot the caller lived in was
/// about to be overwritten. `perf annotate` on `bytecode-call-loop` put
/// **18.82% of the entire dispatch loop** on that single `ptr::write`.
///
/// Giving every frame its own slot removes both copies. A call writes the
/// callee's frame once, built from values already in registers; a return is a
/// pop. GNU gets the same property for free by recursing on the C stack
/// (`exec_byte_code` calling itself) -- at the cost of overflowing that stack
/// on deep Lisp recursion, which is exactly what this iterative driver exists
/// to prevent. This keeps the safety and drops the copies.
///
/// `continuations` is parallel and one shorter: a continuation exists for
/// exactly the SUSPENDED frames, so the active frame cannot carry a stale one
/// and nothing has to invent a placeholder for it. That matters because
/// `BytecodeBacktraceFrame` is `#[must_use]` and must be consumed by a matching
/// pop; a sentinel would be a lie the type is specifically built to prevent.
///
/// No `&mut` into either vector may be held across a push -- a reallocation
/// moves every slot. The driver re-derives the active frame at the head of each
/// `'frame` iteration, which is exactly where a push has just happened.
struct InterpreterCallerStack {
    frames: Vec<InterpreterFrame>,
    continuations: Vec<BytecodeCallContinuation>,
}

impl InterpreterCallerStack {
    fn new(entry: InterpreterFrame) -> Self {
        let mut frames = Vec::with_capacity(8);
        frames.push(entry);
        Self {
            frames,
            continuations: Vec::with_capacity(8),
        }
    }

    /// The frame currently executing.
    #[inline(always)]
    fn active(&self) -> &InterpreterFrame {
        let last = self.frames.len() - 1;
        // SAFETY: constructed with one frame; `leave_callee` refuses to pop the
        // last one, so the stack is never empty.
        unsafe { self.frames.get_unchecked(last) }
    }

    #[inline(always)]
    fn active_mut(&mut self) -> &mut InterpreterFrame {
        let last = self.frames.len() - 1;
        // SAFETY: as `active`.
        unsafe { self.frames.get_unchecked_mut(last) }
    }

    /// How many callers are suspended beneath the active frame.
    #[inline(always)]
    fn suspended_len(&self) -> usize {
        self.continuations.len()
    }

    /// Whether the active frame is the outermost one.
    #[inline(always)]
    fn has_no_suspended_callers(&self) -> bool {
        self.continuations.is_empty()
    }

    /// Suspend the active frame with `continuation` and make `callee` active.
    ///
    /// `callee` arrives BY VALUE, built from registers, so this is one write of
    /// a frame rather than a copy of one already in memory.
    #[inline(always)]
    fn enter_callee(&mut self, continuation: BytecodeCallContinuation, callee: InterpreterFrame) {
        self.continuations.push(continuation);
        self.frames.push(callee);
    }

    /// Discard the active frame and resume its caller, returning the
    /// continuation recorded when that caller suspended. `None` when the active
    /// frame is the outermost one, which is the driver's exit condition.
    #[inline(always)]
    fn leave_callee(&mut self) -> Option<BytecodeCallContinuation> {
        let continuation = self.continuations.pop()?;
        self.frames.pop();
        Some(continuation)
    }
}

enum InterpreterStackCall {
    Enter {
        callee: PreparedInterpreterCallee,
        root_slot: ConsumedCallOperandRootSlot,
        nargs: usize,
        backtrace: BytecodeBacktraceFrame,
    },
    Complete(EvalResult),
}

enum InterpreterFrameCompletion {
    Resume,
    Exit(EvalResult),
}

/// Result of GNU's ordinary `Breturn` transition.
///
/// A successful bytecode return is a tagged value, not a general Lisp control
/// transfer. Keeping that distinction in the type prevents the common path
/// from constructing and copying `Result<Value, Flow>` merely to discover that
/// no unwind is required. Only frames with outstanding dynamic state are sent
/// to the generic cleanup machinery.
enum InterpreterValueCompletion {
    Resume,
    Exit(Value),
    NeedsSlowCleanup(Value),
}

const _: () = {
    assert!(std::mem::size_of::<InterpreterValueCompletion>() <= 16);
};

impl ResolvedByteCodeCallee {
    #[inline(always)]
    fn from_live_function_cell(value: Value) -> Option<Self> {
        (value.veclike_type() == Some(VecLikeType::ByteCode)).then_some(Self(value))
    }

    #[inline(always)]
    fn from_direct_value(value: Value) -> Self {
        debug_assert_eq!(value.veclike_type(), Some(VecLikeType::ByteCode));
        Self(value)
    }

    #[inline(always)]
    fn value(self) -> Value {
        self.0
    }

    /// Project the immutable code address carried by this proof token.
    ///
    /// The constructors prove TYPE only; the projection goes through the
    /// `get_bytecode_data` chokepoint, which is where lazy pdump stubs
    /// materialize — a token may be minted from a cache replay long before
    /// its function is first run, so the proof of materialization belongs at
    /// the projection, not the mint. Bytecode arena objects are immovable,
    /// and the live function cell or direct caller-stack value keeps the
    /// object rooted while this borrow is used.
    #[inline(always)]
    fn code(&self) -> &ByteCodeFunction {
        // Type proven at mint; the chokepoint-resident projection carries the
        // (future) stub-materialization check without re-classifying.
        self.0.bytecode_data_typechecked_by_caller()
    }
}

const _: () =
    assert!(std::mem::size_of::<ResolvedByteCodeCallee>() == std::mem::size_of::<Value>());

impl PreparedInterpreterCall {
    #[inline(always)]
    fn new(callee: ResolvedByteCodeCallee) -> Self {
        Self(callee)
    }

    #[inline(always)]
    fn callee(self) -> ResolvedByteCodeCallee {
        self.0
    }
}

const _: () =
    assert!(std::mem::size_of::<PreparedInterpreterCall>() == std::mem::size_of::<Value>());

impl ResolvedBuiltinCallee {
    #[inline]
    fn from_static_symbol(sym_id: SymId) -> Option<Self> {
        lookup_global_subr_entry(sym_id)
            .is_some_and(|entry| entry.dispatch_kind == SubrDispatchKind::Builtin)
            .then_some(Self(Value::from_sym_id(sym_id)))
    }

    #[inline(always)]
    fn from_subr_value(value: Value) -> Option<Self> {
        if value.veclike_type() != Some(VecLikeType::Subr) {
            return None;
        }
        let ptr = value.as_veclike_ptr()? as *const SubrObj;
        // SAFETY: the veclike type check above proves this points to a live
        // SubrObj. This reads only intrinsic GNU Lisp_Subr metadata; no Lisp
        // runs between classification and dispatch.
        let subr = unsafe { &*ptr };
        (subr.dispatch_kind == SubrDispatchKind::Builtin && subr.function.is_some())
            .then_some(Self(value))
    }

    #[inline]
    fn entry(self) -> (SymId, SubrEntry) {
        if let Some(sym_id) = self.0.as_symbol_id() {
            let entry = lookup_global_subr_entry(sym_id)
                .expect("resolved static builtin must retain its registered entry");
            debug_assert_eq!(entry.dispatch_kind, SubrDispatchKind::Builtin);
            (sym_id, entry)
        } else {
            let (sym_id, entry) = subr_entry_from_value(self.0)
                .expect("resolved builtin object must remain a valid subr");
            debug_assert_eq!(entry.dispatch_kind, SubrDispatchKind::Builtin);
            (sym_id, entry)
        }
    }

    #[inline]
    fn wrong_arity_value(self) -> Value {
        if let Some(sym_id) = self.0.as_symbol_id() {
            Value::subr_from_sym_id(sym_id)
        } else {
            self.0
        }
    }
}

const _: () = assert!(std::mem::size_of::<ResolvedBuiltinCallee>() == std::mem::size_of::<Value>());

/// Debug check for env-less bytecode frames: after the frame body runs,
/// `ctx.lexenv` must be the entry lexenv, possibly EXTENDED by value-less
/// `(defvar x)` markers consed on by the tree interpreter (sf_defvar reached
/// through opcodes that eval forms). Those markers legitimately persist past
/// the frame boundary (GNU behavior: the symbol stays special for the rest of
/// the enclosing scope), so the invariant is tail-reachability, not equality.
#[cfg(debug_assertions)]
fn lexenv_tail_reachable(current: Value, entry: Value) -> bool {
    let mut cursor = current;
    // Bounded walk: defvar markers within one frame are few; a long walk
    // means the invariant is broken anyway.
    for _ in 0..10_000 {
        if cursor.bits() == entry.bits() {
            return true;
        }
        if !cursor.is_cons() {
            return false;
        }
        cursor = cursor.cons_cdr();
    }
    false
}

#[inline(always)]
fn fixnum_tagged_i64(value: Value) -> i64 {
    debug_assert!(value.is_fixnum());
    // GNU bytecode.c compares XFIXNUM values for fixnum comparison opcodes.
    // Neomacs fixnums are `(n << 2) | 2`, so the signed tagged bits preserve
    // the same total order without materializing the untagged integer.
    value.bits() as i64
}

#[inline(always)]
fn fixnum_lt(left: Value, right: Value) -> bool {
    fixnum_tagged_i64(left) < fixnum_tagged_i64(right)
}

#[inline(always)]
fn fixnum_gt(left: Value, right: Value) -> bool {
    fixnum_tagged_i64(left) > fixnum_tagged_i64(right)
}

#[inline(always)]
fn fixnum_le(left: Value, right: Value) -> bool {
    fixnum_tagged_i64(left) <= fixnum_tagged_i64(right)
}

#[inline(always)]
fn fixnum_ge(left: Value, right: Value) -> bool {
    fixnum_tagged_i64(left) >= fixnum_tagged_i64(right)
}

#[inline]
fn plus_sym_id() -> SymId {
    static PLUS: OnceLock<SymId> = OnceLock::new();
    *PLUS.get_or_init(|| intern("+"))
}

#[inline]
fn logand_sym_id() -> SymId {
    static LOGAND: OnceLock<SymId> = OnceLock::new();
    *LOGAND.get_or_init(|| intern("logand"))
}

#[inline]
fn logior_sym_id() -> SymId {
    static LOGIOR: OnceLock<SymId> = OnceLock::new();
    *LOGIOR.get_or_init(|| intern("logior"))
}

#[inline]
fn logxor_sym_id() -> SymId {
    static LOGXOR: OnceLock<SymId> = OnceLock::new();
    *LOGXOR.get_or_init(|| intern("logxor"))
}

#[inline]
fn fillarray_sym_id() -> SymId {
    static FILLARRAY: OnceLock<SymId> = OnceLock::new();
    *FILLARRAY.get_or_init(|| intern("fillarray"))
}

#[inline]
fn aset_sym_id() -> SymId {
    static ASET: OnceLock<SymId> = OnceLock::new();
    *ASET.get_or_init(|| intern("aset"))
}

/// A tiny direct-mapped cache for symbol function cells proven to contain
/// bytecode. It lives only for one [`Vm`] invocation: it is neither a Lisp/GC
/// root nor persisted in pdump state.
///
/// A cached callee remains rooted by the same live function cell that supplied
/// it. Every function-cell mutation advances `Obarray::function_epoch`; an
/// epoch mismatch makes the old bits unreachable before they can be used.
/// `u64::MAX` is reserved by the epoch implementation, so it is an
/// unambiguous empty-entry marker without `Option` padding.
const SYMBOL_BYTECODE_CALL_CACHE_CAPACITY: usize = 8;
const EMPTY_FUNCTION_EPOCH: u64 = u64::MAX;

#[derive(Clone, Copy)]
struct SymbolByteCodeCallCacheEntry {
    function_epoch: u64,
    /// Raw classified callee. Populated ONLY through the typed
    /// `insert_bytecode`/`insert_builtin` proofs, whose value kinds are
    /// disjoint: a bytecode callee is always a ByteCode veclike, a builtin
    /// callee is always a subr object or the static-table symbol fallback.
    /// `get` maps the stored value back to its wrapper class by that kind.
    callee: Value,
    symbol: SymId,
}

impl SymbolByteCodeCallCacheEntry {
    const EMPTY: Self = Self {
        function_epoch: EMPTY_FUNCTION_EPOCH,
        callee: Value::NIL,
        symbol: crate::emacs_core::intern::NIL_SYM_ID,
    };
}

struct SymbolByteCodeCallCache {
    entries: [SymbolByteCodeCallCacheEntry; SYMBOL_BYTECODE_CALL_CACHE_CAPACITY],
}

/// The most recently proven symbol-bound Tier-0 call.
///
/// Real bytecode is strongly monomorphic at an individual call site.  This
/// one-entry cache sits in front of the wider symbol cache so the ordinary
/// repeated call can compare the raw tagged designator before decoding its
/// kind or hashing its `SymId`.  `function_epoch` invalidates every function
/// cell mutation, while `nargs` keeps the `setup_frame` proof exact.
#[derive(Clone, Copy)]
struct RecentInterpreterCall {
    function_epoch: u64,
    designator: Value,
    call: PreparedInterpreterCall,
    nargs: u16,
}

impl RecentInterpreterCall {
    const EMPTY: Self = Self {
        function_epoch: EMPTY_FUNCTION_EPOCH,
        designator: Value::NIL,
        call: PreparedInterpreterCall(ResolvedByteCodeCallee(Value::NIL)),
        nargs: 0,
    };

    #[inline(always)]
    fn get(
        self,
        designator: Value,
        nargs: usize,
        function_epoch: u64,
    ) -> Option<PreparedInterpreterCall> {
        (self.function_epoch == function_epoch
            && self.designator.bits() == designator.bits()
            && usize::from(self.nargs) == nargs)
            .then_some(self.call)
    }

    #[inline(always)]
    fn replace(
        &mut self,
        designator: Value,
        nargs: usize,
        function_epoch: u64,
        call: PreparedInterpreterCall,
    ) {
        let Ok(nargs) = u16::try_from(nargs) else {
            return;
        };
        *self = Self {
            function_epoch,
            designator,
            call,
            nargs,
        };
    }
}

impl SymbolByteCodeCallCache {
    const fn new() -> Self {
        Self {
            entries: [SymbolByteCodeCallCacheEntry::EMPTY; SYMBOL_BYTECODE_CALL_CACHE_CAPACITY],
        }
    }

    #[inline(always)]
    const fn index(symbol: SymId) -> usize {
        symbol.0 as usize & (SYMBOL_BYTECODE_CALL_CACHE_CAPACITY - 1)
    }

    #[inline(always)]
    fn get(&self, symbol: SymId, function_epoch: u64) -> Option<ResolvedStackCallTarget> {
        let entry = self.entries[Self::index(symbol)];
        if entry.function_epoch != function_epoch || entry.symbol != symbol {
            return None;
        }
        // Insert-side value kinds are disjoint (see the `callee` field doc),
        // so one kind test recovers the wrapper class proven at insert.
        Some(
            if matches!(
                entry.callee.kind(),
                ValueKind::Veclike(VecLikeType::ByteCode)
            ) {
                ResolvedStackCallTarget::ByteCode {
                    callee: ResolvedByteCodeCallee(entry.callee),
                }
            } else {
                ResolvedStackCallTarget::Builtin {
                    callee: ResolvedBuiltinCallee(entry.callee),
                }
            },
        )
    }

    #[inline(always)]
    fn insert_bytecode(
        &mut self,
        symbol: SymId,
        function_epoch: u64,
        callee: ResolvedByteCodeCallee,
    ) {
        self.store(symbol, function_epoch, callee.0);
    }

    #[inline(always)]
    fn insert_builtin(
        &mut self,
        symbol: SymId,
        function_epoch: u64,
        callee: ResolvedBuiltinCallee,
    ) {
        self.store(symbol, function_epoch, callee.0);
    }

    #[inline(always)]
    fn store(&mut self, symbol: SymId, function_epoch: u64, callee: Value) {
        self.entries[Self::index(symbol)] = SymbolByteCodeCallCacheEntry {
            function_epoch,
            callee,
            symbol,
        };
    }
}

const _: () = {
    assert!(SYMBOL_BYTECODE_CALL_CACHE_CAPACITY.is_power_of_two());
    assert!(std::mem::size_of::<SymbolByteCodeCallCacheEntry>() == 3 * std::mem::size_of::<u64>());
    assert!(std::mem::size_of::<RecentInterpreterCall>() <= 4 * std::mem::size_of::<u64>());
};

/// Process-selected execution policy for bytecode calls in this VM.
///
/// `NEOVM_JIT=0` means Tier 0 is the only reachable tier.  Encoding that once
/// when the VM is constructed lets GNU's Bcall-shaped hot path skip both call-
/// site feedback and the tier dispatcher; an interpreter-only run cannot
/// accidentally pay for adaptive-tier state through a forgotten call site.
#[cfg(feature = "jit")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BytecodeTierPolicy {
    InterpreterOnly,
    Adaptive,
}

#[cfg(feature = "jit")]
impl BytecodeTierPolicy {
    fn for_process() -> Self {
        if crate::emacs_core::jit::jit_runtime_enabled() {
            Self::Adaptive
        } else {
            Self::InterpreterOnly
        }
    }

    #[inline(always)]
    fn records_call_feedback(self) -> bool {
        matches!(self, Self::Adaptive)
    }
}

/// The bytecode VM execution engine.
///
/// Operates on an Context's obarray and dynamic binding stack.
pub struct Vm<'a> {
    ctx: &'a mut crate::emacs_core::eval::Context,
    symbol_bytecode_call_cache: SymbolByteCodeCallCache,
    recent_interpreter_call: RecentInterpreterCall,
    #[cfg(feature = "jit")]
    bytecode_tier_policy: BytecodeTierPolicy,
    /// Isolation knobs, read once per VM (see `jit::jit_bcall_tier_skipped` /
    /// `jit::jit_bcall_cache_forced`); a field load on the hot path, never an
    /// env read.
    #[cfg(feature = "jit")]
    bcall_tier_skipped: bool,
    #[cfg(feature = "jit")]
    bcall_cache_forced: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FrameArgumentCopy {
    None,
    One,
    Scalar,
    Bulk,
}

impl FrameArgumentCopy {
    /// GNU's bytecode `setup_frame` pushes arguments one word at a time.  That
    /// wins for ordinary Lisp arities because a libc `memmove` dispatch costs
    /// more than a handful of already-capacity-checked stores.  Retain the
    /// bulk path for unusually wide generated functions.
    const fn for_count(count: usize) -> Self {
        const SCALAR_COPY_MAX: usize = 8;
        match count {
            0 => Self::None,
            1 => Self::One,
            2..=SCALAR_COPY_MAX => Self::Scalar,
            _ => Self::Bulk,
        }
    }
}

#[cfg(test)]
thread_local! {
    static FRAME_ARGUMENT_COPY_COUNTS: std::cell::Cell<(usize, usize)> =
        const { std::cell::Cell::new((0, 0)) };
}

#[cfg(test)]
fn reset_frame_argument_copy_counts() {
    FRAME_ARGUMENT_COPY_COUNTS.set((0, 0));
}

#[cfg(test)]
fn frame_argument_copy_counts() -> (usize, usize) {
    FRAME_ARGUMENT_COPY_COUNTS.get()
}

#[inline(always)]
fn copy_frame_arguments(buffer: &mut Vec<Value>, args_start: usize, copied: usize) {
    let strategy = FrameArgumentCopy::for_count(copied);
    #[cfg(test)]
    FRAME_ARGUMENT_COPY_COUNTS.with(|counts| {
        let (scalar, bulk) = counts.get();
        counts.set(match strategy {
            FrameArgumentCopy::None => (scalar, bulk),
            FrameArgumentCopy::One => (scalar + 1, bulk),
            FrameArgumentCopy::Scalar => (scalar + 1, bulk),
            FrameArgumentCopy::Bulk => (scalar, bulk + 1),
        });
    });
    match strategy {
        FrameArgumentCopy::None => {}
        FrameArgumentCopy::One => {
            let value = buffer[args_start];
            buffer.push(value);
        }
        FrameArgumentCopy::Scalar => {
            for offset in 0..copied {
                let value = buffer[args_start + offset];
                buffer.push(value);
            }
        }
        FrameArgumentCopy::Bulk => {
            buffer.extend_from_within(args_start..args_start + copied);
        }
    }
}

// Match the evaluator's coarse stack-growth policy so deeply recursive
// bytecode/macroexpansion paths don't exhaust the native thread stack before
// `max-lisp-eval-depth` handling can fire.
const VM_STACK_RED_ZONE: usize = 128 * 1024;
const VM_STACK_SEGMENT: usize = 2 * 1024 * 1024;
const VM_STACK_GROWTH_PROBE_START_DEPTH: usize = 16;
const VM_STACK_GROWTH_PROBE_INTERVAL: usize = 16;

impl<'a> crate::emacs_core::hook_runtime::HookRuntime for Vm<'a> {
    fn hook_context(&self) -> &crate::emacs_core::eval::Context {
        self.ctx
    }

    fn call_hook_callable(&mut self, function: Value, args: &[Value]) -> EvalResult {
        self.call_function_with_roots(function, args)
    }

    fn report_safe_hook_error(
        &mut self,
        hook_sym: SymId,
        function: Value,
        signal: &crate::emacs_core::error::SignalData,
    ) -> EvalResult {
        crate::emacs_core::hook_runtime::HookRuntime::report_safe_hook_error(
            &mut *self.ctx,
            hook_sym,
            function,
            signal,
        )
    }

    fn remove_hook_function_after_error(&mut self, hook_sym: SymId, function: Value) {
        crate::emacs_core::hook_runtime::HookRuntime::remove_hook_function_after_error(
            &mut *self.ctx,
            hook_sym,
            function,
        );
    }

    fn with_hook_root_scope<T>(
        &mut self,
        f: impl FnOnce(&mut Self) -> Result<T, Flow>,
    ) -> Result<T, Flow> {
        self.with_dynamic_vm_roots(|vm| f(vm))
    }

    fn push_hook_root(&mut self, value: Value) {
        self.push_dynamic_vm_root(value);
    }
}

impl<'a> Vm<'a> {
    pub(crate) fn from_context(ctx: &'a mut crate::emacs_core::eval::Context) -> Self {
        Self {
            ctx,
            symbol_bytecode_call_cache: SymbolByteCodeCallCache::new(),
            recent_interpreter_call: RecentInterpreterCall::EMPTY,
            #[cfg(feature = "jit")]
            bytecode_tier_policy: BytecodeTierPolicy::for_process(),
            #[cfg(feature = "jit")]
            bcall_tier_skipped: crate::emacs_core::jit::jit_bcall_tier_skipped(),
            #[cfg(feature = "jit")]
            bcall_cache_forced: crate::emacs_core::jit::jit_bcall_cache_forced(),
        }
    }

    #[cfg(all(test, feature = "jit"))]
    fn force_interpreter_only_for_test(&mut self) {
        self.bytecode_tier_policy = BytecodeTierPolicy::InterpreterOnly;
    }

    /// Truncate the bytecode operand stack to `len` — used by the JIT call shim
    /// to remove the arguments it pushed onto `bc_buf` for the fast call path,
    /// on every exit (success or signal), keeping the push/truncate symmetric.
    #[cfg(feature = "jit")]
    pub(crate) fn bc_buf_truncate(&mut self, len: usize) {
        self.ctx.bc_buf.truncate(len);
    }

    /// Set the current depth and max_depth (inherited from the Context).
    pub fn set_depth(&mut self, depth: usize, max_depth: usize) {
        self.ctx.depth = depth;
        self.ctx.max_depth = max_depth;
    }

    /// Get the current depth (to sync back to the Context).
    pub fn get_depth(&self) -> usize {
        self.ctx.depth
    }

    #[inline(always)]
    fn with_dynamic_vm_roots<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        let scope = self.ctx.save_vm_roots();
        let result = f(self);
        self.ctx.restore_vm_roots(scope);
        result
    }

    fn with_bytecode_call_depth<T>(
        &mut self,
        f: impl FnOnce(&mut Self) -> Result<T, Flow>,
    ) -> Result<T, Flow> {
        self.enter_bytecode_call_depth()?;
        let result = f(self);
        self.leave_bytecode_call_depth();
        result
    }

    #[inline(always)]
    fn enter_bytecode_call_depth(&mut self) -> Result<(), Flow> {
        self.ctx.depth += 1;
        if self.ctx.depth > self.ctx.max_depth {
            // Cold: the floor-raise + error construction stay out of the hot
            // prologue's codegen; the common shallow call pays one compare.
            if let Err(flow) = self.bytecode_depth_exceeded() {
                self.ctx.depth -= 1;
                return Err(flow);
            }
        }
        Ok(())
    }

    #[inline(always)]
    fn leave_bytecode_call_depth(&mut self) {
        debug_assert!(self.ctx.depth > 0);
        self.ctx.depth -= 1;
    }

    /// Cold arm of [`Vm::with_bytecode_call_depth`]: GNU raises the effective
    /// floor to 100 before signaling, so a pathologically small
    /// max-lisp-eval-depth still leaves room to run the error handler.
    #[cold]
    #[inline(never)]
    fn bytecode_depth_exceeded(&mut self) -> Result<(), Flow> {
        if self.ctx.max_depth < 100 {
            self.ctx.max_depth = 100;
        }
        if self.ctx.depth > self.ctx.max_depth {
            return Err(signal(
                "error",
                vec![Value::string("Lisp nesting exceeds ‘max-lisp-eval-depth’")],
            ));
        }
        Ok(())
    }

    #[inline(always)]
    fn with_vm_root_scope<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        let scope = self.ctx.save_vm_roots();
        let result = f(self);
        self.ctx.restore_vm_roots(scope);
        result
    }

    #[inline(always)]
    fn push_dynamic_vm_root(&mut self, value: Value) {
        self.ctx.push_vm_frame_root(value);
    }

    fn cleanup_bytecode_frame(
        &mut self,
        result: EvalResult,
        condition_stack_base: usize,
        specpdl_base: usize,
        frame_base: usize,
    ) -> EvalResult {
        self.cleanup_bytecode_frame_with_root::<ContextBytecodeFrameRoot>(
            result,
            condition_stack_base,
            specpdl_base,
            frame_base,
        )
    }

    fn cleanup_iterative_bytecode_frame(
        &mut self,
        result: EvalResult,
        condition_stack_base: usize,
        specpdl_base: usize,
        frame_base: usize,
    ) -> EvalResult {
        self.cleanup_bytecode_frame_with_root::<ConsumedCallOperandFrameRoot>(
            result,
            condition_stack_base,
            specpdl_base,
            frame_base,
        )
    }

    fn cleanup_bytecode_frame_with_root<Root: BytecodeFrameRootStorage>(
        &mut self,
        result: EvalResult,
        condition_stack_base: usize,
        specpdl_base: usize,
        frame_base: usize,
    ) -> EvalResult {
        #[cfg(test)]
        GENERIC_BYTECODE_CLEANUP_COUNT.with(|count| count.set(count.get() + 1));

        // GNU bytecode.c keeps a bytecode return value in `TOP` while unwinding
        // back to the caller. Neomacs uses recursive Rust frames, so the result
        // must be rooted only while this frame runs an operation that can GC.
        //
        // Dropping this frame's condition handlers is one such teardown step but
        // cannot GC: truncate_condition_stack is a plain Vec truncate over
        // ConditionFrame (no Drop), so run it unconditionally first, outside any
        // root scope.
        self.ctx.truncate_condition_stack(condition_stack_base);
        // unbind_to (unwind-protect bodies / binding restores) is then the ONLY
        // remaining step that can GC; bc_buf.truncate and release of either
        // root-storage kind merely drop Copy stack slots and hit no safe point.
        // When the frame left no
        // dynamic binds (the common lexical case — args and locals live on the
        // operand stack, not specpdl; a backtrace frame from the caller sits
        // below this frame's specpdl_base), unbind_to would only re-run its
        // fixed profiler_poll / quit-flag preamble over an empty span, nothing
        // can GC, and rooting the result is pure overhead. The result is
        // returned un-rooted in both paths (the caller re-roots it), so skipping
        // the root keeps the same post-return contract.
        if self.ctx.specpdl.len() == specpdl_base {
            self.ctx.bc_buf.truncate(frame_base);
            Root::release(self.ctx);
            return result;
        }
        // Closure fast path: an env=Some frame's sole outstanding entry is
        // its own prologue LexicalEnv save. Popping it is unbind_to's exact
        // restore — a pure `ctx.lexenv = old` assignment (no GC, no watchers,
        // no allocation; see the SpecBinding::LexicalEnv arm of
        // unbind_to_result) — so the result needs no rooting here either.
        // Every closure call (mapcar lambdas and friends) returns through
        // this instead of the save-roots/unbind/restore machinery.
        if self.ctx.specpdl.len() == specpdl_base + 1
            && matches!(
                self.ctx.specpdl.last(),
                Some(crate::emacs_core::eval::SpecBinding::LexicalEnv { .. })
            )
        {
            if let Some(crate::emacs_core::eval::SpecBinding::LexicalEnv { old_lexenv }) =
                self.ctx.specpdl.pop()
            {
                self.ctx.lexenv = old_lexenv;
            }
            self.ctx.bc_buf.truncate(frame_base);
            Root::release(self.ctx);
            return result;
        }
        let result = self.ctx.unbind_to_with_result(specpdl_base, result);
        self.ctx.bc_buf.truncate(frame_base);
        Root::release(self.ctx);
        result
    }

    fn with_frame_roots<T>(
        &mut self,
        _func: &ByteCodeFunction,
        extra: &[Value],
        f: impl FnOnce(&mut Self) -> T,
    ) -> T {
        self.with_dynamic_vm_roots(|vm| {
            // The active bytecode frame already roots its constants for the
            // whole invocation; only transient values removed from bc_buf need
            // an explicit root while a nested call can GC.
            for value in extra.iter().copied() {
                vm.ctx.push_vm_frame_root(value);
            }
            f(vm)
        })
    }

    fn with_frame_arg_roots<A, T>(
        &mut self,
        func: &ByteCodeFunction,
        args: A,
        f: impl FnOnce(&mut Self, A) -> T,
    ) -> T
    where
        A: AsRef<[Value]>,
    {
        self.with_frame_roots(func, &[], |vm| {
            for value in args.as_ref().iter().copied() {
                vm.ctx.push_vm_frame_root(value);
            }
            f(vm, args)
        })
    }

    fn with_frame_call_roots<A, T>(
        &mut self,
        func: &ByteCodeFunction,
        function: Value,
        args: A,
        f: impl FnOnce(&mut Self, A) -> T,
    ) -> T
    where
        A: AsRef<[Value]>,
    {
        self.with_frame_roots(func, &[], |vm| {
            vm.ctx.push_vm_frame_root(function);
            for value in args.as_ref().iter().copied() {
                vm.ctx.push_vm_frame_root(value);
            }
            f(vm, args)
        })
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn with_macro_expansion_scope(
        &mut self,
        f: impl FnOnce(&mut Self) -> EvalResult,
    ) -> EvalResult {
        let state = self.ctx.begin_macro_expansion_scope()?;
        let result = f(self);
        self.ctx.finish_macro_expansion_scope(state, result)
    }

    fn collect_flow_roots(flow: &Flow, out: &mut Vec<Value>) {
        match flow {
            Flow::Signal(sig) => {
                out.push(Value::from_sym_id(sig.symbol));
                out.extend(sig.data.iter().copied());
                if let Some(raw) = sig.raw_data {
                    out.push(raw);
                }
            }
            Flow::Throw(thrown) => {
                out.push(thrown.tag);
                out.push(thrown.value);
            }
            Flow::ThreadBlocked(blocked) => {
                out.push(blocked.blocker);
                out.push(blocked.remaining_forms);
            }
            // Carries only an exit code and a restart flag: no Lisp values to
            // keep alive.
            Flow::Shutdown(_) => {}
        }
    }

    /// Execute a bytecode function with given arguments.
    pub(crate) fn execute(&mut self, func: &ByteCodeFunction, args: Vec<Value>) -> EvalResult {
        // Suite chunks are often hand-assembled and passed here directly,
        // bypassing `Value::make_bytecode`'s test-only sealing; normalize a
        // clone so they may enter the unchecked-fetch driver. Production
        // sealing remains exclusive to the decode installers.
        #[cfg(test)]
        if !func.executes_sealed_ops() {
            let mut sealed = func.clone();
            sealed.seal_hand_assembled_ops_for_test();
            return self.execute_with_func_value(&sealed, args, Value::NIL);
        }
        self.execute_with_func_value(func, args, Value::NIL)
    }

    /// Execute a bytecode function, passing through the original function
    /// value for use in `wrong-number-of-arguments` error reporting.
    ///
    /// Owned-args wrapper over [`Vm::execute_from_stack_args`]: pushes the
    /// args onto the GC-traced `bc_buf` tail (rooting them for the whole
    /// call) and truncates back on every exit, preserving the JIT call
    /// shim's push/truncate symmetry. The hot bytecode→bytecode path skips
    /// this wrapper entirely — its args already live on `bc_buf`.
    pub(crate) fn execute_with_func_value(
        &mut self,
        func: &ByteCodeFunction,
        args: impl Into<LispArgVec>,
        func_value: Value,
    ) -> EvalResult {
        let args = args.into();
        let args_start = self.ctx.bc_buf.len();
        self.ctx.bc_buf.extend_from_slice(&args);
        let result = self.execute_from_stack_args(func, args_start, args.len(), func_value);
        self.ctx.bc_buf.truncate(args_start);
        result
    }

    /// Execute a bytecode function whose arguments live on `bc_buf` at
    /// `[args_start, args_start + nargs)` — the hot entry for
    /// bytecode→bytecode calls (the caller's `Op::Call` already left the
    /// args there; no `LispArgVec`, no per-arg rooting).
    ///
    /// Root the executing function across nested calls that can GC. A heap
    /// func_value is the frame-held function object (GNU fp->fun,
    /// bytecode.c setup_frame): run_frame's own BcFrame { base, fun } push
    /// — visited by trace_roots — is the sole root, transitively marking
    /// the constants vector (the GC traces ByteCodeObj.data.constants, and
    /// post-publish bytecode is immutable). Every caller derives `func` by
    /// dereferencing this same ByteCode object, so no separate per-call
    /// root scope is needed at all.
    ///
    /// WINDOW INVARIANT (load-bearing): nothing between this function's
    /// entry and run_frame's bc_frames.push may allocate a Lisp object or
    /// hit a GC safe point — today that window is only the stacker probe
    /// (native mmap, not Lisp alloc) and run_frame's two len reads. The
    /// debug assertion below enforces it.
    ///
    /// Only the direct/manual path (func_value == NIL/non-heap, e.g.
    /// `execute()`) holds nothing else alive, so it keeps a vm-root scope
    /// rooting each constant individually (trace_roots skips non-heap
    /// BcFrame.fun).
    pub(crate) fn execute_from_stack_args(
        &mut self,
        func: &ByteCodeFunction,
        args_start: usize,
        nargs: usize,
        func_value: Value,
    ) -> EvalResult {
        // Flattened native-stack probe: the common shallow call pays two
        // integer compares straight through to the body — no FnOnce
        // combinator whose two consumption sites (fast path + the stacker
        // closure) forced a memory-materialized closure environment on
        // every call. Only every 16th depth level from 16 up takes the cold
        // stacker path (INTERVAL is a power of two — the is_multiple_of
        // folds to a mask).
        let depth = self.ctx.depth;
        if depth >= VM_STACK_GROWTH_PROBE_START_DEPTH
            && depth.is_multiple_of(VM_STACK_GROWTH_PROBE_INTERVAL)
        {
            return self.execute_from_stack_args_grown(func, args_start, nargs, func_value);
        }
        self.execute_from_stack_args_body(func, args_start, nargs, func_value)
    }

    /// Cold stacker arm of [`Vm::execute_from_stack_args`]: grow the native
    /// stack segment if the red zone is near, then run the body inside it.
    #[cold]
    #[inline(never)]
    fn execute_from_stack_args_grown(
        &mut self,
        func: &ByteCodeFunction,
        args_start: usize,
        nargs: usize,
        func_value: Value,
    ) -> EvalResult {
        stacker::maybe_grow(VM_STACK_RED_ZONE, VM_STACK_SEGMENT, || {
            self.execute_from_stack_args_body(func, args_start, nargs, func_value)
        })
    }

    #[inline]
    fn execute_from_stack_args_body(
        &mut self,
        func: &ByteCodeFunction,
        args_start: usize,
        nargs: usize,
        func_value: Value,
    ) -> EvalResult {
        #[cfg(debug_assertions)]
        let gc_cycles_at_entry = crate::emacs_core::gc_stats::snapshot().collections;
        if func_value.is_heap_object() {
            #[cfg(debug_assertions)]
            debug_assert_eq!(
                crate::emacs_core::gc_stats::snapshot().collections,
                gc_cycles_at_entry,
                "GC ran between execute_from_stack_args entry and run_frame \
                 — the BcFrame.fun rooting window invariant is broken"
            );
            self.run_frame(func, args_start, nargs, func_value)
        } else {
            self.with_dynamic_vm_roots(|vm| {
                for value in func.constants.iter().copied() {
                    vm.push_dynamic_vm_root(value);
                }
                vm.run_frame(func, args_start, nargs, func_value)
            })
        }
    }

    /// Resume a bytecode frame MID-FUNCTION after a precise JIT deopt: a
    /// native guard failed at `start_pc` with the live operand stack `stack`,
    /// `handlers_active` condition frames registered by this frame still on
    /// `ctx.condition_stack`, and `bind_entries` (pre-push specpdl depths,
    /// drained from the JIT bind-stack segment) as the frame's outstanding
    /// dynamic binds. Ownership of those binds/handlers transfers here: the
    /// native caller performed NO frame unwind, and this frame's cleanup uses
    /// the native frame's entry bases (`specpdl_base`/`condition_stack_base`)
    /// so every exit unwinds exactly like the original frame would have.
    ///
    /// lexenv note: deliberately NOT the run_frame LexicalEnv prologue — the
    /// native frame never switched lexenv, and the only compilable op that
    /// reads it (UnwindProtectPop) uses the identical `ctx.lexenv` expression
    /// in its shim and interpreter arm, so resumed ops behave exactly as the
    /// remaining native ops would have.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn run_resumed_frame(
        &mut self,
        func: &ByteCodeFunction,
        func_value: Value,
        start_pc: usize,
        stack: &[Value],
        handlers_active: usize,
        bind_entries: &[usize],
        specpdl_base: usize,
        condition_stack_base: usize,
    ) -> EvalResult {
        let frame_base = self.ctx.bc_buf.len();
        // Native (JIT) catch/condition-case handlers transferred from the deopted
        // frame recorded their `stack_len` frame-RELATIVE (a native frame keeps no
        // operands on bc_buf). The operands are about to be seeded at
        // bc_buf[frame_base..], so rebase those handlers to ABSOLUTE bc_buf
        // positions — otherwise a throw/signal caught by one would truncate bc_buf
        // to the relative length and collapse the caller's operand stack.
        self.ctx
            .rebase_resumed_vm_handler_stack_lens(handlers_active, frame_base);
        self.ctx.bc_frames.push(crate::emacs_core::eval::BcFrame {
            base: frame_base,
            fun: func_value,
        });
        let frame_limit = match frame_base.checked_add(func.max_stack as usize) {
            Some(limit) => limit,
            None => {
                self.ctx.bc_frames.pop();
                return Err(invalid_bytecode_flow());
            }
        };
        if self.ctx.bc_buf.capacity() < frame_limit {
            self.ctx
                .bc_buf
                .reserve_exact(frame_limit - self.ctx.bc_buf.len());
        }
        // Seed the operand stack with the native frame's live values (traced
        // from here on; the caller performed no allocation since reading them
        // out of the spill buffer).
        self.ctx.bc_buf.extend_from_slice(stack);
        let mut pc = start_pc;
        let mut handlers = HandlerStack::new();
        for _ in 0..handlers_active {
            handlers.push(Handler::Condition);
        }
        let mut bind_stack: BindStack = bind_entries.iter().copied().collect();
        let result = self.run_loop(
            func,
            frame_base,
            frame_limit,
            &mut pc,
            &mut handlers,
            &mut bind_stack,
        );
        self.cleanup_bytecode_frame(result, condition_stack_base, specpdl_base, frame_base)
    }

    /// Run a bytecode frame whose arguments live on the GC-traced `bc_buf`
    /// at `[args_start, args_start + nargs)` — the GNU `exec_byte_code`
    /// argument model. The caller's slots are never aliased or mutated: the
    /// frame starts at `bc_buf.len()` and the args are copied ONCE into
    /// fresh callee slots (GNU setup_frame's `PUSH (*args++)` loop,
    /// bytecode.c:542-549), so a zero-copy backtrace span over the caller's
    /// slots (`BacktraceArgs::EvaluatedBcStack`) stay valid and unmutated
    /// for the whole call — exactly GNU's `record_in_backtrace` pointer into
    /// the intact caller stack. Every exit truncates back to the frame base,
    /// leaving the caller's args for the CALLER to pop.
    fn run_frame(
        &mut self,
        func: &ByteCodeFunction,
        args_start: usize,
        nargs: usize,
        func_value: Value,
    ) -> EvalResult {
        let condition_stack_base = self.ctx.condition_stack_len();
        let frame_base = self.ctx.bc_buf.len();
        debug_assert!(
            args_start + nargs <= frame_base,
            "caller args must live at or below the new frame base"
        );
        self.ctx.bc_frames.push(crate::emacs_core::eval::BcFrame {
            base: frame_base,
            fun: func_value,
        });
        let mut pc: usize = 0;
        let mut handlers = HandlerStack::new();
        let specpdl_base = self.ctx.specpdl.len();
        let mut bind_stack = BindStack::new();

        let n_required = func.params.required.len();
        let n_optional = func.params.optional.len();
        let has_rest = func.params.rest.is_some();
        let nonrest = n_required + n_optional;

        // GNU Emacs validates bytecode arity before pushing the frame.
        // See src/bytecode.c: the VM checks the arg descriptor and signals
        // wrong-number-of-arguments immediately instead of nil-padding missing
        // required args.
        if !(n_required <= nargs && (has_rest || nargs <= nonrest)) {
            // GNU bytecode.c signals the raw bytecode descriptor pair
            // (mandatory . nonrest), even when the descriptor has the &rest
            // bit set.  This differs intentionally from func-arity, which
            // reports `many` for the same bytecode function.
            let arity = Value::cons(
                Value::fixnum(n_required as i64),
                Value::fixnum(nonrest as i64),
            );
            self.ctx.bc_buf.truncate(frame_base);
            self.ctx.bc_frames.pop();
            return Err(signal(
                LispCondition::WrongNumberOfArguments,
                vec![arity, Value::fixnum(nargs as i64)],
            ));
        }

        let frame_limit = match frame_base.checked_add(func.max_stack as usize) {
            Some(limit) => limit,
            None => {
                self.ctx.bc_buf.truncate(frame_base);
                self.ctx.bc_frames.pop();
                return Err(invalid_bytecode_flow());
            }
        };
        if self.ctx.bc_buf.capacity() < frame_limit {
            self.ctx
                .bc_buf
                .reserve_exact(frame_limit - self.ctx.bc_buf.len());
        }

        // GNU's bytecode stores lexical params at known stack positions; the
        // byte-compiler emits `byte-stack-ref` for every lexical reference,
        // so the param names are NOT looked up at runtime and don't need any
        // environment entry.  Dynamic params, on the other hand, are
        // referenced via `byte-varref` and must be specbound on the
        // function's specpdl span.  This split mirrors `byte-compile-bind`
        // in bytecomp.el and matches GNU's `funcall_lambda` (eval.c) ->
        // `exec_byte_code` (bytecode.c).  Building an intermediate
        // OrderedRuntimeBindingMap of params per call (which the previous
        // code did even for the lexical case) is dead work that dominated
        // debug-build batch-byte-compile runtime.
        let has_named_params = nonrest > 0 || has_rest;
        let params_on_stack = func.lexical
            || func.env.is_some()
            || matches!(func.arglist.kind(), ValueKind::Fixnum(_));
        if params_on_stack {
            // Lexical bytecode follows GNU bytecode.c: exec_byte_code receives
            // the encoded arg template and pushes incoming arguments into the
            // bytecode frame before executing the first instruction. The seed
            // slots (nonrest params + optional rest list) must fit the frame:
            // the same bound the old per-push checks enforced, folded into one
            // comparison (the error path truncates any partial seed anyway).
            let seed_slots = nonrest + usize::from(has_rest);
            if frame_base + seed_slots > frame_limit {
                self.ctx.bc_buf.truncate(frame_base);
                self.ctx.bc_frames.pop();
                return Err(invalid_bytecode_flow());
            }
            let copied = nargs.min(nonrest);
            for i in 0..copied {
                let v = self.ctx.bc_buf[args_start + i];
                if v.is_string() {
                    let ptr = v.as_string_ptr().unwrap();
                    let hdr = unsafe { &(*ptr).header };
                    if !matches!(hdr.kind, crate::tagged::header::HeapObjectKind::String) {
                        panic!(
                            "RUN_FRAME ARG BUG: arg[{}] = {:#x} (ptr {:?}, kind={:?}) is corrupt string. \
                             nargs={}, func has {} required, {} optional, rest={}",
                            i,
                            v.0,
                            ptr,
                            hdr.kind,
                            nargs,
                            func.params.required.len(),
                            func.params.optional.len(),
                            func.params.rest.is_some(),
                        );
                    }
                }
            }
            // The one arg copy of the call protocol (GNU setup_frame's PUSH
            // loop): caller slots -> fresh callee slots, then nil-pad the
            // missing optionals.
            copy_frame_arguments(&mut self.ctx.bc_buf, args_start, copied);
            for _ in copied..nonrest {
                self.ctx.bc_buf.push(Value::NIL);
            }

            if has_rest {
                // The rest args are read from the GC-traced caller slots,
                // which stay live through the cons allocations.
                let rest_list = if nargs > nonrest {
                    Value::list_from_slice(
                        &self.ctx.bc_buf[args_start + nonrest..args_start + nargs],
                    )
                } else {
                    Value::NIL
                };
                self.ctx.bc_buf.push(rest_list);
            }
        }

        if has_named_params {
            if params_on_stack {
                // Lexical bytecode functions: params live on bc_buf at the
                // bottom of the frame.  Install the captured closure env (if
                // any) and run; the body's stack-ref opcodes find the params
                // via frame_base.
                //
                // The lexenv save/restore (specpdl LexicalEnv entry, popped by
                // cleanup's unbind_to) happens ONLY when this frame actually
                // switches the environment (func.env = Some). An env-less
                // function runs in the caller's lexenv untouched: GNU pushes
                // no specpdl entry for any bytecode frame (bytecode.c
                // setup_frame / Breturn are specpdl-free), and the old
                // unconditional no-op save/restore forced every lexical frame
                // return down cleanup_bytecode_frame's slow path.
                use crate::emacs_core::eval::SpecBinding;
                #[cfg(debug_assertions)]
                let entry_lexenv = self.ctx.lexenv;
                if let Some(env) = func.env {
                    // Push BEFORE assigning: the entry keeps the caller's
                    // lexenv alist GC-traced while ctx.lexenv points at the
                    // closure env.
                    self.ctx.specpdl.push(SpecBinding::LexicalEnv {
                        old_lexenv: self.ctx.lexenv,
                    });
                    self.ctx.lexenv = env;
                }
                let result = self.run_loop(
                    func,
                    frame_base,
                    frame_limit,
                    &mut pc,
                    &mut handlers,
                    &mut bind_stack,
                );
                #[cfg(debug_assertions)]
                if func.env.is_none() {
                    debug_assert!(
                        lexenv_tail_reachable(self.ctx.lexenv, entry_lexenv),
                        "env-less bytecode frame changed ctx.lexenv beyond defvar markers"
                    );
                }
                return self.cleanup_bytecode_frame(
                    result,
                    condition_stack_base,
                    specpdl_base,
                    frame_base,
                );
            }

            // Dynamic bytecode functions: each param needs a specbind so
            // that varref opcodes inside the body can find it via the
            // obarray.  GNU eval.c:funcall_lambda then calls exec_byte_code
            // with zero bytecode arguments, so dynamic params must not occupy
            // bytecode stack slots. The caller's arg span stays live on
            // bc_buf through every specbind (variable watchers can run
            // arbitrary Lisp that captures backtraces reading it).
            let mut arg_idx = 0;
            for param in &func.params.required {
                let val = if arg_idx < nargs {
                    self.ctx.bc_buf[args_start + arg_idx]
                } else {
                    Value::NIL
                };
                if let Err(flow) = self.ctx.try_specbind(*param, val) {
                    return self.cleanup_bytecode_frame(
                        Err(flow),
                        condition_stack_base,
                        specpdl_base,
                        frame_base,
                    );
                }
                arg_idx += 1;
            }
            for param in &func.params.optional {
                let val = if arg_idx < nargs {
                    self.ctx.bc_buf[args_start + arg_idx]
                } else {
                    Value::NIL
                };
                if let Err(flow) = self.ctx.try_specbind(*param, val) {
                    return self.cleanup_bytecode_frame(
                        Err(flow),
                        condition_stack_base,
                        specpdl_base,
                        frame_base,
                    );
                }
                arg_idx += 1;
            }
            if let Some(rest_name) = func.params.rest {
                let rest_list = if arg_idx < nargs {
                    Value::list_from_slice(
                        &self.ctx.bc_buf[args_start + arg_idx..args_start + nargs],
                    )
                } else {
                    Value::NIL
                };
                if let Err(flow) = self.ctx.try_specbind(rest_name, rest_list) {
                    return self.cleanup_bytecode_frame(
                        Err(flow),
                        condition_stack_base,
                        specpdl_base,
                        frame_base,
                    );
                }
            }
            let result = self.run_loop(
                func,
                frame_base,
                frame_limit,
                &mut pc,
                &mut handlers,
                &mut bind_stack,
            );
            return self.cleanup_bytecode_frame(
                result,
                condition_stack_base,
                specpdl_base,
                frame_base,
            );
        }

        // No params: install the captured closure env (if any), then run.
        // Same discipline as the params_on_stack branch above: the specpdl
        // LexicalEnv save/restore exists only for frames that switch the
        // environment; env-less frames (whether or not func.lexical) leave
        // the caller's lexenv untouched and exit via cleanup's fast path,
        // matching GNU's specpdl-free bytecode frames.
        #[cfg(debug_assertions)]
        let entry_lexenv = self.ctx.lexenv;
        {
            use crate::emacs_core::eval::SpecBinding;
            if let Some(env) = func.env {
                // Push BEFORE assigning (see the params_on_stack branch).
                self.ctx.specpdl.push(SpecBinding::LexicalEnv {
                    old_lexenv: self.ctx.lexenv,
                });
                self.ctx.lexenv = env;
            }
        }

        let result = self.run_loop(
            func,
            frame_base,
            frame_limit,
            &mut pc,
            &mut handlers,
            &mut bind_stack,
        );
        #[cfg(debug_assertions)]
        if func.env.is_none() {
            debug_assert!(
                lexenv_tail_reachable(self.ctx.lexenv, entry_lexenv),
                "env-less bytecode frame changed ctx.lexenv beyond defvar markers"
            );
        }
        self.cleanup_bytecode_frame(result, condition_stack_base, specpdl_base, frame_base)
    }

    /// Whether this frame can use the first iterative `setup_frame` slice.
    ///
    /// The slice deliberately starts with GNU's common encoded-argument,
    /// env-less bytecode shape.  Dynamic parameter binding, captured lexical
    /// environments and `&rest` construction still use the established
    /// recursive path until their unwind transitions are represented in the
    /// frame state as well.
    fn can_enter_interpreter_frame_iteratively(
        &self,
        func: &ByteCodeFunction,
        nargs: usize,
    ) -> bool {
        let required = func.params.required.len();
        let optional = func.params.optional.len();
        let nonrest = required + optional;
        let has_named_params = nonrest > 0;
        let params_on_stack = func.lexical || matches!(func.arglist.kind(), ValueKind::Fixnum(_));

        func.env.is_none()
            && func.params.rest.is_none()
            && (!has_named_params || params_on_stack)
            && required <= nargs
            && nargs <= nonrest
            && nonrest <= func.max_stack as usize
            // Sealed-dispatch safety gate for iterative callees, the twin of
            // the entry gate in `run_loop`: only `seal_ops`-normalized code
            // may enter the unchecked-fetch driver. Evaluated at (cacheable)
            // classification time, never on the per-call hot path.
            && func.executes_sealed_ops()
            // The VERIFIED driver instance additionally relies on the callee's
            // operand-stack proof, and the call-target cache is shared between
            // both instances, so admission requires the proof universally. A
            // refused callee (e.g. Switch-bearing) still runs — through the
            // generic call path into its own checked driver.
            && func.executes_verified_ops()
    }

    /// Install one already-validated env-less interpreter frame in place.
    ///
    /// No Lisp allocation or GC safe point occurs here. The exact callee value
    /// replaces the consumed function-designator operand before any bytecode
    /// executes. That caller slot remains in GC-traced `bc_buf` through child
    /// cleanup, so redefining a symbol cannot collect its executing old value.
    ///
    /// `current` must already be suspended on the caller stack; its fields are
    /// overwritten directly, exactly as GNU's `setup_frame` writes the child
    /// `bc_frame` in place instead of materializing it elsewhere first.
    // inline(always): called from both run_interpreter_driver
    // monomorphizations; without the hint LLVM outlines one shared copy and
    // every Bcall pays a call/ret + register marshalling (measured +58
    // Ir/call when the driver split landed).
    #[inline(always)]
    /// Set up the callee's operand frame and RETURN its interpreter frame; the
    /// caller pushes it onto the frame stack.
    fn install_iterative_interpreter_frame(
        &mut self,
        cursor: &mut StackCursor,
        callee: PreparedInterpreterCallee,
        root_slot: ConsumedCallOperandRootSlot,
        nargs: usize,
    ) -> InterpreterFrame {
        // Every stack mutation below goes through the LIVE cursor (GNU's
        // setup_frame works on its register `top` the same way); the context
        // is only consulted for capacity and, on the cold growth branch,
        // synchronized around the reallocation.
        // SAFETY: the root slot indexes a consumed caller operand strictly
        // below the live length.
        root_slot.install_exact_callee(
            unsafe { std::slice::from_raw_parts_mut(cursor.base, cursor.len) },
            callee.value,
        );
        let args_start = root_slot.args_start();
        let func = callee.code();
        debug_assert!(self.can_enter_interpreter_frame_iteratively(func, nargs));
        let condition_stack_base = self.ctx.condition_stack_len();
        let specpdl_base = self.ctx.specpdl.len();
        let frame_base = cursor.len;
        debug_assert!(args_start + nargs <= frame_base);
        let frame_limit = frame_base
            .checked_add(func.max_stack as usize)
            .expect("iterative frame limit prevalidated");

        #[cfg(test)]
        observe_iterative_context_bc_frames_len(self.ctx.bc_frames.len());
        if self.ctx.bc_buf.capacity() < frame_limit {
            // Cold growth: sync the live length, let the Vec reallocate,
            // rearm the base pointer.
            // SAFETY: same initialization argument as `StackCursor::publish`.
            unsafe { self.ctx.bc_buf.set_len(cursor.len) };
            self.ctx.bc_buf.reserve_exact(frame_limit - cursor.len);
            cursor.base = self.ctx.bc_buf.as_mut_ptr();
        }

        let nonrest = func.params.required.len() + func.params.optional.len();
        // GNU setup_frame's PUSH loop on the live cursor: copy the incoming
        // arguments into the fresh frame, then nil-fill missing optionals.
        // SAFETY: capacity >= frame_limit >= frame_base + nonrest
        // (`can_enter` proved `nonrest <= max_stack`), the source span
        // [args_start, args_start + nargs) sits at or below frame_base so the
        // regions are disjoint, and every written slot lands below the new
        // length.
        unsafe {
            // Word-at-a-time for ordinary arities (GNU setup_frame's PUSH
            // loop): a libc memcpy dispatch costs more than a handful of
            // already-capacity-checked stores. Bulk path only for unusually
            // wide generated functions, mirroring FrameArgumentCopy.
            match FrameArgumentCopy::for_count(nargs) {
                FrameArgumentCopy::None => {}
                FrameArgumentCopy::One => {
                    *cursor.base.add(cursor.len) = *cursor.base.add(args_start);
                    cursor.len += 1;
                }
                FrameArgumentCopy::Scalar => {
                    for offset in 0..nargs {
                        *cursor.base.add(cursor.len) = *cursor.base.add(args_start + offset);
                        cursor.len += 1;
                    }
                }
                FrameArgumentCopy::Bulk => {
                    std::ptr::copy_nonoverlapping(
                        cursor.base.add(args_start),
                        cursor.base.add(cursor.len),
                        nargs,
                    );
                    cursor.len += nargs;
                }
            }
            for _ in nargs..nonrest {
                *cursor.base.add(cursor.len) = Value::NIL;
                cursor.len += 1;
            }
        }

        // Built and RETURNED rather than written through a `&mut` into the
        // frame stack: every value here was just computed and is already in a
        // register, and handing the frame back by value lets the caller move it
        // into its own fresh slot. The old shape wrote into the caller's slot,
        // which is why entering a call first had to copy the caller out of it.
        InterpreterFrame {
            function: callee.function,
            frame_base,
            frame_limit,
            #[cfg(feature = "jit")]
            resume: InterpreterResumePoint::new(0, false),
            #[cfg(not(feature = "jit"))]
            pc: 0,
            cleanup: InterpreterFrameCleanup {
                condition_stack_base,
                specpdl_base,
            },
            #[cfg(debug_assertions)]
            entry_lexenv: self.ctx.lexenv,
        }
    }

    fn finish_interpreter_frame(
        &mut self,
        frame: &InterpreterFrame,
        is_entry: bool,
        result: EvalResult,
    ) -> EvalResult {
        if is_entry {
            return result;
        }
        let cleanup = frame.cleanup;
        #[cfg(debug_assertions)]
        debug_assert!(
            lexenv_tail_reachable(self.ctx.lexenv, frame.entry_lexenv),
            "env-less iterative bytecode frame changed ctx.lexenv beyond defvar markers"
        );
        self.cleanup_iterative_bytecode_frame(
            result,
            cleanup.condition_stack_base,
            cleanup.specpdl_base,
            frame.frame_base,
        )
    }

    /// Finish the current frame and either restore its caller or leave the
    /// interpreter driver.  A nonlocal exit is offered to each suspended
    /// caller in turn, exactly as recursive Rust returns used to do, but the
    /// unwind is represented as data rather than host-stack control flow.
    fn complete_interpreter_frame_chain(
        &mut self,
        callers: &mut InterpreterCallerStack,
        aux_stack: &mut InterpreterFrameAuxStack,
        mut result: EvalResult,
    ) -> InterpreterFrameCompletion {
        loop {
            let outermost = callers.has_no_suspended_callers();
            result = self.finish_interpreter_frame(callers.active_mut(), outermost, result);

            let Some(continuation) = callers.leave_callee() else {
                return InterpreterFrameCompletion::Exit(result);
            };

            self.leave_bytecode_call_depth();
            // `dispatch_interpreter_stack_call` created this frame directly
            // from `bc_buf`, and `finish_interpreter_frame` has already
            // unwound the callee to the depth immediately above it.  Unlike a
            // generic funcall backtrace, this representation never owns an
            // out-of-line `backtrace_args_stack` slot, so GNU's Breturn fast
            // path is exactly a specpdl pointer decrement.  Keeping the fast
            // pop behind the typed iterative continuation prevents the
            // generic release/unbind machinery from becoming a per-call tax.
            match self
                .ctx
                .pop_fast_bytecode_backtrace_frame(continuation.backtrace)
            {
                crate::emacs_core::eval::FastBytecodePop::Popped => {}
                // GNU `Breturn`'s `val = call_debugger (list2 (Qexit, val))`
                // (`src/bytecode.c:825-828`): the exit debugger's return value
                // REPLACES this call's, so it has to be spent here where the
                // result is still in hand.  `backtrace-debug` can raise the
                // flag on this frame from inside the call that is returning.
                crate::emacs_core::eval::FastBytecodePop::OwesDebugOnExit(frame) => {
                    result = self
                        .ctx
                        .pop_bytecode_backtrace_token_with_result(frame, result);
                }
            }
            aux_stack.restore_current(InterpreterDriverDepth::from_suspended_callers(
                callers.suspended_len(),
            ));

            match result {
                Ok(value) => {
                    self.ctx.bc_buf.truncate(continuation.stack_after_call);
                    let current = callers.active_mut();
                    debug_assert!(self.ctx.bc_buf.len() < current.frame_limit);
                    self.ctx.bc_buf.push(value);
                    return InterpreterFrameCompletion::Resume;
                }
                Err(flow) => {
                    let (func, mut resume_pc) = {
                        let current = callers.active();
                        (current.function.code(), current.pc())
                    };
                    let aux = aux_stack.current_mut();
                    let resume = self.resume_nonlocal(
                        func,
                        &mut resume_pc,
                        &mut aux.handlers,
                        &mut aux.bind_stack,
                        flow,
                    );
                    callers.active_mut().set_pc(resume_pc);
                    match resume {
                        Ok(()) => return InterpreterFrameCompletion::Resume,
                        Err(flow) => result = Err(flow),
                    }
                }
            }
        }
    }

    /// Perform the common GNU `Breturn` transition without packaging the value
    /// as an `EvalResult` or entering generic unwind machinery.
    ///
    /// Env-less iterative frames normally leave no specpdl entries. A frame
    /// that does have outstanding dynamic state is classified explicitly and
    /// handed back to `complete_interpreter_frame_chain`; this fast path never
    /// approximates or skips an unwind.
    #[inline(always)]
    /// The cursor stays LIVE through this fast path (GNU keeps `top` in a
    /// register across `Breturn`): every stack mutation goes through it, the
    /// context sees nothing, and only the `Exit`/`NeedsSlowCleanup` outcomes
    /// require the caller to publish before leaving the driver's register
    /// state. Nothing here allocates or reaches a GC safe point.
    fn complete_interpreter_frame_value(
        &mut self,
        cursor: &mut StackCursor,
        callers: &mut InterpreterCallerStack,
        aux_stack: &mut InterpreterFrameAuxStack,
        value: Value,
    ) -> InterpreterValueCompletion {
        if callers.has_no_suspended_callers() {
            return InterpreterValueCompletion::Exit(value);
        }

        // Read everything needed from the ACTIVE frame before popping it: after
        // `leave_callee` the top of the stack is the caller, not this frame.
        let (cleanup, frame_base) = {
            let current = callers.active();
            (current.cleanup, current.frame_base)
        };
        #[cfg(debug_assertions)]
        let entry_lexenv = callers.active().entry_lexenv;
        #[cfg(debug_assertions)]
        debug_assert!(
            lexenv_tail_reachable(self.ctx.lexenv, entry_lexenv),
            "env-less iterative bytecode frame changed ctx.lexenv beyond defvar markers"
        );

        // ConditionFrame has no Drop and truncation cannot run Lisp or GC.
        // Do it before classifying the specpdl state so the slow fallback has
        // exactly the same observable unwind state as the generic path.
        self.ctx
            .truncate_condition_stack(cleanup.condition_stack_base);
        // GNU `Breturn` tests `backtrace_debug_on_exit` BEFORE its
        // `specpdl_ptr--` (`src/bytecode.c:825-828`) because the debugger's
        // return value replaces the call's.  This return has no way to carry a
        // replaced value or a nonlocal exit, so a flagged frame is part of the
        // ineligibility test rather than something the pop discovers: the
        // frame this return pops is the one immediately below the callee's
        // base, which the line above just proved is the specpdl top.
        let returning_frame = cleanup.specpdl_base.checked_sub(1);
        if self.ctx.specpdl.len() != cleanup.specpdl_base
            || returning_frame
                .is_some_and(|index| self.ctx.backtrace_frame_wants_debug_on_exit(index))
        {
            return InterpreterValueCompletion::NeedsSlowCleanup(value);
        }

        cursor.truncate(frame_base);

        // SAFETY: the empty caller stack returned `Exit` above, and no code
        // between that proof and this pop can mutate `callers`.  GNU's
        // `Breturn` likewise restores its already-proven saved frame directly.
        // SAFETY-equivalent by construction now: `leave_callee` returns `None`
        // only when there is no suspended caller, which the guard above already
        // rejected. The old unchecked pop existed because the caller frame had
        // to be COPIED back into `current`; there is nothing to copy any more.
        let continuation = callers
            .leave_callee()
            .expect("a suspended caller was proven above");
        self.leave_bytecode_call_depth();
        // The eligibility gate above asked `backtrace_debug_on_exit` about
        // exactly this index, so the checking pop would ask a second time on
        // the interpreter's hottest return.
        self.ctx
            .pop_fast_bytecode_backtrace_frame_unchecked(continuation.backtrace);
        aux_stack.restore_current(InterpreterDriverDepth::from_suspended_callers(
            callers.suspended_len(),
        ));
        // GNU Breturn value delivery on the live cursor: the result lands in
        // the consumed function-operand slot, everything above is discarded.
        // SAFETY: `stack_after_call = args_start - 1 < frame_base`, and the
        // truncate above set the cursor exactly to that frame_base.
        let after = continuation.stack_after_call;
        debug_assert!(after < cursor.len);
        unsafe {
            *cursor.base.add(after) = value;
        }
        cursor.len = after + 1;
        debug_assert!(cursor.len <= callers.active().frame_limit);
        InterpreterValueCompletion::Resume
    }

    /// Deliver a bytecode return value into the restored caller's stack.
    ///
    /// GNU's `Breturn` is `top = fp->saved_top; PUSH (val)`: the result lands
    /// in the consumed function-designator slot and everything above it is
    /// discarded.  Fusing the truncate with the push removes both the truncate
    /// comparison and the push capacity branch — the vector only ever shrinks
    /// here.
    #[inline(always)]
    fn deliver_interpreter_return_value(&mut self, stack_after_call: usize, value: Value) {
        let buf = &mut self.ctx.bc_buf;
        debug_assert!(stack_after_call < buf.len());
        // SAFETY: `stack_after_call` is the caller's consumed function-operand
        // slot, strictly below the callee's `frame_base`
        // (`args_start + nargs <= frame_base` held at frame installation), and
        // the callee was truncated to exactly that `frame_base` immediately
        // before the caller was restored with no `bc_buf` mutation in between.
        // The slot is therefore initialized and in bounds, and the new length
        // only shrinks the vector, so no capacity or drop concerns exist
        // (`Value` is `Copy`).
        unsafe {
            *buf.as_mut_ptr().add(stack_after_call) = value;
            buf.set_len(stack_after_call + 1);
        }
    }

    /// Resolve and dispatch one `Op::Call` after the depth guard has entered.
    ///
    /// `Enter` deliberately leaves the backtrace frame open; the iterative
    /// driver closes it when the callee returns.  Every other target completes
    /// synchronously and preserves the existing call protocol.
    // inline(always): same both-instantiations story as
    // install_iterative_interpreter_frame (measured +60 Ir/call outlined).
    #[inline(always)]
    fn dispatch_interpreter_stack_call(
        &mut self,
        func_val: Value,
        args_start: usize,
        nargs: usize,
        target: ResolvedStackCallTarget,
    ) -> InterpreterStackCall {
        match target {
            ResolvedStackCallTarget::Interpreter { call } => {
                let callee = call.callee();
                let func = callee.code();
                let callee = callee.value();
                let backtrace = self
                    .ctx
                    .push_backtrace_frame_from_bc_stack(func_val, args_start, nargs);
                InterpreterStackCall::Enter {
                    callee: PreparedInterpreterCallee::new(callee, func),
                    root_slot: ConsumedCallOperandRootSlot::from_args_start(args_start),
                    nargs,
                    backtrace,
                }
            }
            ResolvedStackCallTarget::ByteCode { callee } => {
                let func = callee.code();
                let callee = callee.value();
                let backtrace = self
                    .ctx
                    .push_backtrace_frame_from_bc_stack(func_val, args_start, nargs);
                match self.dispatch_bytecode_tier(func, args_start, nargs, callee) {
                    BytecodeStackCallDispatch::Interpret
                        if self.can_enter_interpreter_frame_iteratively(func, nargs) =>
                    {
                        InterpreterStackCall::Enter {
                            callee: PreparedInterpreterCallee::new(callee, func),
                            root_slot: ConsumedCallOperandRootSlot::from_args_start(args_start),
                            nargs,
                            backtrace,
                        }
                    }
                    BytecodeStackCallDispatch::Interpret => {
                        let result = self.execute_from_stack_args(func, args_start, nargs, callee);
                        let result = self.ctx.dispatch_signal_result_if_needed(result);
                        InterpreterStackCall::Complete(
                            self.ctx
                                .pop_bytecode_backtrace_token_with_result(backtrace, result),
                        )
                    }
                    BytecodeStackCallDispatch::Complete(result) => {
                        let result = self.ctx.dispatch_signal_result_if_needed(result);
                        InterpreterStackCall::Complete(
                            self.ctx
                                .pop_bytecode_backtrace_token_with_result(backtrace, result),
                        )
                    }
                }
            }
            ResolvedStackCallTarget::Builtin { callee } => {
                InterpreterStackCall::Complete(Self::call_resolved_builtin_from_stack_args(
                    self.ctx, func_val, args_start, nargs, callee,
                ))
            }
            ResolvedStackCallTarget::Generic => {
                let backtrace = self
                    .ctx
                    .push_backtrace_frame_from_bc_stack(func_val, args_start, nargs);
                let result = self.call_function_untraced_from_stack(func_val, args_start, nargs);
                let result = self.ctx.dispatch_signal_result_if_needed(result);
                InterpreterStackCall::Complete(
                    self.ctx
                        .pop_bytecode_backtrace_token_with_result(backtrace, result),
                )
            }
        }
    }

    /// Consult adaptive tiers only when this VM can reach one.
    ///
    /// The policy branch is deliberately here, before the out-of-line Context
    /// dispatcher: an interpreter-only process performs neither a function
    /// call nor a per-function atomic heat/feedback operation on Bcall.
    #[inline(always)]
    fn dispatch_bytecode_tier(
        &mut self,
        func: &ByteCodeFunction,
        args_start: usize,
        nargs: usize,
        callee: Value,
    ) -> BytecodeStackCallDispatch {
        #[cfg(feature = "jit")]
        if self.bytecode_tier_policy == BytecodeTierPolicy::InterpreterOnly
            || self.bcall_tier_skipped
        {
            return BytecodeStackCallDispatch::Interpret;
        }

        self.ctx
            .dispatch_bytecode_call_from_stack(func, args_start, nargs, callee)
    }

    fn run_loop(
        &mut self,
        entry_func: &ByteCodeFunction,
        frame_base: usize,
        frame_limit: usize,
        pc: &mut usize,
        handlers: &mut HandlerStack,
        bind_stack: &mut BindStack,
    ) -> EvalResult {
        #[cfg(test)]
        let _run_loop_depth = RunLoopDepthGuard::enter();

        // Sealed-dispatch safety gate. The driver fetches instructions without
        // a per-op bound check, which is sound only for `seal_ops`-normalized
        // code (trailing `Return`, in-bounds branch targets, in-range
        // constant indices). The `ops_sealed` marker is set exclusively by
        // the decode installers, so a hand-assembled chunk is rejected here,
        // once per driver entry, before any unchecked fetch — inferring
        // sealedness from the instructions themselves would trust exactly the
        // data the seal exists to prove.
        if !entry_func.executes_sealed_ops() {
            return Err(invalid_bytecode_flow());
        }

        let entry_frame = InterpreterFrame {
            function: InterpreterFunction::new(entry_func),
            frame_base,
            frame_limit,
            #[cfg(feature = "jit")]
            resume: InterpreterResumePoint::new(*pc, false),
            #[cfg(not(feature = "jit"))]
            pc: *pc,
            cleanup: InterpreterFrameCleanup {
                condition_stack_base: 0,
                specpdl_base: 0,
            },
            #[cfg(debug_assertions)]
            entry_lexenv: Value::NIL,
        };
        // Most bytecode bodies make no nested bytecode call, so begin without
        // allocating. The first iterative Bcall grows this once and thereafter
        // gets Vec's single-representation push/pop path; SmallVec's repeated
        // inline-vs-spilled branch was measurable on every Bcall/Breturn.
        let mut callers = InterpreterCallerStack::new(entry_frame);
        // GNU bytecode.c keeps one unsigned quit counter for the whole
        // exec_byte_code driver. setup_frame/Breturn do not save or reset it.
        let mut quitcounter = 1;
        let mut aux_stack =
            InterpreterFrameAuxStack::new(std::mem::take(handlers), std::mem::take(bind_stack));

        // One driver source, two monomorphizations: the VERIFIED instance
        // drops the per-push capacity guard on the strength of the decode-time
        // `verify_stack_effects` proof; everything else runs the checked
        // instance with today's exact behavior. Chosen once per driver entry.
        let result = if entry_func.executes_verified_ops() {
            self.run_interpreter_driver::<true>(&mut callers, &mut aux_stack, &mut quitcounter)
        } else {
            self.run_interpreter_driver::<false>(&mut callers, &mut aux_stack, &mut quitcounter)
        };
        *pc = callers.active().pc();
        let (entry_handlers, entry_bind_stack) = aux_stack.take_entry();
        *handlers = entry_handlers;
        *bind_stack = entry_bind_stack;
        result
    }

    /// Run every eligible Tier-0 bytecode frame in one GNU-shaped dispatcher.
    ///
    /// `Bcall` installs a child and `Breturn` restores its caller inside the
    /// same driver.  Frame transitions therefore cannot accidentally grow the
    /// Rust call stack or package the hot path in a general control-flow enum.
    /// The `'frame` loop is the typed equivalent of GNU bytecode.c's
    /// `setup_frame`/`Breturn` jumps; the inner loop remains opcode dispatch.
    #[inline(always)]
    fn run_interpreter_driver<const VERIFIED: bool>(
        &mut self,
        callers: &mut InterpreterCallerStack,
        aux_stack: &mut InterpreterFrameAuxStack,
        driver_quitcounter: &mut u8,
    ) -> EvalResult {
        // A6, extended across frames: base+len of the operand stack live in
        // registers for the whole DRIVER, not just one frame (GNU keeps
        // top/pc in locals across setup_frame/Breturn, bytecode.c). Escapes
        // publish and reacquire; the iterative Bcall/Breturn transitions keep
        // the cursor live.
        let mut cursor = StackCursor::acquire(self.ctx);
        'frame: loop {
            // Unpack the ACTIVE frame into locals and let the borrow end: the
            // body pushes and pops `callers`, so nothing may hold a reference
            // into it across those. This is also where a `continue 'frame`
            // lands, which is exactly where a push has just happened.
            let (func, frame_base, frame_limit) = {
                let current = callers.active();
                (
                    current.function.code(),
                    current.frame_base,
                    current.frame_limit,
                )
            };
            let ops = func.executable_ops();
            let constants = &func.constants;
            let ops_len = ops.len();
            let ops_ptr = ops.as_ptr();
            let mut pc_local = callers.active().pc();
            let mut quitcounter = *driver_quitcounter;
            // OSR (on-stack replacement): once a hot loop is detected at a backward
            // branch, transfer the rest of this interpreted call into native code at
            // the loop header. `osr_tried` latches so a loop that can't/didn't OSR is
            // probed only once per frame. The opt-in/kill-switch gates are evaluated
            // at the USE site (inside the 1-in-256 back-edge wrap), NOT here: this
            // runs on every interpreted call, and hoisting two `OnceLock` reads onto
            // that path cost +1.8% on byte-compile even with the JIT disabled
            // (measured; the tax persisted under `NEOVM_JIT=0`, which is what pinned
            // it to the interpreter path rather than to compile-time analysis).
            #[cfg(feature = "jit")]
            let mut osr_tried = callers.active().resume.osr_tried();
            #[cfg(not(feature = "jit"))]
            let osr_tried = false;

            macro_rules! stk {
                () => {
                    cursor
                };
            }

            // Resume nonlocal flow at the innermost VM handler, or propagate out
            // of run_loop. The cursor must be PUBLISHED before this runs:
            // resume_nonlocal truncates bc_buf to the handler's stack height and
            // can run unwind-protect cleanup forms (arbitrary Lisp / GC).
            macro_rules! resume_flow {
                ($flow:expr) => {{
                    let resume = {
                        let aux = aux_stack.current_mut();
                        self.resume_nonlocal(
                            func,
                            &mut pc_local,
                            &mut aux.handlers,
                            &mut aux.bind_stack,
                            $flow,
                        )
                    };
                    match resume {
                        Ok(()) => {
                            cursor = StackCursor::acquire(&mut self.ctx);
                            continue;
                        }
                        Err(flow) => {
                            callers
                                .active_mut()
                                .save_execution_state(pc_local, osr_tried);
                            *driver_quitcounter = quitcounter;
                            match self.complete_interpreter_frame_chain(
                                callers,
                                aux_stack,
                                Err(flow),
                            ) {
                                InterpreterFrameCompletion::Resume => {
                                    cursor = StackCursor::acquire(self.ctx);
                                    continue 'frame;
                                }
                                InterpreterFrameCompletion::Exit(result) => return result,
                            }
                        }
                    }
                }};
            }

            macro_rules! complete_value {
                ($value:expr) => {{
                    let value = $value;
                    callers
                        .active_mut()
                        .save_execution_state(pc_local, osr_tried);
                    *driver_quitcounter = quitcounter;
                    // The fast Breturn keeps the cursor live; only leaving the
                    // driver (Exit) or entering the generic unwind machinery
                    // (chain) publishes, and a chain Resume reacquires.
                    match self.complete_interpreter_frame_value(
                        &mut cursor,
                        callers,
                        aux_stack,
                        value,
                    ) {
                        InterpreterValueCompletion::Resume => continue 'frame,
                        InterpreterValueCompletion::Exit(value) => {
                            cursor.publish(self.ctx);
                            return Ok(value);
                        }
                        InterpreterValueCompletion::NeedsSlowCleanup(value) => {
                            cursor.publish(self.ctx);
                            match self.complete_interpreter_frame_chain(
                                callers,
                                aux_stack,
                                Ok(value),
                            ) {
                                InterpreterFrameCompletion::Resume => {
                                    cursor = StackCursor::acquire(self.ctx);
                                    continue 'frame;
                                }
                                InterpreterFrameCompletion::Exit(result) => return result,
                            }
                        }
                    }
                }};
            }

            macro_rules! return_value {
                ($value:expr) => {{
                    let value = $value;
                    complete_value!(value)
                }};
            }

            macro_rules! ensure_stack_push_capacity {
                () => {{
                    debug_assert!(
                        !VERIFIED || cursor.len < frame_limit,
                        "verified bytecode exceeded its proven max_stack"
                    );
                    if !VERIFIED && cursor.len >= frame_limit {
                        let invalid_pc = pc_local.saturating_sub(1);
                        let stack_len = cursor.len;
                        cursor.publish(&mut self.ctx);
                        trace_invalid_bytecode_site(
                            func,
                            "push-frame-limit",
                            invalid_pc,
                            frame_base,
                            frame_limit,
                            stack_len,
                            ops.get(invalid_pc),
                        );
                        resume_flow!(invalid_bytecode_flow())
                    }
                }};
            }

            macro_rules! stk_push {
            ($val:expr) => {{
                let v = $val;
                #[cfg(debug_assertions)]
                if v.is_string() {
                    let ptr = v.as_string_ptr().unwrap();
                    let hdr =
                        unsafe { &(*(ptr as *const crate::tagged::header::StringObj)).header };
                    if !matches!(hdr.kind, crate::tagged::header::HeapObjectKind::String) {
                        panic!(
                            "BC_BUF PUSH BUG: pushing corrupt string {:#x} (ptr {:?}, kind={:?}) \
                             at pc={}, op={:?}, bc_buf.len()={}, frame_base={}",
                            v.0,
                            ptr,
                            hdr.kind,
                            pc_local.saturating_sub(1),
                            ops.get(pc_local.saturating_sub(1)),
                            cursor.len,
                            frame_base,
                        );
                    }
                }
                ensure_stack_push_capacity!();
                // SAFETY: len < frame_limit <= bc_buf capacity (run_frame
                // reserved frame_limit up front).
                unsafe { cursor.push_unchecked(v) };
            }};
        }

            macro_rules! vm_try {
                ($expr:expr) => {{
                    cursor.publish(&mut self.ctx);
                    let result = $expr;
                    cursor = StackCursor::acquire(&mut self.ctx);
                    match result {
                        Ok(value) => value,
                        Err(flow) => {
                            cursor.publish(&mut self.ctx);
                            resume_flow!(flow)
                        }
                    }
                }};
            }

            // For NON-ESCAPING fallible helpers only (no bc_buf access, no GC
            // safe point, no Lisp): evaluates $expr with the cursor live so it
            // may read operands straight off the stack slice; publishes only on
            // the error path (resume_flow requires it).
            // GNU's Bcall runs maybe_quit unconditionally; its fast path is
            // loads-only, so evaluate that condition with the cursor LIVE and
            // pay the publish/reacquire pair only when the poll actually has
            // work (quit pending, profiler tick, throw-on-input armed).
            macro_rules! poll_quit {
                () => {{
                    if !self.ctx.maybe_quit_hot_ok() {
                        cursor.publish(self.ctx);
                        match self.ctx.maybe_quit() {
                            Ok(()) => {
                                cursor = StackCursor::acquire(self.ctx);
                            }
                            Err(flow) => resume_flow!(flow),
                        }
                    }
                }};
            }

            macro_rules! vm_try_pure {
                ($expr:expr) => {{
                    match $expr {
                        Ok(value) => value,
                        Err(flow) => {
                            cursor.publish(&mut self.ctx);
                            resume_flow!(flow)
                        }
                    }
                }};
            }

            macro_rules! branch_to {
                ($target:expr) => {{
                    let target = $target;
                    if target < pc_local {
                        quitcounter = quitcounter.wrapping_add(1);
                        if quitcounter == 0 {
                            quitcounter = 1;
                            // Loop-work heat (jit): 256 backward branches ≈ one call
                            // toward tier-up, so a hot INNER LOOP in a rarely-called
                            // body still goes native on its next entry. Piggybacks on
                            // the existing per-wrap cold path; no per-iteration cost.
                            #[cfg(feature = "jit")]
                            func.jit_runtime().note_loop_work();
                            vm_try!(self.ctx.bytecode_branch_maybe_gc_and_quit());
                            // OSR: the loop is hot and this is a backward branch (its
                            // target is the loop header). If the function is OSR-eligible
                            // and the live operand stack matches the header's entry depth,
                            // transfer into native code and finish there. `Ok` = the
                            // function completed (its result); `Signal` propagates; a
                            // deopt / non-transfer just falls back to interpreting (the
                            // OSR ran in its own frame, so our state is untouched).
                            // Gates ordered cheapest-first: a local bool, then the
                            // opt-in knob (default OFF, so it short-circuits the rest
                            // for every stock build), then the kill switch, then the
                            // heat load.
                            #[cfg(feature = "jit")]
                            if !osr_tried
                                && crate::emacs_core::jit::jit_osr_on()
                                && crate::emacs_core::jit::jit_runtime_enabled()
                                && func.jit_runtime().is_hot()
                            {
                                let depth = cursor.len - frame_base;
                                cursor.publish(&mut self.ctx);
                                let snapshot: Vec<Value> =
                                    self.ctx.bc_buf[frame_base..frame_base + depth].to_vec();
                                let ctx_ptr: *mut crate::emacs_core::eval::Context = &mut *self.ctx;
                                match crate::emacs_core::jit::cache::try_run_osr(
                                    ctx_ptr, func, target, &snapshot,
                                ) {
                                    Some(crate::emacs_core::jit::compile::NativeRun::Ok(bits)) => {
                                        // Cold OSR exit: the native run left the
                                        // context authoritative; rearm the cursor
                                        // for the shared completion path.
                                        cursor = StackCursor::acquire(&mut self.ctx);
                                        complete_value!(Value::from_bits(bits));
                                    }
                                    Some(crate::emacs_core::jit::compile::NativeRun::Signal) => {
                                        let flow =
                                            crate::emacs_core::jit::compile::take_pending_flow()
                                                .expect("OSR Signal must stash a pending flow");
                                        resume_flow!(flow)
                                    }
                                    _ => {
                                        // Deopt / DeoptAt / not-transferred: fall back to
                                        // the interpreter (state unchanged); don't retry.
                                        osr_tried = true;
                                        cursor = StackCursor::acquire(&mut self.ctx);
                                    }
                                }
                            }
                        }
                    }
                    pc_local = target;
                }};
            }

            macro_rules! invalid_bytecode {
                ($reason:expr) => {{
                    let invalid_pc = pc_local.saturating_sub(1);
                    let stack_len = cursor.len;
                    cursor.publish(&mut self.ctx);
                    trace_invalid_bytecode_site(
                        func,
                        $reason,
                        invalid_pc,
                        frame_base,
                        frame_limit,
                        stack_len,
                        ops.get(invalid_pc),
                    );
                    resume_flow!(invalid_bytecode_flow())
                }};
            }

            debug_assert!(
                matches!(ops.last(), Some(Op::Return)),
                "unchecked dispatch requires seal_ops-normalized bytecode"
            );
            loop {
                // SAFETY: GNU's FETCH is `*pc++` with no bound check; the
                // decode-time `seal_ops` invariant makes the same fetch sound
                // here. `pc_local` starts and resumes at a saved value
                // `< ops.len()`, every branch target is `< ops.len()`, and the
                // final instruction is a `Return` that never falls through, so
                // `pc_local` can never reach `ops.len()`. The entry gate in
                // `run_loop` and the iterative-callee gate in
                // `can_enter_interpreter_frame_iteratively` reject unsealed
                // hand-assembled chunks before dispatch.
                let op = unsafe { &*ops_ptr.add(pc_local) };
                pc_local += 1;
                #[cfg(test)]
                OPCODE_DISPATCH_COUNT.with(|count| count.set(count.get() + 1));
                #[cfg(feature = "vm-profile")]
                vm_profile::bump(op);

                match op {
                    // -- Constants and stack --
                    Op::Constant(idx) => {
                        // SAFETY: seal_ops proved every surviving `Constant`
                        // index in range at decode time (out-of-range ones
                        // became `TrapOutOfRangeConstant`); the published
                        // constant pool never shrinks. GNU's Bconstant is the
                        // same unchecked vector read.
                        let value = unsafe { *constants.get_unchecked(*idx as usize) };
                        stk_push!(value);

                        // GNU's threaded interpreter executes Bconstant and
                        // Bstack_ref as two straight-line handlers with no
                        // intervening safe point.  Decode keeps one Op per GNU
                        // bytecode so every original instruction remains a
                        // valid branch target; when control reaches the pair
                        // through its first instruction, execute the second
                        // handler here and skip only its dispatch.  A branch
                        // directly to the StackRef still uses the ordinary arm
                        // below.  Advance pc before validation, matching GNU's
                        // FETCH-before-handler error position.
                        // SAFETY: seal_ops — a non-Return op is never last,
                        // so `pc_local` is in bounds after the increment.
                        let next = unsafe { &*ops_ptr.add(pc_local) };
                        if let Op::StackRef(n) = next {
                            pc_local += 1;
                            #[cfg(feature = "vm-profile")]
                            vm_profile::bump(next);

                            let offset = 1 + *n as usize;
                            let len = stk!().len();
                            if offset <= len {
                                let value = unsafe { *stk!().get_unchecked(len - offset) };
                                stk_push!(value);
                            } else {
                                invalid_bytecode!("stack-ref-out-of-range");
                            }
                        }
                    }
                    Op::TrapOutOfRangeConstant(_) => {
                        invalid_bytecode!("constant-index-out-of-range");
                    }
                    Op::Nil => stk_push!(Value::NIL),
                    Op::True => stk_push!(Value::T),
                    Op::Pop => {
                        debug_assert!(
                            !VERIFIED || !stk!().is_empty(),
                            "verified bytecode underflowed its proven stack depth"
                        );
                        if !VERIFIED && stk!().is_empty() {
                            invalid_bytecode!("pop-empty-stack");
                        }
                        stk!().pop();
                    }
                    Op::Dup => {
                        if pc_local + 2 < ops_len {
                            let next0 = unsafe { &*ops_ptr.add(pc_local) };
                            let next1 = unsafe { &*ops_ptr.add(pc_local + 1) };
                            let next2 = unsafe { &*ops_ptr.add(pc_local + 2) };
                            if let (Op::StackRef(stack_ref), Op::Lss, Op::GotoIfNil(target)) =
                                (next0, next1, next2)
                            {
                                let len = cursor.len;
                                if len == 0 {
                                    invalid_bytecode!("dup-lss-gotoifnil-empty-stack");
                                }
                                debug_assert!(
                                    !VERIFIED || len < frame_limit,
                                    "verified bytecode exceeded its proven max_stack"
                                );
                                if !VERIFIED && len >= frame_limit {
                                    invalid_bytecode!("dup-lss-gotoifnil-stack-at-frame-limit");
                                }

                                let top = unsafe { *cursor.get_unchecked(len - 1) };
                                let after_dup_len = len + 1;
                                let offset = 1 + *stack_ref as usize;

                                debug_assert!(
                                    !VERIFIED || after_dup_len < frame_limit,
                                    "verified bytecode exceeded its proven max_stack"
                                );
                                if offset > after_dup_len
                                    || (!VERIFIED && after_dup_len >= frame_limit)
                                {
                                    // SAFETY: len < frame_limit checked above.
                                    unsafe { cursor.push_unchecked(top) };
                                    pc_local += 1;
                                    invalid_bytecode!("dup-lss-gotoifnil-stackref-out-of-range");
                                }

                                let ref_index = after_dup_len - offset;
                                let ref_value = if ref_index == len {
                                    top
                                } else {
                                    unsafe { *cursor.get_unchecked(ref_index) }
                                };

                                if top.is_fixnum() && ref_value.is_fixnum() {
                                    pc_local += 3;
                                    if !fixnum_lt(top, ref_value) {
                                        branch_to!(*target as usize);
                                    }
                                    continue;
                                }
                            }
                        }

                        if let Some(&top) = stk!().last() {
                            stk_push!(top);
                        } else {
                            invalid_bytecode!("dup-empty-stack");
                        }
                    }
                    Op::StackRef(n) => {
                        let offset = 1 + *n as usize;
                        let len = stk!().len();
                        debug_assert!(
                            !VERIFIED || offset <= len,
                            "verified bytecode underflowed its proven stack depth"
                        );
                        if VERIFIED || offset <= len {
                            // Valid bytecode references an existing stack slot.
                            // Keep the hot path to one explicit check and avoid
                            // the slice indexer's second bounds check.
                            let val = unsafe { *stk!().get_unchecked(len - offset) };

                            // GNU's Bstack_ref handlers perform PUSH + NEXT,
                            // and an adjacent Breturn immediately reads TOP;
                            // neither handler introduces a safe point.  Keep
                            // Return independently addressable, but when
                            // execution arrives through StackRef, return the
                            // selected value without a temporary push or a
                            // second Rust dispatch.  Validate the skipped push
                            // first so malformed max-stack metadata retains
                            // Neomacs's existing error behavior and site.
                            // SAFETY: seal_ops — a non-Return op is never
                            // last, so `pc_local` is in bounds here.
                            let next = unsafe { &*ops_ptr.add(pc_local) };
                            if matches!(next, Op::Return) {
                                ensure_stack_push_capacity!();
                                pc_local += 1;
                                #[cfg(feature = "vm-profile")]
                                vm_profile::bump(next);
                                return_value!(val);
                            }
                            stk_push!(val);
                        } else {
                            invalid_bytecode!("stack-ref-out-of-range");
                        }
                    }
                    Op::StackSet(n) => {
                        let len = stk!().len();
                        debug_assert!(
                            !VERIFIED || len > *n as usize,
                            "verified bytecode underflowed its proven stack depth"
                        );
                        if !VERIFIED && len == 0 {
                            invalid_bytecode!("stack-set-empty-stack");
                        }
                        let n = *n as usize;
                        if n == 0 {
                            stk!().pop();
                            continue;
                        }
                        if VERIFIED || n < len {
                            let val = unsafe { *cursor.get_unchecked(len - 1) };
                            let idx = len - 1 - n;
                            unsafe { *cursor.get_unchecked_mut(idx) = val };
                            cursor.len = len - 1;
                        } else {
                            invalid_bytecode!("stack-set-out-of-range");
                        }
                    }
                    Op::DiscardN(raw) => {
                        let preserve_tos = (raw & 0x80) != 0;
                        let n = (raw & 0x7F) as usize;
                        if n == 0 {
                            continue;
                        }
                        let len = stk!().len();
                        if n > len {
                            invalid_bytecode!("discard-n-out-of-range");
                        }
                        if preserve_tos {
                            if n >= len {
                                invalid_bytecode!("discard-n-preserve-tos-out-of-range");
                            }
                            let top = unsafe { *cursor.get_unchecked(len - 1) };
                            let target = len - 1 - n;
                            unsafe { *cursor.get_unchecked_mut(target) = top };
                        }
                        cursor.len = len - n;
                    }

                    // -- Variable access --
                    Op::VarRef(idx) => {
                        let name_id = sym_id_at(constants, *idx);
                        // Task-4 profiling: class + per-symbol VarRef breakdown
                        // (the BLV-fraction counter the T1 report flagged missing).
                        #[cfg(feature = "vm-profile")]
                        {
                            let (class, via_alias) = self.vm_profile_classify_varref(name_id);
                            vm_profile::bump_varref(name_id, class, via_alias);
                        }
                        let val = vm_try!(self.fast_path_var_ref(name_id));
                        stk_push!(val);
                    }
                    Op::VarSet(idx) => {
                        let name_id = sym_id_at(constants, *idx);
                        let val = stk!().pop().unwrap_or(Value::NIL);
                        let extra = [val];
                        vm_try!(self.with_frame_roots(func, &extra, |vm| {
                            vm.assign_var_id(name_id, val)
                        },));
                    }
                    Op::VarBind(idx) => {
                        // GNU bytecode.c Bvarbind: `specbind (vectorp[arg], POP);`
                        // — always a dynamic binding, no lexical fallback. The
                        // byte-compiler (bytecomp.el byte-compile-bind) emits
                        // `byte-varbind` ONLY for variables that
                        // `cconv--not-lexical-var-p` reports as dynamic — i.e.
                        // members of `byte-compile-bound-variables`, populated
                        // from the file's top-level `(defvar VAR)` declarations
                        // among other sources. Lexical `let` bindings never get
                        // a varbind opcode at all; they live on the value stack
                        // and are tracked via `byte-compile--lexical-environment`.
                        //
                        // Therefore the VM must NOT second-guess the byte-compiler
                        // by inspecting `is_special_id` / `lexenv_declares_special`
                        // at runtime. Doing so misroutes file-local-only dynamic
                        // declarations (e.g. `(defvar cconv-freevars-alist)` in
                        // cconv.el — declared special locally but not globally) to
                        // the lexenv, where they are invisible to other functions
                        // called from the let body and surface as `void-variable`.
                        let name_id = sym_id_at(constants, *idx);
                        let val = stk!().pop().unwrap_or(Value::NIL);
                        let bind_depth = self.ctx.specpdl.len();
                        // vm_try publishes the stack because specbind can run
                        // variable watchers (arbitrary Lisp).
                        vm_try!(self.ctx.try_specbind(name_id, val));
                        aux_stack.current_mut().bind_stack.push(bind_depth);
                    }
                    Op::Unbind(n) => {
                        let n = *n as usize;
                        let target = {
                            let aux = aux_stack.current_mut();
                            if n <= aux.bind_stack.len() {
                                let depth = aux.bind_stack[aux.bind_stack.len() - n];
                                aux.bind_stack.truncate(aux.bind_stack.len() - n);
                                depth
                            } else {
                                aux.bind_stack.clear();
                                0
                            }
                        };
                        // Cleanup watcher/unwind-protect exits supersede normal
                        // bytecode execution and re-enter the VM's nonlocal
                        // dispatcher, exactly like any other fallible opcode.
                        let _ = vm_try!(self.ctx.unbind_to_with_result(target, Ok(Value::NIL)));
                    }

                    // -- Function calls --
                    Op::Call(n) => {
                        let n = *n as usize;
                        let args_start = stk!().len().saturating_sub(n);
                        let stack_after_call = args_start.saturating_sub(1);
                        let func_val = if args_start > 0 {
                            stk!()[args_start - 1]
                        } else {
                            Value::NIL
                        };
                        // JIT Phase 1: record the callee for direct-call speculation.
                        // Only NAMED (symbol) callees carry a SymId; the call-site
                        // index is `pc_local - 1` (pc was advanced past Call above).
                        // GC-safe: a SymId is a stable index, never a heap pointer.
                        // Gated process-wide until a tier consumes the feedback:
                        // see `jit::call_feedback_collection_enabled`.
                        #[cfg(feature = "jit")]
                        if self.bytecode_tier_policy.records_call_feedback()
                            && crate::emacs_core::jit::call_feedback_collection_enabled()
                            && let ValueKind::Symbol(id) = func_val.kind()
                        {
                            func.jit_runtime().record_call(pc_local - 1, ops_len, id);
                        }
                        // Round-2 profiling: attribute this Op::Call to its callee
                        // symbol (the find_spec_sites entry population). Resolve a
                        // subr-object callee to its SymId so both `(f x)` (symbol
                        // callee) and a spilled subr value count the same builtin.
                        // Task-4 profiling: also record the resolution kind
                        // (closure-vs-builtin split) and the callee under its call
                        // site — the execution-weighted per-site polymorphism table.
                        #[cfg(feature = "vm-profile")]
                        {
                            if let Some(id) = match func_val.kind() {
                                ValueKind::Symbol(id) => Some(id),
                                _ => func_val.as_subr_id(),
                            } {
                                vm_profile::bump_entry(id, vm_profile::ENTRY_CALL);
                            }
                            let (site_key, kind) = self.vm_profile_classify_call(func_val);
                            vm_profile::bump_call_site(
                                func as *const ByteCodeFunction as usize,
                                (pc_local - 1) as u32,
                                site_key,
                                kind,
                            );
                        }
                        // GNU `bytecode.c:Bcall` polls `maybe_quit` before
                        // entering the callee. This is observable when bytecode
                        // sets `quit-flag` immediately before a call: the callee
                        // must not run.
                        poll_quit!();
                        // GNU `bytecode.c:795-799`: Bcall records its frame and
                        // then tests `debug_on_next_call`.  Only `Bcall` does --
                        // the inline opcodes (`Op::CallBuiltin`/
                        // `Op::CallBuiltinSym`, GNU `bytecode.c:1412-1545`) are
                        // deliberately not gated, so the arm cannot live in the
                        // shared `call_function`.  This peek cannot arm
                        // anything (it is a bare `bool`); it only steers the
                        // call off the zero-copy fast paths onto the owned
                        // route below, which takes the arm properly.
                        let debug_armed = self.ctx.debug_on_next_call_is_armed();
                        let target = self.resolve_interpreter_stack_call_target(func_val, n);
                        let writeback_names = if matches!(
                            target,
                            ResolvedStackCallTarget::Interpreter { .. }
                                | ResolvedStackCallTarget::ByteCode { .. }
                        ) {
                            // The closed target proof excludes GNU's native
                            // aset/fillarray implementations.  Bytecode may
                            // mutate a string through an explicit primitive,
                            // but the ordinary call itself needs no host-side
                            // replacement-object writeback.
                            None
                        } else if n > 0 && stk!()[args_start].is_string() {
                            self.writeback_mutating_callable_names(&func_val)
                        } else {
                            None
                        };
                        let writeback_args = writeback_names
                            .as_ref()
                            .map(|_| stk!()[args_start..].iter().copied().collect::<LispArgVec>());
                        let result = if debug_armed {
                            let args: LispArgVec = stk!()[args_start..].iter().copied().collect();
                            vm_try!(self.with_bytecode_call_depth(|vm| {
                                vm.call_function_debugged(func_val, args)
                            }))
                        } else if writeback_names.is_none() {
                            if let ResolvedStackCallTarget::Interpreter { call } = target {
                                // Iterative Bcall keeps the cursor LIVE across
                                // the whole transition (GNU keeps `top` in a
                                // register through setup_frame): no publish, no
                                // reacquire, nothing here reaches a GC safe
                                // point. The classify gate already proved the
                                // callee sealed and stack-verified.
                                if let Err(flow) = self.enter_bytecode_call_depth() {
                                    cursor.publish(self.ctx);
                                    resume_flow!(flow)
                                }
                                let prepared = call.callee();
                                let callee_code = prepared.code();
                                let callee_value = prepared.value();
                                #[cfg(debug_assertions)]
                                cursor.debug_sync_len(self.ctx);
                                // The compact span always fits: nargs comes
                                // from Op::Call(u16) and the operand index is
                                // far below the span's start bound, so the
                                // oversized fallback (which would read the
                                // stale context stack) is unreachable here.
                                let backtrace = self
                                    .ctx
                                    .push_backtrace_frame_from_bc_stack(func_val, args_start, n);
                                callers
                                    .active_mut()
                                    .save_execution_state(pc_local, osr_tried);
                                *driver_quitcounter = quitcounter;
                                let caller_depth = InterpreterDriverDepth::from_suspended_callers(
                                    callers.suspended_len(),
                                );
                                aux_stack.suspend_current(caller_depth);
                                let callee_frame = self.install_iterative_interpreter_frame(
                                    &mut cursor,
                                    PreparedInterpreterCallee::new(callee_value, callee_code),
                                    ConsumedCallOperandRootSlot::from_args_start(args_start),
                                    n,
                                );
                                callers.enter_callee(
                                    BytecodeCallContinuation {
                                        stack_after_call,
                                        backtrace,
                                    },
                                    callee_frame,
                                );
                                continue 'frame;
                            }
                            cursor.publish(self.ctx);
                            if let Err(flow) = self.enter_bytecode_call_depth() {
                                resume_flow!(flow)
                            }
                            match self
                                .dispatch_interpreter_stack_call(func_val, args_start, n, target)
                            {
                                InterpreterStackCall::Enter {
                                    callee,
                                    root_slot,
                                    nargs,
                                    backtrace,
                                } => {
                                    // Uncached ByteCode targets route through
                                    // the tier dispatcher (heat/feedback) and
                                    // may still enter iteratively; the cursor
                                    // was published for that escape, so rearm
                                    // it and take the same live-cursor
                                    // installation as the cached fast path.
                                    cursor = StackCursor::acquire(self.ctx);
                                    callers
                                        .active_mut()
                                        .save_execution_state(pc_local, osr_tried);
                                    *driver_quitcounter = quitcounter;
                                    let caller_depth =
                                        InterpreterDriverDepth::from_suspended_callers(
                                            callers.suspended_len(),
                                        );
                                    aux_stack.suspend_current(caller_depth);
                                    let callee_frame = self.install_iterative_interpreter_frame(
                                        &mut cursor,
                                        callee,
                                        root_slot,
                                        nargs,
                                    );
                                    callers.enter_callee(
                                        BytecodeCallContinuation {
                                            stack_after_call,
                                            backtrace,
                                        },
                                        callee_frame,
                                    );
                                    continue 'frame;
                                }
                                InterpreterStackCall::Complete(result) => {
                                    self.leave_bytecode_call_depth();
                                    match result {
                                        Ok(value) => {
                                            cursor = StackCursor::acquire(self.ctx);
                                            value
                                        }
                                        Err(flow) => resume_flow!(flow),
                                    }
                                }
                            }
                        } else {
                            let args: LispArgVec = stk!()[args_start..].iter().copied().collect();
                            vm_try!(self.with_bytecode_call_depth(|vm| {
                                vm.call_function(func_val, args)
                            }))
                        };
                        if let (Some((called_name, alias_target)), Some(writeback_args)) =
                            (writeback_names.as_ref(), writeback_args.as_ref())
                        {
                            let root_scope = self.ctx.save_vm_roots();
                            self.push_dynamic_vm_root(result);
                            for value in writeback_args.iter().copied() {
                                self.push_dynamic_vm_root(value);
                            }
                            self.maybe_writeback_mutating_first_arg(
                                called_name,
                                *alias_target,
                                writeback_args,
                                &result,
                            );
                            self.ctx.restore_vm_roots(root_scope);
                        }
                        stk!().truncate(stack_after_call);
                        stk_push!(result);
                    }
                    Op::Apply(n) => {
                        let n = *n as usize;
                        poll_quit!();
                        if n == 0 {
                            let stack_after_call = stk!().len().saturating_sub(1);
                            let func_val = stk!().last().copied().unwrap_or(Value::NIL);
                            let result = vm_try!(self.call_function(func_val, LispArgVec::new()));
                            stk!().truncate(stack_after_call);
                            stk_push!(result);
                        } else {
                            let args_start = stk!().len().saturating_sub(n);
                            let stack_after_call = args_start.saturating_sub(1);
                            let func_val = if args_start > 0 {
                                stk!()[args_start - 1]
                            } else {
                                Value::NIL
                            };
                            // Spread the trailing list IN PLACE on the GC-traced
                            // bc_buf: the explicit args a1..a(n-1) already sit
                            // contiguously at [args_start, args_start + n - 1);
                            // replace the list's slot with its elements (GNU
                            // Fapply builds the same contiguous spread, then
                            // funcall reads it — eval.c). list_to_vec keeps the
                            // existing dotted/circular semantics (errors -> empty
                            // spread) and its Floyd cycle detection. Checked Vec
                            // ops only: the extension deliberately lives above
                            // this frame's declared max-stack region, which
                            // nothing inspects before the call returns (handler
                            // watermarks below it truncate through it correctly
                            // on a nonlocal exit).
                            let last = stk!()[args_start + n - 1];
                            let spread = list_to_vec(&last).unwrap_or_default();
                            // The spread grows bc_buf (reserve can realloc), so it
                            // runs published; reacquire picks up the new base.
                            cursor.publish(self.ctx);
                            self.ctx.bc_buf.truncate(args_start + n - 1);
                            self.ctx.bc_buf.reserve(spread.len());
                            self.ctx.bc_buf.extend_from_slice(&spread);
                            cursor = StackCursor::acquire(self.ctx);
                            let total = n - 1 + spread.len();
                            // Writeback gate tests the first POST-spread argument
                            // (for (apply f '("str" ...)) the string comes from
                            // the spread).
                            let writeback_names = if total > 0 && stk!()[args_start].is_string() {
                                self.writeback_mutating_callable_names(&func_val)
                            } else {
                                None
                            };
                            let writeback_args: Option<LispArgVec> =
                                writeback_names.as_ref().map(|_| {
                                    stk!()[args_start..args_start + total]
                                        .iter()
                                        .copied()
                                        .collect()
                                });
                            // Same call protocol as before (traced call_function:
                            // backtrace push + generic dispatch, no depth guard,
                            // no direct-builtin fast path), in its stack-args
                            // flavor — the spread args stay rooted on bc_buf for
                            // the whole call; func_val stays rooted in its own
                            // caller slot below args_start.
                            let result =
                                vm_try!(self.call_function_from_stack_args(
                                    func_val, args_start, total, false,
                                ));
                            if let (Some((called_name, alias_target)), Some(writeback_args)) =
                                (writeback_names.as_ref(), writeback_args.as_ref())
                            {
                                let root_scope = self.ctx.save_vm_roots();
                                self.push_dynamic_vm_root(result);
                                for value in writeback_args.iter().copied() {
                                    self.push_dynamic_vm_root(value);
                                }
                                self.maybe_writeback_mutating_first_arg(
                                    called_name,
                                    *alias_target,
                                    writeback_args,
                                    &result,
                                );
                                self.ctx.restore_vm_roots(root_scope);
                            }
                            stk!().truncate(stack_after_call);
                            stk_push!(result);
                        }
                    }

                    // -- Control flow --
                    // Backward branches mirror GNU `bytecode.c:op_branch`: an
                    // unsigned byte `quitcounter` is incremented only for backward
                    // jumps, and `maybe_gc(); maybe_quit();` runs when it wraps.
                    Op::Goto(addr) => {
                        branch_to!(*addr as usize);
                    }
                    Op::GotoIfNil(addr) => {
                        let len = cursor.len;
                        debug_assert!(
                            !VERIFIED || len > 0,
                            "verified bytecode underflowed its proven stack depth"
                        );
                        if !VERIFIED && len == 0 {
                            invalid_bytecode!("goto-if-nil-empty-stack");
                        }
                        let val = unsafe { *cursor.get_unchecked(len - 1) };
                        cursor.len = len - 1;
                        if val.is_nil() {
                            branch_to!(*addr as usize);
                        }
                    }
                    Op::GotoIfNotNil(addr) => {
                        let len = cursor.len;
                        debug_assert!(
                            !VERIFIED || len > 0,
                            "verified bytecode underflowed its proven stack depth"
                        );
                        if !VERIFIED && len == 0 {
                            invalid_bytecode!("goto-if-not-nil-empty-stack");
                        }
                        let val = unsafe { *cursor.get_unchecked(len - 1) };
                        cursor.len = len - 1;
                        if val.is_truthy() {
                            branch_to!(*addr as usize);
                        }
                    }
                    Op::GotoIfNilElsePop(addr) => {
                        let len = cursor.len;
                        debug_assert!(
                            !VERIFIED || len > 0,
                            "verified bytecode underflowed its proven stack depth"
                        );
                        if !VERIFIED && len == 0 {
                            invalid_bytecode!("goto-if-nil-else-pop-empty-stack");
                        }
                        if unsafe { cursor.get_unchecked(len - 1) }.is_nil() {
                            branch_to!(*addr as usize);
                        } else {
                            cursor.len = len - 1;
                        }
                    }
                    Op::GotoIfNotNilElsePop(addr) => {
                        let len = cursor.len;
                        debug_assert!(
                            !VERIFIED || len > 0,
                            "verified bytecode underflowed its proven stack depth"
                        );
                        if !VERIFIED && len == 0 {
                            invalid_bytecode!("goto-if-not-nil-else-pop-empty-stack");
                        }
                        if unsafe { cursor.get_unchecked(len - 1) }.is_truthy() {
                            branch_to!(*addr as usize);
                        } else {
                            cursor.len = len - 1;
                        }
                    }
                    Op::Switch => {
                        let jump_table = stk!().pop().unwrap_or(Value::NIL);
                        let dispatch = stk!().pop().unwrap_or(Value::NIL);

                        if !matches!(
                            jump_table.kind(),
                            ValueKind::Veclike(VecLikeType::HashTable)
                        ) {
                            cursor.publish(self.ctx);
                            resume_flow!(signal(
                                LispCondition::WrongTypeArgument,
                                vec![Value::symbol("hash-table-p"), jump_table],
                            ))
                        }

                        let ht = jump_table.as_hash_table().unwrap();
                        let key =
                            dispatch.to_hash_key_swp(&ht.test, self.ctx.symbols_with_pos_enabled);
                        let target = ht.data.get(&key).copied();

                        if let Some(target_val) = target {
                            match target_val.kind() {
                                ValueKind::Fixnum(addr) => {
                                    pc_local = vm_try!(resolve_switch_target(func, addr));
                                }
                                _ => {
                                    vm_try!(Err(signal(
                                        LispCondition::WrongTypeArgument,
                                        vec![Value::symbol("integerp"), target_val],
                                    )));
                                }
                            }
                        }
                    }
                    Op::Return => {
                        let result = stk!().pop().unwrap_or(Value::NIL);
                        return_value!(result);
                    }
                    Op::SaveCurrentBuffer => {
                        if let Some(buffer_id) =
                            self.ctx.buffers.current_buffer().map(|buffer| buffer.id)
                        {
                            aux_stack
                                .current_mut()
                                .bind_stack
                                .push(self.ctx.specpdl.len());
                            self.ctx
                                .specpdl
                                .push(SpecBinding::SaveCurrentBuffer { buffer_id });
                        }
                    }
                    Op::SaveExcursion => {
                        if let Some(count) = self.ctx.record_save_excursion() {
                            aux_stack.current_mut().bind_stack.push(count);
                        }
                    }
                    Op::SaveRestriction => {
                        if let Some(saved) = self.ctx.buffers.save_current_restriction_state() {
                            aux_stack
                                .current_mut()
                                .bind_stack
                                .push(self.ctx.specpdl.len());
                            self.ctx.specpdl.push(SpecBinding::save_restriction(saved));
                        }
                    }

                    Op::SaveWindowExcursion => {
                        // GNU bytecode.c Bsave_window_excursion (opcode 139):
                        // Pop body form list, evaluate with Fprogn inside
                        // a real window-configuration save/restore.
                        //
                        // GNU `src/bytecode.c:945-952`:
                        //
                        //   record_unwind_protect (restore_window_configuration,
                        //                          Fcurrent_window_configuration (Qnil));
                        //   TOP = Fprogn (TOP);
                        //   unbind_to (count1, TOP);
                        //
                        // `save-some-buffers`, `map-y-or-n-p`, and other
                        // byte-compiled Lisp still rely on this obsolete opcode.
                        // Evaluating the body without restoring the window
                        // configuration leaves minibuffer/window state corrupted.
                        let body = stk!().pop().unwrap_or(Value::NIL);
                        let progn_form = Value::cons(Value::symbol("progn"), body);
                        let saved = vm_try!(
                            crate::emacs_core::builtins::SavedWindowConfiguration::capture(
                                self.ctx,
                                Value::NIL,
                            )
                        );
                        // GNU records the restore on the specpdl before evaluating
                        // the body.  Use the same typed native-unwind action as the
                        // minibuffer lifecycle, so a new Rust `?`/flow path cannot
                        // bypass restoration.
                        cursor.publish(self.ctx);
                        let root_scope = self.ctx.save_vm_roots();
                        self.push_dynamic_vm_root(progn_form);
                        let body_result = self.ctx.with_unwind_scope(|ctx| {
                        ctx.record_native_unwind(
                            crate::emacs_core::eval::NativeUnwindAction::RestoreWindowConfiguration {
                                configuration: saved,
                                options: crate::emacs_core::builtins::WindowConfigurationRestoreOptions::default(),
                            },
                        );
                        ctx.eval_sub(progn_form)
                    });
                        self.ctx.restore_vm_roots(root_scope);
                        cursor = StackCursor::acquire(self.ctx);

                        match body_result {
                            Ok(result) => {
                                stk_push!(result);
                            }
                            Err(flow) => {
                                cursor.publish(self.ctx);
                                resume_flow!(flow)
                            }
                        }
                    }

                    // -- Arithmetic --
                    // Inline fixnum fast paths match GNU Emacs bytecode.c design:
                    // the bytecode opcode IS the contract — no override check needed.
                    Op::Add => {
                        let fallback = {
                            let len = cursor.len;
                            if len < 2 {
                                invalid_bytecode!("add-stack-underflow");
                            }
                            let b = unsafe { *cursor.get_unchecked(len - 1) };
                            let a = unsafe { *cursor.get_unchecked(len - 2) };
                            if a.is_fixnum() && b.is_fixnum() {
                                let av = a.xfixnum();
                                let bv = b.xfixnum();
                                let res = av + bv;
                                if (Value::MOST_NEGATIVE_FIXNUM..=Value::MOST_POSITIVE_FIXNUM)
                                    .contains(&res)
                                {
                                    unsafe {
                                        *cursor.get_unchecked_mut(len - 2) = Value::fixnum(res);
                                    }
                                    cursor.len = len - 1;
                                    None
                                } else {
                                    cursor.len = len - 2;
                                    Some((a, b))
                                }
                            } else {
                                cursor.len = len - 2;
                                Some((a, b))
                            }
                        };
                        if let Some((a, b)) = fallback {
                            let result =
                                vm_try!(self.dispatch_vm_builtin_with_frame(func, "+", vec![a, b]));
                            stk_push!(result);
                        }
                    }
                    Op::Sub => {
                        let len = stk!().len();
                        let b = stk!()[len - 1];
                        let a = stk!()[len - 2];
                        if a.is_fixnum() && b.is_fixnum() {
                            let av = a.xfixnum();
                            let bv = b.xfixnum();
                            let res = av - bv;
                            if (Value::MOST_NEGATIVE_FIXNUM..=Value::MOST_POSITIVE_FIXNUM)
                                .contains(&res)
                            {
                                stk!()[len - 2] = Value::fixnum(res);
                                stk!().pop();
                            } else {
                                stk!().truncate(len - 2);
                                let result = vm_try!(self.dispatch_vm_builtin_with_frame(
                                    func,
                                    "-",
                                    vec![a, b]
                                ));
                                stk_push!(result);
                            }
                        } else {
                            stk!().truncate(len - 2);
                            let result =
                                vm_try!(self.dispatch_vm_builtin_with_frame(func, "-", vec![a, b]));
                            stk_push!(result);
                        }
                    }
                    Op::Mul => {
                        let len = stk!().len();
                        let b = stk!()[len - 1];
                        let a = stk!()[len - 2];
                        if a.is_fixnum() && b.is_fixnum() {
                            let av = a.xfixnum();
                            let bv = b.xfixnum();
                            if let Some(res) = av.checked_mul(bv) {
                                if (Value::MOST_NEGATIVE_FIXNUM..=Value::MOST_POSITIVE_FIXNUM)
                                    .contains(&res)
                                {
                                    stk!()[len - 2] = Value::fixnum(res);
                                    stk!().pop();
                                } else {
                                    stk!().truncate(len - 2);
                                    let result = vm_try!(self.dispatch_vm_builtin_with_frame(
                                        func,
                                        "*",
                                        vec![a, b]
                                    ));
                                    stk_push!(result);
                                }
                            } else {
                                stk!().truncate(len - 2);
                                let result = vm_try!(self.dispatch_vm_builtin_with_frame(
                                    func,
                                    "*",
                                    vec![a, b]
                                ));
                                stk_push!(result);
                            }
                        } else {
                            stk!().truncate(len - 2);
                            let result =
                                vm_try!(self.dispatch_vm_builtin_with_frame(func, "*", vec![a, b]));
                            stk_push!(result);
                        }
                    }
                    Op::Div => {
                        let len = stk!().len();
                        let b = stk!()[len - 1];
                        let a = stk!()[len - 2];
                        if a.is_fixnum() && b.is_fixnum() {
                            let av = a.xfixnum();
                            let bv = b.xfixnum();
                            // The range check matters: most-negative-fixnum / -1
                            // exceeds most-positive-fixnum and must promote to a
                            // bignum via the builtin, like GNU.
                            if bv != 0 && !(av == Value::MOST_NEGATIVE_FIXNUM && bv == -1) {
                                // Emacs truncation division (towards zero), matching C semantics
                                let res = av / bv;
                                stk!()[len - 2] = Value::fixnum(res);
                                stk!().pop();
                            } else {
                                stk!().truncate(len - 2);
                                let result = vm_try!(self.dispatch_vm_builtin_with_frame(
                                    func,
                                    "/",
                                    vec![a, b]
                                ));
                                stk_push!(result);
                            }
                        } else {
                            stk!().truncate(len - 2);
                            let result =
                                vm_try!(self.dispatch_vm_builtin_with_frame(func, "/", vec![a, b]));
                            stk_push!(result);
                        }
                    }
                    Op::Rem => {
                        let len = stk!().len();
                        let b = stk!()[len - 1];
                        let a = stk!()[len - 2];
                        if a.is_fixnum() && b.is_fixnum() {
                            let av = a.xfixnum();
                            let bv = b.xfixnum();
                            if bv != 0 {
                                stk!()[len - 2] = Value::fixnum(av % bv);
                                stk!().pop();
                            } else {
                                stk!().truncate(len - 2);
                                let result = vm_try!(self.dispatch_vm_builtin_with_frame(
                                    func,
                                    "%",
                                    vec![a, b]
                                ));
                                stk_push!(result);
                            }
                        } else {
                            stk!().truncate(len - 2);
                            let result =
                                vm_try!(self.dispatch_vm_builtin_with_frame(func, "%", vec![a, b]));
                            stk_push!(result);
                        }
                    }
                    Op::Add1 => {
                        let fallback = {
                            let len = cursor.len;
                            if len == 0 {
                                invalid_bytecode!("add1-empty-stack");
                            }
                            let top = unsafe { *cursor.get_unchecked(len - 1) };
                            if top.is_fixnum() {
                                let n = top.xfixnum();
                                if n != Value::MOST_POSITIVE_FIXNUM {
                                    unsafe {
                                        *cursor.get_unchecked_mut(len - 1) = Value::fixnum(n + 1);
                                    }
                                    None
                                } else {
                                    cursor.len = len - 1;
                                    Some(top)
                                }
                            } else {
                                cursor.len = len - 1;
                                Some(top)
                            }
                        };
                        if let Some(top) = fallback {
                            let result =
                                vm_try!(self.dispatch_vm_builtin_with_frame(func, "1+", vec![top]));
                            stk_push!(result);
                        }
                    }
                    Op::Sub1 => {
                        let top = *stk!().last().unwrap();
                        if top.is_fixnum() {
                            let n = top.xfixnum();
                            if n != Value::MOST_NEGATIVE_FIXNUM {
                                *stk!().last_mut().unwrap() = Value::fixnum(n - 1);
                            } else {
                                stk!().pop();
                                let result = vm_try!(self.dispatch_vm_builtin_with_frame(
                                    func,
                                    "1-",
                                    vec![top]
                                ));
                                stk_push!(result);
                            }
                        } else {
                            stk!().pop();
                            let result =
                                vm_try!(self.dispatch_vm_builtin_with_frame(func, "1-", vec![top]));
                            stk_push!(result);
                        }
                    }
                    Op::Negate => {
                        let top = *stk!().last().unwrap();
                        if top.is_fixnum() {
                            let n = top.xfixnum();
                            if n != Value::MOST_NEGATIVE_FIXNUM {
                                *stk!().last_mut().unwrap() = Value::fixnum(-n);
                            } else {
                                stk!().pop();
                                let result = vm_try!(self.dispatch_vm_builtin_with_frame(
                                    func,
                                    "-",
                                    vec![top]
                                ));
                                stk_push!(result);
                            }
                        } else {
                            stk!().pop();
                            let result =
                                vm_try!(self.dispatch_vm_builtin_with_frame(func, "-", vec![top]));
                            stk_push!(result);
                        }
                    }

                    // -- Comparison --
                    // Inline fixnum fast paths match GNU Emacs bytecode.c.
                    Op::Eqlsign => {
                        let len = stk!().len();
                        let b = stk!()[len - 1];
                        let a = stk!()[len - 2];
                        if a.is_fixnum() && b.is_fixnum() {
                            stk!()[len - 2] = if a.0 == b.0 { Value::T } else { Value::NIL };
                            stk!().pop();
                        } else {
                            stk!().truncate(len - 2);
                            let result =
                                vm_try!(self.dispatch_vm_builtin_with_frame(func, "=", vec![a, b]));
                            stk_push!(result);
                        }
                    }
                    Op::Gtr => {
                        let len = stk!().len();
                        let b = stk!()[len - 1];
                        let a = stk!()[len - 2];
                        if a.is_fixnum() && b.is_fixnum() {
                            stk!()[len - 2] = if fixnum_gt(a, b) {
                                Value::T
                            } else {
                                Value::NIL
                            };
                            stk!().pop();
                        } else {
                            stk!().truncate(len - 2);
                            let result =
                                vm_try!(self.dispatch_vm_builtin_with_frame(func, ">", vec![a, b]));
                            stk_push!(result);
                        }
                    }
                    Op::Lss => {
                        let fallback = {
                            let len = cursor.len;
                            if len < 2 {
                                invalid_bytecode!("lss-stack-underflow");
                            }
                            let b = unsafe { *cursor.get_unchecked(len - 1) };
                            let a = unsafe { *cursor.get_unchecked(len - 2) };
                            if a.is_fixnum() && b.is_fixnum() {
                                unsafe {
                                    *cursor.get_unchecked_mut(len - 2) = if fixnum_lt(a, b) {
                                        Value::T
                                    } else {
                                        Value::NIL
                                    };
                                }
                                cursor.len = len - 1;
                                None
                            } else {
                                cursor.len = len - 2;
                                Some((a, b))
                            }
                        };
                        if let Some((a, b)) = fallback {
                            let result =
                                vm_try!(self.dispatch_vm_builtin_with_frame(func, "<", vec![a, b]));
                            stk_push!(result);
                        }
                    }
                    Op::Leq => {
                        let len = stk!().len();
                        let b = stk!()[len - 1];
                        let a = stk!()[len - 2];
                        if a.is_fixnum() && b.is_fixnum() {
                            stk!()[len - 2] = if fixnum_le(a, b) {
                                Value::T
                            } else {
                                Value::NIL
                            };
                            stk!().pop();
                        } else {
                            stk!().truncate(len - 2);
                            let result = vm_try!(self.dispatch_vm_builtin_with_frame(
                                func,
                                "<=",
                                vec![a, b]
                            ));
                            stk_push!(result);
                        }
                    }
                    Op::Geq => {
                        let len = stk!().len();
                        let b = stk!()[len - 1];
                        let a = stk!()[len - 2];
                        if a.is_fixnum() && b.is_fixnum() {
                            stk!()[len - 2] = if fixnum_ge(a, b) {
                                Value::T
                            } else {
                                Value::NIL
                            };
                            stk!().pop();
                        } else {
                            stk!().truncate(len - 2);
                            let result = vm_try!(self.dispatch_vm_builtin_with_frame(
                                func,
                                ">=",
                                vec![a, b]
                            ));
                            stk_push!(result);
                        }
                    }
                    Op::Max => {
                        let len = stk!().len();
                        let b = stk!()[len - 1];
                        let a = stk!()[len - 2];
                        if a.is_fixnum() && b.is_fixnum() {
                            stk!()[len - 2] = if fixnum_ge(a, b) { a } else { b };
                            stk!().pop();
                        } else {
                            stk!().truncate(len - 2);
                            let result = vm_try!(self.dispatch_vm_builtin_with_frame(
                                func,
                                "max",
                                vec![a, b]
                            ));
                            stk_push!(result);
                        }
                    }
                    Op::Min => {
                        let len = stk!().len();
                        let b = stk!()[len - 1];
                        let a = stk!()[len - 2];
                        if a.is_fixnum() && b.is_fixnum() {
                            stk!()[len - 2] = if fixnum_le(a, b) { a } else { b };
                            stk!().pop();
                        } else {
                            stk!().truncate(len - 2);
                            let result = vm_try!(self.dispatch_vm_builtin_with_frame(
                                func,
                                "min",
                                vec![a, b]
                            ));
                            stk_push!(result);
                        }
                    }

                    // -- List operations --
                    // Inline car/cdr/car-safe/cdr-safe match GNU Emacs exactly:
                    // direct cons field access, nil passthrough, error on wrong type.
                    Op::Car => {
                        let top = stk!().last_mut().unwrap();
                        if top.is_cons() {
                            *top = top.cons_car();
                        } else if !top.is_nil() {
                            let val = *top;
                            stk!().pop();
                            vm_try!(Err(signal(
                                LispCondition::WrongTypeArgument,
                                vec![Value::symbol("listp"), val]
                            )));
                        }
                        // nil → nil: no change needed
                    }
                    Op::Cdr => {
                        let top = stk!().last_mut().unwrap();
                        if top.is_cons() {
                            *top = top.cons_cdr();
                        } else if !top.is_nil() {
                            let val = *top;
                            stk!().pop();
                            vm_try!(Err(signal(
                                LispCondition::WrongTypeArgument,
                                vec![Value::symbol("listp"), val]
                            )));
                        }
                    }
                    Op::CarSafe => {
                        let top = stk!().last_mut().unwrap();
                        *top = if top.is_cons() {
                            top.cons_car()
                        } else {
                            Value::NIL
                        };
                    }
                    Op::CdrSafe => {
                        let top = stk!().last_mut().unwrap();
                        *top = if top.is_cons() {
                            top.cons_cdr()
                        } else {
                            Value::NIL
                        };
                    }
                    Op::Cons => {
                        let len = stk!().len();
                        let cdr_val = stk!()[len - 1];
                        let car_val = stk!()[len - 2];
                        // The Context owns the heap: allocate through it
                        // directly instead of Value::cons's thread-local
                        // heap lookup (a TLS access per cons on the hottest
                        // allocation opcode).
                        stk!()[len - 2] = self.ctx.tagged_heap.alloc_cons(car_val, cdr_val);
                        stk!().pop();
                    }
                    Op::List(n) => {
                        let n = *n as usize;
                        let start = stk!().len().saturating_sub(n);
                        // GNU bytecode.c:BlistN keeps operands on the bytecode
                        // stack and calls Flist(n, &TOP).  Keep the same stack
                        // rooting discipline here and build from the live slice.
                        let result = Value::list_from_slice(&stk!()[start..]);
                        stk!().truncate(start);
                        stk_push!(result);
                    }
                    Op::Length => {
                        let len = stk!().len();
                        let val = stk!()[len - 1];
                        let result = vm_try!(builtins::builtin_length_1(&mut *self.ctx, val));
                        stk!()[len - 1] = result;
                    }
                    Op::Nth => {
                        let len = stk!().len();
                        let n = stk!()[len - 2];
                        let list = stk!()[len - 1];
                        let result = vm_try!(builtins::builtin_nth_2(&mut *self.ctx, n, list));
                        stk!()[len - 2] = result;
                        stk!().pop();
                    }
                    Op::Nthcdr => {
                        let len = stk!().len();
                        let n = stk!()[len - 2];
                        let list = stk!()[len - 1];
                        let result = vm_try!(builtins::builtin_nthcdr_2(&mut *self.ctx, n, list));
                        stk!()[len - 2] = result;
                        stk!().pop();
                    }
                    Op::Elt => {
                        let len = stk!().len();
                        let seq = stk!()[len - 2];
                        let idx = stk!()[len - 1];
                        let result = vm_try!(builtins::builtin_elt_2(&mut *self.ctx, seq, idx));
                        stk!()[len - 2] = result;
                        stk!().pop();
                    }
                    Op::Setcar => {
                        let len = stk!().len();
                        let cell = stk!()[len - 2];
                        let newcar = stk!()[len - 1];
                        let result =
                            vm_try!(builtins::builtin_setcar_2(&mut *self.ctx, cell, newcar));
                        stk!()[len - 2] = result;
                        stk!().pop();
                    }
                    Op::Setcdr => {
                        let len = stk!().len();
                        let cell = stk!()[len - 2];
                        let newcdr = stk!()[len - 1];
                        let result =
                            vm_try!(builtins::builtin_setcdr_2(&mut *self.ctx, cell, newcdr));
                        stk!()[len - 2] = result;
                        stk!().pop();
                    }
                    Op::Nconc => {
                        let start = stk!().len().saturating_sub(2);
                        let result =
                            vm_try_pure!(builtins::builtin_nconc_slice_values(&stk!()[start..]));
                        stk!().truncate(start);
                        stk_push!(result);
                    }
                    Op::Nreverse => {
                        let len = stk!().len();
                        let value = stk!()[len - 1];
                        let result = vm_try!(builtins::builtin_nreverse_1(&mut *self.ctx, value));
                        stk!()[len - 1] = result;
                    }
                    Op::Member => {
                        let len = stk!().len();
                        let elt = stk!()[len - 2];
                        let list = stk!()[len - 1];
                        let result = vm_try!(builtins::builtin_member_2(&mut *self.ctx, elt, list));
                        stk!()[len - 2] = result;
                        stk!().pop();
                    }
                    Op::Memq => {
                        let len = stk!().len();
                        let elt = stk!()[len - 2];
                        let list = stk!()[len - 1];
                        let result = vm_try!(builtins::builtin_memq_2(&mut *self.ctx, elt, list));
                        stk!()[len - 2] = result;
                        stk!().pop();
                    }
                    Op::Assq => {
                        let len = stk!().len();
                        let key = stk!()[len - 2];
                        let alist = stk!()[len - 1];
                        let result = vm_try!(builtins::builtin_assq_2(&mut *self.ctx, key, alist));
                        stk!()[len - 2] = result;
                        stk!().pop();
                    }

                    // -- Type predicates --
                    // -- Type predicates --
                    // Pure inline tag checks, zero function calls. Matches GNU exactly.
                    Op::Symbolp => {
                        let top = stk!().last_mut().unwrap();
                        let is_sym = top.is_symbol()
                            || (self.ctx.symbols_with_pos_enabled && top.is_symbol_with_pos());
                        *top = if is_sym { Value::T } else { Value::NIL };
                    }
                    Op::Consp => {
                        let top = stk!().last_mut().unwrap();
                        *top = if top.is_cons() { Value::T } else { Value::NIL };
                    }
                    Op::Stringp => {
                        let top = stk!().last_mut().unwrap();
                        *top = if top.is_string() {
                            Value::T
                        } else {
                            Value::NIL
                        };
                    }
                    Op::Listp => {
                        let top = stk!().last_mut().unwrap();
                        *top = if top.is_cons() || top.is_nil() {
                            Value::T
                        } else {
                            Value::NIL
                        };
                    }
                    Op::Integerp => {
                        let top = stk!().last_mut().unwrap();
                        *top = if top.is_integer() {
                            Value::T
                        } else {
                            Value::NIL
                        };
                    }
                    Op::Numberp => {
                        let top = stk!().last_mut().unwrap();
                        *top = if top.is_number() {
                            Value::T
                        } else {
                            Value::NIL
                        };
                    }
                    Op::Null | Op::Not => {
                        let top = stk!().last_mut().unwrap();
                        *top = if top.is_nil() { Value::T } else { Value::NIL };
                    }
                    Op::Eq => {
                        let len = stk!().len();
                        let b = stk!()[len - 1];
                        let a = stk!()[len - 2];
                        let result = if a.0 == b.0 {
                            true
                        } else if self.ctx.symbols_with_pos_enabled {
                            crate::emacs_core::value::eq_value_swp(&a, &b, true)
                        } else {
                            false
                        };
                        stk!()[len - 2] = if result { Value::T } else { Value::NIL };
                        stk!().pop();
                    }
                    Op::Equal => {
                        let len = stk!().len();
                        let a = stk!()[len - 2];
                        let b = stk!()[len - 1];
                        let result = vm_try!(builtins::builtin_equal_2(&mut *self.ctx, a, b));
                        stk!()[len - 2] = result;
                        stk!().pop();
                    }

                    // -- String operations --
                    Op::Concat(n) => {
                        let n = *n as usize;
                        let start = stk!().len().saturating_sub(n);
                        // GNU bytecode.c:BconcatN passes the stack slice directly
                        // to Fconcat instead of materializing an argument vector.
                        let result = vm_try_pure!(builtins::builtin_concat_slice(&stk!()[start..]));
                        stk!().truncate(start);
                        stk_push!(result);
                    }
                    Op::Substring => {
                        let start = stk!().len().saturating_sub(3);
                        let result =
                            vm_try_pure!(builtins::builtin_substring_slice(&stk!()[start..]));
                        stk!().truncate(start);
                        stk_push!(result);
                    }
                    Op::StringEqual => {
                        let len = stk!().len();
                        let a = stk!()[len - 2];
                        let b = stk!()[len - 1];
                        let result =
                            vm_try!(builtins::builtin_string_equal_2(&mut *self.ctx, a, b));
                        stk!()[len - 2] = result;
                        stk!().pop();
                    }
                    Op::StringLessp => {
                        let len = stk!().len();
                        let a = stk!()[len - 2];
                        let b = stk!()[len - 1];
                        let result =
                            vm_try!(builtins::builtin_string_lessp_2(&mut *self.ctx, a, b));
                        stk!()[len - 2] = result;
                        stk!().pop();
                    }

                    // -- Vector operations --
                    Op::Aref => {
                        let len = stk!().len();
                        let array = stk!()[len - 2];
                        let index = stk!()[len - 1];
                        let result =
                            vm_try!(builtins::builtin_aref_2(&mut *self.ctx, array, index));
                        stk!()[len - 2] = result;
                        stk!().pop();
                    }
                    Op::Aset => {
                        let val = stk!().pop().unwrap_or(Value::NIL);
                        let idx_val = stk!().pop().unwrap_or(Value::fixnum(0));
                        let vec_val = stk!().pop().unwrap_or(Value::NIL);
                        let mut call_args = LispArgVec::new();
                        call_args.push(vec_val);
                        call_args.push(idx_val);
                        call_args.push(val);
                        let result = if let Some(result) = vm_try!(
                            self.maybe_call_named_function_cell(func, "aset", call_args.clone(),)
                        ) {
                            result
                        } else {
                            vm_try!(builtins::builtin_aset(call_args.clone().into_vec()))
                        };
                        let root_scope = self.ctx.save_vm_roots();
                        self.push_dynamic_vm_root(result);
                        for value in call_args.iter().copied() {
                            self.push_dynamic_vm_root(value);
                        }
                        self.maybe_writeback_mutating_first_arg("aset", None, &call_args, &result);
                        self.ctx.restore_vm_roots(root_scope);
                        stk_push!(result);
                    }

                    // -- Symbol operations --
                    Op::SymbolValue => {
                        let len = stk!().len();
                        let sym = stk!()[len - 1];
                        let result = vm_try!(builtins::builtin_symbol_value_1(&mut *self.ctx, sym));
                        stk!()[len - 1] = result;
                    }
                    Op::SymbolFunction => {
                        let len = stk!().len();
                        let sym = stk!()[len - 1];
                        let result =
                            vm_try!(builtins::builtin_symbol_function_1(&mut *self.ctx, sym));
                        stk!()[len - 1] = result;
                    }
                    Op::Set => {
                        let len = stk!().len();
                        let sym = stk!()[len - 2];
                        let val = stk!()[len - 1];
                        let result = vm_try!(builtins::builtin_set_2(&mut *self.ctx, sym, val));
                        stk!()[len - 2] = result;
                        stk!().pop();
                    }
                    Op::Fset => {
                        let len = stk!().len();
                        let sym = stk!()[len - 2];
                        let val = stk!()[len - 1];
                        let result = vm_try!(builtins::builtin_fset_2(&mut *self.ctx, sym, val));
                        stk!()[len - 2] = result;
                        stk!().pop();
                    }
                    Op::Get => {
                        let len = stk!().len();
                        let sym = stk!()[len - 2];
                        let prop = stk!()[len - 1];
                        let result = vm_try!(builtins::builtin_get_2(&mut *self.ctx, sym, prop));
                        stk!()[len - 2] = result;
                        stk!().pop();
                    }
                    Op::Put => {
                        let len = stk!().len();
                        let sym = stk!()[len - 3];
                        let prop = stk!()[len - 2];
                        let val = stk!()[len - 1];
                        let result =
                            vm_try!(builtins::builtin_put_3(&mut *self.ctx, sym, prop, val));
                        stk!().truncate(len - 3);
                        stk_push!(result);
                    }

                    // -- Error handling --
                    Op::PushConditionCase(target) => {
                        let stack_len = stk!().len();
                        let spec_depth = self.ctx.specpdl.len();
                        let bsl = aux_stack.current_mut().bind_stack.len();
                        let resume_id = self.ctx.allocate_resume_id();
                        aux_stack.current_mut().handlers.push(Handler::Condition);
                        self.ctx
                            .push_condition_frame(ConditionFrame::ConditionCase {
                                conditions: Value::symbol("error"),
                                resume: ResumeTarget::VmConditionCase {
                                    resume_id,
                                    target: *target,
                                    stack_len,
                                    spec_depth,
                                    bind_stack_len: bsl,
                                },
                            });
                    }
                    Op::PushConditionCaseRaw(target) => {
                        // GNU bytecode consumes the handler pattern operand from TOS.
                        let conditions = stk!().pop().unwrap_or(Value::NIL);
                        let stack_len = stk!().len();
                        let spec_depth = self.ctx.specpdl.len();
                        let bsl = aux_stack.current_mut().bind_stack.len();
                        let resume_id = self.ctx.allocate_resume_id();
                        aux_stack.current_mut().handlers.push(Handler::Condition);
                        self.ctx
                            .push_condition_frame(ConditionFrame::ConditionCase {
                                conditions,
                                resume: ResumeTarget::VmConditionCase {
                                    resume_id,
                                    target: *target,
                                    stack_len,
                                    spec_depth,
                                    bind_stack_len: bsl,
                                },
                            });
                    }
                    Op::PushCatch(target) => {
                        let tag = stk!().pop().unwrap_or(Value::NIL);
                        let stack_len = stk!().len();
                        let spec_depth = self.ctx.specpdl.len();
                        let bsl = aux_stack.current_mut().bind_stack.len();
                        let resume_id = self.ctx.allocate_resume_id();
                        aux_stack.current_mut().handlers.push(Handler::Condition);
                        self.ctx.push_condition_frame(ConditionFrame::Catch {
                            tag,
                            resume: ResumeTarget::VmCatch {
                                resume_id,
                                target: *target,
                                stack_len,
                                spec_depth,
                                bind_stack_len: bsl,
                            },
                        });
                    }
                    Op::PopHandler => {
                        if aux_stack.current_mut().handlers.pop().is_some() {
                            self.ctx.pop_condition_frame();
                        }
                    }
                    Op::UnwindProtectPop => {
                        let cleanup = stk!().pop().unwrap_or(Value::NIL);
                        aux_stack
                            .current_mut()
                            .bind_stack
                            .push(self.ctx.specpdl.len());
                        self.ctx.specpdl.push(SpecBinding::UnwindProtect {
                            forms: cleanup,
                            lexenv: self.ctx.lexenv,
                        });
                    }
                    Op::Throw => {
                        let val = stk!().pop().unwrap_or(Value::NIL);
                        let tag = stk!().pop().unwrap_or(Value::NIL);
                        cursor.publish(self.ctx);
                        resume_flow!(Flow::throw(tag, val))
                    }

                    // -- Closure --
                    Op::MakeClosure(idx) => {
                        let val = constants[*idx as usize];
                        if let Some(bc_data) = val.get_bytecode_data() {
                            let mut closure = bc_data.clone();
                            closure.env = Some(self.ctx.lexenv);
                            stk_push!(Value::make_bytecode(closure));
                        } else {
                            stk_push!(val);
                        }
                    }

                    // -- Builtin escape hatch --
                    Op::CallBuiltin(name_idx, n) => {
                        let name_id = sym_id_at(constants, *name_idx);
                        #[cfg(feature = "vm-profile")]
                        vm_profile::bump_entry(name_id, vm_profile::ENTRY_CALLBUILTIN);
                        let n = *n as usize;
                        let args_start = stk!().len().saturating_sub(n);
                        let writeback_args = (stk!()
                            .get(args_start)
                            .is_some_and(|value| value.is_string())
                            && Self::mutates_first_arg_sym(name_id))
                        .then(|| stk!()[args_start..].iter().copied().collect::<LispArgVec>());
                        let result = if self.named_builtin_fast_path_allowed_id(name_id) {
                            vm_try!(
                                self.dispatch_vm_builtin_by_id_from_stack(
                                    func, name_id, args_start, n
                                )
                            )
                        } else {
                            let args: LispArgVec = stk!()[args_start..].iter().copied().collect();
                            let func_val = Value::from_sym_id(name_id);
                            vm_try!(
                                self.with_frame_call_roots(func, func_val, args, |vm, args| {
                                    vm.call_function(func_val, args)
                                })
                            )
                        };
                        if let Some(writeback_args) = writeback_args.as_ref() {
                            let root_scope = self.ctx.save_vm_roots();
                            self.push_dynamic_vm_root(result);
                            for value in writeback_args.iter().copied() {
                                self.push_dynamic_vm_root(value);
                            }
                            self.maybe_writeback_mutating_first_arg(
                                resolve_sym(name_id),
                                None,
                                writeback_args,
                                &result,
                            );
                            self.ctx.restore_vm_roots(root_scope);
                        }
                        stk!().truncate(args_start);
                        stk_push!(result);
                        poll_quit!();
                    }
                    // Mirrors GNU bytecode.c inline dispatch of opcodes
                    // 0140-0177 etc. — the symbol name is encoded in the
                    // op, no constants-pool lookup.
                    Op::CallBuiltinSym(sym, n) => {
                        #[cfg(feature = "vm-profile")]
                        vm_profile::bump_entry(*sym, vm_profile::ENTRY_CALLBUILTINSYM);
                        let n = *n as usize;
                        let args_start = stk!().len().saturating_sub(n);
                        let result =
                            vm_try!(self.dispatch_call_builtin_sym(func, *sym, args_start, n));
                        stk!().truncate(args_start);
                        stk_push!(result);
                        poll_quit!();
                    }
                }
            }

            // No fall-off tail: `seal_ops` guarantees the dispatch loop above
            // only exits through an explicit control transfer (Return, error
            // flow, or frame transition).
        }
    }

    // -- Helper methods --

    #[inline(always)]
    fn mutates_first_arg_name(name: &str) -> bool {
        name == "fillarray" || name == "aset"
    }

    /// Same predicate as [`Self::mutates_first_arg_name`] as one SymId
    /// compare — the per-call classification below must not resolve and
    /// string-compare symbol names on the hot path.
    #[inline(always)]
    fn mutates_first_arg_sym(id: SymId) -> bool {
        id == fillarray_sym_id() || id == aset_sym_id()
    }

    #[inline]
    fn writeback_mutating_callable_names(
        &self,
        func_val: &Value,
    ) -> Option<(&'static str, Option<&'static str>)> {
        #[cfg(test)]
        MUTATING_WRITEBACK_CLASSIFICATION_COUNT.with(|count| count.set(count.get() + 1));
        match func_val.kind() {
            ValueKind::Subr(_) | ValueKind::Veclike(VecLikeType::Subr)
                if func_val.as_subr_id().is_some() =>
            {
                let id = func_val.as_subr_id().unwrap();
                Self::mutates_first_arg_sym(id).then(|| (resolve_sym(id), None))
            }
            ValueKind::Symbol(id) => {
                if Self::mutates_first_arg_sym(id) {
                    return Some((resolve_sym(id), None));
                }
                let alias_target =
                    self.ctx
                        .obarray
                        .symbol_function_id(id)
                        .and_then(|bound| match bound.kind() {
                            ValueKind::Symbol(tid) => {
                                Self::mutates_first_arg_sym(tid).then(|| resolve_sym(tid))
                            }
                            ValueKind::Subr(_) | ValueKind::Veclike(VecLikeType::Subr) => {
                                let tid = bound.as_subr_id().unwrap();
                                Self::mutates_first_arg_sym(tid).then(|| resolve_sym(tid))
                            }
                            _ => None,
                        });
                alias_target.map(|target| (resolve_sym(id), Some(target)))
            }
            _ => None,
        }
    }

    fn builtin_name_id(name: &str) -> SymId {
        lookup_interned(name).unwrap_or_else(|| intern(name))
    }

    fn named_builtin_fast_path_allowed_id(&self, id: SymId) -> bool {
        if self.ctx.compiler_function_overrides_active() {
            return false;
        }
        match self.ctx.obarray.symbol_function_id(id) {
            Some(val) => match val.kind() {
                ValueKind::Subr(_) | ValueKind::Veclike(VecLikeType::Subr) => {
                    val.as_subr_id() == Some(id)
                }
                ValueKind::Nil => true,
                _ => false,
            },
            None => true,
        }
    }

    fn maybe_call_named_function_cell(
        &mut self,
        func: &ByteCodeFunction,
        name: &str,
        args: LispArgVec,
    ) -> Result<Option<Value>, Flow> {
        let id = Self::builtin_name_id(name);
        if self.named_builtin_fast_path_allowed_id(id) {
            return Ok(None);
        }

        let func_val = Value::from_sym_id(id);
        self.with_frame_call_roots(func, func_val, args, |vm, args| {
            vm.call_function(func_val, args)
        })
        .map(Some)
    }

    fn maybe_writeback_mutating_first_arg(
        &mut self,
        called_name: &str,
        alias_target: Option<&str>,
        call_args: &[Value],
        result: &Value,
    ) {
        let mutates_fillarray =
            called_name == "fillarray" || alias_target.is_some_and(|name| name == "fillarray");
        let mutates_aset = called_name == "aset" || alias_target.is_some_and(|name| name == "aset");
        if !mutates_fillarray && !mutates_aset {
            return;
        }

        let Some(first_arg) = call_args.first() else {
            return;
        };
        if !first_arg.is_string() {
            return;
        }

        let replacement = if mutates_fillarray {
            if !result.is_string() || eq_value(first_arg, result) {
                return;
            }
            *result
        } else {
            if call_args.len() < 3 {
                return;
            }
            let Ok(updated) =
                builtins::aset_string_replacement(first_arg, &call_args[1], &call_args[2])
            else {
                return;
            };
            if eq_value(first_arg, &updated) {
                return;
            }
            updated
        };

        if crate::emacs_core::value::equal_value(first_arg, &replacement, 0) {
            return;
        }

        let mut visited = HashSet::new();
        for value in self.ctx.bc_buf.iter_mut() {
            Self::replace_alias_refs_in_value(value, first_arg, &replacement, &mut visited);
        }
        // Walk the lexenv cons alist and replace alias refs in binding values
        {
            let mut lexenv_val = self.ctx.lexenv;
            Self::replace_alias_refs_in_value(
                &mut lexenv_val,
                first_arg,
                &replacement,
                &mut visited,
            );
            self.ctx.lexenv = lexenv_val;
        }
        // dynamic stack removed — specbind writes directly to obarray
        if let Some(current_id) = self.ctx.buffers.current_buffer_id()
            && let Some(buf) = self.ctx.buffers.get_mut(current_id)
        {
            for value in buf.bound_buffer_local_values_mut() {
                Self::replace_alias_refs_in_value(value, first_arg, &replacement, &mut visited);
            }
        }

        self.ctx.obarray.for_each_value_cell_mut(|value| {
            Self::replace_alias_refs_in_value(value, first_arg, &replacement, &mut visited);
        });
    }

    fn replace_alias_refs_in_value(
        value: &mut Value,
        from: &Value,
        to: &Value,
        visited: &mut HashSet<usize>,
    ) {
        if eq_value(value, from) {
            *value = *to;
            return;
        }

        match value.kind() {
            ValueKind::Cons => {
                let key = value.bits() ^ 0x1;
                if !visited.insert(key) {
                    return;
                }
                let mut new_car = value.cons_car();
                let mut new_cdr = value.cons_cdr();
                Self::replace_alias_refs_in_value(&mut new_car, from, to, visited);
                Self::replace_alias_refs_in_value(&mut new_cdr, from, to, visited);
                value.set_car(new_car);
                value.set_cdr(new_cdr);
            }
            ValueKind::Veclike(VecLikeType::Vector) => {
                let key = value.bits() ^ 0x2;
                if !visited.insert(key) {
                    return;
                }
                let mut data = value.as_vector_data().unwrap().clone();
                for item in data.iter_mut() {
                    Self::replace_alias_refs_in_value(item, from, to, visited);
                }
                let _ = value.replace_vector_data(data);
            }
            ValueKind::Veclike(VecLikeType::HashTable) => {
                let key = value.bits() ^ 0x4;
                if !visited.insert(key) {
                    return;
                }
                let old_ptr = match from.kind() {
                    ValueKind::String => Some(from.bits()),
                    _ => None,
                };
                let new_ptr = match to.kind() {
                    ValueKind::String => Some(to.bits()),
                    _ => None,
                };
                let _ = value.with_hash_table_mut(|ht| {
                    if matches!(ht.test, HashTableTest::Eq | HashTableTest::Eql)
                        && let (Some(old_ptr), Some(new_ptr)) = (old_ptr, new_ptr)
                    {
                        ht.replace_pointer_key(old_ptr, new_ptr, *to);
                    }
                    for item in ht.data.values_mut() {
                        Self::replace_alias_refs_in_value(item, from, to, visited);
                    }
                });
            }
            _ => {}
        }
    }

    /// GNU bytecode `Bvarref` by SymId.
    ///
    /// GNU `src/bytecode.c` reads bytecode variables with `Fsymbol_value`;
    /// it does not consult the interpreter lexical environment.  Lexical
    /// bytecode variables are compiled as stack/closure accesses instead.
    /// Fast path for variable reads matching GNU bytecode.c:626-647
    /// Bvarref: if the symbol is a plain global with a bound value,
    /// read the value cell directly without full symbolic resolution.
    fn fast_path_var_ref(&mut self, name_id: SymId) -> EvalResult {
        let ob = &self.ctx.obarray;
        let sym = ob.get_by_id(name_id).ok_or_else(|| {
            signal(
                LispCondition::VoidVariable,
                vec![Value::from_sym_id(name_id)],
            )
        })?;
        if sym.redirect() == crate::emacs_core::symbol::SymbolRedirect::Plainval {
            // SAFETY: redirect() already confirmed Plainval, so val.plain is active
            let val = unsafe { sym.val.plain };
            if !val.is_unbound() {
                // GNU installs `buffer-undo-list` as a DEFVAR_PER_BUFFER
                // forwarder. Neomacs keeps its value in SharedUndoState so
                // indirect buffers share one history, but classifies that
                // one dedicated local by symbol identity. Ordinary nil-valued
                // globals stay on this direct PLAINVAL path instead of all
                // paying a generic buffer-local probe.
                if !val.is_nil() {
                    return Ok(val);
                }
                if let Some(dedicated) =
                    crate::buffer::buffer::DedicatedBufferLocal::from_sym_id(name_id)
                    && let Some(buf) = self.ctx.buffers.current_buffer()
                {
                    return Ok(dedicated.read(buf));
                }
                return Ok(val);
            }
        }
        // A forwarder whose storage IS the descriptor needs no buffer context,
        // so the read is one indirection instead of `lookup_var_id`'s
        // resolve-alias + gather-buffer-slots-and-defaults path.  This is the
        // hot half of GNU's `Bvarref` for every `DEFVAR_INT`, `DEFVAR_BOOL`,
        // `DEFVAR_LISP` and `DEFVAR_KBOARD` variable.  `LispFwd::load` answers
        // `None` for exactly the one variant that does need the context
        // (`BufferObj`), and the slow path's own non-`BufferObj` arm ends at
        // the same call, so the two cannot drift.
        if sym.redirect() == crate::emacs_core::symbol::SymbolRedirect::Forwarded {
            // SAFETY: redirect() confirmed Forwarded, so val.fwd is active and
            // points at a descriptor `install_*fwd` leaked.
            let fwd = unsafe { &*sym.val.fwd };
            if let Some(value) = fwd.load() {
                return Ok(value);
            }
        }
        self.lookup_var_id(name_id)
    }

    fn lookup_var_id(&mut self, name_id: SymId) -> EvalResult {
        let resolved = crate::emacs_core::builtins::symbols::resolve_variable_alias_id_in_obarray(
            &self.ctx.obarray,
            name_id,
        )?;

        // Phase 9 of the symbol-redirect refactor: if the symbol's
        // redirect tag is LOCALIZED or FORWARDED, the new redirect
        // machinery is the source of truth. Route the read through
        // `find_symbol_value_in_buffer` which will swap the BLV
        // cache for LOCALIZED and read the slot for FORWARDED.
        //
        // For PLAINVAL / VARALIAS, fall through to the PLAINVAL fast path
        // via `find_symbol_value`. With Phase B complete, every LOCALIZED
        // symbol is handled by the redirect dispatch above.
        use crate::emacs_core::symbol::SymbolRedirect;
        let redirect = self.ctx.obarray.get_by_id(resolved).map(|s| s.redirect());
        if matches!(
            redirect,
            Some(SymbolRedirect::Localized | SymbolRedirect::Forwarded)
        ) {
            let (cur_val, alist, slots_ptr, buf_id, local_flags) =
                match self.ctx.buffers.current_buffer() {
                    Some(buf) => (
                        Value::make_buffer(buf.id),
                        buf.local_var_alist_value(),
                        Some(&buf.slots[..] as *const [Value]),
                        Some(buf.id),
                        buf.local_flags,
                    ),
                    None => (Value::NIL, Value::NIL, None, None, 0u64),
                };
            let defaults_ptr: *const [Value] =
                &self.ctx.buffers.buffer_defaults[..] as *const [Value];
            // Safety: the slots and defaults pointers are valid for
            // the duration of this call because we hold `&mut self.ctx`,
            // the buffer and BufferManager live inside `self.ctx`, and
            // `find_symbol_value_in_buffer` does not mutate the
            // buffer manager. The raw pointer dance is only needed
            // because `find_symbol_value_in_buffer` also needs
            // `&mut self.ctx.obarray` for the BLV swap-in, and the
            // borrow checker can't express "hold slices of two
            // fields while mutating a third" across the method call.
            let slots_opt: Option<&[Value]> = slots_ptr.map(|p| unsafe { &*p });
            let defaults_opt: Option<&[Value]> = Some(unsafe { &*defaults_ptr });
            if let Some(val) = self.ctx.obarray.find_symbol_value_in_buffer(
                resolved,
                buf_id,
                cur_val,
                alist,
                slots_opt,
                local_flags,
                defaults_opt,
            ) {
                // `Qunbound` from the BLV cache / alist walk marks a
                // void LOCALIZED binding for this buffer — signal
                // `void-variable` instead of returning the sentinel
                // to the caller. Mirrors GNU `Fsymbol_value` which
                // signals when `find_symbol_value` returns
                // `Qunbound`.
                if val.is_unbound() {
                    return Err(signal(
                        LispCondition::VoidVariable,
                        vec![Value::from_sym_id(name_id)],
                    ));
                }
                return Ok(val);
            }
        }

        // For variables like `buffer-undo-list` that are not slot-backed
        // but have per-buffer state (SharedUndoState), the obarray
        // default is nil while the buffer-local value is the live
        // undo list.  Check buffer-local before falling through to
        // the obarray default so the byte-compiled code sees the
        // correct per-buffer value.
        // Global (Plainval) specials are never in any `local_var_alist`, so skip
        // the per-buffer scan for them (slot/undo names still resolve inside the
        // gated call). See `Obarray::is_localized`.
        let name_localized = self.ctx.obarray.is_localized(name_id);
        if let Some(buf) = self.ctx.buffers.current_buffer()
            && let Some(val) = buf.get_buffer_local_by_sym_id_gated(name_id, name_localized)
            && !val.is_nil()
        {
            return Ok(val);
        }

        // GNU `bytecode.c:Bvarref` falls back to `Fsymbol_value`.
        if let Some(val) = self
            .ctx
            .visible_runtime_variable_value_by_id_resolved(resolved)
        {
            return Ok(val);
        }

        // Retry buffer-local for nil-valued defaults (e.g. unset
        // `buffer-undo-list` on a clean buffer).
        if let Some(buf) = self.ctx.buffers.current_buffer()
            && let Some(val) = buf.get_buffer_local_by_sym_id_gated(name_id, name_localized)
        {
            return Ok(val);
        }

        Err(signal(
            LispCondition::VoidVariable,
            vec![Value::from_sym_id(name_id)],
        ))
    }

    /// GNU bytecode `Bvarset` by SymId.
    ///
    /// Like `Bvarref`, bytecode assignment is dynamic.  Lexical bytecode
    /// locals are stack slots, not `varset` targets.
    fn assign_var_id(&mut self, name_id: SymId, value: Value) -> Result<(), Flow> {
        let resolved = crate::emacs_core::builtins::symbols::resolve_variable_alias_id_in_obarray(
            &self.ctx.obarray,
            name_id,
        )?;

        // GNU `set_internal`'s `SYMBOL_NOWRITE` arm (`src/data.c:1687-1697`):
        // a keyword re-assigned its own value is a silent no-op, not a signal.
        use crate::emacs_core::symbol::ConstantWrite;
        match self.ctx.obarray.classify_constant_write(resolved, value) {
            ConstantWrite::Writable => {}
            ConstantWrite::KeywordSelfAssign => return Ok(()),
            ConstantWrite::Refused => {
                return Err(signal(
                    LispCondition::SettingConstant,
                    vec![Value::from_sym_id(name_id)],
                ));
            }
        }

        // Phase 9b of the symbol-redirect refactor: for LOCALIZED
        // symbols, route the write through
        // Obarray::set_internal_localized which updates the BLV
        // cache and (for auto-create `Set` writes with
        // `local_if_set`) extends the current buffer's
        // local_var_alist. The legacy set_runtime_binding_in_state
        // path below stays populated as a fallback until Phase 10
        // deletes it.
        use crate::emacs_core::symbol::{SetInternalBind, SymbolRedirect};
        // GNU's bytecode `Bvarset` is `Fset` (`src/bytecode.c`), so it lands in
        // the same `set_internal` -> `store_symval_forwarding` the tree-walk
        // interpreter uses. Run the forward type's rule here, once, before any
        // of the storage fast paths below -- each of which writes a different
        // cell and would otherwise have to remember the rule itself.
        let value = crate::emacs_core::eval::check_forwarded_store(
            &self.ctx.obarray,
            &self.ctx.buffers,
            &self.ctx.specpdl,
            resolved,
            value,
        )?
        .value();
        let redirect = self.ctx.obarray.get_by_id(resolved).map(|s| s.redirect());
        // Phase 10B: FORWARDED writes go to the buffer slot the
        // descriptor points at. Mirrors GNU
        // `store_symval_forwarding` for the BUFFER_OBJFWD arm
        // (`data.c:1374-1471`).
        //
        // Phase 10D: for conditional slots (`local_flags_idx >= 0`),
        // also set the per-buffer local-flags bit so subsequent reads
        // route to `slots[off]` rather than `buffer_defaults`. This
        // mirrors GNU `set_internal` SYMBOL_FORWARDED arm at
        // `data.c:1774-1786` which calls `SET_PER_BUFFER_VALUE_P`.
        if matches!(redirect, Some(SymbolRedirect::Forwarded))
            && let Some(buf_id) = self.ctx.buffers.current_buffer_id()
        {
            use crate::emacs_core::forward::{LispBufferObjFwd, LispFwdType};
            let fwd_ptr = self
                .ctx
                .obarray
                .get_by_id(resolved)
                .map(|s| unsafe { s.val.fwd });
            if let Some(fwd) = fwd_ptr {
                // Safety: install_buffer_objfwd leaks a 'static
                // descriptor and the symbol's redirect tag is
                // immutable once installed.
                let header = unsafe { &*fwd };
                if matches!(header.ty, LispFwdType::BufferObj) {
                    let buf_fwd = unsafe { &*(fwd as *const LispBufferObjFwd) };
                    let Some(slot) = crate::buffer::buffer::BufferSlot::from_u16(buf_fwd.offset)
                    else {
                        return Err(signal(
                            "error",
                            vec![Value::string("Invalid buffer slot offset")],
                        ));
                    };
                    let offset = slot.index();
                    let flags_idx = buf_fwd.local_flags_idx;
                    let slot_exists = self
                        .ctx
                        .buffers
                        .get(buf_id)
                        .is_some_and(|buf| offset < buf.slots.len());
                    if slot_exists {
                        crate::emacs_core::eval::validate_buffer_slot_write(
                            buf_fwd.predicate,
                            value,
                        )?;
                        let where_value = Value::make_buffer(buf_id);
                        self.run_variable_watchers_by_id_with_where(
                            resolved,
                            &value,
                            &Value::NIL,
                            "set",
                            &where_value,
                        )?;
                        if let Some(buf) = self.ctx.buffers.get_mut(buf_id) {
                            buf.slots[offset] = value;
                            if flags_idx >= 0 {
                                buf.set_slot_local_flag(slot, true);
                            }
                        }
                        self.ctx
                            .publish_runtime_binding_write_by_id(resolved, value);
                        return Ok(());
                    }
                }
            }
        }

        if matches!(redirect, Some(SymbolRedirect::Localized))
            && let Some(buf_id) = self.ctx.buffers.current_buffer_id()
        {
            // Extract buffer state before obarray borrow.
            let (cur_val, alist) = match self.ctx.buffers.get(buf_id) {
                Some(buf) => (Value::make_buffer(buf.id), buf.local_var_alist_value()),
                None => (Value::NIL, Value::NIL),
            };
            // GNU `eval.c:3559-3577 (let_shadows_buffer_binding_p)`
            // only treats SPECPDL_LET_DEFAULT for the current buffer
            // as shadowing. SPECPDL_LET_LOCAL is explicitly excluded
            // by bug#62419.
            let let_shadows = self.ctx.let_shadows_buffer_binding_p(resolved);
            let where_value = self.ctx.variable_watcher_where_for_set_by_id(resolved);
            self.run_variable_watchers_by_id_with_where(
                resolved,
                &value,
                &Value::NIL,
                "set",
                &where_value,
            )?;
            let new_alist = self.ctx.obarray.set_internal_localized(
                resolved,
                value,
                cur_val,
                alist,
                SetInternalBind::Set,
                let_shadows,
            );
            // Store back the (possibly extended) alist.
            if let Some(buf) = self.ctx.buffers.get_mut(buf_id) {
                buf.replace_local_var_alist(new_alist);
            }
            self.ctx
                .publish_runtime_binding_write_by_id(resolved, value);
            return Ok(());
        }

        // Legacy path: set_runtime_binding_in_state routes to
        // either BufferLocals or the obarray value cell. Phase 10
        // deletes this call once every LOCALIZED symbol is
        // exclusively served by the new BLV path above.
        let where_value = self.ctx.variable_watcher_where_for_set_by_id(resolved);
        self.run_variable_watchers_by_id_with_where(
            resolved,
            &value,
            &Value::NIL,
            "set",
            &where_value,
        )?;
        crate::emacs_core::eval::set_runtime_binding_in_state(&mut *self.ctx, resolved, value)?;
        self.ctx
            .publish_runtime_binding_write_by_id(resolved, value);
        Ok(())
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn lookup_var(&mut self, name: &str) -> EvalResult {
        if name.starts_with(':') {
            return Ok(Value::keyword(name));
        }

        let name_id = intern(name);
        // Match GNU eval_sub: lexical environment lookup happens before
        // alias resolution fallback.
        if let Some(val) = self.ctx.lexenv_lookup_cached_in(self.ctx.lexenv, name_id) {
            return Ok(val);
        }
        let resolved = crate::emacs_core::builtins::symbols::resolve_variable_alias_id_in_obarray(
            &self.ctx.obarray,
            name_id,
        )?;
        if resolved != name_id
            && let Some(val) = self.ctx.lexenv_lookup_cached_in(self.ctx.lexenv, resolved)
        {
            return Ok(val);
        }

        // specbind writes directly to obarray, so dynamic stack lookup is
        // no longer needed — fall through to obarray lookup.

        // GNU `bytecode.c:Bvarref` falls back to `Fsymbol_value`,
        // not the raw symbol cell. Use the shared runtime reader so
        // bytecode observes the same forwarded/localized semantics as
        // tree-walk eval.
        if let Some(val) = self
            .ctx
            .visible_runtime_variable_value_by_id_resolved(resolved)
        {
            return Ok(val);
        }

        Err(signal(
            LispCondition::VoidVariable,
            vec![Value::symbol(name)],
        ))
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn assign_var(&mut self, name: &str, value: Value) -> Result<(), Flow> {
        let name_id = intern(name);
        let resolved = crate::emacs_core::builtins::symbols::resolve_variable_alias_id_in_obarray(
            &self.ctx.obarray,
            name_id,
        )?;
        if let Some(cell_id) = self.ctx.lexenv_assq_cached_in(self.ctx.lexenv, name_id) {
            lexenv_set(cell_id, value);
            return Ok(());
        }
        if resolved != name_id
            && let Some(cell_id) = self.ctx.lexenv_assq_cached_in(self.ctx.lexenv, resolved)
        {
            lexenv_set(cell_id, value);
            return Ok(());
        }

        // specbind writes directly to obarray, so dynamic stack mutation
        // is no longer needed — fall through to obarray write.

        // GNU `set_internal`'s `SYMBOL_NOWRITE` arm (`src/data.c:1687-1697`).
        use crate::emacs_core::symbol::ConstantWrite;
        match self.ctx.obarray.classify_constant_write(resolved, value) {
            ConstantWrite::Writable => {}
            ConstantWrite::KeywordSelfAssign => return Ok(()),
            ConstantWrite::Refused => {
                return Err(signal(
                    LispCondition::SettingConstant,
                    vec![Value::symbol(name)],
                ));
            }
        }

        let where_value = self.ctx.variable_watcher_where_for_set_by_id(resolved);
        self.run_variable_watchers_by_id_with_where(
            resolved,
            &value,
            &Value::NIL,
            "set",
            &where_value,
        )?;
        crate::emacs_core::eval::set_runtime_binding_in_state(&mut *self.ctx, resolved, value)?;
        Ok(())
    }

    fn run_variable_watchers_by_id(
        &mut self,
        sym_id: SymId,
        new_value: &Value,
        old_value: &Value,
        operation: &str,
    ) -> Result<(), Flow> {
        self.run_variable_watchers_by_id_with_where(
            sym_id,
            new_value,
            old_value,
            operation,
            &Value::NIL,
        )
    }

    fn run_variable_watchers_by_id_with_where(
        &mut self,
        sym_id: SymId,
        new_value: &Value,
        old_value: &Value,
        operation: &str,
        where_value: &Value,
    ) -> Result<(), Flow> {
        if !self.ctx.watchers.has_watchers(sym_id) {
            return Ok(());
        }
        if self.ctx.active_variable_watchers.contains(&sym_id) {
            return Ok(());
        }
        let calls =
            self.ctx
                .watchers
                .notify_watchers(sym_id, new_value, old_value, operation, where_value);
        self.ctx.active_variable_watchers.insert(sym_id);
        for (callback, args) in calls {
            if let Err(err) = self.call_function_with_roots(callback, &args) {
                self.ctx.active_variable_watchers.remove(&sym_id);
                return Err(err);
            }
        }
        self.ctx.active_variable_watchers.remove(&sym_id);
        Ok(())
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn run_variable_watchers(
        &mut self,
        name: &str,
        new_value: &Value,
        old_value: &Value,
        operation: &str,
    ) -> Result<(), Flow> {
        self.run_variable_watchers_by_id(intern(name), new_value, old_value, operation)
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn run_variable_watchers_with_where(
        &mut self,
        name: &str,
        new_value: &Value,
        old_value: &Value,
        operation: &str,
        where_value: &Value,
    ) -> Result<(), Flow> {
        self.run_variable_watchers_by_id_with_where(
            intern(name),
            new_value,
            old_value,
            operation,
            where_value,
        )
    }

    fn call_function_with_roots(&mut self, function: Value, args: &[Value]) -> EvalResult {
        self.call_function(function, args.iter().copied().collect::<LispArgVec>())
    }

    #[inline]
    fn call_function1(&mut self, function: Value, arg: Value) -> EvalResult {
        let mut args = LispArgVec::new();
        args.push(arg);
        self.call_function(function, args)
    }

    #[inline]
    fn call_function2(&mut self, function: Value, arg0: Value, arg1: Value) -> EvalResult {
        let mut args = LispArgVec::new();
        args.push(arg0);
        args.push(arg1);
        self.call_function(function, args)
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn ensure_selected_frame_id(&mut self) -> FrameId {
        crate::emacs_core::window_cmds::ensure_selected_frame_id_in_state(
            &mut self.ctx.frames,
            &mut self.ctx.buffers,
        )
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn resolve_frame_id(&mut self, arg: Option<&Value>, predicate: &str) -> Result<FrameId, Flow> {
        let Some(val) = arg else {
            return Ok(self.ensure_selected_frame_id());
        };
        match val.kind() {
            ValueKind::Nil => Ok(self.ensure_selected_frame_id()),
            ValueKind::Fixnum(n) => {
                let fid = FrameId(n as u64);
                if self.ctx.frames.get(fid).is_some() {
                    Ok(fid)
                } else {
                    Err(signal(
                        LispCondition::WrongTypeArgument,
                        vec![Value::symbol(predicate), Value::fixnum(n)],
                    ))
                }
            }
            ValueKind::Veclike(VecLikeType::Frame) => {
                let id = val.as_frame_id().unwrap();
                let fid = FrameId(id);
                if self.ctx.frames.get(fid).is_some() {
                    Ok(fid)
                } else {
                    Err(signal(
                        LispCondition::WrongTypeArgument,
                        vec![Value::symbol(predicate), *val],
                    ))
                }
            }
            _ => Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol(predicate), *val],
            )),
        }
    }

    fn builtin_call_last_kbd_macro_shared(&mut self, args: &[Value]) -> EvalResult {
        crate::emacs_core::kmacro::builtin_call_last_kbd_macro(&mut *self.ctx, args.to_vec())
    }

    fn builtin_execute_kbd_macro_shared(&mut self, args: &[Value]) -> EvalResult {
        crate::emacs_core::kmacro::builtin_execute_kbd_macro(&mut *self.ctx, args.to_vec())
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn case_fold_search_enabled(&mut self) -> bool {
        self.lookup_var("case-fold-search")
            .map(|value| !value.is_nil())
            .unwrap_or(true)
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn visible_variable_value_or_nil(&self, name: &str) -> Value {
        let name_id = intern(name);
        if let Some(value) = self.ctx.lexenv_lookup_cached_in(self.ctx.lexenv, name_id) {
            return value;
        }
        // specbind writes directly to obarray, so no dynamic stack lookup needed.
        if let Some(buffer) = self.ctx.buffers.current_buffer()
            && let Some(binding) = buffer.get_buffer_local_binding(name)
        {
            return binding.as_value().unwrap_or(Value::NIL);
        }
        if let Some(value) = self.ctx.obarray.symbol_value(name).copied() {
            return value;
        }
        if name == "nil" {
            return Value::NIL;
        }
        if name == "t" {
            return Value::T;
        }
        Value::NIL
    }

    /// GNU `bytecode.c:795-799`: a `Bcall` that found `debug_on_next_call` set
    /// records its backtrace frame and enters the ENTRY debugger before it
    /// dispatches; the frame it recorded is also flagged `debug_on_exit`, so
    /// the pop below re-enters the debugger on the way out.
    ///
    /// Deliberately not folded into [`Vm::call_function`]: that helper also
    /// serves `Op::CallBuiltin` and `Op::Apply`, which correspond to GNU's
    /// inline opcodes and to `Fapply`'s own dispatch -- neither of which GNU
    /// arms at the bytecode level.
    #[cold]
    #[inline(never)]
    fn call_function_debugged(&mut self, func_val: Value, args: LispArgVec) -> EvalResult {
        let bt_count = self.ctx.specpdl.len();
        self.ctx.push_backtrace_frame(func_val, &args);
        let entered = match self
            .ctx
            .take_debug_on_call_arm(crate::emacs_core::debug_on_call::DebugOnCallCode::Funcall)
        {
            Some(arm) => self.ctx.do_debug_on_call(arm),
            None => Ok(()),
        };
        let result = match entered {
            Err(flow) => Err(flow),
            Ok(()) => self.call_function_untraced_owned(func_val, args),
        };
        let result = self.ctx.dispatch_signal_result_if_needed(result);
        self.ctx
            .pop_bytecode_backtrace_frame_with_result(bt_count, result)
    }

    fn call_function(&mut self, func_val: Value, args: impl Into<LispArgVec>) -> EvalResult {
        let args = args.into();
        let bt_count = self.ctx.specpdl.len();
        self.ctx.push_backtrace_frame(func_val, &args);
        let result = self.call_function_untraced_owned(func_val, args);
        let result = self.ctx.dispatch_signal_result_if_needed(result);
        // Same GNU Bcall/Breturn single-entry pop as
        // call_function_from_stack_args; falls back inside on imbalance.
        self.ctx
            .pop_bytecode_backtrace_frame_with_result(bt_count, result)
    }

    /// Read a (dynamic/global) variable for JIT code with the interpreter's
    /// `Op::VarRef` semantics — delegates to the same `fast_path_var_ref`
    /// (Plainval fast path, buffer-locals, redirects; signals `void-variable`).
    #[cfg(feature = "jit")]
    pub(crate) fn varref_for_jit(&mut self, name_id: SymId) -> EvalResult {
        self.fast_path_var_ref(name_id)
    }

    /// Assign a (dynamic/global) variable for JIT code with the interpreter's
    /// `Op::VarSet` semantics — delegates to the same `assign_var_id` (may run
    /// variable watchers, i.e. arbitrary lisp; may signal).
    #[cfg(feature = "jit")]
    pub(crate) fn varset_for_jit(&mut self, name_id: SymId, value: Value) -> Result<(), Flow> {
        self.assign_var_id(name_id, value)
    }

    /// One bytecode-level `apply` with the interpreter's `Op::Apply` semantics:
    /// spread the last argument as a list, writeback detection + after-call
    /// writeback, and the plain traced `call_function` path (`Op::Apply` has no
    /// nesting-depth guard — mirror that exactly). Used by the JIT apply shim;
    /// keep in sync with the `Op::Apply` arm of `run_loop`. The caller polls
    /// `maybe_quit` first and roots `func_val` + `raw_args` (the spread values
    /// stay reachable through the rooted list).
    #[cfg(feature = "jit")]
    /// `Op::Aset` for JIT code — the interpreter arm minus the bc-frame
    /// rooting (the JIT shim scratch-roots the operands; nested calls root
    /// their own frames): override-aware named dispatch when `aset`'s
    /// function cell was redefined, the shared `builtin_aset` otherwise, then
    /// the unconditional string-writeback pass.
    pub(crate) fn aset_for_jit(
        &mut self,
        vec_val: Value,
        idx_val: Value,
        val: Value,
    ) -> EvalResult {
        let mut call_args = LispArgVec::new();
        call_args.push(vec_val);
        call_args.push(idx_val);
        call_args.push(val);
        let id = Self::builtin_name_id("aset");
        let result = if self.named_builtin_fast_path_allowed_id(id) {
            builtins::builtin_aset(call_args.clone().into_vec())?
        } else {
            let func_val = Value::from_sym_id(id);
            self.call_function(func_val, call_args.clone())?
        };
        let root_scope = self.ctx.save_vm_roots();
        self.push_dynamic_vm_root(result);
        for value in call_args.iter().copied() {
            self.push_dynamic_vm_root(value);
        }
        self.maybe_writeback_mutating_first_arg("aset", None, &call_args, &result);
        self.ctx.restore_vm_roots(root_scope);
        Ok(result)
    }

    /// `Op::CallBuiltin` for JIT code — the interpreter arm minus the
    /// bc-frame rooting: named fast path when the symbol's function cell is
    /// unmodified, full `call_function` (override/advice) otherwise, the
    /// mutating-first-arg string writeback, and the arm's trailing quit poll.
    pub(crate) fn callbuiltin_for_jit(&mut self, name_id: SymId, args: LispArgVec) -> EvalResult {
        let name = resolve_sym(name_id);
        let writeback_args = (args.first().is_some_and(|value| value.is_string())
            && Self::mutates_first_arg_name(name))
        .then(|| args.clone());
        let result = if self.named_builtin_fast_path_allowed_id(name_id) {
            self.dispatch_vm_builtin(name, args)?
        } else {
            let func_val = Value::from_sym_id(name_id);
            self.call_function(func_val, args)?
        };
        if let Some(writeback_args) = writeback_args.as_ref() {
            let root_scope = self.ctx.save_vm_roots();
            self.push_dynamic_vm_root(result);
            for value in writeback_args.iter().copied() {
                self.push_dynamic_vm_root(value);
            }
            self.maybe_writeback_mutating_first_arg(name, None, writeback_args, &result);
            self.ctx.restore_vm_roots(root_scope);
        }
        self.ctx.maybe_quit()?;
        Ok(result)
    }

    /// `Op::CallBuiltinSym` for JIT code — ALWAYS the direct named dispatch,
    /// never the function cell (GNU parity: bytecode-inlined primitives
    /// bypass advice; see the interpreter arm's comment), plus writeback and
    /// the trailing quit poll.
    pub(crate) fn callbuiltinsym_for_jit(&mut self, sym: SymId, args: LispArgVec) -> EvalResult {
        let name = resolve_sym(sym);
        let writeback_args = (args.first().is_some_and(|value| value.is_string())
            && Self::mutates_first_arg_name(name))
        .then(|| args.clone());
        let result = self.dispatch_vm_builtin(name, args)?;
        if let Some(writeback_args) = writeback_args.as_ref() {
            let root_scope = self.ctx.save_vm_roots();
            self.push_dynamic_vm_root(result);
            for value in writeback_args.iter().copied() {
                self.push_dynamic_vm_root(value);
            }
            self.maybe_writeback_mutating_first_arg(name, None, writeback_args, &result);
            self.ctx.restore_vm_roots(root_scope);
        }
        self.ctx.maybe_quit()?;
        Ok(result)
    }

    pub(crate) fn apply_for_jit(
        &mut self,
        func_val: Value,
        mut raw_args: LispArgVec,
    ) -> EvalResult {
        if raw_args.is_empty() {
            return self.call_function(func_val, LispArgVec::new());
        }
        // Spread the last argument.
        if let Some(last) = raw_args.pop() {
            let spread = list_to_vec(&last).unwrap_or_default();
            raw_args.extend(spread);
        }
        let args = raw_args;
        let writeback_names = if args.first().is_some_and(|value| value.is_string()) {
            self.writeback_mutating_callable_names(&func_val)
        } else {
            None
        };
        let writeback_args = writeback_names.as_ref().map(|_| args.clone());
        let result = self.call_function(func_val, args)?;
        if let (Some((called_name, alias_target)), Some(writeback_args)) =
            (writeback_names.as_ref(), writeback_args.as_ref())
        {
            let root_scope = self.ctx.save_vm_roots();
            self.push_dynamic_vm_root(result);
            for value in writeback_args.iter().copied() {
                self.push_dynamic_vm_root(value);
            }
            self.maybe_writeback_mutating_first_arg(
                called_name,
                *alias_target,
                writeback_args,
                &result,
            );
            self.ctx.restore_vm_roots(root_scope);
        }
        Ok(result)
    }

    /// One bytecode-level function call with the interpreter's `Op::Call`
    /// semantics: mutating-string-arg writeback detection, the lisp-nesting
    /// depth guard, the traced `call_function` path, and the after-call
    /// writeback. Used by the JIT call shim (`jit::compile::neovm_jit_call`) so
    /// compiled code re-enters the runtime through exactly the interpreter's
    /// call path — keep in sync with the `Op::Call` arm of `run_loop` (which
    /// keeps an in-place stack-args fast path for the no-writeback case).
    ///
    /// The caller polls `maybe_quit` first (GNU `bytecode.c:Bcall` order).
    #[cfg(feature = "jit")]
    pub(crate) fn call_for_jit(&mut self, func_val: Value, args: LispArgVec) -> EvalResult {
        // GNU `bytecode.c:795-799`.  This is the JIT's lowering of `Bcall`, so
        // it carries the arm just as the interpreter's `Op::Call` arm does.
        if self.ctx.debug_on_next_call_is_armed() {
            return self.with_bytecode_call_depth(|vm| vm.call_function_debugged(func_val, args));
        }
        let writeback_names = if args.first().is_some_and(|value| value.is_string()) {
            self.writeback_mutating_callable_names(&func_val)
        } else {
            None
        };
        let writeback_args = writeback_names.as_ref().map(|_| args.clone());
        let result = self.with_bytecode_call_depth(|vm| {
            // Fast subr path: the JIT routes subr (primitive) calls — 75.4% of
            // real-elisp calls — through the interpreter's exact direct-subr
            // dispatch (`try_call_builtin_subr_from_stack_args`), skipping
            // call_function's kind resolution + wrapper. It reads its args from
            // the GC-traced `bc_buf`, so push the value args there first (which
            // also roots them across the subr call, which may GC), try it,
            // restore. Falls back to the full call_function for non-subr callees
            // (bytecode/closures/overridden cells). Same depth guard + the
            // writeback wrapper below — behaviour-preserving, faster dispatch.
            let args_start = vm.ctx.bc_buf.len();
            for &a in args.iter() {
                vm.ctx.bc_buf.push(a);
            }
            let nargs = args.len();
            match vm.try_call_builtin_subr_from_stack_args(func_val, args_start, nargs) {
                Some(result) => {
                    vm.ctx.bc_buf.truncate(args_start);
                    result
                }
                None => {
                    vm.ctx.bc_buf.truncate(args_start);
                    vm.call_function(func_val, args)
                }
            }
        })?;
        if let (Some((called_name, alias_target)), Some(writeback_args)) =
            (writeback_names.as_ref(), writeback_args.as_ref())
        {
            let root_scope = self.ctx.save_vm_roots();
            self.push_dynamic_vm_root(result);
            for value in writeback_args.iter().copied() {
                self.push_dynamic_vm_root(value);
            }
            self.maybe_writeback_mutating_first_arg(
                called_name,
                *alias_target,
                writeback_args,
                &result,
            );
            self.ctx.restore_vm_roots(root_scope);
        }
        Ok(result)
    }

    /// Like [`call_for_jit`] but the `nargs` arguments are ALREADY on `bc_buf`
    /// at `args_start` — the JIT shim pushed them straight from its native
    /// call-args slot, skipping the `LispArgVec` round-trip + per-arg scratch
    /// rooting (`bc_buf` is GC-traced, so the args are rooted across the call).
    /// The caller truncates `bc_buf` back to `args_start` afterwards. The subr
    /// fast path reads the args in place; only the non-subr fallback
    /// materializes a `LispArgVec` (for the traced `call_function`). Same
    /// behaviour as `call_for_jit` — fewer copies on the hot path.
    #[cfg(feature = "jit")]
    pub(crate) fn call_for_jit_stack(
        &mut self,
        func_val: Value,
        args_start: usize,
        nargs: usize,
    ) -> EvalResult {
        // GNU `bytecode.c:795-799`, as in [`Vm::call_for_jit`]: the stack-args
        // JIT lowering of `Bcall` arms too.
        if self.ctx.debug_on_next_call_is_armed() {
            let args: LispArgVec = self.ctx.bc_buf[args_start..args_start + nargs]
                .iter()
                .copied()
                .collect();
            return self.with_bytecode_call_depth(|vm| vm.call_function_debugged(func_val, args));
        }
        let first_is_string = nargs > 0 && self.ctx.bc_buf[args_start].is_string();
        let writeback_names = if first_is_string {
            self.writeback_mutating_callable_names(&func_val)
        } else {
            None
        };
        let writeback_args: Option<LispArgVec> = writeback_names.as_ref().map(|_| {
            self.ctx.bc_buf[args_start..args_start + nargs]
                .iter()
                .copied()
                .collect()
        });
        let result = self.with_bytecode_call_depth(|vm| {
            match vm.try_call_builtin_subr_from_stack_args(func_val, args_start, nargs) {
                Some(result) => result,
                // The shim already staged the args on bc_buf at args_start:
                // take the zero-copy stack call protocol (backtrace span +
                // one run_frame copy), same as the interpreter's Op::Call.
                // The direct-builtin probe above already ran, so skip it.
                None => vm.call_function_from_stack_args(func_val, args_start, nargs, false),
            }
        })?;
        if let (Some((called_name, alias_target)), Some(writeback_args)) =
            (writeback_names.as_ref(), writeback_args.as_ref())
        {
            let root_scope = self.ctx.save_vm_roots();
            self.push_dynamic_vm_root(result);
            for value in writeback_args.iter().copied() {
                self.push_dynamic_vm_root(value);
            }
            self.maybe_writeback_mutating_first_arg(
                called_name,
                *alias_target,
                writeback_args,
                &result,
            );
            self.ctx.restore_vm_roots(root_scope);
        }
        Ok(result)
    }

    /// Armed speculated direct-SUBR call for the JIT subr spec shim
    /// (`jit::compile::neovm_jit_call_subr_spec`): the shim VALIDATED that
    /// `sym_id`'s function cell still holds `subr_value` (per-site epoch check
    /// against `function_epoch`, re-validated on epoch moves) and that no
    /// compiler function overrides are active — so the symbol resolution that
    /// `resolve_stack_call_target` would perform is provably redundant and is
    /// skipped. Everything ELSE mirrors [`call_for_jit_stack`] on a symbol
    /// callee resolving to a builtin subr, clause by clause:
    ///
    /// * the recursion-depth guard (`with_bytecode_call_depth`) — one
    ///   increment per call, `max-lisp-eval-depth` signals identically;
    /// * the backtrace frame records the SYMBOL (what the generic path's
    ///   `func_val` is at an `Op::Call` on a constant symbol), args read from
    ///   the GC-traced `bc_buf` in place;
    /// * the `SubrEntry` is read FRESH from the subr object on EVERY call —
    ///   `update_static_subr_object_entry` rewrites entries IN PLACE keeping
    ///   the value bits identical, so the fn pointer / arity / dispatch kind
    ///   may all have changed since compile time while the armed check still
    ///   passes. A rewritten entry that stopped being a plain builtin falls
    ///   back to the traced `call_function` on the SYMBOL — the exact spot
    ///   `resolve_stack_call_target` would classify as generic;
    /// * the arity signal (`wrong-number-of-arguments`) is checked against
    ///   that fresh entry INSIDE the backtrace frame, with the subr object as
    ///   payload (resolved subr-object parity);
    /// * dispatch through the stack-args dispatcher (A0..A8 nil-padding;
    ///   `Many`/`ManySlice` get the exact-length args, so even an in-place
    ///   rewrite to a variadic entry stays correct);
    /// * the debugger dispatch (`dispatch_signal_result_if_needed`) + frame
    ///   pop with result.
    ///
    /// NOT replicated, by static exclusion at the speculation site: the
    /// aset/fillarray mutating-first-string-arg writeback (those names are
    /// never speculated, site or resolved) and the `+`/`logand`/`logior`/
    /// `logxor` fixnum fast-value paths (all `Many`, never speculated — and
    /// they are pure result-equal shortcuts anyway).
    #[cfg(feature = "jit")]
    pub(crate) fn call_spec_subr_stack(
        &mut self,
        sym_id: SymId,
        subr_value: Value,
        args_start: usize,
        nargs: usize,
    ) -> EvalResult {
        self.call_spec_subr_stack_with_frame(
            sym_id,
            subr_value,
            Value::from_sym_id(sym_id),
            args_start,
            nargs,
        )
    }

    /// [`call_spec_subr_stack`](Self::call_spec_subr_stack) with an explicit
    /// backtrace-frame function value. The JIT's speculation shims record the
    /// SYMBOL (what their call site named); the interpreter's
    /// `CallBuiltin`/`CallBuiltinSym` route records the SUBR object, exactly
    /// what its former `funcall_general(subr, args)` route recorded — so
    /// `mapbacktrace` sees `#<subr insert>` identically compiled vs interp
    /// (`jit_cbsym_spec_insert_backtrace_shows_subr_frame_like_interp`).
    pub(crate) fn call_spec_subr_stack_with_frame(
        &mut self,
        sym_id: SymId,
        subr_value: Value,
        frame_func: Value,
        args_start: usize,
        nargs: usize,
    ) -> EvalResult {
        // GNU `bytecode.c:795-799`: the speculated subr call is still a
        // `Bcall`, so it carries the arm like every other lowering of one.
        if self.ctx.debug_on_next_call_is_armed() {
            let func_val = Value::from_sym_id(sym_id);
            let args: LispArgVec = self.ctx.bc_buf[args_start..args_start + nargs]
                .iter()
                .copied()
                .collect();
            return self.with_bytecode_call_depth(|vm| vm.call_function_debugged(func_val, args));
        }
        self.with_bytecode_call_depth(|vm| {
            let func_val = frame_func;
            let entry = subr_entry_from_value(subr_value)
                .map(|(_, entry)| entry)
                .filter(|entry| entry.dispatch_kind == SubrDispatchKind::Builtin);
            let Some(entry) = entry else {
                // The in-place-rewritten entry is no longer a plain builtin:
                // mirror call_for_jit_stack's non-subr arm — full traced call
                // on the SYMBOL.
                let args: LispArgVec = vm.ctx.bc_buf[args_start..args_start + nargs]
                    .iter()
                    .copied()
                    .collect();
                return vm.call_function(func_val, args);
            };
            let backtrace = vm
                .ctx
                .push_backtrace_frame_from_bc_stack(func_val, args_start, nargs);
            let result = if nargs < entry.min_args as usize
                || entry.max_args.is_some_and(|max| nargs > max as usize)
            {
                Err(signal(
                    LispCondition::WrongNumberOfArguments,
                    vec![subr_value, Value::fixnum(nargs as i64)],
                ))
            } else {
                match entry.function {
                    Some(function) => Vm::dispatch_builtin_subr_from_stack_args_unchecked(
                        vm.ctx, function, args_start, nargs,
                    )
                    .unwrap_or_else(|| Err(signal(LispCondition::VoidFunction, vec![func_val]))),
                    None => Err(signal(LispCondition::VoidFunction, vec![func_val])),
                }
            };
            let result = vm.ctx.dispatch_signal_result_if_needed(result);
            vm.ctx
                .pop_bytecode_backtrace_token_with_result(backtrace, result)
        })
    }

    /// V3 + native-to-native speculated direct call: the caller's spec site is
    /// armed, so `callee` is the compile-time bytecode object the symbol still
    /// names, and `args_ptr` addresses `nargs` pre-marshaled argument words (the
    /// caller's native call-args slot). Resolve and cache the callee's compiled
    /// leaf in `leaf_slot`, then run it DIRECTLY under the recursion-depth
    /// guard — skipping the `funcall_general` dispatch and the compiled-cache
    /// hash lookup that `call_for_jit` would pay.
    ///
    /// When the callee is a pure pass-through for this argument count (simple
    /// fixed arity, no `&optional` nil-pad / `&rest` list), the args go
    /// STRAIGHT to the callee's native entry — no `LispArgVec`, no per-arg
    /// scratch rooting, no re-marshal (the per-call cost that dominates
    /// call-heavy compiled code). Otherwise the args are marshaled and rooted
    /// (still skipping dispatch + hash lookup). Returns `None` when the callee
    /// can't be fast-pathed (body `NotCompilable`, or an arity mismatch the
    /// strict path must signal), leaving the shim to fall back to
    /// `call_for_jit`.
    ///
    /// The recursion-depth guard is applied exactly as `call_for_jit` applies
    /// it (one increment per call) so deeply recursive compiled functions
    /// signal `max-lisp-eval-depth` instead of overflowing the native stack.
    /// The cached leaf handle is sound because the per-thread `COMPILED` cache
    /// never evicts. The native pass-through needs no arg rooting: the caller's
    /// `maybe_quit` already returned Ok (which does not collect) and nothing
    /// allocates on a lisp heap before the callee's entry reads its args.
    ///
    /// SAFETY: `args_ptr` addresses `nargs` valid tagged words (the caller's
    /// call-args slot, populated immediately before the spec shim was called).
    ///
    /// Only ever called from the JIT spec shim (`jit::compile`, itself
    /// `#[cfg(feature = "jit")]`) and references `jit::compile`/`jit::cache`
    /// types, so it must be gated too — otherwise the no-jit production build
    /// (workspace `neovm-core` is `default-features = false`) fails to compile.
    #[cfg(feature = "jit")]
    /// Takes the CONTEXT, not a `Vm`: this is the JIT shim's per-call fast
    /// path, and `Vm::from_context` eagerly zero-fills its call caches — a
    /// measured ~22 Ir/call tax the armed path must not pay. Only the cold
    /// defensive deopt fallback below builds a `Vm`.
    pub(crate) fn call_armed_callee_native(
        ctx: &mut crate::emacs_core::eval::Context,
        callee: Value,
        leaf_slot: &core::sync::atomic::AtomicU64,
        args_ptr: *const i64,
        nargs: usize,
    ) -> Option<crate::emacs_core::jit::cache::NativeCallOutcome> {
        use core::sync::atomic::Ordering;
        // GNU `bytecode.c:798`: a `Bcall` with `debug_on_next_call` set must
        // record a frame and enter the entry debugger.  This native-to-native
        // fast path has no way to run Lisp mid-call, so it deopts exactly the
        // way a wrong arg count does -- `None` sends the call to the strict
        // `call_for_jit`/`call_for_jit_stack` path, which arms it.  Cold: the
        // flag is down in every non-debugging process.
        if ctx.debug_on_next_call_is_armed() {
            return None;
        }
        let bc = callee.get_bytecode_data()?;
        let mut ptr = leaf_slot.load(Ordering::Relaxed)
            as *const crate::emacs_core::jit::compile::CompiledLeaf;
        if ptr.is_null() {
            let ctx_ptr = core::ptr::from_mut(&mut *ctx);
            ptr = crate::emacs_core::jit::cache::resolve_compiled_leaf_ptr(ctx_ptr, bc)?;
            leaf_slot.store(ptr as usize as u64, Ordering::Relaxed);
        }
        // SAFETY: `ptr` names a cache-held leaf, valid here because the tagged-heap
        // identity is STABLE during native execution (the only thing that drops
        // cache leaves is `cache::clear()` on a heap-identity change, and the heap
        // is only swapped by top-level entry points, never nested inside a running
        // native leaf — so no `clear()` fires while this spec-slot pointer is live
        // on the native stack). See `resolve_compiled_leaf_ptr` for the full
        // invariant. (NOT "the cache never evicts" — it can; audit #1.)
        let leaf = unsafe { &*ptr };
        if !leaf.accepts(nargs) {
            // Wrong arg count: defer to the strict path, which signals
            // wrong-number-of-arguments exactly as the interpreter would.
            return None;
        }
        let pure = leaf.is_pure_passthrough(nargs);
        // Debug-build evidence that the fast path actually fires (vs silently
        // falling back to call_for_jit on every call).
        #[cfg(debug_assertions)]
        crate::emacs_core::jit::compile::SPEC_FAST_CALL_COUNT.fetch_add(1, Ordering::Relaxed);
        // BACKTRACE PARITY (cc-mode clean-build fix): the interpreter call path
        // pushes a backtrace frame for the callee (call_function_from_stack_args);
        // this native-to-native fast path must too, or `backtrace-frame` walks a
        // stack missing this activation — cc-bytecomp-compiling-or-loading then
        // fails to detect the compiling file and raises "c-lang-defconst can only
        // be used in a file". Args are read from the caller's call-args slot
        // (small-arity inline via SmallVec — no heap for the common ≤4-arg case).
        let bt_count = ctx.specpdl.len();
        // SAFETY: args_ptr addresses `nargs` valid tagged words (the caller's
        // call-args slot), same contract the native run below relies on. This
        // push also ROOTS `callee` (the frame's function field is GC-traced)
        // for the whole native run — the shim's separate scratch-root push
        // became redundant with it.
        unsafe {
            ctx.push_backtrace_frame_from_native_args(callee, args_ptr, nargs);
        }
        // Inline `with_bytecode_call_depth` at the ctx level (GNU's Bcall
        // depth protocol, floor-raise included) — the Vm wrapper exists only
        // for its caches, which this path deliberately avoids constructing.
        ctx.depth += 1;
        if ctx.depth > ctx.max_depth {
            if ctx.max_depth < 100 {
                ctx.max_depth = 100;
            }
            if ctx.depth > ctx.max_depth {
                ctx.depth -= 1;
                let err = Err(signal(
                    "error",
                    vec![Value::string("Lisp nesting exceeds ‘max-lisp-eval-depth’")],
                ));
                let res = ctx.pop_bytecode_backtrace_frame_with_result(bt_count, err);
                return Some(crate::emacs_core::jit::cache::NativeCallOutcome::from_result(res));
            }
        }
        use crate::emacs_core::jit::cache::NativeCallOutcome;
        // Defensive interpreter rerun for the Fallback outcomes (a plain
        // Deopt only arises with a null ctx — never here).
        // Cold defensive path: a full Vm is fine here. A plain fn, not a
        // closure — the closure's environment setup was a measured per-call
        // cost on the hot path that never takes this branch.
        #[cold]
        #[inline(never)]
        fn interp_fallback(
            ctx_ptr: *mut crate::emacs_core::eval::Context,
            bc: &ByteCodeFunction,
            callee: Value,
            args_ptr: *const i64,
            nargs: usize,
        ) -> crate::emacs_core::jit::cache::NativeCallOutcome {
            let mut args = Vec::with_capacity(nargs);
            for i in 0..nargs {
                // SAFETY: args_ptr addresses `nargs` valid words.
                args.push(Value::from_bits(unsafe { *args_ptr.add(i) } as usize));
            }
            let mut vm = Vm::from_context(unsafe { &mut *ctx_ptr });
            crate::emacs_core::jit::cache::NativeCallOutcome::from_result(
                vm.execute_with_func_value(bc, args, callee),
            )
        }
        let outcome = {
            let ctx_ptr = core::ptr::from_mut(&mut *ctx);
            if pure {
                // NATIVE-TO-NATIVE: pass the caller's call-args slot straight
                // through (no LispArgVec, no rooting, no re-marshal) — and a
                // register-sized outcome back (no Result<_, Flow> sret moves;
                // a signalling Flow stays in the pending slot).
                match crate::emacs_core::jit::cache::run_resolved_leaf_native(
                    ctx_ptr, bc, callee, leaf, args_ptr,
                ) {
                    NativeCallOutcome::Fallback => {
                        interp_fallback(ctx_ptr, bc, callee, args_ptr, nargs)
                    }
                    o => o,
                }
            } else {
                // Marshaled (callee has &optional/&rest): build + root args in
                // this branch's OWN scratch-root scope (the shim's armed fast
                // path no longer opens one).
                let saved = crate::emacs_core::eval::save_scratch_gc_roots();
                let mut args = LispArgVec::new();
                for i in 0..nargs {
                    // SAFETY: args_ptr addresses `nargs` valid words.
                    let v = Value::from_bits(unsafe { *args_ptr.add(i) } as usize);
                    crate::emacs_core::eval::push_scratch_gc_root(v);
                    args.push(v);
                }
                let ran = crate::emacs_core::jit::cache::run_resolved_leaf(
                    ctx_ptr, bc, callee, leaf, &args,
                );
                crate::emacs_core::eval::restore_scratch_gc_roots(saved);
                match ran {
                    Ok(Some(bits)) => NativeCallOutcome::Value(Value::from_bits(bits)),
                    Ok(None) => interp_fallback(ctx_ptr, bc, callee, args_ptr, nargs),
                    Err(flow) => {
                        crate::emacs_core::jit::compile::stash_pending_flow(flow);
                        NativeCallOutcome::FlowStashed
                    }
                }
            }
        };
        ctx.depth -= 1;
        // Pop the callee's backtrace frame (balanced single-entry pop; falls back
        // to the general unwinder if a nested imbalance occurred). The fast pop
        // never touches the outcome — the general path's by-value Result
        // round-trip was a measured per-call tax on the native->native
        // transition.
        if ctx.pop_native_backtrace_frame(bt_count) {
            return Some(outcome);
        }
        // Imbalanced (rare): materialize the EvalResult the general unwinder
        // needs, then re-compact.
        let res = match outcome {
            NativeCallOutcome::Value(v) => Ok(v),
            NativeCallOutcome::FlowStashed => {
                Err(crate::emacs_core::jit::compile::take_pending_flow()
                    .expect("FlowStashed implies a pending flow"))
            }
            // Both Fallback sources were resolved through interp_fallback above.
            NativeCallOutcome::Fallback => unreachable!("Fallback resolved before the pop"),
        };
        Some(NativeCallOutcome::from_result(
            ctx.pop_bytecode_backtrace_frame_with_result(bt_count, res),
        ))
    }

    fn call_function_from_stack_args(
        &mut self,
        func_val: Value,
        args_start: usize,
        nargs: usize,
        allow_direct_builtin_subr: bool,
    ) -> EvalResult {
        if allow_direct_builtin_subr {
            match self.resolve_stack_call_target(func_val) {
                ResolvedStackCallTarget::Builtin { callee } => {
                    return Self::call_resolved_builtin_from_stack_args(
                        self.ctx, func_val, args_start, nargs, callee,
                    );
                }
                ResolvedStackCallTarget::ByteCode { callee } => {
                    let callee = callee.value();
                    let backtrace = self
                        .ctx
                        .push_backtrace_frame_from_bc_stack(func_val, args_start, nargs);
                    let bc_data = callee
                        .get_bytecode_data()
                        .expect("resolved bytecode target must remain bytecode");
                    let result = self
                        .ctx
                        .execute_bytecode_call_from_stack(bc_data, args_start, nargs, callee);
                    let result = self.ctx.dispatch_signal_result_if_needed(result);
                    return self
                        .ctx
                        .pop_bytecode_backtrace_token_with_result(backtrace, result);
                }
                ResolvedStackCallTarget::Interpreter { .. } => {
                    unreachable!(
                        "the generic stack-call resolver cannot manufacture an iterative plan"
                    )
                }
                ResolvedStackCallTarget::Generic => {}
            }
        }
        // Zero-copy call protocol (GNU Bcall): the args stay in the caller's
        // bc_buf slots for the whole call — the backtrace entry records the
        // span (GNU record_in_backtrace stores a pointer into the same
        // slots), run_frame copies them ONCE into fresh callee slots (GNU
        // setup_frame's PUSH loop), and the caller pops them only after the
        // call returns. No LispArgVec, no per-arg rooting: bc_buf is
        // GC-traced.
        let backtrace = self
            .ctx
            .push_backtrace_frame_from_bc_stack(func_val, args_start, nargs);
        let result = self.call_function_untraced_from_stack(func_val, args_start, nargs);
        let result = self.ctx.dispatch_signal_result_if_needed(result);
        // GNU Bcall/Breturn exit: pop this call's own backtrace entry with a
        // single-entry pop (specpdl_ptr-- shape); imbalanced/debug-on-exit
        // cases fall back to the general unwinder inside.
        self.ctx
            .pop_bytecode_backtrace_token_with_result(backtrace, result)
    }

    /// Stack-args twin of [`Vm::call_function_untraced_owned`]: dispatch a
    /// callee whose args live on `bc_buf` at `[args_start, args_start +
    /// nargs)`. Bytecode callees run straight from the span through the
    /// tier-up seam; everything else (subrs, lambdas, aliases) materializes
    /// one `LispArgVec` and takes the generic owned path — those calls
    /// either already went through `try_call_builtin_subr_from_stack_args`
    /// or are cold.
    fn call_function_untraced_from_stack(
        &mut self,
        func_val: Value,
        args_start: usize,
        nargs: usize,
    ) -> EvalResult {
        match func_val.kind() {
            ValueKind::Veclike(VecLikeType::ByteCode) => {
                let bc_data = func_val.get_bytecode_data().unwrap();
                self.ctx
                    .execute_bytecode_call_from_stack(bc_data, args_start, nargs, func_val)
            }
            // Symbol-with-bytecode-cell fast path: same resolution discipline
            // as the owned twin (cell re-read live every call; compiler
            // overrides bail to generic).
            ValueKind::Symbol(sym_id) if !self.ctx.compiler_function_overrides_active() => {
                match self.ctx.obarray.symbol_function_id(sym_id) {
                    Some(cell)
                        if matches!(cell.kind(), ValueKind::Veclike(VecLikeType::ByteCode)) =>
                    {
                        let bc_data = cell.get_bytecode_data().unwrap();
                        self.ctx
                            .execute_bytecode_call_from_stack(bc_data, args_start, nargs, cell)
                    }
                    _ => {
                        let args = LispArgVec::from_slice(
                            &self.ctx.bc_buf[args_start..args_start + nargs],
                        );
                        self.ctx.funcall_general_untraced(func_val, args)
                    }
                }
            }
            _ => {
                let args = LispArgVec::from_slice(&self.ctx.bc_buf[args_start..args_start + nargs]);
                self.ctx.funcall_general_untraced(func_val, args)
            }
        }
    }

    fn call_function_untraced_owned(&mut self, func_val: Value, args: LispArgVec) -> EvalResult {
        match func_val.kind() {
            // Fast path: bytecoded calls dispatch through the shared JIT
            // tier-up seam (Context::execute_bytecode_call) — matching GNU's
            // CLOSUREP → goto setup_frame shape when the plan says interpret,
            // and running native code once the callee is hot. Routing the
            // VM's own call path through the seam is what lets functions
            // called ONLY from compiled code tier up at all.
            ValueKind::Veclike(VecLikeType::ByteCode) => {
                let bc_data = func_val.get_bytecode_data().unwrap();
                self.ctx.execute_bytecode_call(bc_data, args, func_val)
            }
            // A symbol whose live function cell is *directly* a byte-compiled
            // function: resolve the cell once and dispatch straight to the
            // bytecode entry, skipping funcall_general → apply_symbol_callable's
            // second resolution (an FxHashMap probe) and re-dispatch. The
            // byte-compiler calls its byte-compiled cconv/macroexp/bytecomp
            // helpers constantly, so this slice is hot. Only the clean
            // direct-bytecode case is taken; aliases, autoloads, advice wrappers,
            // interpreted closures, macros and special forms have non-bytecode
            // cells and fall through to the full generic dispatch unchanged. The
            // cell is re-read live every call so redefinition is honored; the
            // compiler-override guard mirrors resolve_stack_call_target. Both this
            // and funcall_general converge on execute_bytecode_call, so behavior
            // is identical minus the redundant resolution.
            ValueKind::Symbol(sym_id) if !self.ctx.compiler_function_overrides_active() => {
                match self.ctx.obarray.symbol_function_id(sym_id) {
                    Some(cell)
                        if matches!(cell.kind(), ValueKind::Veclike(VecLikeType::ByteCode)) =>
                    {
                        let bc_data = cell.get_bytecode_data().unwrap();
                        self.ctx.execute_bytecode_call(bc_data, args, cell)
                    }
                    _ => self.ctx.funcall_general_untraced(func_val, args),
                }
            }
            // Everything else: shared dispatch via funcall_general on Context.
            // Matches GNU Emacs where exec_byte_code delegates to funcall_general.
            _ => self.ctx.funcall_general_untraced(func_val, args),
        }
    }

    /// JIT generic-call fast path: a SYMBOL callee that resolves to a plain
    /// builtin dispatches at the Context level — no Vm construction (the
    /// per-shim `Vm::from_context` zero-fills the call caches, so nothing is
    /// gained by them here) and no scratch roots (an interned symbol callee
    /// is obarray-rooted; the arguments are staged on the GC-traced bc_buf).
    /// Returns `None` for anything else — non-symbol callees, bytecode/
    /// lambda/alias/advice cells, compiler overrides, and the
    /// fillarray/aset writeback shapes (including alias-to-subr, read off
    /// the resolved cell) — the caller then takes the full Vm path.
    /// Mirrors the spec shim's stage-2 ctx-level dispatch precedent.
    pub(crate) fn call_builtin_symbol_for_jit(
        ctx: &mut crate::emacs_core::eval::Context,
        func_val: Value,
        args_start: usize,
        nargs: usize,
    ) -> Option<EvalResult> {
        let sym_id = func_val.as_symbol_id()?;
        if ctx.compiler_function_overrides_active() {
            return None;
        }
        let cell = ctx.obarray.symbol_function_id(sym_id);
        let callee = match cell {
            Some(value) => {
                if !matches!(
                    value.kind(),
                    ValueKind::Subr(_) | ValueKind::Veclike(VecLikeType::Subr)
                ) {
                    return None;
                }
                ResolvedBuiltinCallee::from_subr_value(value)?
            }
            None => ResolvedBuiltinCallee::from_static_symbol(sym_id)?,
        };
        if nargs > 0 && ctx.bc_buf[args_start].is_string() {
            let target_id = cell.and_then(|value| value.as_subr_id()).unwrap_or(sym_id);
            if Self::mutates_first_arg_sym(sym_id) || Self::mutates_first_arg_sym(target_id) {
                return None;
            }
        }
        // Inline ctx-level bytecode-call depth protocol (GNU's Bcall depth,
        // floor-raise included) — the spec shim's stage-2 shape.
        ctx.depth += 1;
        if ctx.depth > ctx.max_depth {
            if ctx.max_depth < 100 {
                ctx.max_depth = 100;
            }
            if ctx.depth > ctx.max_depth {
                ctx.depth -= 1;
                return Some(Err(signal(
                    "error",
                    vec![Value::string("Lisp nesting exceeds ‘max-lisp-eval-depth’")],
                )));
            }
        }
        let result =
            Self::call_resolved_builtin_from_stack_args(ctx, func_val, args_start, nargs, callee);
        ctx.depth -= 1;
        Some(result)
    }

    fn try_call_builtin_subr_from_stack_args(
        &mut self,
        func_val: Value,
        args_start: usize,
        nargs: usize,
    ) -> Option<EvalResult> {
        let ResolvedStackCallTarget::Builtin { callee } = self.resolve_stack_call_target(func_val)
        else {
            return None;
        };
        Some(Self::call_resolved_builtin_from_stack_args(
            self.ctx, func_val, args_start, nargs, callee,
        ))
    }

    fn call_resolved_builtin_from_stack_args(
        ctx: &mut crate::emacs_core::eval::Context,
        func_val: Value,
        args_start: usize,
        nargs: usize,
        callee: ResolvedBuiltinCallee,
    ) -> EvalResult {
        let (sym_id, entry) = callee.entry();
        let backtrace = ctx.push_backtrace_frame_from_bc_stack(func_val, args_start, nargs);
        let result = if nargs < entry.min_args as usize
            || entry.max_args.is_some_and(|max| nargs > max as usize)
        {
            Err(signal(
                LispCondition::WrongNumberOfArguments,
                vec![callee.wrong_arity_value(), Value::fixnum(nargs as i64)],
            ))
        } else {
            if let Some(value) = Self::try_dispatch_builtin_subr_fast_value_from_stack_args(
                ctx, sym_id, args_start, nargs,
            ) {
                return match ctx.pop_fast_bytecode_backtrace_frame(backtrace) {
                    crate::emacs_core::eval::FastBytecodePop::Popped => Ok(value),
                    // GNU's exit debugger replaces the value it is shown
                    // (`src/bytecode.c:825-828`).
                    crate::emacs_core::eval::FastBytecodePop::OwesDebugOnExit(frame) => {
                        ctx.pop_bytecode_backtrace_token_with_result(frame, Ok(value))
                    }
                };
            }
            match entry.function {
                Some(function) => Self::dispatch_builtin_subr_from_stack_args_unchecked(
                    ctx, function, args_start, nargs,
                )
                .unwrap_or_else(|| {
                    Err(signal(
                        LispCondition::VoidFunction,
                        vec![Value::from_sym_id(sym_id)],
                    ))
                }),
                None => Err(signal(
                    LispCondition::VoidFunction,
                    vec![Value::from_sym_id(sym_id)],
                )),
            }
        };
        let result = ctx.dispatch_signal_result_if_needed(result);
        ctx.pop_bytecode_backtrace_token_with_result(backtrace, result)
    }

    #[inline]
    fn try_dispatch_builtin_subr_fast_value_from_stack_args(
        ctx: &crate::emacs_core::eval::Context,
        sym_id: SymId,
        args_start: usize,
        nargs: usize,
    ) -> Option<Value> {
        if sym_id == plus_sym_id() {
            return Self::try_fast_fixnum_add_value_from_stack_args(ctx, args_start, nargs);
        }
        if sym_id == logand_sym_id() {
            return Self::try_fast_fixnum_logand_value_from_stack_args(ctx, args_start, nargs);
        }
        if sym_id == logior_sym_id() {
            return Self::try_fast_fixnum_logior_value_from_stack_args(ctx, args_start, nargs);
        }
        if sym_id == logxor_sym_id() {
            return Self::try_fast_fixnum_logxor_value_from_stack_args(ctx, args_start, nargs);
        }
        None
    }

    #[inline]
    fn try_fast_fixnum_add_value_from_stack_args(
        ctx: &crate::emacs_core::eval::Context,
        args_start: usize,
        nargs: usize,
    ) -> Option<Value> {
        let args = &ctx.bc_buf;
        match nargs {
            0 => return Some(Value::fixnum(0)),
            1 => {
                let a = unsafe { args.get_unchecked(args_start) }.as_fixnum()?;
                return Some(Value::make_int(a));
            }
            2 => {
                let a = unsafe { args.get_unchecked(args_start) }.as_fixnum()?;
                let b = unsafe { args.get_unchecked(args_start + 1) }.as_fixnum()?;
                return Some(Value::make_int(a.checked_add(b)?));
            }
            3 => {
                let a = unsafe { args.get_unchecked(args_start) }.as_fixnum()?;
                let b = unsafe { args.get_unchecked(args_start + 1) }.as_fixnum()?;
                let c = unsafe { args.get_unchecked(args_start + 2) }.as_fixnum()?;
                let sum = a.checked_add(b)?;
                return Some(Value::make_int(sum.checked_add(c)?));
            }
            4 => {
                let a = unsafe { args.get_unchecked(args_start) }.as_fixnum()?;
                let b = unsafe { args.get_unchecked(args_start + 1) }.as_fixnum()?;
                let c = unsafe { args.get_unchecked(args_start + 2) }.as_fixnum()?;
                let d = unsafe { args.get_unchecked(args_start + 3) }.as_fixnum()?;
                let sum = a.checked_add(b)?;
                let sum = sum.checked_add(c)?;
                return Some(Value::make_int(sum.checked_add(d)?));
            }
            _ => {}
        }
        let mut acc = 0i64;
        for idx in 0..nargs {
            let next = unsafe { args.get_unchecked(args_start + idx) }.as_fixnum()?;
            acc = acc.checked_add(next)?;
        }
        Some(Value::make_int(acc))
    }

    #[inline]
    fn try_fast_fixnum_logand_value_from_stack_args(
        ctx: &crate::emacs_core::eval::Context,
        args_start: usize,
        nargs: usize,
    ) -> Option<Value> {
        let args = &ctx.bc_buf;
        let mut acc = if nargs == 0 {
            -1
        } else {
            unsafe { args.get_unchecked(args_start) }.as_fixnum()?
        };
        for idx in 1..nargs {
            let next = unsafe { args.get_unchecked(args_start + idx) }.as_fixnum()?;
            acc &= next;
        }
        Some(Value::make_int(acc))
    }

    #[inline]
    fn try_fast_fixnum_logior_value_from_stack_args(
        ctx: &crate::emacs_core::eval::Context,
        args_start: usize,
        nargs: usize,
    ) -> Option<Value> {
        let args = &ctx.bc_buf;
        let mut acc = if nargs == 0 {
            0
        } else {
            unsafe { args.get_unchecked(args_start) }.as_fixnum()?
        };
        for idx in 1..nargs {
            let next = unsafe { args.get_unchecked(args_start + idx) }.as_fixnum()?;
            acc |= next;
        }
        Some(Value::make_int(acc))
    }

    #[inline]
    fn try_fast_fixnum_logxor_value_from_stack_args(
        ctx: &crate::emacs_core::eval::Context,
        args_start: usize,
        nargs: usize,
    ) -> Option<Value> {
        let args = &ctx.bc_buf;
        let mut acc = if nargs == 0 {
            0
        } else {
            unsafe { args.get_unchecked(args_start) }.as_fixnum()?
        };
        for idx in 1..nargs {
            let next = unsafe { args.get_unchecked(args_start + idx) }.as_fixnum()?;
            acc ^= next;
        }
        Some(Value::make_int(acc))
    }

    fn dispatch_builtin_subr_from_stack_args_unchecked(
        ctx: &mut crate::emacs_core::eval::Context,
        func: SubrFn,
        args_start: usize,
        nargs: usize,
    ) -> Option<EvalResult> {
        let args = &ctx.bc_buf;
        macro_rules! stack_arg {
            ($idx:expr) => {{
                let idx = $idx;
                if idx < nargs {
                    unsafe { *args.get_unchecked(args_start + idx) }
                } else {
                    Value::NIL
                }
            }};
        }
        match func {
            SubrFn::A0(func) => Some(func(ctx)),
            SubrFn::A1(func) => {
                let arg0 = stack_arg!(0);
                Some(func(ctx, arg0))
            }
            SubrFn::A2(func) => {
                let arg0 = stack_arg!(0);
                let arg1 = stack_arg!(1);
                Some(func(ctx, arg0, arg1))
            }
            SubrFn::A3(func) => {
                let arg0 = stack_arg!(0);
                let arg1 = stack_arg!(1);
                let arg2 = stack_arg!(2);
                Some(func(ctx, arg0, arg1, arg2))
            }
            SubrFn::A4(func) => {
                let arg0 = stack_arg!(0);
                let arg1 = stack_arg!(1);
                let arg2 = stack_arg!(2);
                let arg3 = stack_arg!(3);
                Some(func(ctx, arg0, arg1, arg2, arg3))
            }
            SubrFn::A5(func) => {
                let arg0 = stack_arg!(0);
                let arg1 = stack_arg!(1);
                let arg2 = stack_arg!(2);
                let arg3 = stack_arg!(3);
                let arg4 = stack_arg!(4);
                Some(func(ctx, arg0, arg1, arg2, arg3, arg4))
            }
            SubrFn::A6(func) => {
                let arg0 = stack_arg!(0);
                let arg1 = stack_arg!(1);
                let arg2 = stack_arg!(2);
                let arg3 = stack_arg!(3);
                let arg4 = stack_arg!(4);
                let arg5 = stack_arg!(5);
                Some(func(ctx, arg0, arg1, arg2, arg3, arg4, arg5))
            }
            SubrFn::A7(func) => {
                let arg0 = stack_arg!(0);
                let arg1 = stack_arg!(1);
                let arg2 = stack_arg!(2);
                let arg3 = stack_arg!(3);
                let arg4 = stack_arg!(4);
                let arg5 = stack_arg!(5);
                let arg6 = stack_arg!(6);
                Some(func(ctx, arg0, arg1, arg2, arg3, arg4, arg5, arg6))
            }
            SubrFn::A8(func) => {
                let arg0 = stack_arg!(0);
                let arg1 = stack_arg!(1);
                let arg2 = stack_arg!(2);
                let arg3 = stack_arg!(3);
                let arg4 = stack_arg!(4);
                let arg5 = stack_arg!(5);
                let arg6 = stack_arg!(6);
                let arg7 = stack_arg!(7);
                Some(func(ctx, arg0, arg1, arg2, arg3, arg4, arg5, arg6, arg7))
            }
            SubrFn::Many(func) => {
                let args = args[args_start..args_start + nargs].to_vec();
                Some(func(ctx, args))
            }
            SubrFn::ManyNoContext(func) => {
                let args = args[args_start..args_start + nargs].to_vec();
                Some(func(args))
            }
            SubrFn::ManySlice(func) => Some(Self::call_many_slice_subr_from_stack_args(
                ctx, func, args_start, nargs,
            )),
        }
    }

    fn call_many_slice_subr_from_stack_args(
        ctx: &mut crate::emacs_core::eval::Context,
        func: crate::tagged::header::SubrFnManySlice,
        args_start: usize,
        nargs: usize,
    ) -> EvalResult {
        let args = &ctx.bc_buf;
        match nargs {
            0 => func(ctx, &[]),
            1 => {
                let arg0 = args[args_start];
                func(ctx, &[arg0])
            }
            2 => {
                let arg0 = args[args_start];
                let arg1 = args[args_start + 1];
                func(ctx, &[arg0, arg1])
            }
            3 => {
                let arg0 = args[args_start];
                let arg1 = args[args_start + 1];
                let arg2 = args[args_start + 2];
                func(ctx, &[arg0, arg1, arg2])
            }
            4 => {
                let arg0 = args[args_start];
                let arg1 = args[args_start + 1];
                let arg2 = args[args_start + 2];
                let arg3 = args[args_start + 3];
                func(ctx, &[arg0, arg1, arg2, arg3])
            }
            5 => {
                let arg0 = args[args_start];
                let arg1 = args[args_start + 1];
                let arg2 = args[args_start + 2];
                let arg3 = args[args_start + 3];
                let arg4 = args[args_start + 4];
                func(ctx, &[arg0, arg1, arg2, arg3, arg4])
            }
            6 => {
                let arg0 = args[args_start];
                let arg1 = args[args_start + 1];
                let arg2 = args[args_start + 2];
                let arg3 = args[args_start + 3];
                let arg4 = args[args_start + 4];
                let arg5 = args[args_start + 5];
                func(ctx, &[arg0, arg1, arg2, arg3, arg4, arg5])
            }
            7 => {
                let arg0 = args[args_start];
                let arg1 = args[args_start + 1];
                let arg2 = args[args_start + 2];
                let arg3 = args[args_start + 3];
                let arg4 = args[args_start + 4];
                let arg5 = args[args_start + 5];
                let arg6 = args[args_start + 6];
                func(ctx, &[arg0, arg1, arg2, arg3, arg4, arg5, arg6])
            }
            8 => {
                let arg0 = args[args_start];
                let arg1 = args[args_start + 1];
                let arg2 = args[args_start + 2];
                let arg3 = args[args_start + 3];
                let arg4 = args[args_start + 4];
                let arg5 = args[args_start + 5];
                let arg6 = args[args_start + 6];
                let arg7 = args[args_start + 7];
                func(ctx, &[arg0, arg1, arg2, arg3, arg4, arg5, arg6, arg7])
            }
            _ => {
                let args = LispArgVec::from_slice(&args[args_start..args_start + nargs]);
                func(ctx, &args)
            }
        }
    }

    /// Resolve one ordinary bytecode `Call` and, when possible, prove that it
    /// can enter the current interpreter driver without repeating tier or
    /// `setup_frame` classification in dispatch.
    #[inline(always)]
    fn resolve_interpreter_stack_call_target(
        &mut self,
        func_val: Value,
        nargs: usize,
    ) -> ResolvedStackCallTarget {
        let compiler_overrides_active = self.ctx.compiler_function_overrides_active();
        let function_epoch = self.ctx.obarray.function_epoch();
        if !compiler_overrides_active
            && let Some(call) = self
                .recent_interpreter_call
                .get(func_val, nargs, function_epoch)
        {
            return ResolvedStackCallTarget::Interpreter { call };
        }

        let target = self.resolve_stack_call_target(func_val);
        let ResolvedStackCallTarget::ByteCode { callee } = target else {
            return target;
        };
        if !self.uses_interpreter_only_tier()
            || !self.can_enter_interpreter_frame_iteratively(callee.code(), nargs)
        {
            return target;
        }

        let call = PreparedInterpreterCall::new(callee);
        if !compiler_overrides_active && func_val.as_symbol_id().is_some() {
            self.recent_interpreter_call
                .replace(func_val, nargs, function_epoch, call);
        }
        ResolvedStackCallTarget::Interpreter { call }
    }

    #[inline(always)]
    fn uses_interpreter_only_tier(&self) -> bool {
        #[cfg(feature = "jit")]
        {
            self.bytecode_tier_policy == BytecodeTierPolicy::InterpreterOnly
                || self.bcall_cache_forced
        }
        #[cfg(not(feature = "jit"))]
        {
            true
        }
    }

    fn resolve_stack_call_target(&mut self, func_val: Value) -> ResolvedStackCallTarget {
        match func_val.kind() {
            ValueKind::Veclike(VecLikeType::ByteCode) => ResolvedStackCallTarget::ByteCode {
                callee: ResolvedByteCodeCallee::from_direct_value(func_val),
            },
            ValueKind::Symbol(sym_id) => {
                if self.ctx.compiler_function_overrides_active() {
                    return ResolvedStackCallTarget::Generic;
                }
                let function_epoch = self.ctx.obarray.function_epoch();
                if let Some(target) = self.symbol_bytecode_call_cache.get(sym_id, function_epoch) {
                    return target;
                }
                match self.ctx.obarray.symbol_function_id(sym_id) {
                    Some(value) => {
                        let Some(callee) = ResolvedByteCodeCallee::from_live_function_cell(value)
                        else {
                            return if matches!(
                                value.kind(),
                                ValueKind::Subr(_) | ValueKind::Veclike(VecLikeType::Subr)
                            ) {
                                match ResolvedBuiltinCallee::from_subr_value(value) {
                                    Some(callee) => {
                                        self.symbol_bytecode_call_cache.insert_builtin(
                                            sym_id,
                                            function_epoch,
                                            callee,
                                        );
                                        ResolvedStackCallTarget::Builtin { callee }
                                    }
                                    None => ResolvedStackCallTarget::Generic,
                                }
                            } else {
                                ResolvedStackCallTarget::Generic
                            };
                        };
                        self.symbol_bytecode_call_cache.insert_bytecode(
                            sym_id,
                            function_epoch,
                            callee,
                        );
                        ResolvedStackCallTarget::ByteCode { callee }
                    }
                    // GNU bytecode.c:Bcall resolves a symbol's live function
                    // cell and calls SUBRP function cells directly. Use the
                    // same resolved subr object here instead of consulting the
                    // static table again on the hot path.
                    None => match ResolvedBuiltinCallee::from_static_symbol(sym_id) {
                        Some(callee) => {
                            self.symbol_bytecode_call_cache.insert_builtin(
                                sym_id,
                                function_epoch,
                                callee,
                            );
                            ResolvedStackCallTarget::Builtin { callee }
                        }
                        None => ResolvedStackCallTarget::Generic,
                    },
                }
            }
            ValueKind::Veclike(VecLikeType::Subr) | ValueKind::Subr(_) => {
                ResolvedBuiltinCallee::from_subr_value(func_val)
                    .map_or(ResolvedStackCallTarget::Generic, |callee| {
                        ResolvedStackCallTarget::Builtin { callee }
                    })
            }
            _ => ResolvedStackCallTarget::Generic,
        }
    }

    /// vm-profile only: classify how THIS `Op::Call` callee resolves on the
    /// current dispatch path, without perturbing it — a read-only peek that
    /// mirrors `resolve_stack_call_target` + `call_function_untraced_owned`'s
    /// kind tests. Returns (per-site callee key, CK_* class). Classified
    /// BEFORE the call so the pre-call state is what is counted.
    #[cfg(feature = "vm-profile")]
    fn vm_profile_classify_call(&self, func_val: Value) -> (u64, u8) {
        use vm_profile::*;
        match func_val.kind() {
            ValueKind::Veclike(VecLikeType::ByteCode) => (SITE_KEY_CLOSURE_VAL, CK_CLOSURE_VAL),
            ValueKind::Subr(_) | ValueKind::Veclike(VecLikeType::Subr) => {
                (SITE_KEY_SUBR_VAL, CK_SUBR_VAL)
            }
            ValueKind::Symbol(sym_id) => {
                let key = site_key_for_symbol(sym_id);
                if self.ctx.compiler_function_overrides_active() {
                    return (key, CK_OTHER_SYM);
                }
                let global_subr = || {
                    if lookup_global_subr_entry(sym_id).is_some() {
                        CK_BUILTIN_SYM
                    } else {
                        CK_OTHER_SYM
                    }
                };
                let kind = match self.ctx.obarray.symbol_function_id(sym_id) {
                    Some(cell)
                        if matches!(
                            cell.kind(),
                            ValueKind::Subr(_) | ValueKind::Veclike(VecLikeType::Subr)
                        ) =>
                    {
                        CK_BUILTIN_SYM
                    }
                    Some(cell)
                        if matches!(cell.kind(), ValueKind::Veclike(VecLikeType::ByteCode)) =>
                    {
                        CK_CLOSURE_SYM
                    }
                    Some(cell) if cell.is_nil() => global_subr(),
                    None => global_subr(),
                    _ => CK_OTHER_SYM,
                };
                (key, kind)
            }
            _ => (SITE_KEY_OTHER_VAL, CK_OTHER_VAL),
        }
    }

    /// vm-profile only: classify which branch of `fast_path_var_ref`/
    /// `lookup_var_id` this `Op::VarRef` read takes (read-only mirror of those
    /// branches). Returns (VR_* class, resolution-crossed-an-alias).
    #[cfg(feature = "vm-profile")]
    fn vm_profile_classify_varref(&self, name_id: SymId) -> (u8, bool) {
        use crate::emacs_core::symbol::SymbolRedirect;
        use vm_profile::*;
        let ob = &self.ctx.obarray;
        let Some(sym) = ob.get_by_id(name_id) else {
            return (VR_SLOW_OTHER, false);
        };
        if sym.redirect() == SymbolRedirect::Plainval {
            // SAFETY: redirect() confirmed Plainval, so val.plain is active
            // (same contract as fast_path_var_ref).
            let val = unsafe { sym.val.plain };
            if !val.is_unbound() {
                if !val.is_nil() {
                    return (VR_PLAIN, false);
                }
                if let Some(dedicated) =
                    crate::buffer::buffer::DedicatedBufferLocal::from_sym_id(name_id)
                    && let Some(buf) = self.ctx.buffers.current_buffer()
                    && !dedicated.read(buf).is_nil()
                {
                    return (VR_PLAIN_NIL_BLV, false);
                }
                return (VR_PLAIN_NIL, false);
            }
        }
        let resolved =
            match crate::emacs_core::builtins::symbols::resolve_variable_alias_id_in_obarray(
                ob, name_id,
            ) {
                Ok(id) => id,
                Err(_) => return (VR_SLOW_OTHER, false),
            };
        let via_alias = resolved != name_id;
        let class = match ob.get_by_id(resolved).map(|s| s.redirect()) {
            Some(SymbolRedirect::Localized) => VR_LOCALIZED,
            Some(SymbolRedirect::Forwarded) => VR_FORWARDED,
            _ => VR_SLOW_OTHER,
        };
        (class, via_alias)
    }

    fn resume_nonlocal(
        &mut self,
        _func: &ByteCodeFunction,
        pc: &mut usize,
        handlers: &mut HandlerStack,
        bind_stack: &mut BindStack,
        flow: Flow,
    ) -> Result<(), Flow> {
        match flow {
            // Neither is resumable inside the VM: a blocked thread and a
            // shutdown both unwind past every handler this frame owns.
            Flow::ThreadBlocked(_) | Flow::Shutdown(_) => Err(flow),
            Flow::Throw(thrown) => {
                let (tag, value) = (thrown.tag, thrown.value);
                let selected_resume = self.ctx.matching_catch_resume(&tag);
                if let Some(ResumeTarget::VmCatch {
                    target,
                    stack_len,
                    spec_depth,
                    bind_stack_len,
                    ..
                }) = unwind_handlers_to_selected_resume(
                    handlers,
                    &mut self.ctx.condition_stack,
                    selected_resume.as_ref(),
                ) {
                    let root_scope = self.ctx.save_vm_roots();
                    self.ctx.push_vm_frame_root(tag);
                    self.ctx.push_vm_frame_root(value);
                    let unwind = self.ctx.unbind_to_with_result(spec_depth, Ok(Value::NIL));
                    bind_stack.truncate(bind_stack_len);
                    if let Err(flow) = unwind {
                        self.ctx.restore_vm_roots(root_scope);
                        return self.resume_nonlocal(_func, pc, handlers, bind_stack, flow);
                    }
                    self.ctx.bc_buf.truncate(stack_len);
                    self.ctx.bc_buf.push(value);
                    self.ctx.restore_vm_roots(root_scope);
                    *pc = target as usize;
                    return Ok(());
                }

                if selected_resume.is_some() {
                    return Err(Flow::throw(tag, value));
                }
                tracing::debug!(
                    target: "neomacs::throw_on_input",
                    ?tag,
                    ?value,
                    condition_stack_len = self.ctx.condition_stack.len(),
                    handler_stack_len = handlers.len(),
                    "vm resume_nonlocal: no matching catch for throw"
                );
                Err(signal(LispCondition::NoCatch, vec![tag, value]))
            }
            Flow::Signal(sig) => {
                // dispatch_signal_if_needed may call signal hooks and
                // handler-bind handlers via eval.apply(), which can trigger
                // GC.  We must root the current frame so values survive
                // collection.
                let mut sig_extra = Vec::new();
                Self::collect_flow_roots(&Flow::Signal(sig.clone()), &mut sig_extra);
                let sig = match self.with_frame_roots(_func, &sig_extra, |vm| {
                    vm.ctx.dispatch_signal_if_needed(sig)
                }) {
                    Ok(sig) => sig,
                    Err(flow) => {
                        return self.resume_nonlocal(_func, pc, handlers, bind_stack, flow);
                    }
                };
                if let Some(ResumeTarget::VmConditionCase {
                    target,
                    stack_len,
                    spec_depth,
                    bind_stack_len,
                    ..
                }) = unwind_handlers_to_selected_resume(
                    handlers,
                    &mut self.ctx.condition_stack,
                    sig.selected_resume.as_ref(),
                ) {
                    let root_scope = self.ctx.save_vm_roots();
                    self.ctx.push_vm_frame_root(Value::from_sym_id(sig.symbol));
                    for value in sig.data.iter().copied() {
                        self.ctx.push_vm_frame_root(value);
                    }
                    if let Some(raw_data) = sig.raw_data {
                        self.ctx.push_vm_frame_root(raw_data);
                    }
                    let unwind = self.ctx.unbind_to_with_result(spec_depth, Ok(Value::NIL));
                    bind_stack.truncate(bind_stack_len);
                    if let Err(flow) = unwind {
                        self.ctx.restore_vm_roots(root_scope);
                        return self.resume_nonlocal(_func, pc, handlers, bind_stack, flow);
                    }
                    self.ctx.bc_buf.truncate(stack_len);
                    self.ctx.bc_buf.push(make_signal_binding_value(&sig));
                    self.ctx.restore_vm_roots(root_scope);
                    *pc = target as usize;
                    return Ok(());
                }
                Err(Flow::Signal(sig))
            }
        }
    }

    fn dispatch_vm_builtin_with_frame(
        &mut self,
        func: &ByteCodeFunction,
        name: &str,
        args: impl Into<LispArgVec>,
    ) -> EvalResult {
        let args = args.into();
        self.with_frame_arg_roots(func, args, |vm, args| {
            vm.dispatch_vm_builtin_unrooted(name, args)
        })
    }

    fn dispatch_vm_builtin(&mut self, name: &str, args: impl Into<LispArgVec>) -> EvalResult {
        self.dispatch_vm_builtin_unrooted(name, args.into())
    }

    /// The builtins `dispatch_vm_builtin_unrooted` special-cases by NAME (VM-
    /// level implementations that need `&mut Vm`); everything else is an
    /// ordinary subr. Keyed by `SymId` so the hot dispatch below never resolves
    /// the symbol to a string.
    fn vm_special_builtin_ids() -> &'static [SymId; 13] {
        static IDS: std::sync::OnceLock<[SymId; 13]> = std::sync::OnceLock::new();
        IDS.get_or_init(|| {
            [
                intern("call-interactively"),
                intern("start-kbd-macro"),
                intern("end-kbd-macro"),
                intern("call-last-kbd-macro"),
                intern("execute-kbd-macro"),
                intern("garbage-collect"),
                intern("mapatoms"),
                intern("maphash"),
                intern("store-kbd-macro-event"),
                intern("cancel-kbd-macro-events"),
                intern("%%defvar"),
                intern("%%defconst"),
                intern("%%unimplemented-elc-bytecode"),
            ]
        })
    }

    /// `CallBuiltin`/`CallBuiltinSym` dispatch by symbol id: the op's constant IS
    /// the symbol, so this goes straight to `funcall_general` on the subr for
    /// every ordinary builtin — GNU's `Bcall` on a subr symbol is exactly
    /// `funcall_general` → `funcall_subr` — and falls back to the by-name
    /// `dispatch_vm_builtin_with_frame` only for the VM-level special cases.
    /// The old path resolved the id to a NAME, walked a 13-way string `match`,
    /// then `lookup_interned` the name back to an id on every call (13.7K calls
    /// = 12% of the type window).
    fn dispatch_vm_builtin_by_id_with_frame(
        &mut self,
        func: &ByteCodeFunction,
        sym: SymId,
        args: LispArgVec,
    ) -> EvalResult {
        if Self::vm_special_builtin_ids().contains(&sym) {
            return self.dispatch_vm_builtin_with_frame(func, resolve_sym(sym), args);
        }
        self.with_frame_arg_roots(func, args, |vm, args| {
            vm.ctx.funcall_general(Value::subr_from_sym_id(sym), args)
        })
    }

    /// `CallBuiltin`/`CallBuiltinSym` with the arguments still on the operand
    /// stack (`bc_buf[args_start..args_start + nargs]`): GNU's `Bcall` on a
    /// subr symbol — the backtrace record points at the stack, the subr is
    /// dispatched arity-checked straight from it (`call_spec_subr_stack`, the
    /// same lean route the JIT's subr speculation uses). No `LispArgVec` copy,
    /// no generic `funcall_general` walk, no frame arg copy. The VM-level
    /// special cases keep the by-name route.
    fn dispatch_vm_builtin_by_id_from_stack(
        &mut self,
        func: &ByteCodeFunction,
        sym: SymId,
        args_start: usize,
        nargs: usize,
    ) -> EvalResult {
        if Self::vm_special_builtin_ids().contains(&sym) {
            let args: LispArgVec = self.ctx.bc_buf[args_start..args_start + nargs]
                .iter()
                .copied()
                .collect();
            return self.dispatch_vm_builtin_with_frame(func, resolve_sym(sym), args);
        }
        let subr = Value::subr_from_sym_id(sym);
        self.call_spec_subr_stack_with_frame(sym, subr, subr, args_start, nargs)
    }

    /// Dispatch to builtin functions from the VM.
    /// Dispatch an `Op::CallBuiltinSym` from the operand stack. Kept out of
    /// `run_loop`'s body so the giant dispatch match stays small: a registered
    /// builtin takes GNU's inline-opcode path (direct primitive call, no
    /// backtrace frame, no arity check); everything else keeps the framed
    /// [`Self::dispatch_vm_builtin_by_id_from_stack`] path with the
    /// string-mutation writeback. Arguments live in `ctx.bc_buf[args_start..]`;
    /// the caller truncates and pushes the result.
    #[inline(never)]
    fn dispatch_call_builtin_sym(
        &mut self,
        func: &ByteCodeFunction,
        sym: SymId,
        args_start: usize,
        nargs: usize,
    ) -> EvalResult {
        // GNU-parity: opcodes 0140-0177 (decode.rs) dispatch *directly* to
        // their C implementations (bytecode.c:1412-1545), bypassing the
        // symbol's function cell and advice table. `(advice-add 'point ...)`
        // deliberately does not fire when bytecode calls `(point)` via Bpoint;
        // routing these through the function cell would make neomacs MORE
        // advisable than GNU, breaking parity.
        if let Some(function) = Self::inline_builtin_function(sym) {
            return self.call_inline_builtin(function, sym, args_start, nargs);
        }
        let writeback_args = (self
            .ctx
            .bc_buf
            .get(args_start)
            .is_some_and(|value| value.is_string())
            && Self::mutates_first_arg_sym(sym))
        .then(|| {
            self.ctx.bc_buf[args_start..args_start + nargs]
                .iter()
                .copied()
                .collect::<LispArgVec>()
        });
        let result = self.dispatch_vm_builtin_by_id_from_stack(func, sym, args_start, nargs);
        if let Some(writeback_args) = writeback_args.as_ref() {
            let result = result?;
            let root_scope = self.ctx.save_vm_roots();
            self.push_dynamic_vm_root(result);
            for value in writeback_args.iter().copied() {
                self.push_dynamic_vm_root(value);
            }
            self.maybe_writeback_mutating_first_arg(
                crate::emacs_core::intern::resolve_sym(sym),
                None,
                writeback_args,
                &result,
            );
            self.ctx.restore_vm_roots(root_scope);
            return Ok(result);
        }
        result
    }

    /// The Rust primitive a GNU inline opcode calls directly (bytecode.c
    /// `Bpoint`..`Bwiden`, `Bset_marker`..`Bdowncase`): the symbol's registered
    /// builtin entry. `None` for the VM-owned specials, which need `self`, and
    /// for anything not registered as a plain builtin; those keep the framed
    /// path of [`Self::dispatch_vm_builtin_by_id_from_stack`].
    #[inline]
    pub(crate) fn inline_builtin_function(sym: SymId) -> Option<SubrFn> {
        let entry = lookup_global_subr_entry(sym)?;
        if entry.dispatch_kind != SubrDispatchKind::Builtin
            || Self::vm_special_builtin_ids().contains(&sym)
        {
            return None;
        }
        entry.function
    }

    /// GNU inline-opcode semantics: call the primitive on the operand-stack
    /// arguments with no backtrace frame, no arity check and no eval-depth
    /// accounting, then route a signal through the condition handlers exactly
    /// as the framed path does.
    pub(crate) fn call_inline_builtin_from_stack(
        ctx: &mut crate::emacs_core::eval::Context,
        function: SubrFn,
        sym: SymId,
        args_start: usize,
        nargs: usize,
    ) -> EvalResult {
        #[cfg(test)]
        INLINE_BUILTIN_DIRECT_COUNT.with(|count| count.set(count.get() + 1));
        let result =
            Self::dispatch_builtin_subr_from_stack_args_unchecked(ctx, function, args_start, nargs)
                .unwrap_or_else(|| {
                    Err(signal(
                        LispCondition::VoidFunction,
                        vec![Value::from_sym_id(sym)],
                    ))
                });
        ctx.dispatch_signal_result_if_needed(result)
    }

    fn call_inline_builtin(
        &mut self,
        function: SubrFn,
        sym: SymId,
        args_start: usize,
        nargs: usize,
    ) -> EvalResult {
        Self::call_inline_builtin_from_stack(self.ctx, function, sym, args_start, nargs)
    }

    fn dispatch_vm_builtin_unrooted(&mut self, name: &str, args: LispArgVec) -> EvalResult {
        // VM-internal bytecode operations that are not real Elisp builtins.
        match name {
            "call-interactively" => return self.builtin_call_interactively_shared(&args),
            "start-kbd-macro" => {
                return crate::emacs_core::kmacro::builtin_start_kbd_macro(
                    &mut *self.ctx,
                    args.into_vec(),
                );
            }
            "end-kbd-macro" => {
                return crate::emacs_core::kmacro::builtin_end_kbd_macro(
                    &mut *self.ctx,
                    args.into_vec(),
                );
            }
            "call-last-kbd-macro" => return self.builtin_call_last_kbd_macro_shared(&args),
            "execute-kbd-macro" => return self.builtin_execute_kbd_macro_shared(&args),
            "garbage-collect" => return self.builtin_garbage_collect_shared(&args),
            "mapatoms" => return self.builtin_mapatoms_shared(&args),
            "maphash" => return self.builtin_maphash_shared(&args),
            "store-kbd-macro-event" => {
                return crate::emacs_core::kmacro::builtin_store_kbd_macro_event(
                    &mut *self.ctx,
                    args.into_vec(),
                );
            }
            "cancel-kbd-macro-events" => {
                return crate::emacs_core::builtins::builtin_cancel_kbd_macro_events(
                    &mut *self.ctx,
                    args.into_vec(),
                );
            }
            "%%defvar" => {
                if args.len() >= 2 {
                    let sym_name = args[1].as_symbol_name().unwrap_or("nil").to_string();
                    if !self.ctx.obarray.boundp(&sym_name) {
                        self.ctx.obarray.set_symbol_value(&sym_name, args[0]);
                    }
                    self.ctx.obarray.make_special(&sym_name);
                    return Ok(Value::symbol(sym_name));
                }
                return Ok(Value::NIL);
            }
            "%%defconst" => {
                if args.len() >= 2 {
                    let sym = args[1];
                    let sym_id = sym.as_symbol_id().unwrap_or_else(|| intern("nil"));
                    crate::emacs_core::data::set_default_internal(
                        &mut *self.ctx,
                        Value::from_sym_id(sym_id),
                        args[0],
                        crate::emacs_core::symbol::SetInternalBind::Set,
                    )?;
                    self.ctx.obarray.make_special_id(sym_id);
                    self.ctx.obarray.put_property_id(
                        sym_id,
                        intern("risky-local-variable"),
                        Value::T,
                    )?;
                    return Ok(Value::from_sym_id(sym_id));
                }
                return Ok(Value::NIL);
            }
            "%%unimplemented-elc-bytecode" => {
                return Err(signal(
                    "error",
                    vec![Value::string(
                        "Compiled .elc bytecode execution is not implemented yet",
                    )],
                ));
            }
            _ => {}
        }

        // All real builtins go through funcall_general → dispatch_subr.
        // This matches GNU Emacs where the bytecode VM delegates to
        // funcall_general for everything except bytecoded closures.
        self.ctx
            .funcall_general(Value::subr_from_sym_id(Self::builtin_name_id(name)), args)
    }

    fn builtin_call_interactively_shared(&mut self, args: &[Value]) -> EvalResult {
        crate::emacs_core::interactive::validate_call_interactively_args(args)?;
        let command_identity =
            crate::emacs_core::interactive::CallInteractivelyCommandIdentity::capture(self.ctx);
        self.with_vm_root_scope(|vm| {
            for value in args.iter().copied() {
                vm.push_dynamic_vm_root(value);
            }
            for value in command_identity.values() {
                vm.push_dynamic_vm_root(value);
            }
            let interactive_form = vm.call_function_with_roots(
                crate::emacs_core::interactive::InteractiveFormSymbol::value(),
                &[args[0]],
            )?;
            vm.push_dynamic_vm_root(interactive_form);
            let mut plan =
                crate::emacs_core::interactive::plan_call_interactively_after_interactive_form_in_state(
                    &vm.ctx.obarray,
                    vm.ctx.read_command_keys(),
                    args,
                    interactive_form,
                    command_identity,
                )?;
            for value in plan.gc_roots() {
                vm.push_dynamic_vm_root(value);
            }
            let (_function, call_args) =
                crate::emacs_core::interactive::resolve_call_interactively_target_and_args_with_vm_fallback(
                    vm.ctx,
                    &mut plan,
                )?;
            for value in call_args.iter().copied() {
                vm.push_dynamic_vm_root(value);
            }
            let invocation = plan.restore_for_invocation(vm.ctx);
            let funcall_args = invocation.into_funcall_args(call_args);
            vm.call_function_with_roots(Value::symbol("funcall-interactively"), &funcall_args)
        })
    }

    fn builtin_garbage_collect_shared(&mut self, args: &[Value]) -> EvalResult {
        builtins::expect_args("garbage-collect", args, 0)?;
        self.ctx.gc_collect_exact();
        crate::emacs_core::builtins_extra::builtin_garbage_collect_stats()
    }

    fn builtin_mapatoms_shared(&mut self, args: &[Value]) -> EvalResult {
        let (func, symbols) =
            crate::emacs_core::hashtab::collect_mapatoms_symbols(self.ctx, args.to_vec())?;
        self.with_dynamic_vm_roots(|vm| {
            vm.push_dynamic_vm_root(func);
            // `symbols` contains immediate IDs backed by the append-only
            // global symbol registry, not GC-managed heap pointers.
            for sym in symbols {
                vm.call_function1(func, sym)?;
            }
            Ok(Value::NIL)
        })
    }

    fn builtin_maphash_shared(&mut self, args: &[Value]) -> EvalResult {
        let (func, table) = crate::emacs_core::hashtab::validate_maphash_args(args)?;
        self.with_dynamic_vm_roots(|vm| {
            vm.push_dynamic_vm_root(func);
            vm.push_dynamic_vm_root(table);
            let mut slot = 0_usize;
            loop {
                let Some((key, value)) =
                    crate::emacs_core::hashtab::maphash_entry_at_slot(table, slot)
                else {
                    if slot >= crate::emacs_core::hashtab::maphash_slot_len(table) {
                        break;
                    }
                    slot += 1;
                    continue;
                };
                vm.push_dynamic_vm_root(key);
                vm.push_dynamic_vm_root(value);
                vm.call_function2(func, key, value)?;
                slot += 1;
            }
            Ok(Value::NIL)
        })
    }
}

impl<'a> crate::emacs_core::builtins::symbols::MacroexpandRuntime for Vm<'a> {
    fn symbol_function_by_id(&self, symbol: SymId) -> Option<Value> {
        crate::emacs_core::builtins::symbols::symbol_function_cell_in_obarray(
            &self.ctx.obarray,
            symbol,
        )
    }

    fn autoload_do_load_macro(&mut self, autoload: Value, head: Value) -> Result<(), Flow> {
        let args = vec![autoload, head, Value::symbol("macro")];
        let _ = self.with_vm_root_scope(|vm| {
            for value in args.iter().copied() {
                vm.push_dynamic_vm_root(value);
            }
            crate::emacs_core::autoload::builtin_autoload_do_load_in_vm_runtime(vm.ctx, &args)
        })?;
        Ok(())
    }

    fn apply_macro_function(
        &mut self,
        form: Value,
        function: Value,
        args: Vec<Value>,
        environment: Option<Value>,
    ) -> Result<Value, Flow> {
        let expand_start = std::time::Instant::now();
        self.with_dynamic_vm_roots(move |vm| {
            vm.push_dynamic_vm_root(form);
            vm.push_dynamic_vm_root(function);
            if let Some(environment) = environment {
                vm.push_dynamic_vm_root(environment);
            }
            for value in args.iter().copied() {
                vm.push_dynamic_vm_root(value);
            }
            // GNU `Fmacroexpand` applies macro expanders directly.  Only the
            // ordinary `eval_sub` macro-call path specbinds
            // `lexical-binding`; byte-compiled bytecomp/macroexp code depends
            // on the caller's visible dynamic value while compiling source.
            let expanded = vm.call_function(function, args)?;
            vm.ctx
                .note_runtime_macro_expansion(form, expand_start.elapsed());
            Ok(expanded)
        })
    }
}

impl crate::emacs_core::builtins::higher_order::SortRuntime for Vm<'_> {
    fn call_sort_function1(&mut self, function: Value, arg: Value) -> Result<Value, Flow> {
        self.with_vm_root_scope(|vm| {
            vm.push_dynamic_vm_root(arg);
            vm.call_function1(function, arg)
        })
    }

    fn call_sort_function2(
        &mut self,
        function: Value,
        arg0: Value,
        arg1: Value,
    ) -> Result<Value, Flow> {
        self.with_vm_root_scope(|vm| {
            vm.push_dynamic_vm_root(arg0);
            vm.push_dynamic_vm_root(arg1);
            vm.call_function2(function, arg0, arg1)
        })
    }

    fn root_sort_value(&mut self, value: Value) {
        self.push_dynamic_vm_root(value);
    }

    fn compare_sort_keys(
        &mut self,
        left: &Value,
        right: &Value,
    ) -> Result<std::cmp::Ordering, Flow> {
        crate::emacs_core::builtins::symbols::compare_value_lt(self.ctx, left, right)
    }
}

// -- Arithmetic helpers --

pub(crate) fn condition_frame_resume(frame: ConditionFrame) -> ResumeTarget {
    match frame {
        ConditionFrame::Catch { resume, .. } | ConditionFrame::ConditionCase { resume, .. } => {
            resume
        }
        ConditionFrame::HandlerBind { .. } | ConditionFrame::SkipConditions { .. } => {
            unreachable!("VM handler stack only mirrors catch/condition-case frames")
        }
    }
}

fn unwind_handlers_to_selected_resume(
    handlers: &mut HandlerStack,
    condition_stack: &mut Vec<ConditionFrame>,
    selected_resume: Option<&ResumeTarget>,
) -> Option<ResumeTarget> {
    while let Some(handler) = handlers.pop() {
        match handler {
            Handler::Condition => {
                let resume = condition_frame_resume(
                    condition_stack
                        .pop()
                        .expect("handler stack and condition stack diverged"),
                );
                if selected_resume.is_some_and(|selected| &resume == selected) {
                    return Some(resume);
                }
            }
        }
    }
    None
}

fn resolve_switch_target(func: &ByteCodeFunction, raw_addr: i64) -> Result<usize, Flow> {
    let raw_addr = usize::try_from(raw_addr).map_err(|_| {
        signal(
            "error",
            vec![Value::string(format!(
                "invalid GNU switch target byte offset {}",
                raw_addr
            ))],
        )
    })?;

    if let Some(offset_map) = func.executable_gnu_byte_offset_map() {
        offset_map
            .binary_search_by_key(&raw_addr, |entry| entry.byte_offset)
            .map(|index| offset_map[index].instruction_index)
            .map_err(|_| {
                signal(
                    "error",
                    vec![Value::string(format!(
                        "invalid GNU switch target byte offset {}",
                        raw_addr
                    ))],
                )
            })
    } else {
        Ok(raw_addr)
    }
}

/// Extract a `SymId` from a bytecode constants vector entry without
/// going through the global string interner.
///
/// `Op::VarRef` / `Op::VarSet` / `Op::VarBind` all reference variables
/// by index into the function's constants table.  Each constant is
/// already a `Value::Symbol(SymId)`, so we can extract the SymId via a
/// pure tag inspection.  Going through `as_symbol_name() -> &str ->
/// intern() -> SymId` instead would acquire the global interner
/// `RwLock` twice per opcode, which dominated debug-build runtime when
/// the byte-compiler iterated over hot loops.
///
/// When `read-positioning-symbols` wraps constants as symbol-with-pos,
/// we transparently unwrap to the bare symbol SymId.
fn sym_id_at(constants: &[Value], idx: u16) -> SymId {
    constants
        .get(idx as usize)
        .and_then(|v| {
            v.as_symbol_id().or_else(|| {
                v.as_symbol_with_pos_sym()
                    .and_then(|sym| sym.as_symbol_id())
            })
        })
        .unwrap_or_else(|| intern("nil"))
}
#[cfg(test)]
#[path = "tests/vm.rs"]
mod tests;
