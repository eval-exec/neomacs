//! C5 closed executor. Frozen exact names enter only through admitted input
//! globals; every name operand and symbol answer stays in an owner home.
use super::*;
use crate::emacs_core::intern::SymId;
use crate::tagged::gc::{GlobalError, InitialInternError, SymbolHandle};

use crate::tagged::gc::mutator_registration::symbol_functions::retry::{
    self as contention, Deadline, Retry, retry, retry_owned,
};
use std::ops::ControlFlow;

pub(super) fn check_tape(
    ops: &[Op],
    nslots: usize,
    capabilities: TapeCapabilities,
) -> Result<(), Reject> {
    for &op in ops {
        if let Op::Intern(op) = op {
            match capabilities {
                TapeCapabilities::InitialIntern { .. } => {}
                TapeCapabilities::Core
                | TapeCapabilities::Globals { .. }
                | TapeCapabilities::Functions { .. } => {
                    return Err(Reject::Excluded("initial obarray"));
                }
            }
            let (name, dst) = operands(op);
            slot_index(name, nslots)?;
            slot_index(dst, nslots)?;
        }
        match capabilities {
            TapeCapabilities::InitialIntern { .. } => {}
            TapeCapabilities::Core
            | TapeCapabilities::Globals { .. }
            | TapeCapabilities::Functions { .. } => continue,
        }
        // Scan ALL operands, including unreachable instructions. No string or
        // intern answer can reach the older global-session Car/Cdr exception.
        match op {
            Op::Intern(_) | Op::Safepoint => {}
            Op::Global(
                GlobalOp::Value { .. } | GlobalOp::Boundp { .. } | GlobalOp::Makunbound { .. },
            ) => {}
            Op::ConstNil { dst } | Op::ConstT { dst } => {
                slot_index(dst, nslots)?;
            }
            Op::ConstFixnum { dst, value } => {
                slot_index(dst, nslots)?;
                Fixnum::try_from(value).map_err(|_| Reject::Overflow)?;
            }
            Op::Move { dst, src } => {
                slot_index(dst, nslots)?;
                slot_index(src, nslots)?;
            }
            Op::Add { dst, a, b }
            | Op::Sub { dst, a, b }
            | Op::Lt { dst, a, b }
            | Op::Eq { dst, a, b } => {
                slot_index(dst, nslots)?;
                slot_index(a, nslots)?;
                slot_index(b, nslots)?;
            }
            Op::BranchIfNil { cond, target } => {
                slot_index(cond, nslots)?;
                target_index(target, ops.len())?;
            }
            Op::Jump { target } => {
                target_index(target, ops.len())?;
            }
            Op::Global(GlobalOp::Set { .. })
            | Op::Function(_)
            | Op::Cons { .. }
            | Op::Car { .. }
            | Op::Cdr { .. }
            | Op::SetCar { .. }
            | Op::SetCdr { .. }
            | Op::HtGet { .. }
            | Op::HtPut { .. }
            | Op::HtRem { .. }
            | Op::HtClear { .. }
            | Op::HtMake { .. }
            | Op::HtCopy { .. }
            | Op::HtCount { .. } => {
                return Err(Reject::Excluded("intern closed operations"));
            }
        }
    }
    Ok(())
}

fn operands(op: InternOp) -> (u16, u16) {
    match op {
        InternOp::InitialIntern { name, dst } | InternOp::InitialInternSoft { name, dst } => {
            (name, dst)
        }
    }
}

pub(super) fn abstract_step(op: InternOp, slots: &mut [Abs]) -> Result<(), Reject> {
    let (name, dst) = operands(op);
    slot_index(name, slots.len())?;
    let dst = slot_index(dst, slots.len())?;
    // Intern can return NIL for the spelling "nil". Neither operation is
    // assumed truthy. Wrong-type inputs reach the checked rooted method.
    slots[dst] = Abs::Top;
    Ok(())
}

impl From<InitialInternError> for ExecError {
    fn from(error: InitialInternError) -> Self {
        match error {
            InitialInternError::Authority(error) => Self::Registration(error),
            InitialInternError::ExpectedString => Self::Intern(InitialInternError::ExpectedString),
            InitialInternError::ExpectedPlainSymbol => {
                Self::Intern(InitialInternError::ExpectedPlainSymbol)
            }
            InitialInternError::UnsupportedName => {
                Self::Intern(InitialInternError::UnsupportedName)
            }
        }
    }
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
struct Session {
    handles: Vec<SymbolHandle>,
    programs: Vec<Program>,
}
struct Frame<'era> {
    program: usize,
    slots: Vec<OwnerRootSlot<'era>>,
    machine: Machine,
    steps_left: u32,
    pending_deadline: Deadline,
}
#[derive(Debug, PartialEq, Eq)]
enum AdmissionError {
    Program(Reject),
    Authority(RegistrationError),
    Stop(RendezvousError),
}
#[derive(Debug, PartialEq, Eq)]
enum Step {
    Done,
    Safepoint,
    Progress,
    Retry,
}

/// Only an observable retry outcome is contention. A terminal step error,
/// including the pending instruction's expired Busy, must return unchanged.
enum PendingStepError {
    Contended,
    Refused(ExecError),
}

