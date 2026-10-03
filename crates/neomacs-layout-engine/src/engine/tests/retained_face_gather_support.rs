//! Test-thread numeric observations at the real retained-face materializer.
//! This module stores no Lisp value, Context, row, engine or frame pointer.

use std::{cell::Cell, marker::PhantomData, rc::Rc};

/// Counts owned exclusively by the executing test thread. These copied numeric
/// observations are not shared or published to other tests or Lisp mutators.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Counts {
    pub(crate) materializations: usize,
}

thread_local! {
    static FORCED: Cell<Option<bool>> = const { Cell::new(None) };
    static COUNTS: Cell<Counts> = const { Cell::new(Counts { materializations: 0 }) };
}

/// A numeric test-thread-exclusive scope. The Rc marker makes this guard
/// !Send and !Sync; Drop restores the enclosing policy and counts on ordinary
/// return and unwinding. No Lisp TLS cache or runtime ownership is introduced.
pub(crate) struct Guard {
    previous: Option<bool>,
    previous_counts: Counts,
    _thread_owned: PhantomData<Rc<()>>,
}

impl Guard {
    pub(crate) fn set(enabled: bool) -> Self {
        Self {
            previous: FORCED.with(|slot| slot.replace(Some(enabled))),
            previous_counts: COUNTS.with(|slot| slot.replace(Counts::default())),
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

pub(crate) fn reset_counts() {
    COUNTS.with(|slot| slot.set(Counts::default()));
}

pub(crate) fn note_materialization() {
    COUNTS.with(|slot| {
        let mut counts = slot.get();
        counts.materializations += 1;
        slot.set(counts);
    });
}
