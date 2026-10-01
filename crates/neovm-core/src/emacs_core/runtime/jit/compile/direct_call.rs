//! Direct native calls between compiled leaves (design
//! `p1-1-direct-native-calls` §3.4-§3.5, P1.0 S2.1c; `NEOVM_JIT_DIRECT_CALL`,
//! default off).
//!
//! A speculated `Op::Call` site of a byte-code callee runs GNU's `Bcall`
//! protocol inline instead of calling `neovm_jit_call_spec`, when the site's
//! spec slot holds a direct entry (armed by `arm_direct_entry_if_eligible`:
//! an exact-arity, frameless, register-ABI leaf). The hit path makes the
//! shim fast path's checks, in its order, each its own branch to the shim:
//!
//! 1. the slot's `direct_entry` is armed;
//! 2. the attention words are clear under `AttentionMask::SPEC_CALL` (the
//!    shim's gate: quit-flag, throw-on-input, the force harness) and the
//!    asynchronous word (profiler tick, OS signal, quit requests);
//! 3. the slot's epoch is the obarray's function epoch (no function cell
//!    anywhere changed since the slot was validated);
//! 4. `debug-on-next-call` is not armed;
//! 5. `depth < max_depth`;
//! 6. the specpdl has room for one entry (the shim grows it).
//!
//! It then pushes the frame the shim pushes -- `Backtrace1`/`Backtrace2`
//! with the arguments inline, or `BacktraceNative` pointing at the caller's
//! argument slot, recording the called SYMBOL (GNU `Bcall`'s `call_fun`) --
//! bumps `depth`, and calls the entry with the arguments in registers. On a
//! `STATUS_OK` return whose frame is still the one it pushed, it pops the
//! frame and `depth` inline; anything else (a non-OK status, a frame the
//! debugger flagged or promoted, an unbalanced specpdl) goes to
//! [`neovm_jit_direct_finish`], which is the shim's own cold exit
//! (`call_spec_finish`). A failed check goes to the shim itself, which runs
//! the reference protocol. So every path but the hit is today's code, and
//! the hit does exactly what the shim's fast path does.
//!
//! The finish is called by its baked address, never imported: direct calls
//! are JIT-only, and an AOT object can never reference it.

use super::jit_layout::{
    BacktraceLayout, CONTEXT_ATTENTION_OFFSET, CONTEXT_DEPTH_OFFSET, CONTEXT_MAX_DEPTH_OFFSET,
    CONTEXT_OBARRAY_OFFSET, CONTEXT_SPECPDL_OFFSET, EntryTemplate,
    OBARRAY_DEBUG_ON_NEXT_CALL_FWD_OFFSET, OBARRAY_FUNCTION_EPOCH_OFFSET, VecOffsets,
};
use super::lowering::{RtCtx, iadd_imm_p, icmp_imm_p, ishl_imm_p};
use super::reg_abi::MAX_REG_ARGS;
use super::spec_slot::{
    SPEC_SLOT_DIRECT_ENTRY_OFFSET, SPEC_SLOT_EPOCH_OFFSET, SPEC_SLOT_KEY_OFFSET,
    SPEC_SLOT_LEAF_OFFSET,
};
use super::*;
use std::sync::atomic::AtomicU64;

/// Direct sites a leaf may emit (design §3.9); later sites keep the shim.
pub(crate) const DIRECT_SITE_CAP: u32 = 8;

/// Direct sites emitted, process-wide (the `[neovm-jit-final-builtin-leaves]`
/// census and the tests' engagement evidence).
pub(crate) static DIRECT_SITES_EMITTED: AtomicU64 = AtomicU64::new(0);
/// Direct calls that left the hit path through [`neovm_jit_direct_finish`].
pub(crate) static DIRECT_COLD_EXITS: AtomicU64 = AtomicU64::new(0);

std::thread_local! {
    /// Whether the body being compiled on this thread does unbounded work
    /// per entry (see [`UnboundedBodyScope`]); true outside any scope.
    static UNBOUNDED_BODY: core::cell::Cell<bool> = const { core::cell::Cell::new(true) };
}

/// Whether the body being compiled does unbounded work per entry
/// ([`DirectSitesMode::Unbounded`]); a lowering outside a compile request
/// (an OSR entry, the tests' direct builds) counts as such.
pub(crate) fn unbounded_body() -> bool {
    UNBOUNDED_BODY.with(core::cell::Cell::get)
}

/// For its lifetime, the verdict [`unbounded_body`] answers:
/// `compile_bytecode_function_requested` sets it from the body's shape (a
/// back edge, a call of itself) and the request (a re-tier of a leaf that
/// proved hot); the previous one is restored on drop.
pub(crate) struct UnboundedBodyScope(bool);

impl UnboundedBodyScope {
    pub(crate) fn enter(unbounded: bool) -> Self {
        Self(UNBOUNDED_BODY.with(|c| c.replace(unbounded)))
    }
}

