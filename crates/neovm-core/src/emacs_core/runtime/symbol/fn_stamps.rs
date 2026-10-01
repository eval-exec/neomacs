//! Per-symbol function-binding stamps (design `p1-3-per-symbol-versions`
//! §4.1-4.3, A1): "did THIS symbol's function binding change since clock
//! value `E`?" instead of the obarray-wide "did ANY function binding change"
//! that [`Obarray::function_epoch`](super::Obarray::function_epoch) answers.
//! GNU has no counterpart: it reads `u.s.function` at every call
//! (`funcall_general`, src/eval.c), so it has nothing to invalidate.
//!
//! The global clock stays: `function_epoch` still moves once per change, and
//! every cache still keys its one-compare hit test on it. Two things are
//! added, both stored in the per-chunk side box of
//! [`SymbolChunks`](super::SymbolChunks) (next to the chunk's GC seqlock, so
//! the `Obarray` -- embedded in `Context`, whose field offsets the hot call
//! paths bake -- keeps its layout):
//!
//! - `stamp[sym]`: the clock value the latest function-binding change of
//!   `sym` produced (0 when it never changed);
//! - `floor`: the clock value of the latest change of EVERY symbol's
//!   resolution (the compiler-overrides toggle, a subr-table rewrite). Each
//!   chunk carries a copy; [`ChunkSide::new`] copies it into a new chunk.
//!
//! **Validity rule.** An entry that observed `sym`'s binding with the clock
//! at `E` is still current iff `E != u64::MAX && E >= floor && stamp[sym] <=
//! E` (design I1-I4; `u64::MAX` is every cache's EMPTY / DISARMED sentinel).
//! The answer is exact: every change moves the clock once, so stamps are
//! unique, and "unchanged since `E`" means the cell has held the same object
//! since `E`, which the cell itself keeps alive.
//!
//! **Threading.** One writer at a time: today `&mut Obarray`; with several
//! mutators, the obarray's writer lock (the one that also grows the symbol
//! chunks). The writer publishes in the order cell store -> stamp or floor
//! (`Release`) -> clock. A reader loads the clock FIRST (I2), then the floor
//! and the stamp (`Acquire`), and records the clock value it loaded -- never
//! a re-read -- as its entry's new epoch. Then a reader that saw the clock at
//! or past a change also sees that change's stamp, and a reader that loaded
//! the clock before the change records an epoch below the stamp, so its
//! next validation fails: a resync racing a redefinition on another thread
//! can only make an entry invalid early, never keep a stale one. (Today the
//! clock is a plain `u64` that only `&mut Obarray` writes, so program order
//! is this order; an atomic clock with several mutators stores it with
//! `Release` and loads it with `Acquire`.) A chunk's stamp array is
//! allocated zeroed and published once with `Release`; it never moves and
//! is freed only with its side box, when the obarray drops.

use std::sync::atomic::{AtomicPtr, AtomicU8, AtomicU32, AtomicU64, Ordering};

use super::OBARRAY_CHUNK;

/// One chunk's stamps, index-aligned with its symbol slots.
type StampChunk = [AtomicU64; OBARRAY_CHUNK];

/// The side data of one symbol chunk: its GC seqlock and its function
/// stamps. Boxed by [`SymbolChunks`](super::SymbolChunks), so its address is
/// stable while the spine grows (the concurrent GC scan holds `&seq`).
#[repr(C)]
pub(super) struct ChunkSide {
    /// The chunk's seqlock (see `SymbolChunks::sides`). FIRST, so its
    /// address is the box's: the value-cell write paths that bump it compile
    /// exactly as they did when the box held only this counter.
    pub(super) seq: AtomicU32,
    /// Entries validated at a clock value below this are void for every
    /// symbol of the chunk (the module's `floor`).
    fn_floor: AtomicU64,
    /// The chunk's [`StampChunk`], null until a symbol of the chunk first
    /// changes its function binding.
    fn_stamps: AtomicPtr<StampChunk>,
}

impl ChunkSide {
    /// A new chunk's side: seqlock even, no stamps, the obarray's current
    /// `floor`.
    pub(super) fn new(floor: u64) -> Self {
        Self {
            seq: AtomicU32::new(0),
            fn_floor: AtomicU64::new(floor),
            fn_stamps: AtomicPtr::new(core::ptr::null_mut()),
        }
    }

