//! C3 test executor: whole-session admission, then slot-to-slot lease ops.
//! The session owns its immutable tapes/handle table. Initial owner homes are
//! nil; all later values come from checked canonical graphs or closed ops.
use super::*;
use crate::emacs_core::intern::SymId;
use crate::tagged::gc::{GlobalError, SymbolHandle};

pub(super) fn check_tape(ops: &[Op], nslots: usize, globals: Option<usize>) -> Result<(), Reject> {
    for &op in ops {
        match op {
            Op::Global(op) => {
                let count = globals.ok_or(Reject::Excluded("symbol handles"))?;
                let (symbol, src, dst) = operands(op);
                slot_index(symbol, count)?;
                slot_index(dst, nslots)?;
                if let Some(src) = src {
                    slot_index(src, nslots)?;
                }
            }
            Op::SetCar { .. }
            | Op::SetCdr { .. }
            | Op::HtGet { .. }
            | Op::HtPut { .. }
            | Op::HtRem { .. }
            | Op::HtClear { .. }
            | Op::HtMake { .. }
            | Op::HtCopy { .. }
            | Op::HtCount { .. } => {
                if globals.is_some() {
                    return Err(Reject::Excluded("global session mutation/table"));
                }
            }
            Op::Function(_)
            | Op::Intern(_)
            | Op::ConstNil { .. }
            | Op::ConstT { .. }
            | Op::ConstFixnum { .. }
            | Op::Move { .. }
            | Op::Add { .. }
            | Op::Sub { .. }
            | Op::Lt { .. }
            | Op::Eq { .. }
            | Op::BranchIfNil { .. }
            | Op::Jump { .. }
            | Op::Cons { .. }
            | Op::Car { .. }
            | Op::Cdr { .. }
            | Op::Safepoint => {}
        }
    }
    Ok(())
}

fn operands(op: GlobalOp) -> (u16, Option<u16>, u16) {
    match op {
        GlobalOp::Value { symbol, dst }
        | GlobalOp::Boundp { symbol, dst }
        | GlobalOp::Makunbound { symbol, dst } => (symbol, None, dst),
        GlobalOp::Set { symbol, src, dst } => (symbol, Some(src), dst),
    }
}

pub(super) fn abstract_step(op: GlobalOp, slots: &mut [Abs]) -> Result<(), Reject> {
    let (_, src, dst) = operands(op);
    let result = match op {
        GlobalOp::Value { .. } | GlobalOp::Boundp { .. } => Abs::Top,
        GlobalOp::Makunbound { .. } => Abs::Sym,
        GlobalOp::Set { .. } => slots[slot_index(src.ok_or(Reject::Slot)?, slots.len())?],
    };
    let dst = slot_index(dst, slots.len())?;
    slots[dst] = result;
    Ok(())
}

#[derive(Clone)]
struct Source {
    insns: Vec<Insn>,
    nslots: usize,
    bound: u32,
}

struct Program {
    ops: Vec<Op>,
    nslots: usize,
    bound: u32,
}

struct GlobalSession {
    handles: Vec<SymbolHandle>,
    programs: Vec<Program>,
}

#[derive(Debug, PartialEq, Eq)]
enum AdmissionError {
    Program(Reject),
    Authority(RegistrationError),
    Stop(RendezvousError),
}

impl GlobalSession {
    /// No participants may exist: real handle admission enforces that
    /// precondition. Every participating tape is checked before registration.
    fn admit(
        era: &ParallelHeapEra<'_>,
        ids: &[SymId],
        sources: &[Source],
    ) -> Result<Self, AdmissionError> {
        let mut programs = Vec::with_capacity(sources.len());
        for source in sources {
            let ops = lower(&source.insns).map_err(AdmissionError::Program)?;
            prevalidate_inner(
                &ops,
                source.nslots,
                source.bound,
                STEP_CAP,
                &[],
                TapeCapabilities::Globals { symbols: ids.len() },
            )
            .map_err(AdmissionError::Program)?;
            programs.push(Program {
                ops,
                nslots: source.nslots,
                bound: source.bound,
            });
        }
        let handles = ids
            .iter()
            .map(|&id| era.admit_plain_symbol(id))
            .collect::<Result<Vec<_>, _>>()
            .map_err(AdmissionError::Authority)?;
        let mut stopped = era.stop().map_err(AdmissionError::Stop)?;
        let closed = stopped
            .symbols()
            .map_err(AdmissionError::Stop)?
            .plain_graphs_closed_for_test(&handles, STEP_CAP as usize)
            .map_err(AdmissionError::Authority)?;
        // Unsupported input is an honest admission refusal, not poison.
        stopped.trace(|_, _| Ok(())).map_err(AdmissionError::Stop)?;
        stopped
            .complete()
            .map_err(|failure| AdmissionError::Stop(failure.reason))?;
        if !closed {
            return Err(AdmissionError::Program(Reject::RejectedType));
        }
        Ok(Self { handles, programs })
    }

