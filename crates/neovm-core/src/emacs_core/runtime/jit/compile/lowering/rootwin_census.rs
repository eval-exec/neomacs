//! Compilation-owned root-window diagnostics. Threading: active counters are
//! scalar state on one compiler thread, independent of Lisp/mutator state.

use super::ROOTWIN_CARRY;

/// A count of root-window stores, rather than slots or operand-stack depth.
/// Threading: copied diagnostic data contains no SSA or runtime handles.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RootWinStoreCount(u32);

impl RootWinStoreCount {
    const ZERO: Self = Self(0);

    #[inline]
    fn increment(&mut self) {
        self.0 += 1;
    }
}

impl std::fmt::Display for RootWinStoreCount {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

impl From<RootWinStoreCount> for u32 {
    #[inline]
    fn from(count: RootWinStoreCount) -> Self {
        count.0
    }
}

const _: () = assert!(std::mem::size_of::<RootWinStoreCount>() == std::mem::size_of::<u32>());
static_assertions::assert_impl_all!(RootWinStoreCount: Send, Sync);

/// Named store-count domains prevent emitted/elided tuple ordering at the
/// production dump seam. Threading: this snapshot is copied scalar data.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RootWinCounts {
    emitted: RootWinStoreCount,
    elided: RootWinStoreCount,
}

impl RootWinCounts {
    pub(super) const ZERO: Self = Self {
        emitted: RootWinStoreCount::ZERO,
        elided: RootWinStoreCount::ZERO,
    };

    #[inline]
    pub(super) fn note_emitted(&mut self) {
        self.emitted.increment();
    }

    #[inline]
    pub(super) fn note_elided(&mut self) {
        self.elided.increment();
    }

    #[inline]
    pub(crate) fn emitted(self) -> RootWinStoreCount {
        self.emitted
    }

    #[inline]
    pub(crate) fn elided(self) -> RootWinStoreCount {
        self.elided
    }
}

#[cfg(test)]
impl From<RootWinCounts> for (u32, u32) {
    fn from(counts: RootWinCounts) -> Self {
        (counts.emitted.into(), counts.elided.into())
    }
}

static_assertions::assert_impl_all!(RootWinCounts: Send, Sync);
const _: () =
    assert!(std::mem::size_of::<RootWinCounts>() == 2 * std::mem::size_of::<RootWinStoreCount>());

/// Own one compilation's active diagnostic census. Enter unconditionally at
/// each emitter's function boundary, even when no root window is emitted.
/// A nested scope starts from zero and restores its parent's scalar census on
/// every exit. The store-elision vector remains under the emitter's existing
/// control-flow lifecycle and keeps its reusable allocation; this scope does
/// not make store-elision history or the entire lowering pipeline reentrant.
///
/// Threading: the scope restores compiler-thread cells and cannot cross
/// threads. No borrow, allocation, lock or runtime/Lisp operation is held.
#[derive(Debug)]
#[must_use = "dropping the scope restores the enclosing compiler census"]
pub(crate) struct RootWinCounterScope {
    previous: RootWinCounts,
    _thread: std::marker::PhantomData<*const ()>,
}

static_assertions::assert_not_impl_any!(RootWinCounterScope: Send, Sync);
const _: () =
    assert!(std::mem::size_of::<RootWinCounterScope>() == std::mem::size_of::<RootWinCounts>());

impl RootWinCounterScope {
    pub(crate) fn enter() -> Self {
        Self {
            previous: ROOTWIN_CARRY.with(|carry| carry.counts.replace(RootWinCounts::ZERO)),
            _thread: std::marker::PhantomData,
        }
    }

    /// Read the active compilation's census before handing off its CLIF.
    pub(crate) fn snapshot(&self) -> RootWinCounts {
        ROOTWIN_CARRY.with(|carry| carry.counts.get())
    }

    /// Finish only after the sink successfully defines the leaf. Failed or
    /// unwound compilations never publish a last-completed test observation.
    pub(crate) fn finish(self) {
        #[cfg(test)]
        LAST_COMPLETED.with(|counts| counts.set(self.snapshot()));
    }
}

impl Drop for RootWinCounterScope {
    fn drop(&mut self) {
        // Scalar Cell restoration cannot borrow or panic. During TLS teardown
        // there is no remaining compiler to observe an inaccessible census.
        let _ = ROOTWIN_CARRY.try_with(|carry| carry.counts.set(self.previous));
    }
}

#[cfg(test)]
thread_local! {
    /// Test-only completed-leaf observer; active scopes never read this cell.
    static LAST_COMPLETED: std::cell::Cell<RootWinCounts> =
        const { std::cell::Cell::new(RootWinCounts::ZERO) };
}

#[cfg(test)]
pub(super) fn last_completed_counts() -> RootWinCounts {
    LAST_COMPLETED.with(|counts| counts.get())
}

#[cfg(test)]
#[path = "tests/rootwin_census.rs"]
mod tests;
