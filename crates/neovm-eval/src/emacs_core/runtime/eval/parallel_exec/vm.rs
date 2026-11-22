//! Stage-1 parallel bytecode stepper for the E4 §5 VM roster
//! (VM-EXECUTOR-DESIGN r1), harness only.
//!
//! One frame runs on the owned [`FrameArena`], classified by the generated
//! dispatch table (`opcode_slot`). Scalar opcodes step the frame's operand
//! segment owner-locally; the eight sealed cons opcodes call [`VmSealed`];
//! the fifteen arithmetic opcodes call [`ArithOps`] (R6) and escape only
//! when an operand is a marker; every other World arm, `Return` included,
//! is exactly ONE escape through [`VmEscape`], after the [`VmPoll`] edges
//! r1 §6 names.
//!
//! An escape completion is a typed contract, checked before anything is
//! applied: the operand counts against [`static_stack_effect`] (R3), and the
//! serial record tokens against the top run of each LIFO (R1). A `Flow` from
//! an escape ends the run and nothing resumes over it (R2): handler landing
//! pads are an E6-P prerequisite, not a silent default. A contract fault
//! keeps the scene it describes and refuses further work.
//!
//! Every [`Value`] that must survive a poll is reachable from the published
//! arena: the frame's segment is the arena's operand vector, and a step
//! never holds a value across a poll. The escape handler reads the operands
//! through the arena with the frame and cursor the request carries.
//! This module starts no executor thread and is not reachable from serial
//! dispatch.

#![allow(dead_code)]

use super::dispatch::opcode_slot;
use super::engine::{CoreOutcome, EngineFault, RecordStack};
use super::frames::{
    FrameArena, FrameId, HandlerMark, OperandCursor, OwnerRecordMark, Pc, PollBudget, RootGroup,
    RootHandle, RootRevokeError, SerialHandlerToken, SerialRecordToken, VmFrame,
};
use crate::emacs_core::bytecode::{ArithGenericKind, Op};
use crate::emacs_core::eval::Context;
use crate::emacs_core::eval::world::poll::PollOutcome;
use crate::emacs_core::subr::effect_schema::{
    ArithOperands, ParallelArm, SealedAuthority, WorldSplit,
};
use neovm_value::error::{Flow, signal};
use neovm_value::tagged::gc::RendezvousError;
use neovm_value::value::Value;
use std::num::NonZeroU32;

const FRAME_DEPTH_LIMIT: u32 = 32;

/// The operands one opcode consumes and produces on its fall-through edge.
/// For `Return` the single produced operand is the frame's value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct StackEffect {
    pub(crate) consumed: u16,
    pub(crate) produced: u16,
}

/// A second stack-effect table beside the serial verifier's
/// (`decode::verify_stack_effects_with_tables`): `(consumed, produced)` per
/// opcode, where the verifier's `(min, net)` is `(min, min + net)`. The row
/// `vm_static_stack_effect_agrees_with_the_serial_verifier` ties the two
/// black-box over every `Op` variant. Where the table differs from the
/// verifier, the verifier records nothing: it `continue`s on the opcodes
/// that end a flow, and `Return` here carries the frame's value to its
/// caller (`Breturn`: `val = TOP` … `TOP = val`, bytecode.c:897-921) and
/// `Throw` consumes tag and value (`vm.rs`, the serial `Throw` arm).
pub(crate) fn static_stack_effect(op: &Op) -> StackEffect {
    /// The verifier's `usize` arithmetic, kept in `u16` without a panic on a
    /// payload at the type's edge.
    fn wide(value: u32) -> u16 {
        u16::try_from(value).unwrap_or(u16::MAX)
    }
    let (consumed, produced): (u16, u16) = match op {
        Op::Constant(_) | Op::Nil | Op::True | Op::VarRef(_) | Op::MakeClosure(_) => (0, 1),
        Op::Pop | Op::VarSet(_) | Op::VarBind(_) | Op::UnwindProtectPop => (1, 0),
        Op::Dup => (1, 2),
        Op::StackRef(n) => (wide(u32::from(*n) + 1), wide(u32::from(*n) + 2)),
        Op::StackSet(n) => (wide(u32::from(*n) + 1), *n),
        Op::DiscardN(raw) => {
            let n = u16::from(raw & 0x7F);
            if n == 0 {
                (0, 0)
            } else if raw & 0x80 != 0 {
                (n + 1, 1)
            } else {
                (n, 0)
            }
        }
        Op::TrapOutOfRangeConstant(_) | Op::Unbind(_) | Op::PopHandler | Op::Goto(_) => (0, 0),
        // The fall-through edge; the taken edge of each `*ElsePop` keeps TOS.
        Op::GotoIfNil(_)
        | Op::GotoIfNotNil(_)
        | Op::GotoIfNilElsePop(_)
        | Op::GotoIfNotNilElsePop(_) => (1, 0),
        Op::Call(n) | Op::Apply(n) => (wide(u32::from(*n) + 1), 1),
        // `Bswitch` pops the table and the key (bytecode.c:1754-1757) and a
        // hit leaves through `op_branch`, which pushes nothing.
        Op::Switch | Op::Throw => (2, 0),
        Op::Return => (1, 1),
        Op::List(n) | Op::Concat(n) => (*n, 1),
        Op::CallBuiltin(_, n) | Op::CallBuiltinSym(_, n) => (u16::from(*n), 1),
        Op::SaveCurrentBuffer
        | Op::SaveExcursion
        | Op::SaveRestriction
        | Op::PushConditionCase(_) => (0, 0),
        Op::PushCatch(_) | Op::PushConditionCaseRaw(_) => (1, 0),
        // In place.
        Op::Add1
        | Op::Sub1
        | Op::Negate
        | Op::Car
        | Op::Cdr
        | Op::CarSafe
        | Op::CdrSafe
        | Op::Length
        | Op::Symbolp
        | Op::Consp
        | Op::Stringp
        | Op::Listp
        | Op::Integerp
        | Op::Numberp
        | Op::Null
        | Op::Not
        | Op::Nreverse
        | Op::SymbolValue
        | Op::SymbolFunction
        | Op::SaveWindowExcursion => (1, 1),
        // Two operands, one result.
        Op::Add
        | Op::Sub
        | Op::Mul
        | Op::Div
        | Op::Rem
        | Op::Eqlsign
        | Op::Gtr
        | Op::Lss
        | Op::Leq
        | Op::Geq
        | Op::Max
        | Op::Min
        | Op::Cons
        | Op::Nth
        | Op::Nthcdr
        | Op::Setcar
        | Op::Setcdr
        | Op::Elt
        | Op::Nconc
        | Op::Member
        | Op::Memq
        | Op::Assq
        | Op::Eq
        | Op::Equal
        | Op::StringEqual
        | Op::StringLessp
        | Op::Aref
        | Op::Set
        | Op::Fset
        | Op::Get => (2, 1),
        Op::Aset | Op::Put | Op::Substring => (3, 1),
    };
    StackEffect { consumed, produced }
}

