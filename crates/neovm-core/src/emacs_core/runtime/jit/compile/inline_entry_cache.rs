//! Bounded P2.3 entry-protocol elision in observation-free code.
//!
//! Pure constant callers use physical entry/poll admission: failure resumes
//! the unchanged physical snapshot, where Tier0 decides whether a call occurs.
//! Other pure loop components keep a flag per region until a serviced poll.
//! Asynchronous arrival can be observed at the next retained check (§4.3);
//! deterministic changes cannot cross either proof. Threading: policy and SSA
//! handles belong to one compilation. Generated flags belong to one native
//! activation, never TLS or a shared cache of Lisp state.

use cranelift_codegen::ir::{Block, InstBuilder, types};
use cranelift_frontend::{FunctionBuilder, Variable};

use super::{Cfg, active_numeric_feedback, arith_site_takes_generic, simple_effect};
use crate::emacs_core::bytecode::Op;
use crate::emacs_core::jit::NumericFeedback;
use crate::emacs_core::jit::inline::{FusedBody, RegionKind};
use crate::emacs_core::value::Value;

/// Immutable compile-local policy and optional SSA flags. No runtime cache is
/// shared between mutators, and no generated flag outlives its activation.
pub(crate) struct EntryCache {
    regions: Box<[Option<Variable>]>,
    physical: Box<[bool]>,
}

impl EntryCache {
    pub(crate) fn build(
        fb: &mut FunctionBuilder,
        fused: &FusedBody,
        cfg: &Cfg,
        arity: usize,
        osr_pc: Option<usize>,
        dynamic_prefix: usize,
    ) -> Option<Self> {
        let physical = physical_admission_regions(fused, cfg, arity, osr_pc, dynamic_prefix);
        if physical.iter().any(|&yes| yes) {
            return Some(Self {
                regions: vec![None; fused.regions.len()].into_boxed_slice(),
                physical: physical.into_boxed_slice(),
            });
        }
        let admitted = candidate_regions(fused, cfg, arity, osr_pc, dynamic_prefix);
        if !admitted.iter().any(|&yes| yes) {
            return None;
        }
        let zero = fb.ins().iconst(types::I8, 0);
        let regions = admitted
            .into_iter()
            .map(|yes| {
                yes.then(|| {
                    let var = fb.declare_var(types::I8);
                    fb.def_var(var, zero);
                    var
                })
            })
            .collect();
        Some(Self {
            regions,
            physical: physical.into_boxed_slice(),
        })
    }

    /// Both native entry and every successful service poll must run a physical
    /// admission tripwire before reaching any region selected by this policy.
    pub(crate) fn physical_protocol_admitted(&self) -> bool {
        self.physical.iter().any(|&yes| yes)
    }

    pub(crate) fn elides_constant_entry(&self, region: usize) -> bool {
        self.physical.get(region).copied().unwrap_or(false)
    }

    pub(crate) fn region(&self, region: usize) -> Option<Variable> {
        self.regions.get(region).copied().flatten()
    }

    /// Called on the successful dynamic poll edge, before its target header.
    pub(crate) fn invalidate(&self, fb: &mut FunctionBuilder) {
        if self.physical_protocol_admitted() {
            return; // The poll's physical admission replaces flag invalidation.
        }
        let zero = fb.ins().iconst(types::I8, 0);
        for &var in self.regions.iter().flatten() {
            fb.def_var(var, zero);
        }
    }
}

