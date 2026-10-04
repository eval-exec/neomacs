//! Numeric-only policy and witnesses owned by one test/mutator thread.
//! No Lisp handle, heap pointer, frame publication or cached source is stored.
use std::cell::Cell;
use std::marker::PhantomData;
use std::rc::Rc;

/// Original iterator/query sites and the actual completed Sync install site.
/// Thread-local copies are private numeric observations, never shared owners.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Counts {
    pub(crate) iterators: u64,
    pub(crate) points: u64,
    pub(crate) queries: u64,
    pub(crate) sync_iterators: u64,
    pub(crate) sync_points: u64,
    pub(crate) sync_queries: u64,
    pub(crate) completed_sync_installs: u64,
    pub(crate) shortcuts: u64,
}
thread_local! {
    static FORCED: Cell<Option<bool>> = const { Cell::new(None) };
    static COUNTS: Cell<Counts> = const { Cell::new(Counts {
        iterators: 0, points: 0, queries: 0,
        sync_iterators: 0, sync_points: 0, sync_queries: 0,
        completed_sync_installs: 0, shortcuts: 0,
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
pub(crate) fn still() -> Option<bool> {
    super::STILL_OVERRIDE.with(Cell::get)
}
#[inline]
fn update(f: impl FnOnce(&mut Counts)) {
    COUNTS.with(|cell| {
        let mut c = cell.get();
        f(&mut c);
        cell.set(c);
    });
}
#[inline]
pub(crate) fn note_iterator(sync: bool) {
    update(|c| {
        c.iterators += 1;
        if sync {
            c.sync_iterators += 1;
        }
    });
}
#[inline]
pub(crate) fn note_point(sync: bool) {
    update(|c| {
        c.points += 1;
        if sync {
            c.sync_points += 1;
        }
    });
}
#[inline]
pub(crate) fn note_query(sync: bool) {
    update(|c| {
        c.queries += 1;
        if sync {
            c.sync_queries += 1;
        }
    });
}
#[inline]
pub(crate) fn note_completed_sync_install() {
    update(|c| c.completed_sync_installs += 1);
}
#[inline]
pub(crate) fn note_shortcut() {
    update(|c| c.shortcuts += 1);
}

/// !Send/!Sync numeric scope tied to its original mutator/test thread.
/// Nested scopes and panic unwinding restore every override, witness and Still
/// policy; neither Drop nor observation touches Lisp, source or heap owners.
pub(crate) struct Guard {
    prior: Option<bool>,
    counts: Counts,
    still: Option<bool>,
    _thread: PhantomData<Rc<()>>,
}
impl Guard {
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
