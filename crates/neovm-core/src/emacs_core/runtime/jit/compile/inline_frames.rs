//! T1 static inline frames and precise cold chain spills.
//!
//! Admission is restricted to forward, observation-free bytecode. A virtual
//! activation cannot run Lisp or collect: guards leave through a chain, and
//! cons writes leave through that same chain when a barrier is required.
//! Threading: all SSA snapshots and the mutable table builder belong to one
//! compilation. Published metadata is immutable; no running Lisp state is
//! cached here or shared between mutators.

use std::cell::RefCell;
use std::rc::Rc;

use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{InstBuilder, MemFlagsData, Value as ClifValue, types};
use cranelift_frontend::{FunctionBuilder, Variable};

use super::CompileError;
use super::chain_framestate::{PhysicalFrameState, RegionFrameState, chain_framestate};
use super::jit_layout;
use super::lowering::{self, DeoptCells, PendingDeopt, RegionDeopt, RtCtx, SlotRep};
use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::jit::inline::{FusedBody, RegionKind};
use crate::emacs_core::jit::reopt::DeoptCause;
use crate::emacs_core::jit::vframe::{
    BtState, DeoptChain, Link, RelocIdx, SpillRange, VFrameKind, VFrameMeta,
};
use crate::emacs_core::value::Value;

/// Keep builtin dependencies of admitted mapping sites in the physical leaf.
/// Threading: this mutates compiler-owned metadata before publication; it
/// reads only immutable fused annotations and caches no mutator Lisp state.
pub(crate) fn retain_hof_dependencies(leaf: &mut super::CompiledLeaf, fused: Option<&FusedBody>) {
    let Some(side) = fused.and_then(|body| body.v2.as_ref()) else {
        return;
    };
    let mut deps = leaf.inline_deps.to_vec();
    for site in side.hof_at.values() {
        let name = match site.kind {
            crate::emacs_core::jit::inline::HofKind::Mapc => "mapc",
            crate::emacs_core::jit::inline::HofKind::Mapcar => "mapcar",
        };
        let sym = crate::emacs_core::intern::intern(name);
        if !deps.contains(&sym) {
            deps.push(sym);
        }
    }
    leaf.inline_deps = deps.into_boxed_slice();
}

/// Compile-local stores attached to a cold block. Addresses name the eventual
/// mutator-owned leaf's stable cells, never a cross-mutator runtime cache.
#[derive(Clone)]
pub(crate) struct InlineDeoptWrite {
    chain_addr: i64,
    reason_addr: i64,
    site: Option<u32>,
    cause: Option<DeoptCause>,
}

impl InlineDeoptWrite {
    pub(crate) fn emit(&self, fb: &mut FunctionBuilder) {
        let flags = MemFlagsData::trusted();
        if let Some(site) = self.site {
            let ptr = fb.ins().iconst(types::I64, self.chain_addr);
            let value = fb.ins().iconst(types::I64, i64::from(site));
            fb.ins().store(flags, value, ptr, 0);
        }
        if let Some(cause) = self.cause {
            let ptr = fb.ins().iconst(types::I64, self.reason_addr);
            let value = fb.ins().iconst(types::I64, cause.reason_code());
            fb.ins().store(flags, value, ptr, 0);
        }
    }
}

/// The original pre-call snapshot and source of one flat virtual region.
/// Threading: compile-local handles only; `Rc` explicitly prevents crossing
/// workers. Its table is frozen before the native leaf is published.
#[derive(Clone)]
pub(crate) struct ChainRegion {
    fused: Rc<FusedBody>,
    region: usize,
    function: RelocIdx,
    physical_binds: u16,
    table: Rc<RefCell<Vec<DeoptChain>>>,
    write: InlineDeoptWrite,
    hof: Option<HofChainState>,
}

