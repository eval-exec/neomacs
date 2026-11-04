//! Lower the final IR's explicit epoch-guard schedule. Source constants remain
//! rooted by the leaf; fixed epoch addresses share InlineSwitch's lifetime.
//! Threading: invocation-owned plans; no runtime cache or mutator TLS is added.

use super::*;
use crate::emacs_core::jit::opt::passes::switch_epochs::{CheckSwitchEpoch, SwitchEpochGuards};

/// Keep the ordinary emitter's frame small. Sources without switches never
/// construct hashers, scan blocks or read the switch-hoist knob; active state
/// is allocated and initialized entirely inside the cold builder.
#[derive(Debug)]
pub(super) struct Plans<'a>(Option<Box<ActivePlans<'a>>>);

#[derive(Debug)]
struct ActivePlans<'a> {
    inline: HashMap<ir::Block, switch_dispatch::InlineSwitch>,
    rematerialize: HashSet<u32>,
    deferred: HashMap<ir::Block, (ir::Inst, usize)>,
    guards: SwitchEpochGuards<'a>,
}

impl<'a> Plans<'a> {
    #[inline]
    #[doc(hidden)]
    pub fn build(ctx: &EmitContext<'a, '_>, constants: &[Value]) -> Result<Self, CompileError> {
        if ctx.aot
            || ctx.cfg.switch_targets.is_empty()
            || !jit_inline_switch_on()
            || !super::knobs::jit_switch_hoist_on()
        {
            return Ok(Self(None));
        }
        Self::build_active(ctx, constants)
    }

    #[cold]
    #[inline(never)]
    #[doc(hidden)]
    pub fn build_active(
        ctx: &EmitContext<'a, '_>,
        constants: &[Value],
    ) -> Result<Self, CompileError> {
        let mut inline = HashMap::new();
        let mut rematerialize = HashSet::new();
        let mut deferred = HashMap::new();
        for (i, data) in ctx.func.blocks.iter().enumerate() {
            let block = ir::Block(i as u32);
            if let Some(plan) = inline_for_block(ctx, block, constants) {
                let ir::Term::Switch { table, .. } = &data.term else {
                    unreachable!()
                };
                let ir::ValueDef::Inst(id) =
                    ctx.func.values[canonical(ctx.func, *table).index()].def
                else {
                    unreachable!()
                };
                let ir::Opcode::Const(index) = ctx.func.insts[id.index()].op else {
                    unreachable!()
                };
                rematerialize.insert(index);
                if ctx.verified_sink.is_none()
                    && let Some(id) =
                        crate::emacs_core::jit::opt::passes::switch_epochs::deferred_table_const(
                            ctx.func, block,
                        )
                {
                    deferred.insert(block, (id, constants[index as usize].bits()));
                }
                inline.insert(block, plan);
            }
        }
        let guards = SwitchEpochGuards::build(ctx.func, &inline.keys().copied().collect())
            .map_err(|_| CompileError::UnsupportedOp("opt-switch:guard-schedule"))?;
        Ok(Self(Some(Box::new(ActivePlans {
            inline,
            rematerialize,
            deferred,
            guards,
        }))))
    }

