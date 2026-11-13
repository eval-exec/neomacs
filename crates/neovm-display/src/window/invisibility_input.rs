//! Owned inputs to GNU invisible_prop's ordered membership checks.
use neovm_value::value::Value;
use rustc_hash::FxHashSet;

pub use neovm_eval::emacs_core::eval::data::layout_input::invisibility_input::LayoutInvisibilityInput;
pub(crate) use neovm_eval::emacs_core::eval::data::layout_input::invisibility_input::{
    Entry, Selection,
};

pub(crate) trait LayoutInvisibilityInputExt: Sized {
    fn capture(value: Value) -> Self;
}

impl LayoutInvisibilityInputExt for LayoutInvisibilityInput {
    fn capture(value: Value) -> Self {
        if value == Value::T {
            return Self(Selection::All);
        }
        let mut tail = value;
        let mut seen = FxHashSet::default();
        let mut entries = Vec::new();
        while tail.is_cons() && seen.insert(tail.bits()) {
            let entry = tail.cons_car();
            entries.push(Entry {
                identity: entry.bits(),
                category: entry
                    .is_cons()
                    .then(|| (entry.cons_car().bits(), !entry.cons_cdr().is_nil())),
            });
            tail = tail.cons_cdr();
        }
        if entries.is_empty() {
            Self::default()
        } else {
            Self(Selection::Entries(entries.into()))
        }
    }
}