    fn seed<'era>(
        &self,
        owner: &mut Mutator<'era>,
        program: usize,
        nil: &SharedRoot,
    ) -> Result<Frame<'era>, ExecError> {
        if nil.parallel_word() != TaggedValue::NIL.bits() {
            return Err(ExecError::Type);
        }
        let program_spec = self.programs.get(program).ok_or(ExecError::Slot)?;
        let slots = with_run(owner, |run| {
            (0..program_spec.nslots)
                .map(|_| run.root_slot(nil).map_err(ExecError::from))
                .collect::<Result<Vec<_>, _>>()
        })?;
        Ok(Frame {
            program,
            slots,
            machine: Machine::new(),
            attempts_left: STEP_CAP,
        })
    }

    /// A refusal returns with no RunAccess, word, slice or loan held. The
    /// caller may yield or service a stop before retrying the unchanged pc.
    fn step<'era>(
        &self,
        owner: &mut Mutator<'era>,
        frame: &mut Frame<'era>,
    ) -> Result<Step, ExecError> {
        if frame.attempts_left == 0 {
            return Err(ExecError::Backedge);
        }
        let program = self.programs.get(frame.program).ok_or(ExecError::Slot)?;
        let flow = with_run(owner, |run| {
            run.with_slice(|slice| {
                let mut homes = frame
                    .slots
                    .iter()
                    .map(|slot| slice.bind(slot))
                    .collect::<Result<Vec<_>, _>>()?;
                exec(
                    slice,
                    &mut homes,
                    &program.ops,
                    &mut frame.machine,
                    program.bound,
                    None,
                    0,
                )
            })
            .map_err(ExecError::from)?
        })?;
        let step = match flow {
            Flow::Done => Ok(Step::Done),
            Flow::Safepoint => Ok(Step::Safepoint),
            Flow::Lease => {
                let Some(Op::Global(op)) = program.ops.get(frame.machine.pc).copied() else {
                    return Err(ExecError::Type);
                };
                match with_run(owner, |run| self.apply(run, &frame.slots, op)) {
                    Ok(()) => {
                        frame.machine.pc += 1;
                        Ok(Step::Progress)
                    }
                    Err(ExecError::Registration(RegistrationError::Busy)) => Ok(Step::Retry),
                    Err(error) => Err(error),
                }
            }
        }?;
        match step {
            Step::Retry => {}
            Step::Done | Step::Safepoint | Step::Progress => frame.attempts_left -= 1,
        }
        Ok(step)
    }

    /// Wait for protocol contention without charging the instruction budget.
    fn wait_step<'era>(
        &self,
        owner: &mut Mutator<'era>,
        frame: &mut Frame<'era>,
    ) -> Result<Step, ExecError> {
        retry_exec(|| match self.step(owner, frame)? {
            Step::Retry => Err(ExecError::Registration(RegistrationError::Busy)),
            step @ (Step::Done | Step::Safepoint | Step::Progress) => Ok(step),
        })
    }

    fn apply<'era>(
        &self,
        run: &mut RunAccess<'_, 'era>,
        slots: &[OwnerRootSlot<'era>],
        op: GlobalOp,
    ) -> Result<(), ExecError> {
        let (symbol, src, dst) = operands(op);
        let handle = self
            .handles
            .get(usize::from(symbol))
            .ok_or(ExecError::Slot)?;
        let mut dst = slots.get(usize::from(dst)).ok_or(ExecError::Slot)?.clone();
        let result = match op {
            GlobalOp::Value { .. } => run.sym_value_into(handle, &mut dst),
            GlobalOp::Boundp { .. } => run.sym_boundp_into(handle, &mut dst),
            GlobalOp::Makunbound { .. } => run.sym_makunbound_into(handle, &mut dst),
            GlobalOp::Set { .. } => {
                let src = slots
                    .get(usize::from(src.ok_or(ExecError::Slot)?))
                    .ok_or(ExecError::Slot)?;
                run.sym_set_into(handle, src, &mut dst)
            }
        };
        result.map_err(|error| match error {
            GlobalError::Authority(error) => ExecError::Registration(error),
            GlobalError::VoidVariable => ExecError::VoidVariable,
        })
    }
}

struct Frame<'era> {
    program: usize,
    slots: Vec<OwnerRootSlot<'era>>,
    machine: Machine,
    attempts_left: u32,
}

#[derive(Debug, PartialEq, Eq)]
enum Step {
    Done,
    Safepoint,
    Progress,
    Retry,
}

mod gc;
mod handoff;
mod tests;
mod void;
