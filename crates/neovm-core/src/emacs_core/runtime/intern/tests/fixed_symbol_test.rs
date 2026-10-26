use super::*;
use crate::emacs_core::intern::{
    intern, intern_calls, reset_intern_calls, restore_runtime_interner, unintern_canonical_id,
};
use crate::heap_types::LispString;
use std::sync::{Arc, mpsc};
use std::time::Duration;

static_assertions::assert_impl_all!(EpochCachedSymbol: Send, Sync);

#[test]
fn warm_fixed_symbol_neither_interns_nor_takes_the_registry_lock() {
    let fixed = Arc::new(EpochCachedSymbol::new("ttykey-fixed-warm-lock-proof"));
    let expected = fixed.get();
    let registry = global_symbol_registry().write();
    // An unrelated invalidation may have happened between warming and
    // taking the guard. Seed from the now-stable guarded registry epoch.
    *fixed.resolved.write() = Some(ResolvedSymbol {
        epoch: symbol_registry_epoch_value(),
        symbol: expected,
    });
    let (send, receive) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        reset_intern_calls();
        let answers: Vec<_> = (0..64).map(|_| fixed.get()).collect();
        send.send((answers, intern_calls()))
            .expect("live test receiver");
    });
    let result = receive.recv_timeout(Duration::from_secs(2));
    // Release and join even if the regression blocks on the registry.
    drop(registry);
    worker.join().expect("fixed-symbol worker completed");
    let (answers, calls) = result.expect("warm lookup must finish under registry write guard");
    assert_eq!(answers, vec![expected; 64]);
    assert_eq!(calls, 0);
}

#[test]
fn fixed_symbol_revalidation_on_unrelated_epoch_advance_does_not_intern() {
    let fixed = EpochCachedSymbol::new("ttykey-k13-reval-probe");
    let id = fixed.get();
    let unrelated = intern("ttykey-k13-reval-unrelated-removal");
    assert!(unintern_canonical_id(unrelated));
    reset_intern_calls();
    assert_eq!(fixed.get(), id);
    assert_eq!(intern_calls(), 0, "revalidated id must not re-intern");
}

#[test]
fn fixed_symbol_revalidates_after_unintern_and_reintern() {
    let fixed = EpochCachedSymbol::new("ttykey-fixed-unintern-reintern");
    let old = fixed.get();
    assert_eq!(old, intern(fixed.name));
    assert!(unintern_canonical_id(old));
    let new = intern(fixed.name);
    assert_ne!(new, old);
    assert_eq!(fixed.get(), new);
}

#[test]
fn fixed_symbol_revalidates_after_dump_restore() {
    let fixed = EpochCachedSymbol::new("ttykey-fixed-restore-canonical");
    let old = fixed.get();
    assert!(unintern_canonical_id(old));
    let names = [LispString::from_utf8(fixed.name)];
    let restored =
        restore_runtime_interner(&names, &[0], Some(&[true])).expect("valid one-symbol restore");
    let replacement = restored.symbols[0];
    assert_ne!(replacement, old);
    assert_eq!(fixed.get(), replacement);
    // A second merge preserves canonical identity but still invalidates
    // the fixed-symbol epoch snapshot at the public restore boundary.
    let previous = symbol_registry_epoch_value();
    restore_runtime_interner(&names, &[0], Some(&[true])).expect("valid repeated restore");
    assert!(symbol_registry_epoch_value() > previous);
    reset_intern_calls();
    assert_eq!(fixed.get(), replacement);
    assert_eq!(
        intern_calls(),
        0,
        "preserved canonical mapping must revalidate without interning"
    );
}

#[test]
fn failed_restore_also_publishes_fixed_symbol_invalidation() {
    let fixed = EpochCachedSymbol::new("ttykey-fixed-failed-restore");
    let expected = fixed.get();
    let previous = symbol_registry_epoch_value();
    assert!(restore_runtime_interner(&[], &[0], Some(&[true])).is_err());
    assert!(symbol_registry_epoch_value() > previous);
    assert_eq!(fixed.get(), expected);
}

#[test]
fn completed_cross_thread_unintern_invalidates_all_fixed_symbol_readers() {
    let fixed = Arc::new(EpochCachedSymbol::new(
        "ttykey-fixed-cross-thread-invalidation",
    ));
    let old = fixed.get();
    let replacement = std::thread::spawn(move || {
        assert!(unintern_canonical_id(old));
        intern("ttykey-fixed-cross-thread-invalidation")
    })
    .join()
    .expect("invalidation worker completed");
    let workers: Vec<_> = (0..8)
        .map(|_| {
            let fixed = Arc::clone(&fixed);
            std::thread::spawn(move || (0..64).all(|_| fixed.get() == replacement))
        })
        .collect();
    for worker in workers {
        assert!(worker.join().expect("reader worker completed"));
    }
    assert_ne!(replacement, old);
}

#[test]
fn fixed_symbol_value_preserves_tags_and_symbol_accounting() {
    use crate::emacs_core::Value;
    let ordinary = EpochCachedSymbol::new("editpath-fixed-value-accounting");
    let keyword = EpochCachedSymbol::new(":editpath-fixed-value-accounting");
    let nil = EpochCachedSymbol::new("nil");
    let truth = EpochCachedSymbol::new("t");
    let expected = Value::symbol(ordinary.name);
    let expected_keyword = Value::symbol(keyword.name);
    let before = Value::memory_use_counts_snapshot();
    assert_eq!(Value::symbol(&ordinary), expected);
    assert_eq!(Value::symbol(&ordinary), expected);
    assert_eq!(Value::symbol(&keyword), expected_keyword);
    assert_eq!(Value::symbol(&nil), Value::NIL);
    assert_eq!(Value::symbol(&truth), Value::T);
    let after = Value::memory_use_counts_snapshot();
    assert_eq!(after[3] - before[3], 3);
}

#[test]
fn fixed_symbol_construction_is_lazy_and_each_cell_is_independent() {
    let first = EpochCachedSymbol::new("editpath-fixed-lazy-first");
    let second = EpochCachedSymbol::new("editpath-fixed-lazy-second");
    assert!(first.resolved.read().is_none());
    assert!(second.resolved.read().is_none());
    assert!(global_symbol_registry().read().lookup(first.name).is_none());
    let resolved = first.get();
    assert_eq!(
        global_symbol_registry().read().lookup(first.name),
        Some(resolved)
    );
    assert!(second.resolved.read().is_none());
    assert!(
        global_symbol_registry()
            .read()
            .lookup(second.name)
            .is_none()
    );
}
