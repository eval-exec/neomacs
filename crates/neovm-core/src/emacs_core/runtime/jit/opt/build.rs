//! Bytecode operand-stack variables translated to global, pruned SSA.
//!
//! The mutator snapshots the baseline CFG while its constant pool is rooted.
//! This builder only reads immutable Rust data and opaque constant bits: it
//! never dereferences a Lisp object or consults a mutator-local cache. Separate
//! compiler workers can therefore build independent functions concurrently.
//! A completed single predecessor supplies its stack without parameters. Lazy
//! block parameters stand in for other stack-slot reads, whose inputs sealing
//! resolves before recursively removing trivial loop-invariant parameters.

use std::collections::{HashMap, HashSet, VecDeque};

use super::ir::*;
use super::mem::{AliasClass, Effects};
use super::types::TypeSet;
use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::jit::compile::{Cfg, CompileError, simple_effect};
use crate::emacs_core::jit::inline::FusedBody;

/// Borrowed, immutable compiler input. `cfg` must be analyzed with the source
/// function's GNU offset map before capturing constants as `ValueBits`.
/// The rooted source owner outlives the build; the worker does not dereference
/// its bits. A fused body's replay side tables remain read-only throughout.
pub(crate) struct BuildInput<'a> {
    pub(crate) ops: &'a [Op],
    pub(crate) constants: &'a [ValueBits],
    pub(crate) cfg: &'a Cfg,
    pub(crate) params: ParamShape,
    pub(crate) dynamic_prefix: usize,
    pub(crate) fused: Option<&'a FusedBody>,
    pub(crate) osr: Option<OsrEntry>,
}

/// Build the passes-off IR. Opcode bodies keep the baseline emitter contract;
/// the operand stack, source positions, effects and control edges are explicit.
pub(crate) fn build(input: BuildInput<'_>) -> Result<Func, CompileError> {
    Builder::new(input)?.finish()
}

/// Compile-local SSA state, never shared with a running mutator. Pending
/// parameters are resolved before the function escapes the builder.
struct Builder<'a> {
    input: BuildInput<'a>,
    func: Func,
    block_for: HashMap<usize, Block>,
    leaders: Vec<usize>,
    incoming: Vec<Vec<Vec<Value>>>,
    replay: HashMap<usize, FrameId>,
    source_blocks: usize,
    cfg_predecessors: Vec<usize>,
    cfg_successors: Vec<usize>,
}

