//! Heap writes and allocation inline in JIT code (levers P0.7 and P0.8):
//! the stores `setcar`, `setcdr` and `aset` (of a plain vector or record)
//! make, done in place instead of in `neovm_jit_setcar`/`_setcdr`/`_aset`;
//! and conses and float boxes bumped out of the heap's open allocation
//! regions instead of `neovm_jit_cons`/`neovm_jit_make_float`.
//!
//! The write barrier's whole inline decision is one owner-address window
//! (`tagged::gc::BarrierWindow`), which the heap publishes into its
//! `JitHeapState` at every change of collector state. Compiled code reads it
//! through `vmctx -> Context.tagged_heap -> jit` and stores inline only when
//! the owner lies outside it; everything else — a non-cons, an owner in the
//! window (a concurrent mark or owner tracking makes the window ALL, the
//! dump partition makes it the image span) — takes the unchanged shim, which
//! signals, records and stores exactly as before.
//!
//! Why a plain store is sound outside the window: the window is ALL for the
//! whole life of a concurrent mark (`launch_concurrent_mark` publishes it
//! before the GC thread can read anything and `join_concurrent_mark` only
//! clears it after the thread has exited), so an inline store never races a
//! collector read; the next mark's start handshake (a channel send) orders
//! it before any. A cons is never tenured, so outside the window it has
//! nothing to remember.
//!
//! Allocation: `JitHeapState` also holds each class's region cursor and
//! limit (`tagged::gc::alloc_region`), which the Rust allocator bumps too.
//! A site loads both, bumps the cursor when the region has room and writes
//! the new object's fields; an exhausted (or closed: both 0) region takes
//! the unchanged shim, which refills through the Rust allocator. The region
//! was charged to the consing counters and, for conses allocated while the
//! heap allocates black, pre-marked when it was granted, so the site
//! neither counts nor marks. The cursor is reloaded at every site, never
//! cached across a call: a shim or safe point in between may have closed
//! the region. No site reaches a safe point (the shim contract).
//!
//! A body containing an inline site dereferences its vmctx: see
//! [`inline_heap_sites`] and `CompiledLeaf::needs_vmctx`.

use super::*;
use crate::emacs_core::eval::runtime_projection::CONTEXT_TAGGED_HEAP_OFFSET;
use crate::tagged::gc::{
    FLOAT_SLOT_BYTES, HEAP_JIT_BARRIER_LEN, HEAP_JIT_BARRIER_LO, HEAP_JIT_CONS_CUR,
    HEAP_JIT_CONS_LIM, HEAP_JIT_FLOAT_CUR, HEAP_JIT_FLOAT_LIM,
};

