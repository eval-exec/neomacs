//! value-gc B13 regression guard: `JitHeapState`'s allocation cursors
//! (`cons_cur`, `cons_lim`, `float_cur`, `float_lim`) and barrier window
//! fields must be private so crate/module code cannot safely overwrite bump
//! cursors with arbitrary addresses.
#![allow(dead_code, unexpected_cfgs)]

use std::cell::Cell;
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

#[path = "../../../neovm-value/src/tagged/gc/jit_state.rs"]
mod jit_state;

fn main() {
    let jit = jit_state::JitHeapState::new();
    jit.cons_cur.set(0xdead_beef);
    jit.cons_lim.set(0xdead_bff0);
    jit.float_cur.set(0xdead_beef);
    jit.float_lim.set(0xdead_bff0);
}
