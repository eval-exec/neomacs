//! Closure source slots (design `p2-1-feedback-reopt` §3.5.1, P2.1 C5;
//! `NEOVM_JIT_SPEC_SOURCES`): a T1 or OSR compile turns an `Op::Call` whose
//! callee is not a constant, and whose recorded target is ONE closure
//! source, into a guarded speculated call.
//!
//! The site's guard compares the callee's source identity -- the word its
//! `runtime` field holds (`jit_layout::BYTECODE_RUNTIME_WORD_OFFSET`) --
//! with the recorded source's, after a veclike tag test and a byte-code type
//! test: every `make-closure` instance of the source passes, anything else
//! takes the site's generic call. A hit calls [`neovm_jit_call_source_spec`]:
//! `neovm_jit_call_spec`'s fast path for a callee that IS the byte-code
//! object -- the source's armed leaf (its `leaf_slot`, the same epoch-checked
//! probe and heat bookkeeping the generic shim's native-to-native path
//! uses), the instance's own constant base, the backtrace frame recording
//! the called object, as GNU's `Bcall` does -- and answers
//! `STATUS_NEED_GENERIC` for anything its fast path does not take, which
//! runs the site's generic call. The site never deopts.
//!
//! The slot ([`SpecSlotKind::Source`]) holds the source's identity word in
//! `direct_consts` (immutable); under `NEOVM_JIT_DIRECT_CALL` the shim arms
//! `leaf`, `direct_entry` and `epoch` (the `leaf_slot_epoch` it was armed
//! under) with the source's leaf, and the site calls the leaf directly
//! (`direct_call`, [`SpecSlot::arm_source`]). The leaf walk that retires a
//! leaf (`unlink_spec_slots`) clears a source slot holding it; every other
//! retirement moves the epoch. The leaf holds the source's `RuntimeState`
//! (`CompiledLeaf::feedback_holds`), so the identity it bakes stays unique
//! while its code can run.

use super::dispatch::{FastRun, call_spec_finish, call_spec_framed_run};
use super::jit_layout::{BYTECODE_RUNTIME_WORD_OFFSET, VECLIKE_TYPE_TAG_OFFSET};
use super::lowering::{RtCtx, band_imm_p, icmp_imm_p};
use super::*;
use crate::emacs_core::eval::AttentionMask;
use crate::emacs_core::jit::feedback::{CallTarget, SiteShape};
use crate::tagged::header::VecLikeType;
use cranelift_codegen::isa::CallConv;

/// Calls a source slot's fast path took (tests and debug builds).
#[cfg(any(test, debug_assertions))]
pub(crate) static SOURCE_SLOT_FAST_CALLS: AtomicU64 = AtomicU64::new(0);

/// Times a source slot was (re-)armed with its source's leaf (tests and
/// debug builds).
#[cfg(any(test, debug_assertions))]
pub(crate) static SOURCE_SLOT_ARMINGS: AtomicU64 = AtomicU64::new(0);

/// Calls a source slot handed to the site's generic call (tests and debug
/// builds).
#[cfg(any(test, debug_assertions))]
pub(crate) static SOURCE_SLOT_DECLINED: AtomicU64 = AtomicU64::new(0);

/// Add a source site for every `Op::Call` of `ops` that has no speculation
/// yet, whose callee is not a constant and whose recorded target is one live
/// closure source (T1 and OSR compiles read only `Sources(1)`). Numbers the
/// new slots after `slots`' and records each source to be held by the leaf.
pub(crate) fn add_source_sites(
    ops: &[Op],
    sites: &mut HashMap<usize, SpecSite>,
    slots: &mut Vec<SpecSlot>,
) {
    for (pc, op) in ops.iter().enumerate() {
        if !matches!(op, Op::Call(_)) || sites.contains_key(&pc) {
            continue;
        }
        let Some((CallTarget::Sources(targets), SiteShape::Callee)) =
            super::call_feedback::recorded_target_at(pc)
        else {
            continue;
        };
        let [target] = targets.as_slice() else {
            continue;
        };
        let identity = crate::emacs_core::jit::feedback::identity_word_of(target);
        super::call_feedback::hold_for_leaf(target);
        sites.insert(
            pc,
            SpecSite {
                sym: 0,
                expected_bits: identity as u64,
                slot: slots.len(),
                kind: SpecCalleeKind::Source,
            },
        );
        slots.push(SpecSlot::source(identity as u64));
    }
}

