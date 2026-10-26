//! Owner-local straight-line execution through the sealed kernels only.
use super::*;
use crate::tagged::gc::mutator_registration::symbol_authority::{
    check_owner_home_for_test,
    combined_inputs::{
        AdmittedCombinedInputs, CombinedInput, CombinedInputError, CombinedInputLimit,
    },
};
use crate::tagged::gc::{
    FunctionActivation, FunctionCallCache, FunctionError, FunctionStep, GlobalError,
    InitialInternError, SymbolHandle,
};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct ProgramIndex(u16);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct InstructionOffset(u32);
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct FrameRootCapacity(NonZeroU32);
struct RemainingSteps(u32);
enum PendingInstruction {
    Unstarted,
    Waiting(Deadline),
}
enum SeedBoundary {
    Complete,
    RefuseAfterCache,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum AdmissionError {
    Tape(TapeError),
    Input(CombinedInputError),
    Function(FunctionError),
    Authority(RegistrationError),
    Stop(RendezvousError),
    Programs,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SeedError {
    Authority(RegistrationError),
    Function(FunctionError),
}
impl From<RegistrationError> for SeedError {
    fn from(error: RegistrationError) -> Self {
        Self::Authority(error)
    }
}
impl From<FunctionError> for SeedError {
    fn from(error: FunctionError) -> Self {
        Self::Function(error)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Error {
    Authority(RegistrationError),
    Global(GlobalError),
    Function(FunctionError),
    Intern(InitialInternError),
    Program,
    Input,
    Register,
    StepBudget,
    OccupiedFrame,
    ForeignSession,
    Excluded(ExcludedOperation),
    Seed(SeedError),
    SeedCleanup {
        cause: SeedError,
        cleanup: RegistrationError,
    },
}
macro_rules! error_from {
    ($ty:ty, $variant:ident) => {
        impl From<$ty> for Error {
            fn from(error: $ty) -> Self {
                Self::$variant(error)
            }
        }
    };
}
error_from!(RegistrationError, Authority);
error_from!(GlobalError, Global);
error_from!(FunctionError, Function);
error_from!(InitialInternError, Intern);

pub(super) fn classify(error: &Error) -> Retry {
    match error {
        Error::Authority(error) => contention::registration(error),
        Error::Global(error) => contention::global(error),
        Error::Function(error) => contention::function(error),
        Error::Intern(error) => contention::initial_intern(error),
        Error::Seed(SeedError::Authority(error)) => contention::registration(error),
        Error::Seed(SeedError::Function(error)) => contention::function(error),
        Error::Program
        | Error::Input
        | Error::Register
        | Error::StepBudget
        | Error::OccupiedFrame
        | Error::ForeignSession
        | Error::Excluded(_)
        | Error::SeedCleanup { .. } => Retry::Refuse,
    }
}

struct SessionData {
    inputs: AdmittedCombinedInputs,
    programs: Box<[CombinedTape]>,
}
/// This shared allocation is metadata only, including its frozen input receipt.
/// It owns no GC words and is never published as a substitute root family.
#[must_use]
#[derive(Clone)]
pub(super) struct Session(Arc<SessionData>);

#[must_use = "release the root frame before retiring its owner"]
pub(super) struct Frame<'era> {
    session: Arc<SessionData>,
    program: ProgramIndex,
    pc: InstructionOffset,
    remaining: RemainingSteps,
    pending: PendingInstruction,
    slots: Vec<OwnerRootSlot<'era>>,
    cache: FunctionCallCache<'era>,
    activation: FunctionActivation<'era>,
}
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Step {
    Done,
    Progress,
    Yielded,
    Safepoint,
    Retry,
}
#[derive(Debug, PartialEq, Eq)]
pub(super) enum ReadyStep {
    Done,
    Progress,
    Yielded,
    Safepoint,
}
enum WaitingError {
    Contended,
    Refused(Error),
}

impl InputIndex {
    pub(super) fn checked(index: u16, inputs: &[CombinedInput]) -> Result<Self, TapeError> {
        inputs.get(usize::from(index)).ok_or(TapeError::Input)?;
        Ok(Self(index))
    }
}
impl ProgramIndex {
    pub(super) fn checked(index: u16, session: &Session) -> Result<Self, Error> {
        session
            .0
            .programs
            .get(usize::from(index))
            .ok_or(Error::Program)?;
        Ok(Self(index))
    }
}
impl FrameRootCapacity {
    pub(super) fn as_usize(self) -> usize {
        self.0.get() as usize
    }
}
impl<'era> Frame<'era> {
    pub(super) fn home(&self, register: RootRegister) -> Result<&OwnerRootSlot<'era>, Error> {
        self.slots
            .get(usize::from(register.0))
            .ok_or(Error::Register)
    }
    pub(super) fn position(&self) -> InstructionOffset {
        self.pc
    }
    pub(super) fn expire_pending_for_test(&mut self) {
        self.pending = PendingInstruction::Waiting(Deadline::expired_for_test());
    }
}

impl Session {
    pub(super) fn admit(
        era: &ParallelHeapEra<'_>,
        target: &SymbolHandle,
        inputs: &[CombinedInput],
        programs: Vec<CombinedTape>,
    ) -> Result<Self, AdmissionError> {
        if programs.is_empty() || programs.len() > usize::from(u16::MAX) + 1 {
            return Err(AdmissionError::Programs);
        }
        // Recheck EVERY tape/input position before any preparation or owner.
        // No source can smuggle generic VM operations or mutation exceptions.
        for program in &programs {
            program.validate().map_err(AdmissionError::Tape)?;
            for instruction in &program.instructions {
                match *instruction {
                    CombinedInstruction::GlobalToHome { input, .. }
                    | CombinedInstruction::HomeToGlobal { input, .. } => {
                        InputIndex::checked(input.0, inputs).map_err(AdmissionError::Tape)?;
                    }
                    CombinedInstruction::HomeToFunction { .. }
                    | CombinedInstruction::CallBegin
                    | CombinedInstruction::CallStep { .. }
                    | CombinedInstruction::ClearCache
                    | CombinedInstruction::ClearActivation
                    | CombinedInstruction::ClearHome { .. }
                    | CombinedInstruction::InitialIntern { .. }
                    | CombinedInstruction::InitialSoft { .. }
                    | CombinedInstruction::Safepoint => {}
                    CombinedInstruction::Excluded(excluded) => {
                        return Err(AdmissionError::Tape(TapeError::Excluded(excluded)));
                    }
                }
            }
        }
        let limit = CombinedInputLimit::try_from(inputs.len()).map_err(AdmissionError::Input)?;
        let target = era
            .prepare_name_function_symbol(target)
            .map_err(AdmissionError::Function)?;
        let mut stopped = era.stop().map_err(AdmissionError::Stop)?;
        let admitted = stopped
            .symbols()
            .map_err(AdmissionError::Stop)
            .and_then(|keeper| {
                keeper
                    .admit_combined_inputs(&target, inputs, limit)
                    .map_err(AdmissionError::Input)
            });
        // A shape/identity refusal is outside the callback and remains recoverable.
        // A real callback failure is terminal; this callback always succeeds.
        stopped.trace(|_, _| Ok(())).map_err(AdmissionError::Stop)?;
        stopped
            .complete()
            .map_err(|failed| AdmissionError::Stop(failed.reason))?;
        Ok(Self(Arc::new(SessionData {
            inputs: admitted?,
            programs: programs.into(),
        })))
    }

    pub(super) fn root_capacity(&self, program: ProgramIndex) -> Result<FrameRootCapacity, Error> {
        let capacity = self
            .0
            .programs
            .get(usize::from(program.0))
            .ok_or(Error::Program)?
            .capacity;
        let capacity = capacity.0.get().checked_add(2).ok_or(Error::Register)?;
        NonZeroU32::new(capacity)
            .map(FrameRootCapacity)
            .ok_or(Error::Register)
    }

    pub(super) fn seed<'era>(
        &self,
        owner: &mut Mutator<'era>,
        program: ProgramIndex,
    ) -> Result<Frame<'era>, Error> {
        self.seed_at(owner, program, SeedBoundary::Complete)
    }

