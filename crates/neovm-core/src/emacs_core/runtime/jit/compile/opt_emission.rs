//! Authoritative opt SSA emission through the shared semantic emitters.
//! Threading: this state belongs to one compilation. Opaque constant bits are
//! read on the source-owning front; no Lisp state or SSA facts are cached across
//! compilations or mutators.

use super::*;
use crate::emacs_core::jit::opt::{ir, types::TypeSet};

#[path = "opt_pass_emission.rs"]
mod pass_emission;

#[path = "opt_arith_emission.rs"]
mod arith_emission;

#[path = "opt_array_emission.rs"]
mod array_emission;

type RuntimeValue = (ClifValue, SlotRep);
type LocalValues = HashMap<ir::Value, RuntimeValue>;

/// Global variables carry only values crossing IR blocks; local float payloads
/// remain unboxed until an actual edge or observable operation needs them.
/// Threading: compiler-owned variables and immutable representation metadata.
pub(super) struct SsaValues {
    vars: Vec<Variable>,
    raw: Vec<bool>,
    booleans: Vec<bool>,
    cross: Vec<bool>,
    live_after: Vec<Vec<ir::Value>>,
}

/// The shared leaf scaffold, borrowed for authoritative IR emission. Threading:
/// all mutable state belongs to this compiler invocation and its FunctionBuilder.
pub(super) struct EmitContext<'a, 'b> {
    pub fb: &'a mut FunctionBuilder<'b>,
    pub func: &'a ir::Func,
    pub values: &'a SsaValues,
    pub blocks: &'a [Block],
    pub cfg: &'a Cfg,
    pub seed_vars: &'a [Variable],
    pub variable_raw: &'a [bool],
    pub constants: &'a [Value],
    pub rt: Option<&'a RtCtx>,
    pub spec_sites: &'a HashMap<usize, SpecSite>,
    pub spec_slots: &'a [SpecSlot],
    pub deopt_refs: DeoptRefs,
    pub signal_exit: &'a mut Option<Block>,
    pub backedge_counter: Option<StackSlot>,
    pub out_var: Variable,
    pub out_present: bool,
    pub abi: LeafAbi,
    pub reloc_base: Option<ClifValue>,
    pub reloc_index: &'a HashMap<usize, u32>,
    pub aot: bool,
    pub spec_slot_base: Option<ClifValue>,
    pub spec_expected_base: Option<ClifValue>,
    pub dynamic_prefix: usize,
    pub consts_base: Option<ClifValue>,
    pub ops: &'a [Op],
    pub verified_arrays:
        Option<&'a crate::emacs_core::jit::opt::passes::array_reads::VerifiedArrayReads>,
    pub known_fixnum_slots: &'a HashMap<usize, Vec<bool>>,
}

impl SsaValues {
    pub(super) fn new(fb: &mut FunctionBuilder, func: &ir::Func, raw_slots: &[bool]) -> Self {
        let booleans: Vec<_> = func
            .values
            .iter()
            .map(|v| jit_opt_passes().bool_rep && v.rep == ir::Rep::Bool)
            .collect();
        let vars = booleans
            .iter()
            .map(|&boolean| fb.declare_var(if boolean { types::I8 } else { types::I64 }))
            .collect();
        let raw = if jit_opt_passes().reps || jit_opt_passes().range {
            // Selected representations are properties of SSA identities. A
            // source stack slot can hold different representations at distinct
            // program points; the legacy slot hints cannot select these webs.
            func.values
                .iter()
                .map(|v| v.rep == ir::Rep::RawInt)
                .collect()
        } else {
            let mut raw = vec![true; func.values.len()];
            let mut seen = vec![false; func.values.len()];
            for stack in &func.entry_stacks {
                for (slot, &value) in stack.iter().enumerate() {
                    let value = func.resolve(value).expect("verified opt value").index();
                    seen[value] = true;
                    raw[value] &= raw_slots.get(slot).copied().unwrap_or(false);
                }
            }
            for (raw, seen) in raw.iter_mut().zip(seen) {
                *raw &= seen;
            }
            raw
        };
        let mut definitions = vec![None; func.values.len()];
        for (index, block) in func.blocks.iter().enumerate() {
            let owner = ir::Block(index as u32);
            for &param in &block.params {
                definitions[param.index()] = Some(owner);
            }
            for &inst in &block.insts {
                if let Some(value) = func.insts[inst.index()].result {
                    definitions[value.index()] = Some(owner);
                }
            }
        }
        let mut cross = vec![false; func.values.len()];
        let mut mark = |owner: ir::Block, value: ir::Value| {
            let value = func.resolve(value).expect("verified opt value");
            cross[value.index()] |= definitions[value.index()] != Some(owner);
        };
        for (index, block) in func.blocks.iter().enumerate() {
            let owner = ir::Block(index as u32);
            for &value in func
                .entry_stacks
                .get(index)
                .map_or(&[][..], |stack| &stack[..])
            {
                mark(owner, value);
            }
            for &inst in &block.insts {
                let inst = &func.insts[inst.index()];
                for &value in &inst.args {
                    mark(owner, value);
                }
                let mut frame = inst.frame;
                while let Some(id) = frame {
                    let state = &func.frames[id.index()];
                    for &value in &state.stack {
                        mark(owner, value);
                    }
                    frame = state.parent;
                }
            }
            for edge in block.term.edges() {
                for &value in &edge.args {
                    mark(owner, value);
                }
            }
            match &block.term {
                ir::Term::Return(value) | ir::Term::Branch { flag: value, .. } => {
                    mark(owner, *value)
                }
                ir::Term::Switch { value, table, .. } => {
                    mark(owner, *value);
                    mark(owner, *table);
                }
                ir::Term::Deopt(frame) => {
                    for &value in &func.frames[frame.index()].stack {
                        mark(owner, value);
                    }
                }
                _ => {}
            }
        }
        let live_after = live_after_instructions(func);
        Self {
            vars,
            raw,
            booleans,
            cross,
            live_after,
        }
    }

    fn read(
        &self,
        fb: &mut FunctionBuilder,
        func: &ir::Func,
        local: &mut LocalValues,
        value: ir::Value,
    ) -> RuntimeValue {
        let value = func.resolve(value).expect("verified opt value");
        *local.entry(value).or_insert_with(|| {
            (
                fb.use_var(self.vars[value.index()]),
                self.representation(value),
            )
        })
    }

    fn representation(&self, value: ir::Value) -> SlotRep {
        if self.booleans[value.index()] {
            SlotRep::Bool
        } else {
            SlotRep::raw_if(self.raw[value.index()])
        }
    }