/// `NEOVM_JIT_DIRECT_SHAPES=constant` (P1.1 2b): add a site for every
/// `Op::Call` of `ops` that has none whose callee slot provably holds a
/// constant byte-code object -- a `cl-flet` local, a `lambda` literal the
/// fuser left a call -- that a direct call can enter: a call as laid out
/// (exactly its required parameters, at most `MAX_REG_ARGS`) of a frameless
/// body that may take the register ABI, or an exact framed body under the
/// independent `framed` bit, in a caller that pays a direct
/// site back ([`super::DirectSitesMode`]). The site is a source site of the
/// object's own source ([`SpecCalleeKind::Constant`]): the object cannot be
/// redefined, so the slot's only validation is its leaf's epoch, and the
/// source shim is its slow path. Numbers the new slots after `slots'`.
pub(crate) fn add_constant_sites(
    ops: &[Op],
    constants: &[Value],
    leaders: &[usize],
    sites: &mut HashMap<usize, SpecSite>,
    slots: &mut Vec<SpecSlot>,
) {
    if !jit_direct_shapes().constant
        || jit_force_slow_spec()
        || (jit_direct_sites() == DirectSitesMode::Unbounded
            && !super::direct_call::unbounded_body())
    {
        return;
    }
    let entry = super::spec_tag_entry_states(ops, constants, leaders);
    let mut tags: Vec<Option<u16>> = Vec::new();
    for (pc, op) in ops.iter().enumerate() {
        if leaders.binary_search(&pc).is_ok() {
            tags.clear();
            if let Some(agreed) = entry.get(&pc) {
                tags.extend_from_slice(agreed);
            }
        }
        if let Op::Call(n) = op
            && !sites.contains_key(&pc)
        {
            let nargs = *n as usize;
            if tags.len() > nargs
                && let Some(cidx) = tags[tags.len() - 1 - nargs]
                && let Some(&callee) = constants.get(cidx as usize)
                && let Some(bc) = callee.get_bytecode_data()
                && constant_callee_takes_direct_calls(bc, nargs)
                && let Some(identity) = callee.bytecode_runtime_word().filter(|&w| w != 0)
            {
                sites.insert(
                    pc,
                    SpecSite {
                        sym: 0,
                        expected_bits: callee.bits() as u64,
                        slot: slots.len(),
                        kind: SpecCalleeKind::Constant,
                    },
                );
                slots.push(SpecSlot::source(identity as u64));
            }
        }
        super::spec_tag_transfer(op, constants, &mut tags);
    }
}

/// Whether a direct site can enter the constant byte-code callee `bc` with
/// a call of `nargs` arguments as laid out: exactly its required
/// parameters, of a frameless register body or, under `framed`, a framed
/// memory body the contained trampoline may enter.
fn constant_callee_takes_direct_calls(bc: &ByteCodeFunction, nargs: usize) -> bool {
    let ops = bc.executable_ops();
    bc.params.required.len() == nargs
        && bc.params.optional.is_empty()
        && bc.params.rest.is_none()
        && nargs <= super::reg_abi::MAX_REG_ARGS
        && (bc.jit_runtime().patched_prefix() == 0 || jit_spec_sources_on())
        && (jit_direct_shapes().framed
            || (!super::leaf::body_has_binds(ops) && !super::leaf::body_has_handlers(ops)))
}

impl SpecSlot {
    /// A source site's slot (see the module docs): the source's identity
    /// word (immutable), no leaf yet.
    pub(crate) fn source(identity: u64) -> Self {
        let slot = Self::at_epoch(0);
        slot.direct_consts.store(identity, Ordering::Relaxed);
        slot
    }

    /// A source slot's identity word.
    pub(crate) fn source_identity(&self) -> u64 {
        self.direct_consts.load(Ordering::Relaxed)
    }