impl<'a> Builder<'a> {
    fn new(input: BuildInput<'a>) -> Result<Self, CompileError> {
        if input.ops.is_empty() {
            return Err(CompileError::NoReturn);
        }
        if input.dynamic_prefix > input.constants.len() {
            return Err(CompileError::BadOperand);
        }
        // Exceptional edge values require the handler's push-time stack, not
        // the current stack. Refuse until that dispatch contract is represented.
        for (pc, op) in input.ops.iter().enumerate() {
            let leader = input
                .cfg
                .leaders
                .partition_point(|&leader| leader <= pc)
                .checked_sub(1)
                .and_then(|index| input.cfg.leaders.get(index))
                .copied()
                .ok_or(CompileError::BadOperand)?;
            if !input.cfg.entry_depth.contains_key(&leader) {
                continue;
            }
            let reason = match op {
                Op::PushConditionCase(_) => Some("opt-build:PushConditionCase"),
                Op::PushConditionCaseRaw(_) => Some("opt-build:PushConditionCaseRaw"),
                Op::PushCatch(_) => Some("opt-build:PushCatch"),
                Op::PopHandler => Some("opt-build:PopHandler"),
                Op::Throw => Some("opt-build:Throw"),
                _ => None,
            };
            if let Some(reason) = reason {
                return Err(CompileError::UnsupportedOp(reason));
            }
        }
        if input.osr.is_some() && input.fused.is_some() {
            return Err(CompileError::UnsupportedOp("opt-build:fused-osr"));
        }
        if let Some(fused) = input.fused {
            if !fused.is_v2()
                || fused.ops != input.ops
                || fused.caller_of_fused.len() != input.ops.len()
                || fused.region_of.len() != input.ops.len()
            {
                return Err(CompileError::UnsupportedOp("opt-build:fused-side-tables"));
            }
        }
        let root_pc = input.osr.as_ref().map_or(0, |osr| osr.entry_pc as usize);
        let root_depth = input
            .cfg
            .entry_depth
            .get(&root_pc)
            .copied()
            .ok_or(CompileError::UnsupportedOp("opt-build:unreachable-entry"))?;
        if let Some(osr) = &input.osr {
            if osr.depth != root_depth {
                return Err(CompileError::UnsupportedOp("opt-build:osr-depth"));
            }
            if input.cfg.entry_binds.get(&root_pc).copied().unwrap_or(0) != 0 {
                return Err(CompileError::UnsupportedOp("opt-build:osr-binds"));
            }
        } else if root_depth != input.params.native_arity() {
            return Err(CompileError::BadOperand);
        }
        let reachable = reachable_leaders(&input, root_pc)?;
        let leaders: Vec<_> = input
            .cfg
            .leaders
            .iter()
            .copied()
            .filter(|pc| reachable.contains(pc))
            .collect();
        let mut func = Func::new(
            input.constants.to_vec().into_boxed_slice(),
            input.params,
            input.dynamic_prefix,
        );
        func.census.dead_leaders = input.cfg.leaders.len() - leaders.len();
        func.source_states = vec![None; input.ops.len()];
        let mut block_for = HashMap::new();
        for &pc in &leaders {
            let block = Block(func.blocks.len() as u32);
            block_for.insert(pc, block);
            func.blocks.push(BlockData {
                params: Vec::new(),
                insts: Vec::new(),
                term: Term::Unreachable,
                preds: Vec::new(),
                pc: pc as u32,
                loop_header: None,
                cold: false,
            });
            func.entry_stacks.push(Box::default());
        }
        let source_blocks = func.blocks.len();
        // Both normal and OSR loads feed a synthetic entry. Instruction zero
        // can itself be a loop header: putting Arg loads there would reset a
        // modified argument on its back edge and lose the true entry phi.
        let entry = Block(func.blocks.len() as u32);
        func.blocks.push(BlockData {
            params: Vec::new(),
            insts: Vec::new(),
            term: Term::Unreachable,
            preds: Vec::new(),
            pc: root_pc as u32,
            loop_header: None,
            cold: false,
        });
        func.entry_stacks.push(Box::default());
        func.entry = entry;
        let mut pred_sets = vec![HashSet::new(); source_blocks];
        let mut cfg_successors = vec![0; source_blocks + 1];
        for &leader in &leaders {
            let source = block_for[&leader];
            let successors = successors(&input, leader)?;
            cfg_successors[source.index()] = successors.iter().collect::<HashSet<_>>().len();
            for target in successors {
                pred_sets[block_for[&target].index()].insert(source);
            }
        }
        pred_sets[block_for[&root_pc].index()].insert(entry);
        let cfg_predecessors = pred_sets.iter().map(HashSet::len).collect();
        cfg_successors[entry.index()] = 1;
        let mut this = Self {
            incoming: vec![Vec::new(); func.blocks.len()],
            input,
            func,
            block_for,
            leaders,
            replay: HashMap::new(),
            source_blocks,
            cfg_predecessors,
            cfg_successors,
        };
        let mut initial = Vec::with_capacity(root_depth);
        for slot in 0..root_depth {
            let slot = u16::try_from(slot).map_err(|_| CompileError::BadOperand)?;
            let op = if this.input.osr.is_some() {
                Opcode::OsrSlot(slot)
            } else {
                Opcode::Arg(slot)
            };
            let ty = if this.input.osr.is_none()
                && this.input.params.has_rest
                && slot as usize + 1 == root_depth
            {
                TypeSet::LIST
            } else {
                TypeSet::TOP
            };
            initial.push(
                this.emit(
                    entry,
                    root_pc,
                    op,
                    Vec::new(),
                    Some(ty),
                    Effects::PURE,
                    AliasClass::None,
                    None,
                )
                .expect("entry value"),
            );
        }
        this.func.entry_stacks[entry.index()] = initial.clone().into_boxed_slice();
        if let Some(mut osr) = this.input.osr.clone() {
            osr.header = this.block_for[&root_pc];
            this.func.osr = Some(osr);
        }
        let edge = this.edge(entry, root_pc, root_pc, &initial, None, false)?;
        this.func.blocks[entry.index()].term = Term::Jump(edge);
        Ok(this)
    }

    fn finish(mut self) -> Result<Func, CompileError> {
        let leaders = self.leaders.clone();
        for leader in leaders {
            self.build_block(leader)?;
        }
        self.seal_and_prune()?;
        self.func.census.blocks = self.func.blocks.len();
        self.func.census.insts = self.func.insts.len();
        self.func.census.phis = self.func.blocks.iter().map(|b| b.params.len()).sum();
        self.func.census.frames = self.func.frames.len();
        Ok(self.func)
    }

    fn frame(&mut self, frame: FrameState) -> FrameId {
        self.func.intern_frame(frame)
    }

    fn source_frame(
        &mut self,
        pc: usize,
        stack: &[Value],
        binds: usize,
    ) -> Result<FrameId, CompileError> {
        if let Some(fused) = self.input.fused {
            if let Some(region_index) = fused.region_of[pc] {
                let region = &fused.regions[region_index];
                if pc == region.start {
                    let expected = region
                        .frame_base
                        .checked_add(region.nargs)
                        .ok_or(CompileError::BadOperand)?;
                    if stack.len() != expected {
                        return Err(CompileError::UnsupportedOp("opt-build:inline-frame-depth"));
                    }
                    let frame = self.frame(FrameState {
                        pc: region.call_site_pc as u32,
                        stack: stack.into(),
                        handlers: 0,
                        binds: u16::try_from(binds).map_err(|_| CompileError::BadOperand)?,
                        parent: None,
                        site: Some(InlineSiteId(region_index as u32)),
                    });
                    self.replay.insert(region_index, frame);
                }
                // The allocation-free region replays the original caller's
                // call. No callee frame exists yet; adding a parent would
                // incorrectly materialize a frame in this replay contract.
                return self
                    .replay
                    .get(&region_index)
                    .copied()
                    .ok_or(CompileError::UnsupportedOp("opt-build:inline-region-entry"));
            }
        }
        let runtime_pc = self
            .input
            .fused
            .map_or(Some(pc), |fused| fused.caller_pc(pc))
            .ok_or(CompileError::BadOperand)?;
        Ok(self.frame(FrameState {
            pc: runtime_pc as u32,
            stack: stack.into(),
            handlers: 0,
            binds: u16::try_from(binds).map_err(|_| CompileError::BadOperand)?,
            parent: None,
            site: None,
        }))
    }