impl Drop for UnboundedBodyScope {
    fn drop(&mut self) {
        UNBOUNDED_BODY.with(|c| c.set(self.0));
    }
}

/// The lambda list a direct site calls into, decided at compile time from
/// its expected callee: how the call's arguments become the callee's
/// register words (`nonrest` slots, nil for each one the call lacks, then
/// the `&rest` list when `rest`).
/// Threading: immutable compile-time counts, with no mutator-owned values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CalleeShape {
    pub(crate) required: usize,
    pub(crate) nonrest: usize,
    pub(crate) rest: bool,
}

impl CalleeShape {
    /// The shape of a lambda list of exactly `n` required parameters.
    pub(crate) const fn exact(n: usize) -> Self {
        Self {
            required: n,
            nonrest: n,
            rest: false,
        }
    }

    /// The register words the callee takes (its leaf's `arity`).
    pub(crate) fn arity(self) -> usize {
        self.nonrest + usize::from(self.rest)
    }

    /// Whether a call of `nargs` arguments is the callee's frame as laid
    /// out (no nil, no list): what the spec shim's exact path arms
    /// (`arm_direct_entry_if_eligible`). Any other call's entry is armed by
    /// the site's own slow path, which knows its shape
    /// ([`neovm_jit_direct_slow`]).
    pub(crate) fn passes_through(self, nargs: usize) -> bool {
        !self.rest && self.nonrest == nargs
    }

    /// The shape as one word, for the slow path.
    fn word(self) -> i64 {
        self.nonrest as i64 | (self.required as i64) << 8 | i64::from(self.rest) << 16
    }

    fn from_word(word: i64) -> Self {
        Self {
            nonrest: (word & 0xff) as usize,
            required: ((word >> 8) & 0xff) as usize,
            rest: (word >> 16) & 1 != 0,
        }
    }
}

/// The most arguments a direct call of a `&rest` callee passes: the site
/// conses the words past the callee's `nonrest` slots inline.
pub(crate) const MAX_REST_CALL_ARGS: usize = 8;

/// A site the lowering will emit as a direct call: its constants and the
/// probed layouts its push and pop use.
/// Threading: belongs to one compiler's lowering; the emitted slot belongs
/// to the caller's mutator-owned compiled leaf, as for existing spec sites.
#[derive(Clone, Copy, Debug)]
pub(crate) struct DirectSite {
    expected: u64,
    /// The call's own argument count: what its frame records.
    nargs: usize,
    /// How the call enters the callee's register words.
    callee: CalleeShape,
    frame: EntryTemplate,
    small: u32,
    specpdl: VecOffsets,
    /// A closure source site's (`plan_source`): the offset of the callee
    /// object's constant-base word, its leaf's `aux`.
    consts_ptr: Option<usize>,
}

/// Whose call a direct site makes.
/// Threading: SSA values of one lowering, never shared Lisp runtime state.
#[derive(Clone, Copy)]
pub(crate) enum DirectCallee {
    /// A speculated symbol: the slot's epoch is the function epoch it was
    /// validated at, the frame records the symbol `sym_v`, the slot's key
    /// is `aux`, and the slow path is `neovm_jit_call_spec` on the symbol
    /// and its expected object `exp_v`.
    Symbol { sym_v: ClifValue, exp_v: ClifValue },
    /// A closure source site (`source_slots`): the slot's epoch is the
    /// `leaf_slot_epoch` its leaf was armed under, the frame records the
    /// called object `callee`, `aux` is that object's own constant base,
    /// and the slow path is `neovm_jit_call_source_spec`, whose
    /// `STATUS_NEED_GENERIC` the caller handles. The hit path stores its
    /// value in the call's result slot and joins the others with
    /// `STATUS_OK`.
    Source { callee: ClifValue },
}

