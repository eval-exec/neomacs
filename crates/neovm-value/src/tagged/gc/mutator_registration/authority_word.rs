//! Shared proof-2/proof-3 authority protocol, extracted from table_authority.
//! Table callers retain spin/yield admission; symbol callers try once and
//! return Busy before any lease exists. No serial Sim path uses this module.
use std::sync::atomic::{AtomicU64, Ordering};

/// Bit 63: a writer holds one admitted object across a Rust-side edit.
pub(crate) const WRITER: u64 = 1 << 63;
/// Low 47 bits: in-flight sealed readers walking storage.
pub(crate) const COUNT_MASK: u64 = (1 << 47) - 1;

/// Finite admission for an owner that must return Busy outside every loan.
/// Refuse reserved bits and saturation rather than wrapping the count.
#[inline]
pub(crate) fn try_reader_enter(word: &AtomicU64) -> bool {
    let current = word.load(Ordering::Acquire);
    if current & !COUNT_MASK != 0 || current == COUNT_MASK {
        return false;
    }
    word.compare_exchange(current, current + 1, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
}

/// The same drained-word writer CAS, without a waiting policy.
#[inline]
pub(crate) fn try_writer_enter(word: &AtomicU64) -> bool {
    word.compare_exchange(0, WRITER, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
}

/// Spin shape before each yield (Rev B1-Q2): 64 `spin_loop` hints, then
/// `yield_now`, repeated. Never park or futex — a writer holds WRITER only
/// across a Rust-side edit with no Lisp, no allocation callback and no
/// safepoint inside, so the hold is short by construction.
const SPIN_HINTS: usize = 64;
/// Debug-only starvation tripwire: a writer or reader that cannot make
/// progress after this many yield rounds means the protocol is being used
/// across a safepoint or Lisp call, which the design forbids.
#[cfg(debug_assertions)]
const MAX_YIELD_ROUNDS: u64 = 1 << 22;

/// A fully quiescent word: no writer, no readers.
#[inline]
pub(crate) fn quiescent(word: &AtomicU64) -> bool {
    word.load(Ordering::Acquire) == 0
}

/// Sealed reader enter: CAS-increment the count, refusing while WRITER is
/// set (retry with the Rev B1-Q2 spin/yield shape). Parallel readers never
/// block each other — only this one word is shared.
#[inline]
pub(crate) fn reader_enter(word: &AtomicU64) {
    let mut rounds = 0u64;
    loop {
        let current = word.load(Ordering::Acquire);
        if current & WRITER == 0 {
            debug_assert_eq!(current & !COUNT_MASK, 0, "reserved authority bits set");
            if try_reader_enter(word) {
                return;
            }
        }
        for _ in 0..SPIN_HINTS {
            std::hint::spin_loop();
        }
        std::thread::yield_now();
        rounds += 1;
        #[cfg(debug_assertions)]
        debug_assert!(rounds < MAX_YIELD_ROUNDS, "reader_enter starved");
    }
}

/// Sealed reader exit: plain Release decrement of the count.
#[inline]
pub(crate) fn reader_exit(word: &AtomicU64) {
    let was = word.fetch_sub(1, Ordering::Release);
    debug_assert!(was & COUNT_MASK > 0, "reader_exit without a reader count");
    debug_assert!(was & WRITER == 0, "reader count held across a writer");
}

/// Sealed writer enter: one CAS from a fully quiescent word. Succeeding
/// means every in-flight reader has drained (the B1 `writer_drains_readers`
/// case); new readers refuse while WRITER is set.
#[inline]
pub(crate) fn writer_enter(word: &AtomicU64) {
    let mut rounds = 0u64;
    loop {
        if try_writer_enter(word) {
            debug_assert_eq!(word.load(Ordering::Acquire), WRITER);
            return;
        }
        for _ in 0..SPIN_HINTS {
            std::hint::spin_loop();
        }
        std::thread::yield_now();
        rounds += 1;
        #[cfg(debug_assertions)]
        debug_assert!(rounds < MAX_YIELD_ROUNDS, "writer_enter drain starved");
    }
}

/// Sealed writer exit: plain Release store of the quiescent word.
#[inline]
pub(crate) fn writer_exit(word: &AtomicU64) {
    word.store(0, Ordering::Release);
}