    #[allow(clippy::too_many_arguments)]
    fn emit(
        &mut self,
        block: Block,
        pc: usize,
        op: Opcode,
        args: Vec<Value>,
        ty: Option<TypeSet>,
        eff: Effects,
        mem: AliasClass,
        frame: Option<FrameId>,
    ) -> Option<Value> {
        let inst = Inst(self.func.insts.len() as u32);
        let result = ty.map(|ty| {
            let value = Value(self.func.values.len() as u32);
            self.func.values.push(ValueData {
                ty,
                rep: if matches!(op, Opcode::IsNonNil) {
                    Rep::Bool
                } else {
                    Rep::Tagged
                },
                def: ValueDef::Inst(inst),
            });
            value
        });
        self.func.insts.push(InstData {
            op,
            args,
            result,
            eff,
            mem,
            frame,
            pc: pc as u32,
        });
        self.func.blocks[block.index()].insts.push(inst);
        result
    }

    fn entry_stack(&mut self, block: Block) -> Vec<Value> {
        if !self.func.entry_stacks[block.index()].is_empty() || block == self.func.entry {
            return self.func.entry_stacks[block.index()].to_vec();
        }
        let depth = self.input.cfg.entry_depth[&(self.func.blocks[block.index()].pc as usize)];
        // Once the unique source predecessor has been emitted, its outgoing
        // stack dominates this block. A split edge may supply refined values;
        // those dominate through the edge block as well. Do not forward an
        // unsealed join or loop header: a later predecessor can change a slot.
        // Duplicate Switch edges from one predecessor must also agree exactly.
        if self.cfg_predecessors[block.index()] == 1
            && let Some(first) = self.incoming[block.index()].first()
            && first.len() == depth
            && self.incoming[block.index()]
                .iter()
                .all(|incoming| incoming == first)
        {
            let stack = first.clone();
            self.func.entry_stacks[block.index()] = stack.clone().into_boxed_slice();
            return stack;
        }
        let mut stack = Vec::with_capacity(depth);
        // Each read makes one pending phi. The complete GNU stack is live at
        // deopt/GC boundaries, even when only a subset feeds the next opcode.
        for slot in 0..depth {
            let value = Value(self.func.values.len() as u32);
            self.func.values.push(ValueData {
                ty: TypeSet::TOP,
                rep: Rep::Tagged,
                def: ValueDef::Param {
                    block,
                    index: slot as u32,
                },
            });
            self.func.blocks[block.index()].params.push(value);
            stack.push(value);
        }
        self.func.entry_stacks[block.index()] = stack.clone().into_boxed_slice();
        stack
    }

