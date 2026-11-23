//! Stage-1 VM stepper rows (VM-EXECUTOR-DESIGN r1 §10). Every row name
//! carries the `vm_` prefix. The sealed-ops double reads conses this harness
//! allocated and nothing shared.

use super::super::owner_bindings::{BindingKind, Channel, SlotValue, UndoTarget};
use super::dispatch::{admits_owner_local, opcode_slot};
use super::engine::{CoreOutcome, EngineFault, RecordStack, apply_one};
use super::frames::{
    BindStack, FrameArena, FrameId, HandlerKind, HandlerMark, HandlerRecord, HandlerStack,
    InterpFrame, OperandCursor, OwnerHandler, OwnerRecord, OwnerRecordMark, Pc, PollBudget,
    RootGroup, SerialHandlerToken, SerialRecordToken, SlotGeneration, SlotRecord,
};
use super::vm::{
    ArithOps, ArithRefusal, EscapeCompletion, EscapeStack, EscapeWhy, HandlerDelta, RecordDelta,
    VmEscape, VmEscapeRequest, VmExec, VmOutcome, VmPoll, VmProgram, VmSealed, static_stack_effect,
};
use crate::emacs_core::builtins::{
    builtin_car_1, builtin_car_safe_1, builtin_cdr_1, builtin_cdr_safe_1, builtin_div, builtin_mul,
    builtin_percent, builtin_setcar_2, builtin_setcdr_2,
};
use crate::emacs_core::bytecode::decode::verify_stack_effects;
use crate::emacs_core::bytecode::{ArithGenericKind, Op, Vm};
use crate::emacs_core::eval::Context;
use crate::emacs_core::eval::world::poll::PollOutcome;
use crate::emacs_core::format_eval_result;
use crate::emacs_core::subr::effect_schema::{
    ArithOperands, ParallelArm, SealedAuthority, WorldSplit,
};
use crate::test_utils::unit_runtime;
use neovm_edit::emacs_core::marker::make_marker_value;
use neovm_value::error::{EvalResult, Flow, LispCondition, expect_args, signal};
use neovm_value::forward::TlsIndex;
use neovm_value::intern::NIL_SYM_ID;
use neovm_value::tagged::gc::RendezvousError;
use neovm_value::value::Value;
use std::num::NonZeroU32;

// ---------------------------------------------------------------- doubles

/// The wired VM poll edges. Order inside one stepper is observable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PollEdge {
    VmBackEdge,
    VmCall,
    SerialRequest,
}

struct RecordingPoll {
    edges: Vec<PollEdge>,
    /// The published top segment at each poll, in order.
    segments: Vec<Vec<Value>>,
    /// A one-shot refusal at one edge.
    refusal: Option<(PollEdge, RendezvousError)>,
}

