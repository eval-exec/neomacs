//! Numeric test-only witnesses at the original hash construction sites.
//! Each thread owns policy and counters for its exclusive layout attempts.
//! No Lisp handles, heap pointers, roots, frame publication, or caches live here.
use std::cell::Cell;
use std::marker::PhantomData;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Counts {
    pub(crate) legacy_plans: u64,
    pub(crate) plan_insertions: u64,
    pub(crate) legacy_installs: u64,
    pub(crate) install_insertions: u64,
    pub(crate) dense_plans: u64,
    pub(crate) dense_installs: u64,
}

thread_local! {
    static FORCED: Cell<Option<bool>> = const { Cell::new(None) };
    static COUNTS: Cell<Counts> = const { Cell::new(Counts {
        legacy_plans: 0, plan_insertions: 0, legacy_installs: 0,
        install_insertions: 0, dense_plans: 0, dense_installs: 0,
    }) };
}

#[inline]
pub(crate) fn forced() -> Option<bool> {
    FORCED.with(Cell::get)
}
#[inline]
pub(crate) fn counts() -> Counts {
    COUNTS.with(Cell::get)
}
#[inline]
fn update(f: impl FnOnce(&mut Counts)) {
    COUNTS.with(|cell| {
        let mut counts = cell.get();
        f(&mut counts);
        cell.set(counts);
    });
}
#[inline]
pub(crate) fn note_legacy_plan() {
    update(|c| c.legacy_plans += 1);
}
#[inline]
pub(crate) fn note_plan_insertion() {
    update(|c| c.plan_insertions += 1);
}
#[inline]
pub(crate) fn note_legacy_install() {
    update(|c| c.legacy_installs += 1);
}
#[inline]
pub(crate) fn note_install_insertion() {
    update(|c| c.install_insertions += 1);
}
#[inline]
pub(crate) fn note_dense_plan() {
    update(|c| c.dense_plans += 1);
}
#[inline]
pub(crate) fn note_dense_install() {
    update(|c| c.dense_installs += 1);
}

/// One mutator/test-thread's numeric scope, including the competing Still
/// policy. This !Send/!Sync owner restores every previous value on unwind;
/// it may not migrate while controlling thread-local numeric witnesses.
pub(crate) struct Guard {
    prior: Option<bool>,
    counts: Counts,
    still: Option<bool>,
    _thread: PhantomData<Rc<()>>,
}
impl Guard {
    /// Observe the actual startup policy without writing the Dense override.
    /// This exclusively owned test-thread scope resets numeric witnesses and
    /// selects competing Still OFF, restoring both and the original None policy
    /// on unwind. No Lisp state or production selector state is stored here.
    pub(crate) fn observe_default() -> Self {
        let prior = FORCED.with(Cell::get);
        assert_eq!(prior, None, "actual Dense default must not be forced");
        let counts = COUNTS.with(|cell| cell.replace(Counts::default()));
        let still = super::STILL_OVERRIDE.with(|cell| cell.replace(Some(false)));
        Self {
            prior,
            counts,
            still,
            _thread: PhantomData,
        }
    }

    pub(crate) fn set(enabled: bool) -> Self {
        let prior = FORCED.with(|cell| cell.replace(Some(enabled)));
        let counts = COUNTS.with(|cell| cell.replace(Counts::default()));
        let still = super::STILL_OVERRIDE.with(|cell| cell.replace(Some(false)));
        Self {
            prior,
            counts,
            still,
            _thread: PhantomData,
        }
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        FORCED.with(|cell| cell.set(self.prior));
        COUNTS.with(|cell| cell.set(self.counts));
        super::STILL_OVERRIDE.with(|cell| cell.set(self.still));
    }
}

#[cfg(test)]
#[path = "edit_sync_dense_index_scope_test.rs"]
mod scope_tests;