    fn build_block(&mut self, leader: usize) -> Result<(), CompileError> {
        let block = self.block_for[&leader];
        let mut stack = self.entry_stack(block);
        let mut binds = self
            .input
            .cfg
            .entry_binds
            .get(&leader)
            .copied()
            .unwrap_or(0);
        let end = block_end(self.input.cfg, leader, self.input.ops.len());
        let mut terminated = false;
        for pc in leader..end {
            let op = self.input.ops[pc].clone();
            let pre = stack.clone();
            let frame = self.source_frame(pc, &pre, binds)?;
            if let Some(fused) = self.input.fused {
                if let Some(region) = fused.region_of[pc]
                    && pc == fused.regions[region].start
                {
                    self.emit(
                        block,
                        pc,
                        Opcode::InlineEntry(region as u32),
                        pre.clone(),
                        None,
                        Effects::MAY_DEOPT,
                        AliasClass::None,
                        Some(frame),
                    );
                }
            }
            match &op {
                Op::Return => {
                    self.func.blocks[block.index()].term =
                        Term::Return(*stack.last().ok_or(CompileError::StackUnderflow)?);
                    terminated = true;
                }
                Op::Goto(target) => {
                    let edge = self.edge(block, pc, *target as usize, &stack, None, true)?;
                    self.func.blocks[block.index()].term = Term::Jump(edge);
                    terminated = true;
                }
                Op::GotoIfNil(target)
                | Op::GotoIfNotNil(target)
                | Op::GotoIfNilElsePop(target)
                | Op::GotoIfNotNilElsePop(target) => {
                    let cond = *stack.last().ok_or(CompileError::StackUnderflow)?;
                    let else_pop =
                        matches!(op, Op::GotoIfNilElsePop(_) | Op::GotoIfNotNilElsePop(_));
                    let taken_stack = if else_pop {
                        stack.clone()
                    } else {
                        stack.pop();
                        stack.clone()
                    };
                    if else_pop {
                        stack.pop();
                    }
                    let flag = self
                        .emit(
                            block,
                            pc,
                            Opcode::IsNonNil,
                            vec![cond],
                            Some(TypeSet::BOOLEAN),
                            Effects::PURE,
                            AliasClass::None,
                            None,
                        )
                        .expect("branch flag");
                    let taken_non_nil =
                        matches!(op, Op::GotoIfNotNil(_) | Op::GotoIfNotNilElsePop(_));
                    let taken = self.edge(
                        block,
                        pc,
                        *target as usize,
                        &taken_stack,
                        Some((cond, taken_non_nil)),
                        true,
                    )?;
                    let fall = self.edge(
                        block,
                        pc,
                        pc + 1,
                        &stack,
                        Some((cond, !taken_non_nil)),
                        false,
                    )?;
                    let (if_true, if_false) = if taken_non_nil {
                        (taken, fall)
                    } else {
                        (fall, taken)
                    };
                    self.func.blocks[block.index()].term = Term::Branch {
                        flag,
                        if_true,
                        if_false,
                    };
                    terminated = true;
                }
                Op::Switch => {
                    let table = stack.pop().ok_or(CompileError::StackUnderflow)?;
                    let value = stack.pop().ok_or(CompileError::StackUnderflow)?;
                    let targets = self
                        .input
                        .cfg
                        .switch_targets
                        .get(&pc)
                        .ok_or(CompileError::BadOperand)?
                        .clone();
                    let mut cases = Vec::with_capacity(targets.len());
                    for (key, target) in targets {
                        cases.push(SwitchCase {
                            key,
                            edge: self.edge(block, pc, target, &stack, None, true)?,
                        });
                    }
                    let default = self.edge(block, pc, pc + 1, &stack, None, false)?;
                    self.func.blocks[block.index()].term = Term::Switch {
                        value,
                        table,
                        cases,
                        default,
                    };
                    terminated = true;
                }
                _ => self.value_op(block, pc, &op, &mut stack, frame)?,
            }
            self.func.source_states[pc] = Some(SourceState {
                pre: pre.into_boxed_slice(),
                post: stack.clone().into_boxed_slice(),
                frame,
                block,
            });
            match op {
                Op::VarBind(_)
                | Op::SaveCurrentBuffer
                | Op::SaveExcursion
                | Op::SaveRestriction
                | Op::UnwindProtectPop => binds += 1,
                Op::Unbind(n) => {
                    binds = binds
                        .checked_sub(n as usize)
                        .ok_or(CompileError::UnsupportedOp("opt-build:unbalanced-unbind"))?
                }
                _ => {}
            }
            if terminated {
                break;
            }
        }
        if !terminated {
            if end == self.input.ops.len() {
                return Err(CompileError::NoReturn);
            }
            let edge = self.edge(block, end - 1, end, &stack, None, false)?;
            self.func.blocks[block.index()].term = Term::Jump(edge);
        }
        Ok(())
    }

