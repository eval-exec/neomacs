//! Scope ownership survives nesting, failed attempts and unwinding.
//! Threading: these tests touch only their invoking compiler thread's cells.

use super::*;

fn note_store() {
    ROOTWIN_CARRY.with(|carry| {
        let mut counts = carry.counts.get();
        counts.note_emitted();
        carry.counts.set(counts);
    });
}

#[test]
fn nested_census_restores_the_outer_counts() {
    let outer = RootWinCounterScope::enter();
    note_store();
    let outer_counts = outer.snapshot();
    {
        let inner = RootWinCounterScope::enter();
        assert_eq!(inner.snapshot(), RootWinCounts::ZERO);
        note_store();
        note_store();
        inner.finish();
    }
    assert_eq!(outer.snapshot(), outer_counts);
    assert_eq!(u32::from(last_completed_counts().emitted()), 2);
    outer.finish();
    assert_eq!(last_completed_counts(), outer_counts);
}

#[test]
fn early_return_restores_active_counts_without_publishing() {
    fn incomplete_compile() {
        let _scope = RootWinCounterScope::enter();
        note_store();
    }

    let completed = RootWinCounterScope::enter();
    note_store();
    note_store();
    completed.finish();
    let last_completed = last_completed_counts();
    let outer = RootWinCounterScope::enter();
    note_store();
    let outer_counts = outer.snapshot();
    incomplete_compile();
    assert_eq!(outer.snapshot(), outer_counts);
    assert_eq!(last_completed_counts(), last_completed);
}

#[test]
fn unwind_restores_active_counts_without_publishing() {
    let completed = RootWinCounterScope::enter();
    note_store();
    note_store();
    completed.finish();
    let last_completed = last_completed_counts();
    let outer = RootWinCounterScope::enter();
    note_store();
    let outer_counts = outer.snapshot();
    let unwind = std::panic::catch_unwind(|| {
        let _scope = RootWinCounterScope::enter();
        note_store();
        panic!("test-only compiler unwind");
    });
    assert!(unwind.is_err());
    assert_eq!(outer.snapshot(), outer_counts);
    assert_eq!(last_completed_counts(), last_completed);
}

#[test]
fn scope_drop_is_independent_of_a_live_store_history_borrow() {
    let outer = RootWinCounterScope::enter();
    note_store();
    let outer_counts = outer.snapshot();
    let inner = RootWinCounterScope::enter();
    note_store();
    note_store();
    ROOTWIN_CARRY.with(|carry| {
        let _stored = carry.stored.borrow_mut();
        drop(inner);
        assert_eq!(carry.counts.get(), outer_counts);
    });
}