/// SSA state of a suspended mapping activation. Compile-local only; values
/// are spilled/tagged by the same cold emitter as ordinary inline frames.
#[derive(Clone)]
pub(crate) struct HofChainState {
    pub(crate) kind: crate::emacs_core::jit::vframe::HofKind,
    pub(crate) call_pc: usize,
    pub(crate) state: Vec<ClifValue>,
    pub(crate) callback_entered: bool,
}

/// One compilation's virtual-frame state; never retained by a native call.
pub(crate) struct Frames {
    fused: Option<Rc<FusedBody>>,
    table: Rc<RefCell<Vec<DeoptChain>>>,
    write: InlineDeoptWrite,
}

impl Frames {
    pub(crate) fn new(meta: &DeoptCells) -> Self {
        Self {
            fused: crate::emacs_core::jit::inline::active_fused().filter(|f| f.is_v2()),
            table: Rc::new(RefCell::new(Vec::new())),
            write: InlineDeoptWrite {
                chain_addr: std::ptr::from_ref(&meta.chain) as i64,
                reason_addr: std::ptr::from_ref(&meta.reason) as i64,
                site: None,
                cause: None,
            },
        }
    }

    pub(crate) fn finish(self) -> Vec<DeoptChain> {
        lowering::set_active_region(None);
        self.table.take()
    }