impl DirectSite {
    /// The site of an `Op::Call` of `nargs` arguments speculated on the
    /// byte-code object `expected`, when a direct call is possible: the knob
    /// is on, the force harness off, the build JIT, the callee takes exactly
    /// `nargs` required arguments in registers -- or, under
    /// `NEOVM_JIT_DIRECT_SHAPES`, has `&optional` slots the call fills or
    /// lacks (`optional`) or a `&rest` list (`rest`, at most
    /// [`MAX_REST_CALL_ARGS`] arguments) -- in at most [`MAX_REG_ARGS`]
    /// register words, both layout probes succeeded, and the leaf's budget
    /// of direct sites is not spent.
    ///
    /// The callee must also be a body `LeafAbi::for_build` gives the
    /// register ABI: not `make-closure`-patched, and frameless (no dynamic
    /// bindings, no handler frames). Any other callee's leaf never arms a
    /// direct entry, so its site would only pay the direct site's compile
    /// and its unarmed test on every call. And the caller must be a body
    /// whose calls run often enough to pay the site's compile back
    /// ([`DirectSitesMode`], [`UnboundedBodyScope`]).
    pub(crate) fn plan(rt: &RtCtx, aot: bool, expected: u64, nargs: usize) -> Option<Self> {
        if aot || !jit_direct_call_on() || jit_force_slow_spec() || nargs > MAX_REST_CALL_ARGS {
            return None;
        }
        if jit_direct_sites() == DirectSitesMode::Unbounded && !unbounded_body() {
            return None;
        }
        let bc = Value::from_bits(expected as usize).bytecode_data_if_materialized()?;
        let callee = CalleeShape {
            required: bc.params.required.len(),
            nonrest: bc.params.required.len() + bc.params.optional.len(),
            rest: bc.params.rest.is_some(),
        };
        let shapes = jit_direct_shapes();
        let callable = if callee.rest {
            shapes.rest && nargs >= callee.required
        } else if callee.nonrest > callee.required {
            shapes.optional && (callee.required..=callee.nonrest).contains(&nargs)
        } else {
            callee.required == nargs
        };
        if !callable
            || callee.arity() > MAX_REG_ARGS
            || (!callee.rest && nargs > MAX_REG_ARGS)
            || rt.direct_sites.get() >= DIRECT_SITE_CAP
        {
            return None;
        }
        if bc.jit_runtime().patched_prefix() > 0 {
            return None;
        }
        let ops = bc.executable_ops();
        if super::leaf::body_has_binds(ops) || super::leaf::body_has_handlers(ops) {
            return None;
        }
        let layout: BacktraceLayout = super::jit_layout::backtrace_layout()?;
        let specpdl = super::jit_layout::specpdl_vec_offsets()?;
        let (frame, small) = layout.frame_for(nargs);
        rt.direct_sites.set(rt.direct_sites.get() + 1);
        Some(DirectSite {
            expected,
            nargs,
            callee,
            frame,
            small,
            specpdl,
            consts_ptr: None,
        })
    }

    /// The site of a closure source call (`source_slots`) of `nargs`
    /// arguments, when a direct call is possible: the knob, the build, the
    /// layouts and the budget as [`Self::plan`]; the callee's arity is
    /// checked when its leaf is armed (`arm_source_direct_entry`), and the
    /// constant base is read from the callee object.
    pub(crate) fn plan_source(rt: &RtCtx, aot: bool, nargs: usize) -> Option<Self> {
        if aot || !jit_direct_call_on() || jit_force_slow_spec() || nargs > MAX_REG_ARGS {
            return None;
        }
        if rt.direct_sites.get() >= DIRECT_SITE_CAP {
            return None;
        }
        let layout: BacktraceLayout = super::jit_layout::backtrace_layout()?;
        let specpdl = super::jit_layout::specpdl_vec_offsets()?;
        let (consts_ptr, _) = super::jit_layout::bytecode_constants_offsets()?;
        let (frame, small) = layout.frame_for(nargs);
        rt.direct_sites.set(rt.direct_sites.get() + 1);
        Some(DirectSite {
            expected: 0,
            nargs,
            callee: CalleeShape::exact(nargs),
            frame,
            small,
            specpdl,
            consts_ptr: Some(consts_ptr),
        })
    }
}

/// What [`emit_direct_bytecode_call`] leaves the call lowering: the builder
/// sits in the join of the slow and cold paths, whose status is `status`;
/// `hot_done` is the hit path's end, still to be terminated (restore the
/// root window, jump to the continuation) -- `None` for a source site,
/// whose hit joins the others; `result` holds the call's value on every
/// path that reaches the continuation.
pub(crate) struct DirectCall {
    pub(crate) status: ClifValue,
    pub(crate) result: Variable,
    pub(crate) hot_done: Option<Block>,
}