    #[doc(hidden)]
    pub fn elide(&self, id: ir::Inst) -> bool {
        self.0
            .as_ref()
            .is_some_and(|p| p.deferred.values().any(|&(inst, _)| inst == id))
    }
    #[doc(hidden)]
    pub fn deferred_table(&self, block: ir::Block) -> Option<usize> {
        self.0.as_ref()?.deferred.get(&block).map(|&(_, word)| word)
    }
    #[doc(hidden)]
    pub fn enabled(&self) -> bool {
        self.0.is_some()
    }
    #[doc(hidden)]
    pub fn rematerialize(&self, index: u32) -> bool {
        self.0
            .as_ref()
            .is_some_and(|p| p.rematerialize.contains(&index))
    }
    #[doc(hidden)]
    pub fn inline(&self, block: ir::Block) -> Option<&switch_dispatch::InlineSwitch> {
        self.0.as_ref()?.inline.get(&block)
    }
    #[doc(hidden)]
    pub fn entries(&self, block: ir::Block) -> &[CheckSwitchEpoch] {
        self.0.as_ref().map_or(&[], |p| p.guards.entries(block))
    }
    #[doc(hidden)]
    pub fn after_poll(&self, inst: ir::Inst) -> &[ir::Block] {
        self.0.as_ref().map_or(&[], |p| p.guards.after_poll(inst))
    }
    #[doc(hidden)]
    pub fn is_hoisted(&self, block: ir::Block) -> bool {
        self.0.as_ref().is_some_and(|p| p.guards.is_hoisted(block))
    }
}

static_assertions::assert_impl_all!(Plans<'static>: Send, Sync);

/// The same exact source/SSA table agreement as the original term emitter.
/// This is the source-owning front's heap inspection seam, outside IR analysis.
pub(super) fn inline_for_block(
    ctx: &EmitContext,
    block: ir::Block,
    constants: &[Value],
) -> Option<switch_dispatch::InlineSwitch> {
    if ctx.aot || !jit_inline_switch_on() {
        return None;
    }
    let ir::Term::Switch { table, cases, .. } = &ctx.func.blocks[block.index()].term else {
        return None;
    };
    let pc = source_site(ctx, block)?;
    let ir::ValueDef::Inst(id) = ctx.func.values[ctx.func.resolve(*table)?.index()].def else {
        return None;
    };
    let ir::Opcode::Const(index) = ctx.func.insts[id.index()].op else {
        return None;
    };
    let Some(Op::Constant(source_index)) = pc.checked_sub(1).and_then(|i| ctx.ops.get(i)) else {
        return None;
    };
    if index != *source_index as u32
        || constants.get(index as usize) != ctx.constants.get(index as usize)
    {
        return None;
    }
    let targets: Vec<_> = cases
        .iter()
        .map(|case| (case.key, case.edge.target.index()))
        .collect();
    switch_dispatch::inline_switch_for_site(
        ctx.ops,
        constants,
        ctx.dynamic_prefix,
        &ctx.cfg.leaders,
        pc,
        &targets,
    )
}

pub(super) fn emit_entries(
    ctx: &mut EmitContext,
    local: &mut LocalValues,
    block: ir::Block,
    plans: &Plans,
    deopts: &mut Vec<PendingDeopt>,
) -> Result<(), CompileError> {
    for guard in plans.entries(block) {
        let (stack, reps) = read_stack(ctx, local, &guard.resume.stack);
        emit_checks(
            ctx,
            plans,
            &[guard.switch],
            &(stack, reps, None),
            &guard.resume,
            deopts,
        )?;
    }
    Ok(())
}

pub(super) fn emit_checks(
    ctx: &mut EmitContext,
    plans: &Plans,
    checks: &[ir::Block],
    exact: &Snapshot,
    resume: &ir::FrameState,
    deopts: &mut Vec<PendingDeopt>,
) -> Result<(), CompileError> {
    for &switch in checks {
        let plan = plans.inline(switch).ok_or(CompileError::BadOperand)?;
        let valid = plan.emit_epoch_matches(ctx.fb);
        let stale = deopt_site(
            ctx.fb,
            resume.pc as usize,
            resume.handlers as usize,
            &exact.0,
            &exact.1,
            deopts,
        );
        let pending = deopts.last_mut().expect("queued switch epoch deopt");
        pending.pc = resume.pc as usize;
        pending.region = None;
        pending.sink_cold.clone_from(&exact.2);
        let current = ctx.fb.create_block();
        ctx.fb.ins().brif(valid, current, &[], stale, &[]);
        ctx.fb.switch_to_block(current);
        ctx.fb.seal_block(current);
    }
    Ok(())
}