    fn value_op(
        &mut self,
        block: Block,
        pc: usize,
        op: &Op,
        stack: &mut Vec<Value>,
        frame: FrameId,
    ) -> Result<(), CompileError> {
        match op {
            Op::Constant(index) => {
                let bits = *self
                    .input
                    .constants
                    .get(*index as usize)
                    .ok_or(CompileError::BadOperand)?;
                let (opcode, ty, eff, mem) = if (*index as usize) < self.input.dynamic_prefix {
                    (
                        Opcode::EnvConst(*index as u32),
                        TypeSet::TOP,
                        Effects::READ_HEAP,
                        AliasClass::Unknown,
                    )
                } else {
                    (
                        Opcode::Const(*index as u32),
                        TypeSet::for_constant(bits),
                        Effects::PURE,
                        AliasClass::None,
                    )
                };
                stack.push(
                    self.emit(
                        block,
                        pc,
                        opcode,
                        Vec::new(),
                        Some(ty),
                        eff,
                        mem,
                        Some(frame),
                    )
                    .expect("constant result"),
                );
            }
            Op::Nil | Op::True => {
                let bits = ValueBits(if matches!(op, Op::True) { 8 } else { 0 });
                let index = if let Some(index) = self
                    .func
                    .consts
                    .iter()
                    .enumerate()
                    .skip(self.input.dynamic_prefix)
                    .find_map(|(index, &v)| (v == bits).then_some(index))
                {
                    index
                } else {
                    let mut pool = self.func.consts.to_vec();
                    let index = pool.len();
                    pool.push(bits);
                    self.func.consts = pool.into_boxed_slice();
                    index
                };
                stack.push(
                    self.emit(
                        block,
                        pc,
                        Opcode::Const(index as u32),
                        Vec::new(),
                        Some(TypeSet::for_constant(bits)),
                        Effects::PURE,
                        AliasClass::None,
                        Some(frame),
                    )
                    .expect("constant result"),
                );
            }
            Op::Pop => {
                stack.pop().ok_or(CompileError::StackUnderflow)?;
            }
            Op::Dup => {
                stack.push(*stack.last().ok_or(CompileError::StackUnderflow)?);
            }
            Op::StackRef(depth) => {
                let index = stack
                    .len()
                    .checked_sub(*depth as usize + 1)
                    .ok_or(CompileError::StackUnderflow)?;
                stack.push(stack[index]);
            }
            Op::StackSet(depth) => {
                let value = stack.pop().ok_or(CompileError::StackUnderflow)?;
                if *depth != 0 {
                    let index = stack
                        .len()
                        .checked_sub(*depth as usize)
                        .ok_or(CompileError::StackUnderflow)?;
                    stack[index] = value;
                }
            }
            Op::DiscardN(raw) => {
                let count = (*raw & 0x7f) as usize;
                let next = stack
                    .len()
                    .checked_sub(count)
                    .ok_or(CompileError::StackUnderflow)?;
                if raw & 0x80 != 0 && count > 0 {
                    let top = *stack.last().ok_or(CompileError::StackUnderflow)?;
                    *stack
                        .get_mut(next.checked_sub(1).ok_or(CompileError::StackUnderflow)?)
                        .ok_or(CompileError::StackUnderflow)? = top;
                }
                stack.truncate(next);
            }
            _ => {
                let (needs, delta) = simple_effect(op)?;
                let base = stack
                    .len()
                    .checked_sub(needs)
                    .ok_or(CompileError::StackUnderflow)?;
                let mut args = stack[base..].to_vec();
                let outputs = i64::try_from(needs).map_err(|_| CompileError::BadOperand)? + delta;
                if !(0..=1).contains(&outputs) {
                    return Err(CompileError::BadOperand);
                }
                let guard_ty = match op {
                    Op::Car | Op::Cdr => Some(TypeSet::LIST),
                    Op::Setcar | Op::Setcdr => Some(TypeSet::CONS),
                    _ => None,
                };
                if let Some(ty) = guard_ty {
                    args[0] = self
                        .emit(
                            block,
                            pc,
                            Opcode::CheckType(ty),
                            vec![args[0]],
                            Some(ty),
                            Effects::MAY_DEOPT,
                            AliasClass::None,
                            Some(frame),
                        )
                        .expect("checked value");
                }
                let (effects, alias) = op_effects(op);
                let ty = (outputs == 1).then(|| result_type(op));
                let result = self.emit(
                    block,
                    pc,
                    Opcode::Opaque(op.clone()),
                    args,
                    ty,
                    effects,
                    alias,
                    Some(frame),
                );
                stack.truncate(base);
                if let Some(value) = result {
                    stack.push(value);
                }
            }
        }
        Ok(())
    }

    fn edge(
        &mut self,
        source: Block,
        pc: usize,
        target_pc: usize,
        stack: &[Value],
        condition: Option<(Value, bool)>,
        branch: bool,
    ) -> Result<Edge, CompileError> {
        let target = *self
            .block_for
            .get(&target_pc)
            .ok_or(CompileError::BadOperand)?;
        let backward = branch && target_pc <= pc;
        let refinements = condition
            .map_or_else(Vec::new, |(value, non_nil)| {
                self.refinements(value, non_nil)
            })
            .into_iter()
            .filter(|(value, _)| stack.contains(value))
            .collect::<Vec<_>>();
        let critical = self
            .cfg_successors
            .get(source.index())
            .copied()
            .unwrap_or(0)
            > 1
            && self.cfg_predecessors[target.index()] > 1;
        let split = backward || critical || !refinements.is_empty();
        if backward && self.func.blocks[target.index()].loop_header.is_none() {
            self.func.blocks[target.index()].loop_header = Some(LoopId(target.0));
        }
        if !split {
            self.add_incoming(source, target, stack);
            return Ok(Edge {
                target,
                args: stack.to_vec(),
            });
        }
        let edge_block = Block(self.func.blocks.len() as u32);
        self.func.blocks.push(BlockData {
            params: Vec::new(),
            insts: Vec::new(),
            term: Term::Unreachable,
            preds: vec![source],
            pc: pc as u32,
            loop_header: None,
            cold: false,
        });
        self.func.entry_stacks.push(stack.into());
        // Only source blocks need provisional-parameter inputs. Edge blocks
        // consume dominating values directly and never participate in sealing.
        self.incoming.push(Vec::new());
        self.func.census.critical_edges += usize::from(critical);
        if backward {
            let frame = self.source_frame(
                target_pc,
                stack,
                self.input
                    .cfg
                    .entry_binds
                    .get(&target_pc)
                    .copied()
                    .unwrap_or(0),
            )?;
            self.emit(
                edge_block,
                pc,
                Opcode::Poll,
                Vec::new(),
                None,
                Effects::UNKNOWN,
                AliasClass::Unknown,
                Some(frame),
            );
        }
        let mut args = stack.to_vec();
        for (original, ty) in refinements {
            if args.contains(&original) {
                let refined = self
                    .emit(
                        edge_block,
                        pc,
                        Opcode::Refine(ty),
                        vec![original],
                        Some(ty),
                        Effects::PURE,
                        AliasClass::None,
                        None,
                    )
                    .expect("refined result");
                for value in &mut args {
                    if *value == original {
                        *value = refined;
                    }
                }
                self.func.census.refinements += 1;
            }
        }
        self.add_incoming(edge_block, target, &args);
        self.func.blocks[edge_block.index()].term = Term::Jump(Edge { target, args });
        // Edge-only values dominate this block through its unique predecessor;
        // it needs no redundant full-stack block parameters.
        Ok(Edge {
            target: edge_block,
            args: Vec::new(),
        })
    }

