//! Stage-1 parallel bytecode stepper for the E4 §5 VM roster
//! (VM-EXECUTOR-DESIGN r1), harness only.
//!
//! One frame runs on the owned [`FrameArena`], classified by the generated
//! dispatch table (`opcode_slot`). Scalar opcodes step the frame's operand
//! segment owner-locally; the eight sealed cons opcodes call [`VmSealed`];
//! the fifteen arithmetic opcodes call [`ArithOps`] (R6) and escape only
//! when an operand is a marker; the four Plain variable opcodes run on the
//! owner slab through [`VmPlain`] (V2) and escape only for a symbol the
//! admission refuses or for the serial side's own records under `Unbind`;
//! every other World arm, `Return` included, is exactly ONE escape through
//! [`VmEscape`], after the [`VmPoll`] edges r1 §6 names.
//!
//! An escape completion is a typed contract, checked before anything is
//! applied: ordinary escapes return [`EscapeCompletion`] (L1 single-token
//! handler pushes, L2/L7 delta-free `Caught`/`Flow`, R3 operand counts, R1
//! serial LIFOs), while dynamic-record unwinds (`Op::Unbind` and V8's
//! `unwind_to_handler`) return [`UnwindCompletion`]. A `Flow` ends the run and
//! settles to `Idle`; a `Caught` drives GNU-order unwinding to the matched
//! owner handler's [`LandingPad`]. A contract fault keeps the scene it
//! describes and poisons the engine.
//!
//! Every [`Value`] that must survive a poll is reachable from the published
//! arena: the frame's segment is the arena's operand vector, and a step
//! never holds a value across a poll. The escape handler reads the operands
//! through the arena with the frame and cursor the request carries.
//! This module starts no executor thread and is not reachable from serial
//! dispatch.

#![allow(dead_code)]

use super::super::owner_bindings::OwnerRecordId;
use super::backtrace::{
    BacktraceDebugError, BacktraceDebugUpdate, OwnerBacktraceView, OwnerStopState,
    StoppedOwnerBacktrace,
};
use super::dispatch::opcode_slot;
use super::effective::{
    EffectivePlainKey, EraId, PlainAccess, PlainRefusal, PlainUnbind, PlainUnbindError,
    ShapeGeneration, effective_plain_bind, effective_plain_read, effective_plain_unbind_checked,
    effective_plain_write,
};
use super::engine::{CoreOutcome, EngineFault, RecordStack};
use super::frames::{
    ExitFlags, FrameArena, FrameId, HandlerMark, LandingPad, OperandCursor, OwnerHandler,
    OwnerRecord, OwnerRecordMark, Pc, PollBudget, ReturnTo, RootGroup, RootHandle,
    RootPublishError, RootRevokeError, SerialHandlerToken, SerialRecordToken, VmFrame,
};
use crate::emacs_core::bytecode::arith_kind::ArithGenericKind;
use crate::emacs_core::bytecode::function_slots::FunctionParamsExt as _;
use crate::emacs_core::bytecode::value_api::TaggedValueBytecodeExt;
use crate::emacs_core::bytecode::vm::{bytecode_frame_bytes, charge_frame_position};
use crate::emacs_core::bytecode::{ByteCodeFunction, FunctionParams, Op, Vm};
use crate::emacs_core::eval::Context;
use crate::emacs_core::eval::world::poll::PollOutcome;
use crate::emacs_core::subr::effect_schema::{
    ArithOperands, ParallelArm, PlainVarAccess, ReturnDisposition, SealedAuthority, WorldSplit,
};
use neovm_value::error::{Flow, signal};
use neovm_value::intern::{NIL_SYM_ID, SymId};
use neovm_value::tagged::gc::{RegistrationError, RendezvousError};
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
/// caller (`Breturn`: `val = TOP` … `TOP = val`, bytecode.c:897-921).
/// `Throw` refuses a depth below two and does not fall through; the
/// table's `(2, 0)` is that operand count (`vm.rs`, the serial `Throw` arm).
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
/// [`VmPoll::serial_request`] runs before every escape.
/// [`VmPoll::vm_refill`] serves an instruction-boundary [`Step::Refill`].
///
/// Publication rule as on `engine::OwnerPoll`: a `Value` must not be carried
/// unrooted across a call. Close-before-publish invariant (#12246): no cons
/// region or numeric run may remain open outside a live `RunAccess` —
/// `RunAccess` borrows `&mut Mutator`, its `Drop` poisons the registry if any
/// run is left open, and `publish_roots` refuses `Loan` while admitted. That
/// structural guarantee holds at all four edges.
pub(crate) trait VmPoll {
    fn vm_back_edge(&mut self, arena: &FrameArena) -> Result<PollOutcome, RendezvousError>;

    fn vm_call(&mut self, arena: &FrameArena) -> Result<PollOutcome, RendezvousError>;

    fn serial_request(&mut self, arena: &FrameArena) -> Result<PollOutcome, RendezvousError>;

    /// The driver's refill at an instruction boundary (V5; peer #12100,
    /// #12241, #12246): with no run open (per the structural `RunAccess`
    /// invariant above), publish (`poll_region_refill`) -> the stopped grant
    /// (`resume_cons_storage` mints the class pool on demand, gcpace's N2) ->
    /// restore the generation -> reacquire. No serial ticket, no Baton.
    /// Production cons grants stay gated on L6.5 routing serial GC through
    /// the stop session (#13084, #13088). One call per [`Step::Refill`];
    /// `Ok` means the same pc may re-dispatch, and the caller does not assume
    /// progress — an `Err` is the poll fault and ends the run.
    fn vm_refill(
        &mut self,
        arena: &FrameArena,
        class: NumericClass,
    ) -> Result<PollOutcome, RendezvousError>;
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

/// The numeric class an allocation was refused for (V5; E4 item4 C2).
/// Re-exported from `neovm_value::tagged::gc` so the VM and `RunAccess`
/// share one canonical type.
pub(crate) use neovm_value::tagged::gc::NumericClass;

/// Why the owner-local arithmetic port produced no value.
#[derive(Debug)]
pub(crate) enum ArithRefusal {
    /// An operand is a marker: its number is a buffer position, so the
    /// opcode takes the ONE escape as `Arith(MarkerInvolved)` (R6).
    MarkerInvolved,
    /// The builtin's `Flow`: a wrong-type operand, an integer zero divisor
    /// (§2.4).
    Flow(Flow),
    /// The owner's reserve for `class` is exhausted (V5). Produced only
    /// after every operand check - a marker or a wrong type is decided
    /// first - and before anything is touched: the step leaves the op
    /// un-executed at its instruction boundary, and the driver refills
    /// (E4 item4 C2, peer #12095/#12100; the pool is gcpace's N2). The
    /// port never polls or publishes itself; a refusal is not a fault.
    Exhausted(NumericClass),
    /// A registration or era-poison failure from the owner's allocator
    /// (e.g. poisoned between an `Ok` refill and the retry, M1 / #12221).
    /// Converted via `RendezvousError::from`: `RegistrationError::Poisoned`
    /// surfaces as `VmOutcome::Fault(EngineFault::Poll(RendezvousError::Poisoned))`,
    /// while other registration errors surface as
    /// `VmOutcome::Fault(EngineFault::Poll(RendezvousError::Registration(..)))` —
    /// never as a Lisp signal.
    Fault(RegistrationError),
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

/// The owner-local Plain variable port (V2; E4-P1's composed [`PlainAccess`]
/// for the VM): the owner slot view and the canonical Plain storage, plus
/// the (era, shape) generation a cached key is checked against, read at
/// use. A structural change the serial side made during an escape moves
/// it, and the next Plain opcode re-derives its key through `admission`
/// (the cold compare after every serial escape, E4 §4.2). E4-P2 supplies
/// the production implementation from the owner slab lease and C3; the
/// classification runs under the real symbol-reader authority there.
pub(crate) trait VmPlain: PlainAccess {
    fn generation(&self) -> (EraId, ShapeGeneration);
}

/// One serial unwind step, issued by the VM's unwind loop (V8 L3) or `Unbind`
/// walk (V2 migration, V8 §10) in GNU `unwind_to_catch`/`unbind_to` order
/// (`src/eval.c:1445-1454, 3946-3957`).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct SerialUnwind {
    /// Serial handler markers to remove first, innermost first (popped after
    /// their records per L3 / `src/eval.c:1449-1452`).
    pub pop_handlers: Vec<SerialHandlerToken>,
    /// Contiguous run of serial records to unbind, innermost first.
    pub unbind: Vec<SerialRecordToken>,
}

/// Why an opcode escaped.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum EscapeWhy {
    World(WorldSplit),
    UnimplementedSealed(SealedAuthority),
    /// A Plain variable opcode whose symbol the admission refused, on the
    /// first use or after a stale key was re-derived (V2): the access stays
    /// on the serial path, which sees the access kind and the refusal.
    PlainRefused {
        access: PlainVarAccess,
        refusal: PlainRefusal,
    },
    /// Unified serial unwind step (V8 §4/§10, replacing V2's `SerialUnbind`):
    /// `Unbind n` emits `pop_handlers: []` and `unbind: run`; V8's unwind loop
    /// batches finished intermediate handlers into `pop_handlers` and issues
    /// one final `unbind: []` pop+landing request.
    SerialUnwind(SerialUnwind),
    /// `Unbind n` or V8's L3 unwind reached an owner binding whose symbol the
    /// unbind-time re-validation refused (N-v2-unbind-revalidate): GNU's
    /// `do_one_unbind` for a `SPECPDL_LET` re-reads the symbol as the binding
    /// leaves (`src/eval.c:3848-3870`), so a watcher added while the binding
    /// was live is notified through `set_internal`. The serial side undoes
    /// this one binding, taking the slab record `record`; the owner restored
    /// nothing. The executor's entry left before the escape (GNU's order,
    /// `src/eval.c:3948-3957`), so the walk's check after it is that the
    /// record is gone from the slab.
    PlainUnbind {
        symbol: SymId,
        record: OwnerRecordId,
    },
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

/// The serial binding records one escape removed and added. `popped` is
/// innermost first and must be the LIFO's top run; `pushed` is outermost
/// first.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct RecordDelta {
    pub popped: Vec<SerialRecordToken>,
    pub pushed: Vec<SerialRecordToken>,
}

/// The serial handler records one escape removed and added. `popped` is
/// innermost first and must be the LIFO's top run; `pushed` is at most one
/// token (V8 L1: only `PushCatch`, `PushConditionCase`, and
/// `PushConditionCaseRaw` install a handler with one computed [`LandingPad`]).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct HandlerDelta {
    pub popped: Vec<SerialHandlerToken>,
    pub pushed: Option<SerialHandlerToken>,
}

/// Result of an ordinary opcode escape ([`VmEscape::escape`], r1 R1/R3, V8
/// L1/L2/L7).
///
/// L2 and L7 are structural: ordinary `Caught` and `Flow` cannot carry
/// [`RecordDelta`] or [`HandlerDelta`] at all. Only
/// `Return(NeedsSerialCleanup)` may report `CleanupCaught` with the serial
/// prefix already popped above the returning frame's bases.
#[derive(Debug)]
pub(crate) enum EscapeCompletion {
    /// The fall-through effect (or `Switch` branch / `Return` value) plus
    /// checked LIFO deltas. `jump` is valid only for `Op::Switch` (a hit
    /// leaves through `op_branch`, `src/bytecode.c:1752-1758`).
    Completed {
        consumed: u16,
        produced: Vec<Value>,
        jump: Option<u32>,
        records: RecordDelta,
        handlers: HandlerDelta,
    },
    /// An ordinary opcode signaled or threw into an owner handler (V8 L2;
    /// GNU `signal_or_quit` / `Fthrow`, `src/eval.c:1464-1481, 1996-2064`).
    /// Structurally carries no record or handler deltas: the serial side
    /// unwound only what this escape pushed.
    Caught {
        handler: SerialHandlerToken,
        value: Value,
    },
    /// `Return(NeedsSerialCleanup)` signaled while running the returning
    /// frame's serial cleanup or `debug-on-exit` into a caller's owner
    /// handler: carries only the serial records and handlers already popped
    /// above the returning frame's `bind_base`/`handler_base` (never pushes).
    /// Rejected on any other opcode.
    CleanupCaught {
        handler: SerialHandlerToken,
        value: Value,
        popped_records: Vec<SerialRecordToken>,
        popped_handlers: Vec<SerialHandlerToken>,
    },
    /// The serial side signaled or threw past every owner handler (V8 L7).
    /// Structurally carries no record or handler deltas.
    Flow(Flow),
}

impl EscapeCompletion {
    /// The completion a fall-through escape reports: the opcode's static
    /// effect with nil operands, no jump and no record change.
    pub fn fall_through(op: &Op) -> Self {
        let StackEffect { consumed, produced } = static_stack_effect(op);
        Self::Completed {
            consumed,
            produced: vec![Value::NIL; produced as usize],
            jump: None,
            records: RecordDelta::default(),
            handlers: HandlerDelta::default(),
        }
    }
}