/// Emit a direct call of `args` at a site whose callee and slot are
/// `callee` and `slot_v` (see the module docs and [`DirectCallee`]). The
/// caller has rooted the residual stack; the arguments are not spilled yet.
pub(crate) fn emit_direct_bytecode_call(
    fb: &mut FunctionBuilder,
    rt: &RtCtx,
    site: &DirectSite,
    args: &[ClifValue],
    callee: DirectCallee,
    slot_v: ClifValue,
) -> DirectCall {
    debug_assert_eq!(args.len(), site.nargs);
    DIRECT_SITES_EMITTED.fetch_add(1, Ordering::Relaxed);
    #[cfg(any(test, debug_assertions))]
    DIRECT_SITES_EMITTED_HERE.with(|c| c.set(c.get() + 1));
    let flags = MemFlagsData::trusted();
    let ptr_ty = rt.ptr_ty;
    let status_var = fb.declare_var(types::I64);
    let result = fb.declare_var(types::I64);
    let slow = fb.create_block();
    let join = fb.create_block();
    let hot_done = match callee {
        DirectCallee::Symbol { .. } => Some(fb.create_block()),
        DirectCallee::Source { .. } => None,
    };
    fb.set_cold_block(slow);
    let vmctx = fb.use_var(rt.vmctx_var);

    // Each check is its own branch to the shim, the armed key first.
    let next = |fb: &mut FunctionBuilder, go_slow: ClifValue| {
        let block = fb.create_block();
        fb.ins().brif(go_slow, slow, &[], block, &[]);
        fb.switch_to_block(block);
        fb.seal_block(block);
    };
    // 1. Armed.
    let entry = fb.ins().load(
        types::I64,
        flags,
        slot_v,
        SPEC_SLOT_DIRECT_ENTRY_OFFSET as i32,
    );
    let unarmed = icmp_imm_p(fb, IntCC::Equal, entry, 0);
    next(fb, unarmed);
    // 2. Attention: the shim gate's mask, and the asynchronous word (a
    // JIT-only bake of a process address).
    let attention = fb
        .ins()
        .uload32(flags, vmctx, CONTEXT_ATTENTION_OFFSET as i32);
    let attention = super::lowering::band_imm_p(
        fb,
        attention,
        i64::from(crate::emacs_core::eval::AttentionMask::SPEC_CALL.bits()),
    );
    let async_addr = fb.ins().iconst(
        ptr_ty,
        crate::emacs_core::eval::ASYNC_ATTENTION.addr() as i64,
    );
    let async_word = fb.ins().uload32(flags, async_addr, 0);
    let pending = fb.ins().bor(attention, async_word);
    let pending = icmp_imm_p(fb, IntCC::NotEqual, pending, 0);
    next(fb, pending);
    // 3. Epoch: the function epoch a named slot was validated at, or the
    // leaf-slot epoch a source slot's leaf was armed under (a JIT-only
    // bake of a process address).
    let live_epoch = match callee {
        DirectCallee::Symbol { .. } => fb.ins().load(
            types::I64,
            flags,
            vmctx,
            (CONTEXT_OBARRAY_OFFSET + OBARRAY_FUNCTION_EPOCH_OFFSET) as i32,
        ),
        DirectCallee::Source { .. } => {
            let epoch_addr = fb.ins().iconst(
                ptr_ty,
                crate::emacs_core::jit::cache::leaf_slot_epoch_addr() as i64,
            );
            fb.ins().load(types::I64, flags, epoch_addr, 0)
        }
    };
    let armed_epoch = fb
        .ins()
        .load(types::I64, flags, slot_v, SPEC_SLOT_EPOCH_OFFSET as i32);
    let stale = fb.ins().icmp(IntCC::NotEqual, live_epoch, armed_epoch);
    next(fb, stale);
    // 4. `debug-on-next-call` (the cell is never null).
    let cell = fb.ins().load(
        ptr_ty,
        flags,
        vmctx,
        (CONTEXT_OBARRAY_OFFSET + OBARRAY_DEBUG_ON_NEXT_CALL_FWD_OFFSET) as i32,
    );
    let debug = fb.ins().uload8(
        types::I64,
        flags,
        cell,
        crate::emacs_core::forward::LISP_BOOL_FWD_VALUE_OFFSET as i32,
    );
    let armed_debugger = icmp_imm_p(fb, IntCC::NotEqual, debug, 0);
    next(fb, armed_debugger);
    // 5. Depth.
    let depth = fb
        .ins()
        .load(types::I64, flags, vmctx, CONTEXT_DEPTH_OFFSET as i32);
    let max_depth = fb
        .ins()
        .load(types::I64, flags, vmctx, CONTEXT_MAX_DEPTH_OFFSET as i32);
    let too_deep = fb
        .ins()
        .icmp(IntCC::UnsignedGreaterThanOrEqual, depth, max_depth);
    next(fb, too_deep);
    // 6. Room for the frame.
    let spec_len_off = (CONTEXT_SPECPDL_OFFSET + site.specpdl.len) as i32;
    let spec_ptr_off = (CONTEXT_SPECPDL_OFFSET + site.specpdl.ptr) as i32;
    let spec_cap_off = (CONTEXT_SPECPDL_OFFSET + site.specpdl.cap) as i32;
    let len = fb.ins().load(types::I64, flags, vmctx, spec_len_off);
    let cap = fb.ins().load(types::I64, flags, vmctx, spec_cap_off);
    let full = fb.ins().icmp(IntCC::Equal, len, cap);
    next(fb, full);
    // 7. A `&rest` callee: the slot's key says the leaf it holds takes this
    // call through a list (armed by this site's slow path, which knows the
    // shape), not as laid out -- an entry the spec shim's exact path armed
    // for a different object at the expected bits would read the list as
    // an argument. The key is the `aux` word too.
    let rest_key = match callee {
        DirectCallee::Symbol { .. } if site.callee.rest => {
            let key = fb
                .ins()
                .load(types::I64, flags, slot_v, SPEC_SLOT_KEY_OFFSET as i32);
            let key_flags = super::lowering::band_imm_p(fb, key, SpecSlot::KEY_FLAGS as i64);
            let other = icmp_imm_p(
                fb,
                IntCC::NotEqual,
                key_flags,
                (SpecSlot::KEY_SHORT_CALL | SpecSlot::KEY_REGISTER) as i64,
            );
            next(fb, other);
            Some(key)
        }
        _ => None,
    };
    fb.seal_block(slow);
    // The callee's register words: the given arguments in its `nonrest`
    // slots, nil for each slot the call lacks, then the `&rest` list of the
    // arguments past them -- GNU `funcall_lambda`'s frame (`Flist` of the
    // tail). Consing never collects, and nothing between here and the
    // callee's entry reaches a safe point; the elements are the call's own
    // arguments, which the frame pushed below records.
    let regs: SmallVec<[ClifValue; MAX_REG_ARGS]> = if site.callee.passes_through(site.nargs) {
        args.iter().copied().collect()
    } else {
        let fixed = site.nargs.min(site.callee.nonrest);
        let nil = fb.ins().iconst(types::I64, Value::NIL.bits() as i64);
        let mut regs: SmallVec<[ClifValue; MAX_REG_ARGS]> = args[..fixed].iter().copied().collect();
        regs.extend(core::iter::repeat_n(nil, site.callee.nonrest - fixed));
        if site.callee.rest {
            let mut list = nil;
            for &a in args[fixed..].iter().rev() {
                list = if rt.inline_alloc {
                    super::heap_inline::emit_inline_cons(fb, rt, a, list)
                } else {
                    let cons = rt.refs.get(fb.func, Shim::Cons);
                    let call = fb.ins().call(cons, &[a, list]);
                    fb.inst_results(call)[0]
                };
            }
            regs.push(list);
        }
        regs
    };
    debug_assert_eq!(regs.len(), site.callee.arity());

    // The push: the frame the shim writes, recording the symbol.
    const _: () = assert!(core::mem::size_of::<crate::emacs_core::eval::SpecBinding>() == 32);
    let base = fb.ins().load(ptr_ty, flags, vmctx, spec_ptr_off);
    let byte_off = ishl_imm_p(fb, len, 5);
    let at = fb.ins().iadd(base, byte_off);
    let header = fb
        .ins()
        .iconst(types::I64, site.frame.header_with(site.small) as i64);
    fb.ins()
        .store(flags, header, at, site.frame.header_offset as i32);
    let recorded = match callee {
        DirectCallee::Symbol { sym_v, .. } => sym_v,
        DirectCallee::Source { callee } => callee,
    };
    fb.ins()
        .store(flags, recorded, at, site.frame.field(0) as i32);
    match site.nargs {
        1 => {
            fb.ins()
                .store(flags, args[0], at, site.frame.field(1) as i32);
        }
        2 => {
            fb.ins()
                .store(flags, args[0], at, site.frame.field(1) as i32);
            fb.ins()
                .store(flags, args[1], at, site.frame.field(2) as i32);
        }
        _ => {
            // `BacktraceNative`: the frame reads the caller's argument slot
            // for as long as it lives, like the shim's.
            for (i, &a) in args.iter().enumerate() {
                fb.ins()
                    .stack_store(ptr_ty, a, rt.call_args_slot, (i * 8) as i32);
            }
            let args_addr = fb.ins().stack_addr(ptr_ty, rt.call_args_slot, 0);
            fb.ins()
                .store(flags, args_addr, at, site.frame.field(1) as i32);
        }
    }
    let len1 = iadd_imm_p(fb, len, 1);
    fb.ins().store(flags, len1, vmctx, spec_len_off);
    let depth1 = iadd_imm_p(fb, depth, 1);
    fb.ins()
        .store(flags, depth1, vmctx, CONTEXT_DEPTH_OFFSET as i32);
    // The ENTERED leaf, for the cold exit (a recursive re-arm may change the
    // slot's word while the callee runs), and the callee's constant base.
    let leaf = fb
        .ins()
        .load(types::I64, flags, slot_v, SPEC_SLOT_LEAF_OFFSET as i32);
    let aux = match (callee, site.consts_ptr, rest_key) {
        (DirectCallee::Source { callee }, Some(consts_ptr), _) => {
            // The executing instance's own constant base.
            let object = super::lowering::band_imm_p(fb, callee, !(TAG_MASK as i64));
            fb.ins().load(types::I64, flags, object, consts_ptr as i32)
        }
        (_, _, Some(key)) => super::lowering::band_imm_p(fb, key, !(SpecSlot::KEY_FLAGS as i64)),
        _ => {
            // The key is the constant base with the register flag set.
            let key = fb
                .ins()
                .load(types::I64, flags, slot_v, SPEC_SLOT_KEY_OFFSET as i32);
            super::lowering::band_imm_p(fb, key, !(SpecSlot::KEY_FLAGS as i64))
        }
    };
    let sig = fb.import_signature(
        LeafAbi::Register {
            arity: site.callee.arity() as u8,
        }
        .signature(rt.refs.call_conv, ptr_ty),
    );
    let mut call_args: SmallVec<[ClifValue; 8]> = SmallVec::new();
    call_args.extend([vmctx, aux]);
    call_args.extend(regs.iter().copied());
    let call = fb.ins().call_indirect(sig, entry, &call_args);
    let (value, status) = {
        let r = fb.inst_results(call);
        (r[0], r[1])
    };
    let cold = fb.create_block();
    fb.append_block_param(cold, types::I64); // status
    fb.append_block_param(cold, types::I64); // value
    fb.set_cold_block(cold);
    let returned = fb.create_block();
    let ok = icmp_imm_p(fb, IntCC::Equal, status, STATUS_OK);
    fb.ins().brif(
        ok,
        returned,
        &[],
        cold,
        &[BlockArg::Value(status), BlockArg::Value(value)],
    );
    // The pop: the specpdl is back to our frame, and the frame is still the
    // one we pushed (a flagged or promoted frame goes to the finish, which
    // runs the exit debugger).
    fb.switch_to_block(returned);
    fb.seal_block(returned);
    // Recomputed rather than kept: Cranelift does not rematerialize, and a
    // value kept across the call costs a callee-saved register or a spill.
    let status_ok = fb.ins().iconst(types::I64, STATUS_OK);
    let len1 = iadd_imm_p(fb, len, 1);
    let len2 = fb.ins().load(types::I64, flags, vmctx, spec_len_off);
    let balanced = fb.ins().icmp(IntCC::Equal, len2, len1);
    let ours_check = fb.create_block();
    fb.ins().brif(
        balanced,
        ours_check,
        &[],
        cold,
        &[BlockArg::Value(status_ok), BlockArg::Value(value)],
    );
    fb.switch_to_block(ours_check);
    fb.seal_block(ours_check);
    // Reloaded: a nested push may have reallocated the specpdl.
    let base2 = fb.ins().load(ptr_ty, flags, vmctx, spec_ptr_off);
    let byte_off2 = ishl_imm_p(fb, len, 5);
    let at2 = fb.ins().iadd(base2, byte_off2);
    let word = fb
        .ins()
        .load(types::I64, flags, at2, site.frame.header_offset as i32);
    let mask = fb.ins().iconst(types::I64, site.frame.header_mask as i64);
    let masked = fb.ins().band(word, mask);
    let header2 = fb
        .ins()
        .iconst(types::I64, site.frame.header_with(site.small) as i64);
    let ours = fb.ins().icmp(IntCC::Equal, masked, header2);
    let pop = fb.create_block();
    fb.ins().brif(
        ours,
        pop,
        &[],
        cold,
        &[BlockArg::Value(status_ok), BlockArg::Value(value)],
    );
    fb.seal_block(cold);
    fb.switch_to_block(pop);
    fb.seal_block(pop);
    fb.ins().store(flags, len, vmctx, spec_len_off);
    let depth2 = fb
        .ins()
        .load(types::I64, flags, vmctx, CONTEXT_DEPTH_OFFSET as i32);
    let depth3 = iadd_imm_p(fb, depth2, -1);
    fb.ins()
        .store(flags, depth3, vmctx, CONTEXT_DEPTH_OFFSET as i32);
    fb.def_var(result, value);
    match hot_done {
        Some(hot_done) => {
            fb.ins().jump(hot_done, &[]);
        }
        None => {
            // A source site: the value where the other paths leave theirs.
            fb.ins().stack_store(ptr_ty, value, rt.call_result_slot, 0);
            let ok = fb.ins().iconst(types::I64, STATUS_OK);
            fb.def_var(status_var, ok);
            fb.ins().jump(join, &[]);
        }
    }

    // The cold exit: the shim's own (`call_spec_finish`), by baked address.
    fb.switch_to_block(cold);
    let cold_status = fb.block_params(cold)[0];
    let cold_value = fb.block_params(cold)[1];
    let vmctx_c = fb.use_var(rt.vmctx_var);
    let exp_c = match callee {
        DirectCallee::Symbol { .. } => fb.ins().iconst(types::I64, site.expected as i64),
        DirectCallee::Source { callee } => callee,
    };
    let args_c = if site.nargs <= 2 {
        // The frame holds the arguments; the finish reads them there.
        fb.ins().iconst(ptr_ty, 0)
    } else {
        fb.ins().stack_addr(ptr_ty, rt.call_args_slot, 0)
    };
    let nargs_c = fb.ins().iconst(types::I64, site.nargs as i64);
    let out_c = fb.ins().stack_addr(ptr_ty, rt.call_result_slot, 0);
    let finish_sig = fb.import_signature(finish_signature(rt.refs.call_conv, ptr_ty));
    let finish_addr = fb
        .ins()
        .iconst(ptr_ty, neovm_jit_direct_finish as *const () as usize as i64);
    let finish = fb.ins().call_indirect(
        finish_sig,
        finish_addr,
        &[
            vmctx_c,
            exp_c,
            leaf,
            cold_status,
            cold_value,
            len,
            args_c,
            nargs_c,
            out_c,
        ],
    );
    let finished = fb.inst_results(finish)[0];
    fb.def_var(status_var, finished);
    let loaded = fb
        .ins()
        .stack_load(ptr_ty, types::I64, rt.call_result_slot, 0);
    fb.def_var(result, loaded);
    fb.ins().jump(join, &[]);

    // The shim, verbatim: the reference protocol.
    fb.switch_to_block(slow);
    for (i, &a) in args.iter().enumerate() {
        fb.ins()
            .stack_store(ptr_ty, a, rt.call_args_slot, (i * 8) as i32);
    }
    let vmctx_s = fb.use_var(rt.vmctx_var);
    let args_addr = fb.ins().stack_addr(ptr_ty, rt.call_args_slot, 0);
    let out_addr = fb.ins().stack_addr(ptr_ty, rt.call_result_slot, 0);
    let n_val = fb.ins().iconst(types::I64, site.nargs as i64);
    let shim = match callee {
        DirectCallee::Symbol { sym_v, exp_v } if site.callee.passes_through(site.nargs) => {
            let call_spec = rt.refs.get(fb.func, Shim::CallSpec);
            fb.ins().call(
                call_spec,
                &[vmctx_s, sym_v, exp_v, slot_v, args_addr, n_val, out_addr],
            )
        }
        DirectCallee::Symbol { sym_v, exp_v } => {
            // A call the shim's exact path does not arm: the shim, then
            // this site's own arming of its shape.
            let slow_sig = fb.import_signature(slow_signature(rt.refs.call_conv, ptr_ty));
            let slow_addr = fb
                .ins()
                .iconst(ptr_ty, neovm_jit_direct_slow as *const () as usize as i64);
            let shape = fb.ins().iconst(types::I64, site.callee.word());
            fb.ins().call_indirect(
                slow_sig,
                slow_addr,
                &[
                    vmctx_s, sym_v, exp_v, slot_v, args_addr, n_val, out_addr, shape,
                ],
            )
        }
        DirectCallee::Source { callee } => super::source_slots::emit_source_call(
            fb, rt, slot_v, vmctx_s, callee, args_addr, n_val, out_addr,
        ),
    };
    let shim_status = fb.inst_results(shim)[0];
    fb.def_var(status_var, shim_status);
    let loaded = fb
        .ins()
        .stack_load(ptr_ty, types::I64, rt.call_result_slot, 0);
    fb.def_var(result, loaded);
    fb.ins().jump(join, &[]);

    fb.switch_to_block(join);
    fb.seal_block(join);
    let status = fb.use_var(status_var);
    DirectCall {
        status,
        result,
        hot_done,
    }
}