thread_local! {
    /// Inline heap sites emitted in the function being lowered.
    static INLINE_HEAP_SITES: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Start the inline-heap-site count for a new function.
pub(crate) fn inline_heap_sites_reset() {
    INLINE_HEAP_SITES.with(|c| c.set(0));
}

/// Inline heap sites emitted since the last [`inline_heap_sites_reset`]: a
/// function with any reads the heap through its vmctx, so it must never be
/// entered with a null one.
pub(crate) fn inline_heap_sites() -> u32 {
    INLINE_HEAP_SITES.with(|c| c.get())
}

/// Inline stores emitted, process-wide, so a test can tell "the inline path
/// answered" from "the emitter declined and the shim answered".
#[cfg(debug_assertions)]
pub(crate) static INLINE_HEAP_STORES_EMITTED: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

fn note_inline_site() {
    INLINE_HEAP_SITES.with(|c| c.set(c.get() + 1));
}

/// Load the `*const TaggedHeap` off a `*mut Context`. The box is assigned
/// only by the context's constructors, so the pointer never changes.
pub(crate) fn load_heap_ptr(fb: &mut FunctionBuilder, vmctx: ClifValue) -> ClifValue {
    fb.ins().load(
        types::I64,
        MemFlagsData::trusted(),
        vmctx,
        CONTEXT_TAGGED_HEAP_OFFSET as i32,
    )
}

/// The running context's heap: the entry block's hoisted load when the body
/// has one, else a load at the site.
pub(crate) fn heap_ptr(fb: &mut FunctionBuilder, rt: &RtCtx) -> ClifValue {
    if let Some(heap) = rt.heap {
        return heap;
    }
    let vmctx = fb.use_var(rt.vmctx_var);
    load_heap_ptr(fb, vmctx)
}

/// Branch to `slow` when the owner at untagged address `owner` lies in the
/// published barrier window; leaves the builder in a fresh sealed block on
/// the outside-the-window path. The window is reloaded at every site: a
/// shim or safe point between two sites may have changed it.
fn emit_barrier_window_check(
    fb: &mut FunctionBuilder,
    heap: ClifValue,
    owner: ClifValue,
    slow: Block,
) {
    let lo = fb.ins().load(
        types::I64,
        MemFlagsData::trusted(),
        heap,
        HEAP_JIT_BARRIER_LO as i32,
    );
    let len = fb.ins().load(
        types::I64,
        MemFlagsData::trusted(),
        heap,
        HEAP_JIT_BARRIER_LEN as i32,
    );
    let offset = fb.ins().isub(owner, lo);
    let inside = fb.ins().icmp(IntCC::UnsignedLessThan, offset, len);
    let outside = fb.create_block();
    fb.ins().brif(inside, slow, &[], outside, &[]);
    fb.switch_to_block(outside);
    fb.seal_block(outside);
}

/// `setcar` (`is_cdr == false`) or `setcdr` of `cell` to `value`, inline —
/// GNU `Bsetcar`/`Bsetcdr`'s `XSETCAR`/`XSETCDR`: a cons outside the barrier
/// window is stored in place, `res` is defined as `value` and control jumps
/// to `merge`; anything else branches to `slow`, where the caller emits the
/// unchanged shim call (the non-cons signal included). Neither op consults
/// the function cell, in GNU or here.
pub(crate) fn emit_inline_cons_store(
    fb: &mut FunctionBuilder,
    rt: &RtCtx,
    cell: ClifValue,
    value: ClifValue,
    is_cdr: bool,
    slow: Block,
    res: Variable,
    merge: Block,
) {
    let tag = band_imm_p(fb, cell, TAG_MASK as i64);
    let is_cons = icmp_imm_p(fb, IntCC::Equal, tag, TAG_CONS as i64);
    let typed = fb.create_block();
    fb.ins().brif(is_cons, typed, &[], slow, &[]);
    fb.switch_to_block(typed);
    fb.seal_block(typed);
    // The tag is known, so untagging is a subtract (folds into the store's
    // addressing).
    let ptr = iadd_imm_p(fb, cell, -(TAG_CONS as i64));
    let heap = heap_ptr(fb, rt);
    emit_barrier_window_check(fb, heap, ptr, slow);
    let field = if is_cdr {
        core::mem::offset_of!(ConsCell, cdr_or_next)
    } else {
        core::mem::offset_of!(ConsCell, car)
    };
    fb.ins()
        .store(MemFlagsData::trusted(), value, ptr, field as i32);
    fb.def_var(res, value);
    fb.ins().jump(merge, &[]);
    note_inline_site();
    #[cfg(debug_assertions)]
    INLINE_HEAP_STORES_EMITTED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

/// `aset` of a plain vector or record, inline — GNU `Baset`'s in-bytecode
/// `ASET`: owned storage, an in-range fixnum index, not a tagged char-table
/// or bool-vector, an owner outside the barrier window that is not a
/// tenured owner the remembered set still lacks. On success stores `value`,
/// defines `res` as it and jumps to `cont`; anything else branches to
/// `slow`, where the caller emits the unchanged shim call (strings, signals,
/// mapped storage, the barrier's slow path).
///
/// neomacs's `Op::Aset` honours a redefined or advised `aset` (GNU `Baset`
/// never reads the function cell; a pre-existing deviation kept for tier
/// parity), so the site first compares the context's `aset` epoch cell with
/// the obarray's function epoch — the shim's own test, bit for bit — and a
/// mismatch takes the shim, which re-validates and re-arms the cell.
///
/// Returns `false`, emitting nothing, when a layout probe fails
/// (`LispValueVec::jit_slice_offsets` / `jit_owned_probe`).
pub(crate) fn emit_inline_aset(
    fb: &mut FunctionBuilder,
    rt: &RtCtx,
    array: ClifValue,
    index: ClifValue,
    value: ClifValue,
    slow: Block,
    res: Variable,
    cont: Block,
) -> bool {
    use crate::emacs_core::eval::runtime_projection::CONTEXT_ASET_EPOCH_OFFSET;
    use crate::emacs_core::symbol::OBARRAY_FUNCTION_EPOCH_OFFSET;
    use crate::tagged::header::{GC_HEADER_TENURED_OFFSET, GcHeader, LispValueVec};
    if LispValueVec::jit_slice_offsets().is_none() {
        return false;
    }
    let Some(owned_probe) = LispValueVec::jit_owned_probe() else {
        return false;
    };
    let vmctx = fb.use_var(rt.vmctx_var);
    let armed = fb.ins().load(
        types::I64,
        MemFlagsData::trusted(),
        vmctx,
        CONTEXT_ASET_EPOCH_OFFSET as i32,
    );
    let epoch = fb.ins().load(
        types::I64,
        MemFlagsData::trusted(),
        vmctx,
        (core::mem::offset_of!(Context, obarray) + OBARRAY_FUNCTION_EPOCH_OFFSET) as i32,
    );
    let stale = fb.ins().icmp(IntCC::NotEqual, armed, epoch);
    let armed_block = fb.create_block();
    fb.ins().brif(stale, slow, &[], armed_block, &[]);
    fb.switch_to_block(armed_block);
    fb.seal_block(armed_block);
    let Some(super::lowering::PlainSlot { object, slot }) =
        emit_plain_slot_address(fb, array, index, slow, Some(owned_probe))
    else {
        unreachable!("the slice offsets were probed above");
    };
    let heap = heap_ptr(fb, rt);
    emit_barrier_window_check(fb, heap, object, slow);
    // Outside the window only a tenured owner the remembered set lacks
    // needs the barrier: `tenured` and `remembered` are adjacent header
    // bytes, tested as one `u16`.
    let pair = fb.ins().uload16(
        types::I64,
        MemFlagsData::trusted(),
        object,
        GC_HEADER_TENURED_OFFSET as i32,
    );
    let needs_remembering = icmp_imm_p(
        fb,
        IntCC::Equal,
        pair,
        GcHeader::NEEDS_REMEMBERING_U16 as i64,
    );
    let store = fb.create_block();
    fb.ins().brif(needs_remembering, slow, &[], store, &[]);
    fb.switch_to_block(store);
    fb.seal_block(store);
    fb.ins().store(MemFlagsData::trusted(), value, slot, 0);
    fb.def_var(res, value);
    fb.ins().jump(cont, &[]);
    note_inline_site();
    #[cfg(debug_assertions)]
    INLINE_HEAP_STORES_EMITTED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    true
}

/// Inline allocations emitted, process-wide (see
/// [`INLINE_HEAP_STORES_EMITTED`]).
#[cfg(debug_assertions)]
pub(crate) static INLINE_ALLOCS_EMITTED: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// Bump one object of `stride` bytes out of the region whose cursor and
/// limit sit at heap offsets `cur_off`/`lim_off`: on room, advances the
/// cursor and leaves the builder in the fast block, returning the object's
/// address; otherwise branches to `slow`.
fn emit_region_bump(
    fb: &mut FunctionBuilder,
    heap: ClifValue,
    cur_off: usize,
    lim_off: usize,
    stride: usize,
    slow: Block,
) -> ClifValue {
    let cur = fb
        .ins()
        .load(types::I64, MemFlagsData::trusted(), heap, cur_off as i32);
    let lim = fb
        .ins()
        .load(types::I64, MemFlagsData::trusted(), heap, lim_off as i32);
    // Regions are whole objects, so `cur < lim` iff one is left; a closed
    // region is (0, 0).
    let room = fb.ins().icmp(IntCC::UnsignedLessThan, cur, lim);
    let fast = fb.create_block();
    fb.ins().brif(room, fast, &[], slow, &[]);
    fb.switch_to_block(fast);
    fb.seal_block(fast);
    let next = iadd_imm_p(fb, cur, stride as i64);
    fb.ins()
        .store(MemFlagsData::trusted(), next, heap, cur_off as i32);
    cur
}

/// `(cons car cdr)` inline — GNU `Fcons`: bump a cell out of the open cons
/// region and write car and cdr; an exhausted region calls
/// `neovm_jit_cons` (cold), which refills. Returns the tagged cons. Like
/// the shim, nothing here reaches a safe point, so the operands and the
/// residual stack need no roots.
pub(crate) fn emit_inline_cons(
    fb: &mut FunctionBuilder,
    rt: &RtCtx,
    car: ClifValue,
    cdr: ClifValue,
) -> ClifValue {
    let res = fb.declare_var(types::I64);
    let slow = fb.create_block();
    let merge = fb.create_block();
    let heap = heap_ptr(fb, rt);
    let cell = emit_region_bump(
        fb,
        heap,
        HEAP_JIT_CONS_CUR,
        HEAP_JIT_CONS_LIM,
        core::mem::size_of::<ConsCell>(),
        slow,
    );
    fb.ins().store(
        MemFlagsData::trusted(),
        car,
        cell,
        core::mem::offset_of!(ConsCell, car) as i32,
    );
    fb.ins().store(
        MemFlagsData::trusted(),
        cdr,
        cell,
        core::mem::offset_of!(ConsCell, cdr_or_next) as i32,
    );
    let tagged = bor_imm_p(fb, cell, TAG_CONS as i64);
    fb.def_var(res, tagged);
    fb.ins().jump(merge, &[]);
    fb.switch_to_block(slow);
    fb.seal_block(slow);
    fb.set_cold_block(slow);
    let cons = rt.refs.get(fb.func, Shim::Cons);
    let call = fb.ins().call(cons, &[car, cdr]);
    let boxed = fb.inst_results(call)[0];
    fb.def_var(res, boxed);
    fb.ins().jump(merge, &[]);
    fb.switch_to_block(merge);
    fb.seal_block(merge);
    note_inline_site();
    #[cfg(debug_assertions)]
    INLINE_ALLOCS_EMITTED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    fb.use_var(res)
}

/// Box the `f64` `value` inline — GNU `make_float`: bump a slot out of the
/// open float region (its header was written at the region's grant, born
/// at the current parity) and store the value; an exhausted region calls
/// `neovm_jit_make_float` (cold). Returns the tagged float.
pub(crate) fn emit_inline_box_float(
    fb: &mut FunctionBuilder,
    rt: &RtCtx,
    value: ClifValue,
) -> ClifValue {
    use crate::tagged::header::FLOAT_VALUE_OFFSET;
    let res = fb.declare_var(types::I64);
    let slow = fb.create_block();
    let merge = fb.create_block();
    let heap = heap_ptr(fb, rt);
    let slot = emit_region_bump(
        fb,
        heap,
        HEAP_JIT_FLOAT_CUR,
        HEAP_JIT_FLOAT_LIM,
        FLOAT_SLOT_BYTES,
        slow,
    );
    fb.ins()
        .store(MemFlagsData::trusted(), value, slot, FLOAT_VALUE_OFFSET);
    let tagged = bor_imm_p(fb, slot, crate::tagged::value::TAG_FLOAT as i64);
    fb.def_var(res, tagged);
    fb.ins().jump(merge, &[]);
    fb.switch_to_block(slow);
    fb.seal_block(slow);
    fb.set_cold_block(slow);
    let make_float = rt.refs.get(fb.func, Shim::MakeFloat);
    let call = fb.ins().call(make_float, &[value]);
    let boxed = fb.inst_results(call)[0];
    fb.def_var(res, boxed);
    fb.ins().jump(merge, &[]);
    fb.switch_to_block(merge);
    fb.seal_block(merge);
    note_inline_site();
    #[cfg(debug_assertions)]
    INLINE_ALLOCS_EMITTED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    fb.use_var(res)
}

/// Box `value` at a hot site: inline when the function allows it
/// (`RtCtx::inline_alloc`), else the `neovm_jit_make_float` call.
pub(crate) fn box_f64(fb: &mut FunctionBuilder, rt: &RtCtx, value: ClifValue) -> ClifValue {
    if rt.inline_alloc {
        emit_inline_box_float(fb, rt, value)
    } else {
        let make_float = rt.refs.get(fb.func, Shim::MakeFloat);
        let call = fb.ins().call(make_float, &[value]);
        fb.inst_results(call)[0]
    }
}

/// Whether a baseline body's inline heap sites justify loading the heap
/// pointer once at entry: two sites, or one inside a loop (the root-window
/// hoisting rule). `float_site` says whether the op at a pc is a
/// `Float`-feedback arithmetic site (which boxes its result).
pub(crate) fn hoist_heap_ptr(
    ops: &[Op],
    has_back_edge: bool,
    inline_alloc: bool,
    float_site: impl Fn(usize) -> bool,
) -> bool {
    let sites = ops
        .iter()
        .enumerate()
        .filter(|&(pc, op)| op_is_inline_heap_site(op, inline_alloc, &float_site, pc))
        .count();
    sites >= 2 || (sites == 1 && has_back_edge)
}

/// Whether the baseline lowers the `op` at `pc` with an inline heap site
/// (JIT only).
fn op_is_inline_heap_site(
    op: &Op,
    inline_alloc: bool,
    float_site: &impl Fn(usize) -> bool,
    pc: usize,
) -> bool {
    match op {
        Op::Setcar | Op::Setcdr | Op::Aset => jit_inline_heap_write_on(),
        Op::Cons => inline_alloc,
        Op::Add | Op::Sub | Op::Mul | Op::Div => inline_alloc && float_site(pc),
        _ => false,
    }
}