    /// Hook before each fused op. `true` means the op was emitted here.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn before_op(
        &self,
        fb: &mut FunctionBuilder,
        pc: usize,
        op: &Op,
        rt: Option<&RtCtx>,
        physical_binds: usize,
        handlers: usize,
        stack: &mut Vec<ClifValue>,
        reps: &mut Vec<SlotRep>,
        pending: &mut Vec<PendingDeopt>,
        relocs: &std::collections::HashMap<usize, u32>,
    ) -> Result<bool, CompileError> {
        let Some(fused) = &self.fused else {
            return Ok(false);
        };
        let Some(region_id) = fused.region_of[pc] else {
            lowering::set_active_region(None);
            return Ok(false);
        };
        let region = &fused.regions[region_id];
        let side = fused.v2.as_ref().expect("v2");
        let rt = rt.ok_or(CompileError::UnsupportedOp("inline-vmctx"))?;
        if pc == region.start {
            if handlers != 0 || region.parent.is_some() {
                return Err(CompileError::UnsupportedOp("inline-chain-handlers"));
            }
            lowering::box_all_flonums(fb, Some(rt), stack, reps);
            lowering::set_active_region(None);
            if region.frame_base == 0 || stack.len() != region.frame_base + region.nargs {
                return Err(CompileError::UnsupportedOp("inline-region-depth"));
            }
            // Pure constant callers were admitted at the physical entry and
            // after each service poll. Their independently proven identity
            // needs no per-call test or loop-carried validity flag.
            let physical = rt
                .inline_entry_cache
                .as_ref()
                .is_some_and(|cache| cache.elides_constant_entry(region_id));
            debug_assert!(!physical || matches!(side.region_kind[region_id], RegionKind::Constant));
            if !physical {
                let cached = rt
                    .inline_entry_cache
                    .as_ref()
                    .and_then(|cache| cache.region(region_id));
                let checked_body = if matches!(side.region_kind[region_id], RegionKind::Constant) {
                    cached.map(|valid| (valid, super::inline_entry_cache::begin(fb, valid)))
                } else {
                    None
                };
                let identity = lowering::deopt_site(fb, pc, handlers, stack, reps, pending);
                self.mark(pending, DeoptCause::InlineIdentity);
                let slot = region.frame_base - 1;
                let actual = if reps[slot] == SlotRep::RawFixnum {
                    lowering::retag_fixnum(fb, stack[slot])
                } else {
                    stack[slot]
                };
                match side.region_kind[region_id] {
                    RegionKind::Constant => {
                        let expected = fb.ins().iconst(types::I64, region.callee_bits as i64);
                        let same = fb.ins().icmp(IntCC::Equal, actual, expected);
                        lowering::emit_guard(fb, identity, same);
                    }
                    RegionKind::Closure { prefix, .. } => {
                        let code = Value::from_bits(region.callee_bits as usize)
                            .get_bytecode_data()
                            .ok_or(CompileError::BadOperand)?;
                        let word = jit_layout::runtime_identity_word(&code.jit_runtime());
                        let miss = super::source_slots::emit_source_guard(fb, actual, word as u64);
                        // The source guard has no calls or side effects; its miss
                        // transfers to the original call with unchanged operands.
                        let hit = fb.current_block().expect("source hit");
                        fb.switch_to_block(miss);
                        fb.seal_block(miss);
                        fb.ins().jump(identity, &[]);
                        fb.switch_to_block(hit);
                        super::source_slots::emit_closure_prefix_guard(
                            fb,
                            &code.jit_runtime(),
                            prefix,
                            identity,
                        );
                    }
                }
                if let Some((valid, body)) = checked_body {
                    self.entry_protocol(fb, pc, rt, stack, reps, pending, handlers);
                    super::inline_entry_cache::finish(fb, valid, body);
                } else if let Some(valid) = cached {
                    self.cached_protocol(fb, pc, rt, stack, reps, pending, handlers, valid)?;
                } else {
                    self.entry_protocol(fb, pc, rt, stack, reps, pending, handlers);
                }
            }
            let function = *relocs
                .get(&(region.callee_bits as usize))
                .ok_or(CompileError::UnsupportedOp("inline-callee-reloc"))?;
            lowering::set_active_region(Some(RegionDeopt {
                call_site_pc: region.call_site_pc,
                stack: stack.clone(),
                reps: reps.clone(),
                chain: Some(ChainRegion {
                    fused: fused.clone(),
                    region: region_id,
                    function: RelocIdx(function),
                    physical_binds: u16::try_from(physical_binds)
                        .map_err(|_| CompileError::BadOperand)?,
                    table: self.table.clone(),
                    write: self.write.clone(),
                    hof: None,
                }),
            }));
        }
        if let (RegionKind::Closure { prefix, const_base }, Op::Constant(index)) =
            (side.region_kind[region_id], op)
            && (*index as usize) >= const_base
            && (*index as usize) < const_base + prefix
        {
            let object =
                lowering::band_imm_p(fb, stack[region.frame_base - 1], !(super::TAG_MASK as i64));
            let (ptr_offset, _) = jit_layout::bytecode_constants_offsets()
                .ok_or(CompileError::UnsupportedOp("closure-constant-layout"))?;
            let base = fb.ins().load(
                types::I64,
                MemFlagsData::trusted(),
                object,
                ptr_offset as i32,
            );
            let value = fb.ins().load(
                types::I64,
                MemFlagsData::trusted(),
                base,
                ((*index as usize - const_base) * 8) as i32,
            );
            stack.push(value);
            reps.push(SlotRep::Tagged);
            return Ok(true);
        }
        if matches!(op, Op::Setcar | Op::Setcdr) {
            lowering::materialize_model_stack(fb, Some(rt), stack, reps);
            let deopt = lowering::deopt_site(fb, pc, handlers, stack, reps, pending);
            let value = stack.pop().ok_or(CompileError::StackUnderflow)?;
            let cell = stack.pop().ok_or(CompileError::StackUnderflow)?;
            reps.truncate(stack.len());
            let result = fb.declare_var(types::I64);
            let merge = fb.create_block();
            super::heap_inline::emit_inline_cons_store(
                fb,
                rt,
                cell,
                value,
                matches!(op, Op::Setcdr),
                deopt,
                result,
                merge,
            );
            fb.switch_to_block(merge);
            fb.seal_block(merge);
            stack.push(fb.use_var(result));
            reps.push(SlotRep::Tagged);
            return Ok(true);
        }
        Ok(false)
    }

    pub(crate) fn mark(&self, pending: &mut [PendingDeopt], cause: DeoptCause) {
        let guard = pending.last_mut().expect("guard site");
        let write = guard.inline.get_or_insert_with(|| self.write.clone());
        write.cause = Some(cause);
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn entry_protocol(
        &self,
        fb: &mut FunctionBuilder,
        pc: usize,
        rt: &RtCtx,
        stack: &[ClifValue],
        reps: &[SlotRep],
        pending: &mut Vec<PendingDeopt>,
        handlers: usize,
    ) {
        use crate::emacs_core::eval::AttentionMask;
        use crate::emacs_core::forward::LISP_BOOL_FWD_VALUE_OFFSET;
        let flags = MemFlagsData::trusted();
        let ctx = fb.use_var(rt.vmctx_var);
        let deopt = lowering::deopt_site(fb, pc, handlers, stack, reps, pending);
        self.mark(pending, DeoptCause::InlineAttention);
        let attention = fb
            .ins()
            .uload32(flags, ctx, jit_layout::CONTEXT_ATTENTION_OFFSET as i32);
        let attention =
            lowering::band_imm_p(fb, attention, i64::from(AttentionMask::INLINE_ENTRY.bits()));
        let async_ptr = fb.ins().iconst(
            types::I64,
            crate::emacs_core::eval::ASYNC_ATTENTION.addr() as i64,
        );
        let asynchronous = fb.ins().uload32(flags, async_ptr, 0);
        let debug_ptr = fb.ins().load(
            types::I64,
            flags,
            ctx,
            (jit_layout::CONTEXT_OBARRAY_OFFSET + jit_layout::OBARRAY_DEBUG_ON_NEXT_CALL_FWD_OFFSET)
                as i32,
        );
        let debug = fb.ins().uload8(
            types::I64,
            flags,
            debug_ptr,
            LISP_BOOL_FWD_VALUE_OFFSET as i32,
        );
        let attention = fb.ins().bor(attention, asynchronous);
        let attention = fb.ins().bor(attention, debug);
        let clear = lowering::icmp_imm_p(fb, IntCC::Equal, attention, 0);
        lowering::emit_guard(fb, deopt, clear);
        let deopt = lowering::deopt_site(fb, pc, handlers, stack, reps, pending);
        self.mark(pending, DeoptCause::DepthLimit);
        let depth = fb.ins().load(
            types::I64,
            flags,
            ctx,
            jit_layout::CONTEXT_DEPTH_OFFSET as i32,
        );
        let max = fb.ins().load(
            types::I64,
            flags,
            ctx,
            jit_layout::CONTEXT_MAX_DEPTH_OFFSET as i32,
        );
        let within = fb.ins().icmp(IntCC::UnsignedLessThan, depth, max);
        lowering::emit_guard(fb, deopt, within);
    }

    /// A retained first/post-poll entry test at its original call boundary.
    /// Flags are per activation. Forced-deopt emission keeps the full original
    /// ordered guard sequence instead of passing through the cache branch.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn cached_protocol(
        &self,
        fb: &mut FunctionBuilder,
        pc: usize,
        rt: &RtCtx,
        stack: &[ClifValue],
        reps: &[SlotRep],
        pending: &mut Vec<PendingDeopt>,
        handlers: usize,
        valid: Variable,
    ) -> Result<(), CompileError> {
        if super::jit_force_deopt() {
            self.entry_protocol(fb, pc, rt, stack, reps, pending, handlers);
            return Ok(());
        }
        let body = super::inline_entry_cache::begin(fb, valid);
        self.entry_protocol(fb, pc, rt, stack, reps, pending, handlers);
        super::inline_entry_cache::finish(fb, valid, body);
        Ok(())
    }

    pub(crate) fn enter_hof(&self, physical: RegionDeopt, state: HofChainState, binds: usize) {
        let mut physical = physical;
        physical.chain = Some(ChainRegion {
            fused: self.fused.as_ref().expect("HOF has v2 annotations").clone(),
            region: 0,
            function: RelocIdx(0),
            physical_binds: binds as u16,
            table: self.table.clone(),
            write: self.write.clone(),
            hof: Some(state),
        });
        lowering::set_active_region(Some(physical));
    }
}