    /// Arm a source slot with its source's leaf and its register entry or
    /// framed tag, valid under `epoch` (the `leaf_slot_epoch` the leaf was
    /// armed under): epoch/leaf first, entry published with Release last.
    /// Framed generated readers use an atomic Acquire-or-stronger load;
    /// the slot and leaf retain the existing mutator-owned cache lifetime.
    pub(crate) fn arm_source(&self, leaf: *const CompiledLeaf, entry: *const u8, epoch: u64) {
        // A direct entry never outlives the leaf it was armed for.
        self.direct_entry.store(0, Ordering::Relaxed);
        self.epoch.store(epoch, Ordering::Relaxed);
        self.leaf.store(leaf as usize as u64, Ordering::Relaxed);
        self.direct_entry
            .store(entry as usize as u64, Ordering::Release);
    }

    /// Drop a source slot's leaf: the entry first (the arming order,
    /// reversed); the identity stays.
    pub(crate) fn clear_source(&self) {
        self.direct_entry.store(0, Ordering::Relaxed);
        self.leaf.store(0, Ordering::Relaxed);
        self.epoch.store(0, Ordering::Relaxed);
    }
}

/// Emit a source site's guard on the callee `func_val`: a veclike, a
/// byte-code object, of the source whose identity word is `identity`. Leaves
/// the builder in the hit block and returns the (unsealed) miss block.
pub(crate) fn emit_source_guard(
    fb: &mut FunctionBuilder,
    func_val: ClifValue,
    identity: u64,
) -> Block {
    let flags = MemFlagsData::trusted();
    let miss = fb.create_block();
    let typed = fb.create_block();
    let sourced = fb.create_block();
    let hit = fb.create_block();
    let tag = band_imm_p(fb, func_val, TAG_MASK as i64);
    let veclike = icmp_imm_p(
        fb,
        IntCC::Equal,
        tag,
        crate::tagged::value::TAG_VECLIKE as i64,
    );
    fb.ins().brif(veclike, typed, &[], miss, &[]);
    fb.switch_to_block(typed);
    fb.seal_block(typed);
    let object = band_imm_p(fb, func_val, !(TAG_MASK as i64));
    let type_tag = fb
        .ins()
        .uload8(types::I64, flags, object, VECLIKE_TYPE_TAG_OFFSET as i32);
    let bytecode = icmp_imm_p(
        fb,
        IntCC::Equal,
        type_tag,
        i64::from(VecLikeType::ByteCode as u8),
    );
    fb.ins().brif(bytecode, sourced, &[], miss, &[]);
    fb.switch_to_block(sourced);
    fb.seal_block(sourced);
    let word = fb.ins().load(
        types::I64,
        flags,
        object,
        BYTECODE_RUNTIME_WORD_OFFSET as i32,
    );
    let same = icmp_imm_p(fb, IntCC::Equal, word, identity as i64);
    fb.ins().brif(same, hit, &[], miss, &[]);
    fb.switch_to_block(hit);
    fb.seal_block(hit);
    miss
}

/// Emit a source site's speculated call (after its guard hit): the shim
/// with the slot, the call buffers and the callee.
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_source_call(
    fb: &mut FunctionBuilder,
    rt: &RtCtx,
    slot_v: ClifValue,
    vmctx: ClifValue,
    func_val: ClifValue,
    args_addr: ClifValue,
    n_val: ClifValue,
    out_addr: ClifValue,
) -> cranelift_codegen::ir::Inst {
    let sig = fb.import_signature(source_call_signature(rt.refs.call_conv, rt.ptr_ty));
    let callee = fb.ins().iconst(
        rt.ptr_ty,
        neovm_jit_call_source_spec as *const () as usize as i64,
    );
    fb.ins().call_indirect(
        sig,
        callee,
        &[vmctx, func_val, slot_v, args_addr, n_val, out_addr],
    )
}

fn source_call_signature(call_conv: CallConv, ptr_ty: types::Type) -> Signature {
    let mut sig = Signature::new(call_conv);
    for ty in [ptr_ty, types::I64, types::I64, ptr_ty, types::I64, ptr_ty] {
        sig.params.push(AbiParam::new(ty));
    }
    sig.returns.push(AbiParam::new(types::I64));
    sig
}