    pub(super) fn seed_refusal_after_cache_for_test<'era>(
        &self,
        owner: &mut Mutator<'era>,
        program: ProgramIndex,
    ) -> Result<Frame<'era>, Error> {
        self.seed_at(owner, program, SeedBoundary::RefuseAfterCache)
    }

    fn seed_at<'era>(
        &self,
        owner: &mut Mutator<'era>,
        program: ProgramIndex,
        boundary: SeedBoundary,
    ) -> Result<Frame<'era>, Error> {
        let spec = self
            .0
            .programs
            .get(usize::from(program.0))
            .ok_or(Error::Program)?;
        let capacity = self.root_capacity(program)?.as_usize();
        let mut run = owner.run_access()?;
        self.0.inputs.validate_running(&run)?;
        let local = run
            .owner
            .local
            .as_ref()
            .ok_or(RegistrationError::StaleRegistration)?;
        if !local.root_scratch.is_empty() {
            return Err(Error::OccupiedFrame);
        }
        if capacity > local.root_slot_limit || capacity > local.root_scratch.capacity() {
            return Err(RegistrationError::Capacity.into());
        }
        let built = (|| -> Result<Frame<'era>, SeedError> {
            let mut slots = Vec::new();
            slots
                .try_reserve_exact(spec.capacity.0.get() as usize)
                .map_err(|_| RegistrationError::Capacity)?;
            for _ in 0..spec.capacity.0.get() {
                slots.push(run.private_nil_root()?);
            }
            let cache = FunctionCallCache::new(&mut run)?;
            match boundary {
                SeedBoundary::Complete => {}
                SeedBoundary::RefuseAfterCache => return Err(RegistrationError::Capacity.into()),
            }
            Ok(Frame {
                session: Arc::clone(&self.0),
                program,
                pc: InstructionOffset(0),
                remaining: RemainingSteps(spec.steps.0.get()),
                pending: PendingInstruction::Unstarted,
                slots,
                cache,
                activation: FunctionActivation::new(&mut run)?,
            })
        })();
        match built {
            Ok(frame) => Ok(frame),
            Err(cause) => match run.clear_root_slots() {
                Ok(()) => Err(Error::Seed(cause)),
                Err(cleanup) => Err(Error::SeedCleanup { cause, cleanup }),
            },
        }
    }

    pub(super) fn step<'era>(
        &self,
        owner: &mut Mutator<'era>,
        frame: &mut Frame<'era>,
    ) -> Result<Step, Error> {
        if !Arc::ptr_eq(&self.0, &frame.session) {
            return Err(Error::ForeignSession);
        }
        let program = self
            .0
            .programs
            .get(usize::from(frame.program.0))
            .ok_or(Error::Program)?;
        match &frame.pending {
            PendingInstruction::Unstarted => {}
            PendingInstruction::Waiting(deadline) => {
                deadline.check(Error::Authority(RegistrationError::Busy))?
            }
        }
        let mut run = match owner.run_access() {
            Ok(run) => run,
            Err(error) => return frame.refusal(Error::Authority(error)),
        };
        // Even Done/Safepoint validates the real owner/generation. No frame
        // metadata or a same-index foreign bank grants access by itself.
        check_owner_home_for_test(&run, frame.slots.first().ok_or(Error::Register)?)?;
        let Some(instruction) = program.instructions.get(frame.pc.0 as usize) else {
            return Ok(Step::Done);
        };
        let next = InstructionOffset(frame.pc.0.checked_add(1).ok_or(Error::Program)?);
        let remaining = RemainingSteps(frame.remaining.0.checked_sub(1).ok_or(Error::StepBudget)?);
        match self.apply(&mut run, frame, *instruction) {
            Ok(step) => {
                frame.pc = next;
                frame.remaining = remaining;
                frame.pending = PendingInstruction::Unstarted;
                Ok(step)
            }
            Err(error) => frame.refusal(error),
        }
    }

    pub(super) fn wait_step<'era>(
        &self,
        owner: &mut Mutator<'era>,
        frame: &mut Frame<'era>,
    ) -> Result<ReadyStep, Error> {
        contention::retry(
            Deadline::contention(),
            || match self.step(owner, frame).map_err(WaitingError::Refused)? {
                Step::Done => Ok(ReadyStep::Done),
                Step::Progress => Ok(ReadyStep::Progress),
                Step::Yielded => Ok(ReadyStep::Yielded),
                Step::Safepoint => Ok(ReadyStep::Safepoint),
                Step::Retry => Err(WaitingError::Contended),
            },
            |error| match error {
                WaitingError::Contended => Retry::Busy,
                WaitingError::Refused(_) => Retry::Refuse,
            },
        )
        .map_err(|error| match error {
            WaitingError::Contended => Error::Authority(RegistrationError::Busy),
            WaitingError::Refused(error) => error,
        })
    }

    fn input(&self, input: InputIndex) -> Result<SymbolHandle, Error> {
        self.0
            .inputs
            .inputs()
            .get(usize::from(input.0))
            .map(|input| input.key())
            .ok_or(Error::Input)
    }

    fn apply<'era>(
        &self,
        run: &mut RunAccess<'_, 'era>,
        frame: &mut Frame<'era>,
        instruction: CombinedInstruction,
    ) -> Result<Step, Error> {
        match instruction {
            CombinedInstruction::GlobalToHome { input, destination } => {
                let mut destination = frame.home(destination)?.clone();
                run.sym_value_into(&self.input(input)?, &mut destination)?;
            }
            CombinedInstruction::HomeToGlobal {
                input,
                source,
                destination,
            } => {
                let mut destination = frame.home(destination)?.clone();
                run.sym_set_into(&self.input(input)?, frame.home(source)?, &mut destination)?;
            }
            CombinedInstruction::HomeToFunction {
                source,
                destination,
            } => {
                let mut destination = frame.home(destination)?.clone();
                run.sym_name_fset_into(
                    &self.0.inputs.target(),
                    frame.home(source)?,
                    &mut destination,
                )?;
            }
            CombinedInstruction::CallBegin => run.sym_name_call_begin(
                &self.0.inputs.target(),
                &mut frame.cache,
                &mut frame.activation,
            )?,
            CombinedInstruction::CallStep { destination } => {
                let mut destination = frame.home(destination)?.clone();
                match run.sym_call_step(&mut frame.activation, &mut destination, NonZeroU32::MIN)? {
                    FunctionStep::Yielded => return Ok(Step::Yielded),
                    FunctionStep::Returned => {}
                }
            }
            CombinedInstruction::ClearCache => frame.cache.clear(run)?,
            CombinedInstruction::ClearActivation => frame.activation.clear(run)?,
            CombinedInstruction::ClearHome { home } => {
                run.clear_owner_root_for_test(frame.home(home)?)?;
            }
            CombinedInstruction::InitialIntern { name, destination } => {
                let mut destination = frame.home(destination)?.clone();
                run.initial_intern_into(frame.home(name)?, &mut destination)?;
            }
            CombinedInstruction::InitialSoft { name, destination } => {
                let mut destination = frame.home(destination)?.clone();
                run.initial_intern_soft_into(frame.home(name)?, &mut destination)?;
            }
            CombinedInstruction::Safepoint => return Ok(Step::Safepoint),
            CombinedInstruction::Excluded(operation) => return Err(Error::Excluded(operation)),
        }
        Ok(Step::Progress)
    }
}
impl Frame<'_> {
    fn refusal(&mut self, error: Error) -> Result<Step, Error> {
        match classify(&error) {
            Retry::Refuse => Err(error),
            Retry::Busy => {
                match &self.pending {
                    PendingInstruction::Unstarted => {
                        self.pending = PendingInstruction::Waiting(Deadline::contention())
                    }
                    PendingInstruction::Waiting(deadline) => deadline.check(error)?,
                }
                Ok(Step::Retry)
            }
        }
    }
}

static_assertions::assert_impl_all!(Session: Send, Sync, Clone);
static_assertions::assert_not_impl_any!(Frame<'static>: Send, Sync, Clone, Copy);
static_assertions::assert_eq_size!(ProgramIndex, u16);
static_assertions::assert_eq_size!(InstructionOffset, u32);
static_assertions::assert_eq_size!(FrameRootCapacity, u32);