    fn add_incoming(&mut self, source: Block, target: Block, stack: &[Value]) {
        self.func.blocks[target.index()].preds.push(source);
        self.incoming[target.index()].push(stack.to_vec());
    }

    fn refinements(&self, condition: Value, non_nil: bool) -> Vec<(Value, TypeSet)> {
        let mut out = vec![(
            condition,
            if non_nil {
                TypeSet::TOP.without(TypeSet::NIL)
            } else {
                TypeSet::NIL
            },
        )];
        if let ValueDef::Inst(inst) = self.func.values[condition.index()].def {
            let inst = &self.func.insts[inst.index()];
            if let Opcode::Opaque(op) = &inst.op {
                let exact_type = match op {
                    Op::Consp => Some(TypeSet::CONS),
                    Op::Listp => Some(TypeSet::LIST),
                    Op::Stringp => Some(TypeSet::STRING),
                    Op::Null | Op::Not => Some(TypeSet::NIL),
                    _ => None,
                };
                if let Some(ty) = exact_type {
                    out.push((
                        inst.args[0],
                        if non_nil {
                            ty
                        } else {
                            TypeSet::TOP.without(ty)
                        },
                    ));
                }
                if non_nil && matches!(op, Op::Car | Op::Cdr | Op::CarSafe | Op::CdrSafe) {
                    out.push((inst.args[0], TypeSet::CONS));
                }
                if matches!(op, Op::Eq) {
                    for (value, other) in
                        [(inst.args[0], inst.args[1]), (inst.args[1], inst.args[0])]
                    {
                        if self.func.values[other.index()].ty.is_subset(TypeSet::NIL)
                            && self.func.values[value.index()]
                                .ty
                                .permits_symbol_identity_folding()
                        {
                            out.push((
                                value,
                                if non_nil {
                                    TypeSet::NIL
                                } else {
                                    TypeSet::TOP.without(TypeSet::NIL)
                                },
                            ));
                        }
                    }
                }
            }
        }
        out
    }