/// Result of a dynamic-record unwind request ([`VmEscape::unwind`], shared by
/// `Op::Unbind(n)` and `VmExec::unwind_to_handler`, V8 L2/L3/L5/§10).
///
/// Structurally carries no operand stack effects, no handler pushes, and no
/// pushed records on `Caught` or `Flow` (L2).
#[derive(Debug)]
pub(crate) enum UnwindCompletion {
    /// Every requested handler and binding record unwound cleanly.
    /// `pushed_records` holds any serial records pushed by cleanup Lisp (must
    /// be empty for a Lisp-free `SerialUnwind { unbind: [] }`).
    Completed {
        popped_handlers: Vec<SerialHandlerToken>,
        popped_records: Vec<SerialRecordToken>,
        pushed_records: Vec<SerialRecordToken>,
    },
    /// A cleanup that ran Lisp (`!SerialUnwind::unbind.is_empty()` or
    /// `PlainUnbind`) signaled or threw into an owner handler (V8 L5 /
    /// HIGH-1). Structurally carries no pushed records or handlers (L2).
    /// For `SerialUnwind`, `popped_records` must be a non-empty prefix of
    /// `unwind.unbind` (GNU `src/eval.c:3955`, decrement before `do_one_unbind`).
    Caught {
        handler: SerialHandlerToken,
        value: Value,
        popped_handlers: Vec<SerialHandlerToken>,
        popped_records: Vec<SerialRecordToken>,
    },
    /// A cleanup that ran Lisp signaled or threw past every owner handler
    /// (V8 L7). Structurally carries no deltas.
    Flow(Flow),
}

/// The serial side of an escape. The production impl arrives with E5.
pub(crate) trait VmEscape {
    /// Ordinary opcode escape (`World` ops, refused `Plain` access, marker
    /// arithmetic, `Return(NeedsSerialCleanup)`).
    ///
    /// `plain` is the owner's Plain storage. PROVISIONAL HARNESS SCAFFOLDING
    /// (peer #13059): production releases the executor's Plain lease BY VALUE
    /// into the serial turn and hands a fresh lease back. G1's production
    /// `VmEscape` removes this parameter.
    fn escape(
        &mut self,
        request: VmEscapeRequest,
        arena: &FrameArena,
        plain: &mut dyn PlainAccess,
    ) -> EscapeCompletion;

    /// Dynamic-record unwind escape (`EscapeWhy::SerialUnwind` and
    /// `EscapeWhy::PlainUnbind`), shared by `Op::Unbind(n)` and V8's
    /// `unwind_to_handler`.
    fn unwind(
        &mut self,
        request: VmEscapeRequest,
        arena: &FrameArena,
        plain: &mut dyn PlainAccess,
    ) -> UnwindCompletion;

    /// Pending `backtrace-debug` flag updates recorded while the serial escape
    /// inspected the stopped owner's [`OwnerBacktraceView`] (V6). Drained once
    /// right when `escape` returns and applied while the owner is still stopped
    /// at the escape boundary (only when the completion passes R2/R3/R1).
    fn take_debug_on_exit_updates(&mut self) -> Vec<BacktraceDebugUpdate> {
        Vec::new()
    }

    /// Fresh [`CallEnv`] snapshot supplied by the serial side at run start and
    /// after every checked escape completion (V7 §4). `None` keeps the
    /// executor's current [`CallEnv`].
    fn refresh_call_env(&mut self) -> Option<CallEnv> {
        None
    }
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

/// Entry frame descriptor for [`VmExec::run_with_code`] (V7 §5).
#[derive(Clone, Copy, Debug)]
pub(crate) struct VmEntry {
    pub function: Value,
    pub code: Value,
    pub frame_bytes: usize,
}

/// Why [`VmCode::program`] could not resolve a frame's `code` value (V7 §5).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CodeRefusal {
    NotByteCode,
    NotMaterialized,
    NotDecoded,
}

/// Why an `Op::Call(n)` escaped instead of pushing an owner-local child frame
/// (V7 §4). Every variant is decided read-only before any mutation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CallEscape {
    /// Subr, interpreted closure, alias, autoload, macro, non-function,
    /// unbound symbol, or symbol with position.
    NotByteCode,
    /// Unmaterialized pdump stub (`bytecode_data_if_materialized` is `None`).
    NotMaterialized,
    /// A lazy GNU decode/proof/admission `OnceLock` is not yet initialized.
    ColdAdmission,
    /// The serial iterative predicate refuses (`env`, `&rest`, dynamic params,
    /// `max_stack > ITERATIVE_FRAME_LIMIT`, unsealed/unverified).
    Ineligible,
    /// `nargs` outside `required..=nonrest`: serial signals with exact backtrace.
    WrongNargs,
    /// `arena.depth() + 1 > CallEnv.depth_limit`: serial raises floor & signals.
    Depth,
    /// `charge_frame_position` failed: serial signals `"Bytecode stack overflow"`.
    StackBudget,
    /// `debug-on-next-call` is armed (checked first before resolving).
    DebugOnNextCall,
    /// `internal--compiler-function-overrides` is active (symbol callees).
    CompilerOverrides,
    /// Function epoch moved since last `CallEnv` refresh (symbol callees only).
    FunctionEpoch,
}

/// Typed classification of an `Op::Call(n)` (V7 §4, C1/C4).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CallClass {
    /// Push a child frame. Every field is a fact the classifier read; the
    /// stepper re-derives nothing.
    OwnerLocal {
        code: Value,
        nonrest: u16,
        frame_bytes: usize,
    },
    /// Take today's `Call` escape stub with the caller untouched.
    Escape(CallEscape),
}

/// Owner-private snapshot of call-admission environment (V7 §4).
/// Refreshed at run start and after every escape completion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CallEnv {
    pub depth_limit: u32,
    pub debug_on_next_call: bool,
    pub compiler_overrides: bool,
    pub function_epoch: u64,
}

impl Default for CallEnv {
    fn default() -> Self {
        Self {
            depth_limit: FRAME_DEPTH_LIMIT,
            debug_on_next_call: false,
            compiler_overrides: false,
            function_epoch: 0,
        }
    }
}

/// Result of reading a bare symbol's live function cell for V7 classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SymbolCallLookup {
    /// The World's function epoch (or obarray generation) changed since `CallEnv`.
    EpochChanged,
    /// Live function cell is bound to `Value`.
    Bound(Value),
    /// Empty or explicitly unbound function cell.
    Unbound,
}

/// The program and call-classification port passed by reference to
/// [`VmExec::run_with_code`] (V7 C3, §5).
pub(crate) trait VmCode {
    /// The lowered program of a frame's `code` object.
    fn program(&self, code: Value) -> Result<VmProgram<'_>, CodeRefusal>;

    /// Classify `Op::Call(nargs)` read-only before any mutation (§4).
    fn classify_call(
        &self,
        designator: Value,
        nargs: u16,
        env: &CallEnv,
        arena: &FrameArena,
    ) -> CallClass;
}

/// Read-only instruction slice for an admitted [`ByteCodeFunction`] (V7).
/// Never initializes a cold lazy GNU decode cell; returns `None` when an
/// undecoded lazy cell is present so the call escapes instead.
pub(crate) fn ready_bytecode_ops(func: &ByteCodeFunction) -> Option<&[Op]> {
    match func.lazy_gnu_code() {
        Some(lazy) => lazy.decoded_ready().map(|decoded| decoded.ops().as_slice()),
        None => Some(func.ops().as_slice()),
    }
}

/// Resolve a materialized, decoded [`ByteCodeFunction`] value into a
/// [`VmProgram`] without writing any shared lazy cell (V7 §5).
pub(crate) fn program_from_bytecode(code: Value) -> Result<VmProgram<'static>, CodeRefusal> {
    if !code.is_bytecode() {
        return Err(CodeRefusal::NotByteCode);
    }
    let Some(bc) = code.bytecode_data_if_materialized() else {
        return Err(CodeRefusal::NotMaterialized);
    };
    let Some(ops) = ready_bytecode_ops(bc) else {
        return Err(CodeRefusal::NotDecoded);
    };
    Ok(VmProgram {
        ops,
        constants: bc.constants().as_slice(),
        function: code,
        code,
    })
}

/// Production pure classifier for a materialized [`ByteCodeFunction`] (V7 §4,
/// S1/S2/S4). Never mutates any shared `OnceLock`.
pub(crate) fn classify_bytecode_function(
    callee_value: Value,
    bc: &ByteCodeFunction,
    nargs: u16,
    env: &CallEnv,
    arena: &FrameArena,
) -> CallClass {
    let nargs_usize = usize::from(nargs);
    match Vm::iterative_admission_if_known(bc, nargs_usize) {
        None => return CallClass::Escape(CallEscape::ColdAdmission),
        Some(false) => {
            // S2: `Some(false)` from the peek conflates `Ineligible` and
            // `WrongNargs`. Re-check `nargs` against the pure template reads
            // `(required, nonrest)` so the escape reason is precise while
            // keeping `iterative_admission_if_known` the single authority.
            let is_wrong_nargs = bc.env().is_none()
                && bc.params().stack_shape().is_some_and(|shape| {
                    let params_on_stack = match bc.params() {
                        FunctionParams::Stack(_) => true,
                        FunctionParams::Dynamic(_) => false,
                        FunctionParams::Named(_) => bc.lexical() || bc.arglist().is_fixnum(),
                    };
                    let has_named_params = shape.nonrest() > 0;
                    !shape.rest().is_present()
                        && (!has_named_params || params_on_stack)
                        && (nargs_usize < shape.required() || nargs_usize > shape.nonrest())
                });
            return CallClass::Escape(if is_wrong_nargs {
                CallEscape::WrongNargs
            } else {
                CallEscape::Ineligible
            });
        }
        Some(true) => {}
    }
    if ready_bytecode_ops(bc).is_none() {
        return CallClass::Escape(CallEscape::ColdAdmission);
    }
    let Some(nonrest) = bc
        .params()
        .stack_shape()
        .and_then(|shape| u16::try_from(shape.nonrest()).ok())
    else {
        return CallClass::Escape(CallEscape::Ineligible);
    };
    // S4: Depth and StackBudget come AFTER shape/arity classification so the
    // escape reason matches what the serial path would signal.
    if arena.depth().saturating_add(1) > env.depth_limit {
        return CallClass::Escape(CallEscape::Depth);
    }
    let max_stack = bc.max_stack().get();
    if charge_frame_position(arena.bc_position(), max_stack).is_none() {
        return CallClass::Escape(CallEscape::StackBudget);
    }
    CallClass::OwnerLocal {
        code: callee_value,
        nonrest,
        frame_bytes: bytecode_frame_bytes(max_stack),
    }
}

/// Production pure classifier over a call designator (direct bytecode or bare
/// symbol) plus `CallEnv` and `FrameArena` (V7 §4, S1–S4).
pub(crate) fn classify_call_with_symbols(
    designator: Value,
    nargs: u16,
    env: &CallEnv,
    arena: &FrameArena,
    lookup_symbol: impl FnOnce(SymId) -> SymbolCallLookup,
) -> CallClass {
    // §4 item 7: check `debug-on-next-call` first, before resolving.
    if env.debug_on_next_call {
        return CallClass::Escape(CallEscape::DebugOnNextCall);
    }
    let callee = if designator.is_bytecode() {
        // S3: direct bytecode designators need no compiler_overrides or
        // function_epoch gate.
        designator
    } else if let Some(sym_id) = designator.as_bare_symbol_id() {
        if env.compiler_overrides {
            return CallClass::Escape(CallEscape::CompilerOverrides);
        }
        match lookup_symbol(sym_id) {
            SymbolCallLookup::EpochChanged => return CallClass::Escape(CallEscape::FunctionEpoch),
            SymbolCallLookup::Unbound => return CallClass::Escape(CallEscape::NotByteCode),
            SymbolCallLookup::Bound(val) if val.is_bytecode() => val,
            SymbolCallLookup::Bound(_) => return CallClass::Escape(CallEscape::NotByteCode),
        }
    } else {
        return CallClass::Escape(CallEscape::NotByteCode);
    };
    let Some(bc) = callee.bytecode_data_if_materialized() else {
        return CallClass::Escape(CallEscape::NotMaterialized);
    };
    classify_bytecode_function(callee, bc, nargs, env, arena)
}

/// Default [`VmCode`] adapter used by [`VmExec::run`] for a single entry
/// [`VmProgram`] plus direct bytecode callees (no bare-symbol cell table).
struct EntryCode<'a> {
    entry: VmProgram<'a>,
}