    /// A deep copy for a cloned obarray: the stamps and floor carry over, the
    /// seqlock resets (a clone is never concurrently marked).
    pub(super) fn clone_for_new_obarray(&self) -> Self {
        let side = Self::new(self.floor());
        let src = self.fn_stamps.load(Ordering::Acquire);
        if !src.is_null() {
            let dst = alloc_stamp_chunk();
            // SAFETY: both arrays are live `StampChunk`s; `dst` is not yet
            // published.
            unsafe {
                for (d, s) in (*dst).iter().zip((*src).iter()) {
                    d.store(s.load(Ordering::Acquire), Ordering::Relaxed);
                }
            }
            side.fn_stamps.store(dst, Ordering::Release);
        }
        side
    }

    #[inline]
    pub(super) fn floor(&self) -> u64 {
        self.fn_floor.load(Ordering::Acquire)
    }

    /// Writer only, before the clock moves to `floor`.
    #[inline]
    pub(super) fn set_floor(&self, floor: u64) {
        self.fn_floor.store(floor, Ordering::Release);
    }

    /// The stamp of the symbol at `slot` within the chunk (0 when no symbol
    /// of the chunk ever changed).
    #[inline]
    pub(super) fn stamp(&self, slot: usize) -> u64 {
        let stamps = self.fn_stamps.load(Ordering::Acquire);
        if stamps.is_null() {
            return 0;
        }
        // SAFETY: a published stamp chunk lives as long as this side box.
        unsafe { (*stamps)[slot].load(Ordering::Acquire) }
    }

    /// Writer only, before the clock moves to `epoch`: allocates the chunk's
    /// stamps on first use.
    pub(super) fn set_stamp(&self, slot: usize, epoch: u64) {
        let mut stamps = self.fn_stamps.load(Ordering::Acquire);
        if stamps.is_null() {
            stamps = alloc_stamp_chunk();
            self.fn_stamps.store(stamps, Ordering::Release);
        }
        // SAFETY: as in `stamp`.
        unsafe { (*stamps)[slot].store(epoch, Ordering::Release) };
    }

    /// Whether this chunk holds a stamp array (tests: the lazy allocation).
    #[cfg(test)]
    pub(super) fn has_stamps(&self) -> bool {
        !self.fn_stamps.load(Ordering::Acquire).is_null()
    }
}

impl Drop for ChunkSide {
    fn drop(&mut self) {
        let stamps = *self.fn_stamps.get_mut();
        if !stamps.is_null() {
            // SAFETY: allocated by `alloc_stamp_chunk` (a `Box` of the same
            // type) and owned by this side alone.
            drop(unsafe { Box::from_raw(stamps) });
        }
    }
}

/// A zeroed [`StampChunk`] built directly on the heap (32 KiB: never on the
/// stack -- see `SymbolChunks::grow_for` for why that matters).
#[cold]
#[inline(never)]
fn alloc_stamp_chunk() -> *mut StampChunk {
    let layout = std::alloc::Layout::new::<StampChunk>();
    // SAFETY: the layout is non-zero-sized; an all-zero `AtomicU64` is 0.
    let ptr = unsafe { std::alloc::alloc_zeroed(layout) } as *mut StampChunk;
    if ptr.is_null() {
        std::alloc::handle_alloc_error(layout);
    }
    ptr
}

#[cfg(test)]
thread_local! {
    static FN_STAMPS_TEST_OVERRIDE: std::cell::Cell<Option<bool>> =
        const { std::cell::Cell::new(None) };
}

/// Force `NEOVM_FN_STAMPS` on/off on the current thread (tests only); `None`
/// returns to the environment.
#[cfg(test)]
pub(crate) fn force_fn_stamps_for_test(on: Option<bool>) {
    FN_STAMPS_TEST_OVERRIDE.with(|c| c.set(on));
}

/// Whether the per-symbol validity test may prove an entry current
/// (`NEOVM_FN_STAMPS=on`/`1`; default off, see the knob table in `jit/mod.rs`).
/// Off, [`Obarray::fn_unchanged_since`](super::Obarray::fn_unchanged_since)
/// answers `false` and every consumer takes today's re-validation or refill:
/// the single-build A/B arm. The stamps are written either way, so the
/// answer is sound whenever it is asked. Read once per process; cold paths
/// only.
#[inline]
pub(crate) fn fn_stamps_enabled() -> bool {
    #[cfg(test)]
    if let Some(on) = FN_STAMPS_TEST_OVERRIDE.with(|c| c.get()) {
        return on;
    }
    /// 0 = not read yet, 1 = off, 2 = on.
    static STATE: AtomicU8 = AtomicU8::new(0);
    #[cold]
    #[inline(never)]
    fn read_knob() -> bool {
        let on = matches!(std::env::var("NEOVM_FN_STAMPS").as_deref(), Ok("1" | "on"));
        STATE.store(if on { 2 } else { 1 }, Ordering::Relaxed);
        on
    }
    match STATE.load(Ordering::Relaxed) {
        1 => false,
        2 => true,
        _ => read_knob(),
    }
}
