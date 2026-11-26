//! Positive control for value-gc B5/B13/B14: sealed `HeapPtr::new` (unsafe),
//! `JitHeapState` safe reads + unsafe region installation, and
//! `CollectorTablesSlot::enter` with a real `&AllStopped` compile cleanly.
#![allow(dead_code, unexpected_cfgs)]

use std::cell::Cell;
use std::marker::PhantomData;
use std::mem::size_of;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BarrierWindow {
    lo: usize,
    len: usize,
}
impl BarrierWindow {
    fn lo(&self) -> usize {
        self.lo
    }
    fn len(self) -> usize {
        self.len
    }
    const fn from_lo_len(lo: usize, len: usize) -> Self {
        Self { lo, len }
    }
}

struct FloatObj;
trait PagedObject {
    const SLOT_BYTES: usize;
}
impl PagedObject for FloatObj {
    const SLOT_BYTES: usize = 32;
}
struct ConsCell([usize; 2]);

#[repr(C)]
struct TaggedHeap {
    _pad: [u8; 0xe18],
    jit: jit_state::JitHeapState,
}

mod mutator_registration {
    use super::PhantomData;
    pub struct AllStopped<'era>(pub PhantomData<&'era ()>);
    #[derive(Debug, PartialEq, Eq)]
    pub enum RendezvousError {
        Busy,
    }
}

#[path = "../../../neovm-value/src/tagged/gc/collector_tables_slot.rs"]
mod collector_tables_slot;
#[path = "../../../neovm-value/src/tagged/gc/heap_ptr.rs"]
mod heap_ptr;
#[path = "../../../neovm-value/src/tagged/gc/jit_state.rs"]
mod jit_state;

fn main() {
    // B5 positive control: unsafe constructor + into_raw.
    let mut dummy = std::mem::MaybeUninit::<TaggedHeap>::uninit();
    // SAFETY: compile-only positive control; pointer is never dereferenced.
    let hp = unsafe { heap_ptr::HeapPtr::new(dummy.as_mut_ptr()) };
    let _ = hp.into_raw();

    // B13 positive control: safe reads/clears + unsafe install.
    let jit = jit_state::JitHeapState::new();
    assert_eq!(jit.cons_cur(), 0);
    assert_eq!(jit.cons_lim(), 0);
    assert_eq!(jit.float_cur(), 0);
    assert_eq!(jit.float_lim(), 0);
    // SAFETY: compile-only positive control; cleared immediately without allocation.
    unsafe {
        jit.install_cons_region(0x1000, 0x1010);
        jit.install_float_region(0x2000, 0x2020);
    }
    let _ = jit.clear_cons_region();
    let _ = jit.clear_float_region();

    // B14 positive control: closure-scoped enter with real &AllStopped + RAII clear.
    let stopped = mutator_registration::AllStopped(PhantomData);
    let mut slot = collector_tables_slot::CollectorTablesSlot::new();
    assert!(!slot.is_installed());
    collector_tables_slot::CollectorTablesSlot::with_entered_on(
        &mut slot,
        |s| s,
        &stopped,
        |s| {
            assert!(s.is_installed());
            assert_eq!(
                collector_tables_slot::CollectorTablesSlot::with_entered_on(
                    s,
                    |inner| inner,
                    &stopped,
                    |_| (),
                ),
                Err(mutator_registration::RendezvousError::Busy)
            );
        },
    )
    .expect("first enter succeeds");
    assert!(!slot.is_installed());
}