impl RecordingPoll {
    fn new() -> Self {
        Self {
            edges: Vec::new(),
            segments: Vec::new(),
            refusal: None,
        }
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
        if self.refusal.is_some_and(|(expected, _)| expected == edge) {
            let (_, error) = self.refusal.take().expect("checked just above");
            return Err(error);
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
            OwnerHandler::Serial(token) => Some(token.0),
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

struct RecordingEscape {
    requests: Vec<VmEscapeRequest>,
    /// The escaping frame's segment at each escape, before anything applied.
    segments: Vec<Vec<Value>>,
    /// Both record LIFOs at each escape, before anything applied.
    lifos: Vec<(Lifo, Lifo)>,
    /// One completion per escape, in order; `None` is the opcode's
    /// fall-through.
    script: Vec<Option<EscapeCompletion>>,
    /// An unscripted `Return` reports the operand it published, the frame's
    /// value (`Breturn`: `val = TOP`, bytecode.c:897-921); the default
    /// completion reports nil.
    mirror_return: bool,
}

impl RecordingEscape {
    fn new() -> Self {
        Self {
            requests: Vec::new(),
            segments: Vec::new(),
            lifos: Vec::new(),
            script: Vec::new(),
            mirror_return: false,
        }
    }

    fn with_script(script: Vec<Option<EscapeCompletion>>) -> Self {
        Self {
            script,
            ..Self::new()
        }
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
            script,
            ..Self::mirroring_return()
        }
    }
}

impl VmEscape for RecordingEscape {
    fn escape(&mut self, request: VmEscapeRequest, arena: &FrameArena) -> EscapeCompletion {
        let segment = arena.vm_segment(request.frame).unwrap_or(&[]).to_vec();
        let top = segment.last().copied();
        self.segments.push(segment);
        self.lifos.push(lifos(arena));
        let index = self.requests.len();
        self.requests.push(request.clone());
        self.script
            .get_mut(index)
            .and_then(Option::take)
            .unwrap_or_else(|| {
                if self.mirror_return && request.op == Op::Return {
                    if let Some(top) = top {
                        return completion(&Op::Return, vec![top]);
                    }
                }
                EscapeCompletion::fall_through(&request.op)
            })
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
}

impl HarnessArith {
    fn new() -> Self {
        Self {
            calls: Vec::new(),
            float_slots: Vec::new(),
        }
    }

    /// A float result: one allocation through the owner's float slot.
    fn float(&mut self, value: f64) -> Value {
        self.float_slots.push(value);
        Value::make_float(value)
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
            (ArithGenericKind::Mul, _) => {
                let value = builtin_mul(operands).map_err(ArithRefusal::Flow)?;
                if value.is_float() {
                    self.float_slots.push(value.xfloat());
                }
                Ok(value)
            }
            (ArithGenericKind::Div, _) => {
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
            (ArithGenericKind::Add, [a, b]) => Ok(self.float(number(a)? + number(b)?)),
            (ArithGenericKind::Sub, [a, b]) => Ok(self.float(number(a)? - number(b)?)),
            (ArithGenericKind::Add1, [a]) => Ok(self.float(1.0 + number(a)?)),
            (ArithGenericKind::Sub1, [a]) => Ok(self.float(-1.0 + number(a)?)),
            // GNU `Fminus` on one integer negates in the integers: a bignum
            // at `most-negative-fixnum`, where `make_int` promotes.
            (ArithGenericKind::Negate, [a]) => {
                if let Some(x) = a.as_fixnum() {
                    return x.checked_neg().map(Value::make_int).ok_or_else(unsupported);
                }
                Ok(self.float(-number(a)?))
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

// ---------------------------------------------------------------- helpers

type Harness = VmExec<RecordingEscape, HarnessVmSealed, HarnessArith, RecordingPoll>;

fn context() -> Context {
    Context::new(unit_runtime())
}

fn harness() -> Harness {
    harness_with(RecordingEscape::new())
}

fn harness_with(escape: RecordingEscape) -> Harness {
    VmExec::new(
        escape,
        HarnessVmSealed,
        HarnessArith::new(),
        RecordingPoll::new(),
    )
}

/// A harness whose `Return` hands back the frame's value: the rows that
/// read what an arithmetic opcode left on the stack.
fn arithmetic_harness() -> Harness {
    harness_with(RecordingEscape::mirroring_return())
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

fn done_value(engine: &mut Harness, outcome: VmOutcome) -> Value {
    match outcome {
        VmOutcome::Done { pending } => engine.take(pending).expect("done root"),
        VmOutcome::Signaled(flow) => panic!("signaled {}", signal_key(&flow).0),
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

fn assert_clear<S: VmSealed>(engine: &VmExec<RecordingEscape, S, HarnessArith, RecordingPoll>) {
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
    EscapeCompletion {
        stack: EscapeStack::Completed {
            consumed: static_stack_effect(op).consumed,
            produced,
            jump: None,
        },
        records: RecordDelta::default(),
        handlers: HandlerDelta::default(),
    }
}

/// A completed escape that also jumps: a `Switch` hit, the only opcode
/// that may report one.
fn completion_jumping(op: &Op, produced: Vec<Value>, jump: u32) -> EscapeCompletion {
    EscapeCompletion {
        stack: EscapeStack::Completed {
            consumed: static_stack_effect(op).consumed,
            produced,
            jump: Some(jump),
        },
        records: RecordDelta::default(),
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
        assert_eq!(done_value(&mut engine, outcome), Value::NIL, "{name}");
        let escape = engine.escape_handler();
        assert_eq!(escape.requests.len(), 1, "{name}: escapes");
        assert_eq!(escape.requests[0].op, Op::Return, "{name}");
        assert_eq!(escape.segments[0], expected, "{name}: segment at Return");
        assert_eq!(
            engine.poll_handler().edges,
            vec![PollEdge::SerialRequest],
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
    expected.push(PollEdge::SerialRequest);
    assert_eq!(engine.poll_handler().edges, expected);
    let escape = engine.escape_handler();
    assert_eq!(escape.requests.len(), 1);
    assert_eq!(escape.segments[0], vec![Value::NIL]);
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
fn vm_every_non_arith_world_opcode_is_exactly_one_escape() {
    // The arithmetic opcodes escape only for a marker operand
    // (`vm_a_marker_operand_escapes_through_the_one_stub`).
    let mut escaped = 0;
    let mut ctx = context();
    for (name, op) in op_samples() {
        assert!(every_variant_is_listed(&op), "{name}: listed");
        let ParallelArm::World(split) = opcode_slot(&op).arm else {
            continue;
        };
        if matches!(split, WorldSplit::Arith(_)) {
            continue;
        }
        escaped += 1;
        let effect = static_stack_effect(&op);
        let is_return = op == Op::Return;
        let mut ops = nils(effect.consumed as usize);
        ops.push(op.clone());
        if !is_return {
            // `Return` needs its own operand; an opcode that produces none
            // gets one pushed for it.
            if effect.produced == 0 {
                ops.push(Op::Nil);
            }
            ops.push(Op::Return);
        }
        let mut engine = harness();
        let outcome = engine.run(&mut ctx, program(&ops));
        assert_eq!(done_value(&mut engine, outcome), Value::NIL, "{name}");
        let escape = engine.escape_handler();
        assert_eq!(
            escape.requests.len(),
            if is_return { 1 } else { 2 },
            "{name}"
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
        if is_return {
            assert_eq!(
                engine.poll_handler().edges,
                vec![PollEdge::SerialRequest],
                "{name}: edges"
            );
        } else {
            let mut expected = Vec::new();
            if is_call(&op) {
                expected.push(PollEdge::VmCall);
            }
            expected.extend([PollEdge::SerialRequest, PollEdge::SerialRequest]);
            assert_eq!(engine.poll_handler().edges, expected, "{name}: edges");
            assert_eq!(
                escape.segments[1],
                vec![Value::NIL; effect.produced.max(1) as usize],
                "{name}: segment at Return"
            );
        }
        assert_clear(&engine);
    }
    assert_eq!(escaped, 47, "escaped opcodes");
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
        // no operand is a marker, and escapes every other World arm.
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
        RecordingEscape::with_script(script),
        HarnessVmSealed,
        HarnessArith::new(),
        RecordingPoll::new(),
    );
    let outcome = engine.run(&mut ctx, program_with(&ops, &[cons]));
    assert_eq!(done_value(&mut engine, outcome), Value::NIL);
    let poll = engine.poll_handler();
    assert_eq!(
        poll.edges,
        vec![
            PollEdge::SerialRequest,
            PollEdge::VmBackEdge,
            PollEdge::SerialRequest,
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
    assert_eq!(escape.requests.len(), 3);
    assert_eq!(
        escape.segments,
        vec![
            vec![cons, Value::NIL, Value::NIL],
            vec![cons, cons, Value::NIL, Value::NIL],
            vec![cons, cons]
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
            vec![Op::Nil, Op::PushConditionCase(4), Op::Nil, Op::Return],
            vec![
                Some(EscapeCompletion {
                    records: RecordDelta {
                        popped: Vec::new(),
                        pushed: vec![token(1)],
                    },
                    ..completion(&Op::PushConditionCase(4), Vec::new())
                }),
                Some(EscapeCompletion {
                    records: RecordDelta {
                        popped: vec![token(1)],
                        pushed: Vec::new(),
                    },
                    ..completion_jumping(&Op::Return, vec![Value::T], 0)
                }),
            ],
            Op::Return,
            vec![Some(1)],
        ),
    ];
    for (ops, script, op, bindings) in cases {
        let mut engine = VmExec::new(
            RecordingEscape::with_script(script),
            HarnessVmSealed,
            HarnessArith::new(),
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
            vec![Op::Nil, Op::Return],
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
            RecordingEscape::new(),
            HarnessVmSealed,
            HarnessArith::new(),
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
        Some(EscapeCompletion {
            handlers: HandlerDelta {
                popped: Vec::new(),
                pushed: vec![handler(7)],
            },
            ..completion(&Op::PushConditionCase(4), Vec::new())
        }),
        Some(EscapeCompletion {
            stack: EscapeStack::Flow(signal("error", vec![Value::string("boom")])),
            records: RecordDelta::default(),
            handlers: HandlerDelta::default(),
        }),
    ];
    let mut engine = VmExec::new(
        RecordingEscape::with_script(script),
        HarnessVmSealed,
        HarnessArith::new(),
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
        Op::PushConditionCase(4),
        Op::SymbolValue,
        Op::Return,
    ];
    let script = vec![
        Some(EscapeCompletion {
            records: RecordDelta {
                popped: Vec::new(),
                pushed: vec![token(1)],
            },
            handlers: HandlerDelta {
                popped: Vec::new(),
                pushed: vec![handler(5)],
            },
            ..completion(&Op::PushConditionCase(4), Vec::new())
        }),
        Some(EscapeCompletion {
            stack: EscapeStack::Completed {
                consumed: 1,
                produced: Vec::new(),
                jump: None,
            },
            records: RecordDelta {
                popped: vec![token(1)],
                pushed: vec![token(2)],
            },
            handlers: HandlerDelta {
                popped: vec![handler(5)],
                pushed: vec![handler(6)],
            },
        }),
    ];
    let mut engine = VmExec::new(
        RecordingEscape::with_script(script),
        HarnessVmSealed,
        HarnessArith::new(),
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
        Op::PushConditionCase(4),
        Op::SymbolValue,
        Op::SymbolValue,
        Op::Return,
    ];
    let script = vec![
        // Pushed outermost first: [A, B] with B innermost; one handler.
        Some(EscapeCompletion {
            records: RecordDelta {
                popped: Vec::new(),
                pushed: vec![token(1), token(2)],
            },
            handlers: HandlerDelta {
                popped: Vec::new(),
                pushed: vec![handler(10)],
            },
            ..completion(&Op::PushConditionCase(4), Vec::new())
        }),
        // Popped innermost first: [B, A]; then C.
        Some(EscapeCompletion {
            records: RecordDelta {
                popped: vec![token(2), token(1)],
                pushed: vec![token(3)],
            },
            ..completion(&Op::SymbolValue, vec![Value::NIL])
        }),
        Some(EscapeCompletion {
            records: RecordDelta {
                popped: vec![token(3)],
                pushed: Vec::new(),
            },
            handlers: HandlerDelta {
                popped: vec![handler(10)],
                pushed: Vec::new(),
            },
            ..completion(&Op::SymbolValue, vec![Value::NIL])
        }),
    ];
    let mut engine = VmExec::new(
        RecordingEscape::with_script(script),
        HarnessVmSealed,
        HarnessArith::new(),
        RecordingPoll::new(),
    );
    let outcome = engine.run(&mut context(), program(&ops));
    assert_eq!(done_value(&mut engine, outcome), Value::NIL);
    let escape = engine.escape_handler();
    assert_eq!(
        escape.lifos,
        vec![
            (vec![], vec![]),
            (vec![Some(1), Some(2)], vec![Some(10)]),
            (vec![Some(3)], vec![Some(10)]),
            (vec![], vec![]),
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
                pushed: vec![handler(6)],
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
                pushed: Vec::new(),
            },
            RecordStack::Handlers,
        ),
    ];
    for (name, records, handlers, expected) in cases {
        let ops = vec![
            Op::Nil,
            Op::PushConditionCase(4),
            Op::SymbolValue,
            Op::Return,
        ];
        let script = vec![
            Some(EscapeCompletion {
                records: RecordDelta {
                    popped: Vec::new(),
                    pushed: vec![token(1)],
                },
                handlers: HandlerDelta {
                    popped: Vec::new(),
                    pushed: vec![handler(5)],
                },
                ..completion(&Op::PushConditionCase(4), Vec::new())
            }),
            Some(EscapeCompletion {
                records,
                handlers,
                ..completion(&Op::SymbolValue, vec![Value::NIL])
            }),
        ];
        let mut engine = VmExec::new(
            RecordingEscape::with_script(script),
            HarnessVmSealed,
            HarnessArith::new(),
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
    handlers.push(handler(1));
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

fn slot_record(saved: i64) -> SlotRecord {
    SlotRecord::new(
        NIL_SYM_ID,
        TlsIndex::VOID,
        Channel::Default,
        BindingKind::Let,
        UndoTarget::Previous(SlotValue::new(Value::fixnum(saved))),
        SlotGeneration::new(NonZeroU32::MIN),
    )
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
fn vm_serial_markers_publish_no_roots_and_owner_records_still_do() {
    let mut arena = FrameArena::new(8);
    let frame = arena.push_interp(interp_frame()).expect("interp frame");
    arena.handlers_mut().push(handler_record(frame, 19, 20));
    arena.bindings_mut().push(slot_record(21));
    assert_eq!(
        handler_and_specpdls(&arena),
        vec![
            ("handlers", Some(19)),
            ("handlers", Some(20)),
            ("specpdl", Some(21))
        ]
    );
    // Markers join both LIFOs above the owner records and publish nothing.
    let mark = arena.handlers_mut().mark();
    arena.handlers_mut().push(handler(1));
    arena.bindings_mut().push(token(2));
    assert!(arena.handlers().has_serial_above(mark));
    assert!(arena.bindings().has_serial_above(OwnerRecordMark::EMPTY));
    assert_eq!(handler_and_specpdls(&arena).len(), 3);
    // Truncation to the mark removes the marker and keeps the owner record.
    assert_eq!(arena.handlers_mut().truncate(mark), 1);
    assert!(!arena.handlers().has_serial_above(mark));
    assert_eq!(
        handler_and_specpdls(&arena),
        vec![
            ("handlers", Some(19)),
            ("handlers", Some(20)),
            ("specpdl", Some(21))
        ]
    );
    // Truncation to the base removes owner records and markers alike.
    assert_eq!(arena.handlers_mut().truncate(HandlerMark::EMPTY), 1);
    assert_eq!(arena.bindings_mut().truncate(OwnerRecordMark::EMPTY), 2);
    assert!(handler_and_specpdls(&arena).is_empty());
}

#[test]
fn vm_a_return_escape_ends_the_run_with_the_reported_value() {
    let constants = [Value::fixnum(42)];
    let ops = [Op::Constant(0), Op::Return];
    let script = vec![Some(completion(&Op::Return, vec![Value::fixnum(7)]))];
    let mut engine = VmExec::new(
        RecordingEscape::with_script(script),
        HarnessVmSealed,
        HarnessArith::new(),
        RecordingPoll::new(),
    );
    let outcome = engine.run(&mut context(), program_with(&ops, &constants));
    let pending = match outcome {
        VmOutcome::Done { pending } => pending,
        other => panic!("expected Done, got {other:?}"),
    };
    assert_eq!(engine.escape_handler().segments[0], vec![Value::fixnum(42)]);
    assert_eq!(engine.lookup_root(pending), Some(Value::fixnum(7)));
    assert_eq!(engine.take(pending), Ok(Value::fixnum(7)));
    assert_clear(&engine);
    let outcome = engine.run(&mut context(), program(&[Op::Nil, Op::Return]));
    assert_eq!(done_value(&mut engine, outcome), Value::NIL);
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
    // (name, program without its Return, result at the Return escape)
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
        assert_eq!(done_value(&mut engine, outcome), Value::NIL, "{name}");
        let escape = engine.escape_handler();
        assert_eq!(escape.requests.len(), 1, "{name}: escapes");
        assert_eq!(escape.requests[0].op, Op::Return, "{name}");
        assert_eq!(escape.segments[0], vec![result], "{name}: result");
        assert_eq!(
            engine.poll_handler().edges,
            vec![PollEdge::SerialRequest],
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
        RecordingEscape::new(),
        MissingSealed,
        HarnessArith::new(),
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
    /// The verifier records no effect: `Return`, `Throw`,
    /// `TrapOutOfRangeConstant`.
    Silent,
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
        Op::Return | Op::Throw | Op::TrapOutOfRangeConstant(_) => VerifierVoice::Silent,
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
                // carries `val = TOP` to `TOP = val` (bytecode.c:897-921),
                // `Throw` pops tag and value (the serial `Throw` arm).
                silent += 1;
                for depth in 0..=8 {
                    assert!(
                        verify_stack_effects(&[op.clone(), Op::Return], depth, MAX_STACK),
                        "{name}: silent at {depth}"
                    );
                }
                let documented = match op {
                    Op::Return => (1, 1),
                    Op::Throw => (2, 0),
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
    assert_eq!((tied, silent, refused), (samples.len() - 4, 3, 1));
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
    assert_eq!(escape.requests.len(), 1, "only Return escapes");
    assert_eq!(escape.requests[0].op, Op::Return);
    assert_eq!(
        escape.segments[0],
        vec![Value::fixnum(5)],
        "the sum is published when Return escapes"
    );
    assert_eq!(done, Value::fixnum(5), "the frame returns the sum");
    assert_eq!(
        engine.poll_handler().edges,
        vec![PollEdge::SerialRequest],
        "Return's own request poll; the add took none"
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
        1,
        "only Return escapes"
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
        assert_eq!(escape.requests.len(), 2, "{op:?}: the opcode, then Return");
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
            vec![PollEdge::SerialRequest, PollEdge::SerialRequest],
            "{op:?}: no call poll, no back edge"
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
