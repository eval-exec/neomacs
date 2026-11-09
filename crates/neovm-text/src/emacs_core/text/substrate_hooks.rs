//! Host-installed hooks for the text layer's host-free leaves (stage-3b
//! C7/C8a).
//!
//! The regex compiler, the literal-find/search fast paths and the char-table
//! setters run without a `Context` in scope.  The evaluator publishes the few
//! host services they need once, at construction: the hot-path timing switch
//! and recorder, and the display layout-invalidation note.  Everything here is
//! static — a bool and plain `fn` pointers, never `dyn` — and no TLS is added.
//!
//! With tracing off, [`time_op`] costs one inlined bool load and a branch; the
//! record indirect call exists only on the enabled path.  Timing stays
//! strictly per-operation: [`time_op`] wraps one whole regex or search
//! operation, never a per-character or per-backtrack step.  Char-table
//! mutations are rare, so their hook costs one optional static call.

use std::sync::OnceLock;
use std::time::{Duration, Instant};

use neovm_value::intern::SymId;

/// Fontset charset-preference publisher, installed once by the display layer.
///
/// GNU keeps the charset order in C (`Vcharset_ordered_list`) and every fontset
/// lookup walks it directly. The split put the charset registry in this crate
/// and the fontset in the display crate above it, so the charset layer publishes
/// the preferred identities through this one static hook instead of calling
/// upward. Uninstalled means no fontset exists yet: the note is a no-op.
static CHARSET_PREFERENCE: OnceLock<fn(Vec<SymId>)> = OnceLock::new();

/// Install the fontset's charset-preference publisher.
///
/// Idempotent for the same function pointer, like the layout note; a second,
/// different hook is refused because the fontset ordering would otherwise
/// silently split between two publishers.
#[doc(hidden)]
pub fn install_charset_preference(hook: fn(Vec<SymId>)) {
    let _ = CHARSET_PREFERENCE.set(hook);
    match CHARSET_PREFERENCE.get() {
        Some(installed) if *installed as usize == hook as usize => {}
        Some(_) => panic!("a different charset-preference hook is already installed"),
        None => unreachable!("the charset-preference hook was just installed"),
    }
}

/// Publish the preferred charset identities to the installed fontset hook.
pub(crate) fn publish_charset_preference(preferred: Vec<SymId>) {
    if let Some(hook) = CHARSET_PREFERENCE.get() {
        hook(preferred);
    }
}

/// One measured text operation: the text layer's slice of
/// `emacs_core::perf_trace::HotpathOp`.
///
/// Core maps each variant back to its `HotpathOp` counter when it installs the
/// recorder; this enum is the seam's only vocabulary.
#[doc(hidden)]
#[derive(Clone, Copy)]
pub enum TextPerfOp {
    RegexCompileHit,
    RegexCompileMiss,
    RegexLiteralFind,
    RegexMatchDataChars,
    RegexpQuote,
}

/// Whether hot-path timing is enabled.  Set once by the evaluator from
/// `perf_trace::hotpath_timing_enabled()`; false until then.
static PERF_ENABLED: OnceLock<bool> = OnceLock::new();

/// Per-operation recorder, installed once by the evaluator.
static PERF_RECORD: OnceLock<fn(TextPerfOp, Duration)> = OnceLock::new();

/// Display layout-invalidation note, installed once by the evaluator.
static LAYOUT_MUTATION: OnceLock<fn()> = OnceLock::new();

#[inline(always)]
fn perf_enabled() -> bool {
    PERF_ENABLED.get().copied().unwrap_or(false)
}

/// Install the hot-path timing pair.
///
/// `enabled` is the evaluator's existing `hotpath_timing_enabled()` answer;
/// `record` is the core adapter that maps [`TextPerfOp`] to the existing
/// `HotpathOp` counters.  Idempotent.
#[doc(hidden)]
pub fn install_perf(enabled: bool, record: fn(TextPerfOp, Duration)) {
    let _ = PERF_ENABLED.set(enabled);
    let _ = PERF_RECORD.set(record);
}

/// Install the char-table layout-invalidation note.
///
/// The hook is the display layer's exact note (no wrapper).  A second install
/// with a different pointer is refused: the layout revision epoch is captured
/// from that exact function, so a different hook would silently stop
/// invalidating layout.
#[doc(hidden)]
pub fn install_layout_mutation(hook: fn()) {
    let _ = LAYOUT_MUTATION.set(hook);
    match LAYOUT_MUTATION.get() {
        Some(installed) if *installed as usize == hook as usize => {}
        Some(_) => panic!("a different char-table layout hook is already installed"),
        None => unreachable!("the layout hook was just installed"),
    }
}

/// Measure one whole text operation, unless hot-path timing is disabled.
///
/// Mirrors `perf_trace::time_op`: the enable test runs before the clock read,
/// so the disabled path is one inlined bool load and a branch.
#[inline]
pub fn time_op<R>(op: TextPerfOp, f: impl FnOnce() -> R) -> R {
    if !perf_enabled() {
        return f();
    }
    let start = Instant::now();
    let result = f();
    if let Some(record) = PERF_RECORD.get() {
        record(op, start.elapsed());
    }
    result
}

/// Invalidate display layout after a char-table mutation.
///
/// A no-op before the evaluator installs the note: the layout epoch is core
/// display state, and a char-table used outside an evaluator has no display
/// rows to invalidate.
#[inline]
pub fn note_char_table_layout_mutation() {
    if let Some(hook) = LAYOUT_MUTATION.get() {
        hook();
    }
}
