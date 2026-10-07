use crate::emacs_core::builtins::HashTestGcInhibitAccounting;
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;
use crate::tagged::header::HashTableObj;
use std::cell::UnsafeCell;
use std::ptr::NonNull;

/// An activation owns the mutability guard for its table. Synchronous Lisp
/// execution is serialized for a particular table by its owning mutator;
/// independent mutators own independent Contexts and tables. This guard stores
/// no shared process or thread-local Lisp state and lends no table reference
/// across a callback. The raw pointer names only the mutability metadata byte;
/// GC inhibition keeps its owner alive and nonmoving. Changing this byte adds
/// no Lisp edge and does not invalidate the table's key caches.
struct UserTestMutabilityGuard<'a> {
    context: &'a mut Context,
    mutable: NonNull<bool>,
    previous_accounting: Option<NonNull<HashTestGcInhibitAccounting>>,
}

impl<'a> UserTestMutabilityGuard<'a> {
    #[inline]
    fn enter(
        context: &'a mut Context,
        mutable: NonNull<bool>,
        accounting: NonNull<HashTestGcInhibitAccounting>,
    ) -> Self {
        context.gc_inhibit_depth += 1;
        let previous_accounting = context.replace_user_test_gc_accounting(Some(accounting));
        // SAFETY: a verified, live table's scalar metadata; the owning
        // mutator has exclusive access and collection is now inhibited.
        unsafe { *mutable.as_ptr() = false };
        Self {
            context,
            mutable,
            previous_accounting,
        }
    }

    #[inline]
    fn context(&mut self) -> &mut Context {
        self.context
    }
}

impl Drop for UserTestMutabilityGuard<'_> {
    #[inline]
    fn drop(&mut self) {
        // SAFETY: no collection or table mutation was allowed while
        // the callback ran; the same live metadata byte is restored.
        unsafe { *self.mutable.as_ptr() = true };
        self.context
            .replace_user_test_gc_accounting(self.previous_accounting);
        self.context.gc_inhibit_depth -= 1;
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
    debug_assert!(table.is_hash_table(), "caller has checked the table type");
    let table_object = table
        .as_veclike_ptr()
        .expect("verified hash table")
        .cast_mut()
        .cast::<HashTableObj>();
    // SAFETY: the caller verified the table type. This takes the address
    // through a raw pointer, without creating a payload reference that could
    // remain live across Lisp. The initialized metadata does not need the
    // table's pending key hydration.
    let mutable =
        unsafe { NonNull::new_unchecked(std::ptr::addr_of_mut!((*table_object).table.mutable)) };
    if unsafe { !*mutable.as_ptr() } {
        return callback(eval);
    }
    // Publish only after the UnsafeCell has reached its final pinned stack
    // slot. It is separate from the guard, so borrowing the guard's Context
    // does not retag the accounting. The guard drops and unpublishes first.
    let accounting = std::pin::pin!(UnsafeCell::new(eval.capture_user_test_gc_accounting()));
    let accounting_pointer = NonNull::new(accounting.as_ref().get_ref().get())
        .expect("pinned accounting has a non-null address");
    let mut guard = UserTestMutabilityGuard::enter(eval, mutable, accounting_pointer);
    callback(guard.context())
}

/// GNU raises its allocation countdown to HI_THRESHOLD while inhibited.
/// Thus garbage-collect-maybe sees a signed, usually negative `since_gc`.
/// Threshold changes during the callback adjust GNU's countdown by the same
/// amount; they do not change this difference, so keep the starting threshold.
#[cold]
#[inline(never)]
pub(crate) fn inhibited_user_test_since_gc(
    eval: &mut Context,
    bytes_since_gc: usize,
) -> Option<i128> {
    let pointer = eval.user_test_gc_accounting_pointer()?;
    // SAFETY: the owning Context only publishes pinned activations while
    // their guard lends this same exclusive Context borrow to the callback.
    let accounting = unsafe { *pointer.as_ptr() };
    let threshold = match accounting.threshold_at_start {
        Some(threshold) => threshold.get(),
        None => {
            let threshold = eval.user_test_gc_entry_threshold(&accounting);
            // SAFETY: the pinned UnsafeCell allows this interior update;
            // the synchronous owning mutator is its only accessor.
            unsafe {
                (*pointer.as_ptr()).threshold_at_start = std::num::NonZeroUsize::new(threshold)
            };
            threshold
        }
    };
    let hi_threshold = (i64::MAX as usize) / 2;
    let allocated = bytes_since_gc.saturating_sub(accounting.bytes_at_start);
    Some(threshold.min(hi_threshold) as i128 - hi_threshold as i128 + allocated as i128)
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
#[path = "tests/user_test_guard_test.rs"]
mod tests;
