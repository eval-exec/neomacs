use super::*;
use crate::emacs_core::value::HashTableTest;

fn fixture() -> (Context, Value) {
    crate::test_utils::init_test_tracing();
    let mut context = Context::new();
    let table = context
        .eval_str(
            r#"(progn
                  (setq neovm-guard-table (make-hash-table :test 'equal))
                  (puthash "stored" 7 neovm-guard-table)
                  neovm-guard-table)"#,
        )
        .expect("create guarded table");
    (context, table)
}

#[test]
fn hash_user_guard_preserves_key_cache_epoch() {
    let (mut context, table) = fixture();
    let epoch = table.as_hash_table().unwrap().data.switch_epoch;
    with_user_test_guard(&mut context, table, |context| {
        assert_eq!(table.as_hash_table().unwrap().data.switch_epoch, epoch);
        assert_eq!(
            context
                .eval_str(r#"(gethash "stored" neovm-guard-table)"#)
                .expect("guarded read")
                .as_fixnum(),
            Some(7)
        );
    });
    assert_eq!(table.as_hash_table().unwrap().data.switch_epoch, epoch);
}

#[test]
fn hash_user_guard_restores_success_and_nested_depth() {
    let (mut context, table) = fixture();
    let other = Value::hash_table(HashTableTest::Eq);
    with_user_test_guard(&mut context, table, |context| {
        assert!(!table.as_hash_table().unwrap().mutable);
        assert_eq!(context.gc_inhibit_depth, 1);
        with_user_test_guard(context, table, |context| {
            assert_eq!(context.gc_inhibit_depth, 1);
            with_user_test_guard(context, other, |context| {
                assert_eq!(context.gc_inhibit_depth, 2);
                assert!(!other.as_hash_table().unwrap().mutable);
            });
            assert!(other.as_hash_table().unwrap().mutable);
            assert_eq!(context.gc_inhibit_depth, 1);
        });
        assert!(!table.as_hash_table().unwrap().mutable);
    });
    assert!(table.as_hash_table().unwrap().mutable);
    assert_eq!(context.gc_inhibit_depth, 0);
}

#[test]
fn hash_user_guard_rejects_mutations_before_lookup() {
    let (mut context, table) = fixture();
    for operation in [
        r#"(puthash "stored" 7 neovm-guard-table)"#,
        r#"(puthash "new" 9 neovm-guard-table)"#,
        r#"(remhash "stored" neovm-guard-table)"#,
        r#"(remhash "missing" neovm-guard-table)"#,
        "(clrhash neovm-guard-table)",
    ] {
        let result =
            with_user_test_guard(&mut context, table, |context| context.eval_str(operation));
        let error = result.expect_err("same-table mutation must signal");
        let crate::emacs_core::error::EvalError::Signal { symbol, data, .. } = error else {
            panic!("expected mutation signal");
        };
        assert_eq!(symbol, crate::emacs_core::intern::intern("error"));
        assert_eq!(
            data[0].as_str_owned().as_deref(),
            Some("hash table test modifies table")
        );
        assert_eq!(data[1], table);
        assert!(table.as_hash_table().unwrap().mutable);
        assert_eq!(table.as_hash_table().unwrap().data.len(), 1);
        assert_eq!(context.gc_inhibit_depth, 0);
    }
    context
        .eval_str("(clrhash neovm-guard-table)")
        .expect("clear after guard");
    let result = with_user_test_guard(&mut context, table, |context| {
        context.eval_str("(clrhash neovm-guard-table)")
    });
    assert!(result.is_err(), "even an empty clear must signal");
    assert_eq!(table.as_hash_table().unwrap().data.len(), 0);
}

#[test]
fn hash_user_guard_allows_reads_other_writes_and_caught_error() {
    let (mut context, table) = fixture();
    let result = with_user_test_guard(&mut context, table, |context| {
        context.eval_str(
            r#"(let ((other (make-hash-table)))
                  (puthash 'other 99 other)
                  (condition-case nil
                      (puthash "stored" 8 neovm-guard-table)
                    (error nil))
                  (+ (gethash "stored" neovm-guard-table)
                     (gethash 'other other)))"#,
        )
    })
    .expect("read and independent write must succeed");
    assert_eq!(result.as_fixnum(), Some(106));
    assert!(table.as_hash_table().unwrap().mutable);
    assert_eq!(context.gc_inhibit_depth, 0);
}

#[test]
fn hash_user_guard_restores_after_signal_and_throw() {
    let (mut context, table) = fixture();
    for exit in ["(signal 'error '(guard-signal))", "(throw 'guard-exit 23)"] {
        let result = with_user_test_guard(&mut context, table, |context| context.eval_str(exit));
        assert!(result.is_err(), "non-local exit must escape the callback");
        assert!(table.as_hash_table().unwrap().mutable);
        assert_eq!(context.gc_inhibit_depth, 0);
        context
            .eval_str(r#"(puthash "stored" 11 neovm-guard-table)"#)
            .expect("table is writable after exit");
    }
}

#[test]
fn hash_user_guard_restores_after_rust_unwind() {
    let (mut context, table) = fixture();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        with_user_test_guard(&mut context, table, |context| {
            assert_eq!(context.gc_inhibit_depth, 1);
            assert!(!table.as_hash_table().unwrap().mutable);
            panic!("user-test callback unwind");
        });
    }));
    assert!(result.is_err());
    assert!(table.as_hash_table().unwrap().mutable);
    assert_eq!(context.gc_inhibit_depth, 0);
    assert!(context.user_test_gc_accounting_pointer().is_none());
    context
        .eval_str(r#"(puthash "stored" 13 neovm-guard-table)"#)
        .expect("table is writable after Rust unwind");
}

#[test]
fn hash_user_guard_accounts_gc_maybe_countdown_and_threshold_changes() {
    let (mut context, table) = fixture();
    let hi_threshold = (i64::MAX as usize) / 2;
    assert!(context.tagged_heap.gc_threshold() < hi_threshold);
    with_user_test_guard(&mut context, table, |context| {
        let bytes = context.tagged_heap.bytes_since_gc_exact();
        let initial = inhibited_user_test_since_gc(context, bytes).unwrap();
        assert!(
            initial < 0,
            "ordinary inhibited countdown reports no allocation"
        );
        context.tagged_heap.set_gc_threshold(hi_threshold);
        assert_eq!(inhibited_user_test_since_gc(context, bytes), Some(initial));
        context
            .eval_str("(make-list 32 nil)")
            .expect("allocate during inhibition");
        let current = context.tagged_heap.bytes_since_gc_exact();
        let after = inhibited_user_test_since_gc(context, current).unwrap();
        assert!(after > initial);
        assert!(
            after < 0,
            "raising threshold inside a callback preserves since_gc"
        );
    });
    assert!(context.user_test_gc_accounting_pointer().is_none());
    with_user_test_guard(&mut context, table, |context| {
        let bytes = context.tagged_heap.bytes_since_gc_exact();
        assert_eq!(inhibited_user_test_since_gc(context, bytes), Some(0));
        context
            .eval_str("(make-list 32 nil)")
            .expect("allocate at capped threshold");
        let current = context.tagged_heap.bytes_since_gc_exact();
        assert!(inhibited_user_test_since_gc(context, current).unwrap() > 0);
    });
}

#[test]
fn hash_user_guard_restores_outer_gc_maybe_accounting_after_other_table() {
    let (mut context, table) = fixture();
    let other = Value::hash_table(HashTableTest::Eq);
    with_user_test_guard(&mut context, table, |context| {
        let bytes = context.tagged_heap.bytes_since_gc_exact();
        let outer = inhibited_user_test_since_gc(context, bytes).unwrap();
        assert!(outer < 0);
        context
            .tagged_heap
            .set_gc_threshold((i64::MAX as usize) / 2);
        with_user_test_guard(context, other, |context| {
            assert_eq!(inhibited_user_test_since_gc(context, bytes), Some(0));
        });
        assert_eq!(inhibited_user_test_since_gc(context, bytes), Some(outer));
        with_user_test_guard(context, table, |context| {
            assert_eq!(inhibited_user_test_since_gc(context, bytes), Some(outer));
        });
    });
    assert!(context.user_test_gc_accounting_pointer().is_none());
}

#[test]
fn hash_user_guard_preserves_live_settings_fallback_after_nested_exit() {
    let (mut context, table) = fixture();
    let other = Value::hash_table(HashTableTest::Eq);
    let threshold = crate::emacs_core::intern::intern("gc-cons-threshold");
    let result = with_user_test_guard(&mut context, table, |context| {
        let bytes = context.tagged_heap.bytes_since_gc_exact();
        let outer = inhibited_user_test_since_gc(context, bytes).unwrap();
        context
            .eval_str("(make-local-variable 'gc-cons-percentage)")
            .expect("mark settings volatile while accounting is active");
        assert_eq!(inhibited_user_test_since_gc(context, bytes), Some(outer));
        // A redirected GC variable's base cell can change without its normal
        // canonical publisher. The sticky fallback must read it live.
        context
            .obarray
            .set_symbol_value_id(threshold, Value::fixnum(1_048_576));
        with_user_test_guard(context, other, |context| {
            assert_eq!(
                context
                    .capture_user_test_gc_accounting()
                    .entry_threshold_bytes,
                1_048_576
            );
            let before = context.eval_str("gcs-done").expect("read collection count");
            assert!(
                context
                    .eval_str("(garbage-collect)")
                    .expect("inhibited GC")
                    .is_nil()
            );
            assert_eq!(
                context.eval_str("gcs-done").expect("read collection count"),
                before
            );
        });
        assert_eq!(inhibited_user_test_since_gc(context, bytes), Some(outer));
        context.eval_str("(signal 'error '(sticky-accounting))")
    });
    assert!(result.is_err());
    assert!(table.as_hash_table().unwrap().mutable);
    assert!(other.as_hash_table().unwrap().mutable);
    assert_eq!(context.gc_inhibit_depth, 0);
    assert!(context.user_test_gc_accounting_pointer().is_none());
    context
        .obarray
        .set_symbol_value_id(threshold, Value::fixnum(2_097_152));
    assert_eq!(
        context
            .capture_user_test_gc_accounting()
            .entry_threshold_bytes,
        2_097_152
    );
}

#[test]
fn hash_user_guard_keeps_gc_binding_projection_current() {
    let (mut context, table) = fixture();
    let initial = context.gc_threshold();
    let scope = context.specpdl.len();
    context
        .try_specbind(
            crate::emacs_core::intern::intern("gc-cons-threshold"),
            Value::fixnum(Value::MOST_POSITIVE_FIXNUM),
        )
        .expect("bind allocation threshold");
    assert!(
        context.gc_threshold() > initial,
        "a dynamic GC setting must be published before a callback reads its cached entry operands"
    );
    with_user_test_guard(&mut context, table, |context| {
        let entry_bytes = context.tagged_heap.bytes_since_gc_exact();
        context
            .eval_str("(setq gc-cons-threshold 1048576 gc-cons-percentage 0.0)")
            .expect("change settings before the first GC-maybe query");
        let bytes = context.tagged_heap.bytes_since_gc_exact();
        let hi_threshold = (i64::MAX as usize) / 2;
        assert_eq!(
            inhibited_user_test_since_gc(context, bytes),
            Some(
                Value::MOST_POSITIVE_FIXNUM as i128 - hi_threshold as i128
                    + bytes.saturating_sub(entry_bytes) as i128
            )
        );
    });
    context
        .unbind_to_with_result(scope, Ok(Value::NIL))
        .expect("restore allocation threshold");
    assert!(context.gc_threshold() < Value::MOST_POSITIVE_FIXNUM as usize);
}

#[test]
fn hash_user_guard_refreshes_dynamic_let_gc_threshold() {
    let (mut context, table) = fixture();
    let bound_threshold = Value::MOST_POSITIVE_FIXNUM as usize;
    assert!(context.gc_threshold() < bound_threshold);
    let scope = context.specpdl.len();
    // The same specbind operations used by a Lisp dynamic `let`: a binding
    // updates the forwarded variable while the collector's projection can
    // remain stale. The callback must preserve the live setting in its entry
    // operands even when the first GC-maybe resolves them lazily.
    context
        .try_specbind(
            crate::emacs_core::intern::intern("gc-cons-threshold"),
            Value::fixnum(Value::MOST_POSITIVE_FIXNUM),
        )
        .expect("bind allocation threshold");
    context
        .try_specbind(
            crate::emacs_core::intern::intern("gc-cons-percentage"),
            Value::make_float(0.0),
        )
        .expect("bind allocation percentage");
    with_user_test_guard(&mut context, table, |context| {
        let bytes = context.tagged_heap.bytes_since_gc_exact();
        let hi_threshold = (i64::MAX as usize) / 2;
        assert_eq!(
            inhibited_user_test_since_gc(context, bytes),
            Some(bound_threshold as i128 - hi_threshold as i128)
        );
    });
    context
        .unbind_to_with_result(scope, Ok(Value::NIL))
        .expect("restore dynamic bindings");
    with_user_test_guard(&mut context, table, |context| {
        let bytes = context.tagged_heap.bytes_since_gc_exact();
        let hi_threshold = (i64::MAX as usize) / 2;
        assert!(
            inhibited_user_test_since_gc(context, bytes).unwrap()
                < bound_threshold as i128 - hi_threshold as i128
        );
    });
}
