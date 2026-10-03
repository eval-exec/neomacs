//! Numeric test-thread observations at real property-key construction sites.
//! No Lisp Value, layout view, Context, buffer or frame pointer is stored here.

use std::{cell::Cell, marker::PhantomData, rc::Rc};

/// Copied numeric observations owned exclusively by the executing test thread.
/// Independent mutators/tests neither share nor publish these observations.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Counts {
    pub(crate) heap_materializations: usize,
    pub(crate) inline_constructions: usize,
    pub(crate) alias_upgrades: usize,
}

thread_local! {
    static FORCED: Cell<Option<bool>> = const { Cell::new(None) };
    static COUNTS: Cell<Counts> = const { Cell::new(Counts {
        heap_materializations: 0, inline_constructions: 0, alias_upgrades: 0,
    }) };
}

/// Numeric test-thread-exclusive scope; Rc prevents Send/Sync. Drop restores
/// the enclosing selector/counts on ordinary return and unwinding. No Lisp TLS.
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

#[inline]
pub(crate) fn forced() -> Option<bool> {
    FORCED.with(Cell::get)
}

#[inline]
pub(crate) fn counts() -> Counts {
    COUNTS.with(Cell::get)
}

#[inline]
pub(crate) fn reset_counts() {
    COUNTS.with(|slot| slot.set(Counts::default()));
}

fn note(update: impl FnOnce(&mut Counts)) {
    COUNTS.with(|slot| {
        let mut counts = slot.get();
        update(&mut counts);
        slot.set(counts);
    });
}

#[inline]
pub(crate) fn note_heap_materialization() {
    note(|counts| counts.heap_materializations += 1);
}

#[inline]
pub(crate) fn note_inline_construction() {
    note(|counts| counts.inline_constructions += 1);
}

#[inline]
pub(crate) fn note_alias_upgrade() {
    note(|counts| counts.alias_upgrades += 1);
}