impl<'a> VmCode for EntryCode<'a> {
    fn program(&self, code: Value) -> Result<VmProgram<'_>, CodeRefusal> {
        if code == self.entry.code {
            return Ok(self.entry);
        }
        program_from_bytecode(code)
    }

    fn classify_call(
        &self,
        designator: Value,
        nargs: u16,
        env: &CallEnv,
        arena: &FrameArena,
    ) -> CallClass {
        classify_call_with_symbols(designator, nargs, env, arena, |_| SymbolCallLookup::Unbound)
    }
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
    /// A `VarRef` whose effective value is void: an owner slot that is a
    /// local void binding, or no owner binding and an unbound canonical
    /// value. GNU's bare-symbol read reaches `Fsymbol_value`, which signals
    /// `void-variable` with the symbol as its only datum (`eval.c:2584-2592`,
    /// `data.c:1645-1657`; the serial `fast_path_var_ref`,
    /// bytecode/vm.rs:7320-7327). The typed outcome of `StepOutcome::
    /// VoidVariable`: the caller raises the signal.
    VoidVariable {
        symbol: Value,
    },
    MissingCore {
        name: &'static str,
    },
    Fault(EngineFault),
}

enum Step {
    Continue,
    /// A child frame was pushed or popped: re-resolve top frame and program
    /// in the outer `'frame` loop (V7 C2, Q1).
    Frame,
    /// The op is un-executed at its instruction boundary, pc and operands
    /// unchanged, because the port's reserve for `class` is exhausted
    /// (V5). The driver refills through [`VmPoll::vm_refill`] - publish,
    /// the stopped grant, restore, reacquire - and the same pc
    /// re-dispatches.
    Refill(NumericClass),
    Stop(VmOutcome),
}