    fn write(&self, fb: &mut FunctionBuilder, value: ir::Value, runtime: RuntimeValue) {
        let (word, rep) = runtime;
        let word = if self.booleans[value.index()] {
            debug_assert_eq!(rep, SlotRep::Bool);
            debug_assert_eq!(fb.func.dfg.value_type(word), types::I8);
            word
        } else if fb.func.dfg.value_type(word) == types::I8 {
            // Local Bool flags become full-width only when another IR block
            // reads this identity through an I64 SSA variable.
            fb.ins().uextend(types::I64, word)
        } else {
            match (rep == SlotRep::RawFixnum, self.raw[value.index()]) {
                (true, false) => retag_fixnum(fb, word),
                (false, true) => lowering::sshr_imm_p(fb, word, FIXNUM_SHIFT as i64),
                _ => word,
            }
        };
        fb.def_var(self.vars[value.index()], word);
    }
}

fn read_stack(
    ctx: &mut EmitContext,
    local: &mut LocalValues,
    values: &[ir::Value],
) -> (Vec<ClifValue>, Vec<SlotRep>) {
    values
        .iter()
        .map(|&value| ctx.values.read(ctx.fb, ctx.func, local, value))
        .unzip()
}

// Shared emitters may box every alias of an operand. Propagate those changes
// back to the SSA identities, so another escape of one float reuses its box.
fn synchronize(
    local: &mut LocalValues,
    before: &[RuntimeValue],
    stack: &[ClifValue],
    reps: &[SlotRep],
) {
    for (old, (&word, &rep)) in before.iter().zip(stack.iter().zip(reps)) {
        let new = (word, rep);
        if old.1 != SlotRep::Bool
            && !((jit_opt_passes().reps || jit_opt_passes().range) && old.1 == SlotRep::RawFixnum)
            && *old != new
        {
            for runtime in local.values_mut() {
                if runtime == old {
                    *runtime = new;
                }
            }
        }
    }
}

fn tagged(ctx: &mut EmitContext, local: &mut LocalValues, value: ir::Value) -> ClifValue {
    let runtime = ctx.values.read(ctx.fb, ctx.func, local, value);
    let mut stack = vec![runtime.0];
    let mut reps = vec![runtime.1];
    materialize_model_stack(ctx.fb, ctx.rt, &mut stack, &mut reps);
    synchronize(local, &[runtime], &stack, &reps);
    stack[0]
}

/// Match the baseline's nil test unless Bool optimization is selected. A
/// flonum's word is its nonzero numeric tag word, never its f64 payload; testing
/// it needs no boxing. Threading: only compilation-local operands and settings.
fn non_nil_flag(ctx: &mut EmitContext, word: ClifValue, rep: SlotRep) -> ClifValue {
    if rep == SlotRep::Bool {
        word
    } else if jit_opt_passes().bool_rep && (rep == SlotRep::RawFixnum || rep.is_flonum()) {
        ctx.fb.ins().iconst(types::I8, 1)
    } else {
        let word = if rep == SlotRep::RawFixnum {
            retag_fixnum(ctx.fb, word)
        } else {
            word
        };
        lowering::icmp_imm_p(ctx.fb, IntCC::NotEqual, word, Value::NIL.bits() as i64)
    }
}

fn snapshot(
    ctx: &mut EmitContext,
    local: &mut LocalValues,
    frame: ir::FrameId,
) -> Result<(Vec<ClifValue>, Vec<SlotRep>), CompileError> {
    let state = &ctx.func.frames[frame.index()];
    if state.parent.is_some() || state.handlers != 0 {
        return Err(CompileError::UnsupportedOp(
            "opt-emit:frame-chain-or-handler",
        ));
    }
    if state.stack.iter().any(|&value| {
        let rep = ctx.func.values[canonical(ctx.func, value).index()].rep;
        rep != ir::Rep::Tagged
            && !(jit_opt_passes().bool_rep && rep == ir::Rep::Bool)
            && !((jit_opt_passes().reps || jit_opt_passes().range)
                && matches!(rep, ir::Rep::TaggedFix | ir::Rep::RawInt))
    }) {
        return Err(CompileError::UnsupportedOp("opt-emit:frame-representation"));
    }
    Ok(read_stack(ctx, local, &state.stack))
}

fn set_snapshot(
    ctx: &mut EmitContext,
    local: &mut LocalValues,
    frame: ir::FrameId,
) -> Result<(), CompileError> {
    let _ = snapshot(ctx, local, frame)?;
    // IR frames already encode pure-fuser caller replay. Ordinary frames may
    // contain flonums, so they cannot use the materialized RegionDeopt format.
    lowering::set_active_region(None);
    Ok(())
}

fn override_deopts(
    func: &ir::Func,
    frame: ir::FrameId,
    exact: &(Vec<ClifValue>, Vec<SlotRep>),
    pending: &mut [PendingDeopt],
) {
    for site in pending {
        site.pc = func.frames[frame.index()].pc as usize;
        site.handlers_len = func.frames[frame.index()].handlers as usize;
        site.stack.clone_from(&exact.0);
        site.reps.clone_from(&exact.1);
        site.region = None;
    }
}

fn canonical(func: &ir::Func, value: ir::Value) -> ir::Value {
    func.resolve(value).expect("verified opt value")
}

fn add_frame_uses(func: &ir::Func, frame: ir::FrameId, live: &mut HashSet<ir::Value>) {
    let mut frame = Some(frame);
    while let Some(id) = frame {
        let state = &func.frames[id.index()];
        live.extend(state.stack.iter().map(|&value| canonical(func, value)));
        frame = state.parent;
    }
}

fn term_uses(
    func: &ir::Func,
    block: ir::Block,
    entries: &[HashSet<ir::Value>],
) -> HashSet<ir::Value> {
    let term = &func.blocks[block.index()].term;
    let mut live = HashSet::new();
    for edge in term.edges() {
        let target = &func.blocks[edge.target.index()];
        for &value in &entries[edge.target.index()] {
            if let Some(index) = target.params.iter().position(|&param| param == value) {
                live.insert(canonical(func, edge.args[index]));
            } else {
                live.insert(value);
            }
        }
    }
    match term {
        ir::Term::Return(value) | ir::Term::Branch { flag: value, .. } => {
            live.insert(canonical(func, *value));
        }
        ir::Term::Switch { value, table, .. } => {
            live.insert(canonical(func, *value));
            live.insert(canonical(func, *table));
        }
        ir::Term::Deopt(frame) => add_frame_uses(func, *frame, &mut live),
        _ => {}
    }
    live
}

fn instruction_uses(func: &ir::Func, inst: &ir::InstData, live: &mut HashSet<ir::Value>) {
    if let Some(result) = inst.result {
        live.remove(&canonical(func, result));
    }
    live.extend(inst.args.iter().map(|&value| canonical(func, value)));
    if inst.op.requires_frame(inst.eff)
        && let Some(frame) = inst.frame
    {
        add_frame_uses(func, frame, live);
    }
}