    fn seal_and_prune(&mut self) -> Result<(), CompileError> {
        let mut aliases: Vec<Value> = (0..self.func.values.len())
            .map(|n| Value(n as u32))
            .collect();
        loop {
            let mut changed = false;
            for block in 0..self.source_blocks {
                for (slot, &phi) in self.func.blocks[block].params.iter().enumerate() {
                    if canonical(&aliases, phi) != phi {
                        continue;
                    }
                    let mut other = None;
                    let mut representative = None;
                    let mut same_representation = true;
                    let mut different = false;
                    for incoming in &self.incoming[block] {
                        let incoming = canonical(
                            &aliases,
                            *incoming.get(slot).ok_or(CompileError::BadOperand)?,
                        );
                        let value = runtime_identity(&self.func, &aliases, incoming);
                        if value == phi {
                            continue;
                        }
                        if let Some(old) = representative {
                            same_representation &= old == incoming;
                        } else {
                            representative = Some(incoming);
                        }
                        match other {
                            None => other = Some(value),
                            Some(old) if old == value => {}
                            Some(_) => {
                                different = true;
                                break;
                            }
                        }
                    }
                    if !different && let Some(other) = other {
                        aliases[phi.index()] = if same_representation {
                            representative.expect("non-self incoming value")
                        } else {
                            other
                        };
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        let mut retained = Vec::with_capacity(self.func.blocks.len());
        for (block_index, block) in self.func.blocks.iter_mut().enumerate() {
            let keep: Vec<_> = block
                .params
                .iter()
                .enumerate()
                .filter_map(|(slot, &value)| (canonical(&aliases, value) == value).then_some(slot))
                .collect();
            block.params = keep.iter().map(|&slot| block.params[slot]).collect();
            for (index, &value) in block.params.iter().enumerate() {
                self.func.values[value.index()].def = ValueDef::Param {
                    block: Block(block_index as u32),
                    index: index as u32,
                };
            }
            retained.push(keep);
        }
        for (index, &to) in aliases.iter().enumerate() {
            if to != Value(index as u32) {
                self.func.values[index].def = ValueDef::Alias(canonical(&aliases, to));
            }
        }
        for inst in &mut self.func.insts {
            for arg in &mut inst.args {
                *arg = canonical(&aliases, *arg);
            }
        }
        for block in &mut self.func.blocks {
            map_term(&mut block.term, &aliases, &retained);
        }
        let mut frames_changed = false;
        for frame in &mut self.func.frames {
            for value in &mut frame.stack {
                let canonical = canonical(&aliases, *value);
                frames_changed |= canonical != *value;
                *value = canonical;
            }
        }
        for stack in &mut self.func.entry_stacks {
            for value in stack {
                *value = canonical(&aliases, *value);
            }
        }
        for state in self.func.source_states.iter_mut().flatten() {
            for value in &mut state.pre {
                *value = canonical(&aliases, *value);
            }
            for value in &mut state.post {
                *value = canonical(&aliases, *value);
            }
        }
        if frames_changed {
            self.reintern_frames();
        }
        // Join parameter types only after aliases and edge arities are final.
        // A finite kind-set lattice converges for loops without range widening.
        for block in &self.func.blocks {
            for &value in &block.params {
                self.func.values[value.index()].ty = TypeSet::BOTTOM;
            }
        }
        for _ in 0..self.func.values.len().saturating_add(1) {
            let mut changed = false;
            for block in 0..self.source_blocks {
                for (slot, &value) in self.func.blocks[block].params.iter().enumerate() {
                    let original_slot = retained[block][slot];
                    let mut ty = TypeSet::BOTTOM;
                    for edge in &self.incoming[block] {
                        let incoming = canonical(&aliases, edge[original_slot]);
                        ty = ty.join(self.func.values[incoming.index()].ty);
                    }
                    if ty != self.func.values[value.index()].ty {
                        self.func.values[value.index()].ty = ty;
                        changed = true;
                    }
                }
            }
            for inst in &self.func.insts {
                if let (Opcode::CheckType(ty) | Opcode::Refine(ty), Some(result)) =
                    (&inst.op, inst.result)
                {
                    let input_ty = self.func.values[inst.args[0].index()].ty;
                    let narrowed = input_ty.meet(*ty);
                    if self.func.values[result.index()].ty != narrowed {
                        self.func.values[result.index()].ty = narrowed;
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }
        Ok(())
    }

    fn reintern_frames(&mut self) {
        let old = std::mem::take(&mut self.func.frames);
        self.func.rebuild_frame_intern();
        let mut remap = Vec::with_capacity(old.len());
        for mut frame in old {
            frame.parent = frame.parent.map(|parent| remap[parent.index()]);
            remap.push(self.func.intern_frame(frame));
        }
        for inst in &mut self.func.insts {
            inst.frame = inst.frame.map(|frame| remap[frame.index()]);
        }
        for block in &mut self.func.blocks {
            if let Term::Deopt(frame) = &mut block.term {
                *frame = remap[frame.index()];
            }
        }
        for state in self.func.source_states.iter_mut().flatten() {
            state.frame = remap[state.frame.index()];
        }
    }
}

fn canonical(aliases: &[Value], mut value: Value) -> Value {
    while aliases[value.index()] != value {
        value = aliases[value.index()];
    }
    value
}

// A π refinement changes the compiler's knowledge, never the Lisp identity.
// Treat a refine(phi) back edge as the phi itself when removing invariant
// stack-slot phis; retain the π value on its edge for type facts.
fn runtime_identity(func: &Func, aliases: &[Value], mut value: Value) -> Value {
    loop {
        value = canonical(aliases, value);
        let ValueDef::Inst(inst) = func.values[value.index()].def else {
            return value;
        };
        let inst = &func.insts[inst.index()];
        if matches!(inst.op, Opcode::Refine(_) | Opcode::CheckType(_)) {
            value = inst.args[0];
        } else {
            return value;
        }
    }
}

fn map_term(term: &mut Term, aliases: &[Value], retained: &[Vec<usize>]) {
    let map_edge = |edge: &mut Edge| {
        edge.args = retained[edge.target.index()]
            .iter()
            .map(|&slot| canonical(aliases, edge.args[slot]))
            .collect();
    };
    match term {
        Term::Jump(edge) => map_edge(edge),
        Term::Branch {
            flag,
            if_true,
            if_false,
        } => {
            *flag = canonical(aliases, *flag);
            map_edge(if_true);
            map_edge(if_false);
        }
        Term::Switch {
            value,
            table,
            cases,
            default,
        } => {
            *value = canonical(aliases, *value);
            *table = canonical(aliases, *table);
            for case in cases {
                map_edge(&mut case.edge);
            }
            map_edge(default);
        }
        Term::Return(value) => *value = canonical(aliases, *value),
        Term::Deopt(_) | Term::Unreachable => {}
    }
}

fn block_end(cfg: &Cfg, leader: usize, ops_len: usize) -> usize {
    let next = cfg.leaders.partition_point(|&pc| pc <= leader);
    cfg.leaders.get(next).copied().unwrap_or(ops_len)
}

fn reachable_leaders(input: &BuildInput<'_>, root: usize) -> Result<HashSet<usize>, CompileError> {
    let mut seen = HashSet::new();
    let mut work = VecDeque::from([root]);
    while let Some(leader) = work.pop_front() {
        if !seen.insert(leader) {
            continue;
        }
        if !input.cfg.entry_depth.contains_key(&leader) {
            return Err(CompileError::BadOperand);
        }
        work.extend(successors(input, leader)?);
    }
    Ok(seen)
}

fn successors(input: &BuildInput<'_>, leader: usize) -> Result<Vec<usize>, CompileError> {
    let end = block_end(input.cfg, leader, input.ops.len());
    let op = input
        .ops
        .get(end.checked_sub(1).ok_or(CompileError::NoReturn)?)
        .ok_or(CompileError::BadOperand)?;
    Ok(match op {
        Op::Return => Vec::new(),
        Op::Goto(target) => vec![*target as usize],
        Op::GotoIfNil(target)
        | Op::GotoIfNotNil(target)
        | Op::GotoIfNilElsePop(target)
        | Op::GotoIfNotNilElsePop(target) => vec![*target as usize, end],
        Op::Switch => input
            .cfg
            .switch_targets
            .get(&(end - 1))
            .ok_or(CompileError::BadOperand)?
            .iter()
            .map(|&(_, target)| target)
            .chain([end])
            .collect(),
        _ if end < input.ops.len() => vec![end],
        _ => return Err(CompileError::NoReturn),
    })
}

fn result_type(op: &Op) -> TypeSet {
    match op {
        Op::Null
        | Op::Not
        | Op::Consp
        | Op::Stringp
        | Op::Listp
        | Op::Symbolp
        | Op::Integerp
        | Op::Numberp
        | Op::Eq
        | Op::Equal
        | Op::Eqlsign
        | Op::Lss
        | Op::Gtr
        | Op::Leq
        | Op::Geq
        | Op::StringEqual
        | Op::StringLessp => TypeSet::BOOLEAN,
        Op::Cons => TypeSet::CONS,
        Op::List(0) => TypeSet::NIL,
        Op::List(_) => TypeSet::CONS,
        Op::Add
        | Op::Sub
        | Op::Mul
        | Op::Div
        | Op::Rem
        | Op::Add1
        | Op::Sub1
        | Op::Negate
        | Op::Max
        | Op::Min => TypeSet::NUMBER,
        _ => TypeSet::TOP,
    }
}

fn op_effects(op: &Op) -> (Effects, AliasClass) {
    match op {
        Op::Car | Op::CarSafe => (
            Effects::READ_HEAP.with(Effects::MAY_DEOPT),
            AliasClass::ConsCar,
        ),
        Op::Cdr | Op::CdrSafe => (
            Effects::READ_HEAP.with(Effects::MAY_DEOPT),
            AliasClass::ConsCdr,
        ),
        Op::Setcar => (
            Effects::WRITE_HEAP.with(Effects::MAY_DEOPT),
            AliasClass::ConsCar,
        ),
        Op::Setcdr => (
            Effects::WRITE_HEAP.with(Effects::MAY_DEOPT),
            AliasClass::ConsCdr,
        ),
        Op::Aref => (
            Effects::READ_HEAP.with(Effects::MAY_DEOPT),
            AliasClass::VecElem,
        ),
        Op::Aset => (
            Effects::WRITE_HEAP.with(Effects::MAY_DEOPT),
            AliasClass::VecElem,
        ),
        Op::Cons | Op::List(_) => (Effects::ALLOCATES, AliasClass::None),
        Op::VarRef(_) => (
            Effects::READ_BINDINGS
                .with(Effects::READ_BUFFER)
                .with(Effects::MAY_DEOPT),
            AliasClass::Bindings,
        ),
        Op::VarSet(_) | Op::VarBind(_) => (
            Effects::WRITE_BINDINGS
                .with(Effects::MAY_REENTER)
                .with(Effects::MAY_GC)
                .with(Effects::MAY_SIGNAL)
                .with(Effects::MAY_DEOPT),
            AliasClass::Bindings,
        ),
        Op::Unbind(_) | Op::UnwindProtectPop => (Effects::UNKNOWN, AliasClass::Unknown),
        Op::SaveCurrentBuffer | Op::SaveExcursion | Op::SaveRestriction => (
            Effects::WRITE_BINDINGS
                .with(Effects::READ_BUFFER)
                .with(Effects::MAY_DEOPT),
            AliasClass::Bindings,
        ),
        Op::Add
        | Op::Sub
        | Op::Mul
        | Op::Div
        | Op::Rem
        | Op::Add1
        | Op::Sub1
        | Op::Negate
        | Op::Max
        | Op::Min => (
            Effects::ALLOCATES.with(Effects::MAY_DEOPT),
            AliasClass::None,
        ),
        Op::Eqlsign | Op::Lss | Op::Gtr | Op::Leq | Op::Geq => {
            (Effects::MAY_DEOPT, AliasClass::None)
        }
        Op::Null | Op::Not | Op::Consp | Op::Stringp | Op::Listp => {
            (Effects::PURE, AliasClass::None)
        }
        // GNU's symbol-with-position predicates depend on dynamic runtime
        // state; no exact type-test refinement is generated for these.
        Op::Eq | Op::Symbolp | Op::Integerp | Op::Numberp => (
            Effects::READ_HEAP.with(Effects::READ_BINDINGS),
            AliasClass::Unknown,
        ),
        _ => (Effects::UNKNOWN, AliasClass::Unknown),
    }
}

#[cfg(test)]
#[path = "tests/build_ssa.rs"]
mod tests;
