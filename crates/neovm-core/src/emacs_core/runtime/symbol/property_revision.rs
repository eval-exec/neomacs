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
        if property.is_symbol_named("category")
            && !value.is_nil()
            && let Some(symbol) = value.as_symbol_id()
        {
            Self::observe(symbol);
        }
    }

    pub(super) fn changed(symbol: SymId) {
        if OBSERVED.with(|observed| observed.borrow().contains(&symbol)) {
            REVISION.with(|revision| revision.set(revision.get().wrapping_add(1)));
        }
    }
}