/// Physical admission is stronger than an SCC flag: every reachable lowering
/// from the actual entry must be observation-free, and every reachable inline
/// frame must be a flat, independently proven constant callee. Thus successful
/// entry/poll admission covers all calls until the next service poll. Failure
/// resumes the physical snapshot rather than raising a premature call error.
pub(crate) fn physical_admission_regions(
    fused: &FusedBody,
    cfg: &Cfg,
    arity: usize,
    osr_pc: Option<usize>,
    dynamic_prefix: usize,
) -> Vec<bool> {
    let rejected = || vec![false; fused.regions.len()];
    let Some(side) = &fused.v2 else {
        return rejected();
    };
    if incompatible_protocol_body(fused) {
        return rejected();
    }
    let entry = osr_pc.unwrap_or(0);
    let depth = osr_pc.map_or(Some(arity), |pc| cfg.entry_depth.get(&pc).copied());
    let Some(depth) = depth else {
        return rejected();
    };
    if fused.region_of.get(entry).is_none_or(Option::is_some) {
        return rejected(); // A physical snapshot cannot start inside a virtual frame.
    }
    let graph = successors(&fused.ops);
    let (component, _) = components(&graph, entry);
    // This policy replaces the validity flag of repeated loop calls. Keep
    // forward-only callers' entry guard at their original call boundary.
    if !graph
        .iter()
        .enumerate()
        .any(|(pc, edges)| component[pc] != usize::MAX && edges.iter().any(|&target| target <= pc))
    {
        return rejected();
    }
    if component
        .iter()
        .enumerate()
        .any(|(pc, &id)| id != usize::MAX && !safe_lowering(fused, pc))
    {
        return rejected();
    }
    let Some(constants) = constant_states(fused, &graph, entry, depth, dynamic_prefix) else {
        return rejected();
    };
    let mut admitted = rejected();
    for (region, frame) in fused.regions.iter().enumerate() {
        if component
            .get(frame.start)
            .is_none_or(|&id| id == usize::MAX)
        {
            continue;
        }
        if frame.parent.is_some()
            || !matches!(side.region_kind[region], RegionKind::Constant)
            || !constants[frame.start]
                .as_ref()
                .and_then(|stack| frame.frame_base.checked_sub(1).and_then(|i| stack.get(i)))
                .is_some_and(|tag| *tag == Some(frame.callee_bits))
        {
            return rejected();
        }
        admitted[region] = true;
    }
    admitted
}

fn incompatible_protocol_body(fused: &FusedBody) -> bool {
    super::jit_force_deopt()
        || fused.v2.as_ref().is_none_or(|side| !side.hof_at.is_empty())
        || fused.ops.iter().any(|op| {
            matches!(
                op,
                Op::Switch
                    | Op::PushConditionCase(_)
                    | Op::PushConditionCaseRaw(_)
                    | Op::PushCatch(_)
                    | Op::PopHandler
            )
        })
}

/// Position the builder in the unchecked path; `finish` joins the body path.
pub(crate) fn begin(fb: &mut FunctionBuilder, valid: Variable) -> Block {
    let body = fb.create_block();
    let check = fb.create_block();
    fb.set_cold_block(check);
    let checked = fb.use_var(valid);
    fb.ins().brif(checked, body, &[], check, &[]);
    fb.switch_to_block(check);
    fb.seal_block(check);
    body
}

pub(crate) fn finish(fb: &mut FunctionBuilder, valid: Variable, body: Block) {
    let one = fb.ins().iconst(types::I8, 1);
    fb.def_var(valid, one);
    fb.ins().jump(body, &[]);
    fb.switch_to_block(body);
    fb.seal_block(body);
}

/// Independent identity proof and effect admission; legacy fuser tags are
/// selection hints and are deliberately not accepted as this proof.
pub(crate) fn candidate_regions(
    fused: &FusedBody,
    cfg: &Cfg,
    arity: usize,
    osr_pc: Option<usize>,
    dynamic_prefix: usize,
) -> Vec<bool> {
    let rejected = || vec![false; fused.regions.len()];
    let Some(side) = &fused.v2 else {
        return rejected();
    };
    if incompatible_protocol_body(fused) {
        return rejected();
    }
    let entry = osr_pc.unwrap_or(0);
    let depth = osr_pc.map_or(Some(arity), |pc| cfg.entry_depth.get(&pc).copied());
    let Some(depth) = depth else {
        return rejected();
    };
    let graph = successors(&fused.ops);
    let (component, sizes) = components(&graph, entry);
    let mut safe = vec![true; sizes.len()];
    for (pc, &id) in component.iter().enumerate() {
        if id != usize::MAX && !safe_lowering(fused, pc) {
            safe[id] = false;
        }
    }
    let constants = constant_states(fused, &graph, entry, depth, dynamic_prefix);
    fused
        .regions
        .iter()
        .enumerate()
        .map(|(region, frame)| {
            let id = component[frame.start];
            if id == usize::MAX
                || !safe[id]
                || !(sizes[id] > 1 || graph[frame.start].contains(&frame.start))
            {
                return false;
            }
            match side.region_kind[region] {
                RegionKind::Named { .. } => false,
                RegionKind::Closure { .. } => true, // fresh source/prefix guards remain
                RegionKind::Constant => constants
                    .as_ref()
                    .and_then(|states| states[frame.start].as_ref())
                    .and_then(|stack| frame.frame_base.checked_sub(1).and_then(|i| stack.get(i)))
                    .is_some_and(|tag| *tag == Some(frame.callee_bits)),
            }
        })
        .collect()
}

