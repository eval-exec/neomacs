//! C4 closed executor: all function operands stay in checked owner homes.
use super::*;
use crate::emacs_core::intern::SymId;
use crate::tagged::gc::{
    FunctionActivation, FunctionCallCache, FunctionError, FunctionStep, SymbolHandle,
};

pub(super) fn check_tape(
    ops: &[Op],
    nslots: usize,
    functions: Option<usize>,
) -> Result<(), Reject> {
    for &op in ops {
        if functions.is_some() && matches!(op, Op::Cons { .. }) {
            return Err(Reject::Excluded("function session allocation"));
        }
        if let Op::Function(op) = op {
            let count = functions.ok_or(Reject::Excluded("function handles"))?;
            match op {
                FunctionOp::Function { symbol, dst } | FunctionOp::Fmakunbound { symbol, dst } => {
                    slot_index(symbol, count)?;
                    slot_index(dst, nslots)?;
                }
                FunctionOp::Fset { symbol, src, dst } => {
                    slot_index(symbol, count)?;
                    slot_index(src, nslots)?;
                    slot_index(dst, nslots)?;
                }
                FunctionOp::CallBegin { symbol } => {
                    slot_index(symbol, count)?;
                }
                FunctionOp::CallStep { dst, .. } => {
                    slot_index(dst, nslots)?;
                }
            }
        }
    }
    Ok(())
}