impl Session {
    fn admit(
        era: &ParallelHeapEra<'_>,
        ids: &[SymId],
        sources: &[Source],
    ) -> Result<Self, AdmissionError> {
        if ids.is_empty() || ids.len() > STEP_CAP as usize {
            return Err(AdmissionError::Program(Reject::RejectedType));
        }
        let mut programs = Vec::with_capacity(sources.len());
        for source in sources {
            if source.nslots > usize::from(u16::MAX) + 1 {
                return Err(AdmissionError::Program(Reject::Slot));
            }
            let ops = lower(&source.insns).map_err(AdmissionError::Program)?;
            prevalidate_inner(
                &ops,
                source.nslots,
                source.bound,
                STEP_CAP,
                &[],
                TapeCapabilities::InitialIntern { globals: ids.len() },
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
            .initial_names_closed_for_test(&handles, STEP_CAP as usize)
            .map_err(AdmissionError::Authority)?;
        stopped.trace(|_, _| Ok(())).map_err(AdmissionError::Stop)?;
        stopped
            .complete()
            .map_err(|failed| AdmissionError::Stop(failed.reason))?;
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
        let spec = self.programs.get(program).ok_or(ExecError::Slot)?;
        let slots = with_run(owner, |run| {
            (0..spec.nslots)
                .map(|_| run.root_slot(nil).map_err(ExecError::from))
                .collect::<Result<Vec<_>, _>>()
        })?;
        Ok(Frame {
            program,
            slots,
            machine: Machine::new(),
            steps_left: STEP_CAP,
            pending_deadline: Deadline::contention(),
        })
    }

    fn step<'era>(
        &self,
        owner: &mut Mutator<'era>,
        frame: &mut Frame<'era>,
    ) -> Result<Step, ExecError> {
        if frame.steps_left == 0 {
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
        });
        let result = match flow {
            Ok(Flow::Done) => Ok(Step::Done),
            Ok(Flow::Safepoint) => Ok(Step::Safepoint),
            Ok(Flow::Lease) => {
                let op = *program.ops.get(frame.machine.pc).ok_or(ExecError::Slot)?;
                // with_slice and all branded operands have ended. A lease is
                // rooted, bounded and cannot retain authority across a retry.
                with_run(owner, |run| self.apply(run, &frame.slots, op)).map(|()| {
                    frame.machine.pc += 1;
                    Step::Progress
                })
            }
            Err(error) => Err(error),
        };
        match result {
            Err(ExecError::Registration(RegistrationError::Busy)) => {
                // A pending instruction retains one elapsed deadline. Busy
                // consumes no program step and changes neither PC nor roots.
                frame
                    .pending_deadline
                    .check(ExecError::Registration(RegistrationError::Busy))?;
                Ok(Step::Retry)
            }
            Ok(step) => {
                frame.steps_left -= 1;
                frame.pending_deadline = Deadline::contention();
                Ok(step)
            }
            Err(error) => Err(error),
        }
    }

    fn wait_step<'era>(
        &self,
        owner: &mut Mutator<'era>,
        frame: &mut Frame<'era>,
    ) -> Result<Step, ExecError> {
        retry(
            Deadline::contention(),
            || match self.step(owner, frame).map_err(PendingStepError::Refused)? {
                Step::Retry => Err(PendingStepError::Contended),
                step @ (Step::Done | Step::Safepoint | Step::Progress) => Ok(step),
            },
            |error| match error {
                PendingStepError::Contended => Retry::Busy,
                PendingStepError::Refused(_) => Retry::Refuse,
            },
        )
        .map_err(|error| match error {
            PendingStepError::Contended => ExecError::Registration(RegistrationError::Busy),
            PendingStepError::Refused(error) => error,
        })
    }

    fn apply<'era>(
        &self,
        run: &mut RunAccess<'_, 'era>,
        slots: &[OwnerRootSlot<'era>],
        op: Op,
    ) -> Result<(), ExecError> {
        match op {
            Op::Intern(op) => {
                let (name, dst) = operands(op);
                let name = slots.get(usize::from(name)).ok_or(ExecError::Slot)?;
                let mut dst = slots.get(usize::from(dst)).ok_or(ExecError::Slot)?.clone();
                match op {
                    InternOp::InitialIntern { .. } => run.initial_intern_into(name, &mut dst)?,
                    InternOp::InitialInternSoft { .. } => {
                        run.initial_intern_soft_into(name, &mut dst)?
                    }
                }
            }
            Op::Global(op) => {
                let (symbol, dst) = match op {
                    GlobalOp::Value { symbol, dst }
                    | GlobalOp::Boundp { symbol, dst }
                    | GlobalOp::Makunbound { symbol, dst } => (symbol, dst),
                    GlobalOp::Set { .. } => return Err(ExecError::Type),
                };
                let handle = self
                    .handles
                    .get(usize::from(symbol))
                    .ok_or(ExecError::Slot)?;
                let mut dst = slots.get(usize::from(dst)).ok_or(ExecError::Slot)?.clone();
                let result = match op {
                    GlobalOp::Value { .. } => run.sym_value_into(handle, &mut dst),
                    GlobalOp::Boundp { .. } => run.sym_boundp_into(handle, &mut dst),
                    GlobalOp::Makunbound { .. } => run.sym_makunbound_into(handle, &mut dst),
                    GlobalOp::Set { .. } => return Err(ExecError::Type),
                };
                result.map_err(|error| match error {
                    GlobalError::Authority(error) => ExecError::Registration(error),
                    GlobalError::VoidVariable => ExecError::VoidVariable,
                })?;
            }
            Op::Function(_)
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
            | Op::SetCar { .. }
            | Op::SetCdr { .. }
            | Op::HtGet { .. }
            | Op::HtPut { .. }
            | Op::HtRem { .. }
            | Op::HtClear { .. }
            | Op::HtMake { .. }
            | Op::HtCopy { .. }
            | Op::HtCount { .. }
            | Op::Safepoint => return Err(ExecError::Type),
        }
        Ok(())
    }
}

mod tests;