/// The harness VM stepper. `E` is the escape handler, `S` the sealed-ops
/// port, `A` the arithmetic port, `V` the Plain variable port and `P` the
/// poll capability; tests supply the doubles.
pub(crate) struct VmExec<'era, E, S, A, V, P> {
    arena: FrameArena,
    escape: E,
    sealed: S,
    arith: A,
    plain: V,
    poll: P,
    state: RunState,
    /// Whether this executor is actively stepping bytecode (`Running`) or
    /// stopped (`Stopped`: idle, parked at any [`VmPoll`] edge, or parked in
    /// an escape).
    stop_state: OwnerStopState,
    /// The exit flags the next run's bottom frame carries (V4). The
    /// harness arms them; V6's backtrace adapter sets a stopped owner's.
    entry_exit_flags: ExitFlags,
    /// Owner-private snapshot of call-admission environment (V7 §4).
    call_env: CallEnv,
    /// Recorded [`CallEscape`] reasons during the most recent run (V7 §4).
    call_escapes: Vec<CallEscape>,
    /// Maximum `FrameArena::depth()` observed during the most recent run (V7).
    max_depth: u32,
    /// The Plain keys this stepper admitted, one per symbol (V2; N1 picks
    /// the production structure). A key survives runs; its validity is
    /// checked at use against the port's generation.
    keys: Vec<(SymId, EffectivePlainKey<'era>)>,
    /// The borrow the cached keys are valid in. Production passes the era
    /// the lease lends the stepper (E6-P), so no key outlives it; the
    /// harness passes `&()`.
    era: &'era (),
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

impl<'era, E, S, A, V, P> VmExec<'era, E, S, A, V, P>
where
    E: VmEscape,
    S: VmSealed,
    A: ArithOps,
    V: VmPlain,
    P: VmPoll,
{
    /// `era` is the borrow the admitted keys live in (see the field).
    /// Add `flags` to the exit flags the next run's bottom frame carries
    /// (V4 rows; V6's adapter sets a stopped owner's frame instead). Armed
    /// for ONE run: GNU's flag is per backtrace frame, so it is cleared when
    /// that frame is pushed.
    pub(crate) fn set_entry_exit_flags(&mut self, flags: ExitFlags) {
        self.entry_exit_flags = self.entry_exit_flags.set(flags);
    }

    pub(crate) fn new(era: &'era (), escape: E, sealed: S, arith: A, plain: V, poll: P) -> Self {
        let call_env = CallEnv::default();
        Self {
            arena: FrameArena::new(call_env.depth_limit),
            escape,
            sealed,
            arith,
            plain,
            poll,
            state: RunState::Idle,
            stop_state: OwnerStopState::Stopped,
            entry_exit_flags: ExitFlags::EMPTY,
            call_env,
            call_escapes: Vec::new(),
            max_depth: 0,
            keys: Vec::new(),
            era,
        }
    }

    pub(crate) fn set_call_env(&mut self, env: CallEnv) {
        self.call_env = env;
        self.arena.set_depth_limit(env.depth_limit);
    }

    pub(crate) fn call_env(&self) -> CallEnv {
        self.call_env
    }

    pub(crate) fn call_escapes(&self) -> &[CallEscape] {
        &self.call_escapes
    }

    pub(crate) fn max_depth(&self) -> u32 {
        self.max_depth
    }

    pub(crate) fn escape_handler(&self) -> &E {
        &self.escape
    }

    pub(crate) fn arith_handler(&self) -> &A {
        &self.arith
    }

    pub(crate) fn plain_handler(&self) -> &V {
        &self.plain
    }

    pub(crate) fn plain_handler_mut(&mut self) -> &mut V {
        &mut self.plain
    }

    pub(crate) fn poll_handler(&self) -> &P {
        &self.poll
    }

    /// Release the poll capability. An owner-backed impl returns its
    /// [`Mutator`](neovm_value::tagged::gc::Mutator) so the caller retires it.
    pub(crate) fn into_poll(self) -> P {
        self.poll
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

    /// The active top frame, while one is pushed (V7 Q2: single source of
    /// truth in `FrameArena::top_vm_id`).
    pub(crate) fn frame_id(&self) -> Option<FrameId> {
        self.arena.top_vm_id()
    }

    pub(crate) fn is_idle(&self) -> bool {
        matches!(self.state, RunState::Idle)
    }

    pub(crate) fn stop_state(&self) -> OwnerStopState {
        self.stop_state
    }

    /// Capture an owned, read-only [`OwnerBacktraceView`] of this executor's
    /// published frames. Holds no borrow of `self` or `FrameArena`.
    pub(crate) fn backtrace_view(&self) -> OwnerBacktraceView {
        OwnerBacktraceView::from_arena(&self.arena)
    }

    /// Set or clear `ExitFlags::DEBUG_ON_EXIT` on a live `VmFrame` via the
    /// stopped-owner backtrace adapter. Refused if the owner is running.
    pub(crate) fn set_debug_on_exit(
        &mut self,
        frame_id: FrameId,
        enabled: bool,
    ) -> Result<bool, BacktraceDebugError> {
        let mut stopped = StoppedOwnerBacktrace::try_from_state(&mut self.arena, self.stop_state)?;
        Ok(stopped.set_debug_on_exit(frame_id, enabled))
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
        let code = EntryCode { entry: program };
        let entry = VmEntry {
            function: program.function,
            code: program.code,
            frame_bytes: 0,
        };
        self.run_with_code(ctx, &code, entry)
    }

    /// Run `entry` through the [`VmCode`] port using the iterative `'frame`
    /// loop (V7 C2/C3, §5).
    pub(crate) fn run_with_code<C: VmCode>(
        &mut self,
        ctx: &mut Context,
        code: &C,
        entry: VmEntry,
    ) -> VmOutcome {
        if matches!(self.state, RunState::Poisoned) {
            return VmOutcome::Fault(EngineFault::Poisoned);
        }
        if !matches!(self.state, RunState::Idle) {
            return VmOutcome::Fault(EngineFault::Busy);
        }
        if let Some(env) = self.escape.refresh_call_env() {
            self.set_call_env(env);
        } else {
            self.arena.set_depth_limit(self.call_env.depth_limit);
        }
        self.call_escapes.clear();
        self.max_depth = 0;
        self.arena.reset_bc_position();
        let entry_exit_flags = std::mem::take(&mut self.entry_exit_flags);
        let frame = VmFrame::new(
            entry.function,
            entry.code,
            Pc::new(0),
            OperandCursor::new(0, 0),
            OwnerRecordMark::EMPTY,
            HandlerMark::EMPTY,
            PollBudget::new(NonZeroU32::MIN),
        )
        .with_exit_flags(entry_exit_flags)
        .with_return_to(ReturnTo::Owner)
        .with_frame_bytes(entry.frame_bytes);
        if let Err(error) = self.arena.push_vm(frame) {
            return VmOutcome::Fault(EngineFault::Frame(error));
        }
        self.arena.charge_bc_position(entry.frame_bytes);
        self.max_depth = self.arena.depth();
        self.stop_state = OwnerStopState::Running;
        'frame: loop {
            let Some(top) = self.arena.top_vm_id() else {
                return self.settle(VmOutcome::Fault(EngineFault::NoFrame));
            };
            let Some(frame_code) = self.arena.vm(top).map(|vm| vm.code) else {
                return self.settle(VmOutcome::Fault(EngineFault::StaleFrame));
            };
            let program = match code.program(frame_code) {
                Ok(program) => program,
                Err(_) => {
                    return self
                        .settle(VmOutcome::Fault(EngineFault::DispatchContract(Op::Return)));
                }
            };
            loop {
                match self.step(ctx, code, top, program) {
                    Step::Continue => {}
                    Step::Frame => continue 'frame,
                    Step::Refill(class) => {
                        // The poll is the V5 publish/stop/restore body; it must
                        // not be assumed to make progress (a poisoned or
                        // empty-handed resume refuses again, and that refusal
                        // is the poll fault here, never a silent loop: the
                        // production poll blocks until granted or errors).
                        self.stop_state = OwnerStopState::Stopped;
                        let result = self.poll.vm_refill(&self.arena, class);
                        if let Err(error) = result {
                            return self.settle(VmOutcome::Fault(EngineFault::Poll(error)));
                        }
                        self.stop_state = OwnerStopState::Running;
                    }
                    Step::Stop(outcome) => return self.settle(outcome),
                }
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
        self.stop_state = OwnerStopState::Stopped;
        match &outcome {
            VmOutcome::Done { pending } => self.state = RunState::Finished { handle: *pending },
            VmOutcome::Fault(EngineFault::Busy | EngineFault::Poisoned) => {}
            VmOutcome::Fault(
                EngineFault::EscapeStackEffect { .. }
                | EngineFault::EscapeStackUnderflow { .. }
                | EngineFault::EscapeRecordContract(_)
                | EngineFault::DispatchContract(_),
            ) => self.state = RunState::Poisoned,
            VmOutcome::Signaled(_)
            | VmOutcome::VoidVariable { .. }
            | VmOutcome::MissingCore { .. }
            | VmOutcome::Fault(_) => {
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
    ///
    /// N-v2-idle-slab-undo: an owner binding record names a slab record that
    /// roots its value, so dropping the entry alone leaks it (GNU `unbind_to`
    /// undoes each record as it leaves, eval.c:3940-3954). Each is undone
    /// innermost first through the unbind-time re-validation. A serial marker
    /// is the serial side's, whose own records a signal's unwind already
    /// removed (the `Flow` arm applied no deltas): the marker is dropped. A
    /// record the re-validation refuses needs the serial side, which the
    /// idle unwind cannot reach (V8 runs it with Lisp): `Poisoned`. What
    /// stays then: the binding LIFO, the slab and the roots - the frame and
    /// its handlers are already gone when the record loop runs.
    ///
    /// V8 L7 / E5 contract: an unhandled `Flow` means the signal escaped past
    /// every owner handler (no `Caught` landing pad matched). E5's thread-entry
    /// boundary unwinds serial records (and any watched/redirected owner
    /// bindings requiring Lisp) before returning `Flow`; `unwind_to_idle` then
    /// restores remaining untrapped owner Plain bindings inline and fails
    /// closed (`Poisoned`) if any watched/redirected owner record was left
    /// behind.
    fn unwind_to_idle(&mut self) -> Result<(), EngineFault> {
        while let Some(top) = self.arena.top_vm_id() {
            if self.arena.vm_truncate_operands(top, 0).is_none() || self.arena.pop(top).is_none() {
                self.state = RunState::Poisoned;
                return Err(EngineFault::StaleFrame);
            }
        }
        self.arena.reset_bc_position();
        // truncate(EMPTY) is correct only because VmExec owns its FrameArena exclusively.
        self.arena.handlers_mut().truncate(HandlerMark::EMPTY);
        while let Some(top) = self.arena.bindings().above(OwnerRecordMark::EMPTY).last() {
            match top {
                OwnerRecord::Plain(record) => {
                    match effective_plain_unbind_checked(&mut self.plain, record) {
                        Ok(PlainUnbind::Done) => {
                            self.arena.bindings_mut().pop();
                        }
                        Ok(PlainUnbind::NeedsSerial { .. })
                        | Err(PlainUnbindError::RecordMismatch) => {
                            tracing::error!(
                                record = ?record,
                                "the idle unwind met an owner record the serial side must undo"
                            );
                            self.state = RunState::Poisoned;
                            return Err(EngineFault::EscapeRecordContract(RecordStack::Bindings));
                        }
                        Err(PlainUnbindError::Access(error)) => {
                            tracing::error!(
                                record = ?record,
                                error = ?error,
                                "the idle unwind hit a canonical Plain authority failure"
                            );
                            self.state = RunState::Poisoned;
                            return Err(EngineFault::PlainAccess(error));
                        }
                    }
                }
                OwnerRecord::Serial(_) => {
                    self.arena.bindings_mut().pop();
                }
            }
        }
        self.arena.bindings_mut().truncate(OwnerRecordMark::EMPTY);
        if let Err(error) = self.arena.roots_mut().truncate(0) {
            self.state = RunState::Poisoned;
            return Err(EngineFault::Root(error));
        }
        self.state = RunState::Idle;
        Ok(())
    }

    fn step<C: VmCode>(
        &mut self,
        ctx: &mut Context,
        code: &C,
        frame: FrameId,
        program: VmProgram<'_>,
    ) -> Step {
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
            ParallelArm::World(WorldSplit::PlainVar(access)) => {
                self.step_plain(access, op, frame, operands, branch_pc, program)
            }
            ParallelArm::World(WorldSplit::Return(_)) => {
                self.step_return(op, frame, operands, branch_pc, program)
            }
            ParallelArm::World(WorldSplit::Escape) if matches!(op, Op::Call(_)) => {
                self.step_call(op, code, frame, operands, branch_pc, program)
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
            self.stop_state = OwnerStopState::Stopped;
            let poll = self.poll.vm_back_edge(&self.arena);
            if let Err(error) = poll {
                return Step::Stop(VmOutcome::Fault(EngineFault::Poll(error)));
            }
            self.stop_state = OwnerStopState::Running;
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
            // Nothing was consumed: `args` borrows the segment and the
            // store never ran, so pc, operands and frame are as they were.
            Err(ArithRefusal::Exhausted(class)) => Step::Refill(class),
            Err(ArithRefusal::Fault(error)) => {
                Step::Stop(VmOutcome::Fault(EngineFault::Poll(error.into())))
            }
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

    /// V2: the four Plain variable opcodes on the owner slab (E4 §2.4, the
    /// composed rule through [`VmPlain`]). A key the admission mints runs
    /// owner-locally with no poll, no request and no escape (GNU's
    /// `Bvarref`/`Bvarset`/`Bvarbind` on an untrapped PLAINVAL are a value
    /// cell read, a value cell store and a `specbind`, bytecode.c:638-647,
    /// :710-719, :743-746); a symbol the admission refuses takes the ONE
    /// escape with the refusal and the serial side performs the access;
    /// `Unbind n` walks the frame's own records (R1b).
    fn step_plain(
        &mut self,
        access: PlainVarAccess,
        op: Op,
        frame: FrameId,
        operands: OperandCursor,
        branch_pc: usize,
        program: VmProgram<'_>,
    ) -> Step {
        match op {
            Op::VarRef(_) => self.step_var_ref(access, op, frame, operands, branch_pc, program),
            Op::VarSet(_) | Op::VarBind(_) => {
                self.step_var_write(access, op, frame, operands, branch_pc, program)
            }
            Op::Unbind(_) => self.step_unbind(op, frame, branch_pc),
            _ => Step::Stop(VmOutcome::Fault(EngineFault::DispatchContract(op))),
        }
    }

    /// The symbol a Plain variable opcode names, resolved as the serial
    /// `sym_id_at` (bytecode/vm.rs:10882-10893): the constant as a bare
    /// symbol or the symbol inside a symbol-with-pos, else `nil`. The
    /// stepper invents no signal for a constant that is no symbol (GNU's
    /// `XBARE_SYMBOL` on one is unchecked, bytecode.c:640): `nil` is a
    /// constant symbol, so the admission refuses it and the serial side
    /// runs the opcode.
    fn plain_symbol(program: VmProgram<'_>, op: &Op) -> SymId {
        let index = match op {
            Op::VarRef(index) | Op::VarSet(index) | Op::VarBind(index) => usize::from(*index),
            _ => return NIL_SYM_ID,
        };
        let Some(constant) = program.constants.get(index) else {
            return NIL_SYM_ID;
        };
        let bare = constant.as_symbol_with_pos_sym().unwrap_or(*constant);
        bare.as_bare_symbol_id().unwrap_or(NIL_SYM_ID)
    }

    /// The key for SYM: a cached key that is still valid for the generation
    /// read now is used as is; otherwise the admission re-derives it and
    /// the cache takes the result. A refusal drops the cached entry and is
    /// the caller's ONE escape: nothing is read or written through a stale
    /// key. `admission` is the classification E4-P2 runs under the real
    /// symbol-reader authority.
    fn plain_key(&mut self, sym: SymId) -> Result<EffectivePlainKey<'era>, PlainRefusal> {
        let (era, shape) = self.plain.generation();
        if let Some((_, key)) = self.keys.iter().find(|(cached, _)| *cached == sym)
            && key.is_valid_for(era, shape)
        {
            return Ok(*key);
        }
        let admission = self.plain.admission(sym);
        match EffectivePlainKey::from_admission(&admission, era, shape) {
            Ok(key) => {
                match self.keys.iter_mut().find(|(cached, _)| *cached == sym) {
                    Some(entry) => entry.1 = key,
                    None => self.keys.push((sym, key)),
                }
                Ok(key)
            }
            Err(refusal) => {
                self.keys.retain(|(cached, _)| *cached != sym);
                Err(refusal)
            }
        }
    }

    /// `Bvarref` (bytecode.c:638-647): a PLAINVAL with a value is pushed;
    /// anything else reaches `Fsymbol_value`, which signals
    /// `void-variable` with the symbol (data.c:1645-1657). Serial arm:
    /// bytecode/vm.rs:5665-5675.
    fn step_var_ref(
        &mut self,
        access: PlainVarAccess,
        op: Op,
        frame: FrameId,
        operands: OperandCursor,
        branch_pc: usize,
        program: VmProgram<'_>,
    ) -> Step {
        let sym = Self::plain_symbol(program, &op);
        let key = match self.plain_key(sym) {
            Ok(key) => key,
            Err(refusal) => {
                return self.escape(
                    op,
                    EscapeWhy::PlainRefused { access, refusal },
                    frame,
                    operands,
                    branch_pc,
                    program,
                );
            }
        };
        let len = operands.len();
        let read = match effective_plain_read(&mut self.plain, &key) {
            Ok(read) => read,
            Err(error) => return Step::Stop(VmOutcome::Fault(EngineFault::PlainAccess(error))),
        };
        let Some(value) = read.value().filter(|value| !value.is_unbound()) else {
            return Self::void_variable(sym);
        };
        Self::stop(self.store(frame, branch_pc, len, &[value]))
    }

    fn void_variable(sym: SymId) -> Step {
        Step::Stop(VmOutcome::VoidVariable {
            symbol: Value::from_sym_id(sym),
        })
    }

    /// `Bvarset` (bytecode.c:710-719): a PLAINVAL takes `SET_SYMBOL_VAL`,
    /// anything else `set_internal`; `Bvarbind` (bytecode.c:743-746) is
    /// `specbind (vectorp[arg], POP)`, which for a PLAINVAL saves the old
    /// value and stores the new one (eval.c:3666-3672). Both consume their
    /// value first. Serial arms: bytecode/vm.rs:5676-5733. A missing value
    /// is malformed code: the stepper's `Invalid byte-code`, before any key
    /// or slot access.
    ///
    /// A bind pairs two stores (C2): the slab record is pushed and holds the
    /// binding, and the frame's LIFO then takes the `SlotRecord` naming
    /// that record's id. The serial arm's specpdl entry and frame mark
    /// (bytecode/vm.rs:5722-5734) are the same two facts.
    fn step_var_write(
        &mut self,
        access: PlainVarAccess,
        op: Op,
        frame: FrameId,
        operands: OperandCursor,
        branch_pc: usize,
        program: VmProgram<'_>,
    ) -> Step {
        let len = operands.len();
        let Some(value) = self.peek(operands, 0) else {
            return invalid();
        };
        let sym = Self::plain_symbol(program, &op);
        let key = match self.plain_key(sym) {
            Ok(key) => key,
            Err(refusal) => {
                return self.escape(
                    op,
                    EscapeWhy::PlainRefused { access, refusal },
                    frame,
                    operands,
                    branch_pc,
                    program,
                );
            }
        };
        if matches!(op, Op::VarBind(_)) {
            let record = effective_plain_bind(&mut self.plain, &key, value);
            self.arena.bindings_mut().push(record);
        } else if let Err(error) = effective_plain_write(&mut self.plain, &key, value) {
            return Step::Stop(VmOutcome::Fault(EngineFault::PlainAccess(error)));
        }
        Self::stop(self.store(frame, branch_pc, len - 1, &[]))
    }

    fn record_contract_fault() -> Step {
        Step::Stop(VmOutcome::Fault(EngineFault::EscapeRecordContract(
            RecordStack::Bindings,
        )))
    }

    /// `Bunbind n` (bytecode.c:843-852) is `unbind_to (SPECPDL_INDEX () - n)`:
    /// n records of any kind leave in LIFO order, each undone as it leaves
    /// (eval.c:3946-3957, a loop to the depth target). The serial arm
    /// consumes n of the frame's own marks and unwinds to the depth the
    /// n-th saved (bytecode/vm.rs:5734-5742, frame_bindings.rs:95-113): zero
    /// is a no-op, more than the frame holds is `Invalid byte-code` and
    /// unwinds nothing. R1b: an owner binding record is undone on the owner
    /// slab with no escape when the unbind-time re-validation admits it
    /// (N-v2-unbind-revalidate); one it refuses is ONE `PlainUnbind` escape
    /// naming the symbol and the record, served by the serial unbind's slow
    /// path, the entry leaving before the escape (GNU's order) and the walk
    /// checking the record is gone after. Each maximal run of consecutive
    /// serial records is ONE escape whose request names exactly those
    /// tokens, innermost first, and whose completion must pop exactly them
    /// (R1 checks the popped run against the request): the walk then checks
    /// that none of the named tokens is left above the frame's base. A
    /// record a completion pushes lands above the walk and is walked next,
    /// in addition to n, as GNU's loop to a fixed depth.
    fn step_unbind(&mut self, op: Op, frame: FrameId, branch_pc: usize) -> Step {
        let Op::Unbind(count) = op else {
            return Step::Stop(VmOutcome::Fault(EngineFault::DispatchContract(op)));
        };
        let Some((bind_base, handler_base)) = self
            .arena
            .vm(frame)
            .map(|vm| (vm.bind_base, vm.handler_base))
        else {
            return Step::Stop(VmOutcome::Fault(EngineFault::StaleFrame));
        };
        let count = usize::from(count);
        if count > self.arena.bindings().above(bind_base).count() {
            return invalid();
        }
        let mut remaining = count;
        while remaining > 0 {
            // Read the top again after every escape: the serial side may
            // have changed the run.
            let Some(top) = self.arena.bindings().above(bind_base).last() else {
                return Self::record_contract_fault();
            };
            match top {
                OwnerRecord::Plain(record) => {
                    // N-v2-unbind-revalidate: re-validate the symbol before
                    // anything is taken. GNU's `do_one_unbind` for a LET
                    // re-reads it as the binding leaves (eval.c:3848-3870); a
                    // watcher added while the binding was live, a retired
                    // projection or a dead channel all take the serial
                    // unbind's slow path.
                    let symbol = record.symbol;
                    match effective_plain_unbind_checked(&mut self.plain, record) {
                        Ok(PlainUnbind::Done) => {
                            // C2: the slab record this entry names is taken
                            // from the top of the slab's LIFO and undone;
                            // only then does the entry leave.
                            self.arena.bindings_mut().pop();
                            remaining -= 1;
                        }
                        Ok(PlainUnbind::NeedsSerial { .. }) => {
                            // The record handed off is the slab's top (C2);
                            // checked before anything is touched.
                            if self.plain.top_record() != Some(record.record) {
                                tracing::error!(
                                    op = ?op,
                                    "a Plain binding entry does not name the slab's top record"
                                );
                                return Self::record_contract_fault();
                            }
                            // GNU's `unbind_to` (eval.c:3940-3954) decrements
                            // `specpdl_ptr` BEFORE `do_one_unbind`: an error
                            // in a watcher never unbinds the entry twice.
                            self.arena.bindings_mut().pop();
                            let len_before = self.arena.bindings().above(bind_base).count();
                            match self.unwind_plain_escape(symbol, record.record, &op, branch_pc) {
                                Ok(UnwindEscapeOutcome::Done) => {}
                                Ok(UnwindEscapeOutcome::Restart { target, value }) => {
                                    return self.unwind_to_handler(target, value, op, branch_pc);
                                }
                                Ok(UnwindEscapeOutcome::Flow(flow)) => {
                                    return Step::Stop(VmOutcome::Signaled(flow));
                                }
                                Err(fault) => return Step::Stop(VmOutcome::Fault(fault)),
                            }
                            remaining -= 1;
                            let pushed = self.arena.bindings().above(bind_base).count() as isize
                                - len_before as isize;
                            if pushed < 0 {
                                tracing::error!(
                                    op = ?op,
                                    "a PlainUnbind escape left fewer records than it found"
                                );
                                return Self::record_contract_fault();
                            }
                            remaining += pushed as usize;
                        }
                        Err(PlainUnbindError::RecordMismatch) => {
                            return Self::record_contract_fault();
                        }
                        Err(PlainUnbindError::Access(error)) => {
                            return Step::Stop(VmOutcome::Fault(EngineFault::PlainAccess(error)));
                        }
                    }
                }
                OwnerRecord::Serial(innermost) => {
                    // The maximal run from the top, at most `remaining` long.
                    let mut records = vec![innermost];
                    for record in self
                        .arena
                        .bindings()
                        .above(bind_base)
                        .rev()
                        .skip(1)
                        .take(remaining - 1)
                    {
                        match record {
                            OwnerRecord::Serial(next) => records.push(next),
                            OwnerRecord::Plain(_) => break,
                        }
                    }
                    let run = records.len();
                    let named = records.clone();
                    let len_before = self.arena.bindings().above(bind_base).count();
                    let unwind = SerialUnwind {
                        pop_handlers: Vec::new(),
                        unbind: records,
                    };
                    match self.unwind_serial_escape(unwind, bind_base, handler_base, &op, branch_pc)
                    {
                        Ok(UnwindEscapeOutcome::Done) => {}
                        Ok(UnwindEscapeOutcome::Restart { target, value }) => {
                            // HIGH-1: `unwind_serial_escape` already popped
                            // the non-empty prefix of `unwind.unbind` removed
                            // before the cleanup signaled (GNU `eval.c:3955`),
                            // so `unwind_to_handler` never unbinds it twice.
                            return self.unwind_to_handler(target, value, op, branch_pc);
                        }
                        Ok(UnwindEscapeOutcome::Flow(flow)) => {
                            return Step::Stop(VmOutcome::Signaled(flow));
                        }
                        Err(fault) => return Step::Stop(VmOutcome::Fault(fault)),
                    }
                    let still_named = self
                        .arena
                        .bindings()
                        .above(bind_base)
                        .any(|record| matches!(record, OwnerRecord::Serial(seen) if named.contains(&seen)));
                    if still_named {
                        tracing::error!(
                            op = ?op,
                            run,
                            "an unbind escape left a serial record it was asked to remove"
                        );
                        return Self::record_contract_fault();
                    }
                    remaining -= run;
                    let pushed = self.arena.bindings().above(bind_base).count() as isize
                        - len_before as isize
                        + run as isize;
                    if pushed < 0 {
                        tracing::error!(
                            op = ?op,
                            "an unbind escape left fewer records than it found"
                        );
                        return Self::record_contract_fault();
                    }
                    remaining += pushed as usize;
                }
            }
        }
        Self::stop(self.next_pc(frame, branch_pc))
    }

    /// Owner-local bytecode->bytecode `Call(n)` (V7 §6, C4/C5/C7):
    /// 1. Underflow check before any poll.
    /// 2. `poll_vm_call` once (GNU `Bcall` `maybe_quit`, `bytecode.c:781`).
    /// 3. Read-only `VmCode::classify_call` before any mutation.
    /// 4. On `Escape(reason)`: record `reason` and take the ONE escape stub
    ///    without repeating `vm_call` (caller untouched).
    /// 5. On `OwnerLocal { code, nonrest, frame_bytes }`: commit child `VmFrame`
    ///    (`function = designator`, `code = resolved closure`,
    ///    `return_to = Caller { consumed: n + 1 }`), copy `n` args + nil-pad
    ///    optionals, charge `bc_position`, and return `Step::Frame`.
    fn step_call<C: VmCode>(
        &mut self,
        op: Op,
        code: &C,
        frame: FrameId,
        operands: OperandCursor,
        branch_pc: usize,
        program: VmProgram<'_>,
    ) -> Step {
        let Op::Call(n) = op else {
            return Step::Stop(VmOutcome::Fault(EngineFault::DispatchContract(op)));
        };
        let nargs = usize::from(n);
        let consumed = nargs + 1;
        if operands.len() < consumed {
            return invalid();
        }
        self.stop_state = OwnerStopState::Stopped;
        let call_poll = self.poll.vm_call(&self.arena);
        if let Err(error) = call_poll {
            return Step::Stop(VmOutcome::Fault(EngineFault::Poll(error)));
        }
        self.stop_state = OwnerStopState::Running;

        let segment = self.arena.operand_segment(operands);
        let func_slot = segment.len() - consumed;
        let designator = segment[func_slot];
        let (callee_code, nonrest, frame_bytes) =
            match code.classify_call(designator, n, &self.call_env, &self.arena) {
                CallClass::OwnerLocal {
                    code,
                    nonrest,
                    frame_bytes,
                } => (code, usize::from(nonrest), frame_bytes),
                CallClass::Escape(reason) => {
                    self.call_escapes.push(reason);
                    return self.escape_inner(
                        op,
                        EscapeWhy::World(WorldSplit::Escape),
                        frame,
                        operands,
                        branch_pc,
                        program,
                        true,
                    );
                }
            };

        let args_src_start = operands.base() + func_slot + 1;
        let child_base = self.arena.operand_len();
        let child_frame = VmFrame::new(
            designator,
            callee_code,
            Pc::new(0),
            OperandCursor::new(child_base, 0),
            self.arena.bindings().mark(),
            self.arena.handlers().mark(),
            PollBudget::new(NonZeroU32::MIN),
        )
        .with_return_to(ReturnTo::Caller {
            consumed: u32::from(n) + 1,
        })
        .with_frame_bytes(frame_bytes);
        // Advance the caller's saved pc to the instruction after `Call(n)`
        // before pushing the child frame (`with_vm` requires `frame` on top).
        if let Err(fault) = self.next_pc(frame, branch_pc) {
            return Step::Stop(VmOutcome::Fault(fault));
        }
        let child_id = match self.arena.push_vm(child_frame) {
            Ok(id) => id,
            Err(_) => {
                // §6 step 4.3: a refusal from `push` cannot happen after a
                // successful classification; if it does, it is a contract
                // fault, not an escape.
                let _ = self.set_pc(frame, branch_pc);
                return Step::Stop(VmOutcome::Fault(EngineFault::DispatchContract(op)));
            }
        };
        if self
            .arena
            .vm_copy_args_and_pad(child_id, args_src_start, nargs, nonrest)
            .is_none()
        {
            return Step::Stop(VmOutcome::Fault(EngineFault::DispatchContract(op)));
        }
        self.arena.charge_bc_position(frame_bytes);
        self.max_depth = self.max_depth.max(self.arena.depth());
        Step::Frame
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
        self.escape_inner(op, why, frame, operands, branch_pc, program, false)
    }

    #[allow(clippy::too_many_arguments)]
    fn escape_inner(
        &mut self,
        op: Op,
        why: EscapeWhy,
        frame: FrameId,
        operands: OperandCursor,
        branch_pc: usize,
        program: VmProgram<'_>,
        call_already_polled: bool,
    ) -> Step {
        let effect = static_stack_effect(&op);
        // Malformed code: an opcode reached without the operands its static
        // effect consumes is the Scalar and Sealed arms' `Invalid byte-code`,
        // refused before any poll or request. The underflow check below is
        // the backstop.
        if operands.len() < usize::from(effect.consumed) {
            return invalid();
        }
        // The owner is now stopped across the call/serial-request polls and
        // the escape park until checked deltas/debug-on-exit updates apply.
        self.stop_state = OwnerStopState::Stopped;
        if !call_already_polled
            && matches!(
                op,
                Op::Call(_) | Op::Apply(_) | Op::CallBuiltin(..) | Op::CallBuiltinSym(..)
            )
            && let Err(error) = self.poll.vm_call(&self.arena)
        {
            return Step::Stop(VmOutcome::Fault(EngineFault::Poll(error)));
        }
        // The serial-request poll does not replace the escape: a successful
        // poll still records exactly one request.
        if let Err(error) = self.poll.serial_request(&self.arena) {
            return Step::Stop(VmOutcome::Fault(EngineFault::Poll(error)));
        }
        // The request's `why` names what the serial side may pop; the
        // completion is checked against it below, before anything is applied.
        let named = why.clone();
        let request = VmEscapeRequest {
            op: op.clone(),
            why,
            pc: Pc::new(branch_pc),
            frame,
            operands,
        };
        let completion = self.escape.escape(request, &self.arena, &mut self.plain);
        let debug_updates = self.escape.take_debug_on_exit_updates();
        let (consumed, produced, jump, records, handlers) = match completion {
            EscapeCompletion::Flow(flow) => return Step::Stop(VmOutcome::Signaled(flow)),
            EscapeCompletion::Caught { handler, value } => {
                // V8 L2: `EscapeCompletion::Caught` structurally carries zero
                // record or handler deltas.
                if !debug_updates.is_empty() {
                    let mut stopped = StoppedOwnerBacktrace::from_escape_park(&mut self.arena);
                    for update in debug_updates {
                        let _ = stopped.set_debug_on_exit(update.frame, update.debug_on_exit);
                    }
                }
                if let Some(env) = self.escape.refresh_call_env() {
                    self.set_call_env(env);
                }
                self.stop_state = OwnerStopState::Running;
                return self.unwind_to_handler(handler, value, op, branch_pc);
            }
            EscapeCompletion::CleanupCaught {
                handler,
                value,
                popped_records,
                popped_handlers,
            } => {
                // HIGH-1: `CleanupCaught` is valid ONLY for
                // `Return(NeedsSerialCleanup)` when the returning frame's
                // serial cleanup or `debug-on-exit` signals into a caller's
                // owner handler.
                if !matches!(
                    named,
                    EscapeWhy::World(WorldSplit::Return(ReturnDisposition::NeedsSerialCleanup))
                ) {
                    tracing::error!(
                        op = ?op,
                        "CleanupCaught is only permitted for Return(NeedsSerialCleanup)"
                    );
                    return Step::Stop(VmOutcome::Fault(EngineFault::DispatchContract(op)));
                }
                let Some((bind_base, handler_base)) = self
                    .arena
                    .vm(frame)
                    .map(|vm| (vm.bind_base, vm.handler_base))
                else {
                    return Step::Stop(VmOutcome::Fault(EngineFault::StaleFrame));
                };
                // V8 r2 LOW-1: read-only check-before-mutate LIFO walk from the
                // top above `bind_base` (GNU `src/eval.c:3946-3958`). Each top
                // entry must match either a dead `Plain` record already taken
                // by the serial Return cleanup (`!plain.record_live`) or the
                // next expected token in `popped_records`. Every token in
                // `popped_records` must be consumed and zero dead `Plain`
                // entries may remain below the prefix before EITHER LIFO is
                // mutated.
                if !self
                    .arena
                    .handlers()
                    .serial_run_matches_above(handler_base, &popped_handlers)
                {
                    return Step::Stop(VmOutcome::Fault(EngineFault::EscapeRecordContract(
                        RecordStack::Handlers,
                    )));
                }
                let total_above = self.arena.bindings().above(bind_base).count();
                let mut popped_idx = 0usize;
                let mut remove_count = 0usize;
                for entry in self.arena.bindings().above(bind_base).rev() {
                    match entry {
                        OwnerRecord::Plain(slot) if !self.plain.record_live(slot.record) => {
                            remove_count += 1;
                        }
                        OwnerRecord::Serial(tok)
                            if popped_records.get(popped_idx) == Some(&tok) =>
                        {
                            popped_idx += 1;
                            remove_count += 1;
                        }
                        _ => break,
                    }
                }
                if popped_idx != popped_records.len() {
                    return Step::Stop(VmOutcome::Fault(EngineFault::EscapeRecordContract(
                        RecordStack::Bindings,
                    )));
                }
                let remaining_above = total_above - remove_count;
                if self
                    .arena
                    .bindings()
                    .above(bind_base)
                    .take(remaining_above)
                    .any(|record| {
                        matches!(record, OwnerRecord::Plain(slot) if !self.plain.record_live(slot.record))
                    })
                {
                    return Step::Stop(VmOutcome::Fault(EngineFault::EscapeRecordContract(
                        RecordStack::Bindings,
                    )));
                }
                // All checks passed: apply handler and binding pops atomically.
                if self
                    .arena
                    .handlers_mut()
                    .pop_serial_run_above(handler_base, &popped_handlers)
                    .is_err()
                {
                    return Step::Stop(VmOutcome::Fault(EngineFault::EscapeRecordContract(
                        RecordStack::Handlers,
                    )));
                }
                for _ in 0..remove_count {
                    self.arena.bindings_mut().pop();
                }
                if !debug_updates.is_empty() {
                    let mut stopped = StoppedOwnerBacktrace::from_escape_park(&mut self.arena);
                    for update in debug_updates {
                        let _ = stopped.set_debug_on_exit(update.frame, update.debug_on_exit);
                    }
                }
                if let Some(env) = self.escape.refresh_call_env() {
                    self.set_call_env(env);
                }
                self.stop_state = OwnerStopState::Running;
                return self.unwind_to_handler(handler, value, op, branch_pc);
            }
            EscapeCompletion::Completed {
                consumed,
                produced,
                jump,
                records,
                handlers,
            } => (consumed, produced, jump, records, handlers),
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
        let Some((available, bind_base, handler_base)) = self.arena.vm(frame).and_then(|vm| {
            Some((
                u16::try_from(vm.operands.len()).ok()?,
                vm.bind_base,
                vm.handler_base,
            ))
        }) else {
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
        let handler_dest = match &op {
            Op::PushCatch(dest) | Op::PushConditionCase(dest) | Op::PushConditionCaseRaw(dest) => {
                Some(*dest)
            }
            _ => None,
        };
        // R1: both LIFOs are checked before either is touched.
        let (records_popped_ok, handlers_popped_ok) = if handler_dest.is_some() {
            // V8 r2 LOW-2a: GNU `push_handler_nosignal` (`src/eval.c:1840-1867`)
            // installs one handler and never pops existing specpdl records or
            // handlers.
            (records.popped.is_empty(), handlers.popped.is_empty())
        } else {
            match &named {
                // A refused Plain access pushes (a refused bind) or reads; it
                // never pops a record. A sealed cons op pops none.
                EscapeWhy::PlainRefused { .. } | EscapeWhy::UnimplementedSealed(_) => {
                    (records.popped.is_empty(), handlers.popped.is_empty())
                }
                // A World escape names no token: `WorldSplit::Escape` is one
                // variant for every World op, so its pop is checked as the top
                // run strictly above `bind_base`/`handler_base` (V7 F6).
                _ => (true, true),
            }
        };
        let contract = if !self
            .arena
            .bindings()
            .serial_run_matches_above(bind_base, &records.popped)
            || !records_popped_ok
        {
            Some(RecordStack::Bindings)
        } else if !self
            .arena
            .handlers()
            .serial_run_matches_above(handler_base, &handlers.popped)
            || !handlers_popped_ok
        {
            Some(RecordStack::Handlers)
        } else {
            None
        };
        if let Some(stack) = contract {
            tracing::error!(
                op = ?op,
                stack = ?stack,
                named = ?named,
                "escape completion pops serial tokens that are not the top run, or not the ones the request named"
            );
            return Step::Stop(VmOutcome::Fault(EngineFault::EscapeRecordContract(stack)));
        }
        // V8 L1 (MEDIUM-1.2): ONLY `PushCatch`, `PushConditionCase`, and
        // `PushConditionCaseRaw` may push a serial handler token, and they
        // must push EXACTLY ONE token (structural via `Option`) with ZERO
        // specpdl records pushed (GNU `push_handler_nosignal`,
        // `src/eval.c:1840-1867`).
        let pushed_handler = match (handler_dest, handlers.pushed) {
            (Some(dest), Some(token)) => {
                if !records.pushed.is_empty() {
                    tracing::error!(
                        op = ?op,
                        "handler-push opcode cannot push specpdl binding records"
                    );
                    return Step::Stop(VmOutcome::Fault(EngineFault::EscapeRecordContract(
                        RecordStack::Bindings,
                    )));
                }
                Some((dest, token))
            }
            (Some(_), None) => {
                tracing::error!(
                    op = ?op,
                    "handler-push opcode must push exactly one serial handler token"
                );
                return Step::Stop(VmOutcome::Fault(EngineFault::DispatchContract(op)));
            }
            (None, Some(_)) => {
                tracing::error!(
                    op = ?op,
                    "only handler-push opcodes may leave a surviving serial handler token"
                );
                return Step::Stop(VmOutcome::Fault(EngineFault::DispatchContract(op)));
            }
            (None, None) => None,
        };
        if self
            .arena
            .bindings_mut()
            .pop_serial_run_above(bind_base, &records.popped)
            .is_err()
        {
            return Step::Stop(VmOutcome::Fault(EngineFault::EscapeRecordContract(
                RecordStack::Bindings,
            )));
        }
        if self
            .arena
            .handlers_mut()
            .pop_serial_run_above(handler_base, &handlers.popped)
            .is_err()
        {
            return Step::Stop(VmOutcome::Fault(EngineFault::EscapeRecordContract(
                RecordStack::Handlers,
            )));
        }
        for token in &records.pushed {
            self.arena.bindings_mut().push(*token);
        }
        if let Some((dest, token)) = pushed_handler {
            // V8 L1 & S2: GNU `Bpushcatch`/`Bpushconditioncase` (`src/bytecode.c:974-976`)
            // computes `c->bytecode_top = top` AFTER `POP` of the tag/handlers operand,
            // and `push_handler_nosignal` (`src/eval.c:1857-1859`) records
            // `f_lisp_eval_depth`, `pdlcount`, and `act_rec`.
            let pad = LandingPad {
                frame,
                dest,
                operand_len: available as usize - consumed as usize,
                bind_mark: self.arena.bindings().mark(),
                depth: self.arena.depth(),
            };
            self.arena
                .handlers_mut()
                .push(OwnerHandler::Serial { token, pad });
        }
        if !debug_updates.is_empty() {
            let mut stopped = StoppedOwnerBacktrace::from_escape_park(&mut self.arena);
            for update in debug_updates {
                let _ = stopped.set_debug_on_exit(update.frame, update.debug_on_exit);
            }
        }
        if let Some(env) = self.escape.refresh_call_env() {
            self.set_call_env(env);
        }
        self.stop_state = OwnerStopState::Running;
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

    /// The disposition of a `Return` in `frame` (design §5): the serial
    /// cleanup when GNU would run it, else the owner-local unwind. GNU's
    /// `Breturn` (bytecode.c:892-926) runs the debugger when the frame's
    /// backtrace entry wants debug-on-exit (:900-901), and assumes the
    /// frame's dynamic state is already gone: a frame that still owns a
    /// serial record (R1c) or handler (R1d) must run it on return, through
    /// the serial slow arm (E4-DESIGN :226-228). An owner binding whose
    /// unbind-time re-validation refuses it needs the serial side too
    /// (N-v2-unbind-revalidate). `None` for a stale frame.
    fn return_disposition(&self, frame: FrameId) -> Option<ReturnDisposition> {
        let vm = self.arena.vm(frame)?;
        let needs_serial = vm.exit_flags.contains(ExitFlags::DEBUG_ON_EXIT)
            || self.arena.bindings().has_serial_above(vm.bind_base)
            || self.arena.handlers().has_serial_above(vm.handler_base)
            || self.arena.bindings().above(vm.bind_base).any(|record| {
                matches!(record, OwnerRecord::Plain(entry)
                    if self.plain.unbind_needs_serial(entry.symbol))
            });
        Some(if needs_serial {
            ReturnDisposition::NeedsSerialCleanup
        } else {
            ReturnDisposition::OwnerLocal
        })
    }

    /// `Return` (V4): `val` is the frame's top operand (GNU `Breturn`,
    /// bytecode.c:897); a frame with none is malformed code. The disposition
    /// decides the arm.
    fn step_return(
        &mut self,
        op: Op,
        frame: FrameId,
        operands: OperandCursor,
        branch_pc: usize,
        program: VmProgram<'_>,
    ) -> Step {
        let Some(value) = self.arena.operand_segment(operands).last().copied() else {
            return invalid();
        };
        match self.return_disposition(frame) {
            None => Step::Stop(VmOutcome::Fault(EngineFault::StaleFrame)),
            Some(ReturnDisposition::OwnerLocal) => self.return_owner_local(frame, value),
            Some(ReturnDisposition::NeedsSerialCleanup) => {
                // Before the escape, every owner entry above the base names
                // a live record (C2's pairing). A record a previous exchange
                // took under the stepper is the contract fault here, where it
                // is still distinguishable from one the cleanup takes.
                let bind_base = self.arena.vm(frame).map(|vm| vm.bind_base);
                let Some(bind_base) = bind_base else {
                    return Step::Stop(VmOutcome::Fault(EngineFault::StaleFrame));
                };
                if self.arena.bindings().above(bind_base).any(|record| {
                    matches!(record, OwnerRecord::Plain(entry) if !self.plain.record_live(entry.record))
                }) {
                    tracing::error!(
                        op = ?op,
                        "a Return found an owner entry whose record the slab no longer holds"
                    );
                    return Step::Stop(VmOutcome::Fault(EngineFault::EscapeRecordContract(
                        RecordStack::Bindings,
                    )));
                }
                self.escape(
                    op,
                    EscapeWhy::World(WorldSplit::Return(ReturnDisposition::NeedsSerialCleanup)),
                    frame,
                    operands,
                    branch_pc,
                    program,
                )
            }
        }
    }

    /// The owner-local `Return` (design §5): the frame's owner arena state
    /// goes with it - handlers to `handler_base`, the owner binding records
    /// undone innermost-first through the unbind-time re-validation, the
    /// operand segment dropped, the frame popped (`CallDepth::leave`) - and
    /// the value goes to the owner entry: the bottom frame's `exit`
    /// (bytecode.c:925-926). On compiled code every truncation is a no-op;
    /// on malformed code it is the leak guard.
    fn return_owner_local(&mut self, frame: FrameId, value: Value) -> Step {
        let Some((bind_base, handler_base)) = self
            .arena
            .vm(frame)
            .map(|vm| (vm.bind_base, vm.handler_base))
        else {
            return Step::Stop(VmOutcome::Fault(EngineFault::StaleFrame));
        };
        // Records first, then handlers: a fault in the record unwind keeps
        // the scene whole, handlers included.
        if let Err(fault) = self.unwind_owner_records_above(bind_base, false) {
            return Step::Stop(VmOutcome::Fault(fault));
        }
        self.arena.handlers_mut().truncate(handler_base);
        self.exit_frame_with(frame, value)
    }

    /// Undo every owner binding record above `bind_base`, innermost first,
    /// through the unbind-time re-validation (GNU `unbind_to`,
    /// eval.c:3940-3954: each record undone as it leaves). A serial record
    /// above the base is not the owner's to touch: a caller that finds one
    /// mis-decided its disposition. A record the re-validation refuses needs
    /// the serial side, which a `Return` unwind reaches only through the
    /// disposition that sent it there. Both are the record-contract fault,
    /// the scene kept.
    ///
    /// After the serial cleanup (`serial_cleanup_ran`) the serial side's
    /// `unbind_to` has already undone the frame's owner records - that is
    /// why the disposition sent the Return there - so an entry whose record
    /// is gone from the slab is dropped, not restored again. Before any
    /// escape a missing record is the broken pairing (C2): the fault.
    fn unwind_owner_records_above(
        &mut self,
        bind_base: OwnerRecordMark,
        serial_cleanup_ran: bool,
    ) -> Result<(), EngineFault> {
        while let Some(top) = self.arena.bindings().above(bind_base).last() {
            match top {
                OwnerRecord::Plain(record)
                    if serial_cleanup_ran && !self.plain.record_live(record.record) =>
                {
                    self.arena.bindings_mut().pop();
                }
                OwnerRecord::Plain(record) => {
                    match effective_plain_unbind_checked(&mut self.plain, record) {
                        Ok(PlainUnbind::Done) => {
                            self.arena.bindings_mut().pop();
                        }
                        Ok(PlainUnbind::NeedsSerial { .. })
                        | Err(PlainUnbindError::RecordMismatch) => {
                            tracing::error!(
                                record = ?record,
                                "a Return unwind met an owner record the serial side must undo"
                            );
                            return Err(EngineFault::EscapeRecordContract(RecordStack::Bindings));
                        }
                        Err(PlainUnbindError::Access(error)) => {
                            tracing::error!(
                                record = ?record,
                                error = ?error,
                                "a Return unwind hit a canonical Plain authority failure"
                            );
                            return Err(EngineFault::PlainAccess(error));
                        }
                    }
                }
                OwnerRecord::Serial(token) => {
                    tracing::error!(
                        token = token.0,
                        "a Return unwind met a serial record above the frame's base"
                    );
                    return Err(EngineFault::EscapeRecordContract(RecordStack::Bindings));
                }
            }
        }
        Ok(())
    }

    /// The `Return` escape completed (§5, the `NeedsSerialCleanup` arm): the
    /// serial turn ran the slow arm and its deltas removed the frame's serial
    /// records and handlers; the value the completion carries is the frame's
    /// result (the debugger may have replaced it, bytecode.c:900-901). The
    /// owner's own records above `bind_base` are still ours to undo; then the
    /// frame leaves as in the owner-local arm.
    fn return_from_frame(&mut self, frame: FrameId, produced: Vec<Value>) -> Step {
        let Some(value) = produced.first().copied() else {
            return Step::Stop(VmOutcome::Fault(EngineFault::DispatchContract(Op::Return)));
        };
        let Some((bind_base, handler_base)) = self
            .arena
            .vm(frame)
            .map(|vm| (vm.bind_base, vm.handler_base))
        else {
            return Step::Stop(VmOutcome::Fault(EngineFault::StaleFrame));
        };
        // R1d: an owner truncation never touches a serial handler. A marker
        // the cleanup left above `handler_base` is its contract failure,
        // the fault before anything is truncated (as a leftover record
        // marker faults in the unwind below).
        if self.arena.handlers().has_serial_above(handler_base) {
            tracing::error!(
                frame = ?frame,
                "a Return cleanup left a serial handler marker above the frame's base"
            );
            return Step::Stop(VmOutcome::Fault(EngineFault::EscapeRecordContract(
                RecordStack::Handlers,
            )));
        }
        // Records first, then handlers, as in the owner-local arm.
        if let Err(fault) = self.unwind_owner_records_above(bind_base, true) {
            return Step::Stop(VmOutcome::Fault(fault));
        }
        self.arena.handlers_mut().truncate(handler_base);
        self.exit_frame_with(frame, value)
    }

    /// The frame leaves with `value`: drop its operand segment, pop the frame,
    /// refund its `bc_position`, and dispatch on [`ReturnTo`] (V7 C6, §7):
    /// `Caller { consumed }` delivers `value` into the caller's consumed
    /// designator slot and returns `Step::Frame`; `Owner` publishes the bottom
    /// frame's `Done` root.
    fn exit_frame_with(&mut self, frame: FrameId, value: Value) -> Step {
        let Some((return_to, frame_bytes)) = self
            .arena
            .vm(frame)
            .map(|vm| (vm.return_to, vm.frame_bytes))
        else {
            return Step::Stop(VmOutcome::Fault(EngineFault::StaleFrame));
        };
        if self.arena.vm_truncate_operands(frame, 0).is_none() || self.arena.pop(frame).is_none() {
            return Step::Stop(VmOutcome::Fault(EngineFault::StaleFrame));
        }
        self.arena.refund_bc_position(frame_bytes);
        match return_to {
            ReturnTo::Caller { consumed } => {
                let Some(caller) = self.arena.top_vm_id() else {
                    return Step::Stop(VmOutcome::Fault(EngineFault::NoFrame));
                };
                let Some(caller_len) = self.arena.vm(caller).map(|vm| vm.operands.len()) else {
                    return Step::Stop(VmOutcome::Fault(EngineFault::StaleFrame));
                };
                let Some(keep) = caller_len.checked_sub(consumed as usize) else {
                    return Step::Stop(VmOutcome::Fault(EngineFault::DispatchContract(Op::Return)));
                };
                if self.arena.vm_truncate_operands(caller, keep).is_none() {
                    return Step::Stop(VmOutcome::Fault(EngineFault::StaleFrame));
                }
                if self.arena.vm_push(caller, value).is_none() {
                    return Step::Stop(VmOutcome::Fault(EngineFault::StaleFrame));
                }
                Step::Frame
            }
            ReturnTo::Owner => match self.arena.roots_mut().push(RootGroup::VmFrames, value) {
                Ok(pending) => Step::Stop(VmOutcome::Done { pending }),
                Err(error) => Step::Stop(VmOutcome::Fault(EngineFault::Root(error))),
            },
        }
    }

    /// V8 L3–L7, S1–S3: unwind frames, owner Plain bindings, serial records,
    /// and intermediate handlers in GNU `unwind_to_catch` / `unbind_to` order
    /// (`src/eval.c:1445-1461`, `src/eval.c:3946-3958`, `src/eval.c:3848-3870`),
    /// then land on `target`'s [`LandingPad`] (`src/bytecode.c:983-1003`) with
    /// no poll at landing.
    fn unwind_to_handler(
        &mut self,
        mut target: SerialHandlerToken,
        initial_value: Value,
        origin_op: Op,
        origin_pc: usize,
    ) -> Step {
        let mut value_root = match self
            .arena
            .roots_mut()
            .push(RootGroup::Handlers, initial_value)
        {
            Ok(handle) => handle,
            Err(error) => return Step::Stop(VmOutcome::Fault(EngineFault::Root(error))),
        };

        'restart: loop {
            let Some(target_pad) = self.arena.handlers().find_serial(target) else {
                tracing::error!(
                    target = target.0,
                    "Caught named a serial handler token absent from HandlerStack"
                );
                return Step::Stop(VmOutcome::Fault(EngineFault::EscapeRecordContract(
                    RecordStack::Handlers,
                )));
            };
            if !self.arena.contains_vm_frame(target_pad.frame) {
                tracing::error!(
                    target = target.0,
                    frame = ?target_pad.frame,
                    "Caught target handler's frame is no longer live in FrameArena"
                );
                return Step::Stop(VmOutcome::Fault(EngineFault::EscapeRecordContract(
                    RecordStack::Handlers,
                )));
            }
            let Some(chain) = self.arena.handlers().serial_above_inclusive(target) else {
                return Step::Stop(VmOutcome::Fault(EngineFault::EscapeRecordContract(
                    RecordStack::Handlers,
                )));
            };

            let mut pending_handlers: Vec<SerialHandlerToken> = Vec::new();
            for (h_token, h_pad) in chain {
                loop {
                    // L4 / S1: pop any drained callee frames strictly above
                    // this handler's frame before unwinding further.
                    if let Err(fault) = self.pop_drained_frames_above(h_pad.frame) {
                        return Step::Stop(VmOutcome::Fault(fault));
                    }
                    let Some(top_record) = self.arena.bindings().above(h_pad.bind_mark).last()
                    else {
                        break;
                    };
                    match top_record {
                        OwnerRecord::Plain(record) => {
                            // N-v2-unbind-revalidate (GNU `do_one_unbind` for
                            // `SPECPDL_LET`, `src/eval.c:3848-3870`): re-check
                            // whether the symbol gained a watcher or redirect.
                            match effective_plain_unbind_checked(&mut self.plain, record) {
                                Ok(PlainUnbind::Done) => {
                                    self.arena.bindings_mut().pop();
                                }
                                Ok(PlainUnbind::NeedsSerial { symbol }) => {
                                    if self.plain.top_record() != Some(record.record) {
                                        tracing::error!(
                                            record = ?record,
                                            "L3 unwind Plain binding does not name slab top"
                                        );
                                        return Self::record_contract_fault();
                                    }
                                    // Flush any intermediate handlers that left
                                    // above this outer binding before running
                                    // its watcher (GNU `src/eval.c:1449-1454`).
                                    if !pending_handlers.is_empty() {
                                        let flush = SerialUnwind {
                                            pop_handlers: std::mem::take(&mut pending_handlers),
                                            unbind: Vec::new(),
                                        };
                                        match self.unwind_serial_escape(
                                            flush,
                                            OwnerRecordMark::EMPTY,
                                            HandlerMark::EMPTY,
                                            &origin_op,
                                            origin_pc,
                                        ) {
                                            Ok(UnwindEscapeOutcome::Done) => {}
                                            Ok(UnwindEscapeOutcome::Restart { .. })
                                            | Ok(UnwindEscapeOutcome::Flow(_)) => {
                                                return Step::Stop(VmOutcome::Fault(
                                                    EngineFault::DispatchContract(Op::PopHandler),
                                                ));
                                            }
                                            Err(fault) => {
                                                return Step::Stop(VmOutcome::Fault(fault));
                                            }
                                        }
                                    }
                                    // GNU `unbind_to` (`src/eval.c:3948-3957`):
                                    // decrement before running the unbind so a
                                    // signaling watcher never unbinds it twice.
                                    self.arena.bindings_mut().pop();
                                    match self.unwind_plain_escape(
                                        symbol,
                                        record.record,
                                        &origin_op,
                                        origin_pc,
                                    ) {
                                        Ok(UnwindEscapeOutcome::Done) => {}
                                        Ok(UnwindEscapeOutcome::Restart {
                                            target: new_target,
                                            value: new_value,
                                        }) => {
                                            if let Err(error) =
                                                self.arena.roots_mut().pop(value_root)
                                            {
                                                return Step::Stop(VmOutcome::Fault(
                                                    root_revoke_fault(error),
                                                ));
                                            }
                                            value_root = match self
                                                .arena
                                                .roots_mut()
                                                .push(RootGroup::Handlers, new_value)
                                            {
                                                Ok(handle) => handle,
                                                Err(error) => {
                                                    return Step::Stop(VmOutcome::Fault(
                                                        EngineFault::Root(error),
                                                    ));
                                                }
                                            };
                                            target = new_target;
                                            continue 'restart;
                                        }
                                        Ok(UnwindEscapeOutcome::Flow(flow)) => {
                                            return Step::Stop(VmOutcome::Signaled(flow));
                                        }
                                        Err(fault) => {
                                            return Step::Stop(VmOutcome::Fault(fault));
                                        }
                                    }
                                }
                                Err(PlainUnbindError::RecordMismatch) => {
                                    return Self::record_contract_fault();
                                }
                                Err(PlainUnbindError::Access(error)) => {
                                    return Step::Stop(VmOutcome::Fault(EngineFault::PlainAccess(
                                        error,
                                    )));
                                }
                            }
                        }
                        OwnerRecord::Serial(innermost) => {
                            // HIGH-2 / V8 L4 (GNU `src/eval.c:3946-3958`):
                            // cap the batched serial run at the current top
                            // callee frame's `bind_base` whenever a callee
                            // frame sits above `h_pad.frame`, so the callee
                            // pops before outer cleanups execute and outer
                            // cleanups see the exact live backtrace.
                            let stop_mark = self
                                .arena
                                .top_vm_id()
                                .filter(|top| *top != h_pad.frame)
                                .and_then(|top| self.arena.vm(top).map(|vm| vm.bind_base))
                                .map_or(h_pad.bind_mark, |base| base.max(h_pad.bind_mark));
                            let mut run = vec![innermost];
                            for entry in self.arena.bindings().above(stop_mark).rev().skip(1) {
                                match entry {
                                    OwnerRecord::Serial(next) => run.push(next),
                                    OwnerRecord::Plain(_) => break,
                                }
                            }
                            let unwind = SerialUnwind {
                                pop_handlers: std::mem::take(&mut pending_handlers),
                                unbind: run,
                            };
                            match self.unwind_serial_escape(
                                unwind,
                                OwnerRecordMark::EMPTY,
                                HandlerMark::EMPTY,
                                &origin_op,
                                origin_pc,
                            ) {
                                Ok(UnwindEscapeOutcome::Done) => {}
                                Ok(UnwindEscapeOutcome::Restart {
                                    target: new_target,
                                    value: new_value,
                                }) => {
                                    if let Err(error) = self.arena.roots_mut().pop(value_root) {
                                        return Step::Stop(VmOutcome::Fault(root_revoke_fault(
                                            error,
                                        )));
                                    }
                                    value_root = match self
                                        .arena
                                        .roots_mut()
                                        .push(RootGroup::Handlers, new_value)
                                    {
                                        Ok(handle) => handle,
                                        Err(error) => {
                                            return Step::Stop(VmOutcome::Fault(
                                                EngineFault::Root(error),
                                            ));
                                        }
                                    };
                                    target = new_target;
                                    continue 'restart;
                                }
                                Ok(UnwindEscapeOutcome::Flow(flow)) => {
                                    return Step::Stop(VmOutcome::Signaled(flow));
                                }
                                Err(fault) => return Step::Stop(VmOutcome::Fault(fault)),
                            }
                        }
                    }
                }
                if h_token != target {
                    // GNU `src/eval.c:1451-1452`: intermediate handler leaves
                    // AFTER all records above `h_pad.bind_mark` have unwound.
                    pending_handlers.push(h_token);
                }
            }

            // Drain any remaining callee frames above `target_pad.frame`.
            if let Err(fault) = self.pop_drained_frames_above(target_pad.frame) {
                return Step::Stop(VmOutcome::Fault(fault));
            }
            if self.arena.top_vm_id() != Some(target_pad.frame)
                || self.arena.depth() != target_pad.depth
            {
                tracing::error!(
                    expected_frame = ?target_pad.frame,
                    top_frame = ?self.arena.top_vm_id(),
                    expected_depth = target_pad.depth,
                    got_depth = self.arena.depth(),
                    "unwind reached target handler with mismatched frame or depth"
                );
                return Step::Stop(VmOutcome::Fault(EngineFault::EscapeRecordContract(
                    RecordStack::Handlers,
                )));
            }

            // Final Lisp-free request: pop remaining intermediate handlers +
            // `target` (`unbind: []`). Neither `Flow` nor `Caught` is legal
            // here (§10 rows 10 & 11).
            pending_handlers.push(target);
            let final_unwind = SerialUnwind {
                pop_handlers: pending_handlers,
                unbind: Vec::new(),
            };
            match self.unwind_serial_escape(
                final_unwind,
                OwnerRecordMark::EMPTY,
                HandlerMark::EMPTY,
                &origin_op,
                origin_pc,
            ) {
                Ok(UnwindEscapeOutcome::Done) => {}
                Ok(UnwindEscapeOutcome::Restart { .. }) | Ok(UnwindEscapeOutcome::Flow(_)) => {
                    return Step::Stop(VmOutcome::Fault(EngineFault::DispatchContract(
                        Op::PopHandler,
                    )));
                }
                Err(fault) => return Step::Stop(VmOutcome::Fault(fault)),
            }

            // Land (V8 L6, S2, S3; GNU `src/bytecode.c:983-1003`):
            // restore `operand_len`, push caught `value`, branch to `dest`,
            // with NO poll at landing.
            let value = match self.arena.roots_mut().pop(value_root) {
                Ok(value) => value,
                Err(error) => return Step::Stop(VmOutcome::Fault(root_revoke_fault(error))),
            };
            if self
                .arena
                .vm_truncate_operands(target_pad.frame, target_pad.operand_len)
                .is_none()
            {
                return Step::Stop(VmOutcome::Fault(EngineFault::EscapeRecordContract(
                    RecordStack::Handlers,
                )));
            }
            if self.arena.vm_push(target_pad.frame, value).is_none() {
                return Step::Stop(VmOutcome::Fault(EngineFault::StaleFrame));
            }
            if let Err(fault) = self.set_pc(target_pad.frame, target_pad.dest as usize) {
                return Step::Stop(VmOutcome::Fault(fault));
            }
            return Step::Frame;
        }
    }

    /// Pop any callee `VmFrame` strictly above `stop_frame` whose dynamic
    /// bindings have all been unwound (V8 L4 / S1). Refunds `CallDepth` and
    /// `bc_position`; never fires `debug-on-exit` (GNU `src/bytecode.c:900`).
    fn pop_drained_frames_above(&mut self, stop_frame: FrameId) -> Result<(), EngineFault> {
        while let Some(top) = self.arena.top_vm_id() {
            if top == stop_frame {
                break;
            }
            let Some((bind_base, frame_bytes)) =
                self.arena.vm(top).map(|vm| (vm.bind_base, vm.frame_bytes))
            else {
                return Err(EngineFault::StaleFrame);
            };
            if self.arena.bindings().above(bind_base).next().is_some() {
                break;
            }
            if self.arena.vm_truncate_operands(top, 0).is_none() || self.arena.pop(top).is_none() {
                return Err(EngineFault::StaleFrame);
            }
            self.arena.refund_bc_position(frame_bytes);
        }
        Ok(())
    }

    /// Issue one `EscapeWhy::SerialUnwind` request (shared by `Op::Unbind(n)`
    /// and `unwind_to_handler`, HIGH-1 / V8 L3 / L5 / §10).
    ///
    /// Enforces §10 rows 10 & 11 (`Flow`/`Caught` forbidden when `unbind` is
    /// empty), MEDIUM-1.3 (non-empty prefix on `Caught`, preventing
    /// zero-progress restart loops), structural L2 (no pushed records/handlers
    /// on `Caught`), and exact handler/record token contracts above
    /// `bind_floor`/`handler_floor` before mutating LIFOs.
    fn unwind_serial_escape(
        &mut self,
        unwind: SerialUnwind,
        bind_floor: OwnerRecordMark,
        handler_floor: HandlerMark,
        origin_op: &Op,
        origin_pc: usize,
    ) -> Result<UnwindEscapeOutcome, EngineFault> {
        let runs_lisp = !unwind.unbind.is_empty();
        let top = self.arena.top_vm_id().ok_or(EngineFault::NoFrame)?;
        let operands = self
            .arena
            .vm(top)
            .map(|vm| vm.operands)
            .ok_or(EngineFault::StaleFrame)?;
        self.stop_state = OwnerStopState::Stopped;
        if let Err(error) = self.poll.serial_request(&self.arena) {
            return Err(EngineFault::Poll(error));
        }
        let request = VmEscapeRequest {
            op: origin_op.clone(),
            why: EscapeWhy::SerialUnwind(unwind.clone()),
            pc: Pc::new(origin_pc),
            frame: top,
            operands,
        };
        let completion = self.escape.unwind(request, &self.arena, &mut self.plain);
        let debug_updates = self.escape.take_debug_on_exit_updates();
        match completion {
            UnwindCompletion::Flow(flow) => {
                if !runs_lisp {
                    tracing::error!(
                        op = ?origin_op,
                        "Lisp-free SerialUnwind(unbind=[]) illegally delivered Flow"
                    );
                    return Err(EngineFault::DispatchContract(Op::PopHandler));
                }
                Ok(UnwindEscapeOutcome::Flow(flow))
            }
            UnwindCompletion::Caught {
                handler: new_target,
                value: new_value,
                popped_handlers,
                popped_records,
            } => {
                if !runs_lisp {
                    tracing::error!(
                        op = ?origin_op,
                        "Lisp-free SerialUnwind(unbind=[]) illegally restarted with Caught"
                    );
                    return Err(EngineFault::DispatchContract(Op::PopHandler));
                }
                if popped_handlers != unwind.pop_handlers
                    || !self
                        .arena
                        .handlers()
                        .serial_run_matches_above(handler_floor, &popped_handlers)
                {
                    return Err(EngineFault::EscapeRecordContract(RecordStack::Handlers));
                }
                // MEDIUM-1.3 & GNU `src/eval.c:3955`: a cleanup that ran Lisp
                // and signaled must have popped at least `unwind.unbind[0]`
                // before signaling; an empty prefix would cause a zero-progress
                // restart loop.
                if popped_records.is_empty()
                    || !unwind.unbind.starts_with(&popped_records)
                    || !self
                        .arena
                        .bindings()
                        .serial_run_matches_above(bind_floor, &popped_records)
                {
                    return Err(EngineFault::EscapeRecordContract(RecordStack::Bindings));
                }
                self.arena
                    .handlers_mut()
                    .pop_serial_run_above(handler_floor, &popped_handlers)
                    .map_err(|()| EngineFault::EscapeRecordContract(RecordStack::Handlers))?;
                self.arena
                    .bindings_mut()
                    .pop_serial_run_above(bind_floor, &popped_records)
                    .map_err(|()| EngineFault::EscapeRecordContract(RecordStack::Bindings))?;
                if !debug_updates.is_empty() {
                    let mut stopped = StoppedOwnerBacktrace::from_escape_park(&mut self.arena);
                    for update in debug_updates {
                        let _ = stopped.set_debug_on_exit(update.frame, update.debug_on_exit);
                    }
                }
                if let Some(env) = self.escape.refresh_call_env() {
                    self.set_call_env(env);
                }
                self.stop_state = OwnerStopState::Running;
                Ok(UnwindEscapeOutcome::Restart {
                    target: new_target,
                    value: new_value,
                })
            }
            UnwindCompletion::Completed {
                popped_handlers,
                popped_records,
                pushed_records,
            } => {
                if !runs_lisp && !pushed_records.is_empty() {
                    tracing::error!(
                        op = ?origin_op,
                        "Lisp-free SerialUnwind(unbind=[]) illegally pushed binding records"
                    );
                    return Err(EngineFault::EscapeRecordContract(RecordStack::Bindings));
                }
                if popped_handlers != unwind.pop_handlers
                    || !self
                        .arena
                        .handlers()
                        .serial_run_matches_above(handler_floor, &popped_handlers)
                {
                    return Err(EngineFault::EscapeRecordContract(RecordStack::Handlers));
                }
                if popped_records != unwind.unbind
                    || !self
                        .arena
                        .bindings()
                        .serial_run_matches_above(bind_floor, &popped_records)
                {
                    return Err(EngineFault::EscapeRecordContract(RecordStack::Bindings));
                }
                self.arena
                    .handlers_mut()
                    .pop_serial_run_above(handler_floor, &popped_handlers)
                    .map_err(|()| EngineFault::EscapeRecordContract(RecordStack::Handlers))?;
                self.arena
                    .bindings_mut()
                    .pop_serial_run_above(bind_floor, &popped_records)
                    .map_err(|()| EngineFault::EscapeRecordContract(RecordStack::Bindings))?;
                for token in &pushed_records {
                    self.arena.bindings_mut().push(*token);
                }
                if !debug_updates.is_empty() {
                    let mut stopped = StoppedOwnerBacktrace::from_escape_park(&mut self.arena);
                    for update in debug_updates {
                        let _ = stopped.set_debug_on_exit(update.frame, update.debug_on_exit);
                    }
                }
                if let Some(env) = self.escape.refresh_call_env() {
                    self.set_call_env(env);
                }
                self.stop_state = OwnerStopState::Running;
                Ok(UnwindEscapeOutcome::Done)
            }
        }
    }

    /// Issue one `EscapeWhy::PlainUnbind` hand-off (shared by `Op::Unbind(n)`
    /// and `unwind_to_handler`) for a Plain binding whose symbol gained a
    /// watcher or redirect (V8 §10 row 12; GNU `src/eval.c:3848-3870`).
    ///
    /// MEDIUM-1.5: checks `!self.plain.record_live(record)` on BOTH `Completed`
    /// and `Caught` so a signaling watcher cannot orphan the slab record.
    fn unwind_plain_escape(
        &mut self,
        symbol: SymId,
        record: OwnerRecordId,
        origin_op: &Op,
        origin_pc: usize,
    ) -> Result<UnwindEscapeOutcome, EngineFault> {
        let top = self.arena.top_vm_id().ok_or(EngineFault::NoFrame)?;
        let operands = self
            .arena
            .vm(top)
            .map(|vm| vm.operands)
            .ok_or(EngineFault::StaleFrame)?;
        self.stop_state = OwnerStopState::Stopped;
        if let Err(error) = self.poll.serial_request(&self.arena) {
            return Err(EngineFault::Poll(error));
        }
        let request = VmEscapeRequest {
            op: origin_op.clone(),
            why: EscapeWhy::PlainUnbind { symbol, record },
            pc: Pc::new(origin_pc),
            frame: top,
            operands,
        };
        let completion = self.escape.unwind(request, &self.arena, &mut self.plain);
        let debug_updates = self.escape.take_debug_on_exit_updates();
        match completion {
            UnwindCompletion::Flow(flow) => {
                // V8 r2 LOW-2b: the executor already popped the `Plain` entry
                // before the escape (GNU `src/eval.c:3948-3957`), so
                // `unwind_to_idle` would not see an untaken slab record on
                // `Flow`. Enforce `!record_live` uniformly across `Completed`,
                // `Caught`, and `Flow`.
                if self.plain.record_live(record) {
                    tracing::error!(
                        op = ?origin_op,
                        "PlainUnbind Flow left the owner slab record live"
                    );
                    return Err(EngineFault::EscapeRecordContract(RecordStack::Bindings));
                }
                Ok(UnwindEscapeOutcome::Flow(flow))
            }
            UnwindCompletion::Caught {
                handler: new_target,
                value: new_value,
                popped_handlers,
                popped_records,
            } => {
                if !popped_handlers.is_empty() {
                    return Err(EngineFault::EscapeRecordContract(RecordStack::Handlers));
                }
                if !popped_records.is_empty() {
                    return Err(EngineFault::EscapeRecordContract(RecordStack::Bindings));
                }
                if self.plain.record_live(record) {
                    tracing::error!(
                        op = ?origin_op,
                        "PlainUnbind Caught left the owner slab record live"
                    );
                    return Err(EngineFault::EscapeRecordContract(RecordStack::Bindings));
                }
                if !debug_updates.is_empty() {
                    let mut stopped = StoppedOwnerBacktrace::from_escape_park(&mut self.arena);
                    for update in debug_updates {
                        let _ = stopped.set_debug_on_exit(update.frame, update.debug_on_exit);
                    }
                }
                if let Some(env) = self.escape.refresh_call_env() {
                    self.set_call_env(env);
                }
                self.stop_state = OwnerStopState::Running;
                Ok(UnwindEscapeOutcome::Restart {
                    target: new_target,
                    value: new_value,
                })
            }
            UnwindCompletion::Completed {
                popped_handlers,
                popped_records,
                pushed_records,
            } => {
                if !popped_handlers.is_empty() {
                    return Err(EngineFault::EscapeRecordContract(RecordStack::Handlers));
                }
                if !popped_records.is_empty() {
                    return Err(EngineFault::EscapeRecordContract(RecordStack::Bindings));
                }
                if self.plain.record_live(record) {
                    tracing::error!(
                        op = ?origin_op,
                        "PlainUnbind escape left the owner record it was asked to undo"
                    );
                    return Err(EngineFault::EscapeRecordContract(RecordStack::Bindings));
                }
                for token in &pushed_records {
                    self.arena.bindings_mut().push(*token);
                }
                if !debug_updates.is_empty() {
                    let mut stopped = StoppedOwnerBacktrace::from_escape_park(&mut self.arena);
                    for update in debug_updates {
                        let _ = stopped.set_debug_on_exit(update.frame, update.debug_on_exit);
                    }
                }
                if let Some(env) = self.escape.refresh_call_env() {
                    self.set_call_env(env);
                }
                self.stop_state = OwnerStopState::Running;
                Ok(UnwindEscapeOutcome::Done)
            }
        }
    }
}

enum UnwindEscapeOutcome {
    Done,
    Restart {
        target: SerialHandlerToken,
        value: Value,
    },
    Flow(Flow),
}

fn root_revoke_fault(error: RootRevokeError) -> EngineFault {
    match error {
        RootRevokeError::GenerationExhausted => {
            EngineFault::Root(RootPublishError::GenerationExhausted)
        }
        RootRevokeError::Stale => EngineFault::DispatchContract(Op::PopHandler),
    }
}
