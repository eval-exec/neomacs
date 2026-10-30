use super::HashTestGcInhibitAccounting;
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;
use crate::tagged::header::HashTableObj;
use std::cell::UnsafeCell;
use std::ptr::NonNull;
use std::sync::atomic::{AtomicU8, Ordering};

/// An activation owns the mutability guard for its table. Synchronous Lisp
/// execution is serialized for a particular table by its owning mutator;
/// independent mutators own independent Contexts and tables. This guard stores
/// no shared process or thread-local Lisp state and lends no table reference
/// across a callback. The raw pointer names only the nesting-count metadata
/// byte; GC inhibition keeps its owner alive and nonmoving. Changing this byte
/// adds no Lisp edge and does not invalidate the table's key caches.
struct UserTestMutabilityGuard<'a> {
    context: &'a mut Context,
    depth: NonNull<AtomicU8>,
    previous_accounting: Option<NonNull<HashTestGcInhibitAccounting>>,
}

impl<'a> UserTestMutabilityGuard<'a> {
    #[inline]
    fn enter(
        context: &'a mut Context,
        depth: NonNull<AtomicU8>,
        accounting: NonNull<HashTestGcInhibitAccounting>,
    ) -> Self {
        context.gc_inhibit_depth += 1;
        let previous_accounting = context.replace_user_test_gc_accounting(Some(accounting));
        // The count itself was already taken by `with_user_test_guard`: a
        // direct atomic RMW on the shared byte, NOT routed through
        // `with_hash_table_mut` — it flips no Lisp-visible contents, so
        // `switch_epoch` is untouched.
        Self {
            context,
            depth,
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
        // the callback ran; the same live metadata byte counts back down.
        // Unwind-safe by construction: the fetch_sub is the Drop, the
        // analogue of GNU's specpdl-recorded restore_mutability — and unlike
        // GNU's unconditional `= true`, a nested activation counting back
        // down cannot re-open a table another owner still tests (Rev R2).
        unsafe { (*self.depth.as_ptr()).fetch_sub(1, Ordering::Release) };
        self.context
            .replace_user_test_gc_accounting(self.previous_accounting);
        self.context.gc_inhibit_depth -= 1;
    }
}

/// The nested activation's count hold: the outermost guard already inhibits
/// collection and owns the published accounting, so all this adds is the
/// RAII fetch_sub that survives a Lisp non-local exit or a Rust unwind.
struct NestedTestDepthHold {
    depth: NonNull<AtomicU8>,
}

impl Drop for NestedTestDepthHold {
    #[inline]
    fn drop(&mut self) {
        // SAFETY: the byte was counted up by this activation's caller; the
        // Drop is the unwind-safe restore.
        unsafe { (*self.depth.as_ptr()).fetch_sub(1, Ordering::Release) };
    }
}

/// GNU fns.c `hash_table_user_defined_call` under the nesting count (Rev R2).
/// EVERY activation on the table counts — outermost and nested — so the
/// depth reaches 2 when two owners test the same table, and any owner's
/// mutation signals while it is nonzero. Only the outermost activation
/// (previous depth 0) installs the GC-inhibit and publishes the pinned
/// accounting cell: a nested activation must keep the ORIGINAL accounting
/// live, or `inhibited_user_test_since_gc` would re-base mid-callback. Both
/// paths restore on Lisp non-local exit and Rust unwind, with GC inhibited
/// until the count is restored. The callback owns the only Context borrow
/// for its scope.
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
    let depth =
        unsafe { NonNull::new_unchecked(std::ptr::addr_of_mut!((*table_object).table.test_depth)) };
    // Every activation counts, atomically (Rev R2): two owners in user tests
    // on one shared table nest instead of racing one bool byte.
    let previous = unsafe { (*depth.as_ptr()).fetch_add(1, Ordering::AcqRel) };
    debug_assert!(
        previous < u8::MAX,
        "user test nesting deeper than the count byte"
    );
    if previous > 0 {
        // Nested: the original guard still inhibits collection and owns the
        // published accounting; this hold only counts back down, on every
        // exit path.
        let hold = NestedTestDepthHold { depth };
        let result = callback(eval);
        drop(hold);
        return result;
    }
    // Publish only after the UnsafeCell has reached its final pinned stack
    // slot. It is separate from the guard, so borrowing the guard's Context
    // does not retag the accounting. The guard drops and unpublishes first.
    let accounting = std::pin::pin!(UnsafeCell::new(eval.capture_user_test_gc_accounting()));
    let accounting_pointer = NonNull::new(accounting.as_ref().get_ref().get())
        .expect("pinned accounting has a non-null address");
    let mut guard = UserTestMutabilityGuard::enter(eval, depth, accounting_pointer);
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
