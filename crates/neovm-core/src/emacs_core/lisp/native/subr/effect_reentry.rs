//! Fast-path re-entry classification for inlined call sites, derived from the
//! E1 cold catalog (`effect_catalog`) it is folded into.
//!
//! The JIT's green-guard design classifies an INLINED call site by one
//! criterion: can the site's fast path run Lisp or release the Baton? A
//! native-arithmetic intrinsic cannot (a signal such as `arith-error` leaves
//! compiled code through the slow path), so it owes neither an entry guard
//! nor a return-side revalidation; every other inlined boundary is an
//! original call whose skipped call would have revalidated, and revalidates
//! after every release point instead.
//!
//! The single source is [`INLINED_NATIVE_OPS`] in the cold schema; the source
//! scanner folds it into every generated dispatch row, and this module is
//! only the lowering-facing query over that generated table. Names not listed
//! are `MayReenter` (conservative): an unknown callee is a call boundary, and
//! call sites revalidate by rule rather than by membership.

use super::effect_catalog::reentry_of_primitive_name;

pub(crate) use super::effect_schema::FastPathReentry;
#[cfg(test)]
pub(crate) use super::effect_schema::INLINED_NATIVE_OPS;

/// The classification of `name`'s fast path from the generated cold table.
///
/// `CannotReenter` exactly for the native-arithmetic intrinsics above; the
/// catalog's wait/read/thread family and every unlisted name are
/// `MayReenter`. Both classes are pinned by tests, including the cross-module
/// pin against `arith_op_inlines`.
pub(crate) fn fast_path_reentry(name: &str) -> FastPathReentry {
    reentry_of_primitive_name(name)
}

#[cfg(test)]
#[path = "tests/effect_reentry_test.rs"]
mod tests;
