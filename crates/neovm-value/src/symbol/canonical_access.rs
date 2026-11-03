//! Canonical variable storage, below owner bindings (Phase 0 step 2).
//!
//! Once a variable is projected, its dynamic bindings live in their owner's
//! slab (`eval::owner_bindings`), and what the obarray stores is only the
//! CANONICAL value: what an owner without a binding sees. A Rust read or
//! write of a variable that mirrors GNU's C (`Vfoo`, `BVAR (current_buffer,
//! foo)`) must therefore go through the owner-aware `Context` API
//! (`Context::variable_value`, `Context::store_variable`, ...), which answers
//! the running owner's binding first.
//!
//! The obarray's value accessors take a [`CanonicalAccess`] instead of
//! answering anyone who asks: every caller names why canonical storage is the
//! right answer at its site, and the compiler lists every site that names
//! none. The reasons are the only constructors.

/// Why a site reads or writes a variable's canonical storage directly.
///
/// Zero-sized: naming the reason costs nothing at run time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CanonicalAccess {
    _sealed: (),
}

static_assertions::assert_eq_size!(CanonicalAccess, ());

impl CanonicalAccess {
    /// Construction and bootstrap: the World has no owner yet, so canonical
    /// storage is every owner's view.
    pub const BOOTSTRAP: Self = Self { _sealed: () };
    /// The evaluator's storage layer, on a cell its routing has already
    /// dispatched: unprojected, or revealed for the length of one operation.
    pub const STORAGE: Self = Self { _sealed: () };
    /// Dumping and restoring an image, which hold canonical values.
    pub const DUMP: Self = Self { _sealed: () };
    /// The owner-aware `Context` API's own access to canonical storage: the
    /// dormant answer, or the canonical half of an owner-first rule.
    pub const ROUTED: Self = Self { _sealed: () };
    /// Tests, which set up and inspect canonical storage.
    #[cfg(test)]
    pub const TEST: Self = Self { _sealed: () };
}