#[deny(clippy::wildcard_enum_match_arm)]
fn safe_lowering(fused: &FusedBody, pc: usize) -> bool {
    match fused.ops[pc] {
        Op::Constant(_)
        | Op::Nil
        | Op::True
        | Op::Pop
        | Op::Dup
        | Op::StackRef(_)
        | Op::StackSet(_)
        | Op::DiscardN(_)
        | Op::Goto(_)
        | Op::GotoIfNil(_)
        | Op::GotoIfNotNil(_)
        | Op::GotoIfNilElsePop(_)
        | Op::GotoIfNotNilElsePop(_)
        | Op::Return
        | Op::Null
        | Op::Not
        | Op::Consp
        | Op::Stringp
        | Op::Listp
        | Op::Car
        | Op::Cdr
        | Op::CarSafe
        | Op::CdrSafe => true,
        Op::Add
        | Op::Sub
        | Op::Mul
        | Op::Add1
        | Op::Sub1
        | Op::Negate
        | Op::Eqlsign
        | Op::Lss
        | Op::Gtr
        | Op::Leq
        | Op::Geq
        | Op::Max
        | Op::Min => {
            // Exact lowering conditions: neither a resident/boxed float nor
            // the generic arithmetic shim can survive this admission.
            active_numeric_feedback(pc) == NumericFeedback::FixnumOnly
                && !arith_site_takes_generic(&fused.ops[pc], pc)
        }
        // Only the virtual-frame hook has a guarded no-Lisp store; an
        // ordinary caller store reaches the builtin shim and is rejected.
        Op::Setcar | Op::Setcdr => fused.region_of[pc].is_some(),
        Op::VarRef(..)
        | Op::VarSet(..)
        | Op::VarBind(..)
        | Op::Unbind(..)
        | Op::Call(..)
        | Op::Apply(..)
        | Op::Switch
        | Op::Div
        | Op::Rem
        | Op::Cons
        | Op::List(..)
        | Op::Length
        | Op::Nth
        | Op::Nthcdr
        | Op::Elt
        | Op::Nconc
        | Op::Nreverse
        | Op::Member
        | Op::Memq
        | Op::Assq
        | Op::Symbolp
        | Op::Integerp
        | Op::Numberp
        | Op::Eq
        | Op::Equal
        | Op::Concat(..)
        | Op::Substring
        | Op::StringEqual
        | Op::StringLessp
        | Op::Aref
        | Op::Aset
        | Op::SymbolValue
        | Op::SymbolFunction
        | Op::Set
        | Op::Fset
        | Op::Get
        | Op::Put
        | Op::PushConditionCase(..)
        | Op::PushConditionCaseRaw(..)
        | Op::PushCatch(..)
        | Op::PopHandler
        | Op::UnwindProtectPop
        | Op::Throw
        | Op::SaveCurrentBuffer
        | Op::SaveExcursion
        | Op::SaveRestriction
        | Op::SaveWindowExcursion
        | Op::MakeClosure(..)
        | Op::CallBuiltin(..)
        | Op::CallBuiltinSym(..)
        | Op::TrapOutOfRangeConstant(..) => false,
    }
}

fn successors(ops: &[Op]) -> Vec<Vec<usize>> {
    ops.iter()
        .enumerate()
        .map(|(pc, op)| {
            let mut next = match op {
                Op::Return | Op::Throw => Vec::new(),
                Op::Goto(target) => vec![*target as usize],
                Op::GotoIfNil(target)
                | Op::GotoIfNotNil(target)
                | Op::GotoIfNilElsePop(target)
                | Op::GotoIfNotNilElsePop(target) => vec![*target as usize, pc + 1],
                _ => vec![pc + 1],
            };
            next.retain(|&target| target < ops.len());
            next
        })
        .collect()
}

