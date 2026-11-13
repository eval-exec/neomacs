//! IR scheduling for inline switch epoch guards.
//!
//! `CheckSwitchEpoch` is an explicit guard schedule over authoritative SSA
//! points, separate from the value-producing instruction stream (like array
//! read proofs). It is built after the final transforming pass, borrowed by
//! emission, and never survives a change to its IR. No CLIF loop recognition
//! or backend load motion is involved. The caller supplies the sites with
//! immutable inline plans; this analysis never inspects Lisp constants.
//!
//! A closed natural loop with one unconditional preheader can reuse its entry
//! witness until a safepoint. Calls and MAY_REENTER/MAY_GC operations block
//! admission; unknown writes also clobber the witness, even without Lisp
//! reentry. Poll is the sole safepoint exception. EVERY Poll in the region schedules
//! the same guards on its serviced successor. Its unserviced successor needs
//! no check. Entry failure resumes the header with the preheader's initial
//! stack; poll failure resumes the Poll's own post-edge frame, after service.
//! Both enter Tier-0, whose next Bswitch consults the current table.
//!
//! Threading: immutable compiler IDs and frame descriptions, no runtime cache.
//! GNU Lisp threads switch at yield points: serviced polls revalidate even
//! when another green thread changed the table. Concurrent unsynchronized
//! table mutation between yield points is a Lisp-level data race, as in GNU.

use std::collections::{HashMap, HashSet};

use super::{cfg::Dominance, licm};
use crate::emacs_core::jit::opt::{ir::*, mem::AliasClass, verify::VerifyError};

/// A guard for one switch, with an exact interpreter continuation. Threading:
/// compilation-owned SSA IDs only, no captured Lisp pointers or runtime state.
#[derive(Clone, Debug)]
pub(crate) struct CheckSwitchEpoch {
    pub(crate) switch: Block,
    pub(crate) resume: FrameState,
}

/// A schedule valid only for the borrowed, final IR. Private maps prevent
/// constructing a hoisted switch without its entry and poll guards.
#[derive(Debug, Default)]
#[must_use = "hoisted dispatches require every scheduled entry and poll guard"]
pub(crate) struct SwitchEpochGuards<'a> {
    entries: HashMap<Block, Vec<CheckSwitchEpoch>>,
    polls: HashMap<Inst, Vec<Block>>,
    hoisted: HashSet<Block>,
    _ir: std::marker::PhantomData<&'a Func>,
}

impl<'a> SwitchEpochGuards<'a> {
    #[doc(hidden)]
    pub fn build(func: &'a Func, sites: &HashSet<Block>) -> Result<Self, VerifyError> {
        let mut result = Self::default();
        if sites.is_empty() || !func.sink_recipes.owners.is_empty() {
            return Ok(result);
        }
        let dom = Dominance::new(func)?;
        let Some(loops) = licm::natural_loops(func, &dom) else {
            return Ok(result);
        };
        let mut owners = vec![None; func.values.len()];
        for (i, block) in func.blocks.iter().enumerate() {
            let owner = Block(i as u32);
            for &param in &block.params {
                owners[param.index()] = Some(owner);
            }
            for &id in &block.insts {
                if let Some(value) = func.insts[id.index()].result {
                    owners[value.index()] = Some(owner);
                }
            }
        }
        for natural in loops {
            let mut switches: Vec<_> = sites.intersection(&natural.members).copied().collect();
            switches.sort_unstable();
            if switches.is_empty()
                || natural.members.iter().any(|b| {
                    func.blocks[b.index()].insts.iter().any(|id| {
                        let inst = &func.insts[id.index()];
                        inst.op != Opcode::Poll
                            && (inst.op.is_safepoint(inst.eff)
                                || AliasClass::Unknown.clobbered_by(inst.eff))
                    })
                })
            {
                continue;
            }
            // Terminal Poll lowering can jump directly to the header; decline
            // such an entry rather than place a guard after that transfer.
            if func.blocks[natural.preheader.index()]
                .insts
                .iter()
                .any(|id| func.insts[id.index()].op == Opcode::Poll)
            {
                continue;
            }
            let header = &func.blocks[natural.header.index()];
            let Some(source) = func
                .source_states
                .get(header.pc as usize)
                .and_then(Option::as_ref)
            else {
                continue;
            };
            let base = &func.frames[source.frame.index()];
            if source.block != natural.header
                || base.pc != header.pc
                || base.parent.is_some()
                || base.site.is_some()
                || base.handlers != 0
                || base.stack != func.entry_stacks[natural.header.index()]
            {
                continue;
            }
            let Term::Jump(edge) = &func.blocks[natural.preheader.index()].term else {
                continue;
            };
            let stack: Option<Vec<_>> = base
                .stack
                .iter()
                .map(|&v| {
                    let actual = func.resolve(v)?;
                    let initial = match func.values[actual.index()].def {
                        ValueDef::Param { block, index } if block == natural.header => {
                            func.resolve(*edge.args.get(index as usize)?)?
                        }
                        _ => actual,
                    };
                    dom.dominates(owners[initial.index()]?, natural.preheader)
                        .then_some(initial)
                })
                .collect();
            let Some(stack) = stack else {
                continue;
            };
            let mut resume = base.clone();
            resume.stack = stack.into();
            for &switch in &switches {
                result
                    .entries
                    .entry(natural.preheader)
                    .or_default()
                    .push(CheckSwitchEpoch {
                        switch,
                        resume: resume.clone(),
                    });
                result.hoisted.insert(switch);
            }
            for &b in &natural.members {
                for &id in &func.blocks[b.index()].insts {
                    if func.insts[id.index()].op == Opcode::Poll {
                        result
                            .polls
                            .entry(id)
                            .or_default()
                            .extend_from_slice(&switches);
                    }
                }
            }
        }
        Ok(result)
    }