/// Reuse the shared planner and representation-aware spill emitter. This
/// cold compile-time operation changes no off-path CLIF.
pub(crate) fn plan_deopt(pc: usize, pending: &mut PendingDeopt) {
    let Some(region) = pending.region.as_ref() else {
        return;
    };
    let Some(state) = &region.chain else { return };
    if let Some(hof) = &state.hof {
        let parent_len = region.stack.len();
        let state_len = hof.state.len();
        let frames = vec![
            VFrameMeta {
                kind: VFrameKind::PhysicalBytecode,
                pc: hof.call_pc as u32,
                stack: SpillRange {
                    start: 0,
                    len: parent_len as u32,
                },
                binds: state.physical_binds,
                handlers: 0,
                bt: BtState::Physical,
            },
            VFrameMeta {
                kind: VFrameKind::HofMapping {
                    kind: hof.kind,
                    callback_entered: hof.callback_entered,
                },
                pc: 0,
                stack: SpillRange {
                    start: parent_len as u32,
                    len: state_len as u32,
                },
                binds: 0,
                handlers: 0,
                bt: BtState::Virtual,
            },
            VFrameMeta {
                kind: VFrameKind::ClosureBytecode {
                    function_slot: parent_len as u32,
                    link: Link::HofCallback,
                },
                pc: pc as u32,
                stack: SpillRange {
                    start: (parent_len + state_len) as u32,
                    len: pending.stack.len() as u32,
                },
                binds: 0,
                handlers: 0,
                bt: BtState::Virtual,
            },
        ];
        let mut table = state.table.borrow_mut();
        let site = table.len() as u32;
        table.push(DeoptChain {
            frames: frames.into_boxed_slice(),
        });
        let mut stack = region.stack.clone();
        stack.extend_from_slice(&hof.state);
        stack.extend_from_slice(&pending.stack);
        let mut reps = region.reps.clone();
        reps.extend(std::iter::repeat_n(SlotRep::Tagged, state_len));
        reps.extend_from_slice(&pending.reps);
        pending.stack = stack;
        pending.reps = reps;
        pending.pc = hof.call_pc;
        let mut write = state.write.clone();
        write.site = Some(site);
        pending.inline = Some(write);
        return;
    }
    let meta = &state.fused.regions[state.region];
    let snapshot = RegionFrameState {
        region: state.region,
        function: state.function,
        link: Link::Bcall {
            nargs: meta.nargs as u16,
        },
        bt: BtState::Virtual,
        binds: 0,
        handlers: 0,
        pre_call: region
            .stack
            .iter()
            .copied()
            .zip(region.reps.iter().copied())
            .collect(),
    };
    let mut plan = chain_framestate(
        &state.fused,
        pc,
        &pending.stack,
        &pending.reps,
        PhysicalFrameState {
            binds: state.physical_binds,
            handlers: 0,
        },
        &[snapshot],
    )
    .expect("validated flat virtual region");
    if matches!(
        state.fused.v2.as_ref().expect("v2").region_kind[state.region],
        RegionKind::Closure { .. }
    ) {
        plan.chain.frames[1].kind = VFrameKind::ClosureBytecode {
            function_slot: (meta.frame_base - 1) as u32,
            link: Link::Bcall {
                nargs: meta.nargs as u16,
            },
        };
    }
    let mut table = state.table.borrow_mut();
    let site = u32::try_from(table.len()).expect("inline chain budget");
    table.push(plan.chain);
    pending.stack = plan.spill.iter().map(|(v, _)| *v).collect();
    pending.reps = plan.spill.iter().map(|(_, rep)| *rep).collect();
    let mut write = state.write.clone();
    write.site = Some(site);
    pending.inline = Some(write);
}