/// Iterative Kosaraju traversal keeps large bytecode units off the Rust stack.
fn components(graph: &[Vec<usize>], entry: usize) -> (Vec<usize>, Vec<usize>) {
    let mut seen = vec![false; graph.len()];
    let mut order = Vec::new();
    let mut work = vec![(entry, false)];
    while let Some((pc, done)) = work.pop() {
        if pc >= graph.len() {
            continue;
        }
        if done {
            order.push(pc);
        } else if !seen[pc] {
            seen[pc] = true;
            work.push((pc, true));
            work.extend(graph[pc].iter().rev().map(|&pc| (pc, false)));
        }
    }
    let mut reverse = vec![Vec::new(); graph.len()];
    for (pc, targets) in graph.iter().enumerate() {
        if seen[pc] {
            for &target in targets {
                reverse[target].push(pc);
            }
        }
    }
    let mut component = vec![usize::MAX; graph.len()];
    let mut sizes = Vec::new();
    for &root in order.iter().rev() {
        if component[root] != usize::MAX {
            continue;
        }
        let id = sizes.len();
        let mut size = 0;
        let mut work = vec![root];
        while let Some(pc) = work.pop() {
            if component[pc] != usize::MAX {
                continue;
            }
            component[pc] = id;
            size += 1;
            work.extend(reverse[pc].iter().copied());
        }
        sizes.push(size);
    }
    (component, sizes)
}

type ConstantStack = Vec<Option<u64>>;

fn constant_states(
    fused: &FusedBody,
    graph: &[Vec<usize>],
    entry: usize,
    depth: usize,
    dynamic_prefix: usize,
) -> Option<Vec<Option<ConstantStack>>> {
    let mut states = vec![None; graph.len()];
    *states.get_mut(entry)? = Some(vec![None; depth]);
    let mut work = vec![entry];
    while let Some(pc) = work.pop() {
        let mut stack = states[pc].as_ref()?.clone();
        match fused.ops[pc] {
            Op::Constant(index) => {
                let index = index as usize;
                let captured = fused.region_at(pc).is_some_and(|frame| {
                    let region = fused.region_of[pc].expect("region");
                    match fused.v2.as_ref().expect("v2").region_kind[region] {
                        RegionKind::Closure { prefix, const_base } => {
                            pc >= frame.start && index >= const_base && index < const_base + prefix
                        }
                        RegionKind::Constant | RegionKind::Named { .. } => false,
                    }
                });
                stack.push(if index < dynamic_prefix || captured {
                    None
                } else {
                    Some(fused.constants.get(index)?.bits() as u64)
                });
            }
            Op::Nil => stack.push(Some(Value::NIL.bits() as u64)),
            Op::True => stack.push(Some(Value::T.bits() as u64)),
            Op::Dup => stack.push(*stack.last()?),
            Op::StackRef(n) => stack.push(*stack.get(stack.len().checked_sub(n as usize + 1)?)?),
            Op::StackSet(n) => {
                let top = stack.pop()?;
                if n != 0 {
                    let at = stack.len().checked_sub(n as usize)?;
                    stack[at] = top;
                }
            }
            Op::DiscardN(raw) => {
                let n = (raw & 0x7f) as usize;
                if raw & 0x80 != 0 && n > 0 {
                    let at = stack.len().checked_sub(n + 1)?;
                    stack[at] = *stack.last()?;
                }
                stack.truncate(stack.len().checked_sub(n)?);
            }
            Op::Goto(_) | Op::Return | Op::Throw => {}
            Op::GotoIfNil(_) | Op::GotoIfNotNil(_) => {
                stack.pop()?;
            }
            Op::GotoIfNilElsePop(_) | Op::GotoIfNotNilElsePop(_) => {}
            ref op => {
                let (needs, delta) = simple_effect(op).ok()?;
                stack.truncate(stack.len().checked_sub(needs)?);
                stack.extend(std::iter::repeat_n(
                    None,
                    usize::try_from(needs as i64 + delta).ok()?,
                ));
            }
        }
        for &target in &graph[pc] {
            let mut incoming = stack.clone();
            if matches!(
                fused.ops[pc],
                Op::GotoIfNilElsePop(_) | Op::GotoIfNotNilElsePop(_)
            ) && target == pc + 1
            {
                incoming.pop()?;
            }
            let changed = match &mut states[target] {
                None => {
                    states[target] = Some(incoming);
                    true
                }
                Some(current) => {
                    if current.len() != incoming.len() {
                        return None;
                    }
                    let mut changed = false;
                    for (old, new) in current.iter_mut().zip(incoming) {
                        if *old != new && old.is_some() {
                            *old = None;
                            changed = true;
                        }
                    }
                    changed
                }
            };
            if changed {
                work.push(target);
            }
        }
    }
    Some(states)
}

#[cfg(test)]
#[path = "inline_entry_cache/tests/admission_test.rs"]
mod tests;