/// The production E2 safepoint capability at the VM's edges (r1 §6).
///
/// [`VmPoll::vm_back_edge`] runs once per backward branch, the serial
/// `branch_to!` quitcounter edge of GNU `op_branch` (bytecode.c:861-866).
/// [`VmPoll::vm_call`] runs before `Call`, `Apply`, `CallBuiltin` and
/// `CallBuiltinSym`: GNU `Bcall`'s `maybe_quit` (bytecode.c:781).
/// [`VmPoll::serial_request`] runs before every escape. Publication rule as
/// on `engine::OwnerPoll`: a `Value` must not be carried unrooted across a
/// call.
pub(crate) trait VmPoll {
    fn vm_back_edge(&mut self, arena: &FrameArena) -> Result<PollOutcome, RendezvousError>;

    fn vm_call(&mut self, arena: &FrameArena) -> Result<PollOutcome, RendezvousError>;

    fn serial_request(&mut self, arena: &FrameArena) -> Result<PollOutcome, RendezvousError>;
}

/// The sealed cons authorities this slice runs through the serial core:
/// `ConsFieldRead` (`car`, `cdr`, `car-safe`, `cdr-safe`),
/// `ConsAllocation` (`cons`, `list`) and `ConsFieldWrite` (`setcar`,
/// `setcdr`). Every other sealed authority escapes
/// ([`EscapeWhy::UnimplementedSealed`]).
pub(crate) trait VmSealed {
    fn read_cons_field(
        &mut self,
        ctx: &mut Context,
        name: &'static str,
        args: &[Value],
    ) -> CoreOutcome;

    fn write_cons_field(
        &mut self,
        ctx: &mut Context,
        name: &'static str,
        args: &[Value],
    ) -> CoreOutcome;

    fn allocate_cons(
        &mut self,
        ctx: &mut Context,
        name: &'static str,
        args: &[Value],
    ) -> CoreOutcome;
}

/// Why the owner-local arithmetic port produced no value.
#[derive(Debug)]
pub(crate) enum ArithRefusal {
    /// An operand is a marker: its number is a buffer position, so the
    /// opcode takes the ONE escape as `Arith(MarkerInvolved)` (R6).
    MarkerInvolved,
    /// The builtin's `Flow`: a wrong-type operand, an integer zero divisor
    /// (§2.4).
    Flow(Flow),
}

