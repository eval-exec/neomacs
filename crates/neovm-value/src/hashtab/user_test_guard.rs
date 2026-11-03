use super::HashTestGcInhibitAccounting;
use crate::value::Value;
use std::ptr::NonNull;

/// GNU raises its allocation countdown to HI_THRESHOLD while inhibited.
/// Thus garbage-collect-maybe sees a signed, usually negative `since_gc`.
/// Threshold changes during the callback adjust GNU's countdown by the same
/// amount; they do not change this difference, so keep the starting threshold.
///
/// # Safety
///
/// When `pointer` is `Some`, it must come from the currently active Context's
/// accounting slot. It must point to the initialized accounting entry in that
/// activation's pinned `UnsafeCell`, valid for reads and writes for this entire
/// call, including `entry_threshold`. The calling mutator must have exclusive
/// access to the activation. The callback must not end, replace or unpublish
/// that activation, or otherwise invalidate the pointer.
///
/// Production calls are restricted to `stubs::builtin_garbage_collect_maybe`,
/// which obtains the pointer from its exclusively borrowed active Context.
/// `None` carries no pointer validity obligation.
#[cold]
#[inline(never)]
pub unsafe fn inhibited_user_test_since_gc(
    pointer: Option<NonNull<HashTestGcInhibitAccounting>>,
    bytes_since_gc: usize,
    entry_threshold: impl FnOnce(&HashTestGcInhibitAccounting) -> usize,
) -> Option<i128> {
    let pointer = pointer?;
    // SAFETY: the owning Context only publishes pinned activations while
    // their guard lends this same exclusive Context borrow to the callback.
    let accounting = unsafe { *pointer.as_ptr() };
    let threshold = match accounting.threshold_at_start {
        Some(threshold) => threshold.get(),
        None => {
            let threshold = entry_threshold(&accounting);
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
#[doc(hidden)]
pub fn gc_threshold_integer_fallback(value: Value) -> Option<i64> {
    if super::hash_test_parity_enabled() {
        value
            .as_bignum()
            .and_then(|number| i64::try_from(number).ok())
    } else {
        None
    }
}
