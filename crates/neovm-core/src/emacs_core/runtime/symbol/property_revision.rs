//! Evaluator-thread revision of property-list writes to layout dependencies.
//! Category and overlay-arrow symbols are observed when attached/read. Event
//! metadata and other unrelated symbol properties do not invalidate rows.
use super::{SymId, Value};
use std::cell::{Cell, RefCell};

thread_local! {
    static REVISION: Cell<u64> = const { Cell::new(0) };
    static OBSERVED: RefCell<rustc_hash::FxHashSet<SymId>> = RefCell::new(Default::default());
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SymbolPropertyRevision(u64);

impl SymbolPropertyRevision {
    pub fn current() -> Self {
        Self(REVISION.with(Cell::get))
    }

    /// Symbol identities are permanent; remembering a dependency never roots
    /// a Lisp property value or leaves a dangling object after collection.
    pub fn observe(symbol: SymId) {
        OBSERVED.with(|observed| {
            observed.borrow_mut().insert(symbol);
        });
    }

    pub fn observe_category_property(property: Value, value: Value) {
        if property.is_bare_symbol_named("category")
            && !value.is_nil()
            && let Some(symbol) = value.as_bare_symbol_id()
        {
            Self::observe(symbol);
        }
    }

    pub(crate) fn changed(symbol: SymId) {
        if OBSERVED.with(|observed| observed.borrow().contains(&symbol)) {
            REVISION.with(|revision| revision.set(revision.get().wrapping_add(1)));
        }
    }
}

/// Round 3w (plan-3w.md section 7, C1): this World state lives in a
/// thread-local while its World's Baton holder runs, and in the World while
/// it is parked; the Green runtime moves it at each park and resume, so
/// every Lisp thread sees one value, as in GNU. (N-mutator mode needs shared
/// storage instead: a recorded obligation.)
#[cold]
pub(crate) fn world_state_placeholder() -> Box<dyn std::any::Any> {
    Box::new((0_u64, rustc_hash::FxHashSet::<SymId>::default()))
}

#[cold]
pub(crate) fn swap_world_state(slot: &mut dyn std::any::Any) {
    let slot = slot
        .downcast_mut::<(u64, rustc_hash::FxHashSet<SymId>)>()
        .expect("carried World state");
    REVISION.with(|revision| slot.0 = revision.replace(slot.0));
    OBSERVED.with(|observed| std::mem::swap(&mut *observed.borrow_mut(), &mut slot.1));
}