/// Compiler-only heap values need roots only while live. Frame operands are
/// additional observation uses even where normal IR dataflow would kill them.
/// Threading: fixed-point dataflow over owned SSA handles, no Lisp state.
fn live_after_instructions(func: &ir::Func) -> Vec<Vec<ir::Value>> {
    let mut entries = vec![HashSet::new(); func.blocks.len()];
    loop {
        let mut changed = false;
        for index in (0..func.blocks.len()).rev() {
            let mut live = term_uses(func, ir::Block(index as u32), &entries);
            for &id in func.blocks[index].insts.iter().rev() {
                instruction_uses(func, &func.insts[id.index()], &mut live);
            }
            if live != entries[index] {
                entries[index] = live;
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    let mut after = vec![Vec::new(); func.insts.len()];
    for (index, block) in func.blocks.iter().enumerate() {
        let mut live = term_uses(func, ir::Block(index as u32), &entries);
        for &id in block.insts.iter().rev() {
            after[id.index()] = live.iter().copied().collect();
            after[id.index()].sort_unstable();
            instruction_uses(func, &func.insts[id.index()], &mut live);
        }
    }
    after
}

fn edge_arguments(
    ctx: &mut EmitContext,
    local: &mut LocalValues,
    edge: &ir::Edge,
) -> Result<Vec<BlockArg>, CompileError> {
    edge.args
        .iter()
        .zip(&ctx.func.blocks[edge.target.index()].params)
        .map(|(&value, &param)| {
            let mut runtime = ctx.values.read(ctx.fb, ctx.func, local, value);
            if runtime.1.is_flonum() {
                runtime = (tagged(ctx, local, value), SlotRep::Tagged);
            }
            if jit_opt_passes().reps
                && (runtime.1 == SlotRep::RawFixnum) != ctx.values.raw[param.index()]
            {
                return Err(CompileError::UnsupportedOp(
                    "opt-emit:integer-edge-representation",
                ));
            }
            let word = if ctx.values.booleans[param.index()] {
                debug_assert_eq!(runtime.1, SlotRep::Bool);
                runtime.0
            } else if ctx.fb.func.dfg.value_type(runtime.0) == types::I8 {
                // Block parameters use I64 even when an edge carries a Bool.
                ctx.fb.ins().uextend(types::I64, runtime.0)
            } else {
                match (
                    runtime.1 == SlotRep::RawFixnum,
                    ctx.values.raw[param.index()],
                ) {
                    (true, false) => retag_fixnum(ctx.fb, runtime.0),
                    (false, true) => lowering::sshr_imm_p(ctx.fb, runtime.0, FIXNUM_SHIFT as i64),
                    _ => runtime.0,
                }
            };
            Ok(BlockArg::from(word))
        })
        .collect()
}

fn publish_cross(ctx: &mut EmitContext, local: &mut LocalValues, block: ir::Block) {
    let mut values: Vec<_> = local
        .keys()
        .copied()
        .filter(|v| ctx.values.cross[v.index()])
        .collect();
    values.sort_unstable();
    for value in values {
        let mut runtime = ctx.values.read(ctx.fb, ctx.func, local, value);
        if runtime.1.is_flonum() {
            runtime = (tagged(ctx, local, value), SlotRep::Tagged);
        }
        // Each SSA identity has one defining block. Reads in another block
        // must not create a new definition and change the invariant's phi.
        let owner = match ctx.func.values[value.index()].def {
            ir::ValueDef::Param { block, .. } => block,
            ir::ValueDef::Inst(inst) => {
                if ctx.func.blocks[block.index()].insts.contains(&inst) {
                    block
                } else {
                    continue;
                }
            }
            ir::ValueDef::Alias(_) => continue,
        };
        if owner == block {
            ctx.values.write(ctx.fb, value, runtime);
        }
    }
}

/// A terminal poll can transfer directly to its authoritative IR edge. Trailing
/// refinements change only compiler knowledge, so their identity transport may
/// precede the poll without moving a heap access or a guard across it. Threading:
/// this inspects and updates compilation-owned values only.
fn terminal_poll_edge(
    ctx: &mut EmitContext,
    local: &mut LocalValues,
    block: ir::Block,
    position: usize,
) -> Option<(Block, Vec<BlockArg>)> {
    let data = &ctx.func.blocks[block.index()];
    let ir::Term::Jump(edge) = &data.term else {
        return None;
    };
    if data.insts[position + 1..]
        .iter()
        .any(|id| {
            let op = &ctx.func.insts[id.index()].op;
            !matches!(op, ir::Opcode::Refine(_))
                && !(jit_opt_passes().bool_rep && matches!(op, ir::Opcode::BoolToLisp))
                && !(jit_opt_passes().reps
                    && matches!(op, ir::Opcode::TagFix | ir::Opcode::UntagFix))
        })
        // Publishing a compiler-only flonum can allocate. Keep the continuation
        // in that case so its existing materialization stays after the poll.
        || local.values().any(|runtime| runtime.1.is_flonum())
    {
        return None;
    }
    for &id in &data.insts[position + 1..] {
        let inst = &ctx.func.insts[id.index()];
        let runtime = match inst.op {
            ir::Opcode::BoolToLisp => pass_emission::emit(ctx, local, inst).ok()??,
            ir::Opcode::TagFix | ir::Opcode::UntagFix => {
                arith_emission::emit(ctx, local, inst, &mut Vec::new()).ok()??
            }
            _ => ctx.values.read(ctx.fb, ctx.func, local, inst.args[0]),
        };
        local.insert(inst.result.expect("verified terminal pure result"), runtime);
    }
    publish_cross(ctx, local, block);
    let args = edge_arguments(ctx, local, edge).ok()?;
    Some((ctx.blocks[edge.target.index()], args))
}

fn guard_condition(
    ctx: &mut EmitContext,
    ty: TypeSet,
    word: ClifValue,
) -> Result<ClifValue, CompileError> {
    use crate::emacs_core::jit::opt::types::TypeKind;
    use crate::tagged::value::{TAG_CONS, TAG_FLOAT, TAG_MASK, TAG_STRING};
    if ty == TypeSet::TOP {
        return Ok(ctx.fb.ins().iconst(types::I8, 1));
    }
    if let Some(bits) = ty.singleton() {
        return Ok(lowering::icmp_imm_p(
            ctx.fb,
            IntCC::Equal,
            word,
            bits.0 as i64,
        ));
    }
    let supported = TypeSet::FIXNUM
        .join(TypeSet::LIST)
        .join(TypeSet::T)
        .join(TypeSet::FLOAT)
        .join(TypeSet::STRING);
    if !ty.is_subset(supported) {
        return Err(CompileError::UnsupportedOp("opt-emit:check-type"));
    }
    let mut valid = None;
    for (kind, tag) in [
        (TypeKind::Cons, TAG_CONS),
        (TypeKind::Float, TAG_FLOAT),
        (TypeKind::String, TAG_STRING),
    ] {
        if ty.contains(kind) {
            let masked = lowering::band_imm_p(ctx.fb, word, TAG_MASK as i64);
            let test = lowering::icmp_imm_p(ctx.fb, IntCC::Equal, masked, tag as i64);
            valid = Some(match valid {
                Some(previous) => ctx.fb.ins().bor(previous, test),
                None => test,
            });
        }
    }
    for (kind, bits) in [
        (TypeKind::Nil, Value::NIL.bits()),
        (TypeKind::T, Value::T.bits()),
    ] {
        if ty.contains(kind) {
            let test = lowering::icmp_imm_p(ctx.fb, IntCC::Equal, word, bits as i64);
            valid = Some(match valid {
                Some(previous) => ctx.fb.ins().bor(previous, test),
                None => test,
            });
        }
    }
    if ty.contains(TypeKind::Fixnum) {
        let mut test = lowering::fixnum_tag_test(ctx.fb, word);
        if let Some(range) = ty.range()
            && !(jit_opt_passes().fold && range == crate::emacs_core::jit::opt::types::Range::FULL)
        {
            let raw = lowering::sshr_imm_p(ctx.fb, word, FIXNUM_SHIFT as i64);
            let lo = lowering::icmp_imm_p(ctx.fb, IntCC::SignedGreaterThanOrEqual, raw, range.lo);
            let hi = lowering::icmp_imm_p(ctx.fb, IntCC::SignedLessThanOrEqual, raw, range.hi);
            let interval = ctx.fb.ins().band(lo, hi);
            test = ctx.fb.ins().band(test, interval);
        }
        valid = Some(match valid {
            Some(previous) => ctx.fb.ins().bor(previous, test),
            None => test,
        });
    }
    Ok(valid.unwrap_or_else(|| ctx.fb.ins().iconst(types::I8, 0)))
}

fn coemitted_list_guard(func: &ir::Func, data: &ir::BlockData, position: usize) -> bool {
    let guard = &func.insts[data.insts[position].index()];
    let Some(next) = data
        .insts
        .get(position + 1)
        .map(|id| &func.insts[id.index()])
    else {
        return false;
    };
    let same_source = matches!(guard.op,ir::Opcode::CheckType(ty) if ty == TypeSet::LIST)
        && guard.frame == next.frame
        && guard
            .result
            .is_some_and(|result| next.args.first() == Some(&result));
    // Keep the existing opaque path's predicate and emission order unchanged.
    if matches!(next.op, ir::Opcode::Opaque(Op::Car | Op::Cdr)) {
        return same_source;
    }
    // GVN exposes the same source read as a typed LIST load. The guard may
    // move into that accessor ONLY when the accessor is also dispatched to
    // shared_operation below, which emits the original three-way guard.
    // Threading: this is compiler-local adjacency/identity metadata only.
    jit_opt_passes().gvn
        && same_source
        && guard.frame.is_some()
        && guard.pc == next.pc
        && guard.args.len() == 1
        && guard.eff == crate::emacs_core::jit::opt::mem::Effects::MAY_DEOPT
        && guard.mem == crate::emacs_core::jit::opt::mem::AliasClass::None
        && next.args.len() == 1
        && next
            .result
            .is_some_and(|result| func.values[result.index()].rep == ir::Rep::Tagged)
        && next.eff == crate::emacs_core::jit::opt::mem::Effects::READ_HEAP
        && matches!(
            (next.op.clone(), next.mem),
            (
                ir::Opcode::LoadCar,
                crate::emacs_core::jit::opt::mem::AliasClass::ConsCar
            ) | (
                ir::Opcode::LoadCdr,
                crate::emacs_core::jit::opt::mem::AliasClass::ConsCdr
            )
        )
        && guard.result.is_some_and(|result| {
            let value = &func.values[result.index()];
            value.rep == ir::Rep::Tagged && value.ty == TypeSet::LIST
        })
}

fn coemitted_typed_list_read(func: &ir::Func, data: &ir::BlockData, position: usize) -> bool {
    position
        .checked_sub(1)
        .is_some_and(|previous| coemitted_list_guard(func, data, previous))
}

// A shared store may use this proof only for the exact result of an adjacent
// explicit CONS guard with the same observable frame. The guard still emits its
// deopt (including FORCE_DEOPT); the shared store retains all barrier handling.
fn checked_cons_store(func: &ir::Func, data: &ir::BlockData, position: usize) -> bool {
    let store = &func.insts[data.insts[position].index()];
    let Some(previous) = position
        .checked_sub(1)
        .map(|index| &func.insts[data.insts[index].index()])
    else {
        return false;
    };
    matches!(store.op, ir::Opcode::Opaque(Op::Setcar | Op::Setcdr))
        && matches!(previous.op, ir::Opcode::CheckType(ty) if ty == TypeSet::CONS)
        && previous.frame == store.frame
        && previous
            .result
            .is_some_and(|result| store.args.first() == Some(&result))
}

fn returns_call_result(
    func: &ir::Func,
    data: &ir::BlockData,
    position: usize,
    inst: &ir::InstData,
    live_after: &[ir::Value],
) -> bool {
    let (Some(result), ir::Term::Return(returned)) = (inst.result, &data.term) else {
        return false;
    };
    let result = canonical(func, result);
    position + 1 == data.insts.len()
        && canonical(func, *returned) == result
        && live_after.iter().all(|&value| {
            let value = canonical(func, value);
            value == result || !func.values[value.index()].ty.may_need_root()
        })
        // The baseline tail-call proof also relies on the call's frame tail
        // being its actual operands: those values are rooted by the callee.
        // Rewritten SSA operands retain the complete frame instead.
        && inst.frame.is_some_and(|frame| {
            let stack = &func.frames[frame.index()].stack;
            stack.len() >= inst.args.len()
                && stack[stack.len() - inst.args.len()..]
                    .iter()
                    .zip(&inst.args)
                    .all(|(&a, &b)| canonical(func, a) == canonical(func, b))
        })
}

#[allow(clippy::too_many_arguments)]
fn shared_operation(
    ctx: &mut EmitContext,
    local: &mut LocalValues,
    inst: &ir::InstData,
    live_after: &[ir::Value],
    returns_result: bool,
    checked_cons: bool,
    op: &Op,
    known: &HashSet<ClifValue>,
    deopts: &mut Vec<PendingDeopt>,
    pending: &mut Vec<PendingDispatch>,
    constants: &[Value],
) -> Result<Option<RuntimeValue>, CompileError> {
    let (needs, delta) = simple_effect(op)?;
    if needs != inst.args.len() || needs as i64 + delta != i64::from(inst.result.is_some()) {
        return Err(CompileError::UnsupportedOp("opt-emit:operand-shape"));
    }
    let frame = inst
        .frame
        .ok_or(CompileError::UnsupportedOp("opt-emit:frame"))?;
    let (mut stack, mut reps) = snapshot(ctx, local, frame)?;
    // Framestates specify every residual GNU stack value. The operation's
    // explicit SSA operands specify what it actually consumes, independently
    // of the original bytecode's stack permutations.
    let mut base = stack
        .len()
        .checked_sub(needs)
        .ok_or(CompileError::StackUnderflow)?;
    stack.truncate(base);
    reps.truncate(base);
    let (args, arg_reps) = read_stack(ctx, local, &inst.args);
    stack.extend(args);
    reps.extend(arg_reps);
    let spec = ctx.spec_sites.get(&(inst.pc as usize)).map(|site| {
        (
            site.sym,
            site.expected_bits,
            &ctx.spec_slots[site.slot] as *const SpecSlot as i64,
            site.slot,
            site.kind,
        )
    });
    // This is the baseline's existing tail-call emission policy, proved from
    // the actual SSA return rather than the original bytecode's next opcode.
    // The exact deopt frame remains separate from the runtime root window.
    let dead = lowering::tail_call_dead_residuals(
        op,
        returns_result.then_some(&Op::Return),
        ctx.func.frames[frame.index()].handlers != 0,
        spec.map(|(_, _, _, _, kind)| kind),
        stack.len(),
    );
    if let Some(dead) = dead {
        stack.drain(..dead);
        reps.drain(..dead);
        base -= dead;
    }
    if inst.eff.intersects(
        crate::emacs_core::jit::opt::mem::Effects::MAY_GC
            .with(crate::emacs_core::jit::opt::mem::Effects::MAY_REENTER),
    ) {
        // Rewritten operands can differ from the original frame's tail. Both
        // the full GNU frame and live compiler-only heap identities stay live.
        let mut roots = ctx.func.frames[frame.index()].stack[dead.unwrap_or(0)..].to_vec();
        roots.extend_from_slice(live_after);
        let mut extra = Vec::new();
        for value in roots {
            let value = canonical(ctx.func, value);
            if inst.result == Some(value) || !ctx.func.values[value.index()].ty.may_need_root() {
                continue;
            }
            let runtime = ctx.values.read(ctx.fb, ctx.func, local, value);
            if runtime.1 != SlotRep::Tagged
                || stack
                    .iter()
                    .copied()
                    .zip(reps.iter().copied())
                    .chain(extra.iter().copied())
                    .any(|old| old == runtime)
            {
                continue;
            }
            extra.push(runtime);
        }
        let extra_len = extra.len();
        let (words, kinds): (Vec<_>, Vec<_>) = extra.into_iter().unzip();
        stack.splice(0..0, words);
        reps.splice(0..0, kinds);
        base += extra_len;
    }
    let before: Vec<_> = stack.iter().copied().zip(reps.iter().copied()).collect();
    if !lowering::op_preserves_raw(op) {
        lowering::prepare_op_operands(ctx.fb, ctx.rt, op, &mut stack, &mut reps)?;
        synchronize(local, &before, &stack, &reps);
    }
    set_snapshot(ctx, local, frame)?;
    let exact = snapshot(ctx, local, frame)?;
    let deopt_start = deopts.len();
    let cons_proof = if checked_cons {
        heap_inline::ConsStoreProof::GuardedCons(stack[base])
    } else {
        heap_inline::ConsStoreProof::Dynamic
    };
    if array_emission::has_plain_read(ctx, inst) {
        let result = array_emission::emit_plain_read(ctx, local, inst)?;
        stack.truncate(base);
        reps.truncate(base);
        stack.push(result.0);
        reps.push(result.1);
    } else {
        lowering::lower_simple_op_with_cons_proof_and_result(
            ctx.fb,
            inst.pc as usize,
            deopts,
            ctx.signal_exit,
            constants,
            &mut stack,
            &mut reps,
            ctx.rt,
            &[],
            pending,
            spec,
            op,
            known,
            ctx.reloc_base,
            ctx.reloc_index,
            ctx.aot,
            ctx.spec_slot_base,
            ctx.spec_expected_base,
            ctx.dynamic_prefix,
            ctx.consts_base,
            cons_proof,
            if matches!(inst.op, ir::Opcode::OpaqueBool(_)) {
                boolean::BoolResultMode::BoolFlag
            } else {
                boolean::BoolResultMode::TaggedLisp
            },
        )?;
    }
    override_deopts(ctx.func, frame, &exact, &mut deopts[deopt_start..]);
    synchronize(local, &before[..base], &stack[..base], &reps[..base]);
    // Operand materialization can replace a flonum by its one shared box.
    // The emitter pops operands, so synchronize aliases using its result only
    // where it returned an existing operand (Setcar/Setcdr/Aset).
    if matches!(op, Op::Setcar | Op::Setcdr | Op::Aset) {
        if let Some((&word, &rep)) = stack.last().zip(reps.last()) {
            let old = *before.last().ok_or(CompileError::StackUnderflow)?;
            synchronize(local, &[old], &[word], &[rep]);
        }
    }
    Ok(inst.result.map(|_| {
        (
            *stack.last().expect("one IR result"),
            *reps.last().expect("one IR result rep"),
        )
    }))
}

/// Emit every instruction and terminator of the verified Func. The bytecode
/// stream supplies compile-time specialization metadata only; SSA operands,
/// constants, frames, block parameters and edges determine native behavior.
pub(super) fn emit(mut ctx: EmitContext<'_, '_>) -> Result<(), CompileError> {
    if ctx.func.values.iter().enumerate().any(|(index, value)| {
        !matches!(value.rep, ir::Rep::Tagged | ir::Rep::Bool)
            && !(jit_opt_passes().reps && matches!(value.rep, ir::Rep::TaggedFix | ir::Rep::RawInt))
            && !(jit_opt_passes().range
                && (value.rep == ir::Rep::TaggedFix
                    || (value.rep == ir::Rep::RawInt
                        && array_emission::proved_raw_value(&ctx, ir::Value(index as u32)))))
    }) {
        return Err(CompileError::UnsupportedOp("opt-emit:representation"));
    }
    if !ctx.func.blocks[ctx.func.entry.index()].params.is_empty() {
        return Err(CompileError::UnsupportedOp("opt-emit:entry-parameters"));
    }
    // Value::from_bits does not dereference a heap value. The compilation's
    // source owner retains the source constant pool roots throughout emission.
    let constants: Vec<Value> = ctx
        .func
        .consts
        .iter()
        .map(|bits| Value::from_bits(bits.0 as usize))
        .collect();
    for (index, data) in ctx.func.blocks.iter().enumerate() {
        for &param in &data.params {
            ctx.fb.append_block_param(
                ctx.blocks[index],
                if ctx.values.booleans[param.index()] {
                    types::I8
                } else {
                    types::I64
                },
            );
        }
    }
    for index in 0..ctx.func.blocks.len() {
        let block = ir::Block(index as u32);
        let data = &ctx.func.blocks[index];
        ctx.fb.switch_to_block(ctx.blocks[index]);
        lowering::rootwin_carry_reset();
        lowering::set_active_region(None);
        let mut local = LocalValues::new();
        let params = ctx.fb.block_params(ctx.blocks[index]).to_vec();
        for (&value, &word) in data.params.iter().zip(&params) {
            local.insert(value, (word, ctx.values.representation(value)));
        }
        let mut known = HashSet::new();
        if let Some(facts) = ctx.known_fixnum_slots.get(&(data.pc as usize)) {
            for (&value, &fact) in ctx
                .func
                .entry_stacks
                .get(index)
                .map_or(&[][..], |stack| &stack[..])
                .iter()
                .zip(facts)
            {
                if fact {
                    let runtime = ctx.values.read(ctx.fb, ctx.func, &mut local, value);
                    if runtime.1 == SlotRep::Tagged {
                        known.insert(runtime.0);
                    }
                }
            }
        }
        let mut deopts = Vec::new();
        let mut pending = Vec::new();
        let mut terminated = false;
        for (position, &id) in data.insts.iter().enumerate() {
            let inst = &ctx.func.insts[id.index()];
            let live_after = ctx.values.live_after[id.index()].clone();
            let result = match &inst.op {
                ir::Opcode::Arg(slot) | ir::Opcode::OsrSlot(slot) => {
                    let slot = *slot as usize;
                    let variable = *ctx.seed_vars.get(slot).ok_or(CompileError::BadOperand)?;
                    let word = ctx.fb.use_var(variable);
                    Some(
                        if (jit_opt_passes().reps || jit_opt_passes().range)
                            && ctx.variable_raw[slot]
                        {
                            // Entry guards and ABI slot hints belong to the shared
                            // scaffold. Original Arg/OsrSlot identities still hold
                            // their full GNU words; explicit checked views select
                            // the numeric representations inside the opt body.
                            (retag_fixnum(ctx.fb, word), SlotRep::Tagged)
                        } else {
                            (word, SlotRep::raw_if(ctx.variable_raw[slot]))
                        },
                    )
                }
                ir::Opcode::Const(index)
                    if (jit_opt_passes().reps || jit_opt_passes().licm) && inst.frame.is_none() =>
                {
                    // Integer exposure creates pure immediate zero/one views
                    // without a source operation or recovery frame. Decode
                    // only opaque static fixnum bits on the owning compiler.
                    if (*index as usize) < ctx.dynamic_prefix {
                        return Err(CompileError::UnsupportedOp("opt-emit:constant-prefix"));
                    }
                    let bits = *ctx
                        .func
                        .consts
                        .get(*index as usize)
                        .ok_or(CompileError::BadOperand)?;
                    let ty = TypeSet::for_constant(bits);
                    let result = inst.result.ok_or(CompileError::BadOperand)?;
                    if ty.is_bottom()
                        || !ty.is_subset(TypeSet::FIXNUM)
                        || !ctx.func.values[result.index()].rep.is_tagged()
                    {
                        return Err(CompileError::UnsupportedOp("opt-emit:immediate-proof"));
                    }
                    Some((
                        ctx.fb.ins().iconst(types::I64, bits.0 as i64),
                        SlotRep::Tagged,
                    ))
                }
                ir::Opcode::Const(index) | ir::Opcode::EnvConst(index) => {
                    let index = u16::try_from(*index).map_err(|_| CompileError::BadOperand)?;
                    if matches!(inst.op, ir::Opcode::Const(_))
                        && (index as usize) < ctx.dynamic_prefix
                    {
                        return Err(CompileError::UnsupportedOp("opt-emit:constant-prefix"));
                    }
                    if matches!(inst.op, ir::Opcode::EnvConst(_))
                        && (index as usize) >= ctx.dynamic_prefix
                    {
                        return Err(CompileError::UnsupportedOp("opt-emit:environment-prefix"));
                    }
                    shared_operation(
                        &mut ctx,
                        &mut local,
                        inst,
                        &live_after,
                        false,
                        false,
                        &Op::Constant(index),
                        &known,
                        &mut deopts,
                        &mut pending,
                        &constants,
                    )?
                }
                ir::Opcode::BoolConst(_)
                | ir::Opcode::BoolToLisp
                | ir::Opcode::TypeTest(_)
                | ir::Opcode::Select
                    if jit_opt_passes().fold
                        || jit_opt_passes().bool_rep
                        || jit_opt_passes().reps =>
                {
                    pass_emission::emit(&mut ctx, &mut local, inst)?
                }
                ir::Opcode::LoadCar | ir::Opcode::LoadCdr
                    if jit_opt_passes().fold || jit_opt_passes().gvn =>
                {
                    if coemitted_typed_list_read(ctx.func, data, position) {
                        let op = if matches!(inst.op, ir::Opcode::LoadCdr) {
                            Op::Cdr
                        } else {
                            Op::Car
                        };
                        // This is the counterpart of the deferred CheckType.
                        // The shared accessor emits its real nil/cons/nonlist
                        // guard (including FORCE_DEOPT), then the original
                        // nil/field branch. It never consumes an unchecked
                        // nonlist as a cons pointer. Full frame/pc overrides
                        // remain exactly those used by the opaque baseline.
                        shared_operation(
                            &mut ctx,
                            &mut local,
                            inst,
                            &live_after,
                            false,
                            false,
                            &op,
                            &known,
                            &mut deopts,
                            &mut pending,
                            &constants,
                        )?
                    } else {
                        pass_emission::emit(&mut ctx, &mut local, inst)?
                    }
                }
                ir::Opcode::TagFix
                | ir::Opcode::UntagFix
                | ir::Opcode::FixAdd { .. }
                | ir::Opcode::FixSub { .. }
                | ir::Opcode::FixMul { .. }
                | ir::Opcode::FixCmp(_)
                    if jit_opt_passes().reps =>
                {
                    arith_emission::emit(&mut ctx, &mut local, inst, &mut deopts)?
                }
                ir::Opcode::Opaque(op) | ir::Opcode::OpaqueBool(op) => {
                    if matches!(inst.op, ir::Opcode::OpaqueBool(_))
                        && (!jit_opt_passes().bool_rep || !boolean::BoolResultMode::supports(op))
                    {
                        return Err(CompileError::UnsupportedOp("opt-emit:bool-opcode"));
                    }
                    if matches!(
                        op,
                        Op::StackRef(_) | Op::StackSet(_) | Op::Dup | Op::Pop | Op::DiscardN(_)
                    ) {
                        return Err(CompileError::UnsupportedOp("opt-emit:opaque-stack-op"));
                    }
                    let returns_result =
                        returns_call_result(ctx.func, data, position, inst, &live_after);
                    let checked_cons = checked_cons_store(ctx.func, data, position);
                    shared_operation(
                        &mut ctx,
                        &mut local,
                        inst,
                        &live_after,
                        returns_result,
                        checked_cons,
                        op,
                        &known,
                        &mut deopts,
                        &mut pending,
                        &constants,
                    )?
                }
                ir::Opcode::Refine(_) => {
                    Some(ctx.values.read(ctx.fb, ctx.func, &mut local, inst.args[0]))
                }
                ir::Opcode::CheckType(ty)
                    if jit_opt_passes().range
                        && !ty.is_bottom()
                        && ty.is_subset(TypeSet::VECTOR.join(TypeSet::RECORD)) =>
                {
                    array_emission::emit(&mut ctx, &mut local, id, inst, &mut deopts)?
                }
                ir::Opcode::LoadVecLen | ir::Opcode::CheckBounds if jit_opt_passes().range => {
                    array_emission::emit(&mut ctx, &mut local, id, inst, &mut deopts)?
                }
                ir::Opcode::CheckType(ty) => {
                    let input_rep = ctx.func.values[canonical(ctx.func, inst.args[0]).index()].rep;
                    if input_rep != ir::Rep::Tagged
                        && !((jit_opt_passes().reps || jit_opt_passes().range)
                            && input_rep == ir::Rep::TaggedFix)
                    {
                        return Err(CompileError::UnsupportedOp("opt-emit:guard-representation"));
                    }
                    if !coemitted_list_guard(ctx.func, data, position) {
                        let frame = inst
                            .frame
                            .ok_or(CompileError::UnsupportedOp("opt-emit:guard-frame"))?;
                        let word = tagged(&mut ctx, &mut local, inst.args[0]);
                        set_snapshot(&mut ctx, &mut local, frame)?;
                        let (stack, reps) = snapshot(&mut ctx, &mut local, frame)?;
                        let site = deopt_site(
                            ctx.fb,
                            ctx.func.frames[frame.index()].pc as usize,
                            0,
                            &stack,
                            &reps,
                            &mut deopts,
                        );
                        override_deopts(
                            ctx.func,
                            frame,
                            &(stack, reps),
                            std::slice::from_mut(deopts.last_mut().expect("queued guard deopt")),
                        );
                        let condition = guard_condition(&mut ctx, *ty, word)?;
                        lowering::emit_guard(ctx.fb, site, condition);
                    }
                    Some(ctx.values.read(ctx.fb, ctx.func, &mut local, inst.args[0]))
                }
                ir::Opcode::IsNonNil => {
                    let (word, rep) = ctx.values.read(ctx.fb, ctx.func, &mut local, inst.args[0]);
                    let flag = non_nil_flag(&mut ctx, word, rep);
                    Some((
                        flag,
                        if jit_opt_passes().bool_rep {
                            SlotRep::Bool
                        } else {
                            SlotRep::Tagged
                        },
                    ))
                }
                ir::Opcode::InlineEntry(region) => {
                    let fused = inline::active_fused()
                        .ok_or(CompileError::UnsupportedOp("opt-emit:inline-metadata"))?;
                    let region = fused
                        .regions
                        .get(*region as usize)
                        .ok_or(CompileError::BadOperand)?;
                    let (mut stack, mut reps) = read_stack(&mut ctx, &mut local, &inst.args);
                    let before: Vec<_> = stack.iter().copied().zip(reps.iter().copied()).collect();
                    materialize_model_stack(ctx.fb, ctx.rt, &mut stack, &mut reps);
                    synchronize(&mut local, &before, &stack, &reps);
                    set_snapshot(
                        &mut ctx,
                        &mut local,
                        inst.frame.ok_or(CompileError::BadOperand)?,
                    )?;
                    let frame = inst.frame.ok_or(CompileError::BadOperand)?;
                    let exact = snapshot(&mut ctx, &mut local, frame)?;
                    let deopt_start = deopts.len();
                    lowering::emit_region_entry_guard(
                        ctx.fb,
                        region,
                        0,
                        &stack,
                        &reps,
                        &mut deopts,
                    )?;
                    override_deopts(ctx.func, frame, &exact, &mut deopts[deopt_start..]);
                    None
                }
                ir::Opcode::Poll => {
                    let frame = inst.frame.ok_or(CompileError::BadOperand)?;
                    let (mut stack, mut reps) = snapshot(&mut ctx, &mut local, frame)?;
                    for &value in &live_after {
                        let value = canonical(ctx.func, value);
                        if !ctx.func.values[value.index()].ty.may_need_root() {
                            continue;
                        }
                        let runtime = ctx.values.read(ctx.fb, ctx.func, &mut local, value);
                        if runtime.1 != SlotRep::Tagged
                            || stack
                                .iter()
                                .copied()
                                .zip(reps.iter().copied())
                                .any(|old| old == runtime)
                        {
                            continue;
                        }
                        stack.push(runtime.0);
                        reps.push(runtime.1);
                    }
                    let before: Vec<_> = stack.iter().copied().zip(reps.iter().copied()).collect();
                    box_all_flonums(ctx.fb, ctx.rt, &mut stack, &mut reps);
                    synchronize(&mut local, &before, &stack, &reps);
                    let raw: Vec<_> = reps.iter().map(|rep| *rep == SlotRep::RawFixnum).collect();
                    let direct = terminal_poll_edge(&mut ctx, &mut local, block, position);
                    let (target, args) = match &direct {
                        Some((target, args)) => (*target, &args[..]),
                        None => (ctx.fb.create_block(), &[][..]),
                    };
                    let bools: Vec<_> = reps.iter().map(|rep| *rep == SlotRep::Bool).collect();
                    emit_backedge_jump_with_representations(
                        ctx.fb,
                        ctx.rt
                            .ok_or(CompileError::UnsupportedOp("opt-emit:poll-runtime"))?,
                        ctx.backedge_counter.ok_or(CompileError::BadOperand)?,
                        ctx.signal_exit,
                        &stack,
                        Some(&raw),
                        bools.iter().any(|&flag| flag).then_some(&bools[..]),
                        target,
                        args,
                        &[],
                        &mut pending,
                        None, // v2 entry protocols retain baseline lowering.
                    );
                    if direct.is_some() {
                        terminated = true;
                    } else {
                        ctx.fb.switch_to_block(target);
                        ctx.fb.seal_block(target);
                    }
                    None
                }
                _ => return Err(CompileError::UnsupportedOp("opt-emit:typed-opcode")),
            };
            if let (Some(value), Some(runtime)) = (inst.result, result) {
                local.insert(value, runtime);
            }
            if terminated {
                break;
            }
        }
        lowering::set_active_region(None);
        if !terminated {
            publish_cross(&mut ctx, &mut local, block);
            emit_term(
                &mut ctx,
                &mut local,
                block,
                &mut deopts,
                &mut pending,
                &constants,
            )?;
        }
        for site in &deopts {
            if jit_opt_passes().reps
                || ctx.variable_raw.iter().any(|&raw| raw)
                || site.holds_flonum()
                || site.reps.contains(&SlotRep::Bool)
            {
                ctx.fb.set_cold_block(site.block);
            }
        }
        emit_pending_deopts(
            ctx.fb,
            ctx.deopt_refs,
            &mut deopts,
            ctx.rt.map(|rt| &rt.refs),
            ctx.abi,
        );
        if !pending.is_empty() {
            return Err(CompileError::UnsupportedOp("opt-emit:exceptional-edge"));
        }
    }
    lowering::set_active_region(None);
    Ok(())
}

/// Switch dispatch needs landing blocks without arguments; their small bodies
/// transfer the authoritative edge values into the actual IR block parameters.
/// Threading: these are compiler-local CLIF block handles.
struct SwitchLandings {
    blocks: Vec<Block>,
}
impl switch_dispatch::SwitchLandings for SwitchLandings {
    fn landing(&mut self, _fb: &mut FunctionBuilder, index: usize) -> Block {
        self.blocks[index]
    }
    fn fill_pending(&mut self, _fb: &mut FunctionBuilder) {}
}

fn source_site(ctx: &EmitContext, block: ir::Block) -> Option<usize> {
    ctx.func
        .source_states
        .iter()
        .enumerate()
        .filter_map(|(pc, state)| {
            state
                .as_ref()
                .filter(|state| state.block == block)
                .map(|_| pc)
        })
        .last()
}

#[allow(clippy::too_many_arguments)]
fn emit_term(
    ctx: &mut EmitContext,
    local: &mut LocalValues,
    block: ir::Block,
    deopts: &mut Vec<PendingDeopt>,
    pending: &mut Vec<PendingDispatch>,
    constants: &[Value],
) -> Result<(), CompileError> {
    match &ctx.func.blocks[block.index()].term {
        ir::Term::Return(value) => {
            let result = tagged(ctx, local, *value);
            let out = ctx.out_present.then(|| ctx.fb.use_var(ctx.out_var));
            reg_abi::emit_leaf_return(ctx.fb, ctx.abi, out, Some(result), STATUS_OK);
        }
        ir::Term::Jump(edge) => {
            let args = edge_arguments(ctx, local, edge)?;
            ctx.fb.ins().jump(ctx.blocks[edge.target.index()], &args);
        }
        ir::Term::Branch {
            flag,
            if_true,
            if_false,
        } => {
            let (word, rep) = ctx.values.read(ctx.fb, ctx.func, local, *flag);
            // Bool uses zero/one; tagged Lisp uses nil/non-nil. A raw fixnum
            // always denotes a non-nil Lisp value, including numerical zero.
            // Flonum payloads likewise denote only a float or a fixnum.
            let condition = if ctx.func.values[canonical(ctx.func, *flag).index()].rep
                == ir::Rep::Bool
                && ctx.fb.func.dfg.value_type(word) == types::I8
            {
                word
            } else {
                non_nil_flag(ctx, word, rep)
            };
            let yes = edge_arguments(ctx, local, if_true)?;
            let no = edge_arguments(ctx, local, if_false)?;
            ctx.fb.ins().brif(
                condition,
                ctx.blocks[if_true.target.index()],
                &yes,
                ctx.blocks[if_false.target.index()],
                &no,
            );
        }
        ir::Term::Deopt(frame) => {
            set_snapshot(ctx, local, *frame)?;
            let (stack, reps) = snapshot(ctx, local, *frame)?;
            let site = deopt_site(
                ctx.fb,
                ctx.func.frames[frame.index()].pc as usize,
                0,
                &stack,
                &reps,
                deopts,
            );
            override_deopts(
                ctx.func,
                *frame,
                &(stack, reps),
                std::slice::from_mut(deopts.last_mut().expect("queued terminator deopt")),
            );
            ctx.fb.ins().jump(site, &[]);
        }
        ir::Term::Unreachable => {
            ctx.fb
                .ins()
                .trap(cranelift_codegen::ir::TrapCode::unwrap_user(1));
        }
        ir::Term::Switch {
            value,
            table,
            cases,
            default,
        } => {
            let dispatch = tagged(ctx, local, *value);
            let table_word = tagged(ctx, local, *table);
            let rt = ctx
                .rt
                .ok_or(CompileError::UnsupportedOp("opt-emit:switch-runtime"))?;
            let mut work = Vec::new();
            let mut landing = |ctx: &mut EmitContext, local: &mut LocalValues, edge: &ir::Edge| {
                let args = edge_arguments(ctx, local, edge)?;
                let target = ctx.blocks[edge.target.index()];
                if args.is_empty() {
                    Ok(target)
                } else {
                    let block = ctx.fb.create_block();
                    work.push((block, target, args));
                    Ok(block)
                }
            };
            let miss = landing(ctx, local, default)?;
            let blocks = cases
                .iter()
                .map(|case| landing(ctx, local, &case.edge))
                .collect::<Result<Vec<_>, CompileError>>()?;
            let targets: Vec<_> = cases
                .iter()
                .map(|case| (case.key, case.edge.target.index()))
                .collect();
            let site = source_site(ctx, block);
            let mut stack = Vec::new();
            let mut reps = Vec::new();
            if let Some(frame) =
                site.and_then(|pc| ctx.func.source_states[pc].as_ref().map(|state| state.frame))
            {
                (stack, reps) = snapshot(ctx, local, frame)?;
                if stack.len() >= 2 {
                    stack.truncate(stack.len() - 2);
                    reps.truncate(stack.len());
                }
                materialize_model_stack(ctx.fb, ctx.rt, &mut stack, &mut reps);
            }
            let stale =
                signal_target_for_site(ctx.fb, ctx.signal_exit, &[], pending, &stack, &reps);
            // Inline plans require the exact constant table their source
            // metadata names. Dispatch and landing edges still come from IR.
            let inline = if !ctx.aot && jit_inline_switch_on() {
                site.and_then(|pc| {
                    let ir::ValueDef::Inst(table_inst) =
                        ctx.func.values[ctx.func.resolve(*table)?.index()].def
                    else {
                        return None;
                    };
                    let ir::Opcode::Const(index) = ctx.func.insts[table_inst.index()].op else {
                        return None;
                    };
                    let Some(Op::Constant(source_index)) =
                        pc.checked_sub(1).and_then(|i| ctx.ops.get(i))
                    else {
                        return None;
                    };
                    if index != *source_index as u32
                        || constants.get(index as usize) != ctx.constants.get(index as usize)
                    {
                        return None;
                    }
                    switch_dispatch::inline_switch_for_site(
                        ctx.ops,
                        constants,
                        ctx.dynamic_prefix,
                        &ctx.cfg.leaders,
                        pc,
                        &targets,
                    )
                })
            } else {
                None
            };
            let mut landings = SwitchLandings { blocks };
            switch_dispatch::emit_switch_dispatch(
                ctx.fb,
                rt,
                dispatch,
                table_word,
                &targets,
                miss,
                stale,
                &mut landings,
                inline.as_ref(),
            );
            for (landing, target, args) in work {
                ctx.fb.switch_to_block(landing);
                ctx.fb.seal_block(landing);
                ctx.fb.ins().jump(target, &args);
            }
        }
    }
    Ok(())
}