/// [`neovm_jit_direct_slow`]'s Cranelift signature: `neovm_jit_call_spec`'s
/// and the site's shape word.
fn slow_signature(call_conv: cranelift_codegen::isa::CallConv, ptr_ty: types::Type) -> Signature {
    let mut sig = Signature::new(call_conv);
    for ty in [
        ptr_ty,     // vmctx
        types::I64, // the called symbol
        types::I64, // expected
        types::I64, // slot
        ptr_ty,     // args
        types::I64, // nargs
        ptr_ty,     // out
        types::I64, // the callee's shape (`CalleeShape::word`)
    ] {
        sig.params.push(AbiParam::new(ty));
    }
    sig.returns.push(AbiParam::new(types::I64));
    sig
}

/// The slow path of a direct site whose call is not its callee's frame as
/// laid out (a short call of an `&optional` callee, a call of a `&rest`
/// one): `neovm_jit_call_spec`, the reference protocol, and then -- the
/// slot holding a leaf and no direct entry -- the arming of the leaf's
/// register entry when the leaf takes the call in the site's `shape`
/// ([`super::spec_slot::arm_shaped_direct_entry`]). The spec shim's own
/// arming covers only a call as laid out, the only shape it can check
/// without the site's.
///
/// SAFETY: `neovm_jit_call_spec`'s contract; `slot` is the executing
/// leaf's spec slot of this site.
#[allow(clippy::too_many_arguments, clippy::not_unsafe_ptr_arg_deref)]
#[inline(never)]
pub(crate) extern "C" fn neovm_jit_direct_slow(
    ctx: *mut u8,
    sym_bits: i64,
    expected: i64,
    slot: i64,
    args: *const i64,
    nargs: i64,
    out: *mut i64,
    shape: i64,
) -> i64 {
    let status =
        super::dispatch::neovm_jit_call_spec(ctx, sym_bits, expected, slot, args, nargs, out);
    // SAFETY: the executing leaf's slot (the contract above).
    let slot = unsafe { &*(slot as *const SpecSlot) };
    if slot.direct_entry.load(Ordering::Relaxed) == 0 && !slot.leaf_ptr().is_null() {
        super::spec_slot::arm_shaped_direct_entry(
            slot,
            CalleeShape::from_word(shape),
            nargs as usize,
        );
    }
    status
}