pub(super) fn abstract_step(
    op: FunctionOp,
    slots: &mut [Abs],
    selected: &mut bool,
) -> Result<(), Reject> {
    let (dst, answer) = match op {
        FunctionOp::Function { dst, .. } => (dst, Abs::Top),
        FunctionOp::Fset { src, dst, .. } => (dst, slots[slot_index(src, slots.len())?]),
        FunctionOp::Fmakunbound { dst, .. } => (dst, Abs::Sym),
        FunctionOp::CallBegin { .. } => {
            if *selected {
                return Err(Reject::RejectedType);
            }
            *selected = true;
            return Ok(());
        }
        FunctionOp::CallStep { dst, .. } => {
            if !*selected {
                return Err(Reject::RejectedType);
            }
            *selected = false;
            (dst, Abs::Top)
        }
    };
    let index = slot_index(dst, slots.len())?;
    slots[index] = answer;
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
    root_capacity: usize,
    bound: u32,
}
struct Session {
    handles: Vec<SymbolHandle>,
    programs: Vec<Program>,
}
struct Frame<'era> {
    program: usize,
    slots: Vec<OwnerRootSlot<'era>>,
    cache: FunctionCallCache<'era>,
    activation: FunctionActivation<'era>,
    machine: Machine,
    attempts_left: u32,
}
#[derive(Debug, PartialEq, Eq)]
enum Step {
    Done,
    Safepoint,
    Progress,
    Retry,
    Yielded,
}
impl From<FunctionError> for ExecError {
    fn from(error: FunctionError) -> Self {
        match error {
            FunctionError::Authority(error) => Self::Registration(error),
            error => Self::Function(error),
        }
    }
}
#[derive(Debug, PartialEq, Eq)]
enum AdmissionError {
    Program(Reject),
    Authority(RegistrationError),
    Stop(RendezvousError),
    Function(FunctionError),
}
impl Session {
    fn admit(
        era: &ParallelHeapEra<'_>,
        ids: &[SymId],
        sources: &[Source],
    ) -> Result<Self, AdmissionError> {
        let mut programs = Vec::with_capacity(sources.len());
        // Check EVERY tape before preparing anything or registering an owner.
        for source in sources {
            let root_capacity = source
                .nslots
                .checked_add(2)
                .ok_or(AdmissionError::Program(Reject::Slot))?;
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
                TapeCapabilities::Functions { symbols: ids.len() },
            )
            .map_err(AdmissionError::Program)?;
            programs.push(Program {
                ops,
                nslots: source.nslots,
                root_capacity,
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
        stopped.trace(|_, _| Ok(())).map_err(AdmissionError::Stop)?;
        stopped
            .complete()
            .map_err(|failure| AdmissionError::Stop(failure.reason))?;
        if !closed {
            return Err(AdmissionError::Program(Reject::RejectedType));
        }
        for handle in &handles {
            era.prepare_function_symbol(handle)
                .map_err(AdmissionError::Function)?;
        }
        Ok(Self { handles, programs })
    }

    fn root_capacity(&self, program: usize) -> Result<usize, ExecError> {
        self.programs
            .get(program)
            .map(|p| p.root_capacity)
            .ok_or(ExecError::Slot)
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
        with_run(owner, |run| {
            let slots = (0..spec.nslots)
                .map(|_| run.root_slot(nil).map_err(ExecError::from))
                .collect::<Result<Vec<_>, _>>()?;
            let cache = FunctionCallCache::new(run)?;
            let activation = FunctionActivation::new(run)?;
            Ok(Frame {
                program,
                slots,
                cache,
                activation,
                machine: Machine::new(),
                attempts_left: STEP_CAP,
            })
        })
    }

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
                let op = *program.ops.get(frame.machine.pc).ok_or(ExecError::Slot)?;
                // The slice and all its loans are closed before this access.
                match with_run(owner, |run| self.apply(run, frame, op)) {
                    Ok(false) => Ok(Step::Yielded),
                    Ok(true) => {
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
            Step::Done | Step::Safepoint | Step::Progress | Step::Yielded => {
                frame.attempts_left -= 1;
            }
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
            step @ (Step::Done | Step::Safepoint | Step::Progress | Step::Yielded) => Ok(step),
        })
    }

    fn apply<'era>(
        &self,
        run: &mut RunAccess<'_, 'era>,
        frame: &mut Frame<'era>,
        op: Op,
    ) -> Result<bool, ExecError> {
        match op {
            Op::Function(FunctionOp::CallBegin { symbol }) => {
                let handle = self
                    .handles
                    .get(usize::from(symbol))
                    .ok_or(ExecError::Slot)?;
                run.sym_call_begin(handle, &mut frame.cache, &mut frame.activation)?;
            }
            Op::Function(FunctionOp::CallStep { dst, fuel }) => {
                let mut dst = frame
                    .slots
                    .get(usize::from(dst))
                    .ok_or(ExecError::Slot)?
                    .clone();
                return run
                    .sym_call_step(&mut frame.activation, &mut dst, fuel)
                    .map(|step| step == FunctionStep::Returned)
                    .map_err(ExecError::from);
            }
            Op::Function(op) => {
                let (symbol, dst) = match op {
                    FunctionOp::Function { symbol, dst }
                    | FunctionOp::Fset { symbol, dst, .. }
                    | FunctionOp::Fmakunbound { symbol, dst } => (symbol, dst),
                    _ => return Err(ExecError::Type),
                };
                let handle = self
                    .handles
                    .get(usize::from(symbol))
                    .ok_or(ExecError::Slot)?;
                let mut dst = frame
                    .slots
                    .get(usize::from(dst))
                    .ok_or(ExecError::Slot)?
                    .clone();
                match op {
                    FunctionOp::Function { .. } => run.sym_function_into(handle, &mut dst)?,
                    FunctionOp::Fset { src, .. } => {
                        let src = frame.slots.get(usize::from(src)).ok_or(ExecError::Slot)?;
                        run.sym_fset_into(handle, src, &mut dst)?;
                    }
                    FunctionOp::Fmakunbound { .. } => run.sym_fmakunbound_into(handle, &mut dst)?,
                    _ => return Err(ExecError::Type),
                }
            }
            Op::Global(op) => {
                let (symbol, dst) = match op {
                    GlobalOp::Value { symbol, dst }
                    | GlobalOp::Set { symbol, dst, .. }
                    | GlobalOp::Boundp { symbol, dst }
                    | GlobalOp::Makunbound { symbol, dst } => (symbol, dst),
                };
                let handle = self
                    .handles
                    .get(usize::from(symbol))
                    .ok_or(ExecError::Slot)?;
                let mut dst = frame
                    .slots
                    .get(usize::from(dst))
                    .ok_or(ExecError::Slot)?
                    .clone();
                let result = match op {
                    GlobalOp::Value { .. } => run.sym_value_into(handle, &mut dst),
                    GlobalOp::Set { src, .. } => {
                        let src = frame.slots.get(usize::from(src)).ok_or(ExecError::Slot)?;
                        run.sym_set_into(handle, src, &mut dst)
                    }
                    GlobalOp::Boundp { .. } => run.sym_boundp_into(handle, &mut dst),
                    GlobalOp::Makunbound { .. } => run.sym_makunbound_into(handle, &mut dst),
                };
                result.map_err(|error| match error {
                    crate::tagged::gc::GlobalError::Authority(error) => {
                        ExecError::Registration(error)
                    }
                    crate::tagged::gc::GlobalError::VoidVariable => ExecError::VoidVariable,
                })?;
            }
            _ => return Err(ExecError::Type),
        }
        Ok(true)
    }
}

mod tests;

mod handoff;