/// A source site's speculated call (see the module docs). The site's guard
/// proved `callee_bits` a byte-code object of the slot's source. Answers
/// `STATUS_OK` / `STATUS_SIGNAL` like `neovm_jit_call`, or
/// `STATUS_NEED_GENERIC` -- before any effect -- for a call its fast path
/// does not take: no armed leaf, an argument count the leaf does not take
/// as laid out, the re-tier crossing, attention (quit, a pending signal),
/// `debug-on-next-call`, the depth limit, or `NEOVM_JIT_FORCE_SLOW_SPEC`.
///
/// Like `neovm_jit_call_spec`'s fast path it runs outside a containment
/// frame: loads, compares, the bounded backtrace push, the raw entry of a
/// handler-free body and the balanced pop cannot unwind; a framed body and
/// every non-OK exit take the contained halves `call_spec_framed_run` and
/// `call_spec_finish`, the spec shim's own.
///
/// SAFETY: the call-shim contract of `neovm_jit_call`; `slot` points into
/// the executing leaf's spec slots.
#[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI shim: raw ptrs per documented SAFETY contract; only ever called from generated code.
pub(crate) extern "C" fn neovm_jit_call_source_spec(
    ctx: *mut u8,
    callee_bits: i64,
    slot: i64,
    args_ptr: *const i64,
    nargs: i64,
    out: *mut i64,
) -> i64 {
    // SAFETY: the seam's dormant Context (the shim contract).
    let ctx_ref = unsafe { &mut *(ctx as *mut Context) };
    let callee = Value::from_bits(callee_bits as usize);
    let nargs = nargs as usize;
    debug_assert!({
        // SAFETY: the executing leaf's slot.
        let slot = unsafe { &*(slot as *const SpecSlot) };
        callee.bytecode_runtime_word() == Some(slot.source_identity() as usize)
    });
    // The attention word carries `NEOVM_JIT_FORCE_SLOW_SPEC` too
    // (`AttentionMask::SPEC_CALL`).
    if !ctx_ref.attention_clear(AttentionMask::SPEC_CALL)
        || ctx_ref.debug_on_next_call_is_armed()
        || ctx_ref.depth >= ctx_ref.max_depth
    {
        return source_slot_declined();
    }
    // The guard proved a byte-code object (the type check is debug-only);
    // a pdump stub, whose identity word 0 never passes the guard anyway,
    // would be materialized here.
    let bc = callee.bytecode_data_typechecked_by_caller();
    let rt = bc.jit_runtime();
    // The source's armed leaf, as `cache::armed_leaf_for_native_call` finds
    // it, deciding everything before the heat moves: a declined call runs
    // the generic shim, whose own probe must see the same heat.
    let epoch = crate::emacs_core::jit::cache::leaf_slot_epoch();
    let Some(ptr) = rt.armed_leaf_slot(epoch) else {
        return source_slot_declined();
    };
    #[cfg(test)]
    if rt.force_interpret_for_test() {
        return source_slot_declined();
    }
    // SAFETY: armed under the current `leaf_slot_epoch` (every retire and
    // clear bumps it, and retired leaves stay allocated).
    let leaf = unsafe { &*ptr };
    if !leaf.is_pure_passthrough(nargs)
        || crate::emacs_core::jit::retier_heat()
            .is_some_and(|at| rt.peek_heat().saturating_add(1) == at)
    {
        return source_slot_declined();
    }
    rt.bump_heat();
    #[cfg(any(test, debug_assertions))]
    SOURCE_SLOT_FAST_CALLS.fetch_add(1, Ordering::Relaxed);
    // Remember this leaf (and its epoch) in the slot, with its direct
    // entry under `NEOVM_JIT_DIRECT_CALL`, so the next call of the source
    // enters it from the site. Two compares while the slot holds it.
    {
        // SAFETY: the executing leaf's slot.
        let slot = unsafe { &*(slot as *const SpecSlot) };
        if slot.leaf_ptr() != ptr || slot.epoch.load(Ordering::Relaxed) != epoch {
            arm_source_direct_entry(slot, leaf, nargs, epoch);
        }
    }
    let consts = bc.jit_constant_base();
    let bt_count = ctx_ref.specpdl.len();
    // SAFETY: `args_ptr` addresses `nargs` valid tagged words (the caller's
    // call-args slot). The frame records the called object, as GNU's
    // `Bcall` does for a funcall of a closure, and roots it for the run.
    unsafe { ctx_ref.push_backtrace_frame_from_native_args(callee, args_ptr, nargs) };
    ctx_ref.depth += 1;
    let run = if leaf.direct_call_eligible() {
        let mut bits: i64 = 0;
        // SAFETY: `args_ptr` addresses `arity` live words (a pure
        // pass-through) of a direct-eligible leaf; `ctx` is the dormant
        // seam Context. The leaf's entry shape picks its raw entry.
        let status = if leaf.entry_shape == super::leaf::EntryShape::RawRegister {
            let ret = unsafe { leaf.entry_call_raw_register(ctx, consts, args_ptr) };
            bits = ret.value;
            ret.status
        } else {
            unsafe { leaf.entry_call_raw_memory(ctx, consts, args_ptr, &mut bits) }
        };
        if status == STATUS_OK {
            if ctx_ref.pop_native_backtrace_frame(bt_count) {
                ctx_ref.depth -= 1;
                // SAFETY: `out` is the generated code's result stack slot.
                unsafe { *out = bits };
                return STATUS_OK;
            }
            FastRun::Done(Value::from_bits(bits as usize))
        } else {
            FastRun::Raw(status)
        }
    } else {
        // A framed entry is always a memory entry; the key is the bare
        // constant base (no flags).
        match call_spec_framed_run(ctx, leaf, consts as usize as u64, args_ptr) {
            NativeRun::Ok(bits) => {
                if ctx_ref.pop_native_backtrace_frame(bt_count) {
                    ctx_ref.depth -= 1;
                    // SAFETY: as above.
                    unsafe { *out = bits as i64 };
                    return STATUS_OK;
                }
                FastRun::Done(Value::from_bits(bits))
            }
            // A contained panic: its residue is the caller's healing
            // points' (see `neovm_jit_call_spec`).
            NativeRun::Signal if shim_panic_pending() => {
                // SAFETY: `args_ptr` is the caller's live call-args slot.
                unsafe { ctx_ref.detach_native_frames_into(args_ptr) };
                return STATUS_SIGNAL;
            }
            other => FastRun::Framed(other),
        }
    };
    call_spec_finish(ctx, callee, leaf, args_ptr, nargs, out, bt_count, run)
}