/// The owner-local arithmetic port (E4-DESIGN R6; the approved `ArithOps`
/// row). `operands` is the opcode's operand run in stack order, exactly its
/// static `consumed` count. The port walks them once: a marker is
/// [`ArithRefusal::MarkerInvolved`], a wrong-type operand or an integer zero
/// divisor the builtin's [`ArithRefusal::Flow`]; GNU checks each operand
/// where it stands (`check_number_coerce_marker`, data.c:2697-2703;
/// `check_integer_coerce_marker`, :2688-2694). Otherwise the result is the
/// builtin's, floats and bignums allocated through the owner's regions.
/// There is no poll: the bytecode arithmetics are straight-line
/// (bytecode.c:1244-1410), a fixnum fast path or a direct `Fplus`/`Fquo`/
/// `Frem` call, no `maybe_quit`.
pub(crate) trait ArithOps {
    fn arith(&mut self, kind: ArithGenericKind, operands: &[Value]) -> Result<Value, ArithRefusal>;
}

/// Why an opcode escaped.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum EscapeWhy {
    World(WorldSplit),
    UnimplementedSealed(SealedAuthority),
}

/// One escape request. The handler reads the operands through the arena
/// with `frame` and `operands`; nothing is copied out of the published
/// segment, and `pc` is the escaping opcode's own position.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmEscapeRequest {
    pub op: Op,
    pub why: EscapeWhy,
    pub pc: Pc,
    pub frame: FrameId,
    pub operands: OperandCursor,
}

/// What the serial side did with the operands.
#[derive(Debug)]
pub(crate) enum EscapeStack {
    /// The fall-through effect. `jump` is a branch target inside the same
    /// code, taken in place of the next pc; only `Switch` reports one (a hit
    /// leaves through `op_branch`, bytecode.c:1752-1758). Any other opcode
    /// reporting a jump is a [`EngineFault::DispatchContract`]. V8's handler
    /// landing pads get their own shape.
    Completed {
        consumed: u16,
        produced: Vec<Value>,
        jump: Option<u32>,
    },
    /// The serial side signaled or threw.
    Flow(Flow),
}

/// The serial binding records one escape removed and added. `popped` is
/// innermost first and must be the LIFO's top run; `pushed` is outermost
/// first.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct RecordDelta {
    pub popped: Vec<SerialRecordToken>,
    pub pushed: Vec<SerialRecordToken>,
}

/// The serial handler records one escape removed and added; order and rule
/// are [`RecordDelta`]'s.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct HandlerDelta {
    pub popped: Vec<SerialHandlerToken>,
    pub pushed: Vec<SerialHandlerToken>,
}

/// One escape's result (r1 R1/R3): the operand effect and the record LIFO
/// deltas.
#[derive(Debug)]
pub(crate) struct EscapeCompletion {
    pub stack: EscapeStack,
    pub records: RecordDelta,
    pub handlers: HandlerDelta,
}

impl EscapeCompletion {
    /// The completion a fall-through escape reports: the opcode's static
    /// effect with nil operands, no jump and no record change.
    pub fn fall_through(op: &Op) -> Self {
        let StackEffect { consumed, produced } = static_stack_effect(op);
        Self {
            stack: EscapeStack::Completed {
                consumed,
                produced: vec![Value::NIL; produced as usize],
                jump: None,
            },
            records: RecordDelta::default(),
            handlers: HandlerDelta::default(),
        }
    }
}

/// The serial side of an escape. The production impl arrives with E5.
pub(crate) trait VmEscape {
    fn escape(&mut self, request: VmEscapeRequest, arena: &FrameArena) -> EscapeCompletion;
}

/// The lowered code one run executes.
#[derive(Clone, Copy)]
pub(crate) struct VmProgram<'a> {
    /// The instruction stream. A sealed program ends in `Return`.
    pub ops: &'a [Op],
    /// The pool `Constant` indexes. It must be reachable from `code` in
    /// production; the harness owns its fixture.
    pub constants: &'a [Value],
    pub function: Value,
    pub code: Value,
}

/// Where one run is. `Idle` is the only state in which [`VmExec::run`]
/// accepts work; `Finished` holds the done root until [`VmExec::take`].
/// `Poisoned` means an unwind was refused or a contract fault was raised:
/// the leftover stays put.
enum RunState {
    Idle,
    Finished { handle: RootHandle },
    Poisoned,
}

/// How a run ended.
#[derive(Debug)]
pub(crate) enum VmOutcome {
    /// The `Return` escape completed. `pending` holds the frame's value.
    Done {
        pending: RootHandle,
    },
    Signaled(Flow),
    MissingCore {
        name: &'static str,
    },
    Fault(EngineFault),
}

enum Step {
    Continue,
    Stop(VmOutcome),
}

/// The harness VM stepper. `E` is the escape handler, `S` the sealed-ops
/// port, `A` the arithmetic port and `P` the poll capability; tests supply
/// the doubles.
pub(crate) struct VmExec<E, S, A, P> {
    arena: FrameArena,
    escape: E,
    sealed: S,
    arith: A,
    poll: P,
    frame: Option<FrameId>,
    state: RunState,
}

