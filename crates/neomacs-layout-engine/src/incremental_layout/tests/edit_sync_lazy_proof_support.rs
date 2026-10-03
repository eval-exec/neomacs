//! Numeric probes attached to actual source reads and retained Sync admission.
//! No Lisp value or Context pointer is stored; each test thread owns its probes.

use std::{cell::Cell, marker::PhantomData, rc::Rc};

/// Numeric observations owned by one test thread, copied before its Context
/// drops. Independent tests/mutators never share or publish this state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Counts {
    pub(crate) source_proof_calls: usize,
    pub(crate) char_queries: usize,
    pub(crate) property_queries: usize,
    pub(crate) sync_admissions: usize,
    pub(crate) lazy_entries: usize,
}

thread_local! {
    static FORCED: Cell<Option<bool>> = const { Cell::new(None) };
    static COUNTS: Cell<Counts> = const { Cell::new(Counts {
        source_proof_calls: 0, char_queries: 0, property_queries: 0,
        sync_admissions: 0, lazy_entries: 0,
    }) };
}

/// A test-thread-exclusive numeric policy/probe scope. The Rc marker prevents
/// Send/Sync; Drop restores nested state, including on unwinding. No Lisp TLS.
pub(crate) struct Guard {
    previous: Option<bool>,
    previous_counts: Counts,
    _thread_owned: PhantomData<Rc<()>>,
}
impl Guard {
    pub(crate) fn set(enabled: bool) -> Self {
        let previous = FORCED.with(|slot| slot.replace(Some(enabled)));
        let previous_counts = COUNTS.with(|slot| slot.replace(Counts::default()));
        Self {
            previous,
            previous_counts,
            _thread_owned: PhantomData,
        }
    }
}
impl Drop for Guard {
    fn drop(&mut self) {
        FORCED.with(|slot| slot.set(self.previous));
        COUNTS.with(|slot| slot.set(self.previous_counts));
    }
}
pub(crate) fn forced() -> Option<bool> {
    FORCED.with(Cell::get)
}
pub(crate) fn counts() -> Counts {
    COUNTS.with(Cell::get)
}
fn note(update: impl FnOnce(&mut Counts)) {
    COUNTS.with(|slot| {
        let mut counts = slot.get();
        update(&mut counts);
        slot.set(counts);
    });
}
pub(crate) fn note_source_proof() {
    note(|counts| counts.source_proof_calls += 1);
}
pub(crate) fn note_char_query() {
    note(|counts| counts.char_queries += 1);
}
pub(crate) fn note_property_query() {
    note(|counts| counts.property_queries += 1);
}
pub(crate) fn note_sync_admission() {
    note(|counts| counts.sync_admissions += 1);
}
pub(crate) fn note_lazy_entry() {
    note(|counts| counts.lazy_entries += 1);
}