/// Remember `leaf`, the source's current armed leaf, in a source site's
/// slot, with its direct entry under `NEOVM_JIT_DIRECT_CALL` when the site
/// may enter it (its register ABI takes exactly `nargs` words, it is
/// frameless, and the lean frame layout was probed). Under the framed shape
/// knob an exact required-only framed JIT memory leaf publishes the framed
/// tag instead; no AOT sidecar is admitted. Out of line: once per leaf the
/// source runs.
#[cold]
#[inline(never)]
fn arm_source_direct_entry(slot: &SpecSlot, leaf: &CompiledLeaf, nargs: usize, epoch: u64) {
    #[cfg(any(test, debug_assertions))]
    SOURCE_SLOT_ARMINGS.fetch_add(1, Ordering::Relaxed);
    let eligible = jit_direct_call_on()
        && leaf.abi
            == (LeafAbi::Register {
                arity: nargs.min(u8::MAX as usize) as u8,
            })
        && leaf.arity == nargs
        && !leaf.has_rest
        && leaf.direct_call_eligible()
        && super::jit_layout::backtrace_layout().is_some();
    let framed = jit_direct_shapes().framed
        && super::spec_slot::framed_direct_eligible(leaf, nargs)
        && super::jit_layout::backtrace_layout().is_some();
    // The slot remembers the leaf either way, so the shim's compare holds
    // until the leaf changes; the entry only when the site may enter it.
    slot.arm_source(
        leaf,
        if eligible {
            leaf.entry
        } else if framed {
            super::spec_slot::DirectEntryTag::Framed as u64 as usize as *const u8
        } else {
            std::ptr::null()
        },
        epoch,
    );
}

/// The fast path declined: the site runs its generic call.
#[inline(always)]
fn source_slot_declined() -> i64 {
    #[cfg(any(test, debug_assertions))]
    SOURCE_SLOT_DECLINED.fetch_add(1, Ordering::Relaxed);
    STATUS_NEED_GENERIC
}