fn invalid() -> Step {
    // The serial VM's `invalid_bytecode_flow`: the reason goes to its trace
    // site, the signal carries the same single datum.
    Step::Stop(VmOutcome::Signaled(signal(
        "error",
        vec![Value::string("Invalid byte-code")],
    )))
}

/// The serial core's name for each sealed cons opcode, under the authority
/// the dispatch table assigned it. `None` when an opcode reaches the sealed
/// step with a shape this slice has no core for.
fn sealed_core_name(op: &Op) -> Option<&'static str> {
    match op {
        Op::Car => Some("car"),
        Op::Cdr => Some("cdr"),
        Op::CarSafe => Some("car-safe"),
        Op::CdrSafe => Some("cdr-safe"),
        Op::Cons => Some("cons"),
        Op::List(_) => Some("list"),
        Op::Setcar => Some("setcar"),
        Op::Setcdr => Some("setcdr"),
        _ => None,
    }
}

impl<E, S, A, P> VmExec<E, S, A, P>
where
    E: VmEscape,
    S: VmSealed,
    A: ArithOps,
    P: VmPoll,
{
    pub(crate) fn new(escape: E, sealed: S, arith: A, poll: P) -> Self {
        Self {
            arena: FrameArena::new(FRAME_DEPTH_LIMIT),
            escape,
            sealed,
            arith,
            poll,
            frame: None,
            state: RunState::Idle,
        }
    }

    pub(crate) fn escape_handler(&self) -> &E {
        &self.escape
    }

    pub(crate) fn arith_handler(&self) -> &A {
        &self.arith
    }

    pub(crate) fn poll_handler(&self) -> &P {
        &self.poll
    }

    pub(crate) fn arena(&self) -> &FrameArena {
        &self.arena
    }

    pub(crate) fn arena_depth(&self) -> u32 {
        self.arena.depth()
    }

    pub(crate) fn live_root_count(&self) -> usize {
        self.arena.root_len()
    }

    /// The frame the current run pushed, while it is pushed.
    pub(crate) fn frame_id(&self) -> Option<FrameId> {
        self.frame
    }

    pub(crate) fn is_idle(&self) -> bool {
        matches!(self.state, RunState::Idle)
    }

    pub(crate) fn lookup_root(&mut self, handle: RootHandle) -> Option<Value> {
        self.arena.roots_mut().lookup(handle)
    }

    /// Pop the root a finished run published. A handle that is not that
    /// root is refused, and the registry is left as it was.
    pub(crate) fn take(&mut self, handle: RootHandle) -> Result<Value, RootRevokeError> {
        let RunState::Finished { handle: done } = self.state else {
            return Err(RootRevokeError::Stale);
        };
        if done != handle {
            return Err(RootRevokeError::Stale);
        }
        let value = self.arena.roots_mut().pop(handle)?;
        self.state = RunState::Idle;
        Ok(value)
    }

    /// Run `program` to its `Return`, a signal, or a fault.
    ///
    /// One poll budget of one is copied into the frame; V1 polls at every
    /// named edge and does not consume it.
    pub(crate) fn run(&mut self, ctx: &mut Context, program: VmProgram<'_>) -> VmOutcome {
        if matches!(self.state, RunState::Poisoned) {
            return VmOutcome::Fault(EngineFault::Poisoned);
        }
        if !matches!(self.state, RunState::Idle) {
            return VmOutcome::Fault(EngineFault::Busy);
        }
        let frame = VmFrame::new(
            program.function,
            program.code,
            Pc::new(0),
            OperandCursor::new(0, 0),
            OwnerRecordMark::EMPTY,
            HandlerMark::EMPTY,
            PollBudget::new(NonZeroU32::MIN),
        );
        let id = match self.arena.push_vm(frame) {
            Ok(id) => id,
            Err(error) => return VmOutcome::Fault(EngineFault::Frame(error)),
        };
        self.frame = Some(id);
        loop {
            match self.step(ctx, program) {
                Step::Continue => {}
                Step::Stop(outcome) => return self.settle(outcome),
            }
        }
    }

    /// `Done` already published its root. A signal, a missing core or an
    /// ordinary fault drops the frame and every root this run pushed. A
    /// contract fault keeps the scene it describes: the frame, its segment
    /// and both LIFOs stay as the escape left them, and the engine refuses
    /// further work. It is a defect on one side of the escape, not a Lisp
    /// outcome.
    fn settle(&mut self, outcome: VmOutcome) -> VmOutcome {
        match &outcome {
            VmOutcome::Done { pending } => self.state = RunState::Finished { handle: *pending },
            VmOutcome::Fault(EngineFault::Busy | EngineFault::Poisoned) => {}
            VmOutcome::Fault(
                EngineFault::EscapeStackEffect { .. }
                | EngineFault::EscapeStackUnderflow { .. }
                | EngineFault::EscapeRecordContract(_)
                | EngineFault::DispatchContract(_),
            ) => self.state = RunState::Poisoned,
            VmOutcome::Signaled(_) | VmOutcome::MissingCore { .. } | VmOutcome::Fault(_) => {
                if let Err(fault) = self.unwind_to_idle() {
                    return VmOutcome::Fault(fault);
                }
            }
        }
        outcome
    }

    /// Drop the frame, its operands, every record and every root this run
    /// pushed. A refused pop or truncate sets `Poisoned` and leaves the
    /// leftover in place. `Idle` is set only after every step succeeded.
    fn unwind_to_idle(&mut self) -> Result<(), EngineFault> {
        if let Some(id) = self.frame.take() {
            let _ = self.arena.vm_truncate_operands(id, 0);
            if self.arena.pop(id).is_none() {
                self.state = RunState::Poisoned;
                return Err(EngineFault::StaleFrame);
            }
        }
        // truncate(EMPTY) is correct only because VmExec owns its FrameArena exclusively.
        self.arena.handlers_mut().truncate(HandlerMark::EMPTY);
        self.arena.bindings_mut().truncate(OwnerRecordMark::EMPTY);
        if let Err(error) = self.arena.roots_mut().truncate(0) {
            self.state = RunState::Poisoned;
            return Err(EngineFault::Root(error));
        }
        self.state = RunState::Idle;
        Ok(())
    }

    fn step(&mut self, ctx: &mut Context, program: VmProgram<'_>) -> Step {
        let Some(frame) = self.frame else {
            return Step::Stop(VmOutcome::Fault(EngineFault::NoFrame));
        };
        let Some(vm) = self.arena.vm(frame) else {
            return Step::Stop(VmOutcome::Fault(EngineFault::StaleFrame));
        };
        let pc = vm.pc;
        let operands = vm.operands;
        let branch_pc = pc.get();
        // seal_ops guarantees a sealed program ends in `Return`; anything
        // that runs past the end is malformed.
        let Some(op) = program.ops.get(branch_pc) else {
            return invalid();
        };
        let op = op.clone();
        let slot = opcode_slot(&op);
        match slot.arm {
            ParallelArm::Scalar => self.step_scalar(op, frame, operands, branch_pc, program),
            ParallelArm::Sealed(authority) => {
                self.step_sealed(op, authority, ctx, frame, operands, branch_pc)
            }
            ParallelArm::World(WorldSplit::Arith(_)) => {
                self.step_arith(op, frame, operands, branch_pc, program)
            }
            ParallelArm::World(split) => self.escape(
                op,
                EscapeWhy::World(split),
                frame,
                operands,
                branch_pc,
                program,
            ),
        }
    }

    /// The operand OFFSET slots below the top; 0 is the top.
    fn peek(&self, operands: OperandCursor, offset: usize) -> Option<Value> {
        self.arena
            .operand_segment(operands)
            .len()
            .checked_sub(offset + 1)
            .and_then(|index| self.arena.operand_segment(operands).get(index))
            .copied()
    }

    fn stop(result: Result<(), EngineFault>) -> Step {
        match result {
            Ok(()) => Step::Continue,
            Err(fault) => Step::Stop(VmOutcome::Fault(fault)),
        }
    }

    fn set_pc(&mut self, frame: FrameId, pc: usize) -> Result<(), EngineFault> {
        self.arena
            .with_vm(frame, |vm| vm.pc = Pc::new(pc))
            .ok_or(EngineFault::StaleFrame)
    }

    fn next_pc(&mut self, frame: FrameId, branch_pc: usize) -> Result<(), EngineFault> {
        self.set_pc(frame, branch_pc + 1)
    }

    /// The store of a fall-through step: cut the segment down to KEEP, push
    /// PRODUCED, advance to the next pc.
    fn store(
        &mut self,
        frame: FrameId,
        branch_pc: usize,
        keep: usize,
        produced: &[Value],
    ) -> Result<(), EngineFault> {
        self.arena
            .vm_truncate_operands(frame, keep)
            .ok_or(EngineFault::StaleFrame)?;
        for value in produced {
            self.arena
                .vm_push(frame, *value)
                .ok_or(EngineFault::StaleFrame)?;
        }
        self.next_pc(frame, branch_pc)
    }

    /// A branch to TARGET. The back-edge poll runs once when the target is
    /// at or before the branch: `branch_to!` compares against the pc after
    /// the branch, so `target < pc + 1` is `target <= pc`.
    fn jump(&mut self, frame: FrameId, branch_pc: usize, target: u32, ops_len: usize) -> Step {
        let Ok(target) = usize::try_from(target) else {
            return invalid();
        };
        if target >= ops_len {
            return invalid();
        }
        if let Err(fault) = self.set_pc(frame, target) {
            return Step::Stop(VmOutcome::Fault(fault));
        }
        if target <= branch_pc {
            if let Err(error) = self.poll.vm_back_edge(&self.arena) {
                return Step::Stop(VmOutcome::Fault(EngineFault::Poll(error)));
            }
        }
        Step::Continue
    }

    fn replace_top(
        &mut self,
        frame: FrameId,
        operands: OperandCursor,
        branch_pc: usize,
        test: impl FnOnce(Value) -> bool,
    ) -> Step {
        let Some(top) = self.peek(operands, 0) else {
            return invalid();
        };
        let value = if test(top) { Value::T } else { Value::NIL };
        let result = self
            .arena
            .vm_set_operand(frame, 0, value)
            .ok_or(EngineFault::StaleFrame)
            .and_then(|()| self.next_pc(frame, branch_pc));
        Self::stop(result)
    }

    /// The 19 Scalar opcodes, with the serial `!VERIFIED` semantics
    /// (`bytecode/vm.rs`). An underflow the serial VM reaches only with
    /// malformed code is its `Invalid byte-code` signal here, never a panic.
    fn step_scalar(
        &mut self,
        op: Op,
        frame: FrameId,
        operands: OperandCursor,
        branch_pc: usize,
        program: VmProgram<'_>,
    ) -> Step {
        let len = operands.len();
        let segment = self.arena.operand_segment(operands);
        let ops_len = program.ops.len();
        match op {
            Op::Constant(idx) => {
                // The serial read is unchecked: seal_ops rewrote an
                // out-of-range index into `TrapOutOfRangeConstant`.
                let Some(value) = program.constants.get(idx as usize).copied() else {
                    return invalid();
                };
                Self::stop(self.store(frame, branch_pc, len, &[value]))
            }
            Op::TrapOutOfRangeConstant(_) => invalid(),
            Op::Nil => Self::stop(self.store(frame, branch_pc, len, &[Value::NIL])),
            Op::True => Self::stop(self.store(frame, branch_pc, len, &[Value::T])),
            Op::Pop => {
                if len == 0 {
                    return invalid();
                }
                Self::stop(self.store(frame, branch_pc, len - 1, &[]))
            }
            Op::Dup => {
                let Some(top) = segment.last().copied() else {
                    return invalid();
                };
                Self::stop(self.store(frame, branch_pc, len, &[top]))
            }
            Op::StackRef(n) => {
                // The serial arm reads `stk[len - (1 + n)]`: slot n below
                // the top before the push.
                let Some(value) = self.peek(operands, n as usize) else {
                    return invalid();
                };
                Self::stop(self.store(frame, branch_pc, len, &[value]))
            }
            Op::StackSet(n) => {
                if len == 0 {
                    return invalid();
                }
                let n = n as usize;
                if n == 0 {
                    return Self::stop(self.store(frame, branch_pc, len - 1, &[]));
                }
                if n >= len {
                    return invalid();
                }
                let top = segment[len - 1];
                let result = self
                    .arena
                    .vm_set_operand(frame, n, top)
                    .ok_or(EngineFault::StaleFrame)
                    .and_then(|()| self.store(frame, branch_pc, len - 1, &[]));
                Self::stop(result)
            }
            Op::DiscardN(raw) => {
                let n = (raw & 0x7F) as usize;
                if n == 0 {
                    return Self::stop(self.next_pc(frame, branch_pc));
                }
                if n > len {
                    return invalid();
                }
                let keep = len - n;
                if raw & 0x80 != 0 {
                    if n == len {
                        return invalid();
                    }
                    // The preserved TOS lands on top of the shortened segment.
                    let top = segment[len - 1];
                    Self::stop(self.store(frame, branch_pc, keep - 1, &[top]))
                } else {
                    Self::stop(self.store(frame, branch_pc, keep, &[]))
                }
            }
            Op::Goto(target) => self.jump(frame, branch_pc, target, ops_len),
            Op::GotoIfNil(target) | Op::GotoIfNotNil(target) => {
                let Some(top) = segment.last().copied() else {
                    return invalid();
                };
                let taken = match op {
                    Op::GotoIfNil(_) => top.is_nil(),
                    _ => top.is_truthy(),
                };
                // Both edges pop.
                if let Err(fault) = self.store(frame, branch_pc, len - 1, &[]) {
                    return Step::Stop(VmOutcome::Fault(fault));
                }
                if taken {
                    return self.jump(frame, branch_pc, target, ops_len);
                }
                Step::Continue
            }
            Op::GotoIfNilElsePop(target) | Op::GotoIfNotNilElsePop(target) => {
                let Some(top) = segment.last().copied() else {
                    return invalid();
                };
                let taken = match op {
                    Op::GotoIfNilElsePop(_) => top.is_nil(),
                    _ => top.is_truthy(),
                };
                if taken {
                    // The taken edge keeps TOS.
                    return self.jump(frame, branch_pc, target, ops_len);
                }
                Self::stop(self.store(frame, branch_pc, len - 1, &[]))
            }
            Op::Consp => self.replace_top(frame, operands, branch_pc, |top| top.is_cons()),
            Op::Stringp => self.replace_top(frame, operands, branch_pc, |top| top.is_string()),
            Op::Listp => self.replace_top(frame, operands, branch_pc, |top| {
                top.is_cons() || top.is_nil()
            }),
            Op::Null | Op::Not => self.replace_top(frame, operands, branch_pc, |top| top.is_nil()),
            _ => Step::Stop(VmOutcome::Fault(EngineFault::DispatchContract(op))),
        }
    }

    /// The sealed cons opcodes: read the operands out of the published
    /// segment, call the port, then replace the consumed run with the result
    /// in one store. The segment is unchanged while the core runs.
    fn step_sealed(
        &mut self,
        op: Op,
        authority: SealedAuthority,
        ctx: &mut Context,
        frame: FrameId,
        operands: OperandCursor,
        branch_pc: usize,
    ) -> Step {
        let Some(name) = sealed_core_name(&op) else {
            return Step::Stop(VmOutcome::Fault(EngineFault::DispatchContract(op)));
        };
        let effect = static_stack_effect(&op);
        let consumed = effect.consumed as usize;
        let len = operands.len();
        if consumed > len {
            return invalid();
        }
        let args = self.arena.operand_segment(operands)[len - consumed..].to_vec();
        let outcome = match authority {
            SealedAuthority::ConsFieldRead => self.sealed.read_cons_field(ctx, name, &args),
            SealedAuthority::ConsAllocation => self.sealed.allocate_cons(ctx, name, &args),
            SealedAuthority::ConsFieldWrite => self.sealed.write_cons_field(ctx, name, &args),
            _ => return Step::Stop(VmOutcome::Fault(EngineFault::DispatchContract(op))),
        };
        match outcome {
            CoreOutcome::Value(value) => {
                Self::stop(self.store(frame, branch_pc, len - consumed, &[value]))
            }
            CoreOutcome::Signal(flow) => Step::Stop(VmOutcome::Signaled(flow)),
            CoreOutcome::MissingCore => Step::Stop(VmOutcome::MissingCore { name }),
        }
    }

    /// R6: the arithmetic opcodes dispatch once on their operand kinds.
    /// Numeric operands run owner-locally through [`ArithOps`] with no poll
    /// and no escape; a marker operand takes the ONE escape as
    /// `Arith(MarkerInvolved)`, on the edges every escape takes; the
    /// builtin's `Flow` ends the run (§2.4). The table's `Arith(Numeric)` is
    /// the opcode's class; the operands decide.
    fn step_arith(
        &mut self,
        op: Op,
        frame: FrameId,
        operands: OperandCursor,
        branch_pc: usize,
        program: VmProgram<'_>,
    ) -> Step {
        let Some(kind) = ArithGenericKind::from_op(&op) else {
            return Step::Stop(VmOutcome::Fault(EngineFault::DispatchContract(op)));
        };
        let consumed = usize::from(static_stack_effect(&op).consumed);
        let len = operands.len();
        if consumed > len {
            return invalid();
        }
        let args = &self.arena.operand_segment(operands)[len - consumed..];
        match self.arith.arith(kind, args) {
            Ok(value) => Self::stop(self.store(frame, branch_pc, len - consumed, &[value])),
            Err(ArithRefusal::Flow(flow)) => Step::Stop(VmOutcome::Signaled(flow)),
            Err(ArithRefusal::MarkerInvolved) => self.escape(
                op,
                EscapeWhy::World(WorldSplit::Arith(ArithOperands::MarkerInvolved)),
                frame,
                operands,
                branch_pc,
                program,
            ),
        }
    }

    /// The ONE escape stub (r1 §4.2), in order: the call poll, the
    /// serial-request poll, the request, the completion, R2 (a `Flow` ends
    /// the run), R3 (the typed effect, the jump with it), the available
    /// operands, R1 (both LIFOs), then the store.
    fn escape(
        &mut self,
        op: Op,
        why: EscapeWhy,
        frame: FrameId,
        operands: OperandCursor,
        branch_pc: usize,
        program: VmProgram<'_>,
    ) -> Step {
        let effect = static_stack_effect(&op);
        // Malformed code: an opcode reached without the operands its static
        // effect consumes is the Scalar and Sealed arms' `Invalid byte-code`,
        // refused before any poll or request. The underflow check below is
        // the backstop.
        if operands.len() < usize::from(effect.consumed) {
            return invalid();
        }
        if matches!(
            op,
            Op::Call(_) | Op::Apply(_) | Op::CallBuiltin(..) | Op::CallBuiltinSym(..)
        ) && let Err(error) = self.poll.vm_call(&self.arena)
        {
            return Step::Stop(VmOutcome::Fault(EngineFault::Poll(error)));
        }
        // The serial-request poll does not replace the escape: a successful
        // poll still records exactly one request.
        if let Err(error) = self.poll.serial_request(&self.arena) {
            return Step::Stop(VmOutcome::Fault(EngineFault::Poll(error)));
        }
        let request = VmEscapeRequest {
            op: op.clone(),
            why,
            pc: Pc::new(branch_pc),
            frame,
            operands,
        };
        let completion = self.escape.escape(request, &self.arena);
        let (consumed, produced, jump) = match completion.stack {
            EscapeStack::Flow(flow) => return Step::Stop(VmOutcome::Signaled(flow)),
            EscapeStack::Completed {
                consumed,
                produced,
                jump,
            } => (consumed, produced, jump),
        };
        // R3: the typed effect, before anything is applied.
        let got_produced = u16::try_from(produced.len()).unwrap_or(u16::MAX);
        if consumed != effect.consumed || got_produced != effect.produced {
            tracing::error!(
                op = ?op,
                expected_consumed = effect.consumed,
                expected_produced = effect.produced,
                got_consumed = consumed,
                got_produced,
                "escape completion does not match the opcode's static stack effect"
            );
            return Step::Stop(VmOutcome::Fault(EngineFault::EscapeStackEffect {
                op,
                expected_consumed: effect.consumed,
                expected_produced: effect.produced,
                got_consumed: consumed,
                got_produced,
            }));
        }
        // With R3, before anything is applied: only `Switch` reports a jump
        // (a hit leaves through `op_branch`, bytecode.c:1752-1758). A
        // `Return` that reports one is refused here, so the record mirror
        // below never runs for it.
        let jump = match jump {
            Some(target) if !matches!(op, Op::Switch) => {
                tracing::error!(
                    op = ?op,
                    target,
                    "escape completion reports a jump for an opcode that cannot branch"
                );
                return Step::Stop(VmOutcome::Fault(EngineFault::DispatchContract(op)));
            }
            jump => jump,
        };
        let Some(available) = self
            .arena
            .vm(frame)
            .map(|vm| u16::try_from(vm.operands.len()).ok())
            .flatten()
        else {
            return Step::Stop(VmOutcome::Fault(EngineFault::StaleFrame));
        };
        if consumed > available {
            tracing::error!(
                op = ?op,
                consumed,
                available,
                "escape completion consumes more operands than the segment holds"
            );
            return Step::Stop(VmOutcome::Fault(EngineFault::EscapeStackUnderflow {
                op,
                consumed,
                available,
            }));
        }
        // R1: both LIFOs are checked before either is touched.
        let contract = if !self
            .arena
            .bindings()
            .serial_run_matches(&completion.records.popped)
        {
            Some(RecordStack::Bindings)
        } else if !self
            .arena
            .handlers()
            .serial_run_matches(&completion.handlers.popped)
        {
            Some(RecordStack::Handlers)
        } else {
            None
        };
        if let Some(stack) = contract {
            tracing::error!(
                op = ?op,
                stack = ?stack,
                "escape completion pops serial tokens that are not the top run"
            );
            return Step::Stop(VmOutcome::Fault(EngineFault::EscapeRecordContract(stack)));
        }
        let _ = self
            .arena
            .bindings_mut()
            .pop_serial_run(&completion.records.popped);
        let _ = self
            .arena
            .handlers_mut()
            .pop_serial_run(&completion.handlers.popped);
        for token in &completion.records.pushed {
            self.arena.bindings_mut().push(*token);
        }
        for token in &completion.handlers.pushed {
            self.arena.handlers_mut().push(*token);
        }
        if matches!(op, Op::Return) {
            return self.return_from_frame(frame, produced);
        }
        if let Err(fault) = self.store(
            frame,
            branch_pc,
            available as usize - consumed as usize,
            &produced,
        ) {
            return Step::Stop(VmOutcome::Fault(fault));
        }
        match jump {
            Some(target) => self.jump(frame, branch_pc, target, program.ops.len()),
            None => Step::Continue,
        }
    }

    /// The `Return` escape completed (r1 §5, V1 stand-in): pop the frame and
    /// publish its value. V4 replaces this with the typed disposition.
    fn return_from_frame(&mut self, frame: FrameId, produced: Vec<Value>) -> Step {
        let Some(value) = produced.first().copied() else {
            return Step::Stop(VmOutcome::Fault(EngineFault::DispatchContract(Op::Return)));
        };
        let _ = self.arena.vm_truncate_operands(frame, 0);
        if self.arena.pop(frame).is_none() {
            return Step::Stop(VmOutcome::Fault(EngineFault::StaleFrame));
        }
        self.frame = None;
        match self.arena.roots_mut().push(RootGroup::VmFrames, value) {
            Ok(pending) => Step::Stop(VmOutcome::Done { pending }),
            Err(error) => Step::Stop(VmOutcome::Fault(EngineFault::Root(error))),
        }
    }
}
