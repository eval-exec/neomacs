use crate::emacs_core::builtins::{HashTableTestRegistryHandle, HashTestGcInhibitAccounting};
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;

/// An activation owns the mutability guard for its table. Synchronous Lisp
/// execution is serialized for a particular table by its owning mutator;
/// independent mutators own independent Contexts and tables. This guard stores
/// no shared process or thread-local Lisp state and lends no table reference
/// across a callback. The surrounding GC inhibition keeps the table alive.
struct UserTestMutabilityGuard {
    table: Value,
    registry: HashTableTestRegistryHandle,
    previous_accounting: Option<HashTestGcInhibitAccounting>,
}

impl UserTestMutabilityGuard {
    fn enter(eval: &Context, table: Value) -> Self {
        let registry = eval.hash_table_test_registry.clone();
        let accounting = HashTestGcInhibitAccounting {
            bytes_at_start: eval.tagged_heap.bytes_since_gc_exact(),
            threshold_at_start: eval.tagged_heap.gc_threshold(),
        };
        let previous_accounting = registry
            .borrow_mut()
            .gc_inhibit_accounting
            .replace(accounting);
        let guard = Self {
            table,
            registry,
            previous_accounting,
        };
        let _ = table.with_hash_table_mut(|ht| ht.mutable = false);
        guard
    }
}

impl Drop for UserTestMutabilityGuard {
    fn drop(&mut self) {
        let _ = self.table.with_hash_table_mut(|ht| ht.mutable = true);
        self.registry.borrow_mut().gc_inhibit_accounting = self.previous_accounting;
    }
}

/// GNU fns.c `hash_table_user_defined_call`: only the outermost callback on
/// a table installs guards. Nested reads directly call their functions while
/// the original guard continues to inhibit collection. Both guards restore on
/// Lisp non-local exit and Rust unwind, with GC inhibited until mutability is
/// restored. The callback owns the only Context borrow for this scope.
#[cold]
#[inline(never)]
pub(crate) fn with_user_test_guard<T>(
    eval: &mut Context,
    table: Value,
    callback: impl FnOnce(&mut Context) -> T,
) -> T {
    if table.as_hash_table().is_some_and(|ht| !ht.mutable) {
        return callback(eval);
    }
    eval.with_gc_inhibited(|eval| {
        let _mutability = UserTestMutabilityGuard::enter(eval, table);
        callback(eval)
    })
}

/// GNU raises its allocation countdown to HI_THRESHOLD while inhibited.
/// Thus garbage-collect-maybe sees a signed, usually negative `since_gc`.
/// Threshold changes during the callback adjust GNU's countdown by the same
/// amount; they do not change this difference, so keep the starting threshold.
#[cold]
#[inline(never)]
pub(crate) fn inhibited_user_test_since_gc(eval: &Context, bytes_since_gc: usize) -> Option<i128> {
    let accounting = eval
        .hash_table_test_registry
        .borrow()
        .gc_inhibit_accounting?;
    let hi_threshold = (i64::MAX as usize) / 2;
    let allocated = bytes_since_gc.saturating_sub(accounting.bytes_at_start);
    Some(
        accounting.threshold_at_start.min(hi_threshold) as i128 - hi_threshold as i128
            + allocated as i128,
    )
}

/// GNU accepts bignum GC thresholds fitting intmax_t. Keep that checked
/// conversion outside the ordinary fixnum runtime-settings cache path.
#[cold]
#[inline(never)]
pub(crate) fn gc_threshold_integer_fallback(value: Value) -> Option<i64> {
    if super::hash_test_parity_enabled() {
        value
            .as_bignum()
            .and_then(|number| i64::try_from(number).ok())
    } else {
        None
    }
}

#[cfg(test)]
#[path = "tests/user_test_guard.rs"]
mod tests;