    #[doc(hidden)]
    pub fn entries(&self, block: Block) -> &[CheckSwitchEpoch] {
        self.entries.get(&block).map_or(&[], Vec::as_slice)
    }

    #[doc(hidden)]
    pub fn after_poll(&self, poll: Inst) -> &[Block] {
        self.polls.get(&poll).map_or(&[], Vec::as_slice)
    }

    #[doc(hidden)]
    pub fn is_hoisted(&self, switch: Block) -> bool {
        self.hoisted.contains(&switch)
    }
}

static_assertions::assert_impl_all!(CheckSwitchEpoch: Send, Sync);
static_assertions::assert_impl_all!(SwitchEpochGuards<'static>: Send, Sync);

/// A local static table Const used only by this switch (and its source frame).
/// Actual observers/edges must not need the deferred definition. Source-state
/// snapshots are descriptive, so the adapter may omit the switch's two popped
/// operands before reading its exceptional continuation.
pub(crate) fn deferred_table_const(func: &Func, block: Block) -> Option<Inst> {
    let Term::Switch { table, .. } = &func.blocks[block.index()].term else {
        return None;
    };
    let table = func.resolve(*table)?;
    let ValueDef::Inst(id) = func.values[table.index()].def else {
        return None;
    };
    if !matches!(func.insts[id.index()].op, Opcode::Const(_))
        || !func.blocks[block.index()].insts.contains(&id)
    {
        return None;
    }
    let uses = |value: &Value| func.resolve(*value) == Some(table);
    let observes = |frame: FrameId| {
        let mut cursor = Some(frame);
        while let Some(id) = cursor {
            let state = &func.frames[id.index()];
            if state.stack.iter().any(uses) {
                return true;
            }
            cursor = state.parent;
        }
        false
    };
    if func
        .insts
        .iter()
        .any(|i| i.args.iter().any(uses) || i.frame.is_some_and(observes))
    {
        return None;
    }
    for (index, data) in func.blocks.iter().enumerate() {
        if data
            .term
            .edges()
            .iter()
            .any(|edge| edge.args.iter().any(uses))
        {
            return None;
        }
        match &data.term {
            Term::Switch {
                value,
                table: other,
                ..
            } => {
                if uses(value) || (index != block.index() && uses(other)) {
                    return None;
                }
            }
            Term::Return(value) | Term::Branch { flag: value, .. } if uses(value) => return None,
            Term::Deopt(frame) if observes(*frame) => return None,
            Term::Return(_)
            | Term::Branch { .. }
            | Term::Deopt(_)
            | Term::Jump(_)
            | Term::Unreachable => {}
        }
    }
    Some(id)
}
