//! Fast-path re-entry classification for inlined call sites, beside the
//! effect catalog (`effect_catalog`) it derives from.
//!
//! The JIT's green-guard design classifies an INLINED call site by one
//! criterion: can the site's fast path run Lisp or release the Baton? A
//! native-arithmetic intrinsic cannot (a signal such as `arith-error` leaves
//! compiled code through the slow path), so it owes neither an entry guard
//! nor a return-side revalidation; every other inlined boundary is an
//! original call whose skipped call would have revalidated, and revalidates
//! after every release point instead.
//!
//! This is the single classification source — the lowering asks here, it
//! never keeps its own intrinsic list. Names not listed are `MayReenter`
//! (conservative): an unknown callee is a call boundary, and call sites
//! revalidate by rule rather than by membership.

/// Whether an inlined site's fast path can run Lisp or release the Baton.
///
/// The re-entry axis of the effect vocabulary: orthogonal to the catalog's
/// `Authority` (which classifies parallel admission, not fast-path
/// re-entrancy — `mod` and `sleep-for` are both `Authority::World`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FastPathReentry {
    /// Machine arithmetic over values already in registers: cannot call
    /// Lisp, grow eval depth, allocate or serve attention at the site.
    /// Signaling (`arith-error`) exits compiled code through the slow path.
    CannotReenter,
    /// The fast path can run Lisp or release the Baton (the default for
    /// catalog members: any callee may transitively reach `thread_select`
    /// or a condition/mutex wait).
    MayReenter,
}

/// The intrinsics the JIT bakes as native machine ops (`arith_op_inlines`).
/// Ascending name order; `is_inlined_intrinsic` binary-searches.
const INLINED_NATIVE_OPS: [&str; 5] = ["logand", "logior", "lognot", "logxor", "mod"];

/// The classification of `name`'s fast path.
///
/// `CannotReenter` exactly for the native-arithmetic intrinsics above; the
/// catalog's wait/read/thread family and every unlisted name are
/// `MayReenter`. Both classes are pinned by tests.
pub(crate) fn fast_path_reentry(name: &str) -> FastPathReentry {
    if INLINED_NATIVE_OPS.binary_search(&name).is_ok() {
        FastPathReentry::CannotReenter
    } else {
        FastPathReentry::MayReenter
    }
}

#[cfg(test)]
#[path = "tests/effect_reentry_test.rs"]
mod tests;
