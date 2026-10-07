//! The layout compiled code reads to reach a symbol's value cell without a
//! call (`OBARRAY_JIT_*`, `LISP_SYMBOL_*`): through growth that moves the
//! chunk spine, and in a clone, it must land on the same cell `get_by_id`
//! returns, and the flags and value offsets must read that cell's fields.

use super::*;

/// The cell compiled code would read for `id`, or `None` when its bounds
/// check fails.
fn cell_via_layout(ob: &Obarray, id: SymId) -> Option<*const LispSymbol> {
    let base = ob as *const Obarray as *const u8;
    // SAFETY: the offsets are `offset_of!` within `Obarray`, of `usize` fields.
    let (spine, len) = unsafe {
        (
            *(base.add(OBARRAY_JIT_SPINE_OFFSET) as *const usize),
            *(base.add(OBARRAY_JIT_LEN_OFFSET) as *const usize),
        )
    };
    let idx = id.0 as usize;
    if idx >= len {
        return None;
    }
    // SAFETY: `idx < len`, so chunk `idx >> BITS` exists in the spine.
    let chunk = unsafe { *((spine + (idx >> OBARRAY_CHUNK_BITS) * 8) as *const usize) };
    Some((chunk + (idx & (OBARRAY_CHUNK_SLOTS - 1)) * LISP_SYMBOL_SIZE) as *const LispSymbol)
}

fn check(ob: &Obarray, ids: &[SymId], what: &str) {
    for &id in ids {
        let want = ob.get_by_id(id).map(|s| s as *const LispSymbol);
        assert_eq!(cell_via_layout(ob, id), want, "{what}: {id:?}");
    }
}

#[test]
fn the_jit_layout_reaches_every_symbol_cell_through_growth_and_clone() {
    let mut ob = Obarray::new();
    let ids: Vec<SymId> = (0..12_000)
        .map(|i| intern(&format!("jit-layout-probe-{i}")))
        .collect();
    for (n, &id) in ids.iter().enumerate() {
        ob.ensure_symbol_id(id);
        if n % 1_499 == 0 {
            check(&ob, &ids[..=n], "while growing");
        }
    }
    check(&ob, &ids, "grown");
    let clone = ob.clone();
    check(&clone, &ids, "clone");

    // The flags and value offsets read the cell's own fields.
    let id = ids[7_777];
    ob.set_symbol_value_id(id, Value::fixnum(4242));
    let cell = cell_via_layout(&ob, id).expect("in range") as *const u8;
    // SAFETY: a live cell; the offsets are `offset_of!` of a `u8` flags byte
    // and a one-word value cell.
    let (flags, val) = unsafe {
        (
            *cell.add(LISP_SYMBOL_FLAGS_OFFSET),
            *(cell.add(LISP_SYMBOL_VAL_OFFSET) as *const usize),
        )
    };
    assert_eq!(flags & SYMBOL_FLAGS_REDIRECT_MASK, 0, "a plain value cell");
    assert_eq!(val, Value::fixnum(4242).bits());
}
