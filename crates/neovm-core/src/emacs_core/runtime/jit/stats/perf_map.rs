//! Per-function names for compiled leaves: `lisp:<fn>#<id>:<tier>`.
//!
//! Every JIT leaf used to be declared as `__neovm_jit_leaf` or
//! `__neovm_mir_leaf`, so perf (which reads cranelift-jit's
//! `/tmp/perf-<pid>.map`, written whenever `PERF_BUILDID_DIR` is set) could
//! not split JIT time by Lisp function. When naming is enabled
//! ([`super::naming_enabled`]) the compile-miss path opens a
//! [`LeafLabelScope`] and the two JIT wrappers declare the entry under
//! [`active_label`] instead. The name is a declaration-table string only:
//! the generated code is identical either way. With naming off the scope is
//! never opened and the legacy names are declared.

use std::cell::{Cell, RefCell};

use crate::emacs_core::bytecode::ByteCodeFunction;
use crate::emacs_core::intern::SymId;

/// Longest label name kept, in bytes (cut on a char boundary).
const MAX_NAME_BYTES: usize = 128;

/// Which tier a label names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LabelTier {
    Baseline,
    Mir,
    /// An OSR variant entered at this loop-header pc.
    Osr(usize),
}

/// The function being compiled: its `compiled_id` and sanitized name.
struct ActiveLabel {
    id: u64,
    name: Box<str>,
}

thread_local! {
    /// The label of the compile in progress on this thread (scoped by
    /// [`LeafLabelScope`]; `None` outside a naming compile).
    static ACTIVE_LABEL: RefCell<Option<ActiveLabel>> = const { RefCell::new(None) };
    /// The symbol a speculated call site is about to resolve a callee leaf
    /// for (set by the spec shim's slow half, only under naming): at that
    /// point the callee's backtrace frame is not pushed yet, so the frame
    /// walk alone would name the CALLER.
    static PENDING_CALLEE: Cell<Option<SymId>> = const { Cell::new(None) };
}

/// Publishes the label of one compile and restores the outer one on drop
/// (compiles can nest on a thread only through the cache-miss paths).
pub(crate) struct LeafLabelScope(Option<ActiveLabel>);

impl LeafLabelScope {
    /// Only called when naming is enabled. `name` is the callee's symbol
    /// when the call site knows it; otherwise the label falls back to
    /// `anon[<first symbol constants>]`.
    pub(crate) fn enter(id: u64, name: Option<SymId>, func: &ByteCodeFunction) -> Self {
        let name = match name {
            Some(sym) => sanitize(symbol_name(sym)),
            None => anon_name(func),
        };
        LeafLabelScope(ACTIVE_LABEL.with(|l| l.replace(Some(ActiveLabel { id, name }))))
    }
}

impl Drop for LeafLabelScope {
    fn drop(&mut self) {
        let outer = self.0.take();
        ACTIVE_LABEL.with(|l| *l.borrow_mut() = outer);
    }
}

/// `lisp:<name>#<id>:<tier>` for the compile in progress, or `None` when no
/// [`LeafLabelScope`] is open (naming off, or a compile outside the cache).
pub(crate) fn active_label(tier: LabelTier) -> Option<String> {
    ACTIVE_LABEL.with(|l| {
        l.borrow()
            .as_ref()
            .map(|a| format_label(&a.name, a.id, tier))
    })
}

/// The sanitized name of the function being compiled, if a naming compile
/// is in progress (the `NEOVM_JIT_PROFILE` row's `name` column).
pub(crate) fn active_label_name() -> Option<String> {
    ACTIVE_LABEL.with(|l| l.borrow().as_ref().map(|a| a.name.to_string()))
}

/// The label format: `lisp:<name>#<id>:baseline|mir|osr@<pc>`.
pub(crate) fn format_label(name: &str, id: u64, tier: LabelTier) -> String {
    match tier {
        LabelTier::Baseline => format!("lisp:{name}#{id}:baseline"),
        LabelTier::Mir => format!("lisp:{name}#{id}:mir"),
        LabelTier::Osr(pc) => format!("lisp:{name}#{id}:osr@{pc}"),
    }
}

/// The `<name>` part of a label made by [`format_label`].
pub(crate) fn label_name(label: &str) -> Option<&str> {
    let rest = label.strip_prefix("lisp:")?;
    Some(&rest[..rest.rfind('#')?])
}

/// Whitespace and control characters become `_` (a perf-map line is
/// `start size name\n`; Lisp names may contain anything), and the result is
/// capped at [`MAX_NAME_BYTES`] on a char boundary.
pub(crate) fn sanitize(name: &str) -> Box<str> {
    let mut out = String::with_capacity(name.len().min(MAX_NAME_BYTES));
    for c in name.chars() {
        let c = if c.is_whitespace() || c.is_control() {
            '_'
        } else {
            c
        };
        if out.len() + c.len_utf8() > MAX_NAME_BYTES {
            break;
        }
        out.push(c);
    }
    out.into_boxed_str()
}

/// `anon[a;b]` from the function's first two symbol constants (what it
/// calls or reads first), or `anon` when it has none.
pub(crate) fn anon_name(func: &ByteCodeFunction) -> Box<str> {
    let syms: Vec<&str> = func
        .constants
        .iter()
        .filter_map(|v| v.as_symbol_id())
        .take(2)
        .map(symbol_name)
        .collect();
    if syms.is_empty() {
        "anon".into()
    } else {
        sanitize(&format!("anon[{}]", syms.join(";")))
    }
}

fn symbol_name(sym: SymId) -> &'static str {
    crate::emacs_core::intern::resolve_sym_lisp_string(sym)
        .as_utf8_str()
        .unwrap_or("<non-utf8>")
}

/// Record the symbol a speculated call site is resolving a callee for (see
/// `PENDING_CALLEE`). Cold: once per site per epoch move, under naming only.
#[cold]
#[inline(never)]
pub(crate) fn set_pending_callee(sym: SymId) {
    PENDING_CALLEE.with(|p| p.set(Some(sym)));
}

/// Take the pending speculated-callee symbol, if any.
pub(crate) fn take_pending_callee() -> Option<SymId> {
    PENDING_CALLEE.with(Cell::take)
}

#[cfg(test)]
thread_local! {
    static LAST_ENTRY_NAME: RefCell<String> = const { RefCell::new(String::new()) };
}

/// Test-only: remember the entry name a JIT wrapper declared.
#[cfg(test)]
pub(crate) fn record_entry_name_for_test(name: &str) {
    LAST_ENTRY_NAME.with(|n| *n.borrow_mut() = name.to_string());
}

/// Test-only: the entry name the last JIT wrapper on this thread declared.
#[cfg(test)]
pub(crate) fn last_entry_name_for_test() -> String {
    LAST_ENTRY_NAME.with(|n| n.borrow().clone())
}
