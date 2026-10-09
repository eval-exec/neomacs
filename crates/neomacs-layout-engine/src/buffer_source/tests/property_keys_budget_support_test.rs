//! Numeric test scope over the existing source-budget override. No Lisp Value,
//! Context, heap, buffer or frame reference is stored or published here.

use std::{marker::PhantomData, rc::Rc};

/// This guard owns one test thread's numeric override. Rc prevents Send/Sync;
/// Drop restores the exact enclosing None/Some state, including on unwinding.
/// Independent tests/mutators have distinct existing thread-local slots.
pub(crate) struct SourceBudgetGuard {
    previous: Option<bool>,
    _thread_owned: PhantomData<Rc<()>>,
}

impl SourceBudgetGuard {
    pub(crate) fn set(enabled: bool) -> Self {
        Self::replace(Some(enabled))
    }

    fn replace(value: Option<bool>) -> Self {
        Self {
            previous: super::SYNC_SOURCE_BUDGET_OVERRIDE.with(|slot| slot.replace(value)),
            _thread_owned: PhantomData,
        }
    }
}

impl Drop for SourceBudgetGuard {
    fn drop(&mut self) {
        super::SYNC_SOURCE_BUDGET_OVERRIDE.with(|slot| slot.set(self.previous));
    }
}

#[test]
fn property_keys_source_budget_scope_restores_none_some_and_unwind() {
    // This outer scope restores whatever state the test thread began with.
    let _initial = SourceBudgetGuard::replace(None);
    assert_eq!(
        super::SYNC_SOURCE_BUDGET_OVERRIDE.with(std::cell::Cell::get),
        None
    );
    {
        let _outer = SourceBudgetGuard::set(false);
        assert_eq!(
            super::SYNC_SOURCE_BUDGET_OVERRIDE.with(std::cell::Cell::get),
            Some(false)
        );
        assert!(!super::sync_source_budget_enabled());
        let unwind = std::panic::catch_unwind(|| {
            let _inner = SourceBudgetGuard::set(true);
            assert_eq!(
                super::SYNC_SOURCE_BUDGET_OVERRIDE.with(std::cell::Cell::get),
                Some(true)
            );
            assert!(super::sync_source_budget_enabled());
            panic!("numeric source-budget scope unwind control");
        });
        assert!(unwind.is_err());
        assert_eq!(
            super::SYNC_SOURCE_BUDGET_OVERRIDE.with(std::cell::Cell::get),
            Some(false)
        );
        assert!(!super::sync_source_budget_enabled());
    }
    assert_eq!(
        super::SYNC_SOURCE_BUDGET_OVERRIDE.with(std::cell::Cell::get),
        None
    );
}
