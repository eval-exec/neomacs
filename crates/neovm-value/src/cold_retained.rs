//! Cold allocation retention for upper layers.
//!
//! [`ColdWorldLease`] is the opaque token the evaluator hands the edit
//! layer so a `BufferManager` can retain its World's allocation without
//! naming — or being able to reach — the World. The trait object carries
//! no methods: the value side never recovers the retained allocation,
//! it only shares its strong count. Every property that matters below
//! the evaluator is asserted here: `!Send`/`!Sync` (the last drop runs
//! on the owner thread), `!Clone` (duplication is the named
//! `duplicate_for_view`), and the fat-`Arc` size. Production mints are
//! the evaluator's two World-construction paths, which go through
//! `WorldLease::erase`; see the PROMOTION-LEDGER rows.

use std::fmt;
use std::sync::Arc;

/// Marker for a cold allocation an upper layer may retain opaquely.
///
/// No methods, no `Any`, no downcast: implementing it grants exactly the
/// ability to be held behind a [`ColdWorldLease`], never to be reached.
#[doc(hidden)]
pub trait ColdRetained: 'static {}

/// An erased lease on a [`ColdRetained`] allocation: lifetime only,
/// never mutator authority or Lisp-object reachability.
#[doc(hidden)]
pub struct ColdWorldLease(Arc<dyn ColdRetained>);
static_assertions::assert_not_impl_any!(ColdWorldLease: Send, Sync, Clone);
static_assertions::assert_eq_size!(ColdWorldLease, [usize; 2]);

impl fmt::Debug for ColdWorldLease {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ColdWorldLease")
            .field("owners", &Arc::strong_count(&self.0))
            .finish()
    }
}

impl ColdWorldLease {
    /// Wrap an already-erased lease. The evaluator's only route: it goes
    /// through `WorldLease::erase`, whose unsizing coercion of the same
    /// `Arc` leaves the strong count unchanged.
    #[doc(hidden)]
    pub fn from_erased(retained: Arc<dyn ColdRetained>) -> Self {
        Self(retained)
    }

    /// A second owning view of the same retained allocation: exactly
    /// `Arc::clone`. Cold because duplication happens only at buffer
    /// clone boundaries, never on insert/delete or char access.
    #[cold]
    pub fn duplicate_for_view(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}
