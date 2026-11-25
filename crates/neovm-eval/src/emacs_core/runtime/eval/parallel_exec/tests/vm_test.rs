//! Stage-1 VM stepper rows (VM-EXECUTOR-DESIGN r1 §10). Every row name
//! carries the `vm_` prefix. The sealed-ops double reads conses this harness
//! allocated and nothing shared.

use super::super::owner_bindings::{
    BindingKind, Channel, OwnerBindingRecord, OwnerRecordId, OwnerSlab, SlotState, UndoTarget,
};
use super::backtrace::{
    BacktraceDebugError, BacktraceDebugUpdate, OwnerBacktraceEntry, OwnerBacktraceView,
    OwnerStopState, StoppedOwnerBacktrace,
};
use super::dispatch::{admits_owner_local, opcode_slot};
use super::effective::{
    EraId, OwnerSlots, OwnerSlotsMut, PlainAccess, PlainAdmission, PlainRefusal, ShapeGeneration,
    restore_taken_binding, take_top_binding,
};
use super::engine::{CoreOutcome, EngineFault, RecordStack, apply_one};
use super::frames::{
    BindStack, ExitFlags, FrameArena, FrameId, FrameKind, HandlerKind, HandlerMark, HandlerRecord,
    HandlerStack, InterpFrame, LandingPad, OperandCursor, OwnerHandler, OwnerRecord,
    OwnerRecordMark, OwnerScanJoin, Pc, PollBudget, RootGroup, SerialHandlerToken,
    SerialRecordToken, SlotRecord, VmFrame,
};
use super::poll_tests::{
    OWNER, activate, collect, end_epoch, finish, lend, recv, request_stop, retire, retry,
};
use super::vm::{
    ArithOps, ArithRefusal, CallClass, CallEnv, CallEscape, CodeRefusal, EscapeCompletion,
    EscapeWhy, HandlerDelta, NumericClass, RecordDelta, SerialUnwind, SymbolCallLookup,
    UnwindCompletion, VmCode, VmEntry, VmEscape, VmEscapeRequest, VmExec, VmOutcome, VmPlain,
    VmPoll, VmProgram, VmSealed, classify_bytecode_function, classify_call_with_symbols,
    program_from_bytecode, static_stack_effect,
};
use crate::emacs_core::builtins::{
    builtin_car_1, builtin_car_safe_1, builtin_cdr_1, builtin_cdr_safe_1, builtin_div, builtin_mul,
    builtin_percent, builtin_setcar_2, builtin_setcdr_2,
};
use crate::emacs_core::bytecode::decode::verify_stack_effects;
use crate::emacs_core::bytecode::value_api::TaggedValueBytecodeExt;
use crate::emacs_core::bytecode::vm::{
    BC_STACK_POSITION_LIMIT, bytecode_frame_bytes, charge_frame_position,
};
use crate::emacs_core::bytecode::{
    ArithGenericKind, ByteCodeFunction, ByteCodeFunctionExt, FunctionParams, Op, StackDepth, Vm,
};
use crate::emacs_core::eval::world::poll::{
    PollOutcome, poll_region_refill, poll_serial_request, poll_vm_back_edge, poll_vm_call,
};
use crate::emacs_core::eval::world::serial_gate::stop_for_test;
use crate::emacs_core::eval::{Context, INTERNAL_COMPILER_FUNCTION_OVERRIDES};
use crate::emacs_core::format_eval_result;
use crate::emacs_core::subr::effect_schema::{
    ArithOperands, ParallelArm, PlainVarAccess, ReturnDisposition, SealedAuthority, WorldSplit,
};
use crate::test_utils::unit_runtime;
use neovm_edit::emacs_core::marker::make_marker_value;
use neovm_value::error::{EvalResult, Flow, LispCondition, expect_args, signal};
use neovm_value::forward::TlsIndex;
use neovm_value::intern::{NIL_SYM_ID, SymId, intern};
use neovm_value::tagged::SymbolMode;
use neovm_value::tagged::gc::{
    Mutator, NumericAllocError, RegistrationError, Rendezvous, RendezvousError, TaggedHeap,
};
use neovm_value::tagged::transport::SharedRoot;
use neovm_value::value::{LambdaParams, Value, symbol_id};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::num::NonZeroU32;
use std::rc::Rc;
use std::sync::{Arc, Barrier, mpsc};
use std::thread;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------- doubles

/// The wired VM poll edges. Order inside one stepper is observable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PollEdge {
    VmBackEdge,
    VmCall,
    SerialRequest,
    /// The driver's refill at an instruction boundary (V5).
    VmRefill,
}

struct RecordingPoll {
    edges: Vec<PollEdge>,
    /// The `NumericClass` passed to each `VmRefill` edge, in order (L2).
    refill_classes: Vec<NumericClass>,
    /// The published top segment at each poll, in order.
    segments: Vec<Vec<Value>>,
    /// Every live frame's published `(function, code, pc, operands)` snapshot
    /// innermost-first at each poll (V7 §11 R1).
    published_frames: Vec<Vec<(Value, Value, Option<Pc>, Vec<Value>)>>,
    /// A one-shot refusal at one edge.
    refusal: Option<(PollEdge, RendezvousError)>,
    /// The stopped grant's stand-in (V5): the arith double's reserve, and
    /// the amount each refill edge mints into it, in order; past the list
    /// every edge mints one.
    grant: Option<(Rc<Cell<usize>>, Vec<usize>)>,
}

impl RecordingPoll {
    fn new() -> Self {
        Self {
            edges: Vec::new(),
            refill_classes: Vec::new(),
            segments: Vec::new(),
            published_frames: Vec::new(),
            refusal: None,
            grant: None,
        }
    }

    /// The refill edges mint `amounts` into `budget`, in order.
    fn refilling(mut self, budget: Rc<Cell<usize>>, amounts: Vec<usize>) -> Self {
        self.grant = Some((budget, amounts));
        self
    }

    fn refusing_on(edge: PollEdge, error: RendezvousError) -> Self {
        Self {
            refusal: Some((edge, error)),
            ..Self::new()
        }
    }

    fn poll(&mut self, edge: PollEdge, arena: &FrameArena) -> Result<PollOutcome, RendezvousError> {
        self.edges.push(edge);
        self.segments
            .push(arena.top_vm_segment().unwrap_or(&[]).to_vec());
        let mut frames = Vec::new();
        arena.for_each_frame_innermost_first(|_id, frame, operands| match frame {
            super::frames::OwnedFrame::Vm(vm) => {
                frames.push((vm.function, vm.code, Some(vm.pc), operands.to_vec()));
            }
            super::frames::OwnedFrame::Interp(interp) => {
                frames.push((interp.function, interp.body, None, Vec::new()));
            }
        });
        self.published_frames.push(frames);
        if self.refusal.is_some_and(|(expected, _)| expected == edge) {
            let (_, error) = self.refusal.take().expect("checked just above");
            return Err(error);
        }
        if edge == PollEdge::VmRefill
            && let Some((budget, amounts)) = &self.grant
        {
            let index = self
                .edges
                .iter()
                .filter(|edge| **edge == PollEdge::VmRefill)
                .count()
                - 1;
            let amount = amounts.get(index).copied().unwrap_or(1);
            budget.set(budget.get() + amount);
        }
        Ok(PollOutcome::Continue)
    }
}

impl VmPoll for RecordingPoll {
    fn vm_back_edge(&mut self, arena: &FrameArena) -> Result<PollOutcome, RendezvousError> {
        self.poll(PollEdge::VmBackEdge, arena)
    }

    fn vm_call(&mut self, arena: &FrameArena) -> Result<PollOutcome, RendezvousError> {
        self.poll(PollEdge::VmCall, arena)
    }

    fn serial_request(&mut self, arena: &FrameArena) -> Result<PollOutcome, RendezvousError> {
        self.poll(PollEdge::SerialRequest, arena)
    }

    fn vm_refill(
        &mut self,
        arena: &FrameArena,
        class: NumericClass,
    ) -> Result<PollOutcome, RendezvousError> {
        self.refill_classes.push(class);
        self.poll(PollEdge::VmRefill, arena)
    }
}

/// The serial tokens of one LIFO, outermost first; an owner record is `None`.
type Lifo = Vec<Option<u64>>;

fn serial_records(records: impl Iterator<Item = OwnerRecord>) -> Lifo {
    records
        .map(|record| match record {
            OwnerRecord::Serial(token) => Some(token.0),
            OwnerRecord::Plain(_) => None,
        })
        .collect()
}

fn serial_handlers(records: impl Iterator<Item = OwnerHandler>) -> Lifo {
    records
        .map(|record| match record {
            OwnerHandler::Serial { token, .. } => Some(token.0),
            OwnerHandler::Owner(_) => None,
        })
        .collect()
}

fn lifos(arena: &FrameArena) -> (Lifo, Lifo) {
    (
        serial_records(arena.bindings().above(OwnerRecordMark::EMPTY)),
        serial_handlers(arena.handlers().above(HandlerMark::EMPTY)),
    )
}

/// An ordered effect log the doubles of one harness share: the Plain port's
/// writes and the escape handler's requests land in one order.
#[derive(Clone, Default)]
struct EffectLog(Rc<RefCell<Vec<String>>>);

impl EffectLog {
    fn push(&self, entry: impl Into<String>) {
        self.0.borrow_mut().push(entry.into());
    }

    fn entries(&self) -> Vec<String> {
        self.0.borrow().clone()
    }
}

enum ScriptStep {
    Escape(EscapeCompletion),
    Unwind(UnwindCompletion),
}

struct RecordingEscape {
    requests: Vec<VmEscapeRequest>,
    /// The escaping frame's segment at each escape, before anything applied.
    segments: Vec<Vec<Value>>,
    /// Both record LIFOs at each escape, before anything applied.
    lifos: Vec<(Lifo, Lifo)>,
    /// Published `OwnerBacktraceView` frame depth at each escape/unwind.
    backtrace_depths: Vec<usize>,
    /// Values published under `RootGroup::Handlers` at each escape/unwind.
    handler_roots: Vec<Vec<Value>>,
    /// One completion per escape/unwind, in order; `None` is fall-through.
    script: Vec<Option<ScriptStep>>,
    /// Optional per-escape `(backtrace_level, debug_on_exit)` updates queued
    /// while stopped inside an escape.
    debug_script: Vec<Vec<(usize, bool)>>,
    pending_debug_updates: Vec<BacktraceDebugUpdate>,
    /// An unscripted `Return` reports the operand it published, the frame's
    /// value (`Breturn`: `val = TOP`, bytecode.c:897-921); the default
    /// completion reports nil.
    mirror_return: bool,
    /// Whether the serial side undoes the binding a `PlainUnbind` hand-off
    /// names (the default): the double takes the record through the plain
    /// access and runs the bare restore. `leaving_plain_binding` turns it
    /// off, for the row where the serial side returns without undoing.
    handoff_undoes: bool,
    /// Whether the serial side, on its first non-hand-off escape, pushes a
    /// record onto the owner's slab that the executor's LIFO never hears of
    /// (an `unbind` exchange gone wrong): the O3 row's scene.
    pushes_foreign_record: bool,
    /// Whether the serial side, on its first non-hand-off escape, takes the
    /// slab's top record off the executor's feet (an `unbind` exchange gone
    /// wrong): the "record is gone" scene of the not-the-top row.
    takes_top_record_on_escape: bool,
    /// Where each request is logged, when a row shares the log.
    log: Option<EffectLog>,
    /// Optional `CallEnv` snapshots returned by `refresh_call_env` (V7 §4):
    /// consumed in order at run start and after each checked escape completion.
    env_refreshes: Vec<Option<CallEnv>>,
}

impl RecordingEscape {
    fn new() -> Self {
        Self {
            requests: Vec::new(),
            segments: Vec::new(),
            lifos: Vec::new(),
            backtrace_depths: Vec::new(),
            handler_roots: Vec::new(),
            script: Vec::new(),
            debug_script: Vec::new(),
            pending_debug_updates: Vec::new(),
            mirror_return: false,
            handoff_undoes: true,
            pushes_foreign_record: false,
            takes_top_record_on_escape: false,
            log: None,
            env_refreshes: Vec::new(),
        }
    }

    fn with_env_refreshes(mut self, env_refreshes: Vec<Option<CallEnv>>) -> Self {
        self.env_refreshes = env_refreshes;
        self
    }

    fn with_debug_script(mut self, debug_script: Vec<Vec<(usize, bool)>>) -> Self {
        self.debug_script = debug_script;
        self
    }

    fn with_script(script: Vec<Option<EscapeCompletion>>) -> Self {
        Self {
            script: script
                .into_iter()
                .map(|item| item.map(ScriptStep::Escape))
                .collect(),
            ..Self::new()
        }
    }

    fn with_steps(script: Vec<Option<ScriptStep>>) -> Self {
        Self {
            script,
            ..Self::new()
        }
    }

    /// The serial side returns from a `PlainUnbind` hand-off without undoing
    /// the binding: the record stays live on the slab.
    fn leaving_plain_binding(mut self) -> Self {
        self.handoff_undoes = false;
        self
    }

    /// The serial side pushes a record the executor never saw, once, on its
    /// first non-hand-off escape.
    fn pushing_foreign_record(mut self) -> Self {
        self.pushes_foreign_record = true;
        self
    }

    /// The serial side takes the slab's top record, once, on its first
    /// non-hand-off escape, without restoring anything.
    fn taking_the_record(mut self) -> Self {
        self.takes_top_record_on_escape = true;
        self
    }

    /// The serial side's `Return`: the frame's value is its top operand.
    fn mirroring_return() -> Self {
        Self {
            mirror_return: true,
            ..Self::new()
        }
    }

    fn mirroring_with_script(script: Vec<Option<EscapeCompletion>>) -> Self {
        Self {
            script: script
                .into_iter()
                .map(|item| item.map(ScriptStep::Escape))
                .collect(),
            ..Self::mirroring_return()
        }
    }

    fn logging(log: EffectLog) -> Self {
        Self {
            log: Some(log),
            ..Self::new()
        }
    }

    fn logging_with_script(script: Vec<Option<EscapeCompletion>>, log: EffectLog) -> Self {
        Self {
            script: script
                .into_iter()
                .map(|item| item.map(ScriptStep::Escape))
                .collect(),
            ..Self::logging(log)
        }
    }

    fn logging_with_steps(script: Vec<Option<ScriptStep>>, log: EffectLog) -> Self {
        Self {
            script,
            ..Self::logging(log)
        }
    }

    /// Logs each request and mirrors the serial `Return`.
    fn mirroring_logging_with_script(
        script: Vec<Option<EscapeCompletion>>,
        log: EffectLog,
    ) -> Self {
        Self {
            mirror_return: true,
            ..Self::logging_with_script(script, log)
        }
    }

    fn record_observation(&mut self, request: &VmEscapeRequest, arena: &FrameArena) -> usize {
        let segment = arena.vm_segment(request.frame).unwrap_or(&[]).to_vec();
        if let Some(log) = &self.log {
            let why = match &request.why {
                EscapeWhy::SerialUnwind(unwind) => {
                    if unwind.pop_handlers.is_empty() {
                        format!(
                            " [{}]",
                            unwind
                                .unbind
                                .iter()
                                .map(|token| token.0.to_string())
                                .collect::<Vec<_>>()
                                .join(" ")
                        )
                    } else {
                        format!(
                            " [h:{} r:{}]",
                            unwind
                                .pop_handlers
                                .iter()
                                .map(|token| token.0.to_string())
                                .collect::<Vec<_>>()
                                .join(" "),
                            unwind
                                .unbind
                                .iter()
                                .map(|token| token.0.to_string())
                                .collect::<Vec<_>>()
                                .join(" "),
                        )
                    }
                }
                EscapeWhy::PlainUnbind { .. } => " [plain]".to_string(),
                _ => String::new(),
            };
            log.push(format!("escape {:?}{why}", request.op));
        }
        let view = OwnerBacktraceView::from_arena(arena);
        let index = self.requests.len();
        if let Some(actions) = self.debug_script.get(index) {
            for &(level, enabled) in actions {
                if let Some(entry) = view.frame_at(level) {
                    self.pending_debug_updates.push(BacktraceDebugUpdate {
                        frame: entry.frame_id,
                        debug_on_exit: enabled,
                    });
                }
            }
        }
        let mut h_roots = Vec::new();
        arena.trace_roots(&mut |group, value| {
            if group == RootGroup::Handlers {
                h_roots.push(value);
            }
        });
        self.backtrace_depths.push(view.len());
        self.handler_roots.push(h_roots);
        self.segments.push(segment);
        self.lifos.push(lifos(arena));
        self.requests.push(request.clone());
        index
    }
}

impl VmEscape for RecordingEscape {
    fn escape(
        &mut self,
        request: VmEscapeRequest,
        arena: &FrameArena,
        plain: &mut dyn PlainAccess,
    ) -> EscapeCompletion {
        let top = arena
            .vm_segment(request.frame)
            .and_then(|s| s.last().copied());
        let index = self.record_observation(&request, arena);
        let mut taken = Vec::new();
        if let EscapeWhy::World(WorldSplit::Return(ReturnDisposition::NeedsSerialCleanup)) =
            &request.why
        {
            if self.handoff_undoes {
                let frame_plain_count = arena
                    .vm(request.frame)
                    .map(|vm| {
                        arena
                            .bindings()
                            .above(vm.bind_base)
                            .filter(|entry| matches!(entry, OwnerRecord::Plain(_)))
                            .count()
                    })
                    .unwrap_or(0);
                for _ in 0..frame_plain_count {
                    let Some(top) = plain.top_record() else {
                        break;
                    };
                    match plain.pop_binding(top) {
                        Some(record) => taken.push(record),
                        None => break,
                    }
                }
            }
        } else if self.pushes_foreign_record {
            self.pushes_foreign_record = false;
            plain.push_binding(
                plain_sym("pvm-foreign"),
                TlsIndex::VOID,
                Channel::Default,
                UndoTarget::Retired,
            );
        } else if self.takes_top_record_on_escape {
            self.takes_top_record_on_escape = false;
            if let Some(top) = plain.top_record() {
                plain.pop_binding(top);
            }
        }
        let completion = match self.script.get_mut(index).and_then(Option::take) {
            Some(ScriptStep::Escape(comp)) => comp,
            Some(ScriptStep::Unwind(_)) => {
                panic!("script[{index}] had UnwindCompletion for VmEscape::escape")
            }
            None => {
                if self.mirror_return
                    && request.op == Op::Return
                    && let Some(top) = top
                {
                    completion(&Op::Return, vec![top])
                } else {
                    EscapeCompletion::fall_through(&request.op)
                }
            }
        };
        if matches!(completion, EscapeCompletion::Completed { .. }) {
            for record in taken {
                restore_taken_binding(plain, record);
            }
        }
        completion
    }

    fn unwind(
        &mut self,
        request: VmEscapeRequest,
        arena: &FrameArena,
        plain: &mut dyn PlainAccess,
    ) -> UnwindCompletion {
        let index = self.record_observation(&request, arena);
        let mut taken = Vec::new();
        if let EscapeWhy::PlainUnbind { record, .. } = &request.why
            && self.handoff_undoes
        {
            taken.extend(plain.pop_binding(*record));
        }
        let completion = match self.script.get_mut(index).and_then(Option::take) {
            Some(ScriptStep::Unwind(comp)) => comp,
            Some(ScriptStep::Escape(comp)) => match comp {
                EscapeCompletion::Completed {
                    records, handlers, ..
                } => UnwindCompletion::Completed {
                    popped_handlers: handlers.popped,
                    popped_records: records.popped,
                    pushed_records: records.pushed,
                },
                EscapeCompletion::Caught { handler, value } => UnwindCompletion::Caught {
                    handler,
                    value,
                    popped_handlers: Vec::new(),
                    popped_records: Vec::new(),
                },
                EscapeCompletion::CleanupCaught {
                    handler,
                    value,
                    popped_records,
                    popped_handlers,
                } => UnwindCompletion::Caught {
                    handler,
                    value,
                    popped_handlers,
                    popped_records,
                },
                EscapeCompletion::Flow(flow) => UnwindCompletion::Flow(flow),
            },
            None => UnwindCompletion::Completed {
                popped_handlers: Vec::new(),
                popped_records: Vec::new(),
                pushed_records: Vec::new(),
            },
        };
        if matches!(completion, UnwindCompletion::Completed { .. }) {
            for record in taken {
                restore_taken_binding(plain, record);
            }
        }
        completion
    }

    fn take_debug_on_exit_updates(&mut self) -> Vec<BacktraceDebugUpdate> {
        std::mem::take(&mut self.pending_debug_updates)
    }

    fn refresh_call_env(&mut self) -> Option<CallEnv> {
        if self.env_refreshes.is_empty() {
            None
        } else {
            self.env_refreshes.remove(0)
        }
    }
}

fn apply_two(
    ctx: &mut Context,
    name: &'static str,
    args: &[Value],
    call: fn(&mut Context, Value, Value) -> EvalResult,
) -> CoreOutcome {
    match expect_args(name, args, 2) {
        Ok(()) => match call(ctx, args[0], args[1]) {
            Ok(value) => CoreOutcome::Value(value),
            Err(flow) => CoreOutcome::Signal(flow),
        },
        Err(flow) => CoreOutcome::Signal(flow),
    }
}

/// The sealed cons port over the serial core, the way stage 1's `scalar_core`
/// calls the builtins. Any other name is a missing core.
struct HarnessVmSealed;

impl VmSealed for HarnessVmSealed {
    fn read_cons_field(
        &mut self,
        ctx: &mut Context,
        name: &'static str,
        args: &[Value],
    ) -> CoreOutcome {
        match name {
            "car" => apply_one(ctx, name, args, builtin_car_1),
            "cdr" => apply_one(ctx, name, args, builtin_cdr_1),
            "car-safe" => apply_one(ctx, name, args, builtin_car_safe_1),
            "cdr-safe" => apply_one(ctx, name, args, builtin_cdr_safe_1),
            _ => CoreOutcome::MissingCore,
        }
    }

    fn write_cons_field(
        &mut self,
        ctx: &mut Context,
        name: &'static str,
        args: &[Value],
    ) -> CoreOutcome {
        match name {
            "setcar" => apply_two(ctx, name, args, builtin_setcar_2),
            "setcdr" => apply_two(ctx, name, args, builtin_setcdr_2),
            _ => CoreOutcome::MissingCore,
        }
    }

    fn allocate_cons(
        &mut self,
        _ctx: &mut Context,
        name: &'static str,
        args: &[Value],
    ) -> CoreOutcome {
        match name {
            "cons" => match expect_args(name, args, 2) {
                Ok(()) => CoreOutcome::Value(Value::cons(args[0], args[1])),
                Err(flow) => CoreOutcome::Signal(flow),
            },
            "list" => CoreOutcome::Value(Value::list_from_slice(args)),
            _ => CoreOutcome::MissingCore,
        }
    }
}

/// A sealed port with no serial core behind it.
struct MissingSealed;

impl VmSealed for MissingSealed {
    fn read_cons_field(
        &mut self,
        _ctx: &mut Context,
        _name: &'static str,
        _args: &[Value],
    ) -> CoreOutcome {
        CoreOutcome::MissingCore
    }

    fn write_cons_field(
        &mut self,
        _ctx: &mut Context,
        _name: &'static str,
        _args: &[Value],
    ) -> CoreOutcome {
        CoreOutcome::MissingCore
    }

    fn allocate_cons(
        &mut self,
        _ctx: &mut Context,
        _name: &'static str,
        _args: &[Value],
    ) -> CoreOutcome {
        CoreOutcome::MissingCore
    }
}

// ------------------------------------------------- the arithmetic double

/// The owner-local arithmetic port's double (R6). Where the serial core has
/// a Context-free builtin the double IS the serial answer (`builtin_mul`,
/// `builtin_div`, `Vm::arith_integer_fast`); the rest is fixnum or IEEE
/// arithmetic in the order GNU computes it.
/// `vm_arithmetic_results_tie_the_serial_builtin` ties it over its domain;
/// outside that domain it fails closed on a signal of its own instead of
/// guessing. The production port allocates through the owner's regions;
/// this one allocates on the tagged heap and counts every float it
/// allocates as one owner float slot.
struct HarnessArith {
    /// Every call in order: the kind and the operands as published.
    calls: Vec<(ArithGenericKind, Vec<Value>)>,
    /// The floats allocated for results, in order.
    float_slots: Vec<f64>,
    /// The owner's float reserve (V5): the results this port may still
    /// allocate. Shared with the poll double, which stands for the stopped
    /// grant and refills it. `usize::MAX` is unbounded.
    budget: Rc<Cell<usize>>,
}

impl HarnessArith {
    fn new() -> Self {
        Self {
            calls: Vec::new(),
            float_slots: Vec::new(),
            budget: Rc::new(Cell::new(usize::MAX)),
        }
    }

    /// A port over a reserve of `budget` float results.
    fn with_budget(budget: &Rc<Cell<usize>>) -> Self {
        Self {
            budget: budget.clone(),
            ..Self::new()
        }
    }

    /// One float allocation: refused when the reserve is dry, and only
    /// then - the caller has already decided every operand.
    fn take_float(&mut self) -> Result<(), ArithRefusal> {
        let left = self.budget.get();
        if left == 0 {
            return Err(ArithRefusal::Exhausted(NumericClass::Float));
        }
        self.budget.set(left - 1);
        Ok(())
    }

    /// A float result: one allocation through the owner's float slot.
    fn float(&mut self, value: f64) -> Result<Value, ArithRefusal> {
        self.take_float()?;
        self.float_slots.push(value);
        Ok(Value::make_float(value))
    }
}

/// Not in the double's domain: a signal of its own, never a guess.
fn unsupported() -> ArithRefusal {
    ArithRefusal::Flow(signal(
        "error",
        vec![Value::string(
            "harness arithmetic double: unsupported shape",
        )],
    ))
}

/// The number of a fixnum or float operand.
fn number(value: &Value) -> Result<f64, ArithRefusal> {
    value.as_number_f64().ok_or_else(unsupported)
}

/// GNU `arithcompare` (data.c:2718-2846) over fixnums and floats, as
/// `(lt, eq, gt)`: the doubles, and where a float meets a fixnum and they
/// tie, the exact integers (the tie-break GNU spells out for both orders).
/// A NaN is unordered under all three.
fn compare(a: Value, b: Value) -> Result<(bool, bool, bool), ArithRefusal> {
    if let (Some(x), Some(y)) = (a.as_fixnum(), b.as_fixnum()) {
        return Ok((x < y, x == y, x > y));
    }
    let (f1, f2) = (number(&a)?, number(&b)?);
    if f1 != f2 {
        return Ok((f1 < f2, false, f1 > f2));
    }
    match (a.as_fixnum(), b.as_fixnum()) {
        (Some(x), None) => {
            let i2 = f1 as i64;
            Ok((x < i2, x == i2, x > i2))
        }
        (None, Some(y)) => {
            let i1 = f2 as i64;
            Ok((i1 < y, i1 == y, i1 > y))
        }
        // Two floats.
        _ => Ok((false, true, false)),
    }
}

impl ArithOps for HarnessArith {
    fn arith(&mut self, kind: ArithGenericKind, operands: &[Value]) -> Result<Value, ArithRefusal> {
        self.calls.push((kind, operands.to_vec()));
        // GNU checks each operand where it stands: a marker is coerced
        // (`check_number_coerce_marker`, data.c:2697-2703; `%` takes
        // integers only, `check_integer_coerce_marker`, :2688-2694) and a
        // wrong type signals there. The first such operand decides.
        let integer_only = kind == ArithGenericKind::Rem;
        let predicate = if integer_only {
            "integer-or-marker-p"
        } else {
            "number-or-marker-p"
        };
        for value in operands {
            if value.is_marker() {
                return Err(ArithRefusal::MarkerInvolved);
            }
            if !(value.is_fixnum() || value.is_bignum() || (value.is_float() && !integer_only)) {
                return Err(ArithRefusal::Flow(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol(predicate), *value],
                )));
            }
        }
        let integers = operands
            .iter()
            .all(|value| value.is_fixnum() || value.is_bignum());
        match (kind, operands) {
            // The serial Context-free builtins, integers and floats alike.
            // Check the float reserve BEFORE invoking the builtin when any
            // operand is a float (N1 / #12095 C2: no heap temporary before
            // Exhausted; IEEE float mul/div never signals arith-error).
            (ArithGenericKind::Mul, _) => {
                let will_float = operands.iter().any(|value| value.is_float());
                if will_float {
                    self.take_float()?;
                }
                let value = builtin_mul(operands).map_err(ArithRefusal::Flow)?;
                if value.is_float() {
                    self.float_slots.push(value.xfloat());
                }
                Ok(value)
            }
            (ArithGenericKind::Div, _) => {
                let will_float = operands.iter().any(|value| value.is_float());
                if will_float {
                    self.take_float()?;
                }
                let value = builtin_div(operands).map_err(ArithRefusal::Flow)?;
                if value.is_float() {
                    self.float_slots.push(value.xfloat());
                }
                Ok(value)
            }
            (ArithGenericKind::Rem, [a, b]) => {
                let (Some(a), Some(b)) = (a.as_fixnum(), b.as_fixnum()) else {
                    return Err(unsupported());
                };
                if b == 0 {
                    return Err(ArithRefusal::Flow(signal(
                        LispCondition::ArithError,
                        vec![],
                    )));
                }
                // `checked_rem` also refuses `i64::MIN % -1`, which C leaves
                // undefined: a bignum here, so already refused above.
                a.checked_rem(b).map(Value::fixnum).ok_or_else(unsupported)
            }
            // All-integer operands on the kinds with a direct integer answer.
            _ if integers && kind.integer_op().is_some() => {
                Vm::arith_integer_fast(kind, operands).ok_or_else(unsupported)
            }
            // Fixnums and floats from here: GNU's float arms.
            (ArithGenericKind::Add, [a, b]) => Ok(self.float(number(a)? + number(b)?)?),
            (ArithGenericKind::Sub, [a, b]) => Ok(self.float(number(a)? - number(b)?)?),
            (ArithGenericKind::Add1, [a]) => Ok(self.float(1.0 + number(a)?)?),
            (ArithGenericKind::Sub1, [a]) => Ok(self.float(-1.0 + number(a)?)?),
            // GNU `Fminus` on one integer negates in the integers: a bignum
            // at `most-negative-fixnum`, where `make_int` promotes.
            (ArithGenericKind::Negate, [a]) => {
                if let Some(x) = a.as_fixnum() {
                    return x.checked_neg().map(Value::make_int).ok_or_else(unsupported);
                }
                Ok(self.float(-number(a)?)?)
            }
            // GNU `minmax_driver` (data.c:3462-3474): `accum = args[0]`, and
            // the next operand `val` replaces it only when
            // `arithcompare (val, accum) & cmpmask` holds: `val > accum` for
            // `max`, `val < accum` for `min`. A NaN `val` never compares, and
            // returns then.
            (ArithGenericKind::Max, [a, b]) | (ArithGenericKind::Min, [a, b]) => {
                let (lt, _, gt) = compare(*a, *b)?;
                let wins = if kind == ArithGenericKind::Max {
                    lt
                } else {
                    gt
                };
                if wins || (b.is_float() && b.xfloat().is_nan()) {
                    Ok(*b)
                } else {
                    Ok(*a)
                }
            }
            (ArithGenericKind::NumEq, [a, b])
            | (ArithGenericKind::Lt, [a, b])
            | (ArithGenericKind::Gt, [a, b])
            | (ArithGenericKind::Le, [a, b])
            | (ArithGenericKind::Ge, [a, b]) => {
                let (lt, eq, gt) = compare(*a, *b)?;
                let holds = match kind {
                    ArithGenericKind::NumEq => eq,
                    ArithGenericKind::Lt => lt,
                    ArithGenericKind::Gt => gt,
                    ArithGenericKind::Le => lt || eq,
                    _ => gt || eq,
                };
                Ok(Value::bool_val(holds))
            }
            _ => Err(unsupported()),
        }
    }
}

// -------------------------------------------------- the Plain variable double

/// The owner-local Plain port's double (V2). The slot storage and the
/// binding-record LIFO are a real `OwnerSlab` (C2, one record two views: a
/// bind pushes the slab record, the executor's entry names it, and an unbind
/// takes it from the top). One owner slot: the test seam mints only
/// `TlsIndex::VOID`, so one admitted symbol owns the harness slot and a row
/// admits at most one symbol at a time. The admission is per symbol; the
/// canonical store holds a symbol's value, absent when the variable is void.
/// `era`/`shape` are the generation a row moves to make a cached key stale.
/// Every access is counted, so a row can prove a stale or refused key
/// touched nothing; every write lands in the shared log.
struct HarnessPlain {
    slab: OwnerSlab,
    admissions: HashMap<SymId, PlainAdmission>,
    canonical: HashMap<SymId, Value>,
    era: EraId,
    shape: ShapeGeneration,
    /// The reads, which go through `&self`.
    slot_reads: Cell<usize>,
    canonical_reads: Cell<usize>,
    admissions_asked: Cell<usize>,
    slot_writes: usize,
    canonical_writes: usize,
    /// The symbols whose binding the serial side must undo
    /// (N-v2-unbind-revalidate): a watcher was added while the binding was
    /// live, the projection was retired, or the channel died. Read at unbind
    /// time through `unbind_needs_serial`.
    watched: HashSet<SymId>,
    /// The symbols made local or aliased while the binding was live (C4's
    /// redirect half): the serial unbind's `set_default_internal` path.
    redirected: HashSet<SymId>,
    log: Option<EffectLog>,
}

impl HarnessPlain {
    fn new() -> Self {
        Self {
            slab: OwnerSlab::new(0),
            admissions: HashMap::new(),
            canonical: HashMap::new(),
            era: EraId::new(3),
            shape: ShapeGeneration::new(7),
            slot_reads: Cell::new(0),
            canonical_reads: Cell::new(0),
            admissions_asked: Cell::new(0),
            slot_writes: 0,
            canonical_writes: 0,
            watched: HashSet::new(),
            redirected: HashSet::new(),
            log: None,
        }
    }

    /// A watcher is added for `sym` while its binding is live: the serial
    /// side's `add-variable-watch`, run from an escape inside the `let`.
    fn watched(mut self, sym: SymId) -> Self {
        self.watched.insert(sym);
        self
    }

    /// The variable was made local, or aliased, while its binding was live.
    fn redirected(mut self, sym: SymId) -> Self {
        self.redirected.insert(sym);
        self
    }

    /// Admit SYM with the harness slot.
    fn admit(mut self, sym: SymId) -> Self {
        self.admissions
            .insert(sym, PlainAdmission::mint(sym, TlsIndex::VOID));
        self
    }

    /// Refuse SYM for REASON.
    fn refuse(mut self, sym: SymId, reason: PlainRefusal) -> Self {
        self.admissions.insert(sym, PlainAdmission::refuse(reason));
        self
    }

    /// Refuse SYM from now on, on a harness already in use.
    fn refuse_later(&mut self, sym: SymId, reason: PlainRefusal) {
        self.admissions.insert(sym, PlainAdmission::refuse(reason));
    }

    /// What a read of SYM observes now: the owner slot's value, or the
    /// canonical value when the slot holds no binding.
    fn observed(&self, sym: SymId) -> String {
        match self.slab.slot(TlsIndex::VOID, Channel::Default) {
            SlotState::Bound(value) => plain_value(&value.get()),
            SlotState::NoBinding => {
                plain_value(self.canonical.get(&sym).unwrap_or(&Value::UNBOUND))
            }
        }
    }

    /// Give SYM the canonical value VALUE.
    fn with_canonical(mut self, sym: SymId, value: Value) -> Self {
        self.canonical.insert(sym, value);
        self
    }

    fn logging(mut self, log: EffectLog) -> Self {
        self.log = Some(log);
        self
    }

    /// A structural change the serial side made: the era moves.
    fn next_era(&mut self) {
        self.era = EraId::new(self.era.get() + 1);
    }

    /// (slot reads, slot writes, canonical reads, canonical writes,
    /// admissions) so far.
    fn accesses(&self) -> (usize, usize, usize, usize, usize) {
        (
            self.slot_reads.get(),
            self.slot_writes,
            self.canonical_reads.get(),
            self.canonical_writes,
            self.admissions_asked.get(),
        )
    }

    /// The slab record an executor entry names; it must name a live one.
    fn stored(&self, record: SlotRecord) -> OwnerBindingRecord {
        *self
            .slab
            .record(record.record)
            .expect("the entry names a live slab record")
    }

    /// The slab's top record (the slab's own O(1) read, as the trait's).
    fn top_record(&self) -> Option<OwnerRecordId> {
        self.slab.top_record_id()
    }

    /// The slab's live records, outermost first, as (symbol, kind, undo).
    /// `UndoTarget` has no `PartialEq`: a row matches the undo by hand.
    fn live_records(&self) -> Vec<(SymId, BindingKind, UndoTarget)> {
        self.slab
            .live_records()
            .map(|(_, record)| (record.sym, record.kind, record.undo))
            .collect()
    }

    /// The serial side takes the record ID names off the slab, under the
    /// executor's feet: an `unbind` exchange gone wrong (owner_bindings.rs
    /// has the exchange paths around :1480).
    fn take_record_for_test(&mut self, id: OwnerRecordId) {
        self.slab
            .take_record(id)
            .expect("the record was live before the take");
    }

    /// The serial side pushes a record the executor's LIFO never heard of.
    fn push_foreign_record(&mut self, sym: SymId) {
        self.slab.push_record(
            sym,
            TlsIndex::VOID,
            BindingKind::Let,
            Channel::Default,
            UndoTarget::Retired,
        );
    }
}

impl PlainAccess for HarnessPlain {
    fn admission(&self, sym: SymId) -> PlainAdmission {
        self.admissions_asked.set(self.admissions_asked.get() + 1);
        self.admissions
            .get(&sym)
            .copied()
            .unwrap_or_else(|| PlainAdmission::refuse(PlainRefusal::NotPlain))
    }

    fn canonical_read(&self, sym: SymId) -> Value {
        self.canonical_reads.set(self.canonical_reads.get() + 1);
        self.canonical.get(&sym).copied().unwrap_or(Value::UNBOUND)
    }

    fn canonical_write(&mut self, sym: SymId, value: Value) {
        self.canonical_writes += 1;
        if let Some(log) = &self.log {
            log.push(format!("canonical <- {}", plain_value(&value)));
        }
        self.canonical.insert(sym, value);
    }

    fn canonical_bound(&self, sym: SymId) -> bool {
        self.canonical.contains_key(&sym)
    }

    fn unbind_needs_serial(&self, sym: SymId) -> bool {
        self.watched.contains(&sym) || self.redirected.contains(&sym)
    }

    fn record_live(&self, id: OwnerRecordId) -> bool {
        self.slab.record(id).is_some()
    }

    fn top_record(&self) -> Option<OwnerRecordId> {
        self.slab.top_record_id()
    }

    fn push_binding(
        &mut self,
        sym: SymId,
        slot: TlsIndex,
        channel: Channel,
        undo: UndoTarget,
    ) -> OwnerRecordId {
        self.slab
            .push_record(sym, slot, BindingKind::Let, channel, undo)
    }

    fn pop_binding(&mut self, id: OwnerRecordId) -> Option<OwnerBindingRecord> {
        take_top_binding(&mut self.slab, id)
    }
}

impl OwnerSlots for HarnessPlain {
    fn slot_state(&self, slot: TlsIndex, channel: Channel) -> SlotState {
        self.slot_reads.set(self.slot_reads.get() + 1);
        match channel {
            Channel::Default => self.slab.slot(slot, channel),
            Channel::BufferLocal(_) => SlotState::NoBinding,
        }
    }
}

impl OwnerSlotsMut for HarnessPlain {
    fn set_slot_state(&mut self, slot: TlsIndex, channel: Channel, state: SlotState) {
        self.slot_writes += 1;
        if let Some(log) = &self.log {
            log.push(format!("slot <- {}", plain_state(&state)));
        }
        self.slab.set_slot(slot, channel, state);
    }
}

impl VmPlain for HarnessPlain {
    fn generation(&self) -> (EraId, ShapeGeneration) {
        (self.era, self.shape)
    }
}

/// A value as the log prints it: a fixnum's digits, `unbound`, else its
/// kind name.
fn plain_value(value: &Value) -> String {
    match value.as_fixnum() {
        Some(number) => number.to_string(),
        None if value.is_unbound() => "unbound".to_string(),
        None => format!("{value:?}"),
    }
}

fn plain_state(state: &SlotState) -> String {
    match state {
        SlotState::NoBinding => "none".to_string(),
        SlotState::Bound(value) => plain_value(&value.get()),
    }
}

/// A dynamic test symbol.
fn plain_sym(name: &'static str) -> SymId {
    symbol_id(&Value::symbol(name), SymbolMode::Bare).expect("interned test symbol")
}

// ---------------------------------------------------------------- helpers

type Harness =
    VmExec<'static, RecordingEscape, HarnessVmSealed, HarnessArith, HarnessPlain, RecordingPoll>;

fn context() -> Context {
    Context::new(unit_runtime())
}

fn harness() -> Harness {
    harness_with(RecordingEscape::new())
}

fn harness_with(escape: RecordingEscape) -> Harness {
    harness_with_plain(escape, HarnessPlain::new())
}

fn harness_with_plain(escape: RecordingEscape, plain: HarnessPlain) -> Harness {
    VmExec::new(
        &(),
        escape,
        HarnessVmSealed,
        HarnessArith::new(),
        plain,
        RecordingPoll::new(),
    )
}

/// A harness whose `Return` hands back the frame's value: the rows that
/// read what an arithmetic opcode left on the stack.
fn arithmetic_harness() -> Harness {
    harness_with(RecordingEscape::mirroring_return())
}

/// The same over a Plain port of the row's.
fn value_harness(plain: HarnessPlain) -> Harness {
    harness_with_plain(RecordingEscape::mirroring_return(), plain)
}

fn program(ops: &[Op]) -> VmProgram<'_> {
    VmProgram {
        ops,
        constants: &[],
        function: Value::NIL,
        code: Value::NIL,
    }
}

fn program_with<'a>(ops: &'a [Op], constants: &'a [Value]) -> VmProgram<'a> {
    VmProgram {
        ops,
        constants,
        function: Value::NIL,
        code: Value::NIL,
    }
}

fn signal_key(flow: &Flow) -> (String, Vec<Value>) {
    let data = flow.as_signal().expect("Lisp signal");
    (data.symbol_name().to_string(), data.data.clone())
}

fn done_value<'era, E, S, A, V, P>(
    engine: &mut VmExec<'era, E, S, A, V, P>,
    outcome: VmOutcome,
) -> Value
where
    E: VmEscape,
    S: VmSealed,
    A: ArithOps,
    V: VmPlain,
    P: VmPoll,
{
    match outcome {
        VmOutcome::Done { pending } => engine.take(pending).expect("done root"),
        VmOutcome::Signaled(flow) => panic!("signaled {}", signal_key(&flow).0),
        VmOutcome::VoidVariable { symbol } => panic!("void-variable {symbol:?}"),
        VmOutcome::MissingCore { name } => panic!("missing core {name}"),
        VmOutcome::Fault(fault) => panic!("fault {fault:?}"),
    }
}

fn fault_of(outcome: VmOutcome) -> EngineFault {
    match outcome {
        VmOutcome::Fault(fault) => fault,
        VmOutcome::Signaled(flow) => panic!("signaled {}", signal_key(&flow).0),
        other => panic!("expected a fault, got {other:?}"),
    }
}

fn assert_clear<S: VmSealed, A: ArithOps, V: VmPlain>(
    engine: &VmExec<'_, RecordingEscape, S, A, V, RecordingPoll>,
) {
    assert_eq!(engine.arena_depth(), 0, "arena depth");
    assert_eq!(engine.live_root_count(), 0, "live roots");
    assert_eq!(engine.arena().handlers().len(), 0, "handlers");
    assert_eq!(engine.arena().bindings().len(), 0, "bindings");
    assert!(engine.is_idle(), "idle");
}

fn nils(count: usize) -> Vec<Op> {
    vec![Op::Nil; count]
}

fn token(id: u64) -> SerialRecordToken {
    SerialRecordToken(id)
}

fn handler(id: u64) -> SerialHandlerToken {
    SerialHandlerToken(id)
}

/// A completed escape with the opcode's static `consumed`, no record change
/// and no jump.
fn completion(op: &Op, produced: Vec<Value>) -> EscapeCompletion {
    EscapeCompletion::Completed {
        consumed: static_stack_effect(op).consumed,
        produced,
        jump: None,
        records: RecordDelta::default(),
        handlers: HandlerDelta::default(),
    }
}

/// A completed escape that also jumps: a `Switch` hit, the only opcode
/// that may report one.
fn completion_jumping(op: &Op, produced: Vec<Value>, jump: u32) -> EscapeCompletion {
    EscapeCompletion::Completed {
        consumed: static_stack_effect(op).consumed,
        produced,
        jump: Some(jump),
        records: RecordDelta::default(),
        handlers: HandlerDelta::default(),
    }
}

/// A completed escape that also pushes one serial record: the serial side
/// pushed a specpdl entry the owner did not mirror (`save-excursion`, an
/// unwind-protect, a localized binding).
fn completion_pushing(op: &Op, token: SerialRecordToken) -> EscapeCompletion {
    let effect = static_stack_effect(op);
    EscapeCompletion::Completed {
        consumed: effect.consumed,
        produced: vec![Value::NIL; effect.produced as usize],
        jump: None,
        records: RecordDelta {
            popped: Vec::new(),
            pushed: vec![token],
        },
        handlers: HandlerDelta::default(),
    }
}

/// A completed escape that also pops these serial records, innermost first.
fn completion_popping(op: &Op, popped: &[SerialRecordToken]) -> EscapeCompletion {
    let effect = static_stack_effect(op);
    EscapeCompletion::Completed {
        consumed: effect.consumed,
        produced: vec![Value::NIL; effect.produced as usize],
        jump: None,
        records: RecordDelta {
            popped: popped.to_vec(),
            pushed: Vec::new(),
        },
        handlers: HandlerDelta::default(),
    }
}

// ---------------------------------------------------------------- rows

#[test]
fn vm_scalar_opcodes_run_owner_locally_without_a_poll() {
    // (name, program without its Return, segment at the Return escape)
    let cases: Vec<(&str, Vec<Op>, Vec<Value>)> = vec![
        ("nil+listp", vec![Op::Nil, Op::Listp], vec![Value::T]),
        ("true+consp", vec![Op::True, Op::Consp], vec![Value::NIL]),
        (
            "dup+stack-ref",
            vec![Op::True, Op::Nil, Op::Dup, Op::StackRef(2)],
            vec![Value::T, Value::NIL, Value::NIL, Value::T],
        ),
        (
            "stack-ref-0-is-dup",
            vec![Op::Nil, Op::StackRef(0)],
            vec![Value::NIL, Value::NIL],
        ),
        (
            "stack-set-1",
            vec![Op::Nil, Op::True, Op::StackSet(1)],
            vec![Value::T],
        ),
        (
            "stack-set-0",
            vec![Op::Nil, Op::True, Op::StackSet(0)],
            vec![Value::NIL],
        ),
        (
            "discard-n",
            vec![Op::True, Op::Nil, Op::DiscardN(1)],
            vec![Value::T],
        ),
        (
            "discard-n-preserve-tos",
            vec![Op::Nil, Op::True, Op::DiscardN(0x81)],
            vec![Value::T],
        ),
        (
            "discard-n-0",
            vec![Op::Nil, Op::DiscardN(0)],
            vec![Value::NIL],
        ),
        ("pop", vec![Op::Nil, Op::True, Op::Pop], vec![Value::NIL]),
        (
            "stringp+not",
            vec![Op::Nil, Op::Stringp, Op::Not],
            vec![Value::T],
        ),
        (
            "forward-goto",
            vec![Op::Goto(2), Op::True, Op::Nil],
            vec![Value::NIL],
        ),
        (
            "goto-if-nil taken",
            vec![Op::Nil, Op::GotoIfNil(2), Op::True],
            vec![Value::T],
        ),
        (
            "goto-if-nil not taken",
            vec![Op::True, Op::GotoIfNil(2), Op::Nil],
            vec![Value::NIL],
        ),
        (
            "goto-if-not-nil taken",
            vec![Op::True, Op::GotoIfNotNil(2), Op::Nil],
            vec![Value::NIL],
        ),
        (
            "goto-if-not-nil not taken",
            vec![Op::Nil, Op::GotoIfNotNil(2), Op::True],
            vec![Value::T],
        ),
        (
            "else-pop keeps tos when taken",
            vec![Op::Nil, Op::True, Op::GotoIfNotNilElsePop(3), Op::Nil],
            vec![Value::NIL, Value::T, Value::NIL],
        ),
        (
            "else-pop pops when not taken",
            vec![Op::Nil, Op::True, Op::GotoIfNilElsePop(3), Op::Nil],
            vec![Value::NIL, Value::NIL],
        ),
        (
            "else-pop taken keeps nil",
            vec![Op::Nil, Op::GotoIfNilElsePop(2), Op::True],
            vec![Value::NIL, Value::T],
        ),
    ];
    let mut ctx = context();
    for (name, mut ops, expected) in cases {
        ops.push(Op::Return);
        let mut engine = harness();
        let outcome = engine.run(&mut ctx, program(&ops));
        // V4: `Return` is owner-local and takes the top operand (GNU
        // `Breturn`, `val = TOP`, bytecode.c:897); no escape, no poll.
        let top = expected.last().copied().expect("a segment");
        assert_eq!(
            done_value(&mut engine, outcome),
            top,
            "{name}: the top operand"
        );
        let escape = engine.escape_handler();
        assert_eq!(escape.requests.len(), 0, "{name}: escapes");
        assert_eq!(
            engine.poll_handler().edges,
            Vec::<PollEdge>::new(),
            "{name}: edges"
        );
        assert_clear(&engine);
    }
}

#[test]
fn vm_scalar_loop_polls_only_at_the_back_edge() {
    // [nil, t, nil x 4]; the branch at pc 6 targets itself, so each pass
    // pops one nil and jumps back to pc 6: the loop polls once per pass; the
    // fifth pass finds t, pops it and falls through. A back edge polls; a
    // forward edge and a fall-through do not.
    let ops = vec![
        Op::Nil,
        Op::True,
        Op::Nil,
        Op::Nil,
        Op::Nil,
        Op::Nil,
        Op::GotoIfNil(6),
        Op::Return,
    ];
    let mut engine = harness();
    let outcome = engine.run(&mut context(), program(&ops));
    assert_eq!(done_value(&mut engine, outcome), Value::NIL);
    let mut expected = vec![PollEdge::VmBackEdge; 4];
    assert_eq!(engine.poll_handler().edges, expected);
    let escape = engine.escape_handler();
    assert_eq!(escape.requests.len(), 0, "Return is owner-local (V4)");
    assert_clear(&engine);
}

/// Every `Op` variant with one concrete payload, from two lists.
///
/// The lists feed an exhaustive `match` on `Op` with no wildcard arm, so an
/// `Op` variant without a row here fails the test build; the same lists
/// yield the samples the rows below run.
macro_rules! op_samples {
    (
        unit { $($unit:ident),* $(,)? }
        tuple { $($tuple:ident ($($payload:expr),*)),* $(,)? }
    ) => {
        // Exhaustive by construction: one arm per listed variant, no `_`.
        fn every_variant_is_listed(op: &Op) -> bool {
            match op {
                $(Op::$unit => true,)*
                $(Op::$tuple { .. } => true,)*
            }
        }

        fn op_samples() -> Vec<(&'static str, Op)> {
            vec![
                $((stringify!($unit), Op::$unit),)*
                $((stringify!($tuple), Op::$tuple($($payload),*))),*
            ]
        }
    };
}

op_samples! {
    unit {
        Nil,
        True,
        Pop,
        Dup,
        Switch,
        Return,
        Add,
        Sub,
        Mul,
        Div,
        Rem,
        Add1,
        Sub1,
        Negate,
        Eqlsign,
        Gtr,
        Lss,
        Leq,
        Geq,
        Max,
        Min,
        Car,
        Cdr,
        Cons,
        Length,
        Nth,
        Nthcdr,
        Setcar,
        Setcdr,
        CarSafe,
        CdrSafe,
        Elt,
        Nconc,
        Nreverse,
        Member,
        Memq,
        Assq,
        Symbolp,
        Consp,
        Stringp,
        Listp,
        Integerp,
        Numberp,
        Null,
        Not,
        Eq,
        Equal,
        Substring,
        StringEqual,
        StringLessp,
        Aref,
        Aset,
        SymbolValue,
        SymbolFunction,
        Set,
        Fset,
        Get,
        Put,
        PopHandler,
        UnwindProtectPop,
        Throw,
        SaveCurrentBuffer,
        SaveExcursion,
        SaveRestriction,
        SaveWindowExcursion,
    }
    tuple {
        Constant(0u16),
        TrapOutOfRangeConstant(0u16),
        StackRef(1u16),
        StackSet(1u16),
        DiscardN(2u8),
        VarRef(0u16),
        VarSet(0u16),
        VarBind(0u16),
        Unbind(1u16),
        Call(2u16),
        Apply(2u16),
        Goto(3u32),
        GotoIfNil(3u32),
        GotoIfNotNil(3u32),
        GotoIfNilElsePop(3u32),
        GotoIfNotNilElsePop(3u32),
        List(2u16),
        Concat(2u16),
        PushConditionCase(3u32),
        PushConditionCaseRaw(3u32),
        PushCatch(3u32),
        MakeClosure(0u16),
        CallBuiltin(0u16, 2u8),
        CallBuiltinSym(NIL_SYM_ID, 2u8),
    }
}

/// Payload shapes with an effect of their own, beyond each variant's
/// canonical sample.
fn extra_payload_shapes() -> Vec<(&'static str, Op)> {
    vec![
        ("DiscardN(0)", Op::DiscardN(0)),
        ("DiscardN(preserve 2)", Op::DiscardN(0x82)),
        ("DiscardN(127)", Op::DiscardN(0x7F)),
        ("DiscardN(preserve 127)", Op::DiscardN(0xFF)),
        ("Apply(0)", Op::Apply(0)),
        ("Call(0)", Op::Call(0)),
        ("List(0)", Op::List(0)),
        ("Concat(0)", Op::Concat(0)),
        ("CallBuiltin(0, 0)", Op::CallBuiltin(0, 0)),
        ("CallBuiltinSym(nil, 0)", Op::CallBuiltinSym(NIL_SYM_ID, 0)),
        ("StackRef(0)", Op::StackRef(0)),
        ("StackSet(0)", Op::StackSet(0)),
        ("StackSet(3)", Op::StackSet(3)),
        ("Unbind(127)", Op::Unbind(127)),
    ]
}

fn is_call(op: &Op) -> bool {
    matches!(
        op,
        Op::Call(_) | Op::Apply(_) | Op::CallBuiltin(..) | Op::CallBuiltinSym(..)
    )
}

#[test]
fn vm_every_world_opcode_outside_the_arith_and_plain_classes_is_exactly_one_escape() {
    // The arithmetic opcodes escape only for a marker operand
    // (`vm_a_marker_operand_escapes_through_the_one_stub`); the Plain
    // variable opcodes run on the owner slab and escape only for a refused
    // symbol or a serial record run (the `vm_var…` and `vm_unbind…` rows).
    let mut escaped = 0;
    let mut ctx = context();
    for (name, op) in op_samples() {
        assert!(every_variant_is_listed(&op), "{name}: listed");
        let ParallelArm::World(split) = opcode_slot(&op).arm else {
            continue;
        };
        if matches!(
            split,
            WorldSplit::Arith(_) | WorldSplit::PlainVar(_) | WorldSplit::Return(_)
        ) {
            // `Return` is typed since V4 (owner-local, or the serial cleanup
            // escape): its rows are the vm_owner_local_return_* and
            // vm_debug_on_exit_* ones.
            continue;
        }
        escaped += 1;
        let effect = static_stack_effect(&op);
        let mut ops = nils(effect.consumed as usize);
        ops.push(op.clone());
        if effect.produced == 0 {
            ops.push(Op::Nil);
        }
        ops.push(Op::Return);
        let is_handler_push = matches!(
            op,
            Op::PushCatch(_) | Op::PushConditionCase(_) | Op::PushConditionCaseRaw(_)
        );
        let mut engine = if is_handler_push {
            // V8 L1: handler-push opcodes must push exactly one serial handler
            // token; Return then cleans it up via `NeedsSerialCleanup`.
            let h = handler(1);
            VmExec::new(
                &(),
                RecordingEscape::with_script(vec![
                    Some(EscapeCompletion::Completed {
                        consumed: effect.consumed,
                        produced: Vec::new(),
                        jump: None,
                        records: RecordDelta::default(),
                        handlers: HandlerDelta {
                            popped: Vec::new(),
                            pushed: Some(h),
                        },
                    }),
                    Some(EscapeCompletion::Completed {
                        consumed: 1,
                        produced: vec![Value::NIL],
                        jump: None,
                        records: RecordDelta::default(),
                        handlers: HandlerDelta {
                            popped: vec![h],
                            pushed: None,
                        },
                    }),
                ]),
                HarnessVmSealed,
                HarnessArith::new(),
                HarnessPlain::new(),
                RecordingPoll::new(),
            )
        } else {
            harness()
        };
        let outcome = engine.run(&mut ctx, program(&ops));
        assert_eq!(done_value(&mut engine, outcome), Value::NIL, "{name}");
        let escape = engine.escape_handler();
        let expected_requests = if is_handler_push { 2 } else { 1 };
        assert_eq!(
            escape.requests.len(),
            expected_requests,
            "{name}: the opcode (plus Return cleanup when a serial handler was installed)"
        );
        let request = &escape.requests[0];
        assert_eq!(request.op, op, "{name}");
        assert_eq!(request.why, EscapeWhy::World(split), "{name}");
        assert_eq!(request.pc, Pc::new(effect.consumed as usize), "{name}: pc");
        assert_eq!(
            request.operands,
            OperandCursor::new(0, effect.consumed as usize),
            "{name}: cursor"
        );
        assert_eq!(
            escape.segments[0],
            vec![Value::NIL; effect.consumed as usize],
            "{name}: segment at the escape"
        );
        let mut expected = Vec::new();
        if is_call(&op) {
            expected.push(PollEdge::VmCall);
        }
        expected.push(PollEdge::SerialRequest);
        if is_handler_push {
            expected.push(PollEdge::SerialRequest);
        }
        assert_eq!(engine.poll_handler().edges, expected, "{name}: edges");
        assert_clear(&engine);
    }
    // 43 before V4; `Return` is typed now.
    assert_eq!(escaped, 42, "escaped opcodes");
}

#[test]
fn vm_the_dispatch_table_and_the_inventory_agree_on_all_89_opcodes() {
    let (mut scalar, mut sealed, mut escape) = (0, 0, 0);
    let (mut plain_var, mut ret, mut arith) = (0, 0, 0);
    let mut authorities = Vec::new();
    for (name, op) in op_samples() {
        let slot = opcode_slot(&op);
        // The Arith arm is exactly the fifteen kinds, and a kind's operand
        // count is its opcode's static `consumed`: the stepper hands the
        // port that many operands and no more.
        assert_eq!(
            ArithGenericKind::from_op(&op).is_some(),
            matches!(slot.arm, ParallelArm::World(WorldSplit::Arith(_))),
            "{name}: the Arith arm is exactly the kinds"
        );
        // The PlainVar arm is exactly the four variable opcodes (V2).
        assert_eq!(
            matches!(
                op,
                Op::VarRef(_) | Op::VarSet(_) | Op::VarBind(_) | Op::Unbind(_)
            ),
            matches!(slot.arm, ParallelArm::World(WorldSplit::PlainVar(_))),
            "{name}: the PlainVar arm is exactly the variable opcodes"
        );
        match slot.arm {
            ParallelArm::Scalar => scalar += 1,
            ParallelArm::Sealed(authority) => {
                sealed += 1;
                authorities.push((name, authority));
            }
            ParallelArm::World(WorldSplit::Escape) => escape += 1,
            ParallelArm::World(WorldSplit::PlainVar(_)) => plain_var += 1,
            ParallelArm::World(WorldSplit::Return(_)) => ret += 1,
            ParallelArm::World(WorldSplit::Arith(_)) => {
                arith += 1;
                let kind =
                    ArithGenericKind::from_op(&op).unwrap_or_else(|| panic!("{name}: a kind"));
                assert_eq!(
                    usize::from(static_stack_effect(&op).consumed),
                    kind.arity(),
                    "{name}: the port sees the static operand count"
                );
            }
        }
        // The stage-1 accessor admits every typed World split and no bare
        // escape; this executor runs `Arith(Numeric)` through the port when
        // no operand is a marker and the Plain variable opcodes through the
        // Plain port when the admission mints a key, and escapes every other
        // World arm.
        assert_eq!(
            admits_owner_local(slot),
            !matches!(slot.arm, ParallelArm::World(WorldSplit::Escape)),
            "{name}: admits_owner_local"
        );
    }
    assert_eq!(
        (scalar, sealed, escape, plain_var, ret, arith),
        (19, 8, 42, 4, 1, 15),
        "inventory partition"
    );
    let count = |authority| {
        authorities
            .iter()
            .filter(|(_, seen)| *seen == authority)
            .count()
    };
    assert_eq!(count(SealedAuthority::ConsFieldRead), 4);
    assert_eq!(count(SealedAuthority::ConsAllocation), 2);
    assert_eq!(count(SealedAuthority::ConsFieldWrite), 2);
    assert_eq!(op_samples().len(), 89, "one sample per Op variant");
}

#[test]
fn vm_escape_result_is_in_the_published_segment_before_the_next_poll() {
    // The first Switch escape reports a hit and jumps back to pc 0
    // (`Bswitch` leaves through `op_branch`, bytecode.c:1752-1758): the
    // back-edge poll that follows must see the applied effect in the
    // published segment, the table and the key gone and the cons below
    // them still there. The second reports a miss; Return escapes over
    // [cons cons] and reports the default nil.
    let mut ctx = context();
    let ops = vec![Op::Constant(0), Op::Nil, Op::Nil, Op::Switch, Op::Return];
    let cons = Value::cons(Value::T, Value::NIL);
    let script = vec![
        Some(completion_jumping(&Op::Switch, Vec::new(), 0)),
        Some(completion(&Op::Switch, Vec::new())),
    ];
    let mut engine = VmExec::new(
        &(),
        RecordingEscape::with_script(script),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    let outcome = engine.run(&mut ctx, program_with(&ops, &[cons]));
    // V4: the owner-local Return takes the top operand, `cons` after the loop.
    assert_eq!(done_value(&mut engine, outcome), cons);
    let poll = engine.poll_handler();
    assert_eq!(
        poll.edges,
        vec![
            PollEdge::SerialRequest,
            PollEdge::VmBackEdge,
            PollEdge::SerialRequest
        ]
    );
    let index = poll
        .edges
        .iter()
        .position(|edge| *edge == PollEdge::VmBackEdge)
        .expect("one back edge");
    assert_eq!(poll.segments[index], vec![cons]);
    let escape = engine.escape_handler();
    assert_eq!(escape.requests.len(), 2);
    assert_eq!(
        escape.segments,
        vec![
            vec![cons, Value::NIL, Value::NIL],
            vec![cons, cons, Value::NIL, Value::NIL]
        ]
    );
    assert_clear(&engine);
}

#[test]
fn vm_a_completion_that_jumps_for_an_opcode_other_than_switch_is_a_typed_fault() {
    // Only `Switch` reports a jump (lead C1). For any other opcode the typed
    // fault is raised with R3, before the record mirror and the segment are
    // touched (C2), and the scene stays.
    let mut ctx = context();
    // (program, script, faulting op, the LIFOs that must be untouched)
    let cases: Vec<(Vec<Op>, Vec<Option<EscapeCompletion>>, Op, Lifo)> = vec![
        (
            vec![Op::Nil, Op::Length, Op::Return],
            vec![Some(completion_jumping(&Op::Length, vec![Value::T], 0))],
            Op::Length,
            Vec::new(),
        ),
        (
            // The first escape seeds the bindings with one serial token;
            // Return then reports a jump and pops that token.
            vec![Op::Nil, Op::SaveExcursion, Op::Nil, Op::Return],
            vec![
                Some(completion_pushing(&Op::SaveExcursion, token(1))),
                Some(EscapeCompletion::Completed {
                    consumed: static_stack_effect(&Op::Return).consumed,
                    produced: vec![Value::T],
                    jump: Some(0),
                    records: RecordDelta {
                        popped: vec![token(1)],
                        pushed: Vec::new(),
                    },
                    handlers: HandlerDelta::default(),
                }),
            ],
            Op::Return,
            vec![Some(1)],
        ),
    ];
    for (ops, script, op, bindings) in cases {
        let mut engine = VmExec::new(
            &(),
            RecordingEscape::with_script(script),
            HarnessVmSealed,
            HarnessArith::new(),
            HarnessPlain::new(),
            RecordingPoll::new(),
        );
        let outcome = engine.run(&mut ctx, program(&ops));
        assert!(
            matches!(fault_of(outcome), EngineFault::DispatchContract(op2) if op2 == op),
            "{op:?}"
        );
        let frame = engine
            .frame_id()
            .unwrap_or_else(|| panic!("{op:?}: the frame stays"));
        let segment = engine
            .arena()
            .vm_segment(frame)
            .unwrap_or_else(|| panic!("{op:?}: the segment stays"));
        assert_eq!(segment.last(), Some(&Value::NIL), "{op:?}: segment");
        assert_eq!(engine.arena_depth(), 1, "{op:?}");
        assert_eq!(
            lifos(engine.arena()),
            (bindings, Vec::new()),
            "{op:?}: the mirror never ran"
        );
        assert_eq!(
            engine.escape_handler().requests.len(),
            if op == Op::Return { 2 } else { 1 },
            "{op:?}: one request per escape before the fault"
        );
        assert_eq!(
            engine.poll_handler().edges,
            vec![PollEdge::SerialRequest; if op == Op::Return { 2 } else { 1 }],
            "{op:?}: no back-edge poll for the refused jump"
        );
        // A contract fault refuses further work.
        let outcome = engine.run(&mut ctx, program(&[Op::Nil, Op::Return]));
        assert!(
            matches!(outcome, VmOutcome::Fault(EngineFault::Poisoned)),
            "{op:?}"
        );
    }
}

#[test]
fn vm_poll_refusal_is_engine_fault_poll_and_restarts() {
    // (name, program, refusing edge)
    let cases: Vec<(&str, Vec<Op>, PollEdge)> = vec![
        (
            "serial request",
            vec![Op::Nil, Op::SymbolValue, Op::Return],
            PollEdge::SerialRequest,
        ),
        (
            "back edge",
            vec![Op::Nil, Op::True, Op::Nil, Op::GotoIfNil(3), Op::Return],
            PollEdge::VmBackEdge,
        ),
        (
            "call",
            vec![Op::True, Op::Call(0), Op::Return],
            PollEdge::VmCall,
        ),
    ];
    let mut ctx = context();
    for (name, ops, edge) in cases {
        let mut engine = VmExec::new(
            &(),
            RecordingEscape::new(),
            HarnessVmSealed,
            HarnessArith::new(),
            HarnessPlain::new(),
            RecordingPoll::refusing_on(edge, RendezvousError::Publication),
        );
        let outcome = engine.run(&mut ctx, program(&ops));
        assert!(
            matches!(
                outcome,
                VmOutcome::Fault(EngineFault::Poll(RendezvousError::Publication))
            ),
            "{name}"
        );
        // Every refusal precedes its request: nothing escaped.
        assert_eq!(engine.escape_handler().requests.len(), 0, "{name}: escapes");
        assert_eq!(engine.poll_handler().edges, vec![edge], "{name}: edges");
        assert_clear(&engine);
        // The refusal was one-shot; the same engine runs again.
        let outcome = engine.run(&mut ctx, program(&ops));
        assert_eq!(
            done_value(&mut engine, outcome),
            Value::NIL,
            "{name}: second run"
        );
        assert_clear(&engine);
    }
}

#[test]
fn vm_a_signal_from_an_escape_settles_to_idle_and_never_resumes() {
    // PushConditionCase installs a serial handler marker; SymbolValue
    // signals. The run ends at the signaling opcode: no landing pad, no
    // third poll, the marker is unbound with the frame.
    let mut ctx = context();
    let ops = vec![
        Op::Nil,
        Op::PushConditionCase(4),
        Op::SymbolValue,
        Op::Return,
    ];
    let script = vec![
        Some(EscapeCompletion::Completed {
            consumed: static_stack_effect(&Op::PushConditionCase(4)).consumed,
            produced: Vec::new(),
            jump: None,
            records: RecordDelta::default(),
            handlers: HandlerDelta {
                popped: Vec::new(),
                pushed: Some(handler(7)),
            },
        }),
        Some(EscapeCompletion::Flow(signal(
            "error",
            vec![Value::string("boom")],
        ))),
    ];
    let mut engine = VmExec::new(
        &(),
        RecordingEscape::with_script(script),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    let outcome = engine.run(&mut ctx, program(&ops));
    let flow = match outcome {
        VmOutcome::Signaled(flow) => flow,
        other => panic!("expected a signal, got {other:?}"),
    };
    assert_eq!(
        signal_key(&flow),
        ("error".to_string(), vec![Value::string("boom")])
    );
    let escape = engine.escape_handler();
    assert_eq!(escape.requests.len(), 2);
    assert_eq!(escape.requests[1].op, Op::SymbolValue);
    assert_eq!(escape.requests[1].pc, Pc::new(2));
    assert_eq!(escape.lifos[1], (vec![], vec![Some(7)]));
    assert_eq!(
        engine.poll_handler().edges,
        vec![PollEdge::SerialRequest, PollEdge::SerialRequest]
    );
    assert_clear(&engine);
    let outcome = engine.run(&mut ctx, program(&[Op::Nil, Op::Return]));
    assert_eq!(done_value(&mut engine, outcome), Value::NIL);
}

#[test]
fn vm_an_escape_completion_with_the_wrong_arity_is_a_typed_fault_and_keeps_the_scene() {
    // The first escape seeds both LIFOs with serial markers. The second
    // reports SymbolValue with no produced operand: R3 raises the typed
    // fault before the record pops/pushes or the segment, and the scene
    // stays for inspection.
    let ops = vec![
        Op::Nil,
        Op::SaveExcursion,
        Op::PushConditionCase(5),
        Op::SymbolValue,
        Op::Return,
    ];
    let script = vec![
        Some(completion_pushing(&Op::SaveExcursion, token(1))),
        Some(EscapeCompletion::Completed {
            consumed: static_stack_effect(&Op::PushConditionCase(5)).consumed,
            produced: Vec::new(),
            jump: None,
            records: RecordDelta::default(),
            handlers: HandlerDelta {
                popped: Vec::new(),
                pushed: Some(handler(5)),
            },
        }),
        Some(EscapeCompletion::Completed {
            consumed: 1,
            produced: Vec::new(),
            jump: None,
            records: RecordDelta {
                popped: vec![token(1)],
                pushed: vec![token(2)],
            },
            handlers: HandlerDelta {
                popped: vec![handler(5)],
                pushed: Some(handler(6)),
            },
        }),
    ];
    let mut engine = VmExec::new(
        &(),
        RecordingEscape::with_script(script),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    let outcome = engine.run(&mut context(), program(&ops));
    assert!(matches!(
        fault_of(outcome),
        EngineFault::EscapeStackEffect {
            op: Op::SymbolValue,
            expected_consumed: 1,
            expected_produced: 1,
            got_consumed: 1,
            got_produced: 0,
        }
    ));
    let frame = engine.frame_id().expect("the frame stays");
    assert_eq!(engine.arena().vm_segment(frame), Some(&[Value::NIL][..]));
    assert_eq!(lifos(engine.arena()), (vec![Some(1)], vec![Some(5)]));
    assert_eq!(engine.arena_depth(), 1);
    // A contract fault refuses further work.
    let outcome = engine.run(&mut context(), program(&[Op::Nil, Op::Return]));
    assert!(matches!(outcome, VmOutcome::Fault(EngineFault::Poisoned)));
}

#[test]
fn vm_an_escape_requested_without_its_operands_signals_invalid_byte_code_before_any_poll() {
    // An escaping opcode reached with fewer operands than its static effect
    // consumes is malformed code, the Scalar and Sealed arms' `Invalid
    // byte-code` (lead C3), refused before any poll or request.
    // `EscapeStackUnderflow` stays in the stub as the backstop; the handler
    // sees the arena by shared reference, so no completion can reach it.
    let cases: Vec<(&str, Vec<Op>)> = vec![
        (
            "binary opcode, one operand",
            vec![Op::Nil, Op::Eq, Op::Return],
        ),
        ("unary opcode, none", vec![Op::SymbolValue, Op::Return]),
        ("call without its callee", vec![Op::Call(0), Op::Return]),
        ("return over an empty segment", vec![Op::Return]),
    ];
    let mut ctx = context();
    for (name, ops) in cases {
        let mut engine = harness();
        let outcome = engine.run(&mut ctx, program(&ops));
        let flow = match outcome {
            VmOutcome::Signaled(flow) => flow,
            other => panic!("{name}: expected a signal, got {other:?}"),
        };
        assert_eq!(
            signal_key(&flow),
            (
                "error".to_string(),
                vec![Value::string("Invalid byte-code")]
            ),
            "{name}"
        );
        assert_eq!(
            engine.escape_handler().requests.len(),
            0,
            "{name}: no request"
        );
        assert_eq!(engine.poll_handler().edges, Vec::new(), "{name}: no poll");
        assert_clear(&engine);
        let outcome = engine.run(&mut ctx, program(&[Op::Nil, Op::Return]));
        assert_eq!(
            done_value(&mut engine, outcome),
            Value::NIL,
            "{name}: restart"
        );
    }
}

#[test]
fn vm_escape_completions_mirror_serial_records_and_handlers_in_lifo_order() {
    let ops = vec![
        Op::Nil,
        Op::SaveExcursion,
        Op::PushConditionCase(5),
        Op::SymbolValue,
        Op::SymbolValue,
        Op::Return,
    ];
    let script = vec![
        // Pushed outermost first: [A, B] with B innermost; then one handler.
        Some(EscapeCompletion::Completed {
            consumed: 0,
            produced: Vec::new(),
            jump: None,
            records: RecordDelta {
                popped: Vec::new(),
                pushed: vec![token(1), token(2)],
            },
            handlers: HandlerDelta::default(),
        }),
        Some(EscapeCompletion::Completed {
            consumed: 0,
            produced: Vec::new(),
            jump: None,
            records: RecordDelta::default(),
            handlers: HandlerDelta {
                popped: Vec::new(),
                pushed: Some(handler(10)),
            },
        }),
        // Popped innermost first: [B, A]; then C.
        Some(EscapeCompletion::Completed {
            consumed: static_stack_effect(&Op::SymbolValue).consumed,
            produced: vec![Value::NIL],
            jump: None,
            records: RecordDelta {
                popped: vec![token(2), token(1)],
                pushed: vec![token(3)],
            },
            handlers: HandlerDelta::default(),
        }),
        Some(EscapeCompletion::Completed {
            consumed: static_stack_effect(&Op::SymbolValue).consumed,
            produced: vec![Value::NIL],
            jump: None,
            records: RecordDelta {
                popped: vec![token(3)],
                pushed: Vec::new(),
            },
            handlers: HandlerDelta {
                popped: vec![handler(10)],
                pushed: None,
            },
        }),
    ];
    let mut engine = VmExec::new(
        &(),
        RecordingEscape::with_script(script),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    let outcome = engine.run(&mut context(), program(&ops));
    assert_eq!(done_value(&mut engine, outcome), Value::NIL);
    let escape = engine.escape_handler();
    assert_eq!(
        escape.lifos,
        vec![
            (vec![], vec![]),
            (vec![Some(1), Some(2)], vec![]),
            (vec![Some(1), Some(2)], vec![Some(10)]),
            (vec![Some(3)], vec![Some(10)]),
        ]
    );
    assert_eq!(escape.requests.len(), 4);
    assert_clear(&engine);
}

#[test]
fn vm_a_completion_that_pops_a_non_top_serial_record_faults_and_applies_nothing() {
    // (name, second completion's deltas, the LIFO it names wrongly)
    let cases: Vec<(&str, RecordDelta, HandlerDelta, RecordStack)> = vec![
        (
            "bindings",
            RecordDelta {
                popped: vec![token(2)],
                pushed: vec![token(3)],
            },
            HandlerDelta {
                popped: vec![handler(5)],
                pushed: Some(handler(6)),
            },
            RecordStack::Bindings,
        ),
        (
            "handlers",
            RecordDelta {
                popped: vec![token(1)],
                pushed: vec![token(3)],
            },
            HandlerDelta {
                popped: vec![handler(6)],
                pushed: None,
            },
            RecordStack::Handlers,
        ),
    ];
    for (name, records, handlers, expected) in cases {
        let ops = vec![
            Op::Nil,
            Op::SaveExcursion,
            Op::PushConditionCase(5),
            Op::SymbolValue,
            Op::Return,
        ];
        let script = vec![
            Some(completion_pushing(&Op::SaveExcursion, token(1))),
            Some(EscapeCompletion::Completed {
                consumed: 0,
                produced: Vec::new(),
                jump: None,
                records: RecordDelta::default(),
                handlers: HandlerDelta {
                    popped: Vec::new(),
                    pushed: Some(handler(5)),
                },
            }),
            Some(EscapeCompletion::Completed {
                consumed: static_stack_effect(&Op::SymbolValue).consumed,
                produced: vec![Value::NIL],
                jump: None,
                records,
                handlers,
            }),
        ];
        let mut engine = VmExec::new(
            &(),
            RecordingEscape::with_script(script),
            HarnessVmSealed,
            HarnessArith::new(),
            HarnessPlain::new(),
            RecordingPoll::new(),
        );
        let outcome = engine.run(&mut context(), program(&ops));
        assert!(
            matches!(
                fault_of(outcome),
                EngineFault::EscapeRecordContract(stack) if stack == expected
            ),
            "{name}"
        );
        // Neither LIFO changed and the operand is still there.
        assert_eq!(
            lifos(engine.arena()),
            (vec![Some(1)], vec![Some(5)]),
            "{name}"
        );
        let frame = engine.frame_id().expect("the frame stays");
        assert_eq!(
            engine.arena().vm_segment(frame),
            Some(&[Value::NIL][..]),
            "{name}"
        );
    }
    // The LIFO rule itself: `tokens[0]` is the innermost marker, and a
    // mismatch removes nothing.
    let mut bindings = BindStack::new();
    bindings.push(token(1));
    bindings.push(token(2));
    assert!(bindings.serial_run_matches(&[]));
    assert!(bindings.serial_run_matches(&[token(2)]));
    assert!(bindings.serial_run_matches(&[token(2), token(1)]));
    assert!(!bindings.serial_run_matches(&[token(1)]));
    assert!(!bindings.serial_run_matches(&[token(1), token(2)]));
    assert!(!bindings.serial_run_matches(&[token(3)]));
    assert_eq!(bindings.pop_serial_run(&[token(1)]), Err(()));
    assert_eq!(bindings.len(), 2);
    assert_eq!(bindings.pop_serial_run(&[token(2), token(1)]), Ok(2));
    assert_eq!(bindings.len(), 0);
    let mut handlers = HandlerStack::new();
    let mut dummy_arena = FrameArena::new(4);
    let dummy_frame = dummy_arena.push_interp(interp_frame()).expect("frame");
    handlers.push(OwnerHandler::Serial {
        token: handler(1),
        pad: LandingPad {
            frame: dummy_frame,
            dest: 0,
            operand_len: 0,
            bind_mark: OwnerRecordMark::EMPTY,
            depth: 1,
        },
    });
    assert!(!handlers.serial_run_matches(&[handler(2)]));
    assert_eq!(handlers.pop_serial_run(&[handler(2)]), Err(()));
    assert_eq!(handlers.len(), 1);
    assert_eq!(handlers.pop_serial_run(&[handler(1)]), Ok(1));
    assert_eq!(handlers.len(), 0);
}

fn budget() -> PollBudget {
    PollBudget::new(NonZeroU32::MIN)
}

fn interp_frame() -> InterpFrame {
    InterpFrame::new(
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        1,
        HandlerMark::EMPTY,
        OwnerRecordMark::EMPTY,
        budget(),
    )
}

fn handler_record(frame: FrameId, tag: i64, handlers: i64) -> HandlerRecord {
    HandlerRecord::new(
        HandlerKind::ConditionCase,
        Value::fixnum(tag),
        Value::fixnum(handlers),
        frame,
        OwnerRecordMark::EMPTY,
    )
}

/// An owner binding entry naming a record a throwaway slab issued: it holds
/// no value of its own (C2), so the arena walk publishes nothing for it.
fn slot_record() -> SlotRecord {
    let record = OwnerSlab::new(0).push_record(
        NIL_SYM_ID,
        TlsIndex::VOID,
        BindingKind::Let,
        Channel::Default,
        UndoTarget::Retired,
    );
    SlotRecord::new(NIL_SYM_ID, record)
}

fn handler_and_specpdls(arena: &FrameArena) -> Vec<(&'static str, Option<i64>)> {
    let mut seen = Vec::new();
    arena.trace_roots(&mut |group, value| {
        if matches!(group, RootGroup::Handlers | RootGroup::Specpdl) {
            seen.push((group.name(), value.as_fixnum()));
        }
    });
    seen
}

#[test]
fn vm_serial_markers_and_owner_binding_entries_publish_no_roots() {
    // A handler record publishes its tag and its handler list; a serial
    // marker publishes nothing; an owner binding entry publishes nothing
    // either (C2, peer #11640): it names the slab record that stores the
    // binding's values, and the collector traces the slab.
    let mut arena = FrameArena::new(8);
    let frame = arena.push_interp(interp_frame()).expect("interp frame");
    arena.handlers_mut().push(handler_record(frame, 19, 20));
    arena.bindings_mut().push(slot_record());
    let owner_records = vec![("handlers", Some(19)), ("handlers", Some(20))];
    assert_eq!(handler_and_specpdls(&arena), owner_records);
    // Markers join both LIFOs above the owner records and publish nothing.
    let mark = arena.handlers_mut().mark();
    arena.handlers_mut().push(OwnerHandler::Serial {
        token: handler(1),
        pad: LandingPad {
            frame,
            dest: 0,
            operand_len: 0,
            bind_mark: OwnerRecordMark::EMPTY,
            depth: 1,
        },
    });
    arena.bindings_mut().push(token(2));
    assert!(arena.handlers().has_serial_above(mark));
    assert!(arena.bindings().has_serial_above(OwnerRecordMark::EMPTY));
    assert_eq!(handler_and_specpdls(&arena), owner_records);
    // Truncation to the mark removes the marker and keeps the owner record.
    assert_eq!(arena.handlers_mut().truncate(mark), 1);
    assert!(!arena.handlers().has_serial_above(mark));
    assert_eq!(handler_and_specpdls(&arena), owner_records);
    // Truncation to the base removes owner records and markers alike.
    assert_eq!(arena.handlers_mut().truncate(HandlerMark::EMPTY), 1);
    assert_eq!(arena.bindings_mut().truncate(OwnerRecordMark::EMPTY), 2);
    assert!(handler_and_specpdls(&arena).is_empty());
}

#[test]
fn vm_owner_local_return_of_the_bottom_frame_exits_with_the_value() {
    // V4: a `Return` with no serial state to run is owner-local. GNU
    // `Breturn` (bytecode.c:892-926): `val = TOP` (:897); with no caller,
    // `goto exit` (:925-926) with the value. No escape: the completion the
    // script left for the Return is never taken, and the run's value is the
    // operand, not a reported one. (V1-V3 escaped every `Return`; this row
    // was that stand-in.)
    let constants = [Value::fixnum(42)];
    let ops = [Op::Constant(0), Op::Return];
    let script = vec![Some(completion(&Op::Return, vec![Value::fixnum(7)]))];
    let mut engine = VmExec::new(
        &(),
        RecordingEscape::with_script(script),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    let outcome = engine.run(&mut context(), program_with(&ops, &constants));
    let pending = match outcome {
        VmOutcome::Done { pending } => pending,
        other => panic!("expected Done, got {other:?}"),
    };
    assert_eq!(engine.escape_handler().requests.len(), 0, "no escape");
    assert_eq!(engine.lookup_root(pending), Some(Value::fixnum(42)));
    assert_eq!(engine.take(pending), Ok(Value::fixnum(42)));
    assert_clear(&engine);
    let outcome = engine.run(&mut context(), program(&[Op::Nil, Op::Return]));
    assert_eq!(done_value(&mut engine, outcome), Value::NIL);
}

#[test]
fn vm_debug_on_exit_return_takes_the_serial_cleanup() {
    // V4, the first trigger: GNU `Breturn` runs the debugger when the frame's
    // backtrace entry wants debug-on-exit (bytecode.c:900-901), and the
    // debugger may replace the value. The frame's DEBUG_ON_EXIT flag sends
    // the Return through the ONE escape; the value the completion carries is
    // the run's. The same program without the flag is the owner-local row.
    let constants = [Value::fixnum(42)];
    let ops = [Op::Constant(0), Op::Return];
    let script = vec![Some(completion(&Op::Return, vec![Value::fixnum(7)]))];
    let mut engine = VmExec::new(
        &(),
        RecordingEscape::with_script(script),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    engine.set_entry_exit_flags(ExitFlags::DEBUG_ON_EXIT);
    let outcome = engine.run(&mut context(), program_with(&ops, &constants));
    let pending = match outcome {
        VmOutcome::Done { pending } => pending,
        other => panic!("expected Done, got {other:?}"),
    };
    let escape = engine.escape_handler();
    assert_eq!(escape.requests.len(), 1);
    assert_eq!(escape.requests[0].op, Op::Return);
    assert!(
        matches!(
            escape.requests[0].why,
            EscapeWhy::World(WorldSplit::Return(ReturnDisposition::NeedsSerialCleanup))
        ),
        "the escape names the serial cleanup"
    );
    assert_eq!(
        engine.lookup_root(pending),
        Some(Value::fixnum(7)),
        "the debugger's value"
    );
    assert_eq!(engine.take(pending), Ok(Value::fixnum(7)));
    assert_clear(&engine);
}

#[test]
fn vm_the_entry_debug_on_exit_flag_lasts_one_run() {
    // The harness arms the bottom frame's DEBUG_ON_EXIT for ONE run: GNU's
    // flag is per backtrace frame (bytecode.c:900), and each run pushes a
    // new frame. Run 1's Return escapes; run 2's, on the same engine, is
    // owner-local. A sticky flag (never taken when the frame is pushed)
    // makes run 2 escape too.
    let constants = [Value::fixnum(42)];
    let ops = [Op::Constant(0), Op::Return];
    let script = vec![Some(completion(&Op::Return, vec![Value::fixnum(7)]))];
    let mut engine = VmExec::new(
        &(),
        RecordingEscape::with_script(script),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    engine.set_entry_exit_flags(ExitFlags::DEBUG_ON_EXIT);
    let outcome = engine.run(&mut context(), program_with(&ops, &constants));
    assert_eq!(
        done_value(&mut engine, outcome),
        Value::fixnum(7),
        "run 1: the debugger's value"
    );
    assert_eq!(
        engine.escape_handler().requests.len(),
        1,
        "run 1: the cleanup escaped"
    );
    let outcome = engine.run(&mut context(), program_with(&ops, &constants));
    assert_eq!(
        done_value(&mut engine, outcome),
        Value::fixnum(42),
        "run 2: the operand, owner-local"
    );
    assert_eq!(
        engine.escape_handler().requests.len(),
        1,
        "run 2: no escape, the flag was taken"
    );
    assert_clear(&engine);
}

#[test]
fn vm_owner_local_return_truncates_records_and_pops_the_frame() {
    // V4, the owner-local unwind (design §5): a frame that still owns an
    // owner binding record when it returns - malformed code, since the
    // compiler emits the Unbind - has it undone innermost-first through the
    // unbind-time re-validation on the way out, the frame is popped, and the
    // run ends with the top operand. GNU's `Breturn` assumes the dynamic
    // state is gone; the owner truncation is the leak guard. Before V4 the
    // Return escape left the record live on the slab.
    let mut ctx = context();
    let x = plain_sym("pvm-return-truncates");
    let (xs, one, two) = (Value::from_sym_id(x), Value::fixnum(1), Value::fixnum(2));
    let ops = plain_program(vec![
        Op::Constant(1),
        Op::VarBind(0),
        Op::Constant(2),
        Op::Return,
    ]);
    let log = EffectLog::default();
    let plain = HarnessPlain::new().admit(x).logging(log.clone());
    let mut engine = harness_with_plain(RecordingEscape::logging(log.clone()), plain);
    let outcome = engine.run(&mut ctx, program_with(&ops, &vec![xs, one, two]));
    assert_eq!(done_value(&mut engine, outcome), two);
    assert_eq!(
        log.entries(),
        vec!["slot <- 1", "slot <- none"],
        "the bind, then the unwind's restore; no escape"
    );
    assert_eq!(engine.escape_handler().requests.len(), 0);
    assert!(
        engine.plain_handler().live_records().is_empty(),
        "no record survives the frame"
    );
    assert!(engine.frame_id().is_none(), "the frame is popped");
    assert_clear(&engine);
}

#[test]
fn vm_a_return_under_a_watched_owner_record_takes_the_serial_cleanup() {
    // V4, the fourth trigger: a frame that still owns an owner binding the
    // unbind-time re-validation refuses (its symbol gained a watcher inside
    // the let) must undo it through the serial side on return, as GNU's
    // `unbind_to` notifies the watcher: the serial cleanup escape. The
    // double stands for the slow arm - it takes and restores the frame's
    // records - and the post-escape unwind finds the record gone and drops
    // the entry instead of restoring twice.
    let mut ctx = context();
    let x = plain_sym("pvm-watched-return");
    let (xs, one, two) = (Value::from_sym_id(x), Value::fixnum(1), Value::fixnum(2));
    let ops = plain_program(vec![
        Op::Constant(1),
        Op::VarBind(0),
        Op::Constant(2),
        Op::Return,
    ]);
    let log = EffectLog::default();
    let plain = HarnessPlain::new().admit(x).watched(x).logging(log.clone());
    let mut engine = harness_with_plain(
        RecordingEscape::logging_with_script(
            vec![Some(completion(&Op::Return, vec![two]))],
            log.clone(),
        ),
        plain,
    );
    let outcome = engine.run(&mut ctx, program_with(&ops, &vec![xs, one, two]));
    assert_eq!(done_value(&mut engine, outcome), two);
    assert_eq!(
        log.entries(),
        vec!["slot <- 1", "escape Return", "slot <- none"],
        "the serial cleanup took and restored the watched record; the owner restored nothing"
    );
    let escape = engine.escape_handler();
    assert_eq!(escape.requests.len(), 1);
    assert_eq!(escape.requests[0].op, Op::Return);
    assert!(matches!(
        escape.requests[0].why,
        EscapeWhy::World(WorldSplit::Return(ReturnDisposition::NeedsSerialCleanup))
    ));
    assert!(
        engine.plain_handler().live_records().is_empty(),
        "no record survives; no double restore"
    );
    assert!(lifos(engine.arena()).0.is_empty());
    assert_clear(&engine);
}

/// A V5 engine: the arith port over `budget` float results, the poll double
/// minting `grants` into it at each refill edge (one past the list).
fn refill_harness(
    budget: &Rc<Cell<usize>>,
    grants: Vec<usize>,
) -> VmExec<'static, RecordingEscape, HarnessVmSealed, HarnessArith, HarnessPlain, RecordingPoll> {
    VmExec::new(
        &(),
        RecordingEscape::new(),
        HarnessVmSealed,
        HarnessArith::with_budget(budget),
        HarnessPlain::new(),
        RecordingPoll::new().refilling(budget.clone(), grants),
    )
}

/// The float loop: acc = 1.5 under a list of four nils; each iteration
/// cdrs the list, adds 1.5 to the acc through StackRef/StackSet, and jumps
/// back while the rest is a cons (`GotoIfNotNil`, bytecode.c's Bgoto-if-
/// not-nil). Four adds, three taken back edges; the final StackRef puts acc
/// on top for the Return. `StackRef n` reads `1 + n` from the top and
/// `StackSet n` replaces the slot `n` below the popped top (the serial arms).
fn float_loop() -> Vec<Op> {
    vec![
        Op::Constant(0),
        Op::Constant(1),
        Op::StackRef(0),
        Op::Cdr,
        Op::StackSet(1),
        Op::StackRef(1),
        Op::Constant(0),
        Op::Add,
        Op::StackSet(2),
        Op::StackRef(0),
        Op::GotoIfNotNil(2),
        Op::StackRef(1),
        Op::Return,
    ]
}

/// The loop's constants: 1.5 and a list of four nils. A `Context` first:
/// a cons and a float allocate on the thread's heap.
fn float_loop_constants(ctx: &mut Context) -> Vec<Value> {
    let _ = ctx;
    let list = Value::cons(
        Value::NIL,
        Value::cons(
            Value::NIL,
            Value::cons(Value::NIL, Value::cons(Value::NIL, Value::NIL)),
        ),
    );
    vec![Value::make_float(1.5), list]
}

#[test]
fn vm_an_exhausted_float_result_refills_at_the_boundary_and_the_same_op_re_executes() {
    // V5 (peer #12095 C2, #12100): the port's reserve is dry, so arith()
    // refuses with Exhausted AFTER its operand checks and before anything is
    // touched. The step leaves the op un-executed at its instruction
    // boundary - pc, operands and frame as they were - and the driver takes
    // ONE refill edge, the publish/stop/restore body's stand-in here, which
    // grants. Then the SAME op re-executes from its start with the same
    // operands, and no escape was taken on the way.
    let mut ctx = context();
    let budget = Rc::new(Cell::new(0));
    let ops = plain_program(vec![Op::Constant(0), Op::Constant(1), Op::Add]);
    let constants = [Value::make_float(1.5), Value::make_float(2.5)];
    let mut engine = refill_harness(&budget, vec![1]);
    let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
    let done = done_value(&mut engine, outcome);
    assert_eq!(done.xfloat(), 4.0);
    let poll = engine.poll_handler();
    assert_eq!(
        poll.edges,
        vec![PollEdge::VmRefill],
        "one refill edge, nothing else"
    );
    assert_eq!(poll.refill_classes, vec![NumericClass::Float]);
    assert_eq!(
        poll.segments[0],
        vec![constants[0], constants[1]],
        "the operands are intact at the refill: nothing was consumed"
    );
    let calls = &engine.arith_handler().calls;
    assert_eq!(calls.len(), 2, "the op ran from its start once per attempt");
    assert_eq!(calls[0], calls[1], "with the same kind and operands");
    assert_eq!(engine.escape_handler().requests.len(), 0, "no escape");
    assert_eq!(budget.get(), 0, "the grant was spent by the retried op");
    assert_clear(&engine);
}

#[test]
fn vm_a_refused_refill_poll_faults_and_the_op_never_executes() {
    // The refill edge is a poll: a refusal is the poll fault, and the op
    // that asked for the refill does not run against a dry reserve.
    let mut ctx = context();
    let budget = Rc::new(Cell::new(0));
    let ops = plain_program(vec![Op::Constant(0), Op::Constant(1), Op::Add]);
    let constants = [Value::make_float(1.5), Value::make_float(2.5)];
    let mut engine = VmExec::new(
        &(),
        RecordingEscape::new(),
        HarnessVmSealed,
        HarnessArith::with_budget(&budget),
        HarnessPlain::new(),
        RecordingPoll::refusing_on(PollEdge::VmRefill, RendezvousError::Publication),
    );
    let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
    assert!(
        matches!(
            outcome,
            VmOutcome::Fault(EngineFault::Poll(RendezvousError::Publication))
        ),
        "{outcome:?}"
    );
    assert_eq!(
        engine.arith_handler().calls.len(),
        1,
        "the refused attempt alone"
    );
    assert!(
        engine.arith_handler().float_slots.is_empty(),
        "nothing was allocated"
    );
    assert_eq!(engine.escape_handler().requests.len(), 0);
    assert_clear(&engine);
}

#[test]
fn vm_a_float_loop_with_reserve_never_refills_and_polls_every_back_edge() {
    // The tail-reuse twin (my #12246): once the owner holds a reserve, a
    // float loop's allocations are claims against it - the driver sees no
    // refill edge at all, and polls only at the taken back edges.
    let mut ctx = context();
    let budget = Rc::new(Cell::new(4));
    let ops = float_loop();
    let constants = float_loop_constants(&mut ctx);
    let mut engine = refill_harness(&budget, vec![1]);
    let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
    let done = done_value(&mut engine, outcome);
    assert_eq!(done.xfloat(), 1.5 + 4.0 * 1.5);
    assert_eq!(engine.poll_handler().edges, vec![PollEdge::VmBackEdge; 3]);
    assert_eq!(engine.arith_handler().calls.len(), 4, "each add ran once");
    assert_eq!(budget.get(), 0, "four claims, no refill");
    assert_eq!(
        engine.escape_handler().requests.len(),
        0,
        "the cdrs ran owner-locally"
    );
    assert_clear(&engine);
}

#[test]
fn vm_exhausted_comes_only_after_the_operand_checks() {
    // The port decides a marker or a wrong type before it looks at its
    // reserve (peer #12095 C2): a dry reserve never turns a marker operand
    // into a refill, nor a wrong type into one.
    let mut ctx = context();
    let budget = Rc::new(Cell::new(0));
    let ops = plain_program(vec![Op::Constant(0), Op::Constant(1), Op::Add]);
    // A marker: the ONE escape, as ever.
    let marker = make_marker_value(None, None, false);
    let mut engine = refill_harness(&budget, vec![1]);
    let outcome = engine.run(
        &mut ctx,
        program_with(&ops, &[marker, Value::make_float(2.5)]),
    );
    done_value(&mut engine, outcome);
    assert_eq!(
        engine.escape_handler().requests.len(),
        1,
        "the marker escape"
    );
    assert!(matches!(
        engine.escape_handler().requests[0].why,
        EscapeWhy::World(WorldSplit::Arith(ArithOperands::MarkerInvolved))
    ));
    assert_eq!(
        engine.poll_handler().edges,
        vec![PollEdge::SerialRequest],
        "no refill edge"
    );
    // A wrong type: the builtin's Flow, a signal, no refill.
    let mut engine = refill_harness(&budget, vec![1]);
    let outcome = engine.run(
        &mut ctx,
        program_with(&ops, &[Value::NIL, Value::make_float(2.5)]),
    );
    assert!(matches!(outcome, VmOutcome::Signaled(_)), "{outcome:?}");
    assert!(engine.poll_handler().edges.is_empty(), "no poll at all");
    assert_clear(&engine);
}

#[test]
fn vm_a_refill_that_mints_nothing_polls_again_until_granted() {
    // The driver does not assume the refill made progress (#12162, #12221):
    // a grant of 0 leaves the reserve dry, the op refuses again, and the
    // edge polls once more - pc and operands unchanged at every attempt -
    // until a grant lands. Here two grants mint nothing on the way.
    let mut ctx = context();
    let budget = Rc::new(Cell::new(0));
    let ops = plain_program(vec![Op::Constant(0), Op::Constant(1), Op::Add]);
    let constants = [Value::make_float(1.5), Value::make_float(2.5)];
    let mut engine = refill_harness(&budget, vec![0, 0, 1]);
    let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
    let done = done_value(&mut engine, outcome);
    assert_eq!(done.xfloat(), 4.0);
    assert_eq!(engine.poll_handler().edges, vec![PollEdge::VmRefill; 3]);
    let calls = &engine.arith_handler().calls;
    assert_eq!(calls.len(), 4, "three refusals, then the run");
    assert!(
        calls.windows(2).all(|pair| pair[0] == pair[1]),
        "the same operands every time"
    );
    let segments = &engine.poll_handler().segments;
    assert!(
        segments.windows(2).all(|pair| pair[0] == pair[1]),
        "the segment never moved"
    );
    assert_clear(&engine);
}

#[test]
fn vm_a_float_loop_over_a_short_reserve_refills_mid_loop_and_keeps_its_accumulator() {
    // The refill lands between two iterations of a loop: the back edges
    // and the refill edges interleave, the accumulator below the operands
    // is untouched by the refill, and the loop's result is exact.
    let mut ctx = context();
    let budget = Rc::new(Cell::new(2));
    let ops = float_loop();
    let constants = float_loop_constants(&mut ctx);
    let mut engine = refill_harness(&budget, vec![1, 1]);
    let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
    let done = done_value(&mut engine, outcome);
    assert_eq!(done.xfloat(), 1.5 + 4.0 * 1.5);
    assert_eq!(
        engine.poll_handler().edges,
        vec![
            PollEdge::VmBackEdge,
            PollEdge::VmBackEdge,
            PollEdge::VmRefill,
            PollEdge::VmBackEdge,
            PollEdge::VmRefill,
        ],
        "two refills, each after a back edge and before the next iteration's add"
    );
    assert_eq!(
        engine.arith_handler().calls.len(),
        6,
        "four ran, two refused first"
    );
    assert_clear(&engine);
}

#[test]
fn vm_owner_local_return_of_nested_lets_restores_innermost_first() {
    // F7: two records leave innermost first. The inner record saves the
    // outer value (Previous 1) and the outer saves nothing (Inherited), so
    // the restores run slot <- 1, then slot <- none, and x ends at its
    // canonical value. An outermost-first walk faults through the slab-top
    // pairing with two or more records; one-record rows never reach it.
    let mut ctx = context();
    let x = plain_sym("pvm-nested-return");
    let (xs, one, two, three, ten) = (
        Value::from_sym_id(x),
        Value::fixnum(1),
        Value::fixnum(2),
        Value::fixnum(3),
        Value::fixnum(10),
    );
    let ops = plain_program(vec![
        Op::Constant(1),
        Op::VarBind(0),
        Op::Constant(2),
        Op::VarBind(0),
        Op::Constant(3),
        Op::Return,
    ]);
    let log = EffectLog::default();
    let plain = HarnessPlain::new()
        .admit(x)
        .with_canonical(x, ten)
        .logging(log.clone());
    let mut engine = harness_with_plain(RecordingEscape::logging(log.clone()), plain);
    let outcome = engine.run(&mut ctx, program_with(&ops, &vec![xs, one, two, three]));
    assert_eq!(done_value(&mut engine, outcome), three);
    assert_eq!(
        log.entries(),
        vec!["slot <- 1", "slot <- 2", "slot <- 1", "slot <- none"],
        "innermost first: the inner restore gives the outer value back, the outer clears"
    );
    assert_eq!(engine.escape_handler().requests.len(), 0);
    assert!(engine.plain_handler().live_records().is_empty());
    assert_eq!(
        engine.plain_handler().observed(x),
        "10",
        "x is back at its canonical value"
    );
    assert_clear(&engine);
}

#[test]
fn vm_a_return_cleanup_that_leaves_a_serial_handler_marker_faults() {
    // C2 (lead #13715): the cleanup's completion popped no handler, so the
    // R1d marker is still above handler_base when the escape completes. An
    // owner truncation never touches a serial handler (R1d): the contract
    // fault on the handler stack, before anything is truncated, the scene
    // kept - the twin of a leftover record marker in the unwind.
    let mut ctx = context();
    let ops = plain_program(vec![Op::Constant(0), Op::PushConditionCase(4), Op::Return]);
    let mut engine = harness_with(RecordingEscape::with_script(vec![
        Some(EscapeCompletion::Completed {
            consumed: 0,
            produced: Vec::new(),
            jump: None,
            records: RecordDelta::default(),
            handlers: HandlerDelta {
                popped: Vec::new(),
                pushed: Some(handler(10)),
            },
        }),
        // The cleanup leaves the handler: pops nothing (it carries the value,
        // so the fault is the handler's, not a missing result's).
        Some(completion(&Op::Return, vec![Value::fixnum(1)])),
    ]));
    let outcome = engine.run(&mut ctx, program_with(&ops, &[Value::fixnum(1)]));
    assert!(
        matches!(
            outcome,
            VmOutcome::Fault(EngineFault::EscapeRecordContract(RecordStack::Handlers))
        ),
        "{outcome:?}"
    );
    assert!(!engine.is_idle(), "the scene is kept");
    assert_eq!(engine.frame_id().is_some(), true, "the frame stays");
    assert_eq!(
        lifos(engine.arena()).1,
        vec![Some(10)],
        "the handler marker is still there: not truncated away"
    );
}

#[test]
fn vm_idle_after_a_signal_inside_a_watched_let_poisons() {
    // C3 (lead #13715), N-idle-undo-needs-serial: a signal inside a `let`
    // whose symbol gained a watcher. The idle unwind reaches the record and
    // the unbind-time re-validation refuses it - the watcher's notification
    // needs the serial side, which the idle path cannot reach. The run
    // poisons: the record stays live, the slot keeps the inner value, and
    // the next run is refused. What stays is the binding LIFO, the slab and
    // the roots; the frame and its handlers are already gone. V8 runs the
    // unwind with Lisp and closes this.
    let mut ctx = context();
    let x = plain_sym("pvm-idle-watched");
    let (xs, one) = (Value::from_sym_id(x), Value::fixnum(1));
    let ops = plain_program(vec![
        Op::Constant(1),
        Op::VarBind(0),
        Op::SaveExcursion,
        Op::Nil,
    ]);
    let log = EffectLog::default();
    let plain = HarnessPlain::new().admit(x).watched(x).logging(log.clone());
    let mut engine = harness_with_plain(
        RecordingEscape::logging_with_script(
            vec![Some(EscapeCompletion::Flow(signal(
                "error",
                vec![Value::string("boom")],
            )))],
            log.clone(),
        ),
        plain,
    );
    let outcome = engine.run(&mut ctx, program_with(&ops, &vec![xs, one]));
    assert!(
        matches!(
            outcome,
            VmOutcome::Fault(EngineFault::EscapeRecordContract(RecordStack::Bindings))
        ),
        "{outcome:?}"
    );
    assert_eq!(
        log.entries(),
        vec!["slot <- 1", "escape SaveExcursion"],
        "no owner restore: the watcher must be told first"
    );
    assert_eq!(
        engine.plain_handler().live_records().len(),
        1,
        "the record stays live"
    );
    assert_eq!(
        engine.plain_handler().observed(x),
        "1",
        "the inner value stays in the slot"
    );
    assert_eq!(
        lifos(engine.arena()).0,
        vec![None],
        "the binding entry stays"
    );
    assert!(engine.frame_id().is_none(), "the frame is already gone");
    assert!(
        matches!(
            engine.run(&mut ctx, program(&[Op::Nil, Op::Return])),
            VmOutcome::Fault(EngineFault::Poisoned)
        ),
        "the next run is refused"
    );
}

#[test]
fn vm_a_return_over_a_record_the_exchange_took_faults_before_the_cleanup() {
    // F5 (lead #13715): before the Return escape, every owner entry names a
    // live record. Here a SaveExcursion's serial side took x's record under
    // the stepper (the `taking_the_record` double); the Return under
    // DEBUG_ON_EXIT would otherwise drop the entry silently after the
    // cleanup, since `!record_live` reads as "the cleanup took it". The
    // fault comes before the escape, where the two are still told apart.
    let mut ctx = context();
    let x = plain_sym("pvm-return-over-taken");
    let (xs, one, two) = (Value::from_sym_id(x), Value::fixnum(1), Value::fixnum(2));
    let ops = plain_program(vec![
        Op::Constant(1),
        Op::VarBind(0),
        Op::SaveExcursion,
        Op::Constant(2),
        Op::Return,
    ]);
    let log = EffectLog::default();
    let plain = HarnessPlain::new().admit(x).logging(log.clone());
    let mut engine = harness_with_plain(
        RecordingEscape::logging_with_script(
            vec![
                Some(EscapeCompletion::fall_through(&Op::SaveExcursion)),
                Some(completion(&Op::Return, vec![two])),
            ],
            log.clone(),
        )
        .taking_the_record(),
        plain,
    );
    engine.set_entry_exit_flags(ExitFlags::DEBUG_ON_EXIT);
    let outcome = engine.run(&mut ctx, program_with(&ops, &vec![xs, one, two]));
    assert!(
        matches!(
            outcome,
            VmOutcome::Fault(EngineFault::EscapeRecordContract(RecordStack::Bindings))
        ),
        "{outcome:?}"
    );
    assert_eq!(
        engine.escape_handler().requests.len(),
        1,
        "the fault came before the Return's escape"
    );
    assert!(!engine.is_idle(), "the scene is kept");
    assert_eq!(engine.frame_id().is_some(), true, "the frame stays");
}

#[test]
fn vm_a_return_under_a_serial_record_marker_takes_the_serial_cleanup() {
    // V4, R1c: a frame that still owns a serial record (a save-excursion
    // the compiler did not pop) must run it on return, through the serial
    // slow arm: ONE escape with op Return, the completion's deltas name the
    // marker, and no owner truncation happens before it (the marker is on
    // the LIFO at the escape). The value is the top operand.
    let mut ctx = context();
    let ops = plain_program(vec![Op::Constant(0), Op::SaveExcursion, Op::Return]);
    let mut engine = harness_with(RecordingEscape::with_script(vec![
        Some(completion_pushing(&Op::SaveExcursion, token(9))),
        Some(EscapeCompletion::Completed {
            consumed: 1,
            produced: vec![Value::fixnum(1)],
            jump: None,
            records: RecordDelta {
                popped: vec![token(9)],
                pushed: Vec::new(),
            },
            handlers: HandlerDelta::default(),
        }),
    ]));
    let outcome = engine.run(&mut ctx, program_with(&ops, &[Value::fixnum(1)]));
    assert_eq!(done_value(&mut engine, outcome), Value::fixnum(1));
    let escape = engine.escape_handler();
    assert_eq!(escape.requests.len(), 2, "the save, then the Return");
    assert_eq!(escape.requests[1].op, Op::Return);
    assert_eq!(
        escape.lifos[1].0,
        vec![Some(9)],
        "the serial marker is still there when the Return escapes: no owner truncation first"
    );
    assert!(
        lifos(engine.arena()).0.is_empty(),
        "the completion's delta removed it"
    );
    assert_clear(&engine);
}

#[test]
fn vm_a_return_under_a_serial_handler_marker_takes_the_serial_cleanup() {
    // V4, R1d: the same for a serial handler the frame still owns (a
    // condition-case the compiler did not pop): the handler marker above
    // handler_base sends the Return through the serial cleanup, and the
    // completion's delta removes the handler.
    let mut ctx = context();
    let ops = plain_program(vec![Op::Constant(0), Op::PushConditionCase(4), Op::Return]);
    let mut engine = harness_with(RecordingEscape::with_script(vec![
        Some(EscapeCompletion::Completed {
            consumed: 0,
            produced: Vec::new(),
            jump: None,
            records: RecordDelta::default(),
            handlers: HandlerDelta {
                popped: Vec::new(),
                pushed: Some(handler(10)),
            },
        }),
        Some(EscapeCompletion::Completed {
            consumed: 1,
            produced: vec![Value::fixnum(1)],
            jump: None,
            records: RecordDelta::default(),
            handlers: HandlerDelta {
                popped: vec![handler(10)],
                pushed: None,
            },
        }),
    ]));
    let outcome = engine.run(&mut ctx, program_with(&ops, &[Value::fixnum(1)]));
    assert_eq!(done_value(&mut engine, outcome), Value::fixnum(1));
    let escape = engine.escape_handler();
    assert_eq!(escape.requests.len(), 2, "the push, then the Return");
    assert_eq!(escape.requests[1].op, Op::Return);
    assert_eq!(
        escape.lifos[1].1,
        vec![Some(10)],
        "the handler marker is still there when the Return escapes"
    );
    assert!(
        lifos(engine.arena()).1.is_empty(),
        "the completion's delta removed it"
    );
    assert_clear(&engine);
}

#[test]
fn vm_a_return_with_stale_keys_still_escapes_once() {
    // V4: a stale cached Plain key (the era moved under a surviving key)
    // costs nothing on a Return. Under a serial marker the Return is ONE
    // escape - the serial cleanup - never one per stale key.
    let mut ctx = context();
    let x = plain_sym("pvm-stale-return");
    let (xs, one) = (Value::from_sym_id(x), Value::fixnum(1));
    let plain = HarnessPlain::new().admit(x).with_canonical(x, one);
    // One script for both runs, consumed by escape index: run 1 escapes
    // nothing (the Return is owner-local), so run 2 takes both entries.
    let mut engine = harness_with_plain(
        RecordingEscape::with_script(vec![
            Some(completion_pushing(&Op::SaveExcursion, token(9))),
            Some(EscapeCompletion::Completed {
                consumed: 1,
                produced: vec![one],
                jump: None,
                records: RecordDelta {
                    popped: vec![token(9)],
                    pushed: Vec::new(),
                },
                handlers: HandlerDelta::default(),
            }),
        ]),
        plain,
    );
    // Run 1 mints the key.
    let read = plain_program(vec![Op::VarRef(0)]);
    let outcome = engine.run(&mut ctx, program_with(&read, &vec![xs]));
    done_value(&mut engine, outcome);
    assert_clear(&engine);
    // The era moves: the key is stale.
    engine.plain_handler_mut().next_era();
    // Run 2 returns under a serial marker.
    let ops = plain_program(vec![Op::Constant(0), Op::SaveExcursion, Op::Return]);
    let outcome = engine.run(&mut ctx, program_with(&ops, &vec![one]));
    assert_eq!(done_value(&mut engine, outcome), one);
    let escape = engine.escape_handler();
    assert_eq!(escape.requests.len(), 2, "the save, then ONE Return escape");
    assert_eq!(escape.requests[1].op, Op::Return);
    assert_clear(&engine);
}

#[test]
fn vm_idle_after_a_signal_inside_let_leaves_no_owner_binding() {
    // N-v2-idle-slab-undo (peer #11957): a signal inside a `let` settles the
    // run, and the idle unwind undoes the frame's binding - the record is
    // gone from the slab and the slot restored - instead of dropping the
    // executor's entry and leaking the record with its rooted value. GNU's
    // `unbind_to` undoes each record as it leaves (eval.c:3940-3954).
    let mut ctx = context();
    let x = plain_sym("pvm-idle-undo");
    let (xs, one) = (Value::from_sym_id(x), Value::fixnum(1));
    let ops = plain_program(vec![
        Op::Constant(1),
        Op::VarBind(0),
        Op::SaveExcursion,
        Op::Nil,
    ]);
    let log = EffectLog::default();
    let plain = HarnessPlain::new().admit(x).logging(log.clone());
    let mut engine = harness_with_plain(
        RecordingEscape::logging_with_script(
            vec![Some(EscapeCompletion::Flow(signal(
                "error",
                vec![Value::string("boom")],
            )))],
            log.clone(),
        ),
        plain,
    );
    let outcome = engine.run(&mut ctx, program_with(&ops, &vec![xs, one]));
    assert!(matches!(outcome, VmOutcome::Signaled(_)), "{outcome:?}");
    assert!(engine.is_idle(), "the run settled to Idle");
    assert_eq!(
        log.entries(),
        vec!["slot <- 1", "escape SaveExcursion", "slot <- none"],
        "the bind, the signal, then the idle unwind's restore"
    );
    assert!(
        engine.plain_handler().live_records().is_empty(),
        "no owner binding survives the signal"
    );
    assert!(lifos(engine.arena()).0.is_empty());
}

#[test]
fn vm_unbind_after_a_make_local_inside_let_takes_the_serial_unbind() {
    // C4's redirect half (peer #12980, lead #12991): GNU's `do_one_unbind`
    // for a LET re-reads the redirect too (eval.c:3848-3870); a variable
    // made local or aliased inside the let goes through
    // `set_default_internal`, the serial path. Modelled like the watcher:
    // the harness's `redirected` set is the serial side's make-local-variable,
    // run from inside the let.
    let mut ctx = context();
    let x = plain_sym("pvm-redirected");
    let (xs, one) = (Value::from_sym_id(x), Value::fixnum(1));
    let ops = plain_program(vec![
        Op::Constant(1),
        Op::VarBind(0),
        Op::SaveExcursion,
        Op::Unbind(2),
        Op::Nil,
    ]);
    let log = EffectLog::default();
    let plain = HarnessPlain::new()
        .admit(x)
        .redirected(x)
        .logging(log.clone());
    let mut engine = harness_with_plain(
        RecordingEscape::logging_with_script(
            vec![
                Some(completion_pushing(&Op::SaveExcursion, token(9))),
                Some(completion_popping(&Op::Unbind(2), &[token(9)])),
                Some(EscapeCompletion::fall_through(&Op::Unbind(2))),
                None,
            ],
            log.clone(),
        ),
        plain,
    );
    let outcome = engine.run(&mut ctx, program_with(&ops, &vec![xs, one]));
    assert_eq!(done_value(&mut engine, outcome), Value::NIL);
    assert_eq!(
        log.entries(),
        vec![
            "slot <- 1",
            "escape SaveExcursion",
            "escape Unbind(2) [9]",
            "escape Unbind(2) [plain]",
            "slot <- none",
        ],
        "the serial run leaves first; the redirected binding is handed to the serial unbind; the owner never restores it; Return is owner-local"
    );
    let escape = engine.escape_handler();
    assert!(matches!(
        escape.requests[2].why,
        EscapeWhy::PlainUnbind { symbol, .. } if symbol == x
    ));
    assert!(engine.plain_handler().live_records().is_empty());
    assert_clear(&engine);
}

#[test]
fn vm_run_refuses_a_second_run_until_the_done_root_is_taken() {
    let mut engine = harness();
    let outcome = engine.run(&mut context(), program(&[Op::Nil, Op::Return]));
    let pending = match outcome {
        VmOutcome::Done { pending } => pending,
        other => panic!("expected Done, got {other:?}"),
    };
    // The frame is gone; the done root keeps the engine off Idle.
    assert_eq!(engine.arena_depth(), 0);
    assert_eq!(engine.live_root_count(), 1);
    assert!(!engine.is_idle());
    let outcome = engine.run(&mut context(), program(&[Op::Nil, Op::Return]));
    assert!(matches!(outcome, VmOutcome::Fault(EngineFault::Busy)));
    assert_eq!(engine.live_root_count(), 1);
    assert_eq!(engine.take(pending), Ok(Value::NIL));
    assert!(engine.is_idle());
    assert_clear(&engine);
}

#[test]
fn vm_sealed_cons_opcodes_run_owner_locally_through_the_port() {
    // One runtime for every case: the cons the port mutates is the one the
    // builtins read.
    let mut ctx = context();
    let cons = Value::cons(Value::fixnum(1), Value::fixnum(2));
    let constants = [Value::fixnum(5), cons];
    // (name, program without its Return, its result: the top operand the
    // owner-local Return takes since V4)
    let cases: Vec<(&str, Vec<Op>, Value)> = vec![
        (
            "cons+car",
            vec![Op::Nil, Op::True, Op::Cons, Op::Car],
            Value::NIL,
        ),
        (
            "cons+cdr",
            vec![Op::Nil, Op::True, Op::Cons, Op::Cdr],
            Value::T,
        ),
        ("car-safe", vec![Op::Constant(0), Op::CarSafe], Value::NIL),
        ("cdr-safe", vec![Op::Constant(0), Op::CdrSafe], Value::NIL),
        (
            "list",
            vec![Op::True, Op::Nil, Op::List(2), Op::Car],
            Value::T,
        ),
        ("list-0", vec![Op::List(0), Op::Null], Value::T),
        (
            "setcar",
            vec![Op::Constant(1), Op::True, Op::Setcar],
            Value::T,
        ),
    ];
    for (name, mut ops, result) in cases {
        ops.push(Op::Return);
        let mut engine = harness();
        let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
        assert_eq!(
            done_value(&mut engine, outcome),
            result,
            "{name}: the result"
        );
        let escape = engine.escape_handler();
        assert_eq!(
            escape.requests.len(),
            0,
            "{name}: escapes; Return is owner-local (V4)"
        );
        assert_eq!(
            engine.poll_handler().edges,
            Vec::<PollEdge>::new(),
            "{name}: edges"
        );
        assert_clear(&engine);
    }
    // setcar wrote through to the harness's cons.
    assert_eq!(cons.cons_car(), Value::T);
    assert_eq!(cons.cons_cdr(), Value::fixnum(2));
    let mut engine = harness();
    let outcome = engine.run(
        &mut ctx,
        program_with(
            &[Op::Constant(1), Op::Nil, Op::Setcdr, Op::Return],
            &constants,
        ),
    );
    assert_eq!(done_value(&mut engine, outcome), Value::NIL);
    assert_eq!(cons.cons_cdr(), Value::NIL);
    assert_clear(&engine);
}

#[test]
fn vm_a_sealed_core_signal_settles_to_idle() {
    let mut engine = harness();
    let outcome = engine.run(
        &mut context(),
        program_with(&[Op::Constant(0), Op::Car, Op::Return], &[Value::fixnum(5)]),
    );
    let flow = match outcome {
        VmOutcome::Signaled(flow) => flow,
        other => panic!("expected a signal, got {other:?}"),
    };
    assert_eq!(signal_key(&flow).0, "wrong-type-argument");
    assert_clear(&engine);
}

#[test]
fn vm_a_sealed_port_without_a_core_is_missing_core() {
    let mut engine = VmExec::new(
        &(),
        RecordingEscape::new(),
        MissingSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    let outcome = engine.run(&mut context(), program(&[Op::Nil, Op::Car, Op::Return]));
    assert!(matches!(outcome, VmOutcome::MissingCore { name: "car" }));
    assert!(engine.escape_handler().requests.is_empty());
    assert_eq!(engine.poll_handler().edges, Vec::<PollEdge>::new());
    assert_clear(&engine);
}

#[test]
fn vm_malformed_scalar_programs_signal_invalid_byte_code_instead_of_panicking() {
    let cases: Vec<(&str, Vec<Op>)> = vec![
        ("pop", vec![Op::Pop]),
        ("dup", vec![Op::Dup]),
        ("stack-ref", vec![Op::StackRef(0)]),
        ("stack-ref-1", vec![Op::Nil, Op::StackRef(1)]),
        ("stack-set-1", vec![Op::Nil, Op::StackSet(1)]),
        ("stack-set-0", vec![Op::StackSet(0)]),
        ("discard-n", vec![Op::DiscardN(1)]),
        ("discard-n-preserve", vec![Op::Nil, Op::DiscardN(0x81)]),
        ("trap", vec![Op::TrapOutOfRangeConstant(0)]),
        ("constant", vec![Op::Constant(5)]),
        ("goto", vec![Op::Goto(9)]),
        ("goto-if-nil", vec![Op::Nil, Op::GotoIfNil(9)]),
        ("else-pop", vec![Op::Nil, Op::GotoIfNilElsePop(9)]),
        ("goto-if-nil empty", vec![Op::GotoIfNil(0)]),
        ("consp empty", vec![Op::Consp]),
        ("no-return", vec![Op::Nil]),
        ("car", vec![Op::Car]),
        ("cons", vec![Op::Nil, Op::Cons]),
    ];
    let mut ctx = context();
    for (name, ops) in cases {
        let mut engine = harness();
        let outcome = engine.run(&mut ctx, program(&ops));
        let flow = match outcome {
            VmOutcome::Signaled(flow) => flow,
            other => panic!("{name}: expected a signal, got {other:?}"),
        };
        assert_eq!(
            signal_key(&flow),
            (
                "error".to_string(),
                vec![Value::string("Invalid byte-code")]
            ),
            "{name}"
        );
        assert_clear(&engine);
        let outcome = engine.run(&mut ctx, program(&[Op::Nil, Op::Return]));
        assert_eq!(
            done_value(&mut engine, outcome),
            Value::NIL,
            "{name}: restart"
        );
    }
}

// ------------------------------------------------- the serial verifier tie

/// How the serial verifier can speak about one opcode.
enum VerifierVoice {
    /// A fall-through effect: `[op, X, Return]` proves `(consumed, produced)`.
    Plain,
    /// A branch whose target is a `Return` off the fall-through path; the
    /// verifier proves its fall-through edge only.
    Branch,
    /// The verifier records no effect: `Return`,
    /// `TrapOutOfRangeConstant`.
    Silent,
    /// A terminal that consumes its operands and has no fall-through.
    /// `Throw` pops the tag and the value, then leaves the function.
    Terminal,
    /// The plain form refuses it (`Switch` needs its tables).
    Refused,
}

fn verifier_voice(op: &Op) -> VerifierVoice {
    match op {
        Op::Goto(_)
        | Op::GotoIfNil(_)
        | Op::GotoIfNotNil(_)
        | Op::GotoIfNilElsePop(_)
        | Op::GotoIfNotNilElsePop(_)
        | Op::PushCatch(_)
        | Op::PushConditionCaseRaw(_)
        | Op::PushConditionCase(_) => VerifierVoice::Branch,
        Op::Return | Op::TrapOutOfRangeConstant(_) => VerifierVoice::Silent,
        Op::Throw => VerifierVoice::Terminal,
        Op::Switch => VerifierVoice::Refused,
        _ => VerifierVoice::Plain,
    }
}

/// `MAX_STACK` admits the deepest probe (`DiscardN(preserve 127)` consumes
/// 128); the entry depths searched stay below it.
const MAX_STACK: usize = 256;
const DEPTH_PROBES: usize = 140;

fn probe(op: &Op, voice: &VerifierVoice, then: Op) -> Vec<Op> {
    let mut ops = vec![op.clone(), then, Op::Return];
    if matches!(voice, VerifierVoice::Branch) {
        // The branch target (3) is a second `Return`, off the fall-through
        // path, so no two edges join.
        ops.push(Op::Return);
    }
    ops
}

/// The least entry depth the verifier admits: the opcode's `(min, …)` row.
fn least_entry_depth(ops: &[Op]) -> Option<usize> {
    (0..=DEPTH_PROBES).find(|depth| verify_stack_effects(ops, *depth, MAX_STACK))
}

/// The `(consumed, produced)` the verifier proves for OP's fall-through edge:
/// `produced` is one past the deepest slot a following `StackRef` can read.
fn verifier_effect(op: &Op) -> Option<(usize, usize)> {
    let voice = verifier_voice(op);
    let consumed = least_entry_depth(&probe(op, &voice, Op::Return))?;
    let mut produced = None;
    for below in (0..64).rev() {
        if verify_stack_effects(&probe(op, &voice, Op::StackRef(below)), consumed, MAX_STACK) {
            produced = Some(below as usize + 1);
            break;
        }
    }
    Some((consumed, produced.unwrap_or(0)))
}

#[test]
fn vm_static_stack_effect_agrees_with_the_serial_verifier() {
    let samples = op_samples()
        .into_iter()
        .chain(extra_payload_shapes())
        .collect::<Vec<_>>();
    let (mut tied, mut silent, mut refused) = (0, 0, 0);
    for (name, op) in &samples {
        let effect = static_stack_effect(op);
        match verifier_voice(op) {
            VerifierVoice::Silent => {
                // The verifier accepts every depth and records nothing. The
                // table carries the documented effect instead: `Breturn`
                // carries `val = TOP` to `TOP = val` (bytecode.c:897-921).
                silent += 1;
                for depth in 0..=8 {
                    assert!(
                        verify_stack_effects(&[op.clone(), Op::Return], depth, MAX_STACK),
                        "{name}: silent at {depth}"
                    );
                }
                let documented = match op {
                    Op::Return => (1, 1),
                    Op::TrapOutOfRangeConstant(_) => (0, 0),
                    _ => unreachable!("every silent opcode is documented"),
                };
                assert_eq!((effect.consumed, effect.produced), documented, "{name}");
            }
            VerifierVoice::Refused => {
                refused += 1;
                for depth in 0..=8 {
                    assert!(
                        !verify_stack_effects(&[op.clone(), Op::Return], depth, MAX_STACK),
                        "{name}: refused at {depth}"
                    );
                }
                // `Bswitch` pops the table and the key (bytecode.c:1754-1757);
                // a hit leaves through `op_branch`, which pushes nothing.
                assert_eq!((effect.consumed, effect.produced), (2, 0), "{name}");
            }
            VerifierVoice::Terminal => {
                // `Throw` ends the flow. The least admitted depth is its
                // operand count, and nothing is produced for a following op.
                // A shallower depth is not a proof: the verified driver would
                // pop the tag and value below the frame.
                tied += 1;
                let consumed = least_entry_depth(&[op.clone(), Op::Return])
                    .expect("a terminal opcode admits some depth");
                assert_eq!(effect.consumed as usize, consumed, "{name}: consumed");
                assert_eq!(effect.produced, 0, "{name}: produced");
                assert!(
                    consumed > 0
                        && !verify_stack_effects(
                            &[op.clone(), Op::Return],
                            consumed - 1,
                            MAX_STACK
                        ),
                    "{name}: depth {} is not a proof",
                    consumed - 1
                );
            }
            VerifierVoice::Plain | VerifierVoice::Branch => {
                let (consumed, produced) = verifier_effect(op).expect("the verifier speaks");
                assert_eq!(effect.consumed as usize, consumed, "{name}: consumed");
                if matches!(op, Op::Goto(_)) {
                    // No fall-through edge: the verifier never reaches pc 1.
                    assert_eq!(effect.produced, 0, "{name}: produced");
                } else {
                    assert_eq!(effect.produced as usize, produced, "{name}: produced");
                }
                tied += 1;
            }
        }
    }
    assert_eq!(op_samples().len(), 89, "one sample per Op variant");
    assert_eq!(samples.len(), 89 + extra_payload_shapes().len());
    assert_eq!((tied, silent, refused), (samples.len() - 3, 2, 1));
}

// ---------------------------------------------------------------- V3: R6

/// The operands as constants, `op`, `Return`.
fn arith_program(op: Op, operands: &[Value]) -> Vec<Op> {
    let mut ops: Vec<Op> = (0..operands.len())
        .map(|index| Op::Constant(index as u16))
        .collect();
    ops.push(op);
    ops.push(Op::Return);
    ops
}

/// The opcode of each kind: `ArithGenericKind::from_op`'s inverse.
fn opcode(kind: ArithGenericKind) -> Op {
    match kind {
        ArithGenericKind::Add => Op::Add,
        ArithGenericKind::Sub => Op::Sub,
        ArithGenericKind::Mul => Op::Mul,
        ArithGenericKind::Div => Op::Div,
        ArithGenericKind::Rem => Op::Rem,
        ArithGenericKind::Max => Op::Max,
        ArithGenericKind::Min => Op::Min,
        ArithGenericKind::NumEq => Op::Eqlsign,
        ArithGenericKind::Lt => Op::Lss,
        ArithGenericKind::Gt => Op::Gtr,
        ArithGenericKind::Le => Op::Leq,
        ArithGenericKind::Ge => Op::Geq,
        ArithGenericKind::Add1 => Op::Add1,
        ArithGenericKind::Sub1 => Op::Sub1,
        ArithGenericKind::Negate => Op::Negate,
    }
}

#[test]
fn vm_an_arith_opcode_with_too_few_operands_signals_invalid_byte_code_before_the_port() {
    // An Arith opcode reached with fewer operands than its static effect
    // consumes is malformed code, the Scalar and Sealed arms' `Invalid
    // byte-code` (lead C3), refused before the port is asked and before any
    // poll. GNU's arms POP (bytecode.c:1331-1343); verified code never
    // underflows. One operand short for every kind, and none for the unary
    // kinds; the port is never called and the stepper restarts.
    let mut ctx = context();
    for kind in ArithGenericKind::iter() {
        let op = opcode(kind);
        let consumed = usize::from(static_stack_effect(&op).consumed);
        assert!(consumed >= 1, "{kind:?}: every arith opcode consumes");
        let operands = vec![Value::fixnum(1); consumed - 1];
        let ops = arith_program(op, &operands);
        let mut engine = arithmetic_harness();
        let outcome = engine.run(&mut ctx, program_with(&ops, &operands));
        let flow = match outcome {
            VmOutcome::Signaled(flow) => flow,
            other => panic!(
                "{kind:?} with {} operand(s): expected a signal, got {other:?}",
                operands.len()
            ),
        };
        assert_eq!(
            signal_key(&flow),
            (
                "error".to_string(),
                vec![Value::string("Invalid byte-code")]
            ),
            "{kind:?} with {} operand(s)",
            operands.len()
        );
        assert!(
            engine.arith_handler().calls.is_empty(),
            "{kind:?}: the port is not asked"
        );
        assert!(
            engine.escape_handler().requests.is_empty(),
            "{kind:?}: no request"
        );
        assert!(engine.poll_handler().edges.is_empty(), "{kind:?}: no poll");
        assert!(engine.is_idle(), "{kind:?}: settled to Idle");
        assert_clear(&engine);
        let outcome = engine.run(&mut ctx, program(&[Op::Nil, Op::Return]));
        assert_eq!(
            done_value(&mut engine, outcome),
            Value::NIL,
            "{kind:?}: restart"
        );
        assert_clear(&engine);
    }
}

#[test]
fn vm_fixnum_add_runs_owner_locally() {
    // GNU `Bplus` (bytecode.c:1331-1343) is straight-line code: the fixnum
    // fast path or `Fplus (2, &TOP)`, no `maybe_quit`, no funcall. R6: two
    // numeric operands run on the owner, with no poll and no escape.
    let mut ctx = context();
    let constants = [Value::fixnum(2), Value::fixnum(3)];
    let mut engine = arithmetic_harness();
    let ops = arith_program(Op::Add, &constants);
    let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
    let done = done_value(&mut engine, outcome);
    let escape = engine.escape_handler();
    assert_eq!(
        escape.requests.len(),
        0,
        "no escape: Return is owner-local (V4)"
    );
    assert_eq!(
        done,
        Value::fixnum(5),
        "the frame returns the sum: the top operand (V4)"
    );
    assert_eq!(
        engine.poll_handler().edges,
        Vec::<PollEdge>::new(),
        "no poll: the add took none, and Return is owner-local (V4)"
    );
    assert_eq!(
        engine.arith_handler().calls,
        vec![(
            ArithGenericKind::Add,
            vec![Value::fixnum(2), Value::fixnum(3)]
        )],
        "the port ran once, on the operands in stack order"
    );
    assert!(
        engine.arith_handler().float_slots.is_empty(),
        "a fixnum result allocates no float slot"
    );
    assert_clear(&engine);
}

#[test]
fn vm_float_result_takes_an_owner_float_slot() {
    // R6: a float result is a fresh object allocated through the owner's
    // float slot, a bignum result a fresh object too, never an operand's.
    // The double counts its float slots.
    let mut ctx = context();
    let operands = [Value::make_float(1.5), Value::make_float(2.5)];
    let mut engine = arithmetic_harness();
    let ops = arith_program(Op::Add, &operands);
    let outcome = engine.run(&mut ctx, program_with(&ops, &operands));
    let sum = done_value(&mut engine, outcome);
    assert_eq!(sum.xfloat(), 4.0);
    assert_ne!(sum.bits(), operands[0].bits());
    assert_ne!(sum.bits(), operands[1].bits());
    assert_eq!(
        engine.arith_handler().float_slots,
        vec![4.0],
        "one owner float slot"
    );
    assert_eq!(
        engine.escape_handler().requests.len(),
        0,
        "no escape: Return is owner-local (V4)"
    );
    assert_clear(&engine);

    let operands = [Value::fixnum(Value::MOST_POSITIVE_FIXNUM), Value::fixnum(1)];
    let mut engine = arithmetic_harness();
    let ops = arith_program(Op::Add, &operands);
    let outcome = engine.run(&mut ctx, program_with(&ops, &operands));
    let sum = done_value(&mut engine, outcome);
    assert!(sum.is_bignum(), "promoted, as GNU make_int does");
    assert_ne!(sum.bits(), operands[0].bits());
    assert!(engine.arith_handler().float_slots.is_empty());
    assert_clear(&engine);
}

#[test]
fn vm_a_marker_operand_escapes_through_the_one_stub() {
    // A marker's number is its buffer position (E4-DESIGN R6): any marker
    // operand is `MarkerInvolved` and takes the ONE escape, on either side
    // of the opcode and under a unary one.
    let mut ctx = context();
    let marker = make_marker_value(None, None, false);
    assert!(marker.is_marker(), "a marker operand");
    let four = Value::fixnum(4);
    let cases: Vec<(Op, Vec<Value>, usize)> = vec![
        (Op::Add, vec![marker, four], 2),
        (Op::Add, vec![four, marker], 2),
        (Op::Add1, vec![marker], 1),
    ];
    for (op, constants, pc) in cases {
        let script = vec![Some(completion(&op, vec![four]))];
        let mut engine = harness_with(RecordingEscape::mirroring_with_script(script));
        let ops = arith_program(op.clone(), &constants);
        let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
        assert_eq!(done_value(&mut engine, outcome), four, "{op:?}");
        let escape = engine.escape_handler();
        assert_eq!(
            escape.requests.len(),
            1,
            "{op:?}: the opcode; Return is owner-local (V4)"
        );
        let request = &escape.requests[0];
        assert_eq!(request.op, op, "{op:?}");
        assert_eq!(
            request.why,
            EscapeWhy::World(WorldSplit::Arith(ArithOperands::MarkerInvolved)),
            "{op:?}: the escape says why"
        );
        assert_eq!(request.pc, Pc::new(pc), "{op:?}");
        assert_eq!(
            escape.segments[0], constants,
            "{op:?}: the operands are published at the escape"
        );
        assert_eq!(
            engine.poll_handler().edges,
            vec![PollEdge::SerialRequest],
            "{op:?}: no call poll, no back edge; Return is owner-local"
        );
        assert_eq!(
            engine.arith_handler().calls.len(),
            1,
            "{op:?}: the port saw the operands once and declined"
        );
        assert_clear(&engine);
    }

    // A wrong type before the marker is the builtin's signal, on the owner:
    // GNU checks each operand where it stands (arith_driver, data.c:3251).
    let constants = [Value::NIL, marker];
    let mut engine = harness();
    let ops = arith_program(Op::Add, &constants);
    let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
    let VmOutcome::Signaled(flow) = outcome else {
        panic!("expected a signal, got {outcome:?}")
    };
    assert_eq!(
        signal_key(&flow).0,
        "wrong-type-argument",
        "no escape for a wrong type ahead of the marker"
    );
    assert!(engine.escape_handler().requests.is_empty());
    assert!(engine.is_idle());
    assert_clear(&engine);
}

#[test]
fn vm_arith_division_by_zero_is_a_flow_not_a_panic() {
    // GNU `Bquo`/`Brem` keep a zero divisor off their fixnum fast paths
    // (bytecode.c:1392, 1405) and `Fquo`/`integer_remainder` signal
    // `(arith-error)` (data.c:3365, 3371, 3394): the serial `/` and `%`
    // flows, here through the owner-local port.
    let mut ctx = context();
    let (one, zero) = (Value::fixnum(1), Value::fixnum(0));
    for (op, serial) in [
        (Op::Div, builtin_div(&[one, zero])),
        (Op::Rem, builtin_percent(&mut ctx, one, zero)),
    ] {
        let mut engine = harness();
        let ops = arith_program(op.clone(), &[one, zero]);
        let outcome = engine.run(&mut ctx, program_with(&ops, &[one, zero]));
        let VmOutcome::Signaled(flow) = outcome else {
            panic!("{op:?}: expected a signal, got {outcome:?}")
        };
        assert_eq!(signal_key(&flow).0, "arith-error", "{op:?}");
        assert_eq!(
            signal_key(&flow),
            signal_key(&serial.expect_err("{op:?}: the serial builtin signals too")),
            "{op:?}: the serial flow"
        );
        assert_eq!(
            engine.escape_handler().requests.len(),
            0,
            "{op:?}: no escape"
        );
        assert_eq!(
            engine.arith_handler().calls.len(),
            1,
            "{op:?}: the port produced the flow"
        );
        assert!(engine.is_idle(), "{op:?}: settled to Idle");
        assert_clear(&engine);
    }
}

#[test]
fn vm_a_nan_compared_with_a_non_number_is_a_wrong_type_argument() {
    // GNU's comparison opcodes go straight to `arithcompare` (bytecode.c:1256,
    // 1267-1309), whose float arms validate the other operand whatever the
    // float is (data.c:2718-2746): `(< +nan nil)` signals, on either side,
    // NaN or not. The double answers that on the owner, with no escape; the
    // serial arm answers the same, so the tie row runs these shapes too.
    let mut ctx = context();
    let nan = Value::make_float(f64::NAN);
    let predicate = Value::symbol("number-or-marker-p");
    for kind in [
        ArithGenericKind::NumEq,
        ArithGenericKind::Lt,
        ArithGenericKind::Gt,
        ArithGenericKind::Le,
        ArithGenericKind::Ge,
    ] {
        for operands in [[nan, Value::NIL], [Value::T, nan]] {
            let op = opcode(kind);
            let mut engine = harness();
            let ops = arith_program(op, &operands);
            let outcome = engine.run(&mut ctx, program_with(&ops, &operands));
            let VmOutcome::Signaled(flow) = outcome else {
                panic!("{kind:?} {operands:?}: expected a signal, got {outcome:?}")
            };
            assert_eq!(
                signal_key(&flow),
                (
                    "wrong-type-argument".to_string(),
                    vec![
                        predicate,
                        if operands[0].is_float() {
                            Value::NIL
                        } else {
                            Value::T
                        }
                    ]
                ),
                "{kind:?} {operands:?}: the non-number, in operand order"
            );
            assert!(engine.escape_handler().requests.is_empty(), "{kind:?}");
            assert_eq!(engine.arith_handler().calls.len(), 1, "{kind:?}");
            assert!(engine.is_idle(), "{kind:?}");
            assert_clear(&engine);
        }
    }
}

/// The serial opcode's answer for `operands`: the full arm, the integer
/// fast path then the static subr, as `call_arith_builtin_from_stack_args`
/// runs it (operands staged on `bc_buf`).
fn serial_arith(ctx: &mut Context, kind: ArithGenericKind, operands: &[Value]) -> EvalResult {
    let args_start = ctx.bc_buf.len();
    ctx.bc_buf.extend_from_slice(operands);
    let result = Vm::call_arith_builtin_on_context(ctx, kind, args_start, operands.len())
        .expect("a static arithmetic subr");
    ctx.bc_buf.truncate(args_start);
    result
}

fn printed(result: &EvalResult) -> String {
    match result {
        Ok(value) => format_eval_result(&Ok(*value)),
        Err(flow) => format!("signal {:?}", signal_key(flow)),
    }
}

/// What the double may answer, stated once: a tie outside this is not run.
fn in_domain(kind: ArithGenericKind, operands: &[Value]) -> bool {
    let bignums = operands.iter().filter(|value| value.is_bignum()).count();
    let floats = operands.iter().filter(|value| value.is_float()).count();
    if bignums > 0 {
        // No Context-free serial builtin over bignums for these kinds, and
        // GNU compares a bignum with a float exactly (`mpz_cmp_d`,
        // data.c:2766, 2825), beyond this double's doubles.
        let bignum_kind = matches!(
            kind,
            ArithGenericKind::Rem
                | ArithGenericKind::Max
                | ArithGenericKind::Min
                | ArithGenericKind::Negate
        );
        if bignum_kind || floats > 0 {
            return false;
        }
    }
    true
}

/// One shape through the stepper and through the serial arm: both printed.
fn tie(ctx: &mut Context, kind: ArithGenericKind, operands: &[Value]) -> (String, String) {
    let op = opcode(kind);
    let mut engine = arithmetic_harness();
    let ops = arith_program(op, operands);
    let outcome = engine.run(ctx, program_with(&ops, operands));
    let got = match outcome {
        VmOutcome::Done { .. } => Ok(done_value(&mut engine, outcome)),
        VmOutcome::Signaled(flow) => Err(flow),
        other => panic!("{kind:?} {operands:?}: {other:?}"),
    };
    assert_clear(&engine);
    let want = serial_arith(ctx, kind, operands);
    // Fresh results are unrooted once taken; keep them over the printing.
    for value in [&got, &want]
        .into_iter()
        .filter_map(|result| result.as_ref().ok())
    {
        ctx.push_specpdl_root(*value);
    }
    (printed(&got), printed(&want))
}

#[test]
fn vm_arithmetic_results_tie_the_serial_builtin() {
    // R6's owner numeric path answers what the serial opcode answers. Every
    // kind over fixnums at both edges, floats, bignums on the kinds that
    // take one, and wrong-type operands.
    let mut ctx = context();
    let (max, min) = (Value::MOST_POSITIVE_FIXNUM, Value::MOST_NEGATIVE_FIXNUM);
    let pool: Vec<Value> = vec![
        Value::fixnum(min),
        Value::fixnum(min + 1),
        Value::fixnum(-3),
        Value::fixnum(-1),
        Value::fixnum(0),
        Value::fixnum(1),
        Value::fixnum(2),
        Value::fixnum(3),
        Value::fixnum(max - 1),
        Value::fixnum(max),
        Value::fixnum(max + 1),
        Value::fixnum(min - 1),
        Value::fixnum(i64::MAX),
        Value::fixnum(i64::MIN),
        // One past 2^53 beside its rounded double: GNU's exact tie-break
        // when a float meets a fixnum (arithcompare, data.c:2731-2746).
        Value::fixnum((1 << 53) + 1),
        Value::fixnum(1 << 53),
        Value::make_float((1_i64 << 53) as f64),
        Value::make_float(-1.5),
        Value::make_float(-0.0),
        Value::make_float(0.0),
        Value::make_float(0.5),
        Value::make_float(2.0),
        Value::make_float(1e300),
        // IEEE: a NaN is unordered under every comparison and a `max`/`min`
        // second operand propagates (data.c:3468-3471); `(/ 1 0.0)` is +inf.
        Value::make_float(f64::INFINITY),
        Value::make_float(f64::NAN),
        Value::NIL,
        Value::T,
        Value::string("s"),
    ];
    for value in &pool {
        ctx.push_specpdl_root(*value);
    }
    let mut tied = 0usize;
    for kind in ArithGenericKind::iter() {
        assert_eq!(ArithGenericKind::from_op(&opcode(kind)), Some(kind));
        for a in &pool {
            let shapes = if kind.arity() == 1 {
                vec![vec![*a]]
            } else {
                pool.iter().map(|b| vec![*a, *b]).collect()
            };
            for operands in shapes {
                if !in_domain(kind, &operands) {
                    continue;
                }
                let (got, want) = tie(&mut ctx, kind, &operands);
                assert_eq!(got, want, "{kind:?} {operands:?}");
                tied += 1;
            }
        }
    }
    assert!(tied > 3000, "shapes tied: {tied}");
}

// ------------------------------------------------------- V2: Plain variables

/// `ops`, `Return`: the Plain rows' programs end in `Return`, whose mirror
/// hands back the top operand.
fn plain_program(mut ops: Vec<Op>) -> Vec<Op> {
    ops.push(Op::Return);
    ops
}

/// `assert_clear` over a run whose frame exit left its owner records for
/// V4's typed disposition: the frame and the roots are gone, the stepper is
/// Idle, and the record LIFO holds exactly `records`.
fn assert_finished_with_records<S: VmSealed, V: VmPlain>(
    engine: &VmExec<'_, RecordingEscape, S, HarnessArith, V, RecordingPoll>,
    records: usize,
) {
    assert_eq!(engine.arena_depth(), 0, "arena depth");
    assert_eq!(engine.live_root_count(), 0, "live roots");
    assert_eq!(engine.arena().handlers().len(), 0, "handlers");
    assert_eq!(engine.arena().bindings().len(), records, "bindings left");
    assert!(engine.is_idle(), "idle");
}

/// The outcome as the serial VM's `EvalResult`: a void variable is the
/// signal its datum names (data.c:1645-1657).
fn outcome_result(engine: &mut Harness, outcome: VmOutcome) -> EvalResult {
    match outcome {
        VmOutcome::Done { .. } => Ok(done_value(engine, outcome)),
        VmOutcome::Signaled(flow) => Err(flow),
        VmOutcome::VoidVariable { symbol } => {
            Err(signal(LispCondition::VoidVariable, vec![symbol]))
        }
        other => panic!("expected a value or a signal, got {other:?}"),
    }
}

#[test]
fn vm_varref_reads_the_owner_slot_without_a_poll() {
    // GNU `Bvarref` (bytecode.c:638-647): a PLAINVAL with a value is its
    // value cell read; no call, no `maybe_quit`. E4 §4.2 items 2-3: a Bound
    // owner slot supplies the value, no owner binding the canonical value.
    // One port read per opcode, no request, no poll, no escape.
    let mut ctx = context();
    let x = plain_sym("pvm-varref");
    let (five, seven) = (Value::fixnum(5), Value::fixnum(7));

    // No owner binding: the canonical value.
    let plain = HarnessPlain::new().admit(x).with_canonical(x, five);
    let mut engine = value_harness(plain);
    let constants = [Value::from_sym_id(x)];
    let ops = plain_program(vec![Op::VarRef(0)]);
    let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
    assert_eq!(done_value(&mut engine, outcome), five);
    assert_eq!(
        engine.plain_handler().accesses(),
        (1, 0, 1, 0, 1),
        "one slot read, one canonical read, one admission"
    );
    let escape = engine.escape_handler();
    assert_eq!(
        escape.requests.len(),
        0,
        "no escape: Return is owner-local (V4)"
    );
    // V4: the value is the frame's, read by the owner-local Return; no poll.
    assert_eq!(
        engine.poll_handler().edges,
        Vec::<PollEdge>::new(),
        "no poll: the read took none, and Return is owner-local"
    );
    assert_clear(&engine);

    // An owner binding: the slot's value, twice; the canonical store is not
    // read. The `let`'s record is undone by the Return's unwind (V4).
    let plain = HarnessPlain::new().admit(x).with_canonical(x, five);
    let mut engine = value_harness(plain);
    let constants = [Value::from_sym_id(x), seven];
    let ops = plain_program(vec![
        Op::Constant(1),
        Op::VarBind(0),
        Op::VarRef(0),
        Op::VarRef(0),
    ]);
    let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
    assert_eq!(done_value(&mut engine, outcome), seven);
    assert_eq!(
        engine.plain_handler().accesses(),
        // V4: the second write is the Return's unwind restoring the binding.
        (3, 2, 0, 0, 1),
        "the bind's slot read and write, the two reads, the Return's restore; no canonical read"
    );
    assert_eq!(
        engine.plain_handler().observed(x),
        "5",
        "the binding undone, the canonical value shows"
    );
    // V4: the two values were the frame's segment; the Return took the top
    // and unwound the binding. No escape, no poll, nothing left.
    assert_eq!(engine.poll_handler().edges, Vec::<PollEdge>::new());
    assert_finished_with_records(&engine, 0);
}

#[test]
fn vm_varset_writes_the_owner_slot_without_a_poll() {
    // GNU `Bvarset` (bytecode.c:710-719): a PLAINVAL without a trapped
    // write is `SET_SYMBOL_VAL`, one store. E4 §4.2 item 3: an existing
    // owner slot is replaced in place; no owner binding is a canonical
    // write. The read-back is the proof of the store.
    let mut ctx = context();
    let x = plain_sym("pvm-varset");
    let (five, seven, nine) = (Value::fixnum(5), Value::fixnum(7), Value::fixnum(9));

    // No owner binding: the canonical store.
    let plain = HarnessPlain::new().admit(x);
    let mut engine = value_harness(plain);
    let constants = [Value::from_sym_id(x), nine];
    let ops = plain_program(vec![Op::Constant(1), Op::VarSet(0), Op::VarRef(0)]);
    let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
    assert_eq!(done_value(&mut engine, outcome), nine, "the read-back");
    assert_eq!(engine.plain_handler().canonical.get(&x), Some(&nine));
    assert_eq!(
        engine.plain_handler().accesses(),
        (2, 0, 1, 1, 1),
        "two decision reads, one canonical read, one canonical write; the slot is never written"
    );
    assert_eq!(
        engine.escape_handler().requests.len(),
        0,
        "no escape: Return is owner-local (V4)"
    );
    assert_eq!(engine.poll_handler().edges, Vec::<PollEdge>::new());
    assert_clear(&engine);

    // An owner binding: the slot is replaced; the canonical store keeps 5.
    let plain = HarnessPlain::new().admit(x).with_canonical(x, five);
    let mut engine = value_harness(plain);
    let constants = [Value::from_sym_id(x), seven, nine];
    let ops = plain_program(vec![
        Op::Constant(1),
        Op::VarBind(0),
        Op::Constant(2),
        Op::VarSet(0),
        Op::VarRef(0),
    ]);
    let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
    assert_eq!(done_value(&mut engine, outcome), nine, "the read-back");
    assert_eq!(engine.plain_handler().canonical.get(&x), Some(&five));
    assert_eq!(
        engine.plain_handler().accesses(),
        // V4: the third write is the Return's unwind restoring the binding.
        (3, 3, 0, 0, 1),
        "the bind and the set read and write the slot, the read-back reads it, the Return restores it; no canonical access"
    );
    assert_finished_with_records(&engine, 0);
}

#[test]
fn vm_varbind_pushes_an_owner_binding_record_and_unbind_restores_the_slot() {
    // GNU `Bvarbind` (bytecode.c:743-746) is `specbind (vectorp[arg], POP)`:
    // the old value is saved and the new one stored (eval.c:3664-3672), and
    // `Bunbind` (bytecode.c:843-851) undoes it where it stands. The
    // stepper's mirror: the owner slot is replaced, an owner record that
    // names the slot and the undo goes on the frame's LIFO, and `Unbind`
    // pops it and restores the slot.
    let mut ctx = context();
    let x = plain_sym("pvm-varbind");
    let (one, two) = (Value::fixnum(1), Value::fixnum(2));

    // One let over nothing: the record undoes to "no owner binding".
    // V4: no finished run leaves a record behind (the Return unwinds the
    // frame's), so the record is inspected in a kept scene: an escape whose
    // completion pops a token that is not on the LIFO is the record-contract
    // fault, which poisons the run and keeps everything for inspection.
    let plain = HarnessPlain::new().admit(x);
    let mut engine = harness_with_plain(
        RecordingEscape::with_script(vec![Some(completion_popping(
            &Op::SaveExcursion,
            &[token(9)],
        ))]),
        plain,
    );
    let constants = [Value::from_sym_id(x), one];
    let ops = [Op::Constant(1), Op::VarBind(0), Op::SaveExcursion];
    let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
    assert!(
        matches!(
            outcome,
            VmOutcome::Fault(EngineFault::EscapeRecordContract(RecordStack::Bindings))
        ),
        "{outcome:?}"
    );
    assert!(!engine.is_idle(), "the scene is kept");
    let records: Vec<OwnerRecord> = engine
        .arena()
        .bindings()
        .above(OwnerRecordMark::EMPTY)
        .collect();
    let [OwnerRecord::Plain(record)] = records[..] else {
        panic!("one owner record, got {records:?}")
    };
    // The entry names the record the slab stores, and the slab's top is
    // that record (C2: one record, two views).
    assert_eq!(record.symbol, x);
    assert_eq!(engine.plain_handler().top_record(), Some(record.record));
    let stored = engine.plain_handler().stored(record);
    assert_eq!(stored.sym, x);
    assert_eq!(stored.index, TlsIndex::VOID);
    assert_eq!(stored.channel, Channel::Default);
    assert_eq!(stored.kind, BindingKind::Let);
    assert!(
        matches!(stored.undo, UndoTarget::Inherited { suspended: None }),
        "{:?}",
        stored.undo
    );
    assert_eq!(engine.plain_handler().live_records().len(), 1);

    // A let over a let: the inner record saves the outer value, and it is
    // the slab's top above the outer record.
    let plain = HarnessPlain::new().admit(x);
    let mut engine = harness_with_plain(
        RecordingEscape::with_script(vec![Some(completion_popping(
            &Op::SaveExcursion,
            &[token(9)],
        ))]),
        plain,
    );
    let constants = [Value::from_sym_id(x), one, two];
    let ops = [
        Op::Constant(1),
        Op::VarBind(0),
        Op::Constant(2),
        Op::VarBind(0),
        Op::SaveExcursion,
    ];
    let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
    assert!(
        matches!(
            outcome,
            VmOutcome::Fault(EngineFault::EscapeRecordContract(RecordStack::Bindings))
        ),
        "{outcome:?}"
    );
    let records: Vec<OwnerRecord> = engine
        .arena()
        .bindings()
        .above(OwnerRecordMark::EMPTY)
        .collect();
    assert_eq!(records.len(), 2);
    let [OwnerRecord::Plain(outer), OwnerRecord::Plain(inner)] = records[..] else {
        panic!("two owner entries, got {records:?}")
    };
    assert_ne!(outer.record, inner.record);
    assert_eq!(engine.plain_handler().top_record(), Some(inner.record));
    // The slab holds both records, outermost first, and each entry's record
    // saves what the slot held before it.
    let live = engine.plain_handler().live_records();
    assert_eq!(live.len(), 2);
    assert_eq!(
        (live[0].0, live[0].1, live[1].0, live[1].1),
        (x, BindingKind::Let, x, BindingKind::Let)
    );
    assert!(
        matches!(live[0].2, UndoTarget::Inherited { suspended: None }),
        "{:?}",
        live[0].2
    );
    assert!(
        matches!(live[1].2, UndoTarget::Previous(saved) if saved.get() == one),
        "{:?}",
        live[1].2
    );
    assert!(!engine.is_idle(), "the scene is kept");

    // The unbind: the record leaves and the slot is back to no binding. The
    // probe between the Unbind and the Return (lead C1): the Return escapes
    // (DEBUG_ON_EXIT) and its LIFO snapshot must be empty - else the Return's
    // own unwind would restore what a no-op Unbind left, with the same log.
    let log = EffectLog::default();
    let plain = HarnessPlain::new().admit(x).logging(log.clone());
    let mut engine = harness_with_plain(
        RecordingEscape::logging_with_script(
            vec![Some(completion(&Op::Return, vec![Value::NIL]))],
            log.clone(),
        ),
        plain,
    );
    engine.set_entry_exit_flags(ExitFlags::DEBUG_ON_EXIT);
    let constants = [Value::from_sym_id(x), one];
    let ops = [
        Op::Constant(1),
        Op::VarBind(0),
        Op::Unbind(1),
        Op::Nil,
        Op::Return,
    ];
    let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
    assert_eq!(done_value(&mut engine, outcome), Value::NIL);
    assert_eq!(
        log.entries(),
        vec!["slot <- 1", "slot <- none", "escape Return"],
        "the bind's write, the unbind's restore, then the Return's cleanup"
    );
    assert_eq!(
        engine.escape_handler().lifos[0].0,
        Vec::<Option<u64>>::new(),
        "empty at Return: the Unbind restored, not the Return"
    );
    assert_eq!(
        engine.plain_handler().accesses(),
        (1, 2, 0, 0, 1),
        "the bind reads and writes the slot, the unbind writes it; no canonical access"
    );
    assert_eq!(
        engine
            .escape_handler()
            .requests
            .iter()
            .map(|request| request.op.clone())
            .collect::<Vec<_>>(),
        vec![Op::Return],
        "the owner unbind escapes nothing; only the Return's cleanup escapes"
    );
    assert_eq!(engine.poll_handler().edges, vec![PollEdge::SerialRequest]);
    assert_clear(&engine);
}

#[test]
fn vm_a_void_binding_signals_void_variable_on_the_owner() {
    // GNU `Bvarref` of a void variable reaches `Fsymbol_value`, which signals
    // `void-variable` with the symbol as its only datum (bytecode.c:638-647,
    // data.c:1645-1657; the serial `fast_path_var_ref`,
    // bytecode/vm.rs:7320-7327). E4 §2.4: the owner raises it itself, the
    // typed `StepOutcome::VoidVariable` of C4 (engine.rs:222-236). Three
    // shapes: no owner binding and no canonical value; a canonical unbound
    // value; an owner slot that is a local void binding (GNU `makunbound`
    // inside a `let`, eval.c:3664-3672 saving the void).
    let mut ctx = context();
    let x = plain_sym("pvm-void");
    let (unbound, five) = (Value::UNBOUND, Value::fixnum(5));
    let xs = Value::from_sym_id(x);
    let cases: Vec<(
        &str,
        Vec<Op>,
        Vec<Value>,
        HarnessPlain,
        (usize, usize, usize, usize, usize),
    )> = vec![
        (
            "no owner binding, no canonical value",
            plain_program(vec![Op::VarRef(0)]),
            vec![xs],
            HarnessPlain::new().admit(x),
            (1, 0, 1, 0, 1),
        ),
        (
            "no owner binding, a canonical unbound value",
            plain_program(vec![Op::VarRef(0)]),
            vec![xs],
            HarnessPlain::new().admit(x).with_canonical(x, unbound),
            (1, 0, 1, 0, 1),
        ),
        (
            "a local void binding over a canonical value",
            plain_program(vec![Op::Constant(1), Op::VarBind(0), Op::VarRef(0)]),
            vec![xs, unbound],
            HarnessPlain::new().admit(x).with_canonical(x, five),
            // V4 (N-v2-idle-slab-undo): the signal settles the run, and the
            // idle unwind undoes the frame's binding - the second slot write.
            (2, 2, 0, 0, 1),
        ),
    ];
    for (name, ops, constants, plain, accesses) in cases {
        let mut engine = value_harness(plain);
        let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
        let VmOutcome::VoidVariable { symbol } = outcome else {
            panic!("{name}: got {outcome:?}")
        };
        assert_eq!(
            symbol.as_bare_symbol_id(),
            Some(x),
            "{name}: the symbol is the signal's datum"
        );
        assert_eq!(engine.plain_handler().accesses(), accesses, "{name}");
        assert!(
            engine.escape_handler().requests.is_empty(),
            "{name}: no escape"
        );
        assert!(engine.poll_handler().edges.is_empty(), "{name}: no poll");
        assert!(engine.is_idle(), "{name}: settled to Idle");
        assert_clear(&engine);
    }

    // Unbinding the void binding shows the canonical value again.
    let plain = HarnessPlain::new().admit(x).with_canonical(x, five);
    let mut engine = value_harness(plain);
    let ops = plain_program(vec![
        Op::Constant(1),
        Op::VarBind(0),
        Op::Unbind(1),
        Op::VarRef(0),
    ]);
    let outcome = engine.run(&mut ctx, program_with(&ops, &vec![xs, unbound]));
    assert_eq!(done_value(&mut engine, outcome), five);
    assert_eq!(
        engine.plain_handler().accesses(),
        (2, 2, 1, 0, 1),
        "the bind's read and write, the unbind's write, one canonical read"
    );
    assert_clear(&engine);
}

#[test]
fn vm_a_refused_key_escapes_once_with_zero_slot_access() {
    // A symbol the admission refuses stays on the serial path: the ONE
    // escape carries the access kind and the refusal (E4 §4.2), and the
    // serial side performs the access. Nothing is read or written through
    // the owner before the request. Ref, Set and Bind alike, a `Constant`
    // refusal the same way; a refused bind's serial specbind is a record
    // the completion reports.
    let mut ctx = context();
    let x = plain_sym("pvm-refused");
    let xs = Value::from_sym_id(x);
    let (five, nine) = (Value::fixnum(5), Value::fixnum(9));
    // (name, op, the ops before it, completion, access, refusal, the value
    // Return reports, the records left)
    let cases: Vec<(
        &str,
        Op,
        Vec<Op>,
        EscapeCompletion,
        PlainVarAccess,
        PlainRefusal,
        Value,
        usize,
    )> = vec![
        (
            "ref, not plain",
            Op::VarRef(0),
            Vec::new(),
            completion(&Op::VarRef(0), vec![five]),
            PlainVarAccess::Ref,
            PlainRefusal::NotPlain,
            five,
            0,
        ),
        (
            "set, not plain",
            Op::VarSet(0),
            vec![Op::Constant(1)],
            completion(&Op::VarSet(0), Vec::new()),
            PlainVarAccess::Set,
            PlainRefusal::NotPlain,
            Value::NIL,
            0,
        ),
        (
            "bind, not plain",
            Op::VarBind(0),
            vec![Op::Constant(1)],
            completion_pushing(&Op::VarBind(0), token(5)),
            PlainVarAccess::Bind,
            PlainRefusal::NotPlain,
            Value::NIL,
            // The marker the serial let left is the Return's cleanup's to pop.
            0,
        ),
        (
            "set, a constant symbol",
            Op::VarSet(0),
            vec![Op::Constant(1)],
            completion(&Op::VarSet(0), Vec::new()),
            PlainVarAccess::Set,
            PlainRefusal::Constant,
            Value::NIL,
            0,
        ),
    ];
    for (name, op, before, completion, access, refusal, reported, left) in cases {
        // A ref leaves the serial side's value as the frame's value.
        let after = if matches!(op, Op::VarRef(_)) {
            Vec::new()
        } else {
            vec![Op::Nil]
        };
        // The opcode's pc is after the operands it consumes.
        let opcode_pc = before.len();
        let ops = plain_program([before, vec![op.clone()], after].concat());
        // A refused bind's serial let leaves a marker; the Return under it is
        // the serial cleanup, whose completion pops the marker (V4).
        let cleans = matches!(op, Op::VarBind(_));
        let mut script = vec![Some(completion)];
        if cleans {
            script.push(Some(completion_popping(&Op::Return, &[token(5)])));
        }
        let plain = HarnessPlain::new().refuse(x, refusal);
        let mut engine = harness_with_plain(RecordingEscape::mirroring_with_script(script), plain);
        let constants = [xs, nine];
        let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
        assert_eq!(done_value(&mut engine, outcome), reported, "{name}");
        let escape = engine.escape_handler();
        assert_eq!(
            escape.requests.len(),
            1 + usize::from(cleans),
            "{name}: the access, plus the Return's cleanup when a marker is left"
        );
        assert_eq!(escape.requests[0].op, op, "{name}");
        assert_eq!(
            escape.requests[0].why,
            EscapeWhy::PlainRefused { access, refusal },
            "{name}: the escape says the access and the refusal"
        );
        assert_eq!(
            escape.requests[0].pc,
            Pc::new(opcode_pc),
            "{name}: the opcode's own pc, after its operands"
        );
        assert_eq!(
            engine.plain_handler().accesses(),
            (0, 0, 0, 0, 1),
            "{name}: one admission, no slot or canonical access"
        );
        assert!(engine.plain_handler().canonical.is_empty(), "{name}");
        assert_eq!(
            engine.poll_handler().edges,
            vec![PollEdge::SerialRequest; 1 + usize::from(cleans)],
            "{name}: each escape's request poll"
        );
        assert_finished_with_records(&engine, left);
    }

    // The underflow check comes first: `VarSet` with no operand is the
    // malformed-code signal, before any admission.
    let plain = HarnessPlain::new().refuse(x, PlainRefusal::NotPlain);
    let mut engine = harness_with_plain(RecordingEscape::mirroring_return(), plain);
    let ops = [Op::VarSet(0), Op::Nil, Op::Return];
    let outcome = engine.run(&mut ctx, program_with(&ops, &[xs]));
    let VmOutcome::Signaled(flow) = outcome else {
        panic!("got {outcome:?}")
    };
    assert_eq!(signal_key(&flow).0, "error");
    assert_eq!(
        engine.plain_handler().accesses(),
        (0, 0, 0, 0, 0),
        "the port is not asked"
    );
    assert!(engine.escape_handler().requests.is_empty());
    assert_clear(&engine);
}

#[test]
fn vm_a_stale_plain_key_escapes_once_and_reads_nothing_stale() {
    // E4 §4.2: a structural change the serial side made moves the era; a
    // cached key is checked against the generation at use, and a stale one
    // is re-derived through the admission before any access. Re-derived as
    // refused: the ONE escape, and the stale key read nothing.
    let mut ctx = context();
    let x = plain_sym("pvm-stale");
    let (five, six) = (Value::fixnum(5), Value::fixnum(6));
    let plain = HarnessPlain::new().admit(x).with_canonical(x, five);
    let mut engine = harness_with_plain(
        // Consumed by escape index: run 1 escapes nothing (V4), so run 2's
        // refused read takes the first entry.
        RecordingEscape::mirroring_with_script(vec![
            Some(completion(&Op::VarRef(0), vec![six])),
            None,
        ]),
        plain,
    );
    let ops = plain_program(vec![Op::VarRef(0)]);
    let constants = [Value::from_sym_id(x)];
    let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
    assert_eq!(
        done_value(&mut engine, outcome),
        five,
        "the first run mints the key"
    );
    assert_eq!(engine.plain_handler().accesses(), (1, 0, 1, 0, 1));
    assert_clear(&engine);

    // The era moves, and the symbol is no longer Plain.
    engine.plain_handler_mut().next_era();
    engine
        .plain_handler_mut()
        .refuse_later(x, PlainRefusal::NotPlain);
    let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
    assert_eq!(
        done_value(&mut engine, outcome),
        six,
        "the serial side's value"
    );
    assert_eq!(
        engine.plain_handler().accesses(),
        (1, 0, 1, 0, 2),
        "one more admission, no stale read"
    );
    let escape = engine.escape_handler();
    assert_eq!(
        escape.requests.len(),
        1,
        "the re-derived read alone; Return is owner-local in both runs"
    );
    assert_eq!(escape.requests[0].op, Op::VarRef(0));
    assert_eq!(
        escape.requests[0].why,
        EscapeWhy::PlainRefused {
            access: PlainVarAccess::Ref,
            refusal: PlainRefusal::NotPlain,
        }
    );
    assert_eq!(
        escape.segments[0],
        Vec::<Value>::new(),
        "nothing was pushed before the escape"
    );
    assert_clear(&engine);
}

#[test]
fn vm_a_stale_plain_key_that_is_still_plain_is_rederived_without_an_escape() {
    // Re-derived as admitted: a fresh key for the moved era, the same slot,
    // no escape.
    let mut ctx = context();
    let x = plain_sym("pvm-rederived");
    let five = Value::fixnum(5);
    let plain = HarnessPlain::new().admit(x).with_canonical(x, five);
    let mut engine = value_harness(plain);
    let ops = plain_program(vec![Op::VarRef(0)]);
    let constants = [Value::from_sym_id(x)];
    let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
    assert_eq!(done_value(&mut engine, outcome), five);
    assert_eq!(engine.plain_handler().accesses(), (1, 0, 1, 0, 1));
    assert_clear(&engine);
    engine.plain_handler_mut().next_era();
    let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
    assert_eq!(done_value(&mut engine, outcome), five);
    assert_eq!(
        engine.plain_handler().accesses(),
        (2, 0, 2, 0, 2),
        "each run reads the slot, then the canonical value; the key was re-derived once"
    );
    let escape = engine.escape_handler();
    assert_eq!(
        escape
            .requests
            .iter()
            .map(|request| request.op.clone())
            .collect::<Vec<_>>(),
        Vec::<Op>::new(),
        "no escape for the re-derivation, nor for the owner-local Return (V4)"
    );
    assert_clear(&engine);
}

#[test]
fn vm_a_constant_that_is_no_symbol_is_nil_to_the_admission() {
    // The serial `sym_id_at` (bytecode/vm.rs:10882-10893) resolves a constant
    // that is no symbol, and an index past the pool, to `nil`; GNU's
    // `XBARE_SYMBOL` on one is unchecked (bytecode.c:640). The stepper
    // resolves the same way, so it invents no signal: `nil` reaches the
    // admission, which refuses a constant symbol, and the serial side runs
    // the opcode. A symbol-with-pos resolves to its bare symbol.
    let mut ctx = context();
    let x = plain_sym("pvm-positioned");
    let xs = Value::from_sym_id(x);
    let (five, seven) = (Value::fixnum(5), Value::fixnum(7));
    // (name, op, the ops before it, constants, completion)
    let cases: Vec<(&str, Op, Vec<Op>, Vec<Value>, EscapeCompletion)> = vec![
        (
            "a fixnum",
            Op::VarRef(0),
            Vec::new(),
            vec![five],
            completion(&Op::VarRef(0), vec![Value::NIL]),
        ),
        (
            "an index past the pool",
            Op::VarSet(3),
            vec![Op::Constant(1)],
            vec![xs, five],
            completion(&Op::VarSet(3), Vec::new()),
        ),
    ];
    for (name, op, before, constants, completion) in cases {
        let ops = plain_program([before, vec![op.clone(), Op::Nil]].concat());
        let plain = HarnessPlain::new()
            .admit(x)
            .refuse(NIL_SYM_ID, PlainRefusal::Constant);
        let mut engine = harness_with_plain(
            RecordingEscape::mirroring_with_script(vec![Some(completion), None]),
            plain,
        );
        let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
        assert_eq!(done_value(&mut engine, outcome), Value::NIL, "{name}");
        let escape = engine.escape_handler();
        assert_eq!(
            escape.requests.len(),
            1,
            "{name}: the access; Return is owner-local (V4)"
        );
        assert_eq!(
            escape.requests[0].why,
            EscapeWhy::PlainRefused {
                access: match op {
                    Op::VarRef(_) => PlainVarAccess::Ref,
                    Op::VarSet(_) => PlainVarAccess::Set,
                    _ => PlainVarAccess::Bind,
                },
                refusal: PlainRefusal::Constant,
            },
            "{name}: nil is a constant symbol"
        );
        assert_eq!(
            engine.plain_handler().accesses(),
            (0, 0, 0, 0, 1),
            "{name}: nil's admission, no access"
        );
        assert_clear(&engine);
    }

    // A symbol-with-pos: the bare symbol's slot.
    let positioned = ctx
        .tagged_heap
        .alloc_symbol_with_pos(Value::symbol("pvm-positioned"), Value::fixnum(0));
    assert!(positioned.is_symbol_with_pos(), "the fixture");
    let plain = HarnessPlain::new().admit(x);
    let mut engine = value_harness(plain);
    let ops = plain_program(vec![Op::Constant(1), Op::VarBind(0), Op::VarRef(0)]);
    let outcome = engine.run(&mut ctx, program_with(&ops, &[positioned, seven]));
    assert_eq!(done_value(&mut engine, outcome), seven);
    assert_eq!(
        engine.plain_handler().accesses(),
        // V4: the run ends Done, and the frame's binding is undone on the
        // way out - the Return's unwind, or its serial cleanup when the
        // refused bind left a marker - a second slot write.
        (2, 2, 0, 0, 1),
        "the bind's slot read for its undo target and its write, the ref's slot read, the Return's restore; one admission (the ref's key is cached)"
    );
    assert_eq!(
        engine.escape_handler().requests.len(),
        0,
        "no escape: Return is owner-local (V4)"
    );
    // V4: the Return unwound the frame's binding; nothing is left.
    assert_finished_with_records(&engine, 0);
}

#[test]
fn vm_unbind_walks_mixed_records_in_lifo_order() {
    // GNU `Bunbind n` (bytecode.c:843-851) is `unbind_to (SPECPDL_INDEX ()
    // - n)`: n entries of any kind leave in LIFO order, each undone as it
    // leaves (eval.c:3946-3957). The serial frame marks a `specbind`, a
    // `save-excursion` and an unwind-protect alike (bytecode/vm.rs:5722-5734,
    // 6158-6162, 7176-7186), so the stepper counts the serial tokens the
    // escapes pushed and the owner records in the one order. An owner record
    // is undone on the owner; each serial record is ONE escape whose request
    // names it.
    let mut ctx = context();
    let x = plain_sym("pvm-mixed");
    let y = plain_sym("pvm-localized");
    let (xs, ys) = (Value::from_sym_id(x), Value::from_sym_id(y));
    let (one, two, ninety_nine) = (Value::fixnum(1), Value::fixnum(2), Value::fixnum(99));
    // (name, ops, constants, port, script, the log, the lifos at the Unbind
    // escape and at Return)
    let cases: Vec<(
        &str,
        Vec<Op>,
        Vec<Value>,
        HarnessPlain,
        Vec<Option<EscapeCompletion>>,
        Vec<&str>,
        Lifo,
    )> = vec![
        (
            "let, save-excursion, let",
            vec![
                Op::Constant(1),
                Op::VarBind(0),
                Op::SaveExcursion,
                Op::Constant(2),
                Op::VarBind(0),
                Op::Unbind(3),
                Op::Nil,
            ],
            vec![xs, one, two],
            HarnessPlain::new().admit(x),
            vec![
                Some(completion_pushing(&Op::SaveExcursion, token(9))),
                Some(completion_popping(&Op::Unbind(3), &[token(9)])),
                Some(completion(&Op::Return, vec![Value::NIL])),
            ],
            vec![
                "slot <- 1",
                "escape SaveExcursion",
                "slot <- 2",
                "slot <- 1",
                "escape Unbind(3) [9]",
                "slot <- none",
                "escape Return",
            ],
            vec![None, Some(9)],
        ),
        (
            "let, unwind-protect, let",
            vec![
                Op::Constant(1),
                Op::VarBind(0),
                Op::Constant(2),
                Op::UnwindProtectPop,
                Op::Constant(3),
                Op::VarBind(0),
                Op::Unbind(3),
                Op::Nil,
            ],
            vec![xs, one, ninety_nine, two],
            HarnessPlain::new().admit(x),
            vec![
                Some(completion_pushing(&Op::UnwindProtectPop, token(7))),
                Some(completion_popping(&Op::Unbind(3), &[token(7)])),
                Some(completion(&Op::Return, vec![Value::NIL])),
            ],
            vec![
                "slot <- 1",
                "escape UnwindProtectPop",
                "slot <- 2",
                "slot <- 1",
                "escape Unbind(3) [7]",
                "slot <- none",
                "escape Return",
            ],
            vec![None, Some(7)],
        ),
        (
            "let, a localized binding the admission refused, let",
            vec![
                Op::Constant(2),
                Op::VarBind(0),
                Op::Constant(3),
                Op::VarBind(1),
                Op::Unbind(2),
                Op::Nil,
            ],
            vec![xs, ys, one, two],
            HarnessPlain::new()
                .admit(x)
                .refuse(y, PlainRefusal::NotPlain),
            vec![
                Some(completion_pushing(&Op::VarBind(1), token(5))),
                Some(completion_popping(&Op::Unbind(2), &[token(5)])),
                Some(completion(&Op::Return, vec![Value::NIL])),
            ],
            vec![
                "slot <- 1",
                "escape VarBind(1)",
                "escape Unbind(2) [5]",
                "slot <- none",
                "escape Return",
            ],
            vec![None, Some(5)],
        ),
    ];
    for (name, ops, constants, plain, script, log, lifo) in cases {
        let ops = plain_program(ops);
        let unbind_pc = ops.len() - 3;
        let log_shared = EffectLog::default();
        let plain = plain.logging(log_shared.clone());
        let mut engine = harness_with_plain(
            RecordingEscape::logging_with_script(script, log_shared.clone()),
            plain,
        );
        // The probe between the Unbind and the Return (lead C1): the Return
        // escapes and its LIFO snapshot is the walk's post-state.
        engine.set_entry_exit_flags(ExitFlags::DEBUG_ON_EXIT);
        let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
        assert_eq!(done_value(&mut engine, outcome), Value::NIL, "{name}");
        assert_eq!(log_shared.entries(), log, "{name}");
        let escape = engine.escape_handler();
        assert_eq!(
            escape.requests.len(),
            3,
            "{name}: the record, the Unbind, the Return's cleanup"
        );
        assert_eq!(escape.requests[1].op, ops[unbind_pc], "{name}");
        assert!(
            matches!(
                escape.requests[1].why,
                EscapeWhy::SerialUnwind(ref unwind)
                    if unwind.pop_handlers.is_empty() && unwind.unbind.len() == 1
            ),
            "{name}"
        );
        assert_eq!(escape.requests[1].pc, Pc::new(unbind_pc), "{name}");
        assert_eq!(
            escape.lifos[1].0, lifo,
            "{name}: the LIFO at the Unbind escape"
        );
        assert_eq!(
            escape.lifos[2].0,
            Vec::<Option<u64>>::new(),
            "{name}: empty at Return"
        );
        assert_eq!(
            engine.poll_handler().edges,
            vec![PollEdge::SerialRequest; 3],
            "{name}: each escape's request poll"
        );
        assert_clear(&engine);
    }
}

#[test]
fn vm_unbind_after_a_watcher_added_inside_let_takes_the_serial_unbind() {
    // N-v2-unbind-revalidate. GNU's `do_one_unbind` for a `SPECPDL_LET`
    // (eval.c:3848-3870) re-reads the symbol as the binding leaves: a watcher
    // added while the binding was live gets the `unlet` notification through
    // `set_internal`, and a variable made local or forwarded goes through
    // `set_default_internal`. The serial `unbind_owner_binding`
    // (owner_bindings.rs:1276-1360) keeps a slow path for exactly that; the
    // owner-local restore writes the slot and nothing else. So when the
    // symbol gained a watcher inside the `let`, the walk must NOT restore on
    // the owner: it hands the record to the serial unbind as ONE
    // `PlainUnbind` escape naming the symbol and the record. The serial side
    // takes the record and restores the slot; the walk checks the record is
    // gone. GNU `unbind_to` (eval.c:3940-3954) decrements `specpdl_ptr`
    // BEFORE the unbind runs, so the executor's entry is already gone at
    // the escape. The serial run above it still leaves first, as before.
    let mut ctx = context();
    let x = plain_sym("pvm-watched");
    let (xs, one, two) = (Value::from_sym_id(x), Value::fixnum(1), Value::fixnum(2));
    let ops = plain_program(vec![
        Op::Constant(1),
        Op::VarBind(0),
        Op::SaveExcursion,
        Op::Unbind(2),
        Op::Nil,
    ]);
    // Constant(1)=0, VarBind(0)=1, SaveExcursion=2, Unbind(2)=3, Nil=4.
    let unbind_pc = 3;
    let log_shared = EffectLog::default();
    // The watcher is on `x` by the time the binding leaves: the serial side
    // added it from the SaveExcursion escape, inside the `let`.
    let plain = HarnessPlain::new()
        .admit(x)
        .watched(x)
        .logging(log_shared.clone());
    let mut engine = harness_with_plain(
        RecordingEscape::logging_with_script(
            vec![
                Some(completion_pushing(&Op::SaveExcursion, token(9))),
                Some(completion_popping(&Op::Unbind(2), &[token(9)])),
                // The hand-off: the serial side undoes x's binding through
                // the escape (the double takes the record and restores the
                // slot); its completion moves no serial token.
                Some(EscapeCompletion::fall_through(&Op::Unbind(2))),
                None,
            ],
            log_shared.clone(),
        ),
        plain,
    );
    let outcome = engine.run(&mut ctx, program_with(&ops, &vec![xs, one, two]));
    assert_eq!(done_value(&mut engine, outcome), Value::NIL);
    assert_eq!(
        log_shared.entries(),
        vec![
            "slot <- 1",
            "escape SaveExcursion",
            "escape Unbind(2) [9]",
            "escape Unbind(2) [plain]",
            "slot <- none",
        ],
        "the serial run leaves first; the watched binding is handed to the serial unbind, which takes the record and restores the slot; the owner never restores it"
    );
    let escape = engine.escape_handler();
    assert_eq!(
        escape.requests.len(),
        3,
        "record, serial run, hand-off; Return is owner-local (V4)"
    );
    assert!(
        matches!(
            escape.requests[2].why,
            EscapeWhy::PlainUnbind { symbol, .. } if symbol == x
        ),
        "the second Unbind escape names the watched symbol"
    );
    assert_eq!(escape.requests[2].pc, Pc::new(unbind_pc));
    assert_eq!(escape.requests[2].op, ops[unbind_pc]);
    assert_eq!(
        escape.lifos[2].0,
        Vec::<Option<u64>>::new(),
        "the entry left before the escape (GNU's order)"
    );
    // Post-state: the serial side took the slab record; the owner has none.
    assert!(
        engine.plain_handler().live_records().is_empty(),
        "the serial side took the record"
    );
    assert_eq!(engine.plain_handler().top_record(), None);
}

#[test]
fn vm_a_plain_unbind_hand_off_the_serial_side_left_undone_faults_as_a_record_contract() {
    // The hand-off's own check: after the escape, the slab record the request
    // named must be gone (the serial run's `still_named` check, for the one
    // record). A serial side that returns without undoing the binding leaves
    // it live, and the walk stops with the record-contract fault. Nothing
    // was restored on the owner: the slot still holds the binding.
    let mut ctx = context();
    let x = plain_sym("pvm-watched-unhandled");
    let (xs, one) = (Value::from_sym_id(x), Value::fixnum(1));
    let ops = plain_program(vec![
        Op::Constant(1),
        Op::VarBind(0),
        Op::Unbind(1),
        Op::Nil,
    ]);
    let log_shared = EffectLog::default();
    let plain = HarnessPlain::new()
        .admit(x)
        .watched(x)
        .logging(log_shared.clone());
    let mut engine = harness_with_plain(
        RecordingEscape::logging_with_script(vec![None], log_shared.clone())
            .leaving_plain_binding(),
        plain,
    );
    let outcome = engine.run(&mut ctx, program_with(&ops, &vec![xs, one]));
    assert!(
        matches!(
            fault_of(outcome),
            EngineFault::EscapeRecordContract(RecordStack::Bindings)
        ),
        "a record still live after the hand-off is the record-contract fault"
    );
    assert_eq!(
        log_shared.entries(),
        vec!["slot <- 1", "escape Unbind(1) [plain]"],
        "the owner restored nothing"
    );
    assert_eq!(
        engine.plain_handler().live_records().len(),
        1,
        "the slab record is still live"
    );
}

#[test]
fn vm_unbind_walks_a_record_the_hand_off_pushes_in_addition_to_n() {
    // GNU `unbind_to` (eval.c:3940-3954) walks to an absolute specpdl index:
    // a binding the unbind's own code pushes (a watcher's `unlet` running a
    // `let`) is undone IN ADDITION to the n the opcode named. The serial run
    // above the hand-off leaves first; the hand-off's completion pushes
    // token 7; the walk then undoes 7 as well. The executor's entry for x
    // left before the escape (GNU's order), so it cannot stay under 7, and
    // the walk counts the pushed record as extra, not as one of the n.
    let mut ctx = context();
    let x = plain_sym("pvm-watched-pushes");
    let (xs, one, two) = (Value::from_sym_id(x), Value::fixnum(1), Value::fixnum(2));
    let ops = plain_program(vec![
        Op::Constant(1),
        Op::VarBind(0),
        Op::SaveExcursion,
        Op::Unbind(2),
        Op::Nil,
    ]);
    let log_shared = EffectLog::default();
    let plain = HarnessPlain::new()
        .admit(x)
        .watched(x)
        .logging(log_shared.clone());
    let mut engine = harness_with_plain(
        RecordingEscape::logging_with_script(
            vec![
                Some(completion_pushing(&Op::SaveExcursion, token(9))),
                Some(completion_popping(&Op::Unbind(2), &[token(9)])),
                // The hand-off's completion pushes 7: the watcher's Lisp
                // ran a `let`.
                Some(completion_pushing(&Op::Unbind(2), token(7))),
                Some(completion_popping(&Op::Unbind(2), &[token(7)])),
                None,
            ],
            log_shared.clone(),
        ),
        plain,
    );
    let outcome = engine.run(&mut ctx, program_with(&ops, &vec![xs, one, two]));
    assert_eq!(done_value(&mut engine, outcome), Value::NIL);
    assert_eq!(
        log_shared.entries(),
        vec![
            "slot <- 1",
            "escape SaveExcursion",
            "escape Unbind(2) [9]",
            "escape Unbind(2) [plain]",
            "slot <- none",
            "escape Unbind(2) [7]",
        ],
        "the pushed record is walked after the hand-off, as an extra"
    );
    let escape = engine.escape_handler();
    assert_eq!(
        escape.requests.len(),
        4,
        "record, serial run, hand-off, the pushed run; Return is owner-local"
    );
    assert!(
        matches!(
            escape.requests[3].why,
            EscapeWhy::SerialUnwind(ref unwind)
                if unwind.pop_handlers.is_empty()
                    && unwind.unbind.len() == 1
                    && unwind.unbind[0] == token(7)
        ),
        "the pushed record is the next run"
    );
    assert_eq!(
        escape.lifos[2].0,
        Vec::<Option<u64>>::new(),
        "x's entry left before the hand-off escape"
    );
    assert_eq!(
        escape.lifos[3].0,
        vec![Some(7)],
        "the pushed record is on the LIFO for its own run"
    );
    assert!(engine.plain_handler().live_records().is_empty());
}

#[test]
fn vm_unbind_restores_an_unwatched_sibling_on_the_owner_and_hands_the_watched_one_to_serial() {
    // One `Unbind n` over two owner bindings: only the symbol that gained a
    // watcher is handed to the serial unbind; the sibling is undone on the
    // owner. The sibling is innermost here, so it is restored first.
    let mut ctx = context();
    let x = plain_sym("pvm-watched-sibling");
    let y = plain_sym("pvm-unwatched-sibling");
    let (xs, ys) = (Value::from_sym_id(x), Value::from_sym_id(y));
    let (one, two) = (Value::fixnum(1), Value::fixnum(2));
    // Two symbols precede the values in the pool: Constant(2)=1, Constant(3)=2.
    let ops = plain_program(vec![
        Op::Constant(2),
        Op::VarBind(0),
        Op::Constant(3),
        Op::VarBind(1),
        Op::Unbind(2),
        Op::Nil,
    ]);
    let log_shared = EffectLog::default();
    let plain = HarnessPlain::new()
        .admit(x)
        .admit(y)
        .watched(x)
        .logging(log_shared.clone());
    let mut engine = harness_with_plain(
        RecordingEscape::logging_with_script(
            vec![Some(EscapeCompletion::fall_through(&Op::Unbind(2))), None],
            log_shared.clone(),
        ),
        plain,
    );
    let outcome = engine.run(&mut ctx, program_with(&ops, &vec![xs, ys, one, two]));
    assert_eq!(done_value(&mut engine, outcome), Value::NIL);
    // The harness mints every symbol the same slot (`TlsIndex::VOID`), so
    // y's undo is Previous(1), the value x left there, and its restore on
    // the owner writes 1 back; x's own undo is Inherited, restored by the
    // serial side to none. Per-symbol slots would be a harness improvement,
    // not a change to the walk.
    assert_eq!(
        log_shared.entries(),
        vec![
            "slot <- 1",
            "slot <- 2",
            "slot <- 1",
            "escape Unbind(2) [plain]",
            "slot <- none",
        ],
        "the unwatched sibling is restored on the owner first; the watched one is handed to the serial unbind"
    );
    let escape = engine.escape_handler();
    assert_eq!(
        escape.requests.len(),
        1,
        "the hand-off; Return is owner-local"
    );
    assert!(
        matches!(
            escape.requests[0].why,
            EscapeWhy::PlainUnbind { symbol, .. } if symbol == x
        ),
        "only the watched symbol is handed off"
    );
    assert_eq!(
        escape.lifos[0].0,
        Vec::<Option<u64>>::new(),
        "x's entry left before the hand-off escape; y's was already undone"
    );
    assert!(engine.plain_handler().live_records().is_empty());
}

#[test]
fn vm_unbind_of_nested_watched_bindings_hands_each_record_to_serial_innermost_first() {
    // Two bindings of one symbol, both handed to the serial unbind. GNU
    // undoes them LIFO, each through `do_one_unbind`; each hand-off names
    // its own record, so the restores run innermost (Previous: the outer
    // value) then outermost (Inherited: none). The slab is empty afterwards.
    let mut ctx = context();
    let x = plain_sym("pvm-watched-nested");
    let (xs, one, two) = (Value::from_sym_id(x), Value::fixnum(1), Value::fixnum(2));
    let ops = plain_program(vec![
        Op::Constant(1),
        Op::VarBind(0),
        Op::Constant(2),
        Op::VarBind(0),
        Op::Unbind(2),
        Op::Nil,
    ]);
    let log_shared = EffectLog::default();
    let plain = HarnessPlain::new()
        .admit(x)
        .watched(x)
        .logging(log_shared.clone());
    let mut engine = harness_with_plain(
        RecordingEscape::logging_with_script(
            vec![
                Some(EscapeCompletion::fall_through(&Op::Unbind(2))),
                Some(EscapeCompletion::fall_through(&Op::Unbind(2))),
                None,
            ],
            log_shared.clone(),
        ),
        plain,
    );
    let outcome = engine.run(&mut ctx, program_with(&ops, &vec![xs, one, two]));
    assert_eq!(done_value(&mut engine, outcome), Value::NIL);
    assert_eq!(
        log_shared.entries(),
        vec![
            "slot <- 1",
            "slot <- 2",
            "escape Unbind(2) [plain]",
            "slot <- 1",
            "escape Unbind(2) [plain]",
            "slot <- none",
        ],
        "innermost first: the inner record restores the outer value, the outer restores none"
    );
    let escape = engine.escape_handler();
    assert_eq!(
        escape.requests.len(),
        2,
        "two hand-offs; Return is owner-local"
    );
    let handed: Vec<OwnerRecordId> = escape
        .requests
        .iter()
        .filter_map(|request| match &request.why {
            EscapeWhy::PlainUnbind { record, .. } => Some(*record),
            _ => None,
        })
        .collect();
    assert_eq!(handed.len(), 2, "each record is handed off by name");
    assert_ne!(handed[0], handed[1], "and they are different records");
    assert!(engine.plain_handler().live_records().is_empty());
}

#[test]
fn vm_a_plain_unbind_completion_that_also_pops_a_serial_token_faults_and_leaves_the_rest_bound() {
    // The completion is checked against the request: a `PlainUnbind`
    // hand-off pops no token (the entry already left; the serial side undoes
    // the one binding on the slab). A completion that also pops the serial
    // token below it used to pass, since it was still the top run, and the
    // walk then counted the loss away and unbound past n. GNU `unbind_to`
    // walks to the absolute index the opcode named (eval.c:3940-3954); here
    // `Unbind 2` over [y, t1, x] names x and t1, and y must stay bound.
    let mut ctx = context();
    let x = plain_sym("pvm-watched-overpop-x");
    let y = plain_sym("pvm-watched-overpop-y");
    let (xs, ys) = (Value::from_sym_id(x), Value::from_sym_id(y));
    let (one, two) = (Value::fixnum(1), Value::fixnum(2));
    // Two symbols precede the values: Constant(2)=1, Constant(3)=2.
    let ops = plain_program(vec![
        Op::Constant(2),
        Op::VarBind(0),
        Op::SaveExcursion,
        Op::Constant(3),
        Op::VarBind(1),
        Op::Unbind(2),
        Op::Nil,
    ]);
    let log_shared = EffectLog::default();
    let plain = HarnessPlain::new()
        .admit(x)
        .admit(y)
        .watched(x)
        .logging(log_shared.clone());
    let mut engine = harness_with_plain(
        RecordingEscape::logging_with_script(
            vec![
                Some(completion_pushing(&Op::SaveExcursion, token(1))),
                // The hand-off's completion pops t1 as well: not its to pop.
                Some(completion_popping(&Op::Unbind(2), &[token(1)])),
            ],
            log_shared.clone(),
        ),
        plain,
    );
    let outcome = engine.run(&mut ctx, program_with(&ops, &vec![ys, xs, one, two]));
    assert!(
        matches!(
            fault_of(outcome),
            EngineFault::EscapeRecordContract(RecordStack::Bindings)
        ),
        "a completion popping past its request is the record-contract fault"
    );
    // The harness mints every symbol one slot, so x's undo is Previous(1):
    // the serial side's restore of x writes y's value back. y's own record
    // is untouched.
    assert_eq!(
        log_shared.entries(),
        vec![
            "slot <- 1",
            "escape SaveExcursion",
            "slot <- 2",
            "escape Unbind(2) [plain]",
            "slot <- 1",
        ],
        "the serial side undid x (the hand-off); y was never restored"
    );
    let live = engine.plain_handler().live_records();
    assert_eq!(live.len(), 1, "y's record alone is live");
    assert_eq!(live[0].0, y);
}

#[test]
fn vm_a_serial_unbind_completion_that_pops_past_its_named_run_faults() {
    // `Unbind 1` over [t1, t2] names t2 alone. A completion that pops t2 and
    // t1 is still the top run, so it used to pass `serial_run_matches`; the
    // walk then had two records gone for one, and unbound past n. The
    // completion must pop exactly the tokens the request named.
    let mut ctx = context();
    let ops = plain_program(vec![
        Op::SaveExcursion,
        Op::SaveExcursion,
        Op::Unbind(1),
        Op::Nil,
    ]);
    let log_shared = EffectLog::default();
    let mut engine = harness_with_plain(
        RecordingEscape::logging_with_script(
            vec![
                Some(completion_pushing(&Op::SaveExcursion, token(1))),
                Some(completion_pushing(&Op::SaveExcursion, token(2))),
                // Innermost first: pops t2 (named) and t1 (not named).
                Some(completion_popping(&Op::Unbind(1), &[token(2), token(1)])),
            ],
            log_shared.clone(),
        ),
        HarnessPlain::new().logging(log_shared.clone()),
    );
    let outcome = engine.run(&mut ctx, program_with(&ops, &Vec::new()));
    assert!(
        matches!(
            fault_of(outcome),
            EngineFault::EscapeRecordContract(RecordStack::Bindings)
        ),
        "a completion popping past its named run is the record-contract fault"
    );
    assert_eq!(
        log_shared.entries(),
        vec![
            "escape SaveExcursion",
            "escape SaveExcursion",
            "escape Unbind(1) [2]",
        ]
    );
}

#[test]
fn vm_unbind_walks_a_record_a_serial_run_pushes_in_addition_to_n() {
    // The serial arm's push accounting (K1). `Unbind 2` over [y, S1]: the
    // run [S1] leaves as one escape whose completion pops exactly [S1] and
    // pushes S5 (the cleanup ran a `let`-like save). GNU `unbind_to`
    // (eval.c:3940-3954) walks to the fixed depth, so S5 is walked too, and
    // THEN y is restored on the owner. A walk that lets the pushed record
    // consume one of the n stops before y and leaves it bound.
    let mut ctx = context();
    let y = plain_sym("pvm-pushed-serial");
    let (ys, one) = (Value::from_sym_id(y), Value::fixnum(1));
    let ops = plain_program(vec![
        Op::Constant(1),
        Op::VarBind(0),
        Op::SaveExcursion,
        Op::Unbind(2),
        Op::Nil,
    ]);
    let log_shared = EffectLog::default();
    let plain = HarnessPlain::new().admit(y).logging(log_shared.clone());
    let mut engine = harness_with_plain(
        RecordingEscape::logging_with_script(
            vec![
                Some(completion_pushing(&Op::SaveExcursion, token(1))),
                // The run [S1] leaves; the completion pushes S5.
                Some(EscapeCompletion::Completed {
                    consumed: 0,
                    produced: Vec::new(),
                    jump: None,
                    records: RecordDelta {
                        popped: vec![token(1)],
                        pushed: vec![token(5)],
                    },
                    handlers: HandlerDelta::default(),
                }),
                Some(completion_popping(&Op::Unbind(2), &[token(5)])),
                // The Return's cleanup (DEBUG_ON_EXIT below): moves nothing.
                Some(completion(&Op::Return, vec![Value::NIL])),
            ],
            log_shared.clone(),
        ),
        plain,
    );
    // The probe between the Unbind and the Return (lead C1): with the
    // frame's DEBUG_ON_EXIT set, the Return escapes, and the LIFO at that
    // escape is the walk's post-state. An owner-local Return would undo
    // whatever a short Unbind left, with the same log and end state.
    engine.set_entry_exit_flags(ExitFlags::DEBUG_ON_EXIT);
    let outcome = engine.run(&mut ctx, program_with(&ops, &vec![ys, one]));
    assert_eq!(done_value(&mut engine, outcome), Value::NIL);
    assert_eq!(
        log_shared.entries(),
        vec![
            "slot <- 1",
            "escape SaveExcursion",
            "escape Unbind(2) [1]",
            "escape Unbind(2) [5]",
            "slot <- none",
            "escape Return",
        ],
        "S5 is walked after the run that pushed it, y is restored on the owner, and the Return's cleanup moves nothing"
    );
    let escape = engine.escape_handler();
    assert_eq!(
        escape.requests.len(),
        4,
        "record, [S1], [S5], the Return's cleanup"
    );
    assert_eq!(
        escape.lifos[3].0,
        Vec::<Option<u64>>::new(),
        "y's entry is gone by Return: the walk, not the Return, undid it"
    );
    assert!(
        engine.plain_handler().live_records().is_empty(),
        "y's entry is gone"
    );
}

#[test]
fn vm_a_signalling_plain_unbind_hand_off_leaves_no_entry_and_takes_the_record_first() {
    // O2: GNU runs an unbind take, notify, store. `do_one_unbind` takes the
    // entry off the specpdl first (eval.c:3946-3955); `set_internal` then
    // notifies the watchers with `Qunlet` (data.c:1697-1708) and stores the
    // outer value only after (the `start:` label :1716, the store :1720). So
    // when the watcher's Lisp
    // signals, the record and the executor's entry are already gone and the
    // INNER value stays in place - the store never ran. The run ends
    // Signaled and nothing is retried. Red under a pop-after walk: the
    // entry would still be there.
    let mut ctx = context();
    let x = plain_sym("pvm-watched-signals");
    let (xs, one) = (Value::from_sym_id(x), Value::fixnum(1));
    let ops = plain_program(vec![
        Op::Constant(1),
        Op::VarBind(0),
        Op::Unbind(1),
        Op::Nil,
    ]);
    let log_shared = EffectLog::default();
    let plain = HarnessPlain::new()
        .admit(x)
        .watched(x)
        .logging(log_shared.clone());
    let mut engine = harness_with_plain(
        RecordingEscape::logging_with_script(
            vec![Some(EscapeCompletion::Flow(signal(
                "error",
                vec![Value::string("unlet")],
            )))],
            log_shared.clone(),
        ),
        plain,
    );
    let outcome = engine.run(&mut ctx, program_with(&ops, &vec![xs, one]));
    assert!(
        matches!(outcome, VmOutcome::Signaled(_)),
        "the hand-off's signal ends the run"
    );
    assert_eq!(
        log_shared.entries(),
        vec!["slot <- 1", "escape Unbind(1) [plain]"],
        "the record left the slab; the watcher signaled before the store"
    );
    let escape = engine.escape_handler();
    assert_eq!(escape.requests.len(), 1);
    assert_eq!(
        escape.lifos[0].0,
        Vec::<Option<u64>>::new(),
        "the entry left before the escape; nothing is retried"
    );
    assert!(
        engine.plain_handler().live_records().is_empty(),
        "the record is gone from the slab"
    );
    assert_eq!(
        plain_state(
            &engine
                .plain_handler()
                .slot_state(TlsIndex::VOID, Channel::Default)
        ),
        "1",
        "the inner value stays: GNU stores the outer value only after the watcher"
    );
}

#[test]
fn vm_a_refused_plain_access_completion_that_pops_a_serial_token_faults() {
    // A refused Plain access (here a `let` the admission refused) pushes or
    // reads and never pops a binding record, so its completion may pop none.
    // One that pops the serial token below it is the record-contract fault,
    // nothing applied.
    let mut ctx = context();
    let y = plain_sym("pvm-refused-overpop");
    let (ys, one) = (Value::from_sym_id(y), Value::fixnum(1));
    let ops = plain_program(vec![
        Op::SaveExcursion,
        Op::Constant(1),
        Op::VarBind(0),
        Op::Nil,
    ]);
    let log_shared = EffectLog::default();
    let plain = HarnessPlain::new()
        .refuse(y, PlainRefusal::NotPlain)
        .logging(log_shared.clone());
    let mut engine = harness_with_plain(
        RecordingEscape::logging_with_script(
            vec![
                Some(completion_pushing(&Op::SaveExcursion, token(1))),
                Some(completion_popping(&Op::VarBind(0), &[token(1)])),
            ],
            log_shared.clone(),
        ),
        plain,
    );
    let outcome = engine.run(&mut ctx, program_with(&ops, &vec![ys, one]));
    assert!(
        matches!(
            fault_of(outcome),
            EngineFault::EscapeRecordContract(RecordStack::Bindings)
        ),
        "a refused access's completion popping a record is the record-contract fault"
    );
    assert_eq!(
        log_shared.entries(),
        vec!["escape SaveExcursion", "escape VarBind(0)"]
    );
}

#[test]
fn vm_a_plain_hand_off_under_a_record_the_executor_never_saw_faults_before_the_escape() {
    // O3: the record handed off must be the slab's top. The entries mirror
    // the records (C2), so the top Plain entry above the frame's base IS the
    // slab's top - unless the serial side pushed a record the executor never
    // saw, which is the one-record-two-views invariant broken. Checked
    // before anything is touched: the fault comes with NO hand-off escape
    // issued (the post-escape check would fault too, but only after the
    // serial side had been asked).
    let mut ctx = context();
    let x = plain_sym("pvm-watched-under-foreign");
    let (xs, one) = (Value::from_sym_id(x), Value::fixnum(1));
    let ops = plain_program(vec![
        Op::Constant(1),
        Op::VarBind(0),
        Op::SaveExcursion,
        Op::Unbind(2),
        Op::Nil,
    ]);
    let log_shared = EffectLog::default();
    let plain = HarnessPlain::new()
        .admit(x)
        .watched(x)
        .logging(log_shared.clone());
    let mut engine = harness_with_plain(
        RecordingEscape::logging_with_script(
            vec![
                Some(completion_pushing(&Op::SaveExcursion, token(9))),
                Some(completion_popping(&Op::Unbind(2), &[token(9)])),
            ],
            log_shared.clone(),
        )
        .pushing_foreign_record(),
        plain,
    );
    let outcome = engine.run(&mut ctx, program_with(&ops, &vec![xs, one]));
    assert!(
        matches!(
            fault_of(outcome),
            EngineFault::EscapeRecordContract(RecordStack::Bindings)
        ),
        "an entry that does not name the slab's top is the record-contract fault"
    );
    assert_eq!(
        log_shared.entries(),
        vec!["slot <- 1", "escape SaveExcursion", "escape Unbind(2) [9]"],
        "the fault comes before any hand-off escape is issued"
    );
    assert_eq!(engine.escape_handler().requests.len(), 2);
}

#[test]
fn vm_unbind_splits_into_minimal_serial_runs_in_order() {
    // R1b: each maximal run of consecutive serial records is one escape,
    // the runs in LIFO order, so an owner record between two serial records
    // is undone on the owner between the two escapes.
    let mut ctx = context();
    let x = plain_sym("pvm-runs");
    let (xs, one, two) = (Value::from_sym_id(x), Value::fixnum(1), Value::fixnum(2));
    let ops = plain_program(vec![
        Op::Constant(1),
        Op::VarBind(0),
        Op::SaveExcursion,
        Op::Constant(2),
        Op::VarBind(0),
        Op::SaveExcursion,
        Op::Unbind(4),
        Op::Nil,
    ]);
    let log = EffectLog::default();
    let plain = HarnessPlain::new().admit(x).logging(log.clone());
    let mut engine = harness_with_plain(
        RecordingEscape::logging_with_script(
            vec![
                Some(completion_pushing(&Op::SaveExcursion, token(1))),
                Some(completion_pushing(&Op::SaveExcursion, token(2))),
                Some(completion_popping(&Op::Unbind(4), &[token(2)])),
                Some(completion_popping(&Op::Unbind(4), &[token(1)])),
                // The Return's cleanup (DEBUG_ON_EXIT below): moves nothing.
                Some(completion(&Op::Return, vec![Value::NIL])),
            ],
            log.clone(),
        ),
        plain,
    );
    // The probe between the Unbind and the Return (lead C1): the Return
    // escapes and its LIFO snapshot is the walk's post-state.
    engine.set_entry_exit_flags(ExitFlags::DEBUG_ON_EXIT);
    let outcome = engine.run(&mut ctx, program_with(&ops, &[xs, one, two]));
    assert_eq!(done_value(&mut engine, outcome), Value::NIL);
    assert_eq!(
        log.entries(),
        vec![
            "slot <- 1",
            "escape SaveExcursion",
            "slot <- 2",
            "escape SaveExcursion",
            "escape Unbind(4) [2]",
            "slot <- 1",
            "escape Unbind(4) [1]",
            "slot <- none",
            "escape Return",
        ]
    );
    let escape = engine.escape_handler();
    assert_eq!(
        escape.requests.len(),
        5,
        "the pushes, the two walks, the Return's cleanup"
    );
    let unbind_pc = Pc::new(6);
    assert_eq!(escape.requests[2].op, Op::Unbind(4));
    assert_eq!(escape.requests[2].pc, unbind_pc);
    assert_eq!(
        escape.requests[2].why,
        EscapeWhy::SerialUnwind(SerialUnwind {
            pop_handlers: Vec::new(),
            unbind: vec![token(2)],
        })
    );
    assert_eq!(
        escape.requests[3].why,
        EscapeWhy::SerialUnwind(SerialUnwind {
            pop_handlers: Vec::new(),
            unbind: vec![token(1)],
        })
    );
    assert_eq!(
        escape.requests[3].pc, unbind_pc,
        "the walk is still at the opcode"
    );
    assert_eq!(escape.lifos[2].0, vec![None, Some(1), None, Some(2)]);
    assert_eq!(escape.lifos[3].0, vec![None, Some(1)]);
    assert_eq!(
        engine.poll_handler().edges,
        vec![PollEdge::SerialRequest; 5]
    );
    assert_eq!(
        escape.lifos[4].0,
        Vec::<Option<u64>>::new(),
        "empty at Return: both lets were undone by the walk"
    );
    assert_clear(&engine);
}

#[test]
fn vm_unbind_walks_a_run_of_serial_records_as_one_escape_naming_the_tokens_innermost_first() {
    // The request of a run names exactly its tokens, innermost first; a run
    // longer than the count is cut to the count.
    let mut ctx = context();
    // Two serial records, unbind 2: one escape, both tokens.
    let ops = plain_program(vec![
        Op::SaveExcursion,
        Op::SaveExcursion,
        Op::Unbind(2),
        Op::Nil,
    ]);
    let mut engine = harness_with(RecordingEscape::with_script(vec![
        Some(completion_pushing(&Op::SaveExcursion, token(1))),
        Some(completion_pushing(&Op::SaveExcursion, token(2))),
        Some(completion_popping(&Op::Unbind(2), &[token(2), token(1)])),
        None,
    ]));
    let outcome = engine.run(&mut ctx, program(&ops));
    assert_eq!(done_value(&mut engine, outcome), Value::NIL);
    let escape = engine.escape_handler();
    assert_eq!(
        escape.requests.len(),
        3,
        "the two records, the walk; Return is owner-local"
    );
    assert_eq!(
        escape.requests[2].why,
        EscapeWhy::SerialUnwind(SerialUnwind {
            pop_handlers: Vec::new(),
            unbind: vec![token(2), token(1)],
        })
    );
    assert_eq!(escape.lifos[2].0, vec![Some(1), Some(2)]);
    assert_clear(&engine);

    // The same records, unbind 1: one escape, the innermost token, and the
    // outer record stays for the frame's exit.
    let ops = plain_program(vec![
        Op::SaveExcursion,
        Op::SaveExcursion,
        Op::Unbind(1),
        Op::Nil,
    ]);
    // The count cut the run short of the outer record; it stays for the
    // Return, whose serial cleanup (R1c) pops it (V4).
    let mut engine = harness_with(RecordingEscape::with_script(vec![
        Some(completion_pushing(&Op::SaveExcursion, token(1))),
        Some(completion_pushing(&Op::SaveExcursion, token(2))),
        Some(completion_popping(&Op::Unbind(1), &[token(2)])),
        Some(completion_popping(&Op::Return, &[token(1)])),
    ]));
    let outcome = engine.run(&mut ctx, program(&ops));
    assert_eq!(done_value(&mut engine, outcome), Value::NIL);
    let escape = engine.escape_handler();
    assert_eq!(
        escape.requests.len(),
        4,
        "the two records, the walk, the Return's cleanup"
    );
    assert_eq!(
        escape.requests[2].why,
        EscapeWhy::SerialUnwind(SerialUnwind {
            pop_handlers: Vec::new(),
            unbind: vec![token(2)],
        })
    );
    assert_finished_with_records(&engine, 0);
    assert!(
        lifos(engine.arena()).0.is_empty(),
        "the Return's cleanup left no marker"
    );
}

#[test]
fn vm_unbind_zero_is_a_no_op_without_an_escape() {
    // `Bunbind0` unwinds nothing (frame_bindings.rs:95-113, the serial
    // `consume` of zero): no escape, no record leaves, the owner slot is
    // not touched.
    let mut ctx = context();
    let x = plain_sym("pvm-unbind-zero");
    let (xs, one) = (Value::from_sym_id(x), Value::fixnum(1));
    let log = EffectLog::default();
    let plain = HarnessPlain::new().admit(x).logging(log.clone());
    let mut engine = harness_with_plain(
        RecordingEscape::mirroring_logging_with_script(Vec::new(), log.clone()),
        plain,
    );
    let ops = [
        Op::Constant(1),
        Op::VarBind(0),
        Op::Unbind(0),
        Op::VarRef(0),
        Op::Return,
    ];
    let outcome = engine.run(&mut ctx, program_with(&ops, &[xs, one]));
    assert_eq!(done_value(&mut engine, outcome), one, "still bound");
    // V4: `Unbind 0` touched nothing, and the owner-local Return undid the
    // frame's binding on the way out.
    assert_eq!(log.entries(), vec!["slot <- 1", "slot <- none"]);
    assert_eq!(
        engine.escape_handler().requests.len(),
        0,
        "no escape: Return is owner-local (V4)"
    );
    assert_finished_with_records(&engine, 0);

    // Above a serial record too: the Return's serial cleanup pops the marker.
    let ops = plain_program(vec![Op::SaveExcursion, Op::Unbind(0), Op::Nil]);
    let mut engine = harness_with(RecordingEscape::with_script(vec![
        Some(completion_pushing(&Op::SaveExcursion, token(1))),
        Some(completion_popping(&Op::Return, &[token(1)])),
    ]));
    let outcome = engine.run(&mut ctx, program(&ops));
    assert_eq!(done_value(&mut engine, outcome), Value::NIL);
    assert_eq!(
        engine
            .escape_handler()
            .requests
            .iter()
            .map(|request| request.op.clone())
            .collect::<Vec<_>>(),
        vec![Op::SaveExcursion, Op::Return]
    );
    assert_finished_with_records(&engine, 0);
}

#[test]
fn vm_unbind_beyond_the_frame_records_signals_invalid_byte_code_and_unbinds_nothing() {
    // A count the frame does not own is the serial `consume`'s `TooMany`
    // (frame_bindings.rs:95-113): `Invalid byte-code`, nothing unbound, no
    // escape, before any poll.
    let mut ctx = context();
    let x = plain_sym("pvm-unbind-too-many");
    let (xs, one) = (Value::from_sym_id(x), Value::fixnum(1));
    let plain = HarnessPlain::new().admit(x);
    let mut engine = value_harness(plain);
    let ops = [Op::Constant(1), Op::VarBind(0), Op::Unbind(2), Op::Return];
    let outcome = engine.run(&mut ctx, program_with(&ops, &[xs, one]));
    let VmOutcome::Signaled(flow) = outcome else {
        panic!("got {outcome:?}")
    };
    assert_eq!(
        signal_key(&flow),
        (
            "error".to_string(),
            vec![Value::string("Invalid byte-code")]
        )
    );
    assert!(engine.escape_handler().requests.is_empty());
    assert!(engine.poll_handler().edges.is_empty());
    // V4: the signal settles the run and the idle unwind undoes the
    // frame's binding - the second write. The Unbind itself took nothing.
    assert_eq!(
        engine.plain_handler().slot_writes,
        2,
        "the bind's write, then the idle unwind's restore"
    );
    assert!(engine.is_idle());
    assert_clear(&engine);

    // An empty frame owns no record either.
    let mut engine = harness();
    let ops = [Op::Unbind(1), Op::Return];
    let outcome = engine.run(&mut ctx, program(&ops));
    let VmOutcome::Signaled(flow) = outcome else {
        panic!("got {outcome:?}")
    };
    assert_eq!(signal_key(&flow).0, "error");
    assert_clear(&engine);
}

#[test]
fn vm_an_unbind_escape_that_leaves_a_named_serial_record_faults_as_a_record_contract() {
    // The completion of an unbind escape must pop exactly the run the
    // request named. One that pops only part of it is a defect on one side
    // of the escape: the record-contract fault, and - R1, both LIFOs are
    // checked before either is touched - NOTHING applied. (The pop used to
    // be applied first and the leftover faulted after; the scene then held
    // the half-unwound LIFO.) GNU's `unbind_to` always reaches its index or
    // longjmps out; a completion that stops short is the serial side's
    // contract failure, reported before anything changes.
    let ops = plain_program(vec![
        Op::SaveExcursion,
        Op::SaveExcursion,
        Op::Unbind(2),
        Op::Nil,
    ]);
    let mut engine = harness_with(RecordingEscape::with_script(vec![
        Some(completion_pushing(&Op::SaveExcursion, token(1))),
        Some(completion_pushing(&Op::SaveExcursion, token(2))),
        Some(completion_popping(&Op::Unbind(2), &[token(2)])),
        None,
    ]));
    let outcome = engine.run(&mut context(), program(&ops));
    assert!(matches!(
        fault_of(outcome),
        EngineFault::EscapeRecordContract(RecordStack::Bindings)
    ));
    assert!(!engine.is_idle());
    assert_eq!(lifos(engine.arena()).0, vec![Some(1), Some(2)]);
    assert_eq!(engine.frame_id().is_some(), true, "the frame stays");
    assert!(matches!(
        engine.run(&mut context(), program(&[Op::Nil, Op::Return])),
        VmOutcome::Fault(EngineFault::Poisoned)
    ));
}

#[test]
fn vm_an_unbind_whose_entry_is_not_the_slab_top_faults_as_a_record_contract_and_takes_nothing() {
    // C2 (peer #11640): the executor's entry and the slab record are two
    // views of one record, paired LIFO. An entry that names a record the
    // slab does not hold on top is a defect on one side of the pairing: the
    // record-contract fault, nothing taken, the scene kept. One run: the
    // bind, a serial escape during which the serial side takes the record or
    // pushes one above it, then the Unbind. (V4: no run leaves a record
    // behind any more - the Return unwinds the frame's records - so the
    // scene cannot span two runs.)
    let mut ctx = context();
    let x = plain_sym("pvm-mismatch");
    let (xs, one) = (Value::from_sym_id(x), Value::fixnum(1));
    let ops = plain_program(vec![
        Op::Constant(1),
        Op::VarBind(0),
        Op::SaveExcursion,
        Op::Unbind(1),
        Op::Nil,
    ]);
    for (name, gone) in [
        ("the record is gone", true),
        ("a record sits above it", false),
    ] {
        let log = EffectLog::default();
        let plain = HarnessPlain::new().admit(x).logging(log.clone());
        let escape = RecordingEscape::logging_with_script(
            vec![Some(EscapeCompletion::fall_through(&Op::SaveExcursion))],
            log.clone(),
        );
        let escape = if gone {
            escape.taking_the_record()
        } else {
            escape.pushing_foreign_record()
        };
        let mut engine = harness_with_plain(escape, plain);
        let outcome = engine.run(&mut ctx, program_with(&ops, &vec![xs, one]));
        assert!(
            matches!(
                outcome,
                VmOutcome::Fault(EngineFault::EscapeRecordContract(RecordStack::Bindings))
            ),
            "{name}: got {outcome:?}"
        );
        assert!(!engine.is_idle(), "{name}: the scene is kept");
        assert_eq!(engine.frame_id().is_some(), true, "{name}: the frame stays");
        assert_eq!(
            lifos(engine.arena()).0,
            vec![None],
            "{name}: the executor's entry stays"
        );
        assert_eq!(
            log.entries(),
            vec!["slot <- 1", "escape SaveExcursion"],
            "{name}: the Unbind restored nothing"
        );
        let accesses = engine.plain_handler().accesses();
        assert_eq!(accesses.1, 1, "{name}: the bind's write alone");
        assert_eq!(
            engine.escape_handler().requests.len(),
            1,
            "{name}: no hand-off escape"
        );
        assert_eq!(
            engine.poll_handler().edges,
            vec![PollEdge::SerialRequest],
            "{name}: the save's poll alone"
        );
        assert_eq!(
            engine.plain_handler().live_records().len(),
            if gone { 0 } else { 2 },
            "{name}: the slab is as the serial side left it"
        );
        assert!(
            matches!(
                engine.run(&mut ctx, program(&[Op::Nil, Op::Return])),
                VmOutcome::Fault(EngineFault::Poisoned)
            ),
            "{name}: the next run is refused"
        );
    }
}

/// The serial VM's answer to `ops` over `constants`, through the
/// hand-assembled function of `thread_projection_readers_test.rs`.
fn serial_bytecode(ctx: &mut Context, ops: &[Op], constants: &[Value]) -> EvalResult {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: Vec::new(),
        optional: Vec::new(),
        rest: None,
    });
    f.set_lexical(true);
    f.set_ops(ops.to_vec());
    f.set_constants(constants.to_vec().into());
    f.set_max_stack(StackDepth::for_test(8));
    Vm::from_context(ctx).execute(&f, vec![])
}

#[test]
fn vm_unbind_ties_the_serial_vm_on_the_same_programs() {
    // GNU `Bunbind n` counts every specpdl entry the frame pushed and
    // unwinds to the n-th (bytecode.c:843-851, eval.c:3946-3957): the serial
    // frame's `bind_stack` marks a `specbind`, a `save-excursion` and an
    // unwind-protect alike (bytecode/vm.rs:5722-5734, 6158-6162, 7176-7186;
    // frame_bindings.rs:95-113). The same programs run through the serial
    // VM and through the stepper, whose serial side is scripted to push and
    // pop the same records: both answer alike. (Neomacs against Neomacs: a
    // harness tie, not an oracle run.)
    let mut ctx = context();
    ctx.eval_str("(defvar pvm-tie-x 10) (defvar pvm-tie-y 20)")
        .expect("the fixture defines");
    let x = plain_sym("pvm-tie-x");
    let never = plain_sym("pvm-tie-never");
    let (xs, never_s) = (Value::from_sym_id(x), Value::from_sym_id(never));
    let (one, two) = (Value::fixnum(1), Value::fixnum(2));
    let ten = Value::fixnum(10);
    // (name, ops, constants, port, script, the stepper's x after, the owner
    // entries the Return's cleanup finds on the LIFO: what the program left
    // for it. The Unbind rows' probe (lead C1) - the Return escapes
    // (DEBUG_ON_EXIT) and its snapshot must equal exactly this, so a walk
    // that stops short is caught before the cleanup can undo it.)
    let cases: Vec<(
        &str,
        Vec<Op>,
        Vec<Value>,
        HarnessPlain,
        Vec<Option<EscapeCompletion>>,
        &str,
        usize,
    )> = vec![
        (
            "let, save-excursion, let, unbind 3",
            vec![
                Op::Constant(1),
                Op::VarBind(0),
                Op::SaveExcursion,
                Op::Constant(2),
                Op::VarBind(0),
                Op::Unbind(3),
                Op::Nil,
                Op::Return,
            ],
            vec![xs, one, two],
            HarnessPlain::new().admit(x).with_canonical(x, ten),
            vec![
                Some(completion_pushing(&Op::SaveExcursion, token(9))),
                Some(completion_popping(&Op::Unbind(3), &[token(9)])),
                None,
            ],
            "10",
            0,
        ),
        (
            "let, unwind-protect, let, unbind 3",
            vec![
                Op::Constant(1),
                Op::VarBind(0),
                Op::Constant(2),
                Op::UnwindProtectPop,
                Op::Constant(3),
                Op::VarBind(0),
                Op::Unbind(3),
                Op::Nil,
                Op::Return,
            ],
            // The cleanup form is evaluated by the serial unbind (specpdl.rs
            // pops UnwindProtect with LispCleanup), so it is a form: nil.
            vec![xs, one, Value::NIL, two],
            HarnessPlain::new().admit(x).with_canonical(x, ten),
            vec![
                Some(completion_pushing(&Op::UnwindProtectPop, token(7))),
                Some(completion_popping(&Op::Unbind(3), &[token(7)])),
                None,
            ],
            "10",
            0,
        ),
        (
            "two lets, unbind 2, read",
            vec![
                Op::Constant(1),
                Op::VarBind(0),
                Op::Constant(2),
                Op::VarBind(0),
                Op::Unbind(2),
                Op::VarRef(0),
                Op::Return,
            ],
            vec![xs, one, two],
            HarnessPlain::new().admit(x).with_canonical(x, ten),
            vec![None],
            "10",
            0,
        ),
        (
            "unbind 0 leaves the binding",
            vec![
                Op::Constant(1),
                Op::VarBind(0),
                Op::Unbind(0),
                Op::VarRef(0),
                Op::Return,
            ],
            // `Unbind 0` is a no-op: x is still bound when the Return comes,
            // and the frame's cleanup undoes it - x back at canonical, the
            // one owner entry at the Return's snapshot.
            vec![xs, one],
            HarnessPlain::new().admit(x).with_canonical(x, ten),
            vec![None],
            "10",
            1,
        ),
        (
            "unbind a void binding, read",
            vec![
                Op::Constant(1),
                Op::VarBind(0),
                Op::Unbind(1),
                Op::VarRef(0),
                Op::Return,
            ],
            vec![never_s, one],
            HarnessPlain::new().admit(never),
            vec![None],
            "unbound",
            0,
        ),
        (
            "unbind 2 over one let",
            vec![Op::Constant(1), Op::VarBind(0), Op::Unbind(2), Op::Return],
            vec![xs, one],
            HarnessPlain::new().admit(x).with_canonical(x, ten),
            vec![],
            "",
            0,
        ),
        (
            "unbind 1 over nothing",
            vec![Op::Unbind(1), Op::Return],
            vec![xs],
            HarnessPlain::new().admit(x).with_canonical(x, ten),
            vec![],
            "",
            0,
        ),
    ];
    for (name, ops, constants, plain, script, after, left) in cases {
        // The serial VM: the frame leaves no record behind, and x is back.
        let base = ctx.specpdl.len();
        let want = serial_bytecode(&mut ctx, &ops, &constants);
        assert_eq!(
            ctx.specpdl.len(),
            base,
            "{name}: the serial frame's records left"
        );
        let x_after = ctx
            .eval_str("pvm-tie-x")
            .expect("x is bound after the serial run");
        assert_eq!(
            plain_value(&x_after),
            "10",
            "{name}: the serial VM restored x"
        );
        // The stepper.
        let mut engine = harness_with_plain(RecordingEscape::mirroring_with_script(script), plain);
        // The probe between the Unbind and the Return (lead C1): the Return
        // escapes and its LIFO snapshot is the walk's post-state.
        engine.set_entry_exit_flags(ExitFlags::DEBUG_ON_EXIT);
        let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
        let got = outcome_result(&mut engine, outcome);
        // A signal is compared by its key; a value by its printed form.
        match (&got, &want) {
            (Err(got), Err(want)) => {
                assert_eq!(signal_key(got), signal_key(want), "{name}: the same signal")
            }
            _ => assert_eq!(printed(&got), printed(&want), "{name}: the same answer"),
        }
        if after.is_empty() {
            // A signal: `unwind_to_idle` drops the frame and undoes its
            // records (V4), so x is back at its canonical value here too.
            assert!(engine.is_idle(), "{name}");
            assert_eq!(
                engine.plain_handler().observed(x),
                "10",
                "{name}: restored by the idle unwind"
            );
            assert_clear(&engine);
        } else {
            assert_eq!(
                engine.plain_handler().observed(x),
                after,
                "{name}: the slot after the run"
            );
            assert_finished_with_records(&engine, 0);
            // The probe holds for a run that reached its Return (a signaled
            // run's idle unwind undoes everything, which observed(x) pins).
            if got.is_ok() {
                let escape = engine.escape_handler();
                assert_eq!(
                    escape.requests.last().map(|request| request.op.clone()),
                    Some(Op::Return),
                    "{name}: the Return's cleanup escaped"
                );
                assert_eq!(
                    escape.lifos.last().map(|lifo| lifo.0.clone()),
                    Some(vec![None; left]),
                    "{name}: the Return found exactly what the program left, no more"
                );
            }
        }
    }
}

// ----------------------------------------- V5 real-era poll wiring rows

const OWNER_B: u64 = OWNER + 10;

fn era_constants(words: &[usize]) -> Vec<Value> {
    words
        .iter()
        // SAFETY: every word in `words` was captured from a `Value` allocated
        // on this era's heap during `era_vm_setup` and stays rooted until the
        // owner has loaded it into its published operand segment.
        .map(|&bits| unsafe { Value::from_bits(bits) })
        .collect()
}

fn retire_with_roots(mut owner: Mutator<'_>, roots: &[SharedRoot]) {
    let start = Instant::now();
    let deadline = Duration::from_secs(30);
    loop {
        match owner.retire_rooted(roots) {
            Ok(()) => return,
            Err(failure)
                if failure.reason == RendezvousError::Busy && start.elapsed() < deadline =>
            {
                owner = failure.into_owner();
                thread::yield_now();
            }
            Err(failure) => panic!("retire_with_roots: {:?}", failure.reason),
        }
    }
}

fn retry_reg<T>(label: &str, mut op: impl FnMut() -> Result<T, RegistrationError>) -> T {
    let start = Instant::now();
    let deadline = Duration::from_secs(30);
    loop {
        match op() {
            Ok(value) => return value,
            Err(RegistrationError::Busy) if start.elapsed() < deadline => thread::yield_now(),
            Err(error) => panic!("{label}: {error:?}"),
        }
    }
}

fn era_owns(
    era: &crate::emacs_core::eval::world::WorldProofEra<'_>,
    stopped: &neovm_value::tagged::gc::AllStopped<'_>,
    bits: usize,
) -> bool {
    era.with_stopped_heap_for_test(stopped, |heap| {
        // SAFETY: `bits` is the tagged word of a value allocated on this era's heap.
        heap.owns_heap_value_for_test(unsafe { Value::from_bits(bits) })
    })
    .expect("stopped heap")
}

struct EraVmFixture {
    heap: usize,
    nil_root: SharedRoot,
    words: Vec<usize>,
    roots: Vec<SharedRoot>,
}

fn era_vm_setup(
    context: &mut Context,
    build: impl FnOnce(&mut Context) -> Vec<Value>,
) -> EraVmFixture {
    context.set_gc_stress_for_test(false);
    context.tagged_heap.set_gc_threshold(usize::MAX);
    context.tagged_heap.close_alloc_regions();
    let values = build(context);
    // SAFETY: setup is the sole serial mutator on this era's Context.
    let nil_root = unsafe { SharedRoot::new(&context.tagged_heap, Value::NIL) };
    let mut words = Vec::with_capacity(values.len());
    let mut roots = Vec::with_capacity(values.len());
    for value in values {
        words.push(value.bits());
        // SAFETY: same sole-mutator setup; roots keep constants alive until
        // the test drops them after the owner publishes its operand segment.
        roots.push(unsafe { SharedRoot::new(&context.tagged_heap, value) });
    }
    EraVmFixture {
        heap: context.tagged_heap.heap_root().as_ptr() as usize,
        nil_root,
        words,
        roots,
    }
}

fn finish_with_cons_storage(era: crate::emacs_core::eval::world::WorldProofEra<'_>) {
    let stopped = retry("final stop", || stop_for_test(&era));
    retry_reg("final return cons storage", || {
        era.return_cons_storage(&stopped)
    });
    let mut ticket = lend(&era, stopped);
    collect(&mut ticket);
    end_epoch(ticket);
    finish(era);
}

struct VmEraGate {
    edge: PollEdge,
    skip: usize,
    reached: mpsc::Sender<()>,
    go: mpsc::Receiver<()>,
}

/// The real V5 poll wiring behind [`VmPoll`]: calls `poll_vm_back_edge`,
/// `poll_vm_call`, `poll_serial_request` and `poll_region_refill` against a
/// real [`Mutator`], and records every published `publish_scan` batch and
/// refill [`NumericClass`] so rows assert directly on the published words
/// and requested class. Close-before-publish is structural in `neovm-value`:
/// `RunAccess` borrows `&mut Mutator`, its `Drop` poisons the registry on any
/// unclosed run, and `publish_roots` refuses `Loan` while admitted.
struct VmEraPoll<'era> {
    owner: Rc<RefCell<Option<Mutator<'era>>>>,
    heap: usize,
    rendezvous: Rendezvous<'era>,
    edges: Vec<PollEdge>,
    refill_classes: Vec<NumericClass>,
    published_batches: Vec<Vec<usize>>,
    published_depths: Vec<u32>,
    build_calls: usize,
    back_edge_builds: usize,
    call_builds: usize,
    serial_builds: usize,
    refill_builds: usize,
    gate: Option<VmEraGate>,
    pre_refill_hook: Option<Box<dyn FnOnce()>>,
    refill_publish_hook: Option<Box<dyn FnOnce()>>,
    post_refill_hook: Option<Box<dyn FnOnce()>>,
}

impl<'era> VmEraPoll<'era> {
    fn new(owner: Mutator<'era>, heap: usize, rendezvous: Rendezvous<'era>) -> Self {
        Self {
            owner: Rc::new(RefCell::new(Some(owner))),
            heap,
            rendezvous,
            edges: Vec::new(),
            refill_classes: Vec::new(),
            published_batches: Vec::new(),
            published_depths: Vec::new(),
            build_calls: 0,
            back_edge_builds: 0,
            call_builds: 0,
            serial_builds: 0,
            refill_builds: 0,
            gate: None,
            pre_refill_hook: None,
            refill_publish_hook: None,
            post_refill_hook: None,
        }
    }

    fn owner_handle(&self) -> Rc<RefCell<Option<Mutator<'era>>>> {
        self.owner.clone()
    }

    fn take_owner(&self) -> Mutator<'era> {
        self.owner
            .borrow_mut()
            .take()
            .expect("owner still resident")
    }

    fn with_gate(
        mut self,
        edge: PollEdge,
        skip: usize,
        reached: mpsc::Sender<()>,
        go: mpsc::Receiver<()>,
    ) -> Self {
        self.gate = Some(VmEraGate {
            edge,
            skip,
            reached,
            go,
        });
        self
    }

    fn with_pre_refill_hook(mut self, hook: impl FnOnce() + 'static) -> Self {
        self.pre_refill_hook = Some(Box::new(hook));
        self
    }

    fn with_refill_publish_hook(mut self, hook: impl FnOnce() + 'static) -> Self {
        self.refill_publish_hook = Some(Box::new(hook));
        self
    }

    fn with_post_refill_hook(mut self, hook: impl FnOnce() + 'static) -> Self {
        self.post_refill_hook = Some(Box::new(hook));
        self
    }

    fn run_gate(&mut self, edge: PollEdge) {
        let Some(gate) = self.gate.as_mut() else {
            return;
        };
        if gate.edge != edge {
            return;
        }
        if gate.skip > 0 {
            gate.skip -= 1;
            return;
        }
        let Some(gate) = self.gate.take() else {
            return;
        };
        gate.reached.send(()).expect("gate reached");
        recv(&gate.go);
    }

    fn publish_arena(
        arena: &FrameArena,
        heap: usize,
        published_batches: &mut Vec<Vec<usize>>,
        published_depths: &mut Vec<u32>,
        build_calls: &mut usize,
        edge_builds: &mut usize,
    ) -> Result<Vec<SharedRoot>, RendezvousError> {
        *build_calls += 1;
        *edge_builds += 1;
        published_depths.push(arena.depth());
        // SAFETY: `heap` is the era provenance root captured before
        // admission. `publish_scan` only reads the arena, and the keeper
        // cannot sweep while this owner is inside `Mutator::poll`.
        let heap = unsafe { &*(heap as *const TaggedHeap) };
        let mut batch = Vec::new();
        arena.publish_scan(heap, &mut batch)?;
        published_batches.push(batch.iter().map(SharedRoot::parallel_word).collect());
        Ok(batch)
    }
}

impl VmPoll for VmEraPoll<'_> {
    fn vm_back_edge(&mut self, arena: &FrameArena) -> Result<PollOutcome, RendezvousError> {
        self.edges.push(PollEdge::VmBackEdge);
        let heap = self.heap;
        let published = &mut self.published_batches;
        let depths = &mut self.published_depths;
        let builds = &mut self.build_calls;
        let edge_builds = &mut self.back_edge_builds;
        let outcome = {
            let mut guard = self.owner.borrow_mut();
            let owner = guard.as_mut().expect("owner resident");
            poll_vm_back_edge(owner, || {
                Self::publish_arena(arena, heap, published, depths, builds, edge_builds)
            })
        };
        self.run_gate(PollEdge::VmBackEdge);
        outcome
    }

    fn vm_call(&mut self, arena: &FrameArena) -> Result<PollOutcome, RendezvousError> {
        self.edges.push(PollEdge::VmCall);
        let heap = self.heap;
        let published = &mut self.published_batches;
        let depths = &mut self.published_depths;
        let builds = &mut self.build_calls;
        let edge_builds = &mut self.call_builds;
        let outcome = {
            let mut guard = self.owner.borrow_mut();
            let owner = guard.as_mut().expect("owner resident");
            poll_vm_call(owner, || {
                Self::publish_arena(arena, heap, published, depths, builds, edge_builds)
            })
        };
        self.run_gate(PollEdge::VmCall);
        outcome
    }

    fn serial_request(&mut self, arena: &FrameArena) -> Result<PollOutcome, RendezvousError> {
        self.edges.push(PollEdge::SerialRequest);
        let heap = self.heap;
        let published = &mut self.published_batches;
        let depths = &mut self.published_depths;
        let builds = &mut self.build_calls;
        let edge_builds = &mut self.serial_builds;
        let outcome = {
            let mut guard = self.owner.borrow_mut();
            let owner = guard.as_mut().expect("owner resident");
            poll_serial_request(owner, || {
                Self::publish_arena(arena, heap, published, depths, builds, edge_builds)
            })
        };
        self.run_gate(PollEdge::SerialRequest);
        outcome
    }

    fn vm_refill(
        &mut self,
        arena: &FrameArena,
        class: NumericClass,
    ) -> Result<PollOutcome, RendezvousError> {
        self.edges.push(PollEdge::VmRefill);
        self.refill_classes.push(class);
        if let Some(hook) = self.pre_refill_hook.take() {
            hook();
        }
        let heap = self.heap;
        let rendezvous = self.rendezvous;
        let published = &mut self.published_batches;
        let depths = &mut self.published_depths;
        let builds = &mut self.build_calls;
        let edge_builds = &mut self.refill_builds;
        let mut publish_hook = self.refill_publish_hook.take();
        let outcome = {
            let mut guard = self.owner.borrow_mut();
            let owner = guard.as_mut().expect("owner resident");
            poll_region_refill(owner, rendezvous, || {
                if let Some(hook) = publish_hook.take() {
                    hook();
                }
                Self::publish_arena(arena, heap, published, depths, builds, edge_builds)
            })
        };
        if let Some(hook) = self.post_refill_hook.take() {
            hook();
        }
        self.run_gate(PollEdge::VmRefill);
        outcome
    }
}

/// Real-era arithmetic port backed by `RunAccess::float_into` and
/// `RunAccess::close_all_runs` (gcpace N2). Operand checks precede any
/// allocation; an empty pool maps `NumericAllocError::Exhausted` directly
/// to `ArithRefusal::Exhausted` with nothing consumed, and registration or
/// poison failures map to `ArithRefusal::Fault` (never a Lisp signal, M1).
struct EraArith<'era> {
    owner: Rc<RefCell<Option<Mutator<'era>>>>,
    nil_root: SharedRoot,
    result_slot: SharedRoot,
    calls: Vec<(ArithGenericKind, Vec<Value>)>,
    float_allocs: Vec<f64>,
    /// Test-only defect injection (M4): when true, skips `close_all_runs()`
    /// after claiming a float so `Drop for RunAccess` observes an open run
    /// and poisons the era.
    leave_run_open: bool,
}

impl<'era> EraArith<'era> {
    fn new(owner: Rc<RefCell<Option<Mutator<'era>>>>, nil_root: SharedRoot) -> Self {
        Self {
            owner,
            result_slot: nil_root.clone(),
            nil_root,
            calls: Vec::new(),
            float_allocs: Vec::new(),
            leave_run_open: false,
        }
    }

    fn leaving_run_open(mut self) -> Self {
        self.leave_run_open = true;
        self
    }

    fn alloc_float(&mut self, sum: f64) -> Result<Value, ArithRefusal> {
        let mut guard = self.owner.borrow_mut();
        let owner = guard.as_mut().expect("owner resident");
        // After a multi-owner stop completes, the first waking owner can observe
        // `Phase::DrainingResumes` (`RegistrationError::Busy`) until its peers
        // finish `restore_wait`. Wait for `Phase::Idle` before claiming from the
        // numeric pool rather than mistaking resume drain for exhaustion.
        let start = Instant::now();
        let deadline = Duration::from_secs(30);
        let mut run = loop {
            match owner.run_access() {
                Ok(run) => break run,
                Err(RegistrationError::Busy) if start.elapsed() < deadline => thread::yield_now(),
                Err(error) => return Err(ArithRefusal::Fault(error)),
            }
        };
        match run.float_into(sum, &mut self.result_slot) {
            Ok(()) => {
                let bits = self.result_slot.parallel_word();
                if !self.leave_run_open {
                    run.close_all_runs().map_err(ArithRefusal::Fault)?;
                }
                drop(run);
                // Clear the scratch SharedRoot so only FrameArena's operand
                // segment roots the allocated float at the next safepoint.
                self.result_slot = self.nil_root.clone();
                self.float_allocs.push(sum);
                // SAFETY: `bits` is the tagged word of the float just
                // initialized in this era's numeric storage.
                Ok(unsafe { Value::from_bits(bits) })
            }
            Err(NumericAllocError::Exhausted(class)) => {
                run.close_all_runs().map_err(ArithRefusal::Fault)?;
                Err(ArithRefusal::Exhausted(class))
            }
            Err(NumericAllocError::Registration(error)) => {
                let _ = run.close_all_runs();
                Err(ArithRefusal::Fault(error))
            }
        }
    }
}

impl ArithOps for EraArith<'_> {
    fn arith(&mut self, kind: ArithGenericKind, operands: &[Value]) -> Result<Value, ArithRefusal> {
        self.calls.push((kind, operands.to_vec()));
        let integer_only = kind == ArithGenericKind::Rem;
        let predicate = if integer_only {
            "integer-or-marker-p"
        } else {
            "number-or-marker-p"
        };
        for value in operands {
            if value.is_marker() {
                return Err(ArithRefusal::MarkerInvolved);
            }
            if !(value.is_fixnum() || value.is_bignum() || (value.is_float() && !integer_only)) {
                return Err(ArithRefusal::Flow(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol(predicate), *value],
                )));
            }
        }
        let integers = operands
            .iter()
            .all(|value| value.is_fixnum() || value.is_bignum());
        if integers && kind.integer_op().is_some() {
            return Vm::arith_integer_fast(kind, operands).ok_or_else(unsupported);
        }
        match (kind, operands) {
            (ArithGenericKind::Add, [a, b]) => self.alloc_float(number(a)? + number(b)?),
            (ArithGenericKind::Sub, [a, b]) => self.alloc_float(number(a)? - number(b)?),
            (ArithGenericKind::Add1, [a]) => self.alloc_float(1.0 + number(a)?),
            (ArithGenericKind::Sub1, [a]) => self.alloc_float(-1.0 + number(a)?),
            _ => Err(unsupported()),
        }
    }
}

#[test]
fn vm_wired_fast_path_builds_no_batch() {
    // VM-EXECUTOR-DESIGN §9 V5 / §10: an owner-local backward-branch loop on
    // an admissible phase word calls `poll_vm_back_edge` once per taken back
    // edge and never invokes the slow-path `publish_scan` closure.
    Context::with_test_collection_era(
        |context| {
            era_vm_setup(context, |_ctx| {
                let list = Value::cons(
                    Value::NIL,
                    Value::cons(
                        Value::NIL,
                        Value::cons(Value::NIL, Value::cons(Value::NIL, Value::NIL)),
                    ),
                );
                vec![Value::fixnum(10), list]
            })
        },
        |era, fixture| {
            let permit = era.register(OWNER, 0).expect("register");
            let heap = era.heap_identity();
            let rendezvous = era.rendezvous();
            let heap_addr = fixture.heap;
            let words = fixture.words.clone();
            thread::scope(|scope| {
                scope.spawn(move || {
                    let owner = activate(permit, heap, OWNER);
                    let mut ctx = Context::new(unit_runtime());
                    let constants = era_constants(&words);
                    let ops = vec![
                        Op::Constant(0),
                        Op::Constant(1),
                        Op::Cdr,
                        Op::Dup,
                        Op::GotoIfNotNil(2),
                        Op::Pop,
                        Op::Return,
                    ];
                    let mut engine = VmExec::new(
                        &(),
                        RecordingEscape::new(),
                        HarnessVmSealed,
                        HarnessArith::new(),
                        HarnessPlain::new(),
                        VmEraPoll::new(owner, heap_addr, rendezvous),
                    );
                    let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
                    let done = done_value(&mut engine, outcome);
                    assert_eq!(done, Value::fixnum(10));
                    assert_eq!(engine.escape_handler().requests.len(), 0);
                    let poll = engine.into_poll();
                    assert_eq!(poll.edges, vec![PollEdge::VmBackEdge; 3]);
                    assert_eq!(
                        poll.build_calls, 0,
                        "the admissible fast path must not build a publish batch"
                    );
                    assert_eq!(poll.back_edge_builds, 0);
                    retire(poll.take_owner());
                });
            });
            drop(fixture);
            finish(era);
        },
    );
}

#[test]
fn vm_wired_back_edge_publishes_the_segment_and_parks() {
    // VM-EXECUTOR-DESIGN §9 V5 / §10: `poll_vm_back_edge` parks against the
    // real keeper; the published batch contains the live operand cons (7 . 9);
    // setup drops its own SharedRoots BEFORE `collect` so ONLY the published
    // VM operand segment keeps the cons alive through GC; after resume the
    // frame continues and returns the identical bits with car/cdr intact.
    Context::with_test_collection_era(
        |context| {
            era_vm_setup(context, |_ctx| {
                let payload = Value::cons(Value::fixnum(7), Value::fixnum(9));
                let list = Value::cons(
                    Value::NIL,
                    Value::cons(Value::NIL, Value::cons(Value::NIL, Value::NIL)),
                );
                vec![payload, list]
            })
        },
        |era, mut fixture| {
            let permit = era.register(OWNER, 0).expect("register");
            let heap = era.heap_identity();
            let rendezvous = era.rendezvous();
            let heap_addr = fixture.heap;
            let words = fixture.words.clone();
            let payload_bits = words[0];
            let (gate_tx, gate_rx) = mpsc::channel();
            let (gate_go_tx, gate_go_rx) = mpsc::channel();
            let (done_tx, done_rx) = mpsc::channel();
            thread::scope(|scope| {
                scope.spawn(move || {
                    let owner = activate(permit, heap, OWNER);
                    let mut ctx = Context::new(unit_runtime());
                    let constants = era_constants(&words);
                    let ops = vec![
                        Op::Constant(0),
                        Op::Constant(1),
                        Op::Cdr,
                        Op::Dup,
                        Op::GotoIfNotNil(2),
                        Op::Pop,
                        Op::Return,
                    ];
                    let mut engine = VmExec::new(
                        &(),
                        RecordingEscape::new(),
                        HarnessVmSealed,
                        HarnessArith::new(),
                        HarnessPlain::new(),
                        VmEraPoll::new(owner, heap_addr, rendezvous).with_gate(
                            PollEdge::VmBackEdge,
                            0,
                            gate_tx,
                            gate_go_rx,
                        ),
                    );
                    let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
                    let done = done_value(&mut engine, outcome);
                    let done_bits = done.bits();
                    let car_cdr = (done.cons_car().as_fixnum(), done.cons_cdr().as_fixnum());
                    let poll = engine.into_poll();
                    done_tx
                        .send((
                            done_bits,
                            car_cdr,
                            poll.edges.clone(),
                            poll.build_calls,
                            poll.back_edge_builds,
                            poll.published_batches.clone(),
                        ))
                        .expect("done report");
                    retire(poll.take_owner());
                });
                recv(&gate_rx);
                request_stop(rendezvous);
                gate_go_tx.send(()).expect("gate go");
                let stopped = retry("stop", || stop_for_test(&era));
                // Drop setup's SharedRoots before collection so only the VM's
                // published operand segment roots the live payload cons.
                fixture.roots.clear();
                let mut ticket = lend(&era, stopped);
                collect(&mut ticket);
                assert!(
                    era_owns(&era, ticket.stopped_proof(), payload_bits),
                    "the published VM operand must survive real GC"
                );
                end_epoch(ticket);
                let (done_bits, car_cdr, edges, builds, back_edge_builds, batches) = recv(&done_rx);
                assert_eq!(done_bits, payload_bits, "eq by bits after resume");
                assert_eq!(car_cdr, (Some(7), Some(9)));
                assert_eq!(edges, vec![PollEdge::VmBackEdge; 2]);
                assert_eq!(builds, 1);
                assert_eq!(back_edge_builds, 1);
                assert_eq!(batches.len(), 1);
                assert!(
                    batches[0].contains(&payload_bits),
                    "the published batch must contain the live operand cons"
                );
            });
            drop(fixture);
            finish(era);
        },
    );
}

#[test]
fn vm_wired_call_edge_order_is_vm_call_then_serial_request() {
    // VM-EXECUTOR-DESIGN §9 V5 / §10: `Op::Call(1)` runs `poll_vm_call` then
    // `poll_serial_request`. With a stop active before the call, `poll_vm_call`
    // parks, publishes the live operand cons (11 . 13), real GC runs with
    // setup roots dropped, then after resume `poll_serial_request` runs fast
    // and the escape receives the surviving operand.
    Context::with_test_collection_era(
        |context| {
            era_vm_setup(context, |_ctx| {
                let payload = Value::cons(Value::fixnum(11), Value::fixnum(13));
                vec![Value::fixnum(1), payload]
            })
        },
        |era, mut fixture| {
            let permit = era.register(OWNER, 0).expect("register");
            let heap = era.heap_identity();
            let rendezvous = era.rendezvous();
            let heap_addr = fixture.heap;
            let words = fixture.words.clone();
            let payload_bits = words[1];
            let (ready_tx, ready_rx) = mpsc::channel();
            let (go_tx, go_rx) = mpsc::channel();
            let (done_tx, done_rx) = mpsc::channel();
            thread::scope(|scope| {
                scope.spawn(move || {
                    let owner = activate(permit, heap, OWNER);
                    let mut ctx = Context::new(unit_runtime());
                    let constants = era_constants(&words);
                    let ops = vec![Op::Constant(0), Op::Constant(1), Op::Call(1), Op::Return];
                    let escape = RecordingEscape::with_script(vec![Some(completion(
                        &Op::Call(1),
                        vec![constants[1]],
                    ))]);
                    let mut engine = VmExec::new(
                        &(),
                        escape,
                        HarnessVmSealed,
                        HarnessArith::new(),
                        HarnessPlain::new(),
                        VmEraPoll::new(owner, heap_addr, rendezvous),
                    );
                    ready_tx.send(()).expect("ready");
                    recv(&go_rx);
                    let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
                    let done = done_value(&mut engine, outcome);
                    let done_bits = done.bits();
                    let car_cdr = (done.cons_car().as_fixnum(), done.cons_cdr().as_fixnum());
                    let escaped_seg_bits: Vec<usize> = engine.escape_handler().segments[0]
                        .iter()
                        .map(|v| v.bits())
                        .collect();
                    let poll = engine.into_poll();
                    done_tx
                        .send((
                            done_bits,
                            car_cdr,
                            escaped_seg_bits,
                            poll.edges.clone(),
                            poll.build_calls,
                            poll.call_builds,
                            poll.serial_builds,
                            poll.published_batches.clone(),
                        ))
                        .expect("done report");
                    retire(poll.take_owner());
                });
                recv(&ready_rx);
                request_stop(rendezvous);
                go_tx.send(()).expect("go");
                let stopped = retry("stop", || stop_for_test(&era));
                fixture.roots.clear();
                let mut ticket = lend(&era, stopped);
                collect(&mut ticket);
                assert!(
                    era_owns(&era, ticket.stopped_proof(), payload_bits),
                    "the published call operand must survive real GC"
                );
                end_epoch(ticket);
                let (
                    done_bits,
                    car_cdr,
                    escaped_seg_bits,
                    edges,
                    builds,
                    call_builds,
                    serial_builds,
                    batches,
                ) = recv(&done_rx);
                assert_eq!(done_bits, payload_bits);
                assert_eq!(car_cdr, (Some(11), Some(13)));
                assert!(escaped_seg_bits.contains(&payload_bits));
                assert_eq!(edges, vec![PollEdge::VmCall, PollEdge::SerialRequest]);
                assert_eq!(builds, 1);
                assert_eq!(call_builds, 1, "poll_vm_call parked");
                assert_eq!(serial_builds, 0, "poll_serial_request stayed fast");
                assert_eq!(batches.len(), 1);
                assert!(batches[0].contains(&payload_bits));
            });
            drop(fixture);
            finish(era);
        },
    );
}

#[test]
fn vm_wired_serial_request_publishes_the_segment_and_parks() {
    // VM-EXECUTOR-DESIGN §10: a non-call World escape parks inside
    // `poll_serial_request`, publishes the live operand cons (17 . 19), keeps
    // it alive across real GC with setup roots dropped, and hands the
    // identical bits to the escape handler after resume.
    Context::with_test_collection_era(
        |context| {
            era_vm_setup(context, |_ctx| {
                let payload = Value::cons(Value::fixnum(17), Value::fixnum(19));
                vec![payload]
            })
        },
        |era, mut fixture| {
            let permit = era.register(OWNER, 0).expect("register");
            let heap = era.heap_identity();
            let rendezvous = era.rendezvous();
            let heap_addr = fixture.heap;
            let words = fixture.words.clone();
            let payload_bits = words[0];
            let (ready_tx, ready_rx) = mpsc::channel();
            let (go_tx, go_rx) = mpsc::channel();
            let (done_tx, done_rx) = mpsc::channel();
            thread::scope(|scope| {
                scope.spawn(move || {
                    let owner = activate(permit, heap, OWNER);
                    let mut ctx = Context::new(unit_runtime());
                    let constants = era_constants(&words);
                    let ops = vec![Op::Constant(0), Op::Concat(1), Op::Return];
                    let escape = RecordingEscape::with_script(vec![Some(completion(
                        &Op::Concat(1),
                        vec![constants[0]],
                    ))]);
                    let mut engine = VmExec::new(
                        &(),
                        escape,
                        HarnessVmSealed,
                        HarnessArith::new(),
                        HarnessPlain::new(),
                        VmEraPoll::new(owner, heap_addr, rendezvous),
                    );
                    ready_tx.send(()).expect("ready");
                    recv(&go_rx);
                    let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
                    let done = done_value(&mut engine, outcome);
                    let done_bits = done.bits();
                    let car_cdr = (done.cons_car().as_fixnum(), done.cons_cdr().as_fixnum());
                    let poll = engine.into_poll();
                    done_tx
                        .send((
                            done_bits,
                            car_cdr,
                            poll.edges.clone(),
                            poll.build_calls,
                            poll.serial_builds,
                            poll.published_batches.clone(),
                        ))
                        .expect("done report");
                    retire(poll.take_owner());
                });
                recv(&ready_rx);
                request_stop(rendezvous);
                go_tx.send(()).expect("go");
                let stopped = retry("stop", || stop_for_test(&era));
                fixture.roots.clear();
                let mut ticket = lend(&era, stopped);
                collect(&mut ticket);
                assert!(
                    era_owns(&era, ticket.stopped_proof(), payload_bits),
                    "the published escape operand must survive real GC"
                );
                end_epoch(ticket);
                let (done_bits, car_cdr, edges, builds, serial_builds, batches) = recv(&done_rx);
                assert_eq!(done_bits, payload_bits);
                assert_eq!(car_cdr, (Some(17), Some(19)));
                assert_eq!(edges, vec![PollEdge::SerialRequest]);
                assert_eq!(builds, 1);
                assert_eq!(serial_builds, 1);
                assert_eq!(batches.len(), 1);
                assert!(batches[0].contains(&payload_bits));
            });
            drop(fixture);
            finish(era);
        },
    );
}

#[test]
fn vm_wired_refill_closes_runs_parks_mints_and_re_executes_float_add() {
    // End-to-end V5 contract (#12095 C2, #12100, #12161, #12221, #12246;
    // M3/M4/M5): real `EraArith` over `RunAccess::float_into` hits
    // `Exhausted(Float)` on the empty pool, closes its run before dropping
    // `RunAccess`, emits `Step::Refill(Float)` with pc and operands intact,
    // and parks through `poll_region_refill`. Setup roots are cleared BEFORE
    // `collect`, proving the published operand segment keeps both float
    // operands (1.5 and 2.5) alive across GC. After `resume_cons_storage`
    // mints the pool (serial ring unchanged, no escape), the SAME `Add`
    // re-executes, allocates a fresh 4.0 distinct from both operands, and
    // publishes that `Done` root through `retire_rooted` so it survives the
    // final GC as an era-owned value.
    Context::with_test_collection_era(
        |context| {
            era_vm_setup(context, |_ctx| {
                vec![Value::make_float(1.5), Value::make_float(2.5)]
            })
        },
        |era, mut fixture| {
            era.start_cons_storage().expect("start cons storage");
            assert_eq!(
                retry("serial ring head", || era.serial_ring_head_for_test()),
                (0, None)
            );
            let permit = era.register(OWNER, 0).expect("register");
            let heap = era.heap_identity();
            let rendezvous = era.rendezvous();
            let heap_addr = fixture.heap;
            let nil_root = fixture.nil_root.clone();
            let words = fixture.words.clone();
            let (word0, word1) = (words[0], words[1]);
            let (refill_tx, refill_rx) = mpsc::channel();
            let (done_tx, done_rx) = mpsc::channel();
            let mut result_bits = 0usize;
            thread::scope(|scope| {
                scope.spawn(move || {
                    let owner = activate(permit, heap, OWNER);
                    let mut ctx = Context::new(unit_runtime());
                    let constants = era_constants(&words);
                    let ops = plain_program(vec![Op::Constant(0), Op::Constant(1), Op::Add]);
                    let poll = VmEraPoll::new(owner, heap_addr, rendezvous).with_pre_refill_hook(
                        move || {
                            refill_tx.send(()).expect("refill entered");
                        },
                    );
                    let arith = EraArith::new(poll.owner_handle(), nil_root);
                    let mut engine = VmExec::new(
                        &(),
                        RecordingEscape::new(),
                        HarnessVmSealed,
                        arith,
                        HarnessPlain::new(),
                        poll,
                    );
                    let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
                    let VmOutcome::Done { pending } = outcome else {
                        panic!("expected Done, got {outcome:?}");
                    };
                    let done = engine.lookup_root(pending).expect("done root live");
                    let done_bits = done.bits();
                    let done_float = done.xfloat();
                    // Publish the `Done` root via `arena.publish_scan` before
                    // retiring so the final stopped collection traces it.
                    let mut done_batch = Vec::new();
                    // SAFETY: `heap_addr` is this era's heap provenance root;
                    // this owner is active and no sweep can run until retire.
                    let heap_ref = unsafe { &*(heap_addr as *const TaggedHeap) };
                    engine
                        .arena()
                        .publish_scan(heap_ref, &mut done_batch)
                        .expect("publish done root");
                    assert!(
                        done_batch
                            .iter()
                            .any(|root| root.parallel_word() == done_bits),
                        "Done root must appear in publish_scan"
                    );
                    let calls = engine.arith_handler().calls.clone();
                    let escapes = engine.escape_handler().requests.len();
                    let poll = engine.into_poll();
                    done_tx
                        .send((
                            done_bits,
                            done_float,
                            calls.len(),
                            calls[0] == calls[1],
                            escapes,
                            poll.edges.clone(),
                            poll.refill_classes.clone(),
                            poll.refill_builds,
                            poll.published_batches.clone(),
                        ))
                        .expect("done report");
                    retire_with_roots(poll.take_owner(), &done_batch);
                });
                recv(&refill_rx);
                let stopped = retry("refill stop", || stop_for_test(&era));
                assert_eq!(
                    retry("serial ring head", || era.serial_ring_head_for_test()),
                    (0, None),
                    "refill takes no serial ticket"
                );
                // Clear setup's SharedRoots BEFORE collection (M3) so ONLY
                // the VM's published operand segment roots 1.5 and 2.5.
                fixture.roots.clear();
                retry_reg("return cons storage", || era.return_cons_storage(&stopped));
                let mut ticket = lend(&era, stopped);
                collect(&mut ticket);
                assert!(
                    era_owns(&era, ticket.stopped_proof(), word0),
                    "published operand 1.5 must survive refill GC"
                );
                assert!(
                    era_owns(&era, ticket.stopped_proof(), word1),
                    "published operand 2.5 must survive refill GC"
                );
                retry_reg("resume cons storage", || {
                    era.resume_cons_storage(ticket.stopped_proof())
                });
                end_epoch(ticket);
                let (
                    done_bits,
                    done_float,
                    call_count,
                    same_operands,
                    escapes,
                    edges,
                    refill_classes,
                    refill_builds,
                    batches,
                ) = recv(&done_rx);
                assert_eq!(done_float, 4.0);
                assert_ne!(done_bits, word0, "fresh float result != operand 0");
                assert_ne!(done_bits, word1, "fresh float result != operand 1");
                assert_eq!(call_count, 2, "refused once, then re-executed");
                assert!(same_operands, "same kind and operands on retry");
                assert_eq!(escapes, 0, "no serial escape");
                assert_eq!(edges, vec![PollEdge::VmRefill]);
                assert_eq!(refill_classes, vec![NumericClass::Float]);
                assert_eq!(refill_builds, 1);
                assert_eq!(batches.len(), 1);
                assert!(batches[0].contains(&word0));
                assert!(batches[0].contains(&word1));
                assert_eq!(
                    retry("serial ring head", || era.serial_ring_head_for_test()),
                    (0, None)
                );
                result_bits = done_bits;
            });
            drop(fixture);
            let stopped = retry("final stop", || stop_for_test(&era));
            retry_reg("final return cons storage", || {
                era.return_cons_storage(&stopped)
            });
            let mut ticket = lend(&era, stopped);
            collect(&mut ticket);
            assert!(
                era_owns(&era, ticket.stopped_proof(), result_bits),
                "the newly minted 4.0 float must be owned by the era after GC"
            );
            end_epoch(ticket);
            finish(era);
        },
    );
}

#[test]
fn vm_wired_float_loop_reuses_settled_tail_across_back_edge_polls() {
    // Real-era tail-reuse twin (#12161, #12242, #12246): after the initial
    // empty-pool refill mints one float page, every iteration closes its run
    // via `close_all_runs` and polls `poll_vm_back_edge`, yet iterations 2..4
    // claim the settled partial tail inline with ZERO additional refills and
    // ZERO slow-path back-edge publications.
    Context::with_test_collection_era(
        |context| {
            era_vm_setup(context, |_ctx| {
                let list = Value::cons(
                    Value::NIL,
                    Value::cons(
                        Value::NIL,
                        Value::cons(Value::NIL, Value::cons(Value::NIL, Value::NIL)),
                    ),
                );
                vec![Value::make_float(1.5), list]
            })
        },
        |era, fixture| {
            era.start_cons_storage().expect("start cons storage");
            let permit = era.register(OWNER, 0).expect("register");
            let heap = era.heap_identity();
            let rendezvous = era.rendezvous();
            let heap_addr = fixture.heap;
            let nil_root = fixture.nil_root.clone();
            let words = fixture.words.clone();
            let (refill_tx, refill_rx) = mpsc::channel();
            let (done_tx, done_rx) = mpsc::channel();
            thread::scope(|scope| {
                scope.spawn(move || {
                    let owner = activate(permit, heap, OWNER);
                    let mut ctx = Context::new(unit_runtime());
                    let constants = era_constants(&words);
                    let ops = float_loop();
                    let poll = VmEraPoll::new(owner, heap_addr, rendezvous).with_pre_refill_hook(
                        move || {
                            refill_tx.send(()).expect("refill entered");
                        },
                    );
                    let arith = EraArith::new(poll.owner_handle(), nil_root);
                    let mut engine = VmExec::new(
                        &(),
                        RecordingEscape::new(),
                        HarnessVmSealed,
                        arith,
                        HarnessPlain::new(),
                        poll,
                    );
                    let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
                    let done = done_value(&mut engine, outcome);
                    let done_float = done.xfloat();
                    let call_count = engine.arith_handler().calls.len();
                    let allocs = engine.arith_handler().float_allocs.clone();
                    let poll = engine.into_poll();
                    done_tx
                        .send((
                            done_float,
                            call_count,
                            allocs,
                            poll.edges.clone(),
                            poll.refill_builds,
                            poll.back_edge_builds,
                        ))
                        .expect("done report");
                    retire(poll.take_owner());
                });
                // Exactly one stop services the whole 4-iteration loop.
                recv(&refill_rx);
                let stopped = retry("single refill stop", || stop_for_test(&era));
                retry_reg("return cons storage", || era.return_cons_storage(&stopped));
                let mut ticket = lend(&era, stopped);
                collect(&mut ticket);
                retry_reg("resume cons storage", || {
                    era.resume_cons_storage(ticket.stopped_proof())
                });
                end_epoch(ticket);
                let (done_float, call_count, allocs, edges, refill_builds, back_edge_builds) =
                    recv(&done_rx);
                assert_eq!(done_float, 1.5 + 4.0 * 1.5);
                assert_eq!(call_count, 5, "one initial refusal + four adds");
                assert_eq!(allocs, vec![3.0, 4.5, 6.0, 7.5]);
                assert_eq!(
                    edges,
                    vec![
                        PollEdge::VmRefill,
                        PollEdge::VmBackEdge,
                        PollEdge::VmBackEdge,
                        PollEdge::VmBackEdge,
                    ],
                    "one refill for the empty pool, zero further refills across settled tails"
                );
                assert_eq!(refill_builds, 1);
                assert_eq!(back_edge_builds, 0);
            });
            drop(fixture);
            finish_with_cons_storage(era);
        },
    );
}

#[test]
fn vm_wired_refill_two_requesters_one_stop_both_claim_without_second_stop() {
    // Peer #12221 row 1 (and #12100 row 3): two parallel owners hit an empty
    // float pool and record their requests before the stop. ONE stopped
    // collection + `resume_cons_storage` mints enough pages for both; both
    // resume, re-execute their `Op::Add`, and finish without a second stop
    // and with zero serial tickets queued.
    Context::with_test_collection_era(
        |context| {
            era_vm_setup(context, |_ctx| {
                vec![
                    Value::make_float(1.5),
                    Value::make_float(2.5),
                    Value::make_float(3.5),
                    Value::make_float(4.0),
                ]
            })
        },
        |era, fixture| {
            era.start_cons_storage().expect("start cons storage");
            let permit_a = era.register(OWNER, 0).expect("register A");
            let permit_b = era.register(OWNER_B, 0).expect("register B");
            let heap = era.heap_identity();
            let rendezvous = era.rendezvous();
            let heap_addr = fixture.heap;
            let nil_a = fixture.nil_root.clone();
            let nil_b = fixture.nil_root.clone();
            let words_a = vec![fixture.words[0], fixture.words[1]];
            let words_b = vec![fixture.words[2], fixture.words[3]];
            let barrier = Arc::new(Barrier::new(2));
            let (ready_tx, ready_rx) = mpsc::channel();
            let (done_a_tx, done_a_rx) = mpsc::channel();
            let (done_b_tx, done_b_rx) = mpsc::channel();
            thread::scope(|scope| {
                let spawn_owner =
                    |permit,
                     owner_id,
                     words: Vec<usize>,
                     nil_root: SharedRoot,
                     barrier: Arc<Barrier>,
                     ready_tx: mpsc::Sender<()>,
                     done_tx: mpsc::Sender<(f64, usize, Vec<PollEdge>)>| {
                        scope.spawn(move || {
                            let owner = activate(permit, heap, owner_id);
                            let mut ctx = Context::new(unit_runtime());
                            let constants = era_constants(&words);
                            let ops =
                                plain_program(vec![Op::Constant(0), Op::Constant(1), Op::Add]);
                            let poll = VmEraPoll::new(owner, heap_addr, rendezvous)
                                .with_pre_refill_hook(move || {
                                    // Both owners have already recorded their C5
                                    // request count under the catalog gate before
                                    // either enters `poll_region_refill`.
                                    if barrier.wait().is_leader() {
                                        ready_tx.send(()).expect("both exhausted");
                                    }
                                });
                            let arith = EraArith::new(poll.owner_handle(), nil_root);
                            let mut engine = VmExec::new(
                                &(),
                                RecordingEscape::new(),
                                HarnessVmSealed,
                                arith,
                                HarnessPlain::new(),
                                poll,
                            );
                            let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
                            let done = done_value(&mut engine, outcome);
                            let calls = engine.arith_handler().calls.len();
                            let poll = engine.into_poll();
                            done_tx
                                .send((done.xfloat(), calls, poll.edges.clone()))
                                .expect("done");
                            retire(poll.take_owner());
                        });
                    };
                spawn_owner(
                    permit_a,
                    OWNER,
                    words_a,
                    nil_a,
                    Arc::clone(&barrier),
                    ready_tx.clone(),
                    done_a_tx,
                );
                spawn_owner(
                    permit_b, OWNER_B, words_b, nil_b, barrier, ready_tx, done_b_tx,
                );
                recv(&ready_rx);
                let stopped = retry("shared refill stop", || stop_for_test(&era));
                assert_eq!(
                    retry("serial ring head", || era.serial_ring_head_for_test()),
                    (0, None),
                    "shared refill queues no serial ticket"
                );
                retry_reg("return cons storage", || era.return_cons_storage(&stopped));
                let mut ticket = lend(&era, stopped);
                collect(&mut ticket);
                retry_reg("resume cons storage", || {
                    era.resume_cons_storage(ticket.stopped_proof())
                });
                end_epoch(ticket);
                let (val_a, calls_a, edges_a) = recv(&done_a_rx);
                let (val_b, calls_b, edges_b) = recv(&done_b_rx);
                assert_eq!(val_a, 4.0);
                assert_eq!(val_b, 7.5);
                assert_eq!(calls_a, 2);
                assert_eq!(calls_b, 2);
                assert_eq!(edges_a, vec![PollEdge::VmRefill], "A took one stop only");
                assert_eq!(edges_b, vec![PollEdge::VmRefill], "B took one stop only");
                assert_eq!(
                    retry("serial ring head", || era.serial_ring_head_for_test()),
                    (0, None)
                );
            });
            drop(fixture);
            finish_with_cons_storage(era);
        },
    );
}

#[test]
fn vm_wired_refill_poisoned_era_surfaces_poll_fault_and_never_loops() {
    // Pre-publish poison: when the era is poisoned before `poll_region_refill`
    // requests a stop, `request_active` refuses with `RendezvousError::Poisoned`
    // before `publish_scan` runs (`build_calls == 0`), and the driver surfaces
    // `Fault(EngineFault::Poll(RendezvousError::Poisoned))` immediately instead
    // of looping `Step::Refill`. (Post-publish poison is the sibling row below.)
    Context::with_test_collection_era(
        |context| {
            era_vm_setup(context, |_ctx| {
                vec![Value::make_float(1.5), Value::make_float(2.5)]
            })
        },
        |era, fixture| {
            era.start_cons_storage().expect("start cons storage");
            let permit = era.register(OWNER, 0).expect("register");
            let heap = era.heap_identity();
            let rendezvous = era.rendezvous();
            let heap_addr = fixture.heap;
            let nil_root = fixture.nil_root.clone();
            let words = fixture.words.clone();
            let (exhausted_tx, exhausted_rx) = mpsc::channel();
            let (poisoned_tx, poisoned_rx) = mpsc::channel();
            let (done_tx, done_rx) = mpsc::channel();
            thread::scope(|scope| {
                scope.spawn(move || {
                    let owner = activate(permit, heap, OWNER);
                    let mut ctx = Context::new(unit_runtime());
                    let constants = era_constants(&words);
                    let ops = plain_program(vec![Op::Constant(0), Op::Constant(1), Op::Add]);
                    let poll = VmEraPoll::new(owner, heap_addr, rendezvous).with_pre_refill_hook(
                        move || {
                            exhausted_tx.send(()).expect("exhausted");
                            recv(&poisoned_rx);
                        },
                    );
                    let arith = EraArith::new(poll.owner_handle(), nil_root);
                    let mut engine = VmExec::new(
                        &(),
                        RecordingEscape::new(),
                        HarnessVmSealed,
                        arith,
                        HarnessPlain::new(),
                        poll,
                    );
                    let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
                    let is_poison_fault = matches!(
                        outcome,
                        VmOutcome::Fault(EngineFault::Poll(RendezvousError::Poisoned))
                    );
                    let calls = engine.arith_handler().calls.len();
                    let allocs = engine.arith_handler().float_allocs.len();
                    let is_idle = engine.is_idle();
                    let poll = engine.into_poll();
                    let edges = poll.edges.clone();
                    let builds = poll.build_calls;
                    // Era is already poisoned; drop the owner without retiring.
                    drop(poll.take_owner());
                    done_tx
                        .send((is_poison_fault, calls, allocs, is_idle, edges, builds))
                        .expect("done");
                });
                recv(&exhausted_rx);
                // Dropping an unretired active owner poisons the era's registry.
                let poison_permit = era.register(OWNER_B, 0).expect("register poisoner");
                let poisoner = activate(poison_permit, heap, OWNER_B);
                drop(poisoner);
                assert!(era.registry_poisoned_for_test());
                poisoned_tx.send(()).expect("poisoned");
                let (is_poison_fault, calls, allocs, is_idle, edges, builds) = recv(&done_rx);
                assert!(
                    is_poison_fault,
                    "poisoned era must surface Fault(Poll(RendezvousError::Poisoned))"
                );
                assert_eq!(calls, 1, "must not loop Step::Refill on poison");
                assert_eq!(allocs, 0);
                assert!(is_idle, "unwound to Idle");
                assert_eq!(edges, vec![PollEdge::VmRefill]);
                assert_eq!(
                    builds, 0,
                    "pre-publish poison faults in request_active before publish_scan"
                );
            });
            assert!(era.registry_poisoned_for_test());
            drop(fixture);
            drop(era);
        },
    );
}

#[test]
fn vm_wired_refill_poisoned_after_publish_surfaces_poll_fault_and_never_loops() {
    // M2 (#12221 row 2): the owner publishes at `poll_region_refill`
    // (`build_calls == 1`) and ACKs the stop; the keeper then poisons the era
    // AFTER publication by dropping `AllStopped` before resume. `vm_refill`
    // wakes with `RendezvousError::Poisoned` and `VmExec::run` surfaces
    // `Fault(EngineFault::Poll(RendezvousError::Poisoned))` immediately with
    // `calls == 1`, `build_calls == 1`, and `is_idle == true`.
    Context::with_test_collection_era(
        |context| {
            era_vm_setup(context, |_ctx| {
                vec![Value::make_float(1.5), Value::make_float(2.5)]
            })
        },
        |era, fixture| {
            era.start_cons_storage().expect("start cons storage");
            let permit = era.register(OWNER, 0).expect("register");
            let heap = era.heap_identity();
            let rendezvous = era.rendezvous();
            let heap_addr = fixture.heap;
            let nil_root = fixture.nil_root.clone();
            let words = fixture.words.clone();
            let (refill_tx, refill_rx) = mpsc::channel();
            let (done_tx, done_rx) = mpsc::channel();
            thread::scope(|scope| {
                scope.spawn(move || {
                    let owner = activate(permit, heap, OWNER);
                    let mut ctx = Context::new(unit_runtime());
                    let constants = era_constants(&words);
                    let ops = plain_program(vec![Op::Constant(0), Op::Constant(1), Op::Add]);
                    let poll = VmEraPoll::new(owner, heap_addr, rendezvous).with_pre_refill_hook(
                        move || {
                            refill_tx.send(()).expect("refill entered");
                        },
                    );
                    let arith = EraArith::new(poll.owner_handle(), nil_root);
                    let mut engine = VmExec::new(
                        &(),
                        RecordingEscape::new(),
                        HarnessVmSealed,
                        arith,
                        HarnessPlain::new(),
                        poll,
                    );
                    let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
                    let is_poison_fault = matches!(
                        outcome,
                        VmOutcome::Fault(EngineFault::Poll(RendezvousError::Poisoned))
                    );
                    let calls = engine.arith_handler().calls.len();
                    let allocs = engine.arith_handler().float_allocs.len();
                    let is_idle = engine.is_idle();
                    let poll = engine.into_poll();
                    let edges = poll.edges.clone();
                    let builds = poll.build_calls;
                    let refill_builds = poll.refill_builds;
                    drop(poll.take_owner());
                    done_tx
                        .send((
                            is_poison_fault,
                            calls,
                            allocs,
                            is_idle,
                            edges,
                            builds,
                            refill_builds,
                        ))
                        .expect("done");
                });
                recv(&refill_rx);
                // Wait until the owner has published and ACKed the stop.
                let stopped = retry("post-publish stop", || stop_for_test(&era));
                // Dropping `AllStopped` without completing poisons the era
                // AFTER the owner published its scan batch.
                drop(stopped);
                assert!(era.registry_poisoned_for_test());
                let (is_poison_fault, calls, allocs, is_idle, edges, builds, refill_builds) =
                    recv(&done_rx);
                assert!(
                    is_poison_fault,
                    "post-publish poison must surface Fault(Poll(RendezvousError::Poisoned))"
                );
                assert_eq!(calls, 1, "must not loop Step::Refill after poisoned resume");
                assert_eq!(allocs, 0);
                assert!(is_idle, "unwound to Idle");
                assert_eq!(edges, vec![PollEdge::VmRefill]);
                assert_eq!(builds, 1, "owner published before the era poisoned");
                assert_eq!(refill_builds, 1);
            });
            assert!(era.registry_poisoned_for_test());
            drop(fixture);
            drop(era);
        },
    );
}

#[test]
fn vm_wired_refill_poison_between_ok_refill_and_retry_faults_not_signals() {
    // M1: if the era turns poisoned AFTER an `Ok` refill and BEFORE the
    // retried `Op::Add` runs, `EraArith` returns `ArithRefusal::Fault(Poisoned)`
    // and `step_arith` ends the run in `VmOutcome::Fault(EngineFault::Poll(..))`
    // — never a fabricated Lisp `Signaled` error.
    Context::with_test_collection_era(
        |context| {
            era_vm_setup(context, |_ctx| {
                vec![Value::make_float(1.5), Value::make_float(2.5)]
            })
        },
        |era, fixture| {
            era.start_cons_storage().expect("start cons storage");
            let permit = era.register(OWNER, 0).expect("register");
            let heap = era.heap_identity();
            let rendezvous = era.rendezvous();
            let heap_addr = fixture.heap;
            let nil_root = fixture.nil_root.clone();
            let words = fixture.words.clone();
            let (refill_tx, refill_rx) = mpsc::channel();
            let (resumed_tx, resumed_rx) = mpsc::channel();
            let (poisoned_tx, poisoned_rx) = mpsc::channel();
            let (done_tx, done_rx) = mpsc::channel();
            thread::scope(|scope| {
                scope.spawn(move || {
                    let owner = activate(permit, heap, OWNER);
                    let mut ctx = Context::new(unit_runtime());
                    let constants = era_constants(&words);
                    let ops = plain_program(vec![Op::Constant(0), Op::Constant(1), Op::Add]);
                    let poll = VmEraPoll::new(owner, heap_addr, rendezvous)
                        .with_pre_refill_hook(move || {
                            refill_tx.send(()).expect("refill entered");
                        })
                        .with_post_refill_hook(move || {
                            resumed_tx.send(()).expect("refill returned Ok");
                            recv(&poisoned_rx);
                        });
                    let arith = EraArith::new(poll.owner_handle(), nil_root);
                    let mut engine = VmExec::new(
                        &(),
                        RecordingEscape::new(),
                        HarnessVmSealed,
                        arith,
                        HarnessPlain::new(),
                        poll,
                    );
                    let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
                    let is_fault = matches!(
                        outcome,
                        VmOutcome::Fault(EngineFault::Poll(RendezvousError::Poisoned))
                    );
                    let calls = engine.arith_handler().calls.len();
                    let is_idle = engine.is_idle();
                    let poll = engine.into_poll();
                    let edges = poll.edges.clone();
                    drop(poll.take_owner());
                    done_tx
                        .send((is_fault, calls, is_idle, edges))
                        .expect("done");
                });
                recv(&refill_rx);
                let stopped = retry("ok refill stop", || stop_for_test(&era));
                retry_reg("return cons storage", || era.return_cons_storage(&stopped));
                let mut ticket = lend(&era, stopped);
                collect(&mut ticket);
                retry_reg("resume cons storage", || {
                    era.resume_cons_storage(ticket.stopped_proof())
                });
                end_epoch(ticket);
                // Wait until `poll_region_refill` has returned `Ok(Resumed)`,
                // then poison the era before the retried `Add` executes.
                recv(&resumed_rx);
                let poison_permit = era.register(OWNER_B, 0).expect("register poisoner");
                let poisoner = activate(poison_permit, heap, OWNER_B);
                drop(poisoner);
                assert!(era.registry_poisoned_for_test());
                poisoned_tx.send(()).expect("poisoned");
                let (is_fault, calls, is_idle, edges) = recv(&done_rx);
                assert!(
                    is_fault,
                    "retry on a poisoned era must surface Fault(Poll(Poisoned)), never Signaled"
                );
                assert_eq!(calls, 2, "initial Exhausted + poisoned retry");
                assert!(is_idle, "unwound to Idle");
                assert_eq!(edges, vec![PollEdge::VmRefill]);
            });
            assert!(era.registry_poisoned_for_test());
            drop(fixture);
            drop(era);
        },
    );
}

#[test]
fn vm_wired_open_numeric_run_dropped_without_close_poisons_era_and_faults_poll() {
    // M4 structural close-before-publish proof: if an arithmetic port claims a
    // float and drops `RunAccess` WITHOUT calling `close_all_runs()`, `Drop
    // for RunAccess` immediately poisons the registry (`neovm-value`
    // token.rs:159-170) and leaves admission pinned, so the next VM poll
    // (`poll_vm_back_edge`) faults `Fault(Poll(RendezvousError::Poisoned))` and
    // never publishes or ACKs the open run.
    Context::with_test_collection_era(
        |context| {
            era_vm_setup(context, |_ctx| {
                let list = Value::cons(Value::NIL, Value::cons(Value::NIL, Value::NIL));
                vec![Value::make_float(1.5), list]
            })
        },
        |era, fixture| {
            era.start_cons_storage().expect("start cons storage");
            let permit = era.register(OWNER, 0).expect("register");
            let heap = era.heap_identity();
            let rendezvous = era.rendezvous();
            let heap_addr = fixture.heap;
            let nil_root = fixture.nil_root.clone();
            let words = fixture.words.clone();
            let (refill_tx, refill_rx) = mpsc::channel();
            let (done_tx, done_rx) = mpsc::channel();
            thread::scope(|scope| {
                scope.spawn(move || {
                    let owner = activate(permit, heap, OWNER);
                    let mut ctx = Context::new(unit_runtime());
                    let constants = era_constants(&words);
                    // Iteration 1 hits empty pool -> Refill (closes cleanly) ->
                    // retry claims a float and intentionally leaves the numeric
                    // run open across `Drop for RunAccess` -> back-edge poll
                    // observes the poisoned era and faults.
                    let ops = float_loop();
                    let poll = VmEraPoll::new(owner, heap_addr, rendezvous).with_pre_refill_hook(
                        move || {
                            refill_tx.send(()).expect("refill entered");
                        },
                    );
                    let arith = EraArith::new(poll.owner_handle(), nil_root).leaving_run_open();
                    let mut engine = VmExec::new(
                        &(),
                        RecordingEscape::new(),
                        HarnessVmSealed,
                        arith,
                        HarnessPlain::new(),
                        poll,
                    );
                    let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
                    let is_poison_fault = matches!(
                        outcome,
                        VmOutcome::Fault(EngineFault::Poll(RendezvousError::Poisoned))
                    );
                    let is_idle = engine.is_idle();
                    let poll = engine.into_poll();
                    let edges = poll.edges.clone();
                    let owner = poll.take_owner();
                    let still_admitted = owner.entry_admitted_for_test();
                    drop(owner);
                    done_tx
                        .send((is_poison_fault, is_idle, edges, still_admitted))
                        .expect("done");
                });
                recv(&refill_rx);
                let stopped = retry("initial refill stop", || stop_for_test(&era));
                retry_reg("return cons storage", || era.return_cons_storage(&stopped));
                let mut ticket = lend(&era, stopped);
                collect(&mut ticket);
                retry_reg("resume cons storage", || {
                    era.resume_cons_storage(ticket.stopped_proof())
                });
                end_epoch(ticket);
                let (is_poison_fault, is_idle, edges, still_admitted) = recv(&done_rx);
                assert!(
                    is_poison_fault,
                    "unclosed numeric run must poison the era and fault the next poll"
                );
                assert!(is_idle);
                assert!(
                    still_admitted,
                    "Drop for RunAccess keeps IN_SLICE pinned so an open run can never ACK"
                );
                assert_eq!(edges, vec![PollEdge::VmRefill, PollEdge::VmBackEdge]);
            });
            assert!(era.registry_poisoned_for_test());
            drop(fixture);
            drop(era);
        },
    );
}

#[test]
fn vm_wired_refill_completes_while_peer_runs_with_ring_length_unchanged() {
    // M5 (#12100 row 3): Owner B is actively Running an owner-local back-edge
    // loop when Owner A hits an empty float pool and triggers `Step::Refill`.
    // The refill completes via stop priority with zero escapes and
    // `serial_ring_head_for_test() == Ok((0, None))` before, during, and after
    // the refill; Owner B parks at `poll_vm_back_edge`, resumes, and finishes.
    Context::with_test_collection_era(
        |context| {
            era_vm_setup(context, |_ctx| {
                let mut list = Value::NIL;
                for _ in 0..8 {
                    list = Value::cons(Value::NIL, list);
                }
                vec![Value::make_float(1.5), Value::make_float(2.5), list]
            })
        },
        |era, fixture| {
            era.start_cons_storage().expect("start cons storage");
            assert_eq!(
                retry("serial ring head", || era.serial_ring_head_for_test()),
                (0, None)
            );
            let permit_a = era.register(OWNER, 0).expect("register A");
            let permit_b = era.register(OWNER_B, 0).expect("register B");
            let heap = era.heap_identity();
            let rendezvous = era.rendezvous();
            let heap_addr = fixture.heap;
            let nil_a = fixture.nil_root.clone();
            let words_a = vec![fixture.words[0], fixture.words[1]];
            let words_b = vec![fixture.words[2]];
            let (peer_running_tx, peer_running_rx) = mpsc::channel();
            let (peer_go_tx, peer_go_rx) = mpsc::channel();
            let (refill_tx, refill_rx) = mpsc::channel();
            let (done_a_tx, done_a_rx) = mpsc::channel();
            let (done_b_tx, done_b_rx) = mpsc::channel();
            thread::scope(|scope| {
                // Owner B: pure owner-local loop polling `poll_vm_back_edge`.
                // Signals `peer_running_tx` on its first fast back-edge poll,
                // then continues running once Owner A enters refill so the
                // refill stop catches Owner B mid-loop at a back edge.
                scope.spawn(move || {
                    let owner = activate(permit_b, heap, OWNER_B);
                    let mut ctx = Context::new(unit_runtime());
                    let constants = era_constants(&words_b);
                    let ops = vec![
                        Op::Constant(0),
                        Op::Cdr,
                        Op::Dup,
                        Op::GotoIfNotNil(1),
                        Op::Pop,
                        Op::True,
                        Op::Return,
                    ];
                    let mut engine = VmExec::new(
                        &(),
                        RecordingEscape::new(),
                        HarnessVmSealed,
                        HarnessArith::new(),
                        HarnessPlain::new(),
                        VmEraPoll::new(owner, heap_addr, rendezvous).with_gate(
                            PollEdge::VmBackEdge,
                            0,
                            peer_running_tx,
                            peer_go_rx,
                        ),
                    );
                    let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
                    let done = done_value(&mut engine, outcome);
                    assert_eq!(done, Value::T);
                    let escapes = engine.escape_handler().requests.len();
                    let poll = engine.into_poll();
                    let edges = poll.edges.clone();
                    let back_edge_builds = poll.back_edge_builds;
                    done_b_tx
                        .send((escapes, edges, back_edge_builds))
                        .expect("done B");
                    retire(poll.take_owner());
                });
                // Owner A: waits until Owner B is actively mid-loop, then
                // triggers a float refill while Owner B is Running.
                scope.spawn(move || {
                    let owner = activate(permit_a, heap, OWNER);
                    recv(&peer_running_rx);
                    let mut ctx = Context::new(unit_runtime());
                    let constants = era_constants(&words_a);
                    let ops = plain_program(vec![Op::Constant(0), Op::Constant(1), Op::Add]);
                    let poll = VmEraPoll::new(owner, heap_addr, rendezvous)
                        .with_refill_publish_hook(move || {
                            // Runs inside `poll_region_refill`'s publish closure,
                            // strictly AFTER `request_active` has transitioned the
                            // era to `Stopping`, so releasing Owner B below always
                            // catches Owner B on its next `poll_vm_back_edge`
                            // rather than racing ahead into `retire()`.
                            refill_tx.send(()).expect("A entered refill stop");
                        });
                    let arith = EraArith::new(poll.owner_handle(), nil_a);
                    let mut engine = VmExec::new(
                        &(),
                        RecordingEscape::new(),
                        HarnessVmSealed,
                        arith,
                        HarnessPlain::new(),
                        poll,
                    );
                    let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
                    let done = done_value(&mut engine, outcome);
                    let escapes = engine.escape_handler().requests.len();
                    let poll = engine.into_poll();
                    done_a_tx
                        .send((
                            done.xfloat(),
                            escapes,
                            poll.edges.clone(),
                            poll.refill_classes.clone(),
                        ))
                        .expect("done A");
                    retire(poll.take_owner());
                });
                recv(&refill_rx);
                // Release Owner B so its running loop reaches the next
                // `poll_vm_back_edge` and parks for Owner A's refill stop.
                peer_go_tx.send(()).expect("release running peer B");
                let stopped = retry("peer-running refill stop", || stop_for_test(&era));
                assert_eq!(
                    retry("serial ring head", || era.serial_ring_head_for_test()),
                    (0, None),
                    "no serial ticket queued during refill"
                );
                retry_reg("return cons storage", || era.return_cons_storage(&stopped));
                let mut ticket = lend(&era, stopped);
                collect(&mut ticket);
                retry_reg("resume cons storage", || {
                    era.resume_cons_storage(ticket.stopped_proof())
                });
                end_epoch(ticket);
                let (val_a, escapes_a, edges_a, classes_a) = recv(&done_a_rx);
                let (escapes_b, edges_b, back_edge_builds_b) = recv(&done_b_rx);
                assert_eq!(val_a, 4.0);
                assert_eq!(escapes_a, 0);
                assert_eq!(escapes_b, 0);
                assert_eq!(edges_a, vec![PollEdge::VmRefill]);
                assert_eq!(classes_a, vec![NumericClass::Float]);
                assert_eq!(edges_b, vec![PollEdge::VmBackEdge; 7]);
                assert_eq!(
                    back_edge_builds_b, 1,
                    "peer B parks once at poll_vm_back_edge for A's refill stop"
                );
                assert_eq!(
                    retry("serial ring head", || era.serial_ring_head_for_test()),
                    (0, None),
                    "serial ring length unchanged after refill"
                );
            });
            drop(fixture);
            finish_with_cons_storage(era);
        },
    );
}

#[test]
fn vm_refused_refill_inside_varbind_unwinds_record_and_restores_slot() {
    // L1: a refused refill inside an active `VarBind` must go through `settle`
    // -> `unwind_to_idle`, undoing the owner binding record and leaving the
    // slab and arena clear. Bypassing `settle` leaves the binding leaked.
    let mut ctx = context();
    let x = plain_sym("pvm-refill-refused-let");
    let xs = Value::from_sym_id(x);
    let ops = plain_program(vec![
        Op::Constant(1),
        Op::VarBind(0),
        Op::Constant(2),
        Op::Constant(3),
        Op::Add,
    ]);
    let constants = [
        xs,
        Value::fixnum(1),
        Value::make_float(1.5),
        Value::make_float(2.5),
    ];
    let log = EffectLog::default();
    let budget = Rc::new(Cell::new(0));
    let mut engine = VmExec::new(
        &(),
        RecordingEscape::logging(log.clone()),
        HarnessVmSealed,
        HarnessArith::with_budget(&budget),
        HarnessPlain::new().admit(x).logging(log.clone()),
        RecordingPoll::refusing_on(PollEdge::VmRefill, RendezvousError::Publication),
    );
    let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
    assert!(
        matches!(
            outcome,
            VmOutcome::Fault(EngineFault::Poll(RendezvousError::Publication))
        ),
        "{outcome:?}"
    );
    assert_eq!(
        log.entries(),
        vec!["slot <- 1", "slot <- none"],
        "idle unwind must restore the live VarBind after a refused refill"
    );
    assert!(engine.plain_handler().live_records().is_empty());
    assert!(engine.is_idle());
    assert_clear(&engine);
}

#[test]
fn vm_granted_refill_inside_varbind_retries_add_without_rebinding() {
    // L1 twin: a granted refill inside `VarBind` retries only the `Add` at its
    // pc — `VarBind` does not re-execute, and the following `Unbind(1)` undoes
    // the single binding once.
    let mut ctx = context();
    let x = plain_sym("pvm-refill-granted-let");
    let xs = Value::from_sym_id(x);
    let ops = plain_program(vec![
        Op::Constant(1),
        Op::VarBind(0),
        Op::Constant(2),
        Op::Constant(3),
        Op::Add,
        Op::Unbind(1),
    ]);
    let constants = [
        xs,
        Value::fixnum(1),
        Value::make_float(1.5),
        Value::make_float(2.5),
    ];
    let log = EffectLog::default();
    let budget = Rc::new(Cell::new(0));
    let mut engine = VmExec::new(
        &(),
        RecordingEscape::logging(log.clone()),
        HarnessVmSealed,
        HarnessArith::with_budget(&budget),
        HarnessPlain::new().admit(x).logging(log.clone()),
        RecordingPoll::new().refilling(budget.clone(), vec![1]),
    );
    let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
    let done = done_value(&mut engine, outcome);
    assert_eq!(done.xfloat(), 4.0);
    assert_eq!(
        log.entries(),
        vec!["slot <- 1", "slot <- none"],
        "one bind and one unbind: retry never re-ran VarBind"
    );
    assert_eq!(engine.poll_handler().edges, vec![PollEdge::VmRefill]);
    assert_eq!(
        engine.poll_handler().refill_classes,
        vec![NumericClass::Float]
    );
    assert!(engine.plain_handler().live_records().is_empty());
    assert_clear(&engine);
}

#[test]
fn vm_an_exhausted_bignum_refill_passes_bignum_class_to_vm_refill() {
    // L2: `Step::Refill(class)` must forward the exact `NumericClass` returned
    // by `ArithRefusal::Exhausted(class)` into `VmPoll::vm_refill`, not a
    // hard-coded `NumericClass::Float`.
    struct BignumOnceArith {
        refused: bool,
    }
    impl ArithOps for BignumOnceArith {
        fn arith(
            &mut self,
            _kind: ArithGenericKind,
            _operands: &[Value],
        ) -> Result<Value, ArithRefusal> {
            if !self.refused {
                self.refused = true;
                return Err(ArithRefusal::Exhausted(NumericClass::Bignum));
            }
            Ok(Value::fixnum(99))
        }
    }
    let mut ctx = context();
    let ops = plain_program(vec![Op::Constant(0), Op::Constant(1), Op::Add]);
    let constants = [Value::fixnum(1), Value::fixnum(2)];
    let mut engine = VmExec::new(
        &(),
        RecordingEscape::new(),
        HarnessVmSealed,
        BignumOnceArith { refused: false },
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
    assert_eq!(done_value(&mut engine, outcome), Value::fixnum(99));
    assert_eq!(engine.poll_handler().edges, vec![PollEdge::VmRefill]);
    assert_eq!(
        engine.poll_handler().refill_classes,
        vec![NumericClass::Bignum],
        "vm_refill must receive NumericClass::Bignum, not a hard-coded Float"
    );
    assert_clear(&engine);
}

// =========================================================================
// V6 — Backtrace adapter rows (E4-DESIGN §5 :551-561, §8.4a; VM-EXECUTOR V6)
// =========================================================================

fn v6_program<'a>(function: Value, ops: &'a [Op], constants: &'a [Value]) -> VmProgram<'a> {
    VmProgram {
        ops,
        constants,
        function,
        code: Value::NIL,
    }
}

/// Harness escape double that models a serial turn installing an
/// `OwnerBacktraceView` on its `SerialContext` for the duration of the ticket,
/// optionally invoking `(backtrace-debug frame flag)` while the owner is
/// stopped, and tearing the view down before the ticket returns.
struct BacktraceAdapterEscape {
    /// Active view installed on the mock `SerialContext` while an escape is in
    /// flight; must be `None` before entry and after ticket return.
    installed_view: Rc<RefCell<Option<OwnerBacktraceView>>>,
    /// Snapshots observed during each escape, in call order.
    observed_views: Vec<OwnerBacktraceView>,
    /// Whether the mock `SerialContext` had an active view installed during
    /// each escape.
    was_installed_during_escape: Vec<bool>,
    /// Per-escape script of `(target_level, debug_on_exit)` requests to apply
    /// to the stopped owner before resuming.
    debug_script: Vec<Vec<(usize, bool)>>,
    /// Per-escape scripted completions (falls back to `fall_through`).
    completions: Vec<Option<EscapeCompletion>>,
    /// Pending updates drained by `VmExec::escape` while the owner is stopped.
    pending_updates: Vec<BacktraceDebugUpdate>,
    /// Recorded requests.
    requests: Vec<VmEscapeRequest>,
    /// Optional external slot that retains a copy of the view beyond the escape
    /// to prove `OwnerBacktraceView` holds no borrow of a running owner.
    retained_view: Option<Rc<RefCell<Option<OwnerBacktraceView>>>>,
}

impl BacktraceAdapterEscape {
    fn new(installed_view: Rc<RefCell<Option<OwnerBacktraceView>>>) -> Self {
        Self {
            installed_view,
            observed_views: Vec::new(),
            was_installed_during_escape: Vec::new(),
            debug_script: Vec::new(),
            completions: Vec::new(),
            pending_updates: Vec::new(),
            requests: Vec::new(),
            retained_view: None,
        }
    }

    fn with_debug_script(mut self, script: Vec<Vec<(usize, bool)>>) -> Self {
        self.debug_script = script;
        self
    }

    fn with_completions(mut self, completions: Vec<Option<EscapeCompletion>>) -> Self {
        self.completions = completions;
        self
    }

    fn retaining_view_in(mut self, slot: Rc<RefCell<Option<OwnerBacktraceView>>>) -> Self {
        self.retained_view = Some(slot);
        self
    }
}

impl VmEscape for BacktraceAdapterEscape {
    fn escape(
        &mut self,
        request: VmEscapeRequest,
        arena: &FrameArena,
        _plain: &mut dyn PlainAccess,
    ) -> EscapeCompletion {
        assert!(
            self.installed_view.borrow().is_none(),
            "previous ticket must have torn down its OwnerBacktraceView"
        );
        let view = OwnerBacktraceView::from_arena(arena);
        *self.installed_view.borrow_mut() = Some(view.clone());
        self.was_installed_during_escape
            .push(self.installed_view.borrow().is_some());
        if let Some(slot) = &self.retained_view {
            *slot.borrow_mut() = Some(view.clone());
        }
        let call_index = self.requests.len();
        if let Some(actions) = self.debug_script.get(call_index) {
            for &(level, enabled) in actions {
                if let Some(entry) = view.frame_at(level) {
                    self.pending_updates.push(BacktraceDebugUpdate {
                        frame: entry.frame_id,
                        debug_on_exit: enabled,
                    });
                }
            }
        }
        self.observed_views.push(view);
        self.requests.push(request.clone());
        // Tear down the view before the ticket returns (E4-DESIGN §5 :560).
        *self.installed_view.borrow_mut() = None;
        self.completions
            .get_mut(call_index)
            .and_then(Option::take)
            .unwrap_or_else(|| EscapeCompletion::fall_through(&request.op))
    }

    fn unwind(
        &mut self,
        _request: VmEscapeRequest,
        _arena: &FrameArena,
        _plain: &mut dyn PlainAccess,
    ) -> UnwindCompletion {
        UnwindCompletion::Completed {
            popped_handlers: Vec::new(),
            popped_records: Vec::new(),
            pushed_records: Vec::new(),
        }
    }

    fn take_debug_on_exit_updates(&mut self) -> Vec<BacktraceDebugUpdate> {
        std::mem::take(&mut self.pending_updates)
    }
}

#[test]
fn vm_the_adapter_renders_owner_vm_frames_from_the_published_image() {
    // V6 / E4-DESIGN §5 :551-561 & §8.4a:
    // 1. Empty-owner control: an empty FrameArena produces an empty
    //    OwnerBacktraceView, and combining with serial escape frames shows ONLY
    //    the serial frames.
    // 2. Multi-frame published image: OwnerBacktraceView renders frames
    //    innermost-first (newest first, matching GNU `mapbacktrace`), with
    //    frame_id, kind, evald, function, args/operands, pc, debug_on_exit,
    //    and Lisp flags plist (`(:debug-on-exit t)` vs `nil`).
    // 3. Live escape rendering: during a VmExec escape, the adapter installs
    //    the view, exposes the live VM frame at the escaping pc, and tears the
    //    view down before the ticket returns.
    let mut ctx = context();
    let empty_arena = FrameArena::new(8);
    let empty_view = OwnerBacktraceView::from_arena(&empty_arena);
    assert!(empty_view.is_empty());
    assert_eq!(empty_view.len(), 0);
    assert!(empty_view.frame_at(0).is_none());
    let serial_only = empty_view.combine_with_serial(&["serial-debugger", "serial-escape"], |e| {
        e.function
            .as_symbol_name(SymbolMode::Bare)
            .unwrap_or("<non-sym>")
    });
    assert_eq!(
        serial_only,
        vec!["serial-debugger", "serial-escape"],
        "§8.4a control: empty owner shows only the serial frames"
    );

    // Multi-frame published arena: outer VM -> middle Interp -> inner VM.
    let mut arena = FrameArena::new(8);
    let outer_fn = Value::symbol("pvm-bt-outer");
    let mid_fn = Value::symbol("pvm-bt-mid");
    let inner_fn = Value::symbol("pvm-bt-inner");
    let mid_body = Value::list(vec![Value::symbol("quote"), Value::fixnum(5)]);

    let outer_id = arena
        .push_vm(VmFrame::new(
            outer_fn,
            Value::NIL,
            Pc::new(3),
            OperandCursor::new(0, 0),
            OwnerRecordMark::EMPTY,
            HandlerMark::EMPTY,
            PollBudget::new(NonZeroU32::MIN),
        ))
        .expect("push outer vm");
    arena
        .vm_push(outer_id, Value::fixnum(10))
        .expect("push outer arg0");
    arena
        .vm_push(outer_id, Value::fixnum(20))
        .expect("push outer arg1");
    let inner_base = arena.vm_segment(outer_id).expect("outer segment").len();

    let mid_id = arena
        .push_interp(InterpFrame::new(
            mid_fn,
            mid_body,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            1,
            HandlerMark::EMPTY,
            OwnerRecordMark::EMPTY,
            PollBudget::new(NonZeroU32::MIN),
        ))
        .expect("push mid interp");

    let inner_id = arena
        .push_vm(
            VmFrame::new(
                inner_fn,
                Value::NIL,
                Pc::new(9),
                OperandCursor::new(inner_base, 0),
                OwnerRecordMark::EMPTY,
                HandlerMark::EMPTY,
                PollBudget::new(NonZeroU32::MIN),
            )
            .with_exit_flags(ExitFlags::DEBUG_ON_EXIT),
        )
        .expect("push inner vm");
    arena
        .vm_push(inner_id, Value::fixnum(77))
        .expect("push inner arg0");

    let view = OwnerBacktraceView::from_arena(&arena);
    assert_eq!(view.len(), 3);
    assert_eq!(view.vm_frames().count(), 2);

    // Innermost-first order: [inner VM, middle Interp, outer VM].
    let e0 = view.frame_at(0).expect("level 0");
    assert_eq!(e0.frame_id, inner_id);
    assert_eq!(e0.kind, FrameKind::Vm);
    assert!(e0.evald);
    assert_eq!(e0.function, inner_fn);
    assert_eq!(e0.args, vec![Value::fixnum(77)]);
    assert_eq!(e0.pc, Some(Pc::new(9)));
    assert!(e0.debug_on_exit());
    let (evald0, fn0, args0, flags0) = e0.to_lisp_quad();
    assert_eq!(evald0, Value::T);
    assert_eq!(fn0, inner_fn);
    assert_eq!(args0.cons_car(), Value::fixnum(77));
    assert_eq!(args0.cons_cdr(), Value::NIL);
    assert_eq!(flags0.cons_car(), Value::symbol(":debug-on-exit"));
    assert_eq!(flags0.cons_cdr().cons_car(), Value::T);

    let e1 = view.frame_at(1).expect("level 1");
    assert_eq!(e1.frame_id, mid_id);
    assert_eq!(e1.kind, FrameKind::Interp);
    assert!(!e1.evald);
    assert_eq!(e1.function, mid_fn);
    assert_eq!(e1.args, vec![mid_body]);
    assert_eq!(e1.pc, None);
    assert!(!e1.debug_on_exit());
    assert_eq!(e1.flags_plist(), Value::NIL);

    let e2 = view.frame_at(2).expect("level 2");
    assert_eq!(e2.frame_id, outer_id);
    assert_eq!(e2.kind, FrameKind::Vm);
    assert!(e2.evald);
    assert_eq!(e2.function, outer_fn);
    assert_eq!(e2.args, vec![Value::fixnum(10), Value::fixnum(20)]);
    assert_eq!(e2.pc, Some(Pc::new(3)));
    assert!(!e2.debug_on_exit());
    assert_eq!(e2.flags_plist(), Value::NIL);

    let mut mapped = Vec::new();
    view.map_frames::<()>(|entry| {
        mapped.push(entry.frame_id);
        Ok(())
    })
    .expect("map_frames");
    assert_eq!(mapped, vec![inner_id, mid_id, outer_id]);

    let combined = view.combine_with_serial(&["serial-escape"], |entry| {
        entry
            .function
            .as_symbol_name(SymbolMode::Bare)
            .unwrap_or("<non-sym>")
    });
    assert_eq!(
        combined,
        vec![
            "serial-escape",
            "pvm-bt-inner",
            "pvm-bt-mid",
            "pvm-bt-outer"
        ],
        "owner frames sit below serial escape frames in innermost-first order"
    );

    // Live VmExec escape renders the executing VM frame from its published image.
    let worker_fn = Value::symbol("pvm-live-worker");
    let callee = Value::symbol("pvm-callee");
    let constants = [Value::fixnum(11), callee, Value::fixnum(22)];
    let ops = [
        Op::Constant(0), // 11 stays on stack below call
        Op::Constant(1), // callee
        Op::Constant(2), // arg 22
        Op::Call(1),     // pc = 3, escapes with [11, callee, 22]
        Op::Add,         // 11 + call_result
        Op::Return,
    ];
    let installed = Rc::new(RefCell::new(None));
    let escape = BacktraceAdapterEscape::new(installed.clone()).with_completions(vec![Some(
        completion(&Op::Call(1), vec![Value::fixnum(30)]),
    )]);
    let mut engine = VmExec::new(
        &(),
        escape,
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    let outcome = engine.run(&mut ctx, v6_program(worker_fn, &ops, &constants));
    assert_eq!(done_value(&mut engine, outcome), Value::fixnum(41));
    assert!(
        installed.borrow().is_none(),
        "OwnerBacktraceView must be torn down after the escape ticket returns"
    );
    let handler = engine.escape_handler();
    assert_eq!(handler.was_installed_during_escape, vec![true]);
    assert_eq!(handler.observed_views.len(), 1);
    let live_entry: &OwnerBacktraceEntry = handler.observed_views[0]
        .frame_at(0)
        .expect("one live VM frame during escape");
    assert_eq!(live_entry.frame_id, handler.requests[0].frame);
    assert_eq!(live_entry.kind, FrameKind::Vm);
    assert!(live_entry.evald);
    assert_eq!(live_entry.function, worker_fn);
    assert_eq!(
        live_entry.args,
        vec![Value::fixnum(11), callee, Value::fixnum(22)]
    );
    assert_eq!(live_entry.pc, Some(Pc::new(3)));
    assert!(!live_entry.debug_on_exit());
    assert_eq!(engine.arena_depth(), 0);
}

#[test]
fn vm_backtrace_debug_on_an_owner_vm_frame_sets_its_exit_flag() {
    // V6: `set_debug_on_exit(frame_id, bool)` is valid ONLY while the owner is
    // stopped, sets/clears `ExitFlags::DEBUG_ON_EXIT` on any live VmFrame (both
    // top and non-top), refuses a running owner with `OwnerRunning`, and
    // returns `false` on stale, wrong-generation, out-of-range, or Interp
    // FrameIds without mutating any live frame. Also proves construction under
    // a real E2 `&AllStopped<'_>` proof.
    let _ctx = context();
    let mut arena = FrameArena::new(8);
    let outer_id = arena
        .push_vm(VmFrame::new(
            Value::symbol("pvm-outer"),
            Value::NIL,
            Pc::new(1),
            OperandCursor::new(0, 0),
            OwnerRecordMark::EMPTY,
            HandlerMark::EMPTY,
            PollBudget::new(NonZeroU32::MIN),
        ))
        .expect("push outer");
    let mid_interp_id = arena
        .push_interp(InterpFrame::new(
            Value::symbol("pvm-interp"),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            1,
            HandlerMark::EMPTY,
            OwnerRecordMark::EMPTY,
            PollBudget::new(NonZeroU32::MIN),
        ))
        .expect("push interp");
    let inner_id = arena
        .push_vm(VmFrame::new(
            Value::symbol("pvm-inner"),
            Value::NIL,
            Pc::new(4),
            OperandCursor::new(0, 0),
            OwnerRecordMark::EMPTY,
            HandlerMark::EMPTY,
            PollBudget::new(NonZeroU32::MIN),
        ))
        .expect("push inner");

    // 1. Running owner MUST be refused; exit flags remain untouched.
    assert_eq!(
        StoppedOwnerBacktrace::try_from_state(&mut arena, OwnerStopState::Running).err(),
        Some(BacktraceDebugError::OwnerRunning)
    );
    assert_eq!(arena.vm_exit_flags(inner_id), Some(ExitFlags::EMPTY));
    assert_eq!(arena.vm_exit_flags(outer_id), Some(ExitFlags::EMPTY));

    // 2. Stopped owner can set & clear DEBUG_ON_EXIT on BOTH top and non-top VmFrames.
    {
        let mut stopped =
            StoppedOwnerBacktrace::try_from_state(&mut arena, OwnerStopState::Stopped)
                .expect("stopped owner opens adapter");
        assert!(stopped.set_debug_on_exit(outer_id, true));
        assert!(stopped.set_debug_on_exit(inner_id, true));
        // InterpFrame is rejected with false (GNU `Fbacktrace_debug` no-op guard).
        assert!(!stopped.set_debug_on_exit(mid_interp_id, true));

        let view = stopped.view();
        assert!(view.frame_at(0).expect("inner").debug_on_exit());
        assert!(!view.frame_at(1).expect("interp").debug_on_exit());
        assert!(view.frame_at(2).expect("outer").debug_on_exit());

        // Clearing back to false restores ExitFlags::EMPTY.
        assert!(stopped.set_debug_on_exit(outer_id, false));
        let view_after = stopped.view();
        assert!(view_after.frame_at(0).expect("inner").debug_on_exit());
        assert!(!view_after.frame_at(2).expect("outer").debug_on_exit());
    }
    assert_eq!(
        arena.vm_exit_flags(inner_id),
        Some(ExitFlags::DEBUG_ON_EXIT)
    );
    assert_eq!(arena.vm_exit_flags(outer_id), Some(ExitFlags::EMPTY));

    // 3. Stale FrameId (same depth reused by a newer generation) is refused.
    assert!(arena.pop(inner_id).is_some());
    let reused_inner_id = arena
        .push_vm(VmFrame::new(
            Value::symbol("pvm-inner-reused"),
            Value::NIL,
            Pc::new(0),
            OperandCursor::new(0, 0),
            OwnerRecordMark::EMPTY,
            HandlerMark::EMPTY,
            PollBudget::new(NonZeroU32::MIN),
        ))
        .expect("push reused inner");
    assert_ne!(inner_id, reused_inner_id);
    {
        let mut stopped =
            StoppedOwnerBacktrace::try_from_state(&mut arena, OwnerStopState::Stopped)
                .expect("stopped");
        assert!(
            !stopped.set_debug_on_exit(inner_id, true),
            "stale FrameId from an older generation at the same depth must be refused"
        );
        assert!(
            !stopped.view().frame_at(0).expect("reused").debug_on_exit(),
            "stale write must not mutate the reused slot"
        );
    }

    // 4. Real-era `&AllStopped<'_>` proof: `from_all_stopped` updates the
    //    stopped owner's VM frame under a genuine E2 stop proof.
    drop(_ctx);
    Context::with_test_collection_era(
        |context| era_vm_setup(context, |_ctx| vec![Value::fixnum(1)]),
        |era, fixture| {
            let stopped = retry("v6 all-stopped proof", || stop_for_test(&era));
            {
                let mut adapter = StoppedOwnerBacktrace::from_all_stopped(&mut arena, &stopped);
                assert!(adapter.set_debug_on_exit(reused_inner_id, true));
                assert!(adapter.view().frame_at(0).expect("reused").debug_on_exit());
                assert!(adapter.set_debug_on_exit(reused_inner_id, false));
                assert!(!adapter.view().frame_at(0).expect("reused").debug_on_exit());
            }
            let mut ticket = lend(&era, stopped);
            collect(&mut ticket);
            end_epoch(ticket);
            drop(fixture);
            finish(era);
        },
    );
}

#[test]
fn vm_a_flagged_return_takes_the_serial_cleanup() {
    // V6 / VM-EXECUTOR-DESIGN §10: the frame enters WITHOUT `DEBUG_ON_EXIT`.
    // Mid-run, the owner stops at an escape (`Op::Call(0)`), the backtrace
    // adapter renders `OwnerBacktraceView` (seeing `debug_on_exit == false`)
    // and requests `set_debug_on_exit(frame_id, true)` while stopped. When the
    // owner resumes and reaches `Op::Return`, `return_disposition` takes
    // `ReturnDisposition::NeedsSerialCleanup` (escaping with `Op::Return`,
    // where the debugger sees `debug_on_exit == true` and replaces 42 with 99).
    let mut ctx = context();
    let worker_fn = Value::symbol("pvm-flagged-fn");
    let callee = Value::symbol("pvm-debug-hook");
    let constants = [callee, Value::fixnum(42)];
    let ops = [
        Op::Constant(0), // callee
        Op::Call(0),     // escape #0: adapter sets DEBUG_ON_EXIT = true while stopped
        Op::Pop,         // discard call result
        Op::Constant(1), // 42
        Op::Return,      // escape #1: NeedsSerialCleanup -> debugger returns 99
    ];
    let installed = Rc::new(RefCell::new(None));
    let escape = BacktraceAdapterEscape::new(installed.clone())
        .with_debug_script(vec![vec![(0, true)]])
        .with_completions(vec![
            Some(completion(&Op::Call(0), vec![Value::NIL])),
            Some(completion(&Op::Return, vec![Value::fixnum(99)])),
        ]);
    let mut engine = VmExec::new(
        &(),
        escape,
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    let outcome = engine.run(&mut ctx, v6_program(worker_fn, &ops, &constants));
    assert_eq!(
        done_value(&mut engine, outcome),
        Value::fixnum(99),
        "debugger return value replaces 42 on flagged Return"
    );
    assert!(installed.borrow().is_none());
    let handler = engine.escape_handler();
    assert_eq!(handler.requests.len(), 2, "Call(0) escape + Return cleanup");
    assert_eq!(handler.requests[0].op, Op::Call(0));
    assert_eq!(handler.requests[1].op, Op::Return);
    assert!(matches!(
        handler.requests[1].why,
        EscapeWhy::World(WorldSplit::Return(ReturnDisposition::NeedsSerialCleanup))
    ));
    assert!(
        !handler.observed_views[0]
            .frame_at(0)
            .expect("call view")
            .debug_on_exit(),
        "before adapter set the flag, debug_on_exit was false"
    );
    assert!(
        handler.observed_views[1]
            .frame_at(0)
            .expect("return view")
            .debug_on_exit(),
        "at Return cleanup, debug_on_exit is true on the same frame"
    );
    assert_eq!(engine.stop_state(), OwnerStopState::Stopped);

    // Twin control: setting DEBUG_ON_EXIT = true on escape #0 then clearing it
    // back to false on escape #1 restores OwnerLocal Return (no Return escape).
    let ops_clear = [
        Op::Constant(0),
        Op::Call(0), // escape #0: set true
        Op::Pop,
        Op::Constant(0),
        Op::Call(0), // escape #1: clear back to false
        Op::Pop,
        Op::Constant(1), // 42
        Op::Return,      // stays OwnerLocal -> 42, zero Return escape
    ];
    let escape_clear = BacktraceAdapterEscape::new(installed.clone())
        .with_debug_script(vec![vec![(0, true)], vec![(0, false)]]);
    let mut engine_clear = VmExec::new(
        &(),
        escape_clear,
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    let outcome_clear = engine_clear.run(&mut ctx, v6_program(worker_fn, &ops_clear, &constants));
    assert_eq!(
        done_value(&mut engine_clear, outcome_clear),
        Value::fixnum(42)
    );
    assert_eq!(
        engine_clear.escape_handler().requests.len(),
        2,
        "cleared flag reverts Return to OwnerLocal (no 3rd escape)"
    );
    assert!(
        engine_clear.escape_handler().observed_views[1]
            .frame_at(0)
            .expect("second call view")
            .debug_on_exit(),
        "second escape observed the flag set by the first escape"
    );

    // Atomicity control: if an escape queues DEBUG_ON_EXIT = true AND violates
    // R3 (wrong produced arity), the contract fault keeps the scene untouched
    // and does NOT apply the debug_on_exit update.
    let bad_completion = EscapeCompletion::Completed {
        consumed: 1,
        produced: vec![Value::NIL, Value::NIL], // Call(0) expects produced == 1
        jump: None,
        records: RecordDelta::default(),
        handlers: HandlerDelta::default(),
    };
    let escape_fault = BacktraceAdapterEscape::new(installed)
        .with_debug_script(vec![vec![(0, true)]])
        .with_completions(vec![Some(bad_completion)]);
    let mut engine_fault = VmExec::new(
        &(),
        escape_fault,
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    let fault = fault_of(engine_fault.run(&mut ctx, v6_program(worker_fn, &ops, &constants)));
    assert!(matches!(fault, EngineFault::EscapeStackEffect { .. }));
    let preserved = engine_fault.backtrace_view();
    assert_eq!(
        preserved.len(),
        1,
        "contract fault preserves the frame scene"
    );
    assert!(
        !preserved.frame_at(0).expect("preserved").debug_on_exit(),
        "R3 contract fault must not apply debug_on_exit updates"
    );
}

#[test]
fn vm_the_adapter_holds_no_borrow_of_a_running_owner() {
    // V6 / E4-DESIGN §5 :559: `OwnerBacktraceView` is an owned `'static`
    // snapshot with zero borrow of `FrameArena` or `VmExec`.
    // 1. Compile-time `'static` proof on both `OwnerBacktraceView` and
    //    `OwnerBacktraceEntry`.
    // 2. Live stepping + second run while a previously captured view remains
    //    alive: the engine mutates operands, pops the frame, unwinds to Idle,
    //    and runs a second program without borrow conflict, while the retained
    //    view stays immutable.
    // 3. Real-era safepoint proof: while parked at `poll_vm_back_edge`, the
    //    owner captures `OwnerBacktraceView`, setup roots are cleared, real GC
    //    runs, the owner resumes and finishes, and the captured view's cons
    //    bits match the surviving era-owned value.
    fn assert_static<T: 'static>(_: &T) {}

    let mut ctx = context();
    let fn1 = Value::symbol("pvm-first-run");
    let fn2 = Value::symbol("pvm-second-run");
    let callee = Value::symbol("pvm-escape-target");
    let constants1 = [Value::fixnum(10), callee, Value::fixnum(5)];
    let ops1 = [
        Op::Constant(0), // 10
        Op::Constant(1), // callee
        Op::Call(0),     // escape captures & retains OwnerBacktraceView
        Op::Add,         // 10 + 7 = 17
        Op::Constant(2), // 5
        Op::Add,         // 17 + 5 = 22
        Op::Return,
    ];
    let installed = Rc::new(RefCell::new(None));
    let retained = Rc::new(RefCell::new(None));
    let escape = BacktraceAdapterEscape::new(installed.clone())
        .with_completions(vec![Some(completion(&Op::Call(0), vec![Value::fixnum(7)]))])
        .retaining_view_in(retained.clone());
    let mut engine = VmExec::new(
        &(),
        escape,
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    let outcome1 = engine.run(&mut ctx, v6_program(fn1, &ops1, &constants1));
    assert_eq!(done_value(&mut engine, outcome1), Value::fixnum(22));
    assert!(installed.borrow().is_none());

    // Take the captured view out and hold it alive across a SECOND full run on
    // the exact same `VmExec` — impossible if the view borrowed `VmExec` or
    // `FrameArena`.
    let retained_view = retained
        .borrow_mut()
        .take()
        .expect("view captured at escape");
    assert_static(&retained_view);
    assert_static(retained_view.frame_at(0).expect("entry"));

    let constants2 = [Value::fixnum(100), Value::fixnum(23)];
    let ops2 = [Op::Constant(0), Op::Constant(1), Op::Add, Op::Return];
    let outcome2 = engine.run(&mut ctx, v6_program(fn2, &ops2, &constants2));
    assert_eq!(done_value(&mut engine, outcome2), Value::fixnum(123));

    // The retained snapshot still reflects run 1's escape point untouched.
    assert_eq!(retained_view.len(), 1);
    let snap = retained_view.frame_at(0).expect("retained entry");
    assert_eq!(snap.function, fn1);
    assert_eq!(snap.args, vec![Value::fixnum(10), callee]);
    assert_eq!(snap.pc, Some(Pc::new(2)));
    assert!(!snap.debug_on_exit());
    drop(retained_view);
    drop(ctx);

    // Real-era safepoint proof: capture OwnerBacktraceView inside the
    // `poll_vm_back_edge` publish closure, run real GC with setup roots
    // cleared, resume the owner while the view is held, and verify the live
    // cons operand survives and matches the view's snapshot bits.
    Context::with_test_collection_era(
        |context| {
            era_vm_setup(context, |_ctx| {
                let payload = Value::cons(Value::fixnum(31), Value::fixnum(37));
                vec![payload]
            })
        },
        |era, mut fixture| {
            let permit = era.register(OWNER, 0).expect("register");
            let heap = era.heap_identity();
            let rendezvous = era.rendezvous();
            let heap_addr = fixture.heap;
            let words = fixture.words.clone();
            let payload_bits = words[0];
            let (ready_tx, ready_rx) = mpsc::channel();
            let (go_tx, go_rx) = mpsc::channel();
            let (done_tx, done_rx) = mpsc::channel();
            thread::scope(|scope| {
                scope.spawn(move || {
                    let owner = activate(permit, heap, OWNER);
                    let mut ctx = Context::new(unit_runtime());
                    let constants = era_constants(&words);
                    // Loop once with `payload` live on stack, then return `payload`.
                    let ops = [
                        Op::Constant(0), // payload
                        Op::True,        // loop flag for iter 1
                        Op::GotoIfNil(5),
                        Op::Nil,     // iter 2 flag
                        Op::Goto(2), // backward branch -> poll_vm_back_edge
                        Op::Return,
                    ];
                    struct BackEdgeViewPoll<'era> {
                        owner: Mutator<'era>,
                        heap: usize,
                        captured: Vec<OwnerBacktraceView>,
                    }
                    impl VmPoll for BackEdgeViewPoll<'_> {
                        fn vm_back_edge(
                            &mut self,
                            arena: &FrameArena,
                        ) -> Result<PollOutcome, RendezvousError> {
                            let heap = self.heap;
                            let captured = &mut self.captured;
                            poll_vm_back_edge(&mut self.owner, || {
                                captured.push(OwnerBacktraceView::from_arena(arena));
                                let mut batch = Vec::new();
                                // SAFETY: `heap` is this era's heap provenance
                                // root and `self.owner` is active in this era.
                                let heap_ref = unsafe { &*(heap as *const TaggedHeap) };
                                arena.publish_scan(heap_ref, &mut batch)?;
                                Ok(batch)
                            })
                        }
                        fn vm_call(
                            &mut self,
                            _arena: &FrameArena,
                        ) -> Result<PollOutcome, RendezvousError> {
                            Ok(PollOutcome::Continue)
                        }
                        fn serial_request(
                            &mut self,
                            _arena: &FrameArena,
                        ) -> Result<PollOutcome, RendezvousError> {
                            Ok(PollOutcome::Continue)
                        }
                        fn vm_refill(
                            &mut self,
                            _arena: &FrameArena,
                            _class: NumericClass,
                        ) -> Result<PollOutcome, RendezvousError> {
                            Ok(PollOutcome::Continue)
                        }
                    }
                    let worker_fn = Value::symbol("pvm-era-back-edge");
                    let mut engine = VmExec::new(
                        &(),
                        RecordingEscape::new(),
                        HarnessVmSealed,
                        HarnessArith::new(),
                        HarnessPlain::new(),
                        BackEdgeViewPoll {
                            owner,
                            heap: heap_addr,
                            captured: Vec::new(),
                        },
                    );
                    ready_tx.send(()).expect("ready");
                    recv(&go_rx);
                    let outcome = engine.run(&mut ctx, v6_program(worker_fn, &ops, &constants));
                    let done = done_value(&mut engine, outcome);
                    let done_bits = done.bits();
                    let car_cdr = (done.cons_car().as_fixnum(), done.cons_cdr().as_fixnum());
                    let poll = engine.into_poll();
                    let captured_len = poll.captured.len();
                    let captured_bits: Vec<usize> = poll.captured[0]
                        .frame_at(0)
                        .expect("captured back-edge frame")
                        .args
                        .iter()
                        .map(|v| v.bits())
                        .collect();
                    let captured_pc = poll.captured[0].frame_at(0).expect("frame").pc;
                    retire(poll.owner);
                    done_tx
                        .send((done_bits, car_cdr, captured_len, captured_bits, captured_pc))
                        .expect("done");
                });
                recv(&ready_rx);
                request_stop(rendezvous);
                go_tx.send(()).expect("go");
                let stopped = retry("v6 back-edge stop", || stop_for_test(&era));
                fixture.roots.clear();
                let mut ticket = lend(&era, stopped);
                collect(&mut ticket);
                assert!(
                    era_owns(&era, ticket.stopped_proof(), payload_bits),
                    "published operand cons must survive real GC"
                );
                end_epoch(ticket);
                let (done_bits, car_cdr, captured_len, captured_bits, captured_pc) = recv(&done_rx);
                assert_eq!(done_bits, payload_bits);
                assert_eq!(car_cdr, (Some(31), Some(37)));
                assert_eq!(captured_len, 1);
                assert!(
                    captured_bits.contains(&payload_bits),
                    "captured OwnerBacktraceView recorded the published payload cons"
                );
                assert_eq!(
                    captured_pc,
                    Some(Pc::new(2)),
                    "back-edge poll publishes the loop target pc (Op::Goto(2) -> Pc(2))"
                );
            });
            drop(fixture);
            finish(era);
        },
    );
}

// =========================================================================
// V7 — Owner-local bytecode->bytecode Call: child VmFrame, iterative (r1 §9)
// =========================================================================

fn make_v7_bytecode(
    required: usize,
    optional: usize,
    ops: Vec<Op>,
    constants: Vec<Value>,
    max_stack: usize,
) -> Value {
    const REQ_NAMES: [&str; 4] = [
        "pvm-v7-req-0",
        "pvm-v7-req-1",
        "pvm-v7-req-2",
        "pvm-v7-req-3",
    ];
    const OPT_NAMES: [&str; 4] = [
        "pvm-v7-opt-0",
        "pvm-v7-opt-1",
        "pvm-v7-opt-2",
        "pvm-v7-opt-3",
    ];
    let req = REQ_NAMES[..required].iter().map(|n| plain_sym(n)).collect();
    let opt = OPT_NAMES[..optional].iter().map(|n| plain_sym(n)).collect();
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: req,
        optional: opt,
        rest: None,
    });
    f.set_lexical(true);
    f.set_ops(ops);
    f.set_constants(constants.into());
    f.set_max_stack(StackDepth::for_test(max_stack));
    Value::make_bytecode(f)
}

#[test]
fn vm_owner_local_call_pushes_a_child_frame_and_returns_into_the_caller_segment() {
    // V7 row 1: an admitted materialized ByteCodeFunction callee runs
    // owner-locally in a child VmFrame (iterative, zero escapes, zero
    // `serial_request`, one `vm_call` poll per Call) and delivers its return
    // value directly into the caller's consumed function slot while preserving
    // caller operands below the call.
    let mut ctx = context();

    // 1. 2-arg arithmetic callee: (lambda (a b) (+ (* a 10) b))
    // Entry stack in child: [a, b] (StackRef(1) = a, StackRef(0) = b).
    let callee_add = make_v7_bytecode(
        2,
        0,
        vec![
            Op::StackRef(1), // a
            Op::Constant(0), // 10
            Op::Mul,         // a * 10
            Op::StackRef(1), // b (shifted by 1)
            Op::Add,         // a * 10 + b
            Op::Return,
        ],
        vec![Value::fixnum(10)],
        8,
    );
    let constants = [
        Value::fixnum(100), // caller operand preserved below Call(2)
        callee_add,
        Value::fixnum(3),
        Value::fixnum(4),
    ];
    let ops = [
        Op::Constant(0), // 100
        Op::Constant(1), // callee_add
        Op::Constant(2), // arg0 = 3
        Op::Constant(3), // arg1 = 4
        Op::Call(2),     // -> 34 in callee slot; caller segment becomes [100, 34]
        Op::Add,         // 100 + 34 = 134
        Op::Return,
    ];
    let mut engine = VmExec::new(
        &(),
        RecordingEscape::new(),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
    assert_eq!(done_value(&mut engine, outcome), Value::fixnum(134));
    assert_eq!(
        engine.escape_handler().requests.len(),
        0,
        "owner-local bytecode->bytecode Call and Return take zero escapes"
    );
    assert_eq!(
        engine.poll_handler().edges,
        vec![PollEdge::VmCall],
        "Bcall polls vm_call once and never polls serial_request"
    );
    assert_clear(&engine);

    // 2. Optional parameter NIL-padding: (lambda (a &optional b) (if b (+ a b) a))
    // Called with 1 arg -> b is NIL -> returns a; called with 2 args -> returns a + b.
    let callee_opt = make_v7_bytecode(
        1,
        1,
        vec![
            Op::StackRef(0), // b
            Op::GotoIfNil(6),
            Op::StackRef(1), // a
            Op::StackRef(1), // b
            Op::Add,
            Op::Return,
            Op::StackRef(1), // a
            Op::Return,
        ],
        vec![],
        8,
    );
    let opt_constants = [callee_opt, Value::fixnum(7), Value::fixnum(5)];
    let ops_opt1 = [
        Op::Constant(0),
        Op::Constant(1),
        Op::Call(1), // omitted optional b -> nil -> 7
        Op::Constant(0),
        Op::Constant(1),
        Op::Constant(2),
        Op::Call(2), // supplied optional b -> 12
        Op::Add,     // 7 + 12 = 19
        Op::Return,
    ];
    let outcome = engine.run(&mut ctx, program_with(&ops_opt1, &opt_constants));
    assert_eq!(done_value(&mut engine, outcome), Value::fixnum(19));
    assert_eq!(engine.escape_handler().requests.len(), 0);
    assert_clear(&engine);

    // 3. Multi-level iterative chain A -> B -> C (3 live frames deep).
    let fn_c = make_v7_bytecode(
        1,
        0,
        vec![Op::StackRef(0), Op::Constant(0), Op::Add, Op::Return],
        vec![Value::fixnum(1)],
        8,
    );
    let fn_b = make_v7_bytecode(
        1,
        0,
        vec![
            Op::Constant(0), // fn_c
            Op::StackRef(1), // x
            Op::Constant(1), // 2
            Op::Mul,         // x * 2
            Op::Call(1),     // fn_c(x * 2) = 2x + 1
            Op::Return,
        ],
        vec![fn_c, Value::fixnum(2)],
        8,
    );
    let chain_constants = [fn_b, Value::fixnum(20)];
    let ops_chain = [Op::Constant(0), Op::Constant(1), Op::Call(1), Op::Return];
    let outcome = engine.run(&mut ctx, program_with(&ops_chain, &chain_constants));
    assert_eq!(done_value(&mut engine, outcome), Value::fixnum(41));
    assert_eq!(engine.escape_handler().requests.len(), 0);
    assert_clear(&engine);

    // 4. V6+V7 backtrace composition: child frame escapes on an unadmitted
    //    symbol call; OwnerBacktraceView shows [child, caller] innermost-first;
    //    adapter flags CALLER (level 1) with DEBUG_ON_EXIT; child returns
    //    OwnerLocal while caller's Return takes NeedsSerialCleanup!
    let escape_sym = Value::symbol("pvm-v7-inner-escape");
    let child_escaping = make_v7_bytecode(
        1,
        0,
        vec![
            Op::Constant(0), // escape_sym -> forces escape inside child
            Op::Call(0),
            Op::Pop,
            Op::StackRef(0), // return incoming arg
            Op::Return,      // stays OwnerLocal!
        ],
        vec![escape_sym],
        8,
    );
    let caller_fn = Value::symbol("pvm-v7-caller");
    let bt_constants = [child_escaping, Value::fixnum(55)];
    let bt_ops = [
        Op::Constant(0),
        Op::Constant(1),
        Op::Call(1), // owner-local into child_escaping
        Op::Return,  // caller flagged by child's escape -> NeedsSerialCleanup -> 777
    ];
    let installed = Rc::new(RefCell::new(None));
    let bt_escape = BacktraceAdapterEscape::new(installed)
        .with_debug_script(vec![vec![(1, true)]]) // level 1 = caller!
        .with_completions(vec![
            Some(completion(&Op::Call(0), vec![Value::NIL])),
            Some(completion(&Op::Return, vec![Value::fixnum(777)])),
        ]);
    let mut bt_engine = VmExec::new(
        &(),
        bt_escape,
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    let outcome = bt_engine.run(&mut ctx, v6_program(caller_fn, &bt_ops, &bt_constants));
    assert_eq!(done_value(&mut bt_engine, outcome), Value::fixnum(777));
    let h = bt_engine.escape_handler();
    assert_eq!(
        h.requests.len(),
        2,
        "only inner Call(0) and caller Return escaped (child Call and child Return stayed owner-local)"
    );
    assert_eq!(h.requests[0].op, Op::Call(0));
    assert_eq!(h.requests[1].op, Op::Return);
    assert_eq!(
        h.observed_views[0].len(),
        2,
        "OwnerBacktraceView inside child escape sees both child (level 0) and caller (level 1)"
    );
    assert_eq!(
        h.observed_views[0].frame_at(0).expect("child").function,
        child_escaping
    );
    assert_eq!(
        h.observed_views[0].frame_at(1).expect("caller").function,
        caller_fn
    );
    assert_eq!(
        h.observed_views[1].len(),
        1,
        "at caller Return cleanup, child frame has already popped"
    );
    assert!(
        h.observed_views[1]
            .frame_at(0)
            .expect("caller")
            .debug_on_exit()
    );
}

#[test]
fn vm_a_callee_signal_settles_to_idle_until_v8() {
    // V7 row 2: a signal inside an owner-local callee (either owner-local
    // arithmetic Flow or an escape Flow) unwinds BOTH child and caller frames,
    // undoes owner Plain bindings in LIFO order across both frames, empties
    // arena/LIFOs/roots, settles to Idle, and allows a fresh subsequent run.
    let mut ctx = context();
    let x = plain_sym("pvm-v7-sig-x");
    let log = EffectLog::default();

    // Callee shadows `x = 20` then triggers integer division by zero (arith-error).
    let callee_div0 = make_v7_bytecode(
        0,
        0,
        vec![
            Op::Constant(0), // 20
            Op::VarBind(1),  // bind x = 20 (shadows caller's x = 10)
            Op::Constant(0), // 20
            Op::Constant(2), // 0
            Op::Div,         // signals arith-error!
            Op::Unbind(1),
            Op::Return,
        ],
        vec![Value::fixnum(20), Value::from_sym_id(x), Value::fixnum(0)],
        8,
    );
    let constants = [Value::fixnum(10), Value::from_sym_id(x), callee_div0];
    let ops = [
        Op::Constant(0), // 10
        Op::VarBind(1),  // bind x = 10 in caller
        Op::Constant(2), // callee_div0
        Op::Call(0),     // owner-local into callee_div0
        Op::Unbind(1),
        Op::Return,
    ];
    let mut engine = VmExec::new(
        &(),
        RecordingEscape::new(),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new().admit(x).logging(log.clone()),
        RecordingPoll::new(),
    );
    let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
    let VmOutcome::Signaled(flow) = outcome else {
        panic!("expected Signaled from callee, got {outcome:?}");
    };
    assert_eq!(signal_key(&flow).0, "arith-error");
    assert_eq!(
        engine.escape_handler().requests.len(),
        0,
        "zero escapes before owner-local callee arith-error"
    );
    assert_eq!(
        log.entries(),
        vec!["slot <- 10", "slot <- 20", "slot <- 10", "slot <- none"],
        "callee shadow x=20 undone back to 10 before caller x=10 undone to none (strict LIFO)"
    );
    assert!(engine.plain_handler().live_records().is_empty());
    assert_eq!(
        engine.arena().bc_position(),
        0,
        "bc_position resets to 0 on settle-to-Idle"
    );
    assert_clear(&engine);

    // Depth-3 callee signal (entry -> mid -> leaf) settles all 3 frames to Idle.
    let leaf_sig = make_v7_bytecode(
        0,
        0,
        vec![Op::Constant(0), Op::Constant(1), Op::Div, Op::Return],
        vec![Value::fixnum(1), Value::fixnum(0)],
        4,
    );
    let mid_sig = make_v7_bytecode(
        0,
        0,
        vec![Op::Constant(0), Op::Call(0), Op::Return],
        vec![leaf_sig],
        4,
    );
    let depth3_constants = [mid_sig];
    let depth3_ops = [Op::Constant(0), Op::Call(0), Op::Return];
    let depth3_out = engine.run(&mut ctx, program_with(&depth3_ops, &depth3_constants));
    let VmOutcome::Signaled(depth3_flow) = depth3_out else {
        panic!("expected Signaled at depth 3, got {depth3_out:?}");
    };
    assert_eq!(signal_key(&depth3_flow).0, "arith-error");
    assert_eq!(engine.max_depth(), 3, "reached depth 3 before signaling");
    assert_eq!(engine.arena_depth(), 0);
    assert_eq!(engine.arena().bc_position(), 0);
    assert_eq!(engine.live_root_count(), 0);
    assert_clear(&engine);

    // Subsequent run on the same engine succeeds cleanly after settle-to-Idle.
    let ok_constants = [Value::fixnum(42)];
    let ok_ops = [Op::Constant(0), Op::Return];
    let next = engine.run(&mut ctx, program_with(&ok_ops, &ok_constants));
    assert_eq!(done_value(&mut engine, next), Value::fixnum(42));
    assert_clear(&engine);

    // Sibling: escape Flow inside callee also unwinds child + caller cleanly.
    let boom_sym = Value::symbol("pvm-v7-boom");
    let callee_escape_flow = make_v7_bytecode(
        0,
        0,
        vec![Op::Constant(0), Op::Call(0), Op::Return],
        vec![boom_sym],
        8,
    );
    let flow_constants = [callee_escape_flow];
    let flow_ops = [Op::Constant(0), Op::Call(0), Op::Return];
    let flow_completion =
        EscapeCompletion::Flow(signal("error", vec![Value::string("callee boom")]));
    let mut engine2 = VmExec::new(
        &(),
        RecordingEscape::with_script(vec![Some(flow_completion)]),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    let outcome2 = engine2.run(&mut ctx, program_with(&flow_ops, &flow_constants));
    let VmOutcome::Signaled(flow2) = outcome2 else {
        panic!("expected Signaled from callee escape, got {outcome2:?}");
    };
    assert_eq!(signal_key(&flow2).0, "error");
    assert_eq!(engine2.escape_handler().requests.len(), 1);
    assert_clear(&engine2);
}

#[test]
fn vm_unadmitted_bytecode_call_escapes_with_vm_call_then_serial_request() {
    // V7 admission boundary: any callee that `iterative_admission_if_known`
    // rejects (wrong arity, &rest, dynamic params, closure with env, or
    // non-bytecode symbol) escapes through the ONE stub with EXACTLY ONE
    // `VmCall` followed by `SerialRequest` (never a double `VmCall` poll).
    let mut ctx = context();

    // 1. Wrong arity: expects 2 required, called with 1.
    let wrong_arity = make_v7_bytecode(2, 0, vec![Op::StackRef(0), Op::Return], vec![], 8);
    // 2. &rest parameter: not admitted iteratively.
    let rest_fn = {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: vec![],
            optional: vec![],
            rest: Some(plain_sym("pvm-v7-rest")),
        });
        f.set_lexical(true);
        f.set_ops(vec![Op::Nil, Op::Return]);
        f.set_max_stack(StackDepth::for_test(4));
        Value::make_bytecode(f)
    };
    // 3. Dynamic-binding bytecode (lexical = false with named param): not admitted.
    let dynamic_fn = {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: vec![plain_sym("pvm-v7-dyn")],
            optional: vec![],
            rest: None,
        });
        f.set_lexical(false);
        f.set_ops(vec![Op::Nil, Op::Return]);
        f.set_max_stack(StackDepth::for_test(4));
        Value::make_bytecode(f)
    };
    // 4. Non-bytecode symbol.
    let sym_fn = Value::symbol("pvm-v7-sym-callee");

    for (label, callee, nargs) in [
        ("wrong-arity", wrong_arity, 1u16),
        ("rest", rest_fn, 0u16),
        ("dynamic", dynamic_fn, 1u16),
        ("symbol", sym_fn, 0u16),
    ] {
        let mut constants = vec![callee];
        let mut ops = vec![Op::Constant(0)];
        for i in 0..nargs {
            constants.push(Value::fixnum(i64::from(i + 1)));
            ops.push(Op::Constant(i + 1));
        }
        ops.push(Op::Call(nargs));
        ops.push(Op::Return);

        let mut engine = VmExec::new(
            &(),
            RecordingEscape::with_script(vec![Some(completion(
                &Op::Call(nargs),
                vec![Value::fixnum(88)],
            ))]),
            HarnessVmSealed,
            HarnessArith::new(),
            HarnessPlain::new(),
            RecordingPoll::new(),
        );
        let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
        assert_eq!(
            done_value(&mut engine, outcome),
            Value::fixnum(88),
            "{label}"
        );
        assert_eq!(
            engine.escape_handler().requests.len(),
            1,
            "{label}: must escape once"
        );
        assert_eq!(
            engine.poll_handler().edges,
            vec![PollEdge::VmCall, PollEdge::SerialRequest],
            "{label}: exact [VmCall, SerialRequest] order with no double VmCall poll"
        );
        assert_clear(&engine);
    }
}

#[test]
fn vm_child_return_cleanup_cannot_pop_caller_serial_markers_f6() {
    // V7 (F6, vm.rs:1661-1665): a child frame's escape (including `Return`
    // `NeedsSerialCleanup`) may ONLY pop serial record/handler markers above
    // its OWN `bind_base` and `handler_base`. Trying to pop a caller's marker
    // below the child's base faults with `EscapeRecordContract` and leaves
    // both frames and LIFOs intact.
    let mut ctx = context();
    let caller_rec = token(101);
    let child_rec = token(202);

    // Child pushes a serial record (`SaveExcursion`), then returns ->
    // `NeedsSerialCleanup` because `child_rec` sits above child's `bind_base`.
    let child_fn = make_v7_bytecode(
        0,
        0,
        vec![Op::SaveExcursion, Op::Constant(0), Op::Return],
        vec![Value::fixnum(9)],
        8,
    );
    let constants = [child_fn];
    // Caller pushes `caller_rec`, calls child, unbinds `caller_rec`, returns.
    let ops = [
        Op::SaveExcursion,
        Op::Constant(0),
        Op::Call(0),
        Op::Unbind(1),
        Op::Return,
    ];

    // 1. Bad child Return completion tries to pop BOTH `child_rec` AND
    //    `caller_rec` (crossing below child's `bind_base`) -> faults!
    let bad_script = vec![
        Some(completion_pushing(&Op::SaveExcursion, caller_rec)), // caller
        Some(completion_pushing(&Op::SaveExcursion, child_rec)),  // child
        Some(EscapeCompletion::Completed {
            consumed: 1,
            produced: vec![Value::fixnum(9)],
            jump: None,
            records: RecordDelta {
                popped: vec![child_rec, caller_rec],
                pushed: vec![],
            },
            handlers: HandlerDelta::default(),
        }),
    ];
    let mut bad_engine = VmExec::new(
        &(),
        RecordingEscape::with_script(bad_script),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    let fault = fault_of(bad_engine.run(&mut ctx, program_with(&ops, &constants)));
    assert!(
        matches!(
            fault,
            EngineFault::EscapeRecordContract(RecordStack::Bindings)
        ),
        "child Return cleanup popping caller's marker below bind_base must fault, got {fault:?}"
    );
    assert_eq!(
        bad_engine.arena_depth(),
        2,
        "contract fault preserves both caller and child frames"
    );
    assert_eq!(
        bad_engine.arena().bindings().len(),
        2,
        "neither marker was popped on contract fault"
    );

    // 2. Well-behaved child Return completion pops ONLY `child_rec` (above
    //    child's `bind_base`); caller's `caller_rec` survives for caller's
    //    subsequent `Unbind(1)`.
    let good_script = vec![
        Some(completion_pushing(&Op::SaveExcursion, caller_rec)), // caller
        Some(completion_pushing(&Op::SaveExcursion, child_rec)),  // child
        Some(EscapeCompletion::Completed {
            consumed: 1,
            produced: vec![Value::fixnum(9)],
            jump: None,
            records: RecordDelta {
                popped: vec![child_rec],
                pushed: vec![],
            },
            handlers: HandlerDelta::default(),
        }),
        Some(completion_popping(&Op::Unbind(1), &[caller_rec])), // caller unbind
    ];
    let mut good_engine = VmExec::new(
        &(),
        RecordingEscape::with_script(good_script),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    let outcome = good_engine.run(&mut ctx, program_with(&ops, &constants));
    assert_eq!(done_value(&mut good_engine, outcome), Value::fixnum(9));
    assert_clear(&good_engine);

    // 3. F6 HANDLER HALF (HIGH-2): a child frame's escape / Return cleanup
    //    may ONLY pop serial handler markers strictly above its OWN
    //    `handler_base` (`HandlerStack::serial_run_matches_above` /
    //    `pop_serial_run_above`).
    let caller_h = SerialHandlerToken(301);
    let child_h = SerialHandlerToken(402);
    let child_handler_fn = make_v7_bytecode(
        0,
        0,
        vec![Op::PushConditionCase(2), Op::Constant(0), Op::Return],
        vec![Value::fixnum(14)],
        8,
    );
    let h_constants = [child_handler_fn];
    let h_ops = [
        Op::PushConditionCase(4), // pushes caller_h
        Op::Constant(0),
        Op::Call(0),
        Op::PopHandler, // pops caller_h
        Op::Return,
    ];
    let push_handler = |op: &Op, tok: SerialHandlerToken| EscapeCompletion::Completed {
        consumed: static_stack_effect(op).consumed,
        produced: vec![],
        jump: None,
        records: RecordDelta::default(),
        handlers: HandlerDelta {
            popped: vec![],
            pushed: Some(tok),
        },
    };
    let pop_handler = |op: &Op, toks: &[SerialHandlerToken]| EscapeCompletion::Completed {
        consumed: static_stack_effect(op).consumed,
        produced: vec![],
        jump: None,
        records: RecordDelta::default(),
        handlers: HandlerDelta {
            popped: toks.to_vec(),
            pushed: None,
        },
    };

    // 3a. Bad child Return completion tries to pop BOTH `child_h` AND
    //     `caller_h` (crossing below child's `handler_base`) -> faults with
    //     `EscapeRecordContract(RecordStack::Handlers)` leaving both frames
    //     and both handler markers intact!
    let bad_h_script = vec![
        Some(push_handler(&Op::PushConditionCase(4), caller_h)),
        Some(push_handler(&Op::PushConditionCase(2), child_h)),
        Some(EscapeCompletion::Completed {
            consumed: 1,
            produced: vec![Value::fixnum(14)],
            jump: None,
            records: RecordDelta::default(),
            handlers: HandlerDelta {
                popped: vec![child_h, caller_h],
                pushed: None,
            },
        }),
    ];
    let mut bad_h_engine = VmExec::new(
        &(),
        RecordingEscape::with_script(bad_h_script),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    let h_fault = fault_of(bad_h_engine.run(&mut ctx, program_with(&h_ops, &h_constants)));
    assert!(
        matches!(
            h_fault,
            EngineFault::EscapeRecordContract(RecordStack::Handlers)
        ),
        "child Return cleanup popping caller's handler marker below handler_base must fault, got {h_fault:?}"
    );
    assert_eq!(
        bad_h_engine.arena_depth(),
        2,
        "handler contract fault preserves both caller and child frames"
    );
    assert_eq!(
        bad_h_engine.arena().handlers().len(),
        2,
        "neither handler marker was popped on contract fault"
    );

    // 3b. Good child Return completion pops ONLY `child_h`; `caller_h`
    //     survives for caller's `PopHandler`.
    let good_h_script = vec![
        Some(push_handler(&Op::PushConditionCase(4), caller_h)),
        Some(push_handler(&Op::PushConditionCase(2), child_h)),
        Some(EscapeCompletion::Completed {
            consumed: 1,
            produced: vec![Value::fixnum(14)],
            jump: None,
            records: RecordDelta::default(),
            handlers: HandlerDelta {
                popped: vec![child_h],
                pushed: None,
            },
        }),
        Some(pop_handler(&Op::PopHandler, &[caller_h])),
    ];
    let mut good_h_engine = VmExec::new(
        &(),
        RecordingEscape::with_script(good_h_script),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    let h_out = good_h_engine.run(&mut ctx, program_with(&h_ops, &h_constants));
    assert_eq!(done_value(&mut good_h_engine, h_out), Value::fixnum(14));
    assert_clear(&good_h_engine);

    // 4. F6 `RecordingEscape` Plain take bound (HIGH-2): caller holds a live
    //    owner `Plain` binding (`x = 55`) while child takes a
    //    `NeedsSerialCleanup` `Return` (binding `x = 88` + pushing `child_rec`);
    //    child's serial Return cleanup stops at child's `bind_base` so caller's
    //    owner `Plain` binding (`x = 55`) survives for caller's `VarRef(x)`!
    let px = plain_sym("pvm-v7-f6-caller-plain");
    let child_plain_fn = make_v7_bytecode(
        0,
        0,
        vec![
            Op::Constant(0), // 88
            Op::VarBind(1),  // child binds x = 88 on owner slab
            Op::SaveExcursion,
            Op::VarRef(1), // returns 88 (NeedsSerialCleanup because child_rec is live)
            Op::Return,
        ],
        vec![Value::fixnum(88), Value::from_sym_id(px)],
        8,
    );
    let p_constants = [Value::fixnum(55), Value::from_sym_id(px), child_plain_fn];
    let p_ops = [
        Op::Constant(0), // 55
        Op::VarBind(1),  // caller binds x = 55 on owner slab
        Op::Constant(2), // child_plain_fn
        Op::Call(0),     // child returns 88 via NeedsSerialCleanup (must NOT pop caller's x=55!)
        Op::VarRef(1),   // caller reads x -> must STILL be 55!
        Op::Add,         // 88 + 55 = 143
        Op::Unbind(1),   // caller unbinds its own x = 55
        Op::Return,
    ];
    let p_script = vec![
        Some(completion_pushing(&Op::SaveExcursion, child_rec)),
        Some(EscapeCompletion::Completed {
            consumed: 1,
            produced: vec![Value::fixnum(88)],
            jump: None,
            records: RecordDelta {
                popped: vec![child_rec],
                pushed: vec![],
            },
            handlers: HandlerDelta::default(),
        }),
    ];
    let mut p_engine = VmExec::new(
        &(),
        RecordingEscape::with_script(p_script),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new().admit(px),
        RecordingPoll::new(),
    );
    let p_out = p_engine.run(&mut ctx, program_with(&p_ops, &p_constants));
    assert_eq!(
        done_value(&mut p_engine, p_out),
        Value::fixnum(143),
        "caller's Plain binding x=55 must survive child's NeedsSerialCleanup Return (88 + 55 = 143)"
    );
    assert!(p_engine.plain_handler().live_records().is_empty());
    assert_clear(&p_engine);
}

#[test]
fn vm_owner_local_call_preserves_caller_and_child_operands_across_real_gc() {
    // Real-era V7 GC proof: caller keeps `cons_a` live on its operand stack,
    // calls an admitted child `ByteCodeFunction` passing `cons_b`, child parks
    // at `poll_vm_back_edge` with both frames live, setup roots are cleared,
    // real GC runs, both frames resume, child returns `cons_b`, and caller
    // combines `(car cons_a) + (cdr cons_b)` while asserting exact bit
    // survival for `cons_a`, `cons_b`, and `child_fn`.
    Context::with_test_collection_era(
        |context| {
            era_vm_setup(context, |_ctx| {
                let cons_a = Value::cons(Value::fixnum(13), Value::fixnum(17));
                let cons_b = Value::cons(Value::fixnum(19), Value::fixnum(29));
                // Child: loop once with `cons_b` live (polling `vm_back_edge`),
                // then return `cons_b`.
                let child = make_v7_bytecode(
                    1,
                    0,
                    vec![
                        Op::True,         // iter 1 flag -> [cons_b, t]
                        Op::GotoIfNil(4), // pc 1 -> pops flag
                        Op::Nil,          // iter 2 flag -> [cons_b, nil]
                        Op::Goto(1),      // backward branch -> poll_vm_back_edge
                        Op::Return,       // returns cons_b
                    ],
                    vec![],
                    8,
                );
                vec![cons_a, cons_b, child]
            })
        },
        |era, mut fixture| {
            let permit = era.register(OWNER, 0).expect("register");
            let heap = era.heap_identity();
            let rendezvous = era.rendezvous();
            let heap_addr = fixture.heap;
            let words = fixture.words.clone();
            let (cons_a_bits, cons_b_bits, child_bits) = (words[0], words[1], words[2]);
            let (gate_reached_tx, gate_reached_rx) = mpsc::channel();
            let (gate_go_tx, gate_go_rx) = mpsc::channel();
            let (done_tx, done_rx) = mpsc::channel();
            thread::scope(|scope| {
                scope.spawn(move || {
                    let owner = activate(permit, heap, OWNER);
                    let mut ctx = Context::new(unit_runtime());
                    let constants = era_constants(&words);
                    // Caller: push cons_a, call child(cons_b) -> [cons_a, cons_b],
                    // compute (+ (car cons_a) (cdr cons_b)) = 13 + 29 = 42.
                    let ops = [
                        Op::Constant(0), // cons_a
                        Op::Constant(2), // child
                        Op::Constant(1), // cons_b
                        Op::Call(1),     // -> [cons_a, cons_b]
                        Op::Cdr,         // -> [cons_a, 29]
                        Op::StackRef(1), // -> [cons_a, 29, cons_a]
                        Op::Car,         // -> [cons_a, 29, 13]
                        Op::Add,         // -> [cons_a, 42]
                        Op::Return,
                    ];
                    let mut engine = VmExec::new(
                        &(),
                        RecordingEscape::new(),
                        HarnessVmSealed,
                        HarnessArith::new(),
                        HarnessPlain::new(),
                        VmEraPoll::new(owner, heap_addr, rendezvous).with_gate(
                            PollEdge::VmCall,
                            0,
                            gate_reached_tx,
                            gate_go_rx,
                        ),
                    );
                    let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
                    let done = done_value(&mut engine, outcome);
                    let escapes = engine.escape_handler().requests.len();
                    let poll = engine.into_poll();
                    done_tx
                        .send((
                            done.as_fixnum(),
                            escapes,
                            poll.edges.clone(),
                            poll.call_builds,
                            poll.back_edge_builds,
                            poll.published_depths.clone(),
                            poll.published_batches.clone(),
                        ))
                        .expect("done");
                    retire(poll.take_owner());
                });
                // Wait until `VmCall` finishes on the uncontested fast path
                // (`call_builds == 0`), then arm the stop so `step_call` pushes
                // the child frame (`depth == 2`) and the child deterministically
                // parks and publishes at `PollEdge::VmBackEdge` with both
                // frames live.
                recv(&gate_reached_rx);
                request_stop(rendezvous);
                gate_go_tx.send(()).expect("go");
                let stopped = retry("v7 call gc stop", || stop_for_test(&era));
                fixture.roots.clear();
                let mut ticket = lend(&era, stopped);
                collect(&mut ticket);
                assert!(era_owns(&era, ticket.stopped_proof(), cons_a_bits));
                assert!(era_owns(&era, ticket.stopped_proof(), cons_b_bits));
                assert!(era_owns(&era, ticket.stopped_proof(), child_bits));
                end_epoch(ticket);
                let (sum, escapes, edges, call_builds, back_edge_builds, depths, batches) =
                    recv(&done_rx);
                assert_eq!(sum, Some(42));
                assert_eq!(escapes, 0, "owner-local child Call took zero escapes");
                assert_eq!(edges, vec![PollEdge::VmCall, PollEdge::VmBackEdge]);
                assert_eq!(
                    call_builds, 0,
                    "VmCall must take the uncontested fast path before stop"
                );
                assert_eq!(
                    back_edge_builds, 1,
                    "GC must stop and publish inside the child at VmBackEdge"
                );
                assert_eq!(
                    depths,
                    vec![2],
                    "GC publication must happen with BOTH caller and child frames live (depth 2)"
                );
                assert!(!batches.is_empty());
                assert!(batches[0].contains(&cons_a_bits));
                assert!(batches[0].contains(&cons_b_bits));
                assert!(batches[0].contains(&child_bits));
            });
            drop(fixture);
            finish(era);
        },
    );
}

// ---------------------------------------------------------------- V7-b harness & §11 rows

struct OwnedEntryProgram {
    ops: Vec<Op>,
    constants: Vec<Value>,
    function: Value,
    code: Value,
}

/// Harness `VmCode` port supporting an optional entry fixture program plus a
/// live bare-symbol function table and world `function_epoch` (V7 §4/§5).
struct HarnessVmCode {
    entry: Option<OwnedEntryProgram>,
    symbols: Rc<RefCell<HashMap<SymId, Value>>>,
    world_epoch: Rc<Cell<u64>>,
    not_materialized_sentinel: Option<Value>,
}

impl HarnessVmCode {
    fn new(entry: OwnedEntryProgram) -> Self {
        Self {
            entry: Some(entry),
            symbols: Rc::new(RefCell::new(HashMap::new())),
            world_epoch: Rc::new(Cell::new(0)),
            not_materialized_sentinel: None,
        }
    }

    fn bind_symbol(&self, sym: SymId, value: Value) {
        self.symbols.borrow_mut().insert(sym, value);
    }

    fn fset(&self, sym: SymId, value: Value) -> u64 {
        self.symbols.borrow_mut().insert(sym, value);
        let next = self.world_epoch.get() + 1;
        self.world_epoch.set(next);
        next
    }

    fn with_not_materialized(mut self, sentinel: Value) -> Self {
        self.not_materialized_sentinel = Some(sentinel);
        self
    }
}

impl VmCode for HarnessVmCode {
    fn program(&self, code: Value) -> Result<VmProgram<'_>, CodeRefusal> {
        if let Some(entry) = &self.entry
            && entry.code == code
        {
            return Ok(VmProgram {
                ops: &entry.ops,
                constants: &entry.constants,
                function: entry.function,
                code: entry.code,
            });
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
        if env.debug_on_next_call {
            return CallClass::Escape(CallEscape::DebugOnNextCall);
        }
        if self.not_materialized_sentinel == Some(designator) {
            return CallClass::Escape(CallEscape::NotMaterialized);
        }
        classify_call_with_symbols(designator, nargs, env, arena, |sym| {
            if self.world_epoch.get() != env.function_epoch {
                SymbolCallLookup::EpochChanged
            } else if let Some(&val) = self.symbols.borrow().get(&sym) {
                SymbolCallLookup::Bound(val)
            } else {
                SymbolCallLookup::Unbound
            }
        })
    }
}

fn make_lazy_v7_bytecode(
    argdesc: i64,
    max_stack: usize,
    bytes: Vec<u8>,
    constants: Vec<Value>,
) -> ByteCodeFunction {
    let params = FunctionParams::try_from(Value::fixnum(argdesc)).unwrap();
    let mut func = ByteCodeFunction::new(params);
    func.set_max_stack(StackDepth::for_test(max_stack));
    func.set_constants(constants.into());
    func.set_gnu_bytecode_bytes(Some(neovm_value::tagged::header::LispByteVec::owned(bytes)));
    func.defer_gnu_decode();
    assert!(func.lazy_gnu_code().is_some());
    func
}

fn make_fib_bytecode(fib_sym: Value) -> Value {
    // (lambda (n) (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))
    make_v7_bytecode(
        1,
        0,
        vec![
            Op::StackRef(0),  // 0: [n, n]
            Op::Constant(1),  // 1: [n, n, 2]
            Op::Lss,          // 2: [n, n<2]
            Op::GotoIfNil(6), // 3: [n]
            Op::StackRef(0),  // 4: [n, n]
            Op::Return,       // 5: return n
            Op::Constant(0),  // 6: [n, fib]
            Op::StackRef(1),  // 7: [n, fib, n]
            Op::Constant(2),  // 8: [n, fib, n, 1]
            Op::Sub,          // 9: [n, fib, n-1]
            Op::Call(1),      // 10: [n, fib(n-1)]
            Op::Constant(0),  // 11: [n, fib(n-1), fib]
            Op::StackRef(2),  // 12: [n, fib(n-1), fib, n]
            Op::Constant(1),  // 13: [n, fib(n-1), fib, n, 2]
            Op::Sub,          // 14: [n, fib(n-1), fib, n-2]
            Op::Call(1),      // 15: [n, fib(n-1), fib(n-2)]
            Op::Add,          // 16: [n, sum]
            Op::Return,       // 17: return sum
        ],
        vec![fib_sym, Value::fixnum(2), Value::fixnum(1)],
        8,
    )
}

#[test]
fn vm_owner_local_call_polls_vm_call_once_and_requests_no_serial_turn() {
    // V7 §11: exact edge counts — owner-local Call is 1 VmCall / 0 SerialRequest;
    // escaped Call is 1 VmCall / 1 SerialRequest.
    let mut ctx = context();
    let callee = make_v7_bytecode(1, 0, vec![Op::StackRef(0), Op::Add1, Op::Return], vec![], 4);
    let constants = [callee, Value::fixnum(41)];
    let ops = [Op::Constant(0), Op::Constant(1), Op::Call(1), Op::Return];
    let mut local_engine = VmExec::new(
        &(),
        RecordingEscape::new(),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    let outcome = local_engine.run(&mut ctx, program_with(&ops, &constants));
    assert_eq!(done_value(&mut local_engine, outcome), Value::fixnum(42));
    assert_eq!(
        local_engine
            .poll_handler()
            .edges
            .iter()
            .filter(|e| **e == PollEdge::VmCall)
            .count(),
        1
    );
    assert_eq!(
        local_engine
            .poll_handler()
            .edges
            .iter()
            .filter(|e| **e == PollEdge::SerialRequest)
            .count(),
        0
    );
    assert_eq!(local_engine.escape_handler().requests.len(), 0);
    assert_clear(&local_engine);

    let sym_callee = Value::symbol("pvm-v7-escaped-edge-count");
    let esc_constants = [sym_callee, Value::fixnum(41)];
    let mut esc_engine = VmExec::new(
        &(),
        RecordingEscape::with_script(vec![Some(completion(
            &Op::Call(1),
            vec![Value::fixnum(42)],
        ))]),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    let outcome = esc_engine.run(&mut ctx, program_with(&ops, &esc_constants));
    assert_eq!(done_value(&mut esc_engine, outcome), Value::fixnum(42));
    assert_eq!(
        esc_engine
            .poll_handler()
            .edges
            .iter()
            .filter(|e| **e == PollEdge::VmCall)
            .count(),
        1
    );
    assert_eq!(
        esc_engine
            .poll_handler()
            .edges
            .iter()
            .filter(|e| **e == PollEdge::SerialRequest)
            .count(),
        1
    );
    assert_eq!(esc_engine.escape_handler().requests.len(), 1);
    assert_eq!(esc_engine.call_escapes(), &[CallEscape::NotByteCode]);
    assert_clear(&esc_engine);
}

#[test]
fn vm_nested_owner_local_calls_match_the_serial_vm() {
    // V7 §11 (CRITICAL differential row): recursive `fib(15)` run through BOTH
    // the serial VM and `VmExec` returns the exact same value (610), with zero
    // escapes on `VmExec` and `max_depth` equal to the recursion depth (16:
    // 1 entry wrapper + 15 recursive frames down to `fib(1)`).
    let mut ctx = context();
    let fib_sym_id = plain_sym("pvm-v7-fib");
    let fib_sym = Value::from_sym_id(fib_sym_id);
    let fib_bc = make_fib_bytecode(fib_sym);

    // 1. Run through the real serial VM.
    ctx.obarray.set_symbol_function_id(fib_sym_id, fib_bc);
    let serial_result = ctx
        .apply(fib_bc, vec![Value::fixnum(15)])
        .expect("serial VM fib(15)");
    assert_eq!(serial_result, Value::fixnum(610));

    // 2. Run the exact same ByteCodeFunction through `VmExec` via `VmCode`.
    let entry_code = Value::symbol("pvm-v7-fib-entry");
    let code = HarnessVmCode::new(OwnedEntryProgram {
        ops: vec![Op::Constant(0), Op::Constant(1), Op::Call(1), Op::Return],
        constants: vec![fib_sym, Value::fixnum(15)],
        function: entry_code,
        code: entry_code,
    });
    code.bind_symbol(fib_sym_id, fib_bc);

    let mut engine = VmExec::new(
        &(),
        RecordingEscape::new(),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    let outcome = engine.run_with_code(
        &mut ctx,
        &code,
        VmEntry {
            function: entry_code,
            code: entry_code,
            frame_bytes: bytecode_frame_bytes(4),
        },
    );
    let parallel_result = done_value(&mut engine, outcome);
    assert_eq!(
        parallel_result, serial_result,
        "parallel VmExec fib(15) must match serial VM"
    );
    assert_eq!(
        engine.escape_handler().requests.len(),
        0,
        "all recursive fib(15) calls must stay owner-local"
    );
    assert_eq!(
        engine.max_depth(),
        16,
        "1 entry frame + 15 recursive fib frames (fib 15 -> ... -> fib 1)"
    );
    assert_eq!(engine.arena().bc_position(), 0);
    assert_clear(&engine);
}

#[test]
fn vm_ineligible_callees_escape_with_the_caller_untouched() {
    // V7 §11: one table row per every `CallEscape` variant (all 10 variants).
    // Asserts: exact `CallEscape` reason recorded, escape request `pc` equals
    // the `Op::Call` pc, and caller's segment, pc, and both LIFOs at the escape
    // equal the published pre-call state (caller completely untouched).
    let mut ctx = context();

    let admitted_fn = make_v7_bytecode(1, 0, vec![Op::StackRef(0), Op::Return], vec![], 4);
    let rest_fn = {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: vec![],
            optional: vec![],
            rest: Some(plain_sym("pvm-v7-tbl-rest")),
        });
        f.set_lexical(true);
        f.set_ops(vec![Op::Nil, Op::Return]);
        f.set_max_stack(StackDepth::for_test(4));
        Value::make_bytecode(f)
    };
    let cold_lazy_fn = Value::make_bytecode(make_lazy_v7_bytecode(
        (1 << 8) | 1,
        4,
        vec![192, 135],
        vec![Value::fixnum(7)],
    ));
    let not_materialized = Value::symbol("pvm-v7-not-materialized-stub");
    let sym_ok_id = plain_sym("pvm-v7-tbl-sym");
    let sym_ok = Value::from_sym_id(sym_ok_id);
    let caller_rec = token(501);

    struct Case {
        name: &'static str,
        designator: Value,
        nargs: u16,
        env: CallEnv,
        entry_frame_bytes: usize,
        bump_world_epoch: bool,
        expected: CallEscape,
    }

    let cases = [
        Case {
            name: "NotByteCode",
            designator: Value::fixnum(123),
            nargs: 1,
            env: CallEnv::default(),
            entry_frame_bytes: 0,
            bump_world_epoch: false,
            expected: CallEscape::NotByteCode,
        },
        Case {
            name: "NotMaterialized",
            designator: not_materialized,
            nargs: 1,
            env: CallEnv::default(),
            entry_frame_bytes: 0,
            bump_world_epoch: false,
            expected: CallEscape::NotMaterialized,
        },
        Case {
            name: "ColdAdmission",
            designator: cold_lazy_fn,
            nargs: 1,
            env: CallEnv::default(),
            entry_frame_bytes: 0,
            bump_world_epoch: false,
            expected: CallEscape::ColdAdmission,
        },
        Case {
            name: "Ineligible",
            designator: rest_fn,
            nargs: 1,
            env: CallEnv::default(),
            entry_frame_bytes: 0,
            bump_world_epoch: false,
            expected: CallEscape::Ineligible,
        },
        Case {
            name: "WrongNargs",
            designator: admitted_fn,
            nargs: 0,
            env: CallEnv::default(),
            entry_frame_bytes: 0,
            bump_world_epoch: false,
            expected: CallEscape::WrongNargs,
        },
        Case {
            name: "Depth",
            designator: admitted_fn,
            nargs: 1,
            env: CallEnv {
                depth_limit: 1,
                ..CallEnv::default()
            },
            entry_frame_bytes: 0,
            bump_world_epoch: false,
            expected: CallEscape::Depth,
        },
        Case {
            name: "StackBudget",
            designator: admitted_fn,
            nargs: 1,
            env: CallEnv::default(),
            entry_frame_bytes: BC_STACK_POSITION_LIMIT,
            bump_world_epoch: false,
            expected: CallEscape::StackBudget,
        },
        Case {
            name: "DebugOnNextCall",
            designator: admitted_fn,
            nargs: 1,
            env: CallEnv {
                debug_on_next_call: true,
                ..CallEnv::default()
            },
            entry_frame_bytes: 0,
            bump_world_epoch: false,
            expected: CallEscape::DebugOnNextCall,
        },
        Case {
            name: "CompilerOverrides",
            designator: sym_ok,
            nargs: 1,
            env: CallEnv {
                compiler_overrides: true,
                ..CallEnv::default()
            },
            entry_frame_bytes: 0,
            bump_world_epoch: false,
            expected: CallEscape::CompilerOverrides,
        },
        Case {
            name: "FunctionEpoch",
            designator: sym_ok,
            nargs: 1,
            env: CallEnv {
                function_epoch: 0,
                ..CallEnv::default()
            },
            entry_frame_bytes: 0,
            bump_world_epoch: true,
            expected: CallEscape::FunctionEpoch,
        },
    ];

    for case in cases {
        let entry_code = Value::symbol("pvm-v7-tbl-entry");
        let preserved = Value::fixnum(900);
        let arg0 = Value::fixnum(11);
        let mut ops = vec![
            Op::SaveExcursion, // 0: pushes caller_rec onto bind LIFO
            Op::Constant(0),   // 1: preserved caller operand
            Op::Constant(1),   // 2: designator
        ];
        if case.nargs == 1 {
            ops.push(Op::Constant(2)); // 3: arg0
        }
        let call_pc = ops.len();
        ops.push(Op::Call(case.nargs));
        ops.push(Op::Add);
        ops.push(Op::Unbind(1));
        ops.push(Op::Return);

        let code = HarnessVmCode::new(OwnedEntryProgram {
            ops,
            constants: vec![preserved, case.designator, arg0],
            function: entry_code,
            code: entry_code,
        })
        .with_not_materialized(not_materialized);
        code.bind_symbol(sym_ok_id, admitted_fn);
        if case.bump_world_epoch {
            code.fset(sym_ok_id, admitted_fn);
        }

        let mut expected_seg = vec![preserved, case.designator];
        if case.nargs == 1 {
            expected_seg.push(arg0);
        }

        let script = vec![
            Some(completion_pushing(&Op::SaveExcursion, caller_rec)),
            Some(completion(&Op::Call(case.nargs), vec![Value::fixnum(77)])),
            Some(completion_popping(&Op::Unbind(1), &[caller_rec])),
        ];
        let mut engine = VmExec::new(
            &(),
            RecordingEscape::with_script(script),
            HarnessVmSealed,
            HarnessArith::new(),
            HarnessPlain::new(),
            RecordingPoll::new(),
        );
        engine.set_call_env(case.env);
        let outcome = engine.run_with_code(
            &mut ctx,
            &code,
            VmEntry {
                function: entry_code,
                code: entry_code,
                frame_bytes: case.entry_frame_bytes,
            },
        );
        assert_eq!(
            done_value(&mut engine, outcome),
            Value::fixnum(977),
            "{}: run finishes after escaped Call",
            case.name
        );
        assert_eq!(
            engine.call_escapes(),
            &[case.expected],
            "{}: exact CallEscape reason",
            case.name
        );
        let esc = engine.escape_handler();
        assert_eq!(esc.requests[1].op, Op::Call(case.nargs), "{}", case.name);
        assert_eq!(esc.requests[1].pc, Pc::new(call_pc), "{}", case.name);
        assert_eq!(
            esc.segments[1], expected_seg,
            "{}: caller segment untouched at escape",
            case.name
        );
        assert_eq!(
            esc.lifos[1],
            (vec![Some(caller_rec.0)], vec![]),
            "{}: caller LIFOs untouched at escape",
            case.name
        );
        assert_eq!(
            engine.arena().bc_position(),
            0,
            "{}: bc_position refunded",
            case.name
        );
        assert_clear(&engine);
    }
}

#[test]
fn vm_a_symbol_callee_reads_its_live_cell_and_an_epoch_change_escapes_once() {
    // V7 §11: bare symbol resolves through its live cell when epoch matches;
    // `fset` bumps the epoch -> ONE escape (`CallEscape::FunctionEpoch`) whose
    // completion refreshes `CallEnv.function_epoch` -> subsequent call runs
    // owner-local against the new definition with zero escapes.
    let mut ctx = context();
    let sym_id = plain_sym("pvm-v7-live-sym");
    let sym_val = Value::from_sym_id(sym_id);

    let f1 = make_v7_bytecode(
        1,
        0,
        vec![Op::StackRef(0), Op::Constant(0), Op::Add, Op::Return],
        vec![Value::fixnum(1)],
        4,
    );
    let f2 = make_v7_bytecode(
        1,
        0,
        vec![Op::StackRef(0), Op::Constant(0), Op::Add, Op::Return],
        vec![Value::fixnum(100)],
        4,
    );
    let entry_code = Value::symbol("pvm-v7-sym-entry");
    let code = HarnessVmCode::new(OwnedEntryProgram {
        ops: vec![Op::Constant(0), Op::Constant(1), Op::Call(1), Op::Return],
        constants: vec![sym_val, Value::fixnum(10)],
        function: entry_code,
        code: entry_code,
    });
    code.bind_symbol(sym_id, f1);

    // Run 1: epoch matches (0 == 0) -> owner-local against f1 -> 11, 0 escapes.
    let mut engine = VmExec::new(
        &(),
        RecordingEscape::new(),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    let out1 = engine.run_with_code(
        &mut ctx,
        &code,
        VmEntry {
            function: entry_code,
            code: entry_code,
            frame_bytes: 0,
        },
    );
    assert_eq!(done_value(&mut engine, out1), Value::fixnum(11));
    assert_eq!(engine.escape_handler().requests.len(), 0);
    assert!(engine.call_escapes().is_empty());
    assert_clear(&engine);

    // Run 2 & 3 in one two-call program after `fset` updates symbol to f2 and
    // bumps world epoch to 1:
    //   call #1 sees stale epoch 0 != 1 -> escapes ONCE with FunctionEpoch,
    //           escape refreshes CallEnv.function_epoch = 1;
    //   call #2 sees fresh epoch 1 == 1 -> owner-local against f2 -> 110!
    let two_call_code = Value::symbol("pvm-v7-sym-two-calls");
    let code2 = HarnessVmCode::new(OwnedEntryProgram {
        ops: vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::Call(1), // stale epoch -> escapes once -> 110 & refreshes CallEnv
            Op::Constant(0),
            Op::Constant(1),
            Op::Call(1), // fresh epoch -> owner-local against f2 -> 110
            Op::Add,     // 220
            Op::Return,
        ],
        constants: vec![sym_val, Value::fixnum(10)],
        function: two_call_code,
        code: two_call_code,
    });
    code2.fset(sym_id, f2); // world_epoch = 1, sym -> f2

    let refreshed_env = CallEnv {
        function_epoch: 1,
        ..CallEnv::default()
    };
    let mut engine2 = VmExec::new(
        &(),
        RecordingEscape::with_script(vec![Some(completion(
            &Op::Call(1),
            vec![Value::fixnum(110)],
        ))])
        .with_env_refreshes(vec![
            None,                // run start keeps stale function_epoch = 0
            Some(refreshed_env), // escape completion refreshes to epoch 1
        ]),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    let out2 = engine2.run_with_code(
        &mut ctx,
        &code2,
        VmEntry {
            function: two_call_code,
            code: two_call_code,
            frame_bytes: 0,
        },
    );
    assert_eq!(done_value(&mut engine2, out2), Value::fixnum(220));
    assert_eq!(
        engine2.escape_handler().requests.len(),
        1,
        "epoch change escapes once; second call is owner-local against f2"
    );
    assert_eq!(engine2.call_escapes(), &[CallEscape::FunctionEpoch]);
    assert_eq!(engine2.call_env().function_epoch, 1);
    assert_clear(&engine2);
}

#[test]
fn vm_the_depth_limit_escapes_the_call_instead_of_faulting() {
    // V7 §11: at `CallEnv.depth_limit`, the call escapes with `CallEscape::Depth`
    // instead of faulting with `EngineFault::Frame(Depth)`.
    let mut ctx = context();
    let leaf = make_v7_bytecode(
        0,
        0,
        vec![Op::Constant(0), Op::Return],
        vec![Value::fixnum(1)],
        4,
    );
    let mid = make_v7_bytecode(
        0,
        0,
        vec![Op::Constant(0), Op::Call(0), Op::Return],
        vec![leaf],
        4,
    );
    let constants = [mid];
    let ops = [Op::Constant(0), Op::Call(0), Op::Return];

    let mut engine = VmExec::new(
        &(),
        RecordingEscape::with_script(vec![Some(completion(
            &Op::Call(0),
            vec![Value::fixnum(99)],
        ))]),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    // Depth 1 = entry, depth 2 = mid (allowed), depth 3 = leaf (exceeds 2 -> escapes).
    engine.set_call_env(CallEnv {
        depth_limit: 2,
        ..CallEnv::default()
    });
    let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
    assert_eq!(done_value(&mut engine, outcome), Value::fixnum(99));
    assert_eq!(engine.escape_handler().requests.len(), 1);
    assert_eq!(engine.call_escapes(), &[CallEscape::Depth]);
    assert_eq!(engine.max_depth(), 2);
    assert_clear(&engine);
}

#[test]
fn vm_the_bytecode_stack_budget_escapes_like_the_serial_charge() {
    // V7 §11: a callee whose `max_stack` `charge_frame_position` refuses escapes
    // with `CallEscape::StackBudget`; sequential calls that fit refund their
    // `frame_bytes` on return so they never exhaust the budget.
    let mut ctx = context();
    let callee = make_v7_bytecode(
        0,
        0,
        vec![Op::Constant(0), Op::Return],
        vec![Value::fixnum(5)],
        8,
    );
    let callee_bytes = bytecode_frame_bytes(8);
    assert!(charge_frame_position(BC_STACK_POSITION_LIMIT - callee_bytes, 8).is_some());
    assert!(charge_frame_position(BC_STACK_POSITION_LIMIT - callee_bytes + 1, 8).is_none());

    let entry_code = Value::symbol("pvm-v7-budget-entry");
    let code = HarnessVmCode::new(OwnedEntryProgram {
        ops: vec![
            Op::Constant(0),
            Op::Call(0),
            Op::Constant(0),
            Op::Call(0),
            Op::Add,
            Op::Return,
        ],
        constants: vec![callee],
        function: entry_code,
        code: entry_code,
    });

    // 1. Exact fit: two sequential calls both succeed owner-locally because the
    //    first call refunds its `frame_bytes` on Return.
    let mut fit_engine = VmExec::new(
        &(),
        RecordingEscape::new(),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    let fit_out = fit_engine.run_with_code(
        &mut ctx,
        &code,
        VmEntry {
            function: entry_code,
            code: entry_code,
            frame_bytes: BC_STACK_POSITION_LIMIT - callee_bytes,
        },
    );
    assert_eq!(done_value(&mut fit_engine, fit_out), Value::fixnum(10));
    assert_eq!(fit_engine.escape_handler().requests.len(), 0);
    assert_eq!(fit_engine.arena().bc_position(), 0);
    assert_clear(&fit_engine);

    // 2. One byte over the serial `charge_frame_position` limit: escapes as
    //    `CallEscape::StackBudget` without faulting.
    let mut over_engine = VmExec::new(
        &(),
        RecordingEscape::with_script(vec![
            Some(completion(&Op::Call(0), vec![Value::fixnum(5)])),
            Some(completion(&Op::Call(0), vec![Value::fixnum(5)])),
        ]),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    let over_out = over_engine.run_with_code(
        &mut ctx,
        &code,
        VmEntry {
            function: entry_code,
            code: entry_code,
            frame_bytes: BC_STACK_POSITION_LIMIT - callee_bytes + 1,
        },
    );
    assert_eq!(done_value(&mut over_engine, over_out), Value::fixnum(10));
    assert_eq!(
        over_engine.call_escapes(),
        &[CallEscape::StackBudget, CallEscape::StackBudget]
    );
    assert_eq!(over_engine.arena().bc_position(), 0);
    assert_clear(&over_engine);
}

#[test]
fn vm_debug_on_next_call_armed_escapes_every_call() {
    // V7 §11: `debug-on-next-call` is checked FIRST before resolving, so an
    // admitted direct bytecode callee AND an unbound symbol both report
    // `CallEscape::DebugOnNextCall`.
    let mut ctx = context();
    let admitted = make_v7_bytecode(
        0,
        0,
        vec![Op::Constant(0), Op::Return],
        vec![Value::fixnum(1)],
        4,
    );
    let unbound_sym = Value::symbol("pvm-v7-debug-unbound");
    let constants = [admitted, unbound_sym];
    let ops = [
        Op::Constant(0),
        Op::Call(0),
        Op::Constant(1),
        Op::Call(0),
        Op::Add,
        Op::Return,
    ];
    let mut engine = VmExec::new(
        &(),
        RecordingEscape::with_script(vec![
            Some(completion(&Op::Call(0), vec![Value::fixnum(10)])),
            Some(completion(&Op::Call(0), vec![Value::fixnum(20)])),
        ]),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    engine.set_call_env(CallEnv {
        debug_on_next_call: true,
        ..CallEnv::default()
    });
    let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
    assert_eq!(done_value(&mut engine, outcome), Value::fixnum(30));
    assert_eq!(
        engine.call_escapes(),
        &[CallEscape::DebugOnNextCall, CallEscape::DebugOnNextCall],
        "debug-on-next-call must win before target resolution"
    );
    assert_clear(&engine);
}

#[test]
fn vm_a_poll_inside_a_callee_publishes_every_frame_and_its_segment() {
    // V7 §11: R1 — a recording poll inside an owner-local callee sees EVERY
    // live frame's `function`/`code` and BOTH segments (including the caller's
    // preserved locals AND its callee designator + arguments kept in C5 layout).
    let mut ctx = context();
    let callee_sym_id = plain_sym("pvm-v7-poll-callee-sym");
    let callee_sym = Value::from_sym_id(callee_sym_id);

    // Callee takes 2 args (a, b), loops once to trigger `vm_back_edge`, returns a + b.
    let callee_bc = make_v7_bytecode(
        2,
        0,
        vec![
            Op::True,         // 0: [a, b, t]
            Op::GotoIfNil(4), // 1: [a, b]
            Op::Nil,          // 2: [a, b, nil]
            Op::Goto(1),      // 3: backward branch -> vm_back_edge poll
            Op::Add,          // 4: [a + b]
            Op::Return,       // 5
        ],
        vec![],
        6,
    );

    let caller_fn = Value::symbol("pvm-v7-poll-caller-fn");
    let caller_local = Value::fixnum(100);
    let arg_a = Value::fixnum(20);
    let arg_b = Value::fixnum(3);
    let code = HarnessVmCode::new(OwnedEntryProgram {
        ops: vec![
            Op::Constant(0), // 0: caller_local (100)
            Op::Constant(1), // 1: callee_sym
            Op::Constant(2), // 2: arg_a (20)
            Op::Constant(3), // 3: arg_b (3)
            Op::Call(2),     // 4: owner-local call via bare symbol
            Op::Add,         // 5: 100 + 23 = 123
            Op::Return,      // 6
        ],
        constants: vec![caller_local, callee_sym, arg_a, arg_b],
        function: caller_fn,
        code: caller_fn,
    });
    code.bind_symbol(callee_sym_id, callee_bc);

    let mut engine = VmExec::new(
        &(),
        RecordingEscape::new(),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    let outcome = engine.run_with_code(
        &mut ctx,
        &code,
        VmEntry {
            function: caller_fn,
            code: caller_fn,
            frame_bytes: bytecode_frame_bytes(6),
        },
    );
    assert_eq!(done_value(&mut engine, outcome), Value::fixnum(123));
    let poll = engine.poll_handler();
    assert_eq!(poll.edges, vec![PollEdge::VmCall, PollEdge::VmBackEdge]);
    // At index 1 (`VmBackEdge` inside callee), both frames are live!
    let snapshot = &poll.published_frames[1];
    assert_eq!(snapshot.len(), 2, "child (level 0) and caller (level 1)");
    let (child_fn, child_code, _child_pc, child_operands) = &snapshot[0];
    let (caller_seen_fn, caller_seen_code, caller_pc, caller_operands) = &snapshot[1];
    assert_eq!(
        *child_fn, callee_sym,
        "GNU backtrace records the symbol designator in child VmFrame.function"
    );
    assert_eq!(
        *child_code, callee_bc,
        "child VmFrame.code roots the resolved bytecode closure"
    );
    assert_eq!(
        *child_operands,
        vec![arg_a, arg_b, Value::NIL],
        "child segment holds its copied arguments plus loop flag"
    );
    assert_eq!(*caller_seen_fn, caller_fn);
    assert_eq!(*caller_seen_code, caller_fn);
    assert_eq!(
        *caller_pc,
        Some(Pc::new(5)),
        "caller saved_pc already points past Call(2)"
    );
    assert_eq!(
        *caller_operands,
        vec![caller_local, callee_sym, arg_a, arg_b],
        "C5: caller keeps preserved local, designator, and arguments until child returns"
    );
    assert_clear(&engine);
}

#[test]
fn vm_a_child_return_with_a_serial_marker_above_its_bind_base_takes_needs_serial_cleanup() {
    // V7 §11 (V4 interplay): a child frame with a serial marker above its own
    // `bind_base` takes a `Return` escape (`NeedsSerialCleanup`), and the
    // completion's (possibly debugger-replaced) value is delivered into the
    // caller's segment.
    let mut ctx = context();
    let child_marker = token(707);
    let child = make_v7_bytecode(
        0,
        0,
        vec![Op::SaveExcursion, Op::Constant(0), Op::Return],
        vec![Value::fixnum(9)],
        4,
    );
    let constants = [Value::fixnum(100), child];
    let ops = [
        Op::Constant(0), // 100
        Op::Constant(1), // child
        Op::Call(0),     // child returns 9, debugger replaces with 99
        Op::Add,         // 100 + 99 = 199
        Op::Return,
    ];
    let script = vec![
        Some(completion_pushing(&Op::SaveExcursion, child_marker)),
        Some(EscapeCompletion::Completed {
            consumed: 1,
            produced: vec![Value::fixnum(99)],
            jump: None,
            records: RecordDelta {
                popped: vec![child_marker],
                pushed: vec![],
            },
            handlers: HandlerDelta::default(),
        }),
    ];
    let mut engine = VmExec::new(
        &(),
        RecordingEscape::with_script(script),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
    assert_eq!(done_value(&mut engine, outcome), Value::fixnum(199));
    let esc = engine.escape_handler();
    assert_eq!(esc.requests.len(), 2, "child SaveExcursion + child Return");
    assert_eq!(esc.requests[1].op, Op::Return);
    assert_eq!(
        esc.requests[1].why,
        EscapeWhy::World(WorldSplit::Return(ReturnDisposition::NeedsSerialCleanup))
    );
    assert_clear(&engine);
}

#[test]
fn vm_parallel_and_serial_call_classifiers_agree_on_the_shape_table() {
    // V7 §11 (CRITICAL) & §14 r1: compare the production parallel classifier
    // (`classify_bytecode_function`) against serial
    // `Vm::can_enter_eager_interpreter_frame_iteratively` /
    // `Vm::iterative_admission_if_known` across real `ByteCodeFunction`s:
    // valid required, optional, env, &rest, dynamic, wrong nargs both ways,
    // max_stack > ITERATIVE_FRAME_LIMIT, nonrest > max_stack, unverified,
    // unsealed, AND lazy unset `OnceLock` -> `None` / `ColdAdmission`.
    let _ctx = context();
    let arena = FrameArena::new(32);
    let env = CallEnv::default();

    // 1. Valid required-only (1 arg)
    let req1 = make_v7_bytecode(1, 0, vec![Op::StackRef(0), Op::Return], vec![], 4);
    // 2. Valid required + optional (1 req, 2 opt -> accepts 1..=3)
    let opt12 = make_v7_bytecode(1, 2, vec![Op::StackRef(0), Op::Return], vec![], 6);
    // 3. Captured env
    let with_env = {
        let mut f = ByteCodeFunction::new(LambdaParams::simple(vec![plain_sym("pvm-v7-env-a")]));
        f.set_lexical(true);
        f.set_env(Some(Value::NIL));
        f.set_ops(vec![Op::Nil, Op::Return]);
        f.set_max_stack(StackDepth::for_test(4));
        Value::make_bytecode(f)
    };
    // 4. &rest parameter
    let with_rest = {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: vec![plain_sym("pvm-v7-rest-a")],
            optional: vec![],
            rest: Some(plain_sym("pvm-v7-rest-r")),
        });
        f.set_lexical(true);
        f.set_ops(vec![Op::Nil, Op::Return]);
        f.set_max_stack(StackDepth::for_test(4));
        Value::make_bytecode(f)
    };
    // 5. Dynamic params (lexical = false with named param)
    let dynamic = {
        let mut f = ByteCodeFunction::new(LambdaParams::simple(vec![plain_sym("pvm-v7-dyn-a")]));
        f.set_lexical(false);
        f.set_ops(vec![Op::Nil, Op::Return]);
        f.set_max_stack(StackDepth::for_test(4));
        Value::make_bytecode(f)
    };
    // 6. max_stack > StackDepth::ITERATIVE_FRAME_LIMIT
    let oversized_stack = make_v7_bytecode(
        0,
        0,
        vec![Op::Nil, Op::Return],
        vec![],
        StackDepth::ITERATIVE_FRAME_LIMIT.get() + 1,
    );
    // 7. nonrest > max_stack
    let nonrest_over_stack = make_v7_bytecode(2, 0, vec![Op::Nil, Op::Return], vec![], 1);
    // 8. Unverified ops (Pop at empty stack)
    let unverified = make_v7_bytecode(0, 0, vec![Op::Pop, Op::Return], vec![], 4);

    let eager_cases: Vec<(&str, Value, u16, Option<CallEscape>)> = vec![
        ("req1 valid", req1, 1, None),
        ("req1 too few", req1, 0, Some(CallEscape::WrongNargs)),
        ("req1 too many", req1, 2, Some(CallEscape::WrongNargs)),
        ("opt12 min", opt12, 1, None),
        ("opt12 mid", opt12, 2, None),
        ("opt12 max", opt12, 3, None),
        ("opt12 too few", opt12, 0, Some(CallEscape::WrongNargs)),
        ("opt12 too many", opt12, 4, Some(CallEscape::WrongNargs)),
        ("captured env", with_env, 1, Some(CallEscape::Ineligible)),
        ("&rest", with_rest, 1, Some(CallEscape::Ineligible)),
        ("dynamic params", dynamic, 1, Some(CallEscape::Ineligible)),
        (
            "oversized max_stack",
            oversized_stack,
            0,
            Some(CallEscape::Ineligible),
        ),
        (
            "nonrest > max_stack",
            nonrest_over_stack,
            2,
            Some(CallEscape::Ineligible),
        ),
        (
            "unverified ops",
            unverified,
            0,
            Some(CallEscape::Ineligible),
        ),
    ];

    for (label, val, nargs, expected_escape) in eager_cases {
        let bc = val.bytecode_data_if_materialized().expect("materialized");
        let serial_admitted =
            Vm::can_enter_eager_interpreter_frame_iteratively(bc, usize::from(nargs));
        let parallel_class = classify_bytecode_function(val, bc, nargs, &env, &arena);
        match expected_escape {
            None => {
                assert!(serial_admitted, "{label}: serial must admit");
                assert!(
                    matches!(parallel_class, CallClass::OwnerLocal { .. }),
                    "{label}: parallel must admit, got {parallel_class:?}"
                );
            }
            Some(reason) => {
                assert!(!serial_admitted, "{label}: serial must refuse");
                assert_eq!(
                    parallel_class,
                    CallClass::Escape(reason),
                    "{label}: parallel escape reason mismatch"
                );
            }
        }
    }

    // 9. Unsealed hand-constructed ByteCodeFunction (direct struct, not sealed):
    let mut unsealed = ByteCodeFunction::new(LambdaParams::simple(vec![]));
    unsealed.set_lexical(true);
    unsealed.set_ops(vec![Op::Nil, Op::Return]);
    unsealed.set_max_stack(StackDepth::for_test(4));
    assert!(!unsealed.executes_sealed_ops());
    assert!(!Vm::can_enter_eager_interpreter_frame_iteratively(
        &unsealed, 0
    ));
    assert_eq!(
        classify_bytecode_function(Value::NIL, &unsealed, 0, &env, &arena),
        CallClass::Escape(CallEscape::Ineligible),
        "unsealed function must be refused by both serial and parallel classifiers"
    );

    // 10. Lazy GNU ByteCodeFunction with unset OnceLock (§14 r1 :278):
    //     peek returns `None` -> `ColdAdmission`, and OnceLock stays unset!
    //     Wrong nargs on cold lazy returns `Some(false)` -> `WrongNargs` without
    //     initializing the OnceLock!
    //     After serial warms it, both agree on admission.
    let lazy_val = Value::make_bytecode(make_lazy_v7_bytecode(
        (2 << 8) | 1, // 1 req, 2 nonrest
        4,
        vec![192, 135], // Constant(0), Return
        vec![Value::fixnum(42)],
    ));
    let lazy_bc = lazy_val
        .bytecode_data_if_materialized()
        .expect("materialized");
    assert!(
        lazy_bc
            .lazy_gnu_code()
            .as_ref()
            .unwrap()
            .iterative_admission()
            .is_none()
    );
    assert_eq!(Vm::iterative_admission_if_known(lazy_bc, 1), None);
    assert_eq!(
        classify_bytecode_function(lazy_val, lazy_bc, 1, &env, &arena),
        CallClass::Escape(CallEscape::ColdAdmission)
    );
    assert!(
        lazy_bc
            .lazy_gnu_code()
            .as_ref()
            .unwrap()
            .iterative_admission()
            .is_none(),
        "ColdAdmission peek must leave lazy OnceLock unset"
    );
    assert_eq!(
        classify_bytecode_function(lazy_val, lazy_bc, 0, &env, &arena),
        CallClass::Escape(CallEscape::WrongNargs)
    );
    assert_eq!(
        classify_bytecode_function(lazy_val, lazy_bc, 3, &env, &arena),
        CallClass::Escape(CallEscape::WrongNargs)
    );
    assert!(
        lazy_bc
            .lazy_gnu_code()
            .as_ref()
            .unwrap()
            .iterative_admission()
            .is_none(),
        "WrongNargs on cold lazy function must keep OnceLock unset"
    );
    // Warm via serial lazy admission:
    assert_eq!(lazy_bc.lazy_iterative_admission(1), Some(true));
    assert_eq!(Vm::iterative_admission_if_known(lazy_bc, 1), Some(true));
    assert!(matches!(
        classify_bytecode_function(lazy_val, lazy_bc, 1, &env, &arena),
        CallClass::OwnerLocal { nonrest: 2, .. }
    ));
    assert_eq!(
        classify_bytecode_function(lazy_val, lazy_bc, 0, &env, &arena),
        CallClass::Escape(CallEscape::WrongNargs)
    );
}

#[test]
fn vm_a_setq_of_a_call_env_variable_escapes() {
    // V7 §4 (:135-138) & §11:
    // Part A: verify on a REAL `Context` via `era.admit_plain_symbol` (which
    // invokes the real `SymbolBacking::classify`) that ALL THREE `CallEnv`
    // variables (`max-lisp-eval-depth`, `debug-on-next-call`, and
    // `internal--compiler-function-overrides`) are refused!
    let depth_sym = intern("max-lisp-eval-depth");
    let debug_sym = intern("debug-on-next-call");
    let overrides_sym = intern(INTERNAL_COMPILER_FUNCTION_OVERRIDES);

    Context::with_test_mutator_era(|era| {
        // Positive control: an ordinary non-forwarded, non-projected plain
        // bootstrap symbol is admitted in the same era, so the three refusals
        // below cannot pass vacuously.
        let control = era.admit_plain_symbol(intern("frame--special-parameters"));
        assert!(
            control.is_ok(),
            "positive control: ordinary plain bootstrap symbol must be admitted, got {control:?}"
        );
        for (name, sym) in [
            ("max-lisp-eval-depth", depth_sym),
            ("debug-on-next-call", debug_sym),
            (INTERNAL_COMPILER_FUNCTION_OVERRIDES, overrides_sym),
        ] {
            let result = era.admit_plain_symbol(sym);
            assert_eq!(
                result,
                Err(RegistrationError::Publication),
                "{name}: real SymbolBacking::classify must refuse every CallEnv variable"
            );
        }
        finish(era);
    });

    // Part B: `VarSet` and `VarBind` of all three symbols escape through
    // `PlainRefused { refusal: NotPlain }` and the completion refreshes
    // `CallEnv` so the very next `Op::Call` sees the updated `CallEnv`!
    let mut ctx = context();
    let admitted_callee = make_v7_bytecode(
        0,
        0,
        vec![Op::Constant(0), Op::Return],
        vec![Value::fixnum(1)],
        4,
    );
    let sym_callee_id = plain_sym("pvm-v7-env-var-callee");
    let sym_callee = Value::from_sym_id(sym_callee_id);
    let bind_rec = token(901);

    let plain = HarnessPlain::new()
        .refuse(depth_sym, PlainRefusal::NotPlain)
        .refuse(debug_sym, PlainRefusal::NotPlain)
        .refuse(overrides_sym, PlainRefusal::NotPlain);

    let entry_code = Value::symbol("pvm-v7-env-vars-entry");
    let code = HarnessVmCode::new(OwnedEntryProgram {
        ops: vec![
            Op::Constant(3), // 1 -> setq max-lisp-eval-depth
            Op::VarSet(0),   // escapes -> refreshes depth_limit = 1
            Op::Constant(4), // admitted_callee
            Op::Call(0),     // escapes with Depth!
            Op::Pop,
            Op::True,        // t -> setq debug-on-next-call
            Op::VarSet(1),   // escapes -> refreshes debug_on_next_call = true
            Op::Constant(4), // admitted_callee
            Op::Call(0),     // escapes with DebugOnNextCall!
            Op::Pop,
            Op::True,        // t -> let-bind internal--compiler-function-overrides
            Op::VarBind(2),  // escapes -> pushes bind_rec & refreshes compiler_overrides = true
            Op::Constant(5), // sym_callee
            Op::Call(0),     // escapes with CompilerOverrides!
            Op::Unbind(1),
            Op::Return,
        ],
        constants: vec![
            Value::from_sym_id(depth_sym),
            Value::from_sym_id(debug_sym),
            Value::from_sym_id(overrides_sym),
            Value::fixnum(1),
            admitted_callee,
            sym_callee,
        ],
        function: entry_code,
        code: entry_code,
    });
    code.bind_symbol(sym_callee_id, admitted_callee);

    let script = vec![
        Some(completion(&Op::VarSet(0), vec![])),
        Some(completion(&Op::Call(0), vec![Value::fixnum(10)])),
        Some(completion(&Op::VarSet(1), vec![])),
        Some(completion(&Op::Call(0), vec![Value::fixnum(20)])),
        Some(completion_pushing(&Op::VarBind(2), bind_rec)),
        Some(completion(&Op::Call(0), vec![Value::fixnum(30)])),
        Some(completion_popping(&Op::Unbind(1), &[bind_rec])),
    ];
    let env_refreshes = vec![
        None, // run start
        Some(CallEnv {
            depth_limit: 1,
            ..CallEnv::default()
        }), // after VarSet(max-lisp-eval-depth)
        None, // after Call(Depth)
        Some(CallEnv {
            debug_on_next_call: true,
            ..CallEnv::default()
        }), // after VarSet(debug-on-next-call)
        None, // after Call(DebugOnNextCall)
        Some(CallEnv {
            compiler_overrides: true,
            ..CallEnv::default()
        }), // after VarBind(internal--compiler-function-overrides)
        None, // after Call(CompilerOverrides)
        Some(CallEnv::default()), // after Unbind(1)
    ];

    let mut engine = VmExec::new(
        &(),
        RecordingEscape::with_script(script).with_env_refreshes(env_refreshes),
        HarnessVmSealed,
        HarnessArith::new(),
        plain,
        RecordingPoll::new(),
    );
    let outcome = engine.run_with_code(
        &mut ctx,
        &code,
        VmEntry {
            function: entry_code,
            code: entry_code,
            frame_bytes: 0,
        },
    );
    assert_eq!(done_value(&mut engine, outcome), Value::fixnum(30));
    assert_eq!(
        engine.call_escapes(),
        &[
            CallEscape::Depth,
            CallEscape::DebugOnNextCall,
            CallEscape::CompilerOverrides,
        ],
        "each refreshed CallEnv immediately governed the subsequent Call"
    );
    let esc = engine.escape_handler();
    assert_eq!(
        esc.requests[0].why,
        EscapeWhy::PlainRefused {
            access: PlainVarAccess::Set,
            refusal: PlainRefusal::NotPlain,
        }
    );
    assert_eq!(
        esc.requests[2].why,
        EscapeWhy::PlainRefused {
            access: PlainVarAccess::Set,
            refusal: PlainRefusal::NotPlain,
        }
    );
    assert_eq!(
        esc.requests[4].why,
        EscapeWhy::PlainRefused {
            access: PlainVarAccess::Bind,
            refusal: PlainRefusal::NotPlain,
        }
    );
    assert_clear(&engine);
}

#[test]
fn vm_an_owner_local_call_reads_no_cold_admission() {
    // V7 §11 & §14 S1: a lazy GNU `ByteCodeFunction` whose admission `OnceLock`
    // is unset classifies as `ColdAdmission` (escapes), and BOTH its `decoded`
    // and `iterative_admission` cells stay unset after the call. Once the
    // serial side warms the cell, a subsequent call runs owner-local with zero
    // escapes.
    let mut ctx = context();
    let lazy_val = Value::make_bytecode(make_lazy_v7_bytecode(
        0,
        4,
        vec![192, 135], // Constant(0), Return
        vec![Value::fixnum(42)],
    ));
    let lazy_bc = lazy_val
        .bytecode_data_if_materialized()
        .expect("materialized");
    let lazy_cell = lazy_bc.lazy_gnu_code().as_ref().expect("lazy cell");
    assert!(lazy_cell.decoded().is_none());
    assert!(lazy_cell.iterative_admission().is_none());

    let constants = [lazy_val];
    let ops = [Op::Constant(0), Op::Call(0), Op::Return];

    // 1. Cold call: escapes with ColdAdmission, and cell remains completely cold.
    let mut cold_engine = VmExec::new(
        &(),
        RecordingEscape::with_script(vec![Some(completion(
            &Op::Call(0),
            vec![Value::fixnum(42)],
        ))]),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    let cold_out = cold_engine.run(&mut ctx, program_with(&ops, &constants));
    assert_eq!(done_value(&mut cold_engine, cold_out), Value::fixnum(42));
    assert_eq!(cold_engine.call_escapes(), &[CallEscape::ColdAdmission]);
    assert!(
        lazy_cell.decoded().is_none(),
        "parallel call must not initialize lazy decoded cell"
    );
    assert!(
        lazy_cell.iterative_admission().is_none(),
        "parallel call must not initialize lazy iterative_admission cell"
    );
    assert_clear(&cold_engine);

    // 2. Warm through serial lazy admission, then parallel call runs owner-local!
    assert_eq!(lazy_bc.lazy_iterative_admission(0), Some(true));
    let mut warm_engine = VmExec::new(
        &(),
        RecordingEscape::new(),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    let warm_out = warm_engine.run(&mut ctx, program_with(&ops, &constants));
    assert_eq!(done_value(&mut warm_engine, warm_out), Value::fixnum(42));
    assert_eq!(warm_engine.escape_handler().requests.len(), 0);
    assert!(warm_engine.call_escapes().is_empty());
    assert_clear(&warm_engine);
}

#[test]
fn vm_owner_local_call_with_bad_nonrest_contract_poisons_with_dispatch_contract() {
    // V7 review nit 1: a misbehaving `VmCode::classify_call` that returns
    // `OwnerLocal` with `nonrest < nargs` violates the classifier contract;
    // `step_call` must surface `EngineFault::DispatchContract(Op::Call(1))`
    // and poison the engine (matching `push_vm` refusal) rather than settling
    // cleanly via `EngineFault::StaleFrame`.
    struct BadNonrestCode {
        entry_ops: Vec<Op>,
        entry_constants: Vec<Value>,
        entry_code: Value,
        callee: Value,
    }
    impl VmCode for BadNonrestCode {
        fn program(&self, code: Value) -> Result<VmProgram<'_>, CodeRefusal> {
            if code == self.entry_code {
                return Ok(VmProgram {
                    ops: &self.entry_ops,
                    constants: &self.entry_constants,
                    function: self.entry_code,
                    code: self.entry_code,
                });
            }
            program_from_bytecode(code)
        }
        fn classify_call(
            &self,
            _designator: Value,
            _nargs: u16,
            _env: &CallEnv,
            _arena: &FrameArena,
        ) -> CallClass {
            // Contract violation: Call(1) has nargs = 1, but nonrest = 0.
            CallClass::OwnerLocal {
                code: self.callee,
                nonrest: 0,
                frame_bytes: 0,
            }
        }
    }

    let mut ctx = context();
    let callee = make_v7_bytecode(0, 0, vec![Op::Nil, Op::Return], vec![], 4);
    let entry_code = Value::symbol("pvm-v7-bad-nonrest-entry");
    let code = BadNonrestCode {
        entry_ops: vec![Op::Constant(0), Op::Constant(1), Op::Call(1), Op::Return],
        entry_constants: vec![callee, Value::fixnum(1)],
        entry_code,
        callee,
    };
    let mut engine = VmExec::new(
        &(),
        RecordingEscape::new(),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    let outcome = engine.run_with_code(
        &mut ctx,
        &code,
        VmEntry {
            function: entry_code,
            code: entry_code,
            frame_bytes: 0,
        },
    );
    let fault = fault_of(outcome);
    assert!(
        matches!(fault, EngineFault::DispatchContract(Op::Call(1))),
        "bad nonrest < nargs must fault with DispatchContract(Call(1)), got {fault:?}"
    );
    let second = engine.run_with_code(
        &mut ctx,
        &code,
        VmEntry {
            function: entry_code,
            code: entry_code,
            frame_bytes: 0,
        },
    );
    assert!(
        matches!(second, VmOutcome::Fault(EngineFault::Poisoned)),
        "DispatchContract fault must poison the engine, got {second:?}"
    );
}

// =========================================================================
// V8 — Handler landing pads and GNU-order unwind (V8-LANDING-PADS.md r1)
// =========================================================================

fn v8_push_handler(op: &Op, tok: SerialHandlerToken) -> ScriptStep {
    let effect = static_stack_effect(op);
    ScriptStep::Escape(EscapeCompletion::Completed {
        consumed: effect.consumed,
        produced: vec![Value::NIL; effect.produced as usize],
        jump: None,
        records: RecordDelta::default(),
        handlers: HandlerDelta {
            popped: Vec::new(),
            pushed: Some(tok),
        },
    })
}

fn v8_caught(tok: SerialHandlerToken, value: Value) -> ScriptStep {
    ScriptStep::Escape(EscapeCompletion::Caught {
        handler: tok,
        value,
    })
}

fn v8_escape_flow(flow: Flow) -> ScriptStep {
    ScriptStep::Escape(EscapeCompletion::Flow(flow))
}

fn v8_unwind_done(
    pop_handlers: &[SerialHandlerToken],
    pop_records: &[SerialRecordToken],
) -> ScriptStep {
    ScriptStep::Unwind(UnwindCompletion::Completed {
        popped_handlers: pop_handlers.to_vec(),
        popped_records: pop_records.to_vec(),
        pushed_records: Vec::new(),
    })
}

fn v8_unwind_restart(
    pop_handlers: &[SerialHandlerToken],
    pop_records: &[SerialRecordToken],
    new_target: SerialHandlerToken,
    value: Value,
) -> ScriptStep {
    ScriptStep::Unwind(UnwindCompletion::Caught {
        handler: new_target,
        value,
        popped_handlers: pop_handlers.to_vec(),
        popped_records: pop_records.to_vec(),
    })
}

fn v8_unwind_flow(flow: Flow) -> ScriptStep {
    ScriptStep::Unwind(UnwindCompletion::Flow(flow))
}

#[test]
fn vm_a_caught_signal_restores_the_operand_depth_marks_and_dest_in_the_same_frame() {
    // V8 §7 row 1 & S2.
    // GNU source contract (emacs-31.1 a360712c9d272d950d8d8255ef74570f7e90b7d9):
    // - src/bytecode.c:970-976 (`Bpushconditioncase`: records `c->bytecode_dest`
    //   and `c->bytecode_top = top` after any operand pop; in Neomacs's bytecode
    //   IR `Op::PushConditionCase` has `consumed = 0` while
    //   `Op::PushConditionCaseRaw` and `Op::PushCatch` have `consumed = 1`,
    //   both exercised below);
    // - src/eval.c:1857-1859 (`push_handler_nosignal`: records `f_lisp_eval_depth`,
    //   `pdlcount`, `act_rec`);
    // - src/bytecode.c:983-1003 (landing: `handlerlist = c->next`,
    //   `top = c->bytecode_top`, `arg = c->bytecode_dest`, `PUSH(c->val)`,
    //   `goto op_branch`).
    // Mirrors Elisp:
    //   (let ((a 10))
    //     (+ a (condition-case e
    //              (let ((x 99)) (progn (push-temps) (signal 'error '(42))))
    //            (error e))))  => 52, with operand_len == 1 restored before push.
    let mut ctx = context();
    let x = plain_sym("pvm-v8-same-frame-x");
    let h1 = handler(11);
    let constants = [
        Value::fixnum(10),     // 0: caller operand kept below handler (`operand_len == 1`)
        Value::from_sym_id(x), // 1: dynamic var bound inside protected body
        Value::fixnum(99),     // 2: temporary binding value
        Value::fixnum(777),    // 3: extra operand junk pushed inside body
        Value::fixnum(888),    // 4: extra operand junk pushed inside body
    ];
    let ops = [
        Op::Constant(0),          // 0: [10] -> operand_len at push = 1 (explicit S2 constant)
        Op::PushConditionCase(9), // 1: installs h1 with operand_len=1, bind_mark=0, dest=9
        Op::Constant(2),          // 2: [10, 99]
        Op::VarBind(1),           // 3: binds x=99 (bind_mark 0 -> 1)
        Op::Constant(3),          // 4: [10, 777] (extra temp)
        Op::Constant(4),          // 5: [10, 777, 888] (extra temp)
        Op::SymbolValue,          // 6: signals -> Caught { h1, 42 }
        Op::PopHandler,           // 7: skipped
        Op::Return,               // 8: skipped
        // dest = 9: segment truncated to operand_len=1 ([10]) + caught value 42 => [10, 42]
        Op::Add,    // 9: 10 + 42 = 52
        Op::Return, // 10
    ];
    let script = vec![
        Some(v8_push_handler(&Op::PushConditionCase(9), h1)),
        Some(v8_caught(h1, Value::fixnum(42))),
        Some(v8_unwind_done(&[h1], &[])),
    ];
    let mut engine = VmExec::new(
        &(),
        RecordingEscape::with_steps(script),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new().admit(x),
        RecordingPoll::new(),
    );
    let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
    assert_eq!(done_value(&mut engine, outcome), Value::fixnum(52));
    assert!(
        engine.plain_handler().live_records().is_empty(),
        "body's Plain binding x=99 must be unwound back to bind_mark=0"
    );
    assert_clear(&engine);

    // Part B (MEDIUM-2.4): `Op::PushConditionCaseRaw` (`consumed = 1`, popping
    // the condition list operand) computes `operand_len = available - 1`
    // (`src/bytecode.c:974-976`: `top` AFTER `POP`), preserving `[10]` below.
    let h_raw = handler(12);
    let raw_ops = [
        Op::Constant(0),             // 0: [10]
        Op::Nil,                     // 1: [10, cond-list] (popped by PushConditionCaseRaw)
        Op::PushConditionCaseRaw(7), // 2: available=2, consumed=1 -> operand_len=1 ([10]), dest=7
        Op::Constant(3),             // 3: [10, 777]
        Op::SymbolValue,             // 4: Caught { h_raw, 42 }
        Op::PopHandler,              // 5: skipped
        Op::Return,                  // 6: skipped
        Op::Add,                     // 7: [10, 42] -> 52
        Op::Return,                  // 8
    ];
    let raw_script = vec![
        Some(v8_push_handler(&Op::PushConditionCaseRaw(7), h_raw)),
        Some(v8_caught(h_raw, Value::fixnum(42))),
        Some(v8_unwind_done(&[h_raw], &[])),
    ];
    let mut raw_engine = VmExec::new(
        &(),
        RecordingEscape::with_steps(raw_script),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    let raw_out = raw_engine.run(&mut ctx, program_with(&raw_ops, &constants));
    assert_eq!(done_value(&mut raw_engine, raw_out), Value::fixnum(52));
    assert_clear(&raw_engine);
}

#[test]
fn vm_the_landing_pad_pc_is_the_handler_dest_not_the_next_pc() {
    // V8 §7 row 2 (GNU-observable shape: condition-case body after signal never executes).
    // GNU source contract (emacs-31.1 a360712c9d272d950d8d8255ef74570f7e90b7d9):
    // - src/bytecode.c:975 (`c->bytecode_dest = FETCH2`);
    // - src/bytecode.c:986, 1003 (`arg = c->bytecode_dest; ... goto op_branch`).
    // Mirrors Elisp:
    //   (condition-case e
    //       (progn (signal 'error '(777)) 999)
    //     (error (+ e 1000)))  => 1777 (not 999 at next_pc).
    let mut ctx = context();
    let h1 = handler(21);
    let constants = [Value::fixnum(999), Value::fixnum(1000)];
    let ops = [
        Op::PushConditionCase(6), // 0: dest = 6
        Op::Nil,                  // 1
        Op::SymbolValue,          // 2: signals -> Caught { h1, 777 }
        // next_pc = 3 MUST NOT execute (would push 999 and return 999)
        Op::Constant(0), // 3: unreachable
        Op::PopHandler,  // 4: unreachable
        Op::Return,      // 5: unreachable
        // dest = 6: handler body receives 777 on stack and adds 1000 -> 1777
        Op::Constant(1), // 6
        Op::Add,         // 7
        Op::Return,      // 8
    ];
    let script = vec![
        Some(v8_push_handler(&Op::PushConditionCase(6), h1)),
        Some(v8_caught(h1, Value::fixnum(777))),
        Some(v8_unwind_done(&[h1], &[])),
    ];
    let mut engine = VmExec::new(
        &(),
        RecordingEscape::with_steps(script),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
    assert_eq!(
        done_value(&mut engine, outcome),
        Value::fixnum(1777),
        "execution must land at handler dest=6 (777 + 1000), never fall through to next_pc=3"
    );
    assert_clear(&engine);
}

#[test]
fn vm_a_signal_with_no_matching_handler_still_settles_to_idle() {
    // V8 §7 row 3 / L7 (GNU-observable shape: unmatched signal propagates out).
    // GNU source contract (emacs-31.1 a360712c9d272d950d8d8255ef74570f7e90b7d9):
    // - src/eval.c:1897-1928 (`Fsignal` -> `signal_or_quit`);
    // - src/eval.c:1996-2070 (`signal_or_quit`: walks `handlerlist`, no matching
    //   `CONDITION_CASE` clause -> escapes to top-level / thread boundary);
    // - src/eval.c:1722-1741 (`internal_condition_case_1`).
    // Mirrors Elisp:
    //   (condition-case nil (signal 'arith-error nil) (wrong-type-argument 'nope))
    //   => uncaught `arith-error`.
    let mut ctx = context();
    let h1 = handler(31);
    let ops = [
        Op::PushConditionCase(4), // 0: non-matching handler
        Op::Nil,                  // 1
        Op::SymbolValue,          // 2: serial returns Flow (no owner handler matched)
        Op::PopHandler,           // 3
        Op::Return,               // 4
    ];
    let script = vec![
        Some(v8_push_handler(&Op::PushConditionCase(4), h1)),
        Some(v8_escape_flow(signal("arith-error", vec![]))),
    ];
    let mut engine = VmExec::new(
        &(),
        RecordingEscape::with_steps(script),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    let outcome = engine.run(&mut ctx, program(&ops));
    let VmOutcome::Signaled(flow) = outcome else {
        panic!("expected Signaled for unmatched Flow, got {outcome:?}");
    };
    assert_eq!(signal_key(&flow).0, "arith-error");
    assert_eq!(engine.arena_depth(), 0);
    assert_eq!(engine.arena().bc_position(), 0);
    assert_eq!(engine.live_root_count(), 0);
    assert_clear(&engine);

    // Subsequent run on the settled Idle engine works cleanly.
    let ok = engine.run(
        &mut ctx,
        program_with(&[Op::Constant(0), Op::Return], &[Value::fixnum(7)]),
    );
    assert_eq!(done_value(&mut engine, ok), Value::fixnum(7));
    assert_clear(&engine);
}

#[test]
fn vm_a_signal_in_a_v7_callee_lands_in_the_callers_handler_and_pops_the_callee_frames() {
    // V8 §7 row 4 / L4: depth 3 down to 1, CallDepth & bc_position refunded per
    // popped frame, `ExitFlags::DEBUG_ON_EXIT` armed on BOTH popped callees
    // (`leaf` and `mid`) and suppressed (no exit hooks fired on nonlocal unwind),
    // caller segment restored to `operand_len + 1`.
    // GNU source contract (emacs-31.1 a360712c9d272d950d8d8255ef74570f7e90b7d9):
    // - src/eval.c:1445-1459 (`unwind_to_catch`: `unbind_to`,
    //   `lisp_eval_depth = catch->f_lisp_eval_depth`, `set_act_rec`);
    // - src/bytecode.c:983-1003 (restores caller's `fp`, `top`, `pc`);
    // - src/bytecode.c:900 (`debug-on-exit` runs only on normal `Breturn`,
    //   never on nonlocal unwind).
    // Mirrors Elisp:
    //   (+ 100 (condition-case e (mid (leaf-that-signals)) (error e))) => 155.
    let mut ctx = context();
    let h1 = handler(41);
    let leaf_x = plain_sym("pvm-v8-leaf-x");

    // Depth 3: leaf binds leaf_x=5, arms DEBUG_ON_EXIT on leaf (level 0) and
    // mid (level 1) during an escape, then signals -> Caught { h1, 55 }.
    let leaf = make_v7_bytecode(
        0,
        0,
        vec![
            Op::Constant(0), // 5
            Op::VarBind(1),  // bind leaf_x = 5
            Op::Nil,
            Op::SymbolValue, // escape #1: arms DEBUG_ON_EXIT on leaf & mid
            Op::SymbolValue, // escape #2: signals -> Caught { h1, 55 }
            Op::Unbind(1),
            Op::Return,
        ],
        vec![Value::fixnum(5), Value::from_sym_id(leaf_x)],
        8,
    );
    // Depth 2: mid calls leaf
    let mid = make_v7_bytecode(
        0,
        0,
        vec![Op::Constant(0), Op::Call(0), Op::Return],
        vec![leaf],
        6,
    );
    let caller_bytes = bytecode_frame_bytes(4);
    let entry_code = Value::symbol("pvm-v8-depth3-caller");
    let code = HarnessVmCode::new(OwnedEntryProgram {
        ops: vec![
            Op::Constant(0),          // 0: [100] (operand_len = 1 at PushConditionCase)
            Op::PushConditionCase(6), // 1: installs h1 at depth=1, operand_len=1, dest=6
            Op::Constant(1),          // 2: [100, mid]
            Op::Call(0),              // 3: depth 1 -> 2 (mid) -> 3 (leaf)
            Op::PopHandler,           // 4: skipped
            Op::Return,               // 5: skipped
            // dest = 6: [100, 55] -> 155
            Op::Add,    // 6
            Op::Return, // 7
        ],
        constants: vec![Value::fixnum(100), mid],
        function: entry_code,
        code: entry_code,
    });
    let script = vec![
        Some(v8_push_handler(&Op::PushConditionCase(6), h1)),
        Some(ScriptStep::Escape(EscapeCompletion::fall_through(
            &Op::SymbolValue,
        ))),
        Some(v8_caught(h1, Value::fixnum(55))),
        Some(v8_unwind_done(&[h1], &[])),
    ];
    // Escape index 1 (inside `leaf` at depth 3) arms DEBUG_ON_EXIT on level 0
    // (`leaf`) and level 1 (`mid`) so the unwind proves neither fires an exit hook.
    let escape = RecordingEscape::with_steps(script)
        .with_debug_script(vec![vec![], vec![(0, true), (1, true)]]);
    let mut engine = VmExec::new(
        &(),
        escape,
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new().admit(leaf_x),
        RecordingPoll::new(),
    );
    let outcome = engine.run_with_code(
        &mut ctx,
        &code,
        VmEntry {
            function: entry_code,
            code: entry_code,
            frame_bytes: caller_bytes,
        },
    );
    assert_eq!(done_value(&mut engine, outcome), Value::fixnum(155));
    assert_eq!(
        engine.max_depth(),
        3,
        "reached depth 3 before catching at depth 1"
    );
    assert_eq!(
        engine.arena().bc_position(),
        0,
        "bc_position refunded for both popped callees and returning caller"
    );
    assert!(engine.plain_handler().live_records().is_empty());
    assert_eq!(
        engine.escape_handler().requests.len(),
        4,
        "PushConditionCase, arming SymbolValue, signaling SymbolValue(Caught), final SerialUnwind(pop h1); zero Return/debug-on-exit escapes despite DEBUG_ON_EXIT armed on leaf and mid"
    );
    assert!(
        engine
            .escape_handler()
            .requests
            .iter()
            .all(|r| r.op != Op::Return),
        "nonlocal unwind must never fire debug-on-exit on popped callees"
    );
    assert_clear(&engine);
}

#[test]
fn vm_interleaved_owner_and_serial_records_unwind_in_lifo_order() {
    // V8 §7 row 5 & #14586 condition 4 (highest-risk interleaved Plain/Serial row):
    // Explicit LIFO above h1's bind_mark:
    //   [Plain a=1, Serial u1, Plain w=2 (gains watcher!), Plain b=3, Serial u2]
    // GNU source contract (emacs-31.1 a360712c9d272d950d8d8255ef74570f7e90b7d9):
    // - src/eval.c:1445-1454 (`unwind_to_catch`: `unbind_to(handlerlist->pdlcount, Qnil)`);
    // - src/eval.c:3946-3958 (`unbind_to`: LIFO decrement-before-do_one_unbind);
    // - src/eval.c:3848-3870 (`do_one_unbind` for `SPECPDL_LET`: re-reads
    //   `redirect`/`trapped_write`; untrapped plain restores directly, watched
    //   routes through `set_internal(..., SET_INTERNAL_UNBIND)`).
    // Mirrors Elisp:
    //   (condition-case e
    //       (let ((a 1))
    //         (unwind-protect
    //             (let ((w 2)) ;; w gains watcher before unwind!
    //               (let ((b 3))
    //                 (unwind-protect (signal 'error '(boom)) (u2))))
    //           (u1)))
    //     (error e))
    let mut ctx = context();
    let a = plain_sym("pvm-v8-interleave-a");
    let w = plain_sym("pvm-v8-interleave-w");
    let b = plain_sym("pvm-v8-interleave-b");
    let h1 = handler(51);
    let u1 = token(101);
    let u2 = token(202);
    let log = EffectLog::default();

    let constants = [
        Value::from_sym_id(a), // 0
        Value::from_sym_id(w), // 1
        Value::from_sym_id(b), // 2
        Value::fixnum(1),      // 3
        Value::fixnum(2),      // 4
        Value::fixnum(3),      // 5
    ];
    let ops = [
        Op::PushConditionCase(12), // 0: installs h1 at bind_mark=0, dest=12
        Op::Constant(3),           // 1: 1
        Op::VarBind(0),            // 2: Plain a = 1
        Op::SaveExcursion,         // 3: Serial u1
        Op::Constant(4),           // 4: 2
        Op::VarBind(1),            // 5: Plain w = 2 (will be watched at unbind!)
        Op::Constant(5),           // 6: 3
        Op::VarBind(2),            // 7: Plain b = 3
        Op::SaveExcursion,         // 8: Serial u2
        Op::Nil,                   // 9
        Op::SymbolValue,           // 10: signals -> Caught { h1, 505 }
        Op::Return,                // 11: skipped
        Op::Return,                // 12: dest -> returns caught 505
    ];
    let script = vec![
        Some(v8_push_handler(&Op::PushConditionCase(12), h1)),
        Some(ScriptStep::Escape(completion_pushing(
            &Op::SaveExcursion,
            u1,
        ))),
        Some(ScriptStep::Escape(completion_pushing(
            &Op::SaveExcursion,
            u2,
        ))),
        Some(v8_caught(h1, Value::fixnum(505))),
        // Unwind step 1: Serial u2 (while b=3 is still bound!)
        Some(v8_unwind_done(&[], &[u2])),
        // Unwind step 2: Plain b=3 undone owner-locally -> slot restored to 2!
        // Unwind step 3: Plain w=2 gained a watcher -> PlainUnbind handoff to serial!
        Some(v8_unwind_done(&[], &[])),
        // Unwind step 4: Serial u1 (runs AFTER b and w were both unbound!)
        Some(v8_unwind_done(&[], &[u1])),
        // Unwind step 5: Plain a=1 undone owner-locally -> slot restored to none!
        // Unwind step 6: Final Lisp-free handler pop of h1 + landing!
        Some(v8_unwind_done(&[h1], &[])),
    ];
    let plain = HarnessPlain::new()
        .admit(a)
        .admit(w)
        .admit(b)
        .watched(w)
        .logging(log.clone());
    let mut engine = VmExec::new(
        &(),
        RecordingEscape::logging_with_steps(script, log.clone()),
        HarnessVmSealed,
        HarnessArith::new(),
        plain,
        RecordingPoll::new(),
    );
    let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
    assert_eq!(done_value(&mut engine, outcome), Value::fixnum(505));
    assert_eq!(
        log.entries(),
        vec![
            "escape PushConditionCase(12)",
            "slot <- 1",
            "escape SaveExcursion",
            "slot <- 2",
            "slot <- 3",
            "escape SaveExcursion",
            "escape SymbolValue",
            // Exact LIFO unwind: u2 -> b (owner) -> w (watched serial handoff) -> u1 -> a (owner) -> pop h1
            "escape SymbolValue [202]",
            "slot <- 2",
            "escape SymbolValue [plain]",
            "slot <- 1",
            "escape SymbolValue [101]",
            "slot <- none",
            "escape SymbolValue [h:51 r:]",
        ],
        "exact GNU LIFO unwind order across Plain, watched Plain, and Serial records"
    );
    // MEDIUM-2.5: prove the PlainUnbind hand-off specifically named `w`.
    assert!(
        matches!(
            engine.escape_handler().requests[5].why,
            EscapeWhy::PlainUnbind { symbol, .. } if symbol == w
        ),
        "interleaved watched hand-off must specifically name symbol `w`"
    );
    assert!(engine.plain_handler().live_records().is_empty());
    assert_clear(&engine);
}

#[test]
fn vm_handler_pops_interleave_with_record_unbinding_in_gnu_order() {
    // V8 §7 row 6 / L3:
    // GNU source contract (emacs-31.1 a360712c9d272d950d8d8255ef74570f7e90b7d9):
    // - src/eval.c:1445-1454 (`unwind_to_catch`: `do { unbind_to(handlerlist->pdlcount, Qnil);
    //   last_time = handlerlist == catch; if (!last_time) handlerlist = handlerlist->next; }`).
    // Mirrors Elisp:
    //   (condition-case e1
    //       (unwind-protect
    //           (condition-case e2
    //               (unwind-protect (signal 'e1 nil) (u-inner))
    //             (e2 'inner))
    //         (u-outer))
    //     (e1 'outer))
    // Proves h2 stays installed on HandlerStack while u-inner runs (`pop_handlers: []`),
    // then h2 is popped in `pop_handlers: [h2]` alongside u-outer, then h1 is popped last.
    let mut ctx = context();
    let h1 = handler(61);
    let h2 = handler(62);
    let u_outer = token(601);
    let u_inner = token(602);
    let ops = [
        Op::PushConditionCase(8), // 0: h1, dest=8
        Op::SaveExcursion,        // 1: u_outer
        Op::PushConditionCase(7), // 2: h2, dest=7
        Op::SaveExcursion,        // 3: u_inner
        Op::Nil,                  // 4
        Op::SymbolValue,          // 5: Caught { h1, 610 }
        Op::Return,               // 6: skipped
        Op::Return,               // 7: h2 dest (skipped)
        Op::Return,               // 8: h1 dest -> 610
    ];
    let script = vec![
        Some(v8_push_handler(&Op::PushConditionCase(8), h1)),
        Some(ScriptStep::Escape(completion_pushing(
            &Op::SaveExcursion,
            u_outer,
        ))),
        Some(v8_push_handler(&Op::PushConditionCase(7), h2)),
        Some(ScriptStep::Escape(completion_pushing(
            &Op::SaveExcursion,
            u_inner,
        ))),
        Some(v8_caught(h1, Value::fixnum(610))),
        // Step 1: u_inner runs with h2 STILL installed (`pop_handlers: []`)!
        Some(v8_unwind_done(&[], &[u_inner])),
        // Step 2: h2 popped before u_outer runs (`pop_handlers: [h2], unbind: [u_outer]`)!
        Some(v8_unwind_done(&[h2], &[u_outer])),
        // Step 3: final pop of h1 + land!
        Some(v8_unwind_done(&[h1], &[])),
    ];
    let mut engine = VmExec::new(
        &(),
        RecordingEscape::with_steps(script),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    let outcome = engine.run(&mut ctx, program(&ops));
    assert_eq!(done_value(&mut engine, outcome), Value::fixnum(610));
    let esc = engine.escape_handler();
    assert_eq!(esc.requests.len(), 8);
    // Request 5: u_inner with h2 STILL on HandlerStack!
    assert_eq!(
        esc.requests[5].why,
        EscapeWhy::SerialUnwind(SerialUnwind {
            pop_handlers: vec![],
            unbind: vec![u_inner],
        })
    );
    assert_eq!(
        esc.lifos[5].1,
        vec![Some(h1.0), Some(h2.0)],
        "h2 must remain installed while its own records (u_inner) unbind (eval.c:1449-1452)"
    );
    // Request 6: h2 popped alongside u_outer!
    assert_eq!(
        esc.requests[6].why,
        EscapeWhy::SerialUnwind(SerialUnwind {
            pop_handlers: vec![h2],
            unbind: vec![u_outer],
        })
    );
    // Request 7: final pop of h1!
    assert_eq!(
        esc.requests[7].why,
        EscapeWhy::SerialUnwind(SerialUnwind {
            pop_handlers: vec![h1],
            unbind: vec![],
        })
    );
    assert_eq!(esc.lifos[7].1, vec![Some(h1.0)]);
    assert_clear(&engine);
}

#[test]
fn vm_a_cleanup_that_signals_during_unwind_is_caught_by_the_intermediate_handler() {
    // V8 §7 row 7 / L5 & #14586 condition 4:
    // GNU source contract (emacs-31.1 a360712c9d272d950d8d8255ef74570f7e90b7d9):
    // - src/eval.c:1445-1454 (`h2` stays installed while `unbind_to(h2->pdlcount)` runs);
    // - src/eval.c:3948-3957 (`specpdl_ptr` decremented BEFORE `do_one_unbind`, so
    //   records already unbound before the cleanup signals are NOT unbound again);
    // - src/eval.c:1996-2064 (nested `signal_or_quit` redirects to `h2`).
    // Mirrors Elisp:
    //   (condition-case e1
    //       (unwind-protect
    //           (condition-case e2
    //               (unwind-protect
    //                   (let ((p 1))
    //                     (unwind-protect (signal 'e1 nil) (u-done)))
    //                 (u-raises-e2))
    //             (e2 e2))
    //         (u-outer))
    //     (e1 'outer))
    //   => e2 caught by h2; u-done & p_done & u-raises-e2 each unwound ONCE;
    //      u-outer NOT unwound during catch (still live for normal exit).
    let mut ctx = context();
    let p = plain_sym("pvm-v8-restart-p");
    let h1 = handler(71);
    let h2 = handler(72);
    let u_outer = token(701);
    let u_raises = token(702);
    let u_done = token(703);
    let log = EffectLog::default();

    let constants = [Value::from_sym_id(p), Value::fixnum(1)];
    let ops = [
        Op::PushConditionCase(12), // 0: h1, dest=12
        Op::SaveExcursion,         // 1: u_outer (below h2)
        Op::PushConditionCase(9),  // 2: h2, dest=9
        Op::SaveExcursion,         // 3: u_raises (signals e2 during unwind!)
        Op::Constant(1),           // 4: 1
        Op::VarBind(0),            // 5: Plain p = 1 (above u_raises)
        Op::SaveExcursion,         // 6: u_done (already unbound before u_raises!)
        Op::Nil,                   // 7
        Op::SymbolValue,           // 8: initial signal targeting h1!
        // dest = 9 (h2 handler!): caught 404, then cleanly unbind u_outer & pop h1
        Op::Unbind(1),  // 9: unbind u_outer cleanly after landing in h2
        Op::PopHandler, // 10: pop h1 cleanly
        Op::Return,     // 11: return 404
        Op::Return,     // 12: h1 dest (never reached!)
    ];
    let script = vec![
        Some(v8_push_handler(&Op::PushConditionCase(12), h1)),
        Some(ScriptStep::Escape(completion_pushing(
            &Op::SaveExcursion,
            u_outer,
        ))),
        Some(v8_push_handler(&Op::PushConditionCase(9), h2)),
        Some(ScriptStep::Escape(completion_pushing(
            &Op::SaveExcursion,
            u_raises,
        ))),
        Some(ScriptStep::Escape(completion_pushing(
            &Op::SaveExcursion,
            u_done,
        ))),
        // Initial signal targets outer h1:
        Some(v8_caught(h1, Value::fixnum(111))),
        // Step 1: u_done runs and succeeds -> popped!
        Some(v8_unwind_done(&[], &[u_done])),
        // Step 2: Plain p=1 unwinds owner-locally -> restored to none!
        // Step 3: u_raises runs and SIGNALS -> caught by intermediate h2!
        // u_raises itself left specpdl before its body ran (`eval.c:3955`).
        Some(v8_unwind_restart(&[], &[u_raises], h2, Value::fixnum(404))),
        // Restarted unwind targets h2: all records above h2.bind_mark are ALREADY
        // gone (u_done, p, u_raises are NOT unbound a second time!). One final
        // pop of h2 + land at dest=9!
        Some(v8_unwind_done(&[h2], &[])),
        // Normal exit after h2 landed: Unbind(1) pops u_outer, PopHandler pops h1!
        Some(v8_unwind_done(&[], &[u_outer])),
        Some(ScriptStep::Escape(EscapeCompletion::Completed {
            consumed: 0,
            produced: Vec::new(),
            jump: None,
            records: RecordDelta::default(),
            handlers: HandlerDelta {
                popped: vec![h1],
                pushed: None,
            },
        })),
    ];
    let mut engine = VmExec::new(
        &(),
        RecordingEscape::logging_with_steps(script, log.clone()),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new().admit(p).logging(log.clone()),
        RecordingPoll::new(),
    );
    let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
    assert_eq!(
        done_value(&mut engine, outcome),
        Value::fixnum(404),
        "restart must land in intermediate h2 with 404, not outer h1"
    );
    assert_eq!(
        log.entries(),
        vec![
            "escape PushConditionCase(12)",
            "escape SaveExcursion",
            "escape PushConditionCase(9)",
            "escape SaveExcursion",
            "slot <- 1",
            "escape SaveExcursion",
            "escape SymbolValue",
            // Unwind toward h1: u_done once, p once, u_raises once (restarts to h2!)
            "escape SymbolValue [703]",
            "slot <- none",
            "escape SymbolValue [702]",
            // Restarted unwind to h2: zero double-unbinds, pops h2 and lands at pc=9
            "escape SymbolValue [h:72 r:]",
            // Post-landing normal execution inside h1: u_outer still ran on normal Unbind!
            "escape Unbind(1) [701]",
            "escape PopHandler",
        ],
        "already-unbound records (u_done, p, u_raises) must never be unbound twice on restart"
    );
    assert_clear(&engine);
}

#[test]
fn vm_throw_to_an_owner_catch_lands_with_the_thrown_value() {
    // V8 §7 row 8 (GNU-observable shape: `(catch 'k (throw 'k 314))`).
    // GNU source contract (emacs-31.1 a360712c9d272d950d8d8255ef74570f7e90b7d9):
    // - src/bytecode.c:967-969, 974-1004 (`Bpushcatch` pops tag, sets `bytecode_top`
    //   and `bytecode_dest`; landing pushes `c->val` and branches to `dest`);
    // - src/eval.c:1464-1481 (`Fthrow`: matches `CATCHER` by `EQ(tag)`, calls
    //   `unwind_to_catch(c, NONLOCAL_EXIT_THROW, value)`).
    // Mirrors Elisp:
    //   (+ 1 (catch 'k (let ((x 9)) (throw 'k 314))))  => 315.
    let mut ctx = context();
    let x = plain_sym("pvm-v8-catch-x");
    let tag = Value::symbol("pvm-v8-catch-tag");
    let h_catch = handler(81);
    let constants = [
        Value::fixnum(1),      // 0: preserved below catch (`operand_len == 1`)
        tag,                   // 1: catch tag
        Value::from_sym_id(x), // 2: let-bound symbol
        Value::fixnum(9),      // 3: let value
        Value::fixnum(314),    // 4: thrown value
    ];
    let ops = [
        Op::Constant(0),  // 0: [1]
        Op::Constant(1),  // 1: [1, 'k]
        Op::PushCatch(9), // 2: pops 'k -> operand_len == 1 ([1]), dest = 9
        Op::Constant(3),  // 3: [1, 9]
        Op::VarBind(2),   // 4: bind x = 9
        Op::Constant(1),  // 5: [1, 'k]
        Op::Constant(4),  // 6: [1, 'k, 314]
        Op::Throw,        // 7: escapes -> Caught { h_catch, 314 }
        Op::Return,       // 8: skipped
        // dest = 9: [1, 314] -> Add -> 315
        Op::Add,    // 9
        Op::Return, // 10
    ];
    let script = vec![
        Some(v8_push_handler(&Op::PushCatch(9), h_catch)),
        Some(v8_caught(h_catch, Value::fixnum(314))),
        Some(v8_unwind_done(&[h_catch], &[])),
    ];
    let mut engine = VmExec::new(
        &(),
        RecordingEscape::with_steps(script),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new().admit(x),
        RecordingPoll::new(),
    );
    let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
    assert_eq!(done_value(&mut engine, outcome), Value::fixnum(315));
    assert!(engine.plain_handler().live_records().is_empty());
    assert_clear(&engine);
}

#[test]
fn vm_a_pure_owner_unwind_issues_exactly_one_escape() {
    // V8 §7 row 9 (§5 :108): with only owner-local Plain bindings and zero
    // serial records above the handler's `bind_mark`, the unwind loop issues
    // EXACTLY ONE escape (the final handler pop + landing).
    // GNU source contract (emacs-31.1 a360712c9d272d950d8d8255ef74570f7e90b7d9):
    // - src/eval.c:1445-1454, 3848-3859 (untrapped PLAINVAL bindings restore
    //   inline with no Lisp callbacks).
    let mut ctx = context();
    let x = plain_sym("pvm-v8-pure-x");
    let y = plain_sym("pvm-v8-pure-y");
    let h1 = handler(91);
    let constants = [
        Value::from_sym_id(x),
        Value::from_sym_id(y),
        Value::fixnum(1),
        Value::fixnum(2),
    ];
    let ops = [
        Op::PushConditionCase(8), // 0: h1
        Op::Constant(2),          // 1
        Op::VarBind(0),           // 2: Plain x = 1
        Op::Constant(3),          // 3
        Op::VarBind(1),           // 4: Plain y = 2
        Op::Nil,                  // 5
        Op::SymbolValue,          // 6: Caught { h1, 910 }
        Op::Return,               // 7: skipped
        Op::Return,               // 8: dest -> 910
    ];
    let script = vec![
        Some(v8_push_handler(&Op::PushConditionCase(8), h1)),
        Some(v8_caught(h1, Value::fixnum(910))),
        Some(v8_unwind_done(&[h1], &[])),
    ];
    let mut engine = VmExec::new(
        &(),
        RecordingEscape::with_steps(script),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new().admit(x).admit(y),
        RecordingPoll::new(),
    );
    let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
    assert_eq!(done_value(&mut engine, outcome), Value::fixnum(910));
    let esc = engine.escape_handler();
    let unwind_requests: Vec<_> = esc
        .requests
        .iter()
        .filter(|r| matches!(r.why, EscapeWhy::SerialUnwind(_)))
        .collect();
    assert_eq!(
        unwind_requests.len(),
        1,
        "pure owner unwind must issue exactly ONE SerialUnwind escape (the final pop+landing)"
    );
    assert_eq!(
        unwind_requests[0].why,
        EscapeWhy::SerialUnwind(SerialUnwind {
            pop_handlers: vec![h1],
            unbind: vec![],
        })
    );
    assert!(engine.plain_handler().live_records().is_empty());
    assert_clear(&engine);
}

#[test]
fn vm_a_pending_signal_is_not_delivered_inside_the_final_unwind_request() {
    // V8 §10 row 10 & #14608: a `SerialUnwind` with empty `unbind` runs NO
    // Lisp and is NOT a pending-signal delivery point.
    // Exact GNU function ordering (`unwind_to_catch` vs `handle_interrupt` vs
    // `Fsignal`, emacs-31.1 a360712c9d272d950d8d8255ef74570f7e90b7d9):
    // - `unwind_to_catch` (`src/eval.c:1423-1462`): restores C counters
    //   (`:1435-1443`), calling `unblock_input_to` (`:1436` ->
    //   `src/keyboard.c:8379-8386`). When `level == 0 && pending_signals`,
    //   `unblock_input_to` calls `process_pending_signals()` (`:8367-8372`) ->
    //   `handle_async_input()` -> `handle_interrupt` (`src/keyboard.c:12615-12749`),
    //   which ONLY sets `Vquit_flag = Qt` (`:12719`) and NEVER calls `Fsignal` or
    //   `quit()` (the only throw in `handle_interrupt` is `quit_throw_to_read_char`
    //   at `:12747-12748` when `waiting_for_input && !echoing`, false during unwind).
    // - `unbind_to` (`src/eval.c:3940-3964`, called at `unwind_to_catch:1449`):
    //   saves & clears `Vquit_flag` at `:3942-3944` and restores it at `:3960-3961`
    //   without calling `maybe_quit()`.
    // - `thread-signal` (`post_acquire_global_lock`, `src/thread.c:146-154`):
    //   calls `Fsignal(sym, data)` only on global-lock acquisition, never inside
    //   a Lisp-free `unwind_to_catch`.
    // - Pending `Vquit_flag` / signals are delivered ONLY at explicit
    //   `maybe_quit()` (`src/lisp.h:3897-3901`) -> `probably_quit()`
    //   (`src/eval.c:1887-1895`) -> `process_quit_flag()` (`:1875-1884`) ->
    //   `quit()` / `Fsignal` (`src/eval.c:1897-1935`), which `unwind_to_catch`
    //   never calls before `sys_longjmp(catch->jmp, 1)` (`src/eval.c:1461`).
    // - Therefore a restart or signal delivery is structurally impossible inside
    //   the Lisp-free final `SerialUnwind { pop_handlers, unbind: [] }` (only
    //   `Bpophandler`-style list unlinking `src/bytecode.c:1010`). Instead,
    //   landing sets `quitcounter = (unsigned char)-1` (`src/bytecode.c:982, 1003`)
    //   so any pending signal is observed AFTER landing at the next poll/escape.
    let mut ctx = context();
    let h1 = handler(101);
    let ops = [
        Op::PushConditionCase(4), // 0: dest = 4
        Op::Nil,                  // 1
        Op::SymbolValue,          // 2: Caught { h1, 10 }
        Op::Return,               // 3: skipped
        // dest = 4: landing performs NO poll; handler body's next escape (SymbolValue)
        // is where a deferred pending signal is observed.
        Op::SymbolValue, // 4: pending quit delivered here AFTER landing!
        Op::Return,      // 5
    ];
    // Part A: normal deferred delivery — final request completes cleanly, landing
    // takes NO poll, and the pending quit arrives only at pc=4 after landing.
    let ok_script = vec![
        Some(v8_push_handler(&Op::PushConditionCase(4), h1)),
        Some(v8_caught(h1, Value::fixnum(10))),
        Some(v8_unwind_done(&[h1], &[])),
        Some(v8_escape_flow(signal("quit", vec![]))),
    ];
    let mut ok_engine = VmExec::new(
        &(),
        RecordingEscape::with_steps(ok_script),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    let ok_out = ok_engine.run(&mut ctx, program(&ops));
    let VmOutcome::Signaled(flow) = ok_out else {
        panic!("expected deferred quit after landing, got {ok_out:?}");
    };
    assert_eq!(signal_key(&flow).0, "quit");
    assert_eq!(
        ok_engine.escape_handler().requests[3].pc,
        Pc::new(4),
        "pending quit was observed AFTER landing at dest=4"
    );
    assert_clear(&ok_engine);

    // Part B: structural contract — if a buggy serial side illegally delivers
    // `UnwindCompletion::Flow` inside the Lisp-free final `SerialUnwind { unbind: [] }`
    // request, the VM refuses it as a contract fault (`DispatchContract(PopHandler)`)
    // and poisons.
    let bad_script = vec![
        Some(v8_push_handler(&Op::PushConditionCase(4), h1)),
        Some(v8_caught(h1, Value::fixnum(10))),
        Some(v8_unwind_flow(signal("quit", vec![]))),
    ];
    let mut bad_engine = VmExec::new(
        &(),
        RecordingEscape::with_steps(bad_script),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    let fault = fault_of(bad_engine.run(&mut ctx, program(&ops)));
    assert!(
        matches!(fault, EngineFault::DispatchContract(Op::PopHandler)),
        "Flow inside Lisp-free final SerialUnwind(unbind=[]) must fault as DispatchContract(PopHandler), got {fault:?}"
    );
    assert_eq!(
        bad_engine.escape_handler().requests.len(),
        3,
        "fault must happen at the final Lisp-free SerialUnwind request, not earlier"
    );
    assert!(
        matches!(
            bad_engine.run(&mut ctx, program(&ops)),
            VmOutcome::Fault(EngineFault::Poisoned)
        ),
        "illegal Flow inside final unwind request must poison"
    );
}

#[test]
fn vm_a_restart_only_follows_a_cleanup_that_ran_lisp() {
    // V8 §10 row 11: a restart (`UnwindCompletion::Caught`) is legal ONLY from a
    // `SerialUnwind` with non-empty `unbind` (a cleanup that ran Lisp). Returning
    // `Caught` from a Lisp-free `SerialUnwind { unbind: [] }` is a contract fault.
    // GNU source contract (emacs-31.1 a360712c9d272d950d8d8255ef74570f7e90b7d9):
    // - src/bytecode.c:1010 (`Bpophandler` runs no Lisp);
    // - src/eval.c:1445-1461 (only `unbind_to` cleanups execute Lisp and can signal).
    let mut ctx = context();
    let h1 = handler(111);
    let h2 = handler(112);
    let ops = [
        Op::PushConditionCase(5), // 0: h1
        Op::PushConditionCase(4), // 1: h2
        Op::Nil,                  // 2
        Op::SymbolValue,          // 3: Caught { h1, 1 }
        Op::Return,               // 4: h2 dest
        Op::Return,               // 5: h1 dest
    ];
    // No serial records exist above h1 or h2, so the unwind issues ONE Lisp-free
    // final `SerialUnwind { pop_handlers: [h2, h1], unbind: [] }`. Returning
    // `Caught` (restart) from it violates the structural rule -> DispatchContract!
    let bad_script = vec![
        Some(v8_push_handler(&Op::PushConditionCase(5), h1)),
        Some(v8_push_handler(&Op::PushConditionCase(4), h2)),
        Some(v8_caught(h1, Value::fixnum(1))),
        Some(v8_unwind_restart(&[h2, h1], &[], h2, Value::fixnum(2))),
    ];
    let mut engine = VmExec::new(
        &(),
        RecordingEscape::with_steps(bad_script),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    let fault = fault_of(engine.run(&mut ctx, program(&ops)));
    assert!(
        matches!(fault, EngineFault::DispatchContract(Op::PopHandler)),
        "Caught/restart from Lisp-free SerialUnwind(unbind=[]) must fault as DispatchContract(PopHandler), got {fault:?}"
    );
    assert_eq!(
        engine.escape_handler().requests.len(),
        4,
        "fault must happen at the final Lisp-free SerialUnwind request, not earlier"
    );
    assert!(
        matches!(
            engine.run(&mut ctx, program(&ops)),
            VmOutcome::Fault(EngineFault::Poisoned)
        ),
        "illegal restart from Lisp-free unwind must poison"
    );
}

#[test]
fn vm_an_unwind_over_a_record_whose_symbol_gained_a_watcher_takes_the_serial_unbind() {
    // V8 §10 row 12 (lead #11987, N-v2-unbind-revalidate in L3 unwind loop):
    // GNU source contract (emacs-31.1 a360712c9d272d950d8d8255ef74570f7e90b7d9):
    // - src/eval.c:3848-3870 (`do_one_unbind` for `SPECPDL_LET`: re-reads
    //   `trapped_write`; watched symbol routes through `set_internal(..., SET_INTERNAL_UNBIND)`);
    // - src/eval.c:3948-3957 (`specpdl_ptr` decremented BEFORE `do_one_unbind`).
    // Mirrors Elisp:
    //   (let ((x 10))
    //     (condition-case e
    //         (let ((x 20)) (add-variable-watcher 'x fn) (signal 'error '(99)))
    //       (error (list x e))))  => (10 99), with watcher notified on unlet.
    let mut ctx = context();
    let x = plain_sym("pvm-v8-watched-unwind-x");
    let h1 = handler(121);
    let log = EffectLog::default();
    let constants = [
        Value::from_sym_id(x), // 0
        Value::fixnum(10),     // 1: outer x
        Value::fixnum(20),     // 2: inner x (gained watcher!)
    ];
    let ops = [
        Op::Constant(1),          // 0: 10
        Op::VarBind(0),           // 1: outer x = 10 (stays bound across catch!)
        Op::PushConditionCase(8), // 2: h1 at bind_mark = 1, dest = 8
        Op::Constant(2),          // 3: 20
        Op::VarBind(0),           // 4: inner x = 20 (above h1.bind_mark)
        Op::Nil,                  // 5
        Op::SymbolValue,          // 6: Caught { h1, 99 }
        Op::Return,               // 7: skipped
        // dest = 8: x was restored to 10 by serial PlainUnbind; compute (+ 99 x) = 109!
        Op::VarRef(0), // 8: reads outer x == 10
        Op::Add,       // 9: 99 + 10 = 109
        Op::Unbind(1), // 10: outer x = 10 also watched -> serial PlainUnbind
        Op::Return,    // 11
    ];
    let script = vec![
        Some(v8_push_handler(&Op::PushConditionCase(8), h1)),
        Some(v8_caught(h1, Value::fixnum(99))),
        // Watched inner x=20 handed to serial PlainUnbind during L3 unwind!
        Some(v8_unwind_done(&[], &[])),
        // Final pop of h1 + landing at dest = 8!
        Some(v8_unwind_done(&[h1], &[])),
        // Outer Unbind(1) also hands watched outer x=10 to serial PlainUnbind!
        Some(v8_unwind_done(&[], &[])),
    ];
    let mut engine = VmExec::new(
        &(),
        RecordingEscape::logging_with_steps(script, log.clone()),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new().admit(x).watched(x).logging(log.clone()),
        RecordingPoll::new(),
    );
    let outcome = engine.run(&mut ctx, program_with(&ops, &constants));
    assert_eq!(done_value(&mut engine, outcome), Value::fixnum(109));
    assert_eq!(
        log.entries(),
        vec![
            "slot <- 10",
            "escape PushConditionCase(8)",
            "slot <- 20",
            "escape SymbolValue",
            // L3 unwind hands watched inner x=20 to serial PlainUnbind -> restores 10
            "escape SymbolValue [plain]",
            "slot <- 10",
            // Final pop of h1 + land at dest=8
            "escape SymbolValue [h:121 r:]",
            // Normal Unbind(1) of outer x=10 -> restores none
            "escape Unbind(1) [plain]",
            "slot <- none",
        ],
        "watched Plain binding inside L3 unwind must route through serial PlainUnbind"
    );
    let esc = engine.escape_handler();
    assert!(
        matches!(
            esc.requests[2].why,
            EscapeWhy::PlainUnbind { symbol, .. } if symbol == x
        ),
        "L3 unwind must hand off watched x via EscapeWhy::PlainUnbind"
    );
    assert!(engine.plain_handler().live_records().is_empty());
    assert_clear(&engine);
}

#[test]
fn vm_unbind_cleanup_that_signals_pops_prefix_and_lands_without_double_unbind() {
    // V8 r2 HIGH-1 regression test:
    // GNU source contract (emacs-31.1 a360712c9d272d950d8d8255ef74570f7e90b7d9):
    // - src/eval.c:3955-3957 (`unbind_to`: `this_binding = *--specpdl_ptr`
    //   pops the record BEFORE `do_one_unbind` runs its cleanup Lisp);
    // - src/bytecode.c:843-852 (`Bunbind n` -> `unbind_to`).
    // Part A mirrors Elisp:
    //   (condition-case e (unwind-protect 1 (signal 'error '(boom))) (error e)) => 77.
    let mut ctx = context();
    let h1 = handler(131);
    let u1 = token(1301);
    let ops = [
        Op::PushConditionCase(5), // 0: h1, dest = 5
        Op::SaveExcursion,        // 1: u1 (models unwind-protect cleanup)
        Op::Unbind(1),            // 2: runs u1; u1 signals -> Caught { h1, 77, popped: [u1] }
        Op::PopHandler,           // 3: skipped
        Op::Return,               // 4: skipped
        Op::Return,               // 5: dest -> returns caught 77
    ];
    let script = vec![
        Some(v8_push_handler(&Op::PushConditionCase(5), h1)),
        Some(ScriptStep::Escape(completion_pushing(
            &Op::SaveExcursion,
            u1,
        ))),
        // Op::Unbind(1) pops u1 before u1's cleanup signals into h1!
        Some(v8_unwind_restart(&[], &[u1], h1, Value::fixnum(77))),
        // unwind_to_handler sees u1 is ALREADY gone (never sent a second time!),
        // so it issues ONLY the final pop of h1 + landing!
        Some(v8_unwind_done(&[h1], &[])),
    ];
    let mut engine = VmExec::new(
        &(),
        RecordingEscape::with_steps(script),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    let outcome = engine.run(&mut ctx, program(&ops));
    assert_eq!(done_value(&mut engine, outcome), Value::fixnum(77));
    let esc = engine.escape_handler();
    assert_eq!(
        esc.requests.len(),
        4,
        "PushConditionCase, SaveExcursion(u1), Unbind(1)->Caught(popped=[u1]), final SerialUnwind(pop h1); u1 must NEVER be sent a second time"
    );
    assert_eq!(
        esc.requests[3].why,
        EscapeWhy::SerialUnwind(SerialUnwind {
            pop_handlers: vec![h1],
            unbind: vec![],
        })
    );
    assert_clear(&engine);

    // Part B: multi-record `Op::Unbind(2)` over [u_outer, u_inner] where
    // u_inner succeeds and u_outer signals -> `popped_records = [u_inner, u_outer]`,
    // neither unbound twice; remaining record `u_below` above `h1` unwinds next.
    let u_below = token(1302);
    let u_outer = token(1303);
    let u_inner = token(1304);
    let ops_b = [
        Op::PushConditionCase(7), // 0: h1, dest = 7
        Op::SaveExcursion,        // 1: u_below
        Op::SaveExcursion,        // 2: u_outer
        Op::SaveExcursion,        // 3: u_inner
        Op::Unbind(2),            // 4: [u_inner, u_outer] -> both popped before u_outer signals!
        Op::PopHandler,           // 5: skipped
        Op::Return,               // 6: skipped
        Op::Return,               // 7: dest -> 88
    ];
    let script_b = vec![
        Some(v8_push_handler(&Op::PushConditionCase(7), h1)),
        Some(ScriptStep::Escape(completion_pushing(
            &Op::SaveExcursion,
            u_below,
        ))),
        Some(ScriptStep::Escape(completion_pushing(
            &Op::SaveExcursion,
            u_outer,
        ))),
        Some(ScriptStep::Escape(completion_pushing(
            &Op::SaveExcursion,
            u_inner,
        ))),
        // Unbind(2) pops [u_inner, u_outer]; u_outer signals into h1:
        Some(v8_unwind_restart(
            &[],
            &[u_inner, u_outer],
            h1,
            Value::fixnum(88),
        )),
        // unwind_to_handler unwinds ONLY remaining u_below, then pops h1:
        Some(v8_unwind_done(&[], &[u_below])),
        Some(v8_unwind_done(&[h1], &[])),
    ];
    let mut engine_b = VmExec::new(
        &(),
        RecordingEscape::with_steps(script_b),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    let out_b = engine_b.run(&mut ctx, program(&ops_b));
    assert_eq!(done_value(&mut engine_b, out_b), Value::fixnum(88));
    assert_eq!(
        engine_b.escape_handler().requests[5].why,
        EscapeWhy::SerialUnwind(SerialUnwind {
            pop_handlers: vec![],
            unbind: vec![u_below],
        }),
        "only u_below remains after Unbind(2) popped [u_inner, u_outer]"
    );
    assert_clear(&engine_b);

    // Part C: `Return(NeedsSerialCleanup)` signals during callee frame cleanup
    // into caller's owner handler (`EscapeCompletion::CleanupCaught`), popping
    // callee's serial record prefix and landing in caller without double-unbind.
    let u_callee = token(1305);
    let callee = make_v7_bytecode(
        0,
        0,
        vec![Op::SaveExcursion, Op::Nil, Op::Return],
        vec![],
        6,
    );
    let ops_c = [
        Op::PushConditionCase(5), // 0: h1, dest = 5
        Op::Constant(0),          // 1: callee
        Op::Call(0),              // 2: callee returns via NeedsSerialCleanup -> CleanupCaught
        Op::PopHandler,           // 3: skipped
        Op::Return,               // 4: skipped
        Op::Return,               // 5: dest -> 99
    ];
    let script_c = vec![
        Some(v8_push_handler(&Op::PushConditionCase(5), h1)),
        Some(ScriptStep::Escape(completion_pushing(
            &Op::SaveExcursion,
            u_callee,
        ))),
        Some(ScriptStep::Escape(EscapeCompletion::CleanupCaught {
            handler: h1,
            value: Value::fixnum(99),
            popped_records: vec![u_callee],
            popped_handlers: vec![],
        })),
        Some(v8_unwind_done(&[h1], &[])),
    ];
    let mut engine_c = VmExec::new(
        &(),
        RecordingEscape::with_steps(script_c),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    let out_c = engine_c.run(&mut ctx, program_with(&ops_c, &[callee]));
    assert_eq!(done_value(&mut engine_c, out_c), Value::fixnum(99));
    assert_eq!(
        engine_c.escape_handler().requests.len(),
        4,
        "u_callee popped by Return CleanupCaught and never re-sent by unwind_to_handler"
    );
    assert_clear(&engine_c);
}

#[test]
fn vm_serial_unwind_stops_at_callee_bind_base_so_outer_cleanup_sees_popped_backtrace() {
    // V8 r2 HIGH-2 regression test (V8 L4, GNU `src/eval.c:3946-3958`):
    // Caller F1 installs h1 and pushes `u_outer`; V7 callee F2 pushes `u_inner`
    // and signals targeting h1. `unwind_to_handler` MUST NOT batch `[u_inner, u_outer]`
    // across F2's `bind_base`: `u_inner` must unwind first with backtrace depth 2
    // (`[F2, F1]`), then F2 pops immediately at `F2.bind_base`, and `u_outer`
    // unwinds in the next `SerialUnwind` with backtrace depth 1 (`[F1]`).
    // Mirrors Elisp:
    //   (condition-case e
    //       (unwind-protect
    //           (callee-with-unwind-protect-that-signals)
    //         (record-backtrace-in-outer-cleanup))
    //     (error e))
    let mut ctx = context();
    let h1 = handler(141);
    let u_outer = token(1401);
    let u_inner = token(1402);

    let callee = make_v7_bytecode(
        0,
        0,
        vec![
            Op::SaveExcursion, // u_inner inside F2 (above F2.bind_base = 1)
            Op::Nil,
            Op::SymbolValue, // signals -> Caught { h1, 222 }
            Op::Unbind(1),
            Op::Return,
        ],
        vec![],
        6,
    );
    let ops = [
        Op::PushConditionCase(7), // 0: h1 at bind_mark = 0, dest = 7
        Op::SaveExcursion,        // 1: u_outer in F1 (below F2.bind_base = 1)
        Op::Constant(0),          // 2: callee
        Op::Call(0),              // 3: enters F2 at depth 2 with bind_base = 1
        Op::Unbind(1),            // 4: skipped
        Op::PopHandler,           // 5: skipped
        Op::Return,               // 6: skipped
        Op::Return,               // 7: dest -> 222
    ];
    let script = vec![
        Some(v8_push_handler(&Op::PushConditionCase(7), h1)),
        Some(ScriptStep::Escape(completion_pushing(
            &Op::SaveExcursion,
            u_outer,
        ))),
        Some(ScriptStep::Escape(completion_pushing(
            &Op::SaveExcursion,
            u_inner,
        ))),
        Some(v8_caught(h1, Value::fixnum(222))),
        // Unwind step 1: ONLY u_inner (capped at F2.bind_base = 1), F2 still live!
        Some(v8_unwind_done(&[], &[u_inner])),
        // Unwind step 2: F2 popped at F2.bind_base; now u_outer runs with ONLY F1 live!
        Some(v8_unwind_done(&[], &[u_outer])),
        // Unwind step 3: final pop of h1 + land in F1!
        Some(v8_unwind_done(&[h1], &[])),
    ];
    let mut engine = VmExec::new(
        &(),
        RecordingEscape::with_steps(script),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    let outcome = engine.run(&mut ctx, program_with(&ops, &[callee]));
    assert_eq!(done_value(&mut engine, outcome), Value::fixnum(222));
    let esc = engine.escape_handler();
    assert_eq!(esc.requests.len(), 7);
    assert_eq!(
        esc.requests[4].why,
        EscapeWhy::SerialUnwind(SerialUnwind {
            pop_handlers: vec![],
            unbind: vec![u_inner],
        }),
        "u_inner must unwind in its own SerialUnwind capped at callee bind_base"
    );
    assert_eq!(
        esc.backtrace_depths[4], 2,
        "OwnerBacktraceView must show depth 2 (F2 + F1) while u_inner runs"
    );
    assert_eq!(
        esc.requests[5].why,
        EscapeWhy::SerialUnwind(SerialUnwind {
            pop_handlers: vec![],
            unbind: vec![u_outer],
        }),
        "u_outer must run in a separate SerialUnwind after F2 popped"
    );
    assert_eq!(
        esc.backtrace_depths[5], 1,
        "OwnerBacktraceView must show depth 1 (F1 only — F2 already popped!) while u_outer runs"
    );
    assert_eq!(esc.backtrace_depths[6], 1);
    assert_clear(&engine);
}

#[test]
fn vm_v8_contract_holes_rejected_l1_cardinality_empty_prefix_and_slab_leak() {
    // V8 r2 MEDIUM-1 regression test:
    // 1. L1 push cardinality: handler-push returning `handlers.pushed = None` faults.
    // 2. L1 empty records: handler-push returning non-empty `records.pushed` faults.
    // 3. Zero-progress restart: `UnwindCompletion::Caught` with empty `popped_records`
    //    faults instead of infinite-looping (`src/eval.c:3955`).
    // 4. PlainUnbind Caught slab leak: `UnwindCompletion::Caught` leaving slab
    //    record live faults (`EscapeRecordContract(Bindings)`).
    // 5. `CleanupCaught` on an ordinary opcode (not `Return(NeedsSerialCleanup)`) faults.
    let mut ctx = context();

    // 1. PushConditionCase returning handlers.pushed == None -> DispatchContract!
    let mut e1 = VmExec::new(
        &(),
        RecordingEscape::with_steps(vec![Some(ScriptStep::Escape(
            EscapeCompletion::fall_through(&Op::PushConditionCase(2)),
        ))]),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    assert!(matches!(
        fault_of(e1.run(
            &mut ctx,
            program(&[Op::PushConditionCase(2), Op::Nil, Op::Return])
        )),
        EngineFault::DispatchContract(Op::PushConditionCase(2))
    ));
    assert!(matches!(
        e1.run(&mut ctx, program(&[Op::Nil, Op::Return])),
        VmOutcome::Fault(EngineFault::Poisoned)
    ));

    // 2. PushConditionCase pushing a specpdl record alongside handler -> EscapeRecordContract!
    let mut e2 = VmExec::new(
        &(),
        RecordingEscape::with_steps(vec![Some(ScriptStep::Escape(
            EscapeCompletion::Completed {
                consumed: 0,
                produced: Vec::new(),
                jump: None,
                records: RecordDelta {
                    popped: Vec::new(),
                    pushed: vec![token(1)],
                },
                handlers: HandlerDelta {
                    popped: Vec::new(),
                    pushed: Some(handler(151)),
                },
            },
        ))]),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    assert!(matches!(
        fault_of(e2.run(
            &mut ctx,
            program(&[Op::PushConditionCase(2), Op::Nil, Op::Return])
        )),
        EngineFault::EscapeRecordContract(RecordStack::Bindings)
    ));

    // 3. SerialUnwind Caught with empty `popped_records` -> EscapeRecordContract (no loop!)
    let h1 = handler(152);
    let u1 = token(1501);
    let mut e3 = VmExec::new(
        &(),
        RecordingEscape::with_steps(vec![
            Some(v8_push_handler(&Op::PushConditionCase(5), h1)),
            Some(ScriptStep::Escape(completion_pushing(
                &Op::SaveExcursion,
                u1,
            ))),
            Some(v8_caught(h1, Value::fixnum(1))),
            Some(v8_unwind_restart(&[], &[], h1, Value::fixnum(2))),
        ]),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    assert!(matches!(
        fault_of(e3.run(
            &mut ctx,
            program(&[
                Op::PushConditionCase(5),
                Op::SaveExcursion,
                Op::Nil,
                Op::SymbolValue,
                Op::Return,
                Op::Return,
            ])
        )),
        EngineFault::EscapeRecordContract(RecordStack::Bindings)
    ));
    assert_eq!(
        e3.escape_handler().requests.len(),
        4,
        "empty-prefix Caught must fault immediately on request #4 without looping to a retry"
    );

    // 4. PlainUnbind Caught leaving slab record live -> EscapeRecordContract (no slab leak!)
    let x = plain_sym("pvm-v8-slab-leak-x");
    let mut e4 = VmExec::new(
        &(),
        RecordingEscape::with_steps(vec![
            Some(v8_push_handler(&Op::PushConditionCase(7), h1)),
            Some(v8_caught(h1, Value::fixnum(1))),
            Some(v8_unwind_restart(&[], &[], h1, Value::fixnum(2))),
        ])
        .leaving_plain_binding(),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new().admit(x).watched(x),
        RecordingPoll::new(),
    );
    assert!(matches!(
        fault_of(e4.run(
            &mut ctx,
            program_with(
                &[
                    Op::PushConditionCase(7),
                    Op::Constant(1),
                    Op::VarBind(0),
                    Op::Nil,
                    Op::SymbolValue,
                    Op::Return,
                    Op::Return,
                    Op::Return,
                ],
                &[Value::from_sym_id(x), Value::fixnum(10)],
            )
        )),
        EngineFault::EscapeRecordContract(RecordStack::Bindings)
    ));
    assert_eq!(
        e4.escape_handler().requests.len(),
        3,
        "PlainUnbind Caught leaving slab record live must fault immediately on request #3"
    );

    // 5. CleanupCaught on ordinary SymbolValue -> DispatchContract!
    let mut e5 = VmExec::new(
        &(),
        RecordingEscape::with_steps(vec![
            Some(v8_push_handler(&Op::PushConditionCase(4), h1)),
            Some(ScriptStep::Escape(EscapeCompletion::CleanupCaught {
                handler: h1,
                value: Value::fixnum(1),
                popped_records: vec![],
                popped_handlers: vec![],
            })),
        ]),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    assert!(matches!(
        fault_of(e5.run(
            &mut ctx,
            program(&[
                Op::PushConditionCase(4),
                Op::Nil,
                Op::SymbolValue,
                Op::Return,
                Op::Return,
            ])
        )),
        EngineFault::DispatchContract(Op::SymbolValue)
    ));
}

#[test]
fn vm_caught_heap_value_stays_rooted_in_handler_group_across_unwind_polls_and_restart() {
    // V8 r2 MEDIUM-2.2 regression test:
    // Proves a heap-allocated caught `Value` is published in `RootGroup::Handlers`
    // across every unwind poll/escape, cleanly replaced with the new heap `Value`
    // on an L5 restart, delivered to the landing pad, and revoked to 0 roots.
    let mut ctx = context();
    let h1 = handler(161);
    let h2 = handler(162);
    let u_raises = token(1601);
    let v1 = Value::cons(Value::string("err-outer"), Value::fixnum(1));
    let v2 = Value::cons(Value::string("err-restart"), Value::fixnum(2));
    let ops = [
        Op::PushConditionCase(7), // 0: h1
        Op::PushConditionCase(6), // 1: h2, dest = 6
        Op::SaveExcursion,        // 2: u_raises
        Op::Nil,                  // 3
        Op::SymbolValue,          // 4: Caught { h1, v1 }
        Op::Return,               // 5: skipped
        // dest = 6 (h2): lands with v2, pops h1, returns v2
        Op::PopHandler, // 6
        Op::Return,     // 7
    ];
    let script = vec![
        Some(v8_push_handler(&Op::PushConditionCase(7), h1)),
        Some(v8_push_handler(&Op::PushConditionCase(6), h2)),
        Some(ScriptStep::Escape(completion_pushing(
            &Op::SaveExcursion,
            u_raises,
        ))),
        Some(v8_caught(h1, v1)),
        // u_raises runs while v1 is rooted in RootGroup::Handlers, then restarts to h2 with v2!
        Some(v8_unwind_restart(&[], &[u_raises], h2, v2)),
        // Final pop of h2 runs while v2 (NOT v1!) is rooted in RootGroup::Handlers!
        Some(v8_unwind_done(&[h2], &[])),
        // Normal PopHandler of h1 after landing: 0 handler roots live!
        Some(ScriptStep::Escape(EscapeCompletion::Completed {
            consumed: 0,
            produced: Vec::new(),
            jump: None,
            records: RecordDelta::default(),
            handlers: HandlerDelta {
                popped: vec![h1],
                pushed: None,
            },
        })),
    ];
    let mut engine = VmExec::new(
        &(),
        RecordingEscape::with_steps(script),
        HarnessVmSealed,
        HarnessArith::new(),
        HarnessPlain::new(),
        RecordingPoll::new(),
    );
    let outcome = engine.run(&mut ctx, program(&ops));
    let landed = done_value(&mut engine, outcome);
    assert_eq!(landed, v2);
    assert_eq!(landed.cons_cdr(), Value::fixnum(2));
    let esc = engine.escape_handler();
    assert_eq!(
        esc.handler_roots[4],
        vec![v1],
        "initial heap caught value v1 must be published in RootGroup::Handlers during unwind escape"
    );
    assert_eq!(
        esc.handler_roots[5],
        vec![v2],
        "L5 restart must swap RootGroup::Handlers root from v1 to v2"
    );
    assert!(
        esc.handler_roots[6].is_empty(),
        "landing must pop RootGroup::Handlers root once value moves onto operand stack"
    );
    assert_clear(&engine);
}