/// `neovm_jit_direct_finish`'s Cranelift signature.
fn finish_signature(call_conv: cranelift_codegen::isa::CallConv, ptr_ty: types::Type) -> Signature {
    let mut sig = Signature::new(call_conv);
    for ty in [
        ptr_ty,     // vmctx
        types::I64, // expected
        types::I64, // the entered leaf
        types::I64, // status
        types::I64, // value
        types::I64, // bt_count (the frame's specpdl index)
        ptr_ty,     // args (null: read them from the frame)
        types::I64, // nargs
        ptr_ty,     // out
    ] {
        sig.params.push(AbiParam::new(ty));
    }
    sig.returns.push(AbiParam::new(types::I64));
    sig
}

/// The cold exit of a direct call: every way the call can end but a
/// balanced `STATUS_OK` with its frame untouched -- a signal, a deopt
/// (precise resume or rerun), a contained panic, a frame the debugger
/// flagged, an unbalanced specpdl. It is the shim fast path's exit,
/// `call_spec_finish`, run with the frame still pushed and the depth still
/// counted, as there. `leaf` is the leaf the site entered and `bt_count`
/// its frame's specpdl index; `args` addresses the call's arguments, or is
/// null when the frame holds them inline (one or two arguments), and they
/// are read from the frame.
///
/// SAFETY: called only by a direct site's cold block, with the vmctx
/// contract of every shim and a live `leaf`.
#[allow(clippy::too_many_arguments, clippy::not_unsafe_ptr_arg_deref)]
#[cold]
#[inline(never)]
pub(crate) extern "C" fn neovm_jit_direct_finish(
    ctx: *mut u8,
    expected: i64,
    leaf: i64,
    status: i64,
    value: i64,
    bt_count: i64,
    args: *const i64,
    nargs: i64,
    out: *mut i64,
) -> i64 {
    DIRECT_COLD_EXITS.fetch_add(1, Ordering::Relaxed);
    let bt_count = bt_count as usize;
    let nargs = nargs as usize;
    let mut inline_args = [0i64; 2];
    let args = if args.is_null() {
        // SAFETY: the dormant seam Context (the shim contract); reads only.
        let ctx_ref = unsafe { &*(ctx as *const crate::emacs_core::eval::Context) };
        if let Some((_, values, _, _)) = ctx_ref
            .specpdl
            .get(bt_count)
            .and_then(|entry| ctx_ref.backtrace_entry_values(entry))
        {
            for (slot, v) in inline_args.iter_mut().zip(values.iter()) {
                *slot = v.bits() as i64;
            }
        }
        inline_args.as_ptr()
    } else {
        args
    };
    // SAFETY: the site passes the leaf it entered, which stays allocated
    // (retired leaves are never freed under a native frame).
    let leaf = unsafe { &*(leaf as usize as *const CompiledLeaf) };
    let run = if status == STATUS_OK {
        super::dispatch::FastRun::Done(Value::from_bits(value as usize))
    } else {
        super::dispatch::FastRun::Raw(status)
    };
    super::dispatch::call_spec_finish(
        ctx,
        Value::from_bits(expected as usize),
        leaf,
        args,
        nargs,
        out,
        bt_count,
        run,
    )
}

#[cfg(any(test, debug_assertions))]
thread_local! {
    /// Direct sites emitted on this thread (tests' engagement evidence).
    pub(crate) static DIRECT_SITES_EMITTED_HERE: core::cell::Cell<usize> =
        const { core::cell::Cell::new(0) };
}

/// Direct sites emitted on this thread so far (tests).
#[cfg(test)]
pub(crate) fn direct_sites_emitted_for_test() -> usize {
    DIRECT_SITES_EMITTED_HERE.with(core::cell::Cell::get)
}

/// The census entry of the `[neovm-jit-final-builtin-leaves]` line, when a
/// direct site was emitted.
pub(crate) fn render_direct_call_stats() -> Option<String> {
    let sites = DIRECT_SITES_EMITTED.load(Ordering::Relaxed);
    (sites > 0).then(|| {
        format!(
            "direct-call:sites={sites},armed={},cold={}",
            super::spec_slot::DIRECT_ENTRIES_ARMED.load(Ordering::Relaxed),
            DIRECT_COLD_EXITS.load(Ordering::Relaxed)
        )
    })
}
