//! Stage-1 harness stepper. The sealed-ops impl reads only conses this
//! harness allocated. It does not read a shared object.

use super::super::owner_bindings::{Channel, SlotState, SlotValue};
use super::dispatch::primitive_slot_by_name;
use super::effective::{
    EffectivePlainKey, EffectiveRead, EffectiveWrite, EraId, OwnerSlots, OwnerSlotsMut,
    ShapeGeneration,
};
use super::engine::{
    CoreOutcome, EngineFault, EscapeClass, EscapeRequest, SealedOps, SerialEscape, Stage1,
    StepOutcome, apply_one, apply_slice, apply_vec,
};
use super::frames::{
    FrameArena, HandlerMark, InterpFrame, OwnerRecordMark, PollBudget, RootRevokeError,
};
use crate::emacs_core::builtins::{
    builtin_arrayp, builtin_car_1, builtin_car_safe_1, builtin_cdr_1, builtin_cdr_safe_1,
    builtin_hash_table_p, builtin_integerp_1, builtin_module_function_p, builtin_numberp_1,
    builtin_sequencep, builtin_symbol_with_pos_p_1, builtin_user_ptrp,
    builtin_vector_or_char_table_p, builtin_vectorp_1,
};
use crate::emacs_core::builtins_extra::{
    builtin_byte_code_function_p, builtin_closurep, builtin_subrp,
};
use crate::emacs_core::eval::Context;
use crate::emacs_core::subr::effect_catalog::inventory;
use crate::emacs_core::subr::effect_schema::{
    CatalogKind, ParallelArm, SealedAuthority, WorldSplit,
};
use crate::emacs_core::subr_info::builtin_interpreted_function_p;
use crate::test_utils::unit_runtime;
use neovm_edit::emacs_core::buffer::builtin_bufferp;
use neovm_edit::emacs_core::marker::{builtin_markerp, make_marker_value};
use neovm_text::emacs_core::chartable::builtin_char_table_p;
use neovm_text::encoding::builtin_multibyte_string_p_1;
use neovm_value::boolvec::builtin_bool_vector_p;
use neovm_value::error::Flow;
use neovm_value::forward::TlsIndex;
use neovm_value::intern::NIL_SYM_ID;
use neovm_value::value::{Value, eq_value};
use std::cell::Cell;
use std::num::NonZeroU32;

struct RecordingEscape {
    requests: Vec<EscapeRequest>,
}

impl RecordingEscape {
    fn new() -> Self {
        Self {
            requests: Vec::new(),
        }
    }
}

impl SerialEscape for RecordingEscape {
    fn escape(&mut self, request: EscapeRequest) {
        self.requests.push(request);
    }
}

/// Test double for sealed reads. Cons fields are read only when the caller
/// already owns the cons; this impl never looks a cons up in a shared heap.
struct HarnessSealed;

/// Sealed ops that admit the catalog name and still have no core. The
/// stepper must unwind that outcome instead of leaving the frame pushed.
struct MissingSealed;

impl SealedOps for MissingSealed {
    fn read_cons_field(&mut self, _ctx: &mut Context, _name: &str, _args: &[Value]) -> CoreOutcome {
        CoreOutcome::MissingCore
    }

    fn read_header_word(
        &mut self,
        _ctx: &mut Context,
        _name: &str,
        _args: &[Value],
    ) -> CoreOutcome {
        CoreOutcome::MissingCore
    }
}

impl SealedOps for HarnessSealed {
    fn read_cons_field(&mut self, ctx: &mut Context, name: &str, args: &[Value]) -> CoreOutcome {
        match name {
            "car" => apply_one(ctx, "car", args, builtin_car_1),
            "cdr" => apply_one(ctx, "cdr", args, builtin_cdr_1),
            "car-safe" => apply_one(ctx, "car-safe", args, builtin_car_safe_1),
            "cdr-safe" => apply_one(ctx, "cdr-safe", args, builtin_cdr_safe_1),
            _ => CoreOutcome::MissingCore,
        }
    }

    fn read_header_word(&mut self, ctx: &mut Context, name: &str, args: &[Value]) -> CoreOutcome {
        match name {
            "arrayp" => apply_slice(args, builtin_arrayp),
            "bool-vector-p" => apply_vec(args, builtin_bool_vector_p),
            "bufferp" => apply_vec(args, builtin_bufferp),
            "byte-code-function-p" => apply_vec(args, builtin_byte_code_function_p),
            "char-table-p" => apply_vec(args, builtin_char_table_p),
            "closurep" => apply_vec(args, builtin_closurep),
            "hash-table-p" => apply_vec(args, builtin_hash_table_p),
            "integerp" => apply_one(ctx, "integerp", args, builtin_integerp_1),
            "interpreted-function-p" => apply_vec(args, builtin_interpreted_function_p),
            "markerp" => apply_vec(args, builtin_markerp),
            "module-function-p" => apply_vec(args, builtin_module_function_p),
            "multibyte-string-p" => apply_one(ctx, "multibyte-string-p", args, multibyte_string_p),
            "numberp" => apply_one(ctx, "numberp", args, builtin_numberp_1),
            "sequencep" => apply_vec(args, builtin_sequencep),
            "subrp" => apply_slice(args, builtin_subrp),
            "symbol-with-pos-p" => {
                apply_one(ctx, "symbol-with-pos-p", args, builtin_symbol_with_pos_p_1)
            }
            "user-ptrp" => apply_vec(args, builtin_user_ptrp),
            "vector-or-char-table-p" => apply_vec(args, builtin_vector_or_char_table_p),
            "vectorp" => apply_one(ctx, "vectorp", args, builtin_vectorp_1),
            _ => CoreOutcome::MissingCore,
        }
    }
}

struct TestSlots {
    state: SlotState,
}

impl TestSlots {
    fn empty() -> Self {
        Self {
            state: SlotState::NoBinding,
        }
    }

    fn bound(value: Value) -> Self {
        Self {
            state: SlotState::Bound(SlotValue::new(value)),
        }
    }
}

impl OwnerSlots for TestSlots {
    fn slot_state(&self, _slot: TlsIndex, _channel: Channel) -> SlotState {
        self.state
    }
}

impl OwnerSlotsMut for TestSlots {
    fn set_slot_state(&mut self, _slot: TlsIndex, _channel: Channel, state: SlotState) {
        self.state = state;
    }
}

fn multibyte_string_p(ctx: &mut Context, arg: Value) -> neovm_value::error::EvalResult {
    builtin_multibyte_string_p_1(ctx, arg)
}

fn context() -> Context {
    Context::new(unit_runtime())
}

fn engine(slots: TestSlots) -> Stage1<TestSlots, RecordingEscape, HarnessSealed> {
    Stage1::new(
        slots,
        RecordingEscape::new(),
        HarnessSealed,
        EraId::new(3),
        ShapeGeneration::new(7),
    )
}

fn wide() -> NonZeroU32 {
    NonZeroU32::new(64).expect("poll interval")
}

fn call(name: &'static str, args: &[Value]) -> Value {
    let mut forms = vec![Value::symbol(name)];
    forms.extend_from_slice(args);
    Value::list(forms)
}

fn quote(value: Value) -> Value {
    Value::list(vec![Value::symbol("quote"), value])
}

fn body(forms: &[Value]) -> Value {
    Value::list(forms.to_vec())
}

fn key() -> EffectivePlainKey<'static> {
    EffectivePlainKey::test_key(
        NIL_SYM_ID,
        TlsIndex::VOID,
        Channel::Default,
        EraId::new(3),
        ShapeGeneration::new(7),
    )
}

fn stale_key() -> EffectivePlainKey<'static> {
    EffectivePlainKey::test_key(
        NIL_SYM_ID,
        TlsIndex::VOID,
        Channel::Default,
        EraId::new(9),
        ShapeGeneration::new(7),
    )
}

fn signal_key(flow: &Flow) -> (String, Vec<Value>) {
    let data = flow.as_signal().expect("Lisp signal");
    (data.symbol_name().to_string(), data.data.clone())
}

fn assert_clear<O: SealedOps>(engine: &Stage1<TestSlots, RecordingEscape, O>) {
    assert_eq!(engine.arena_depth(), 0, "arena depth");
    assert_eq!(engine.live_root_count(), 0, "live roots");
}

fn done_value<O: SealedOps>(
    engine: &mut Stage1<TestSlots, RecordingEscape, O>,
    outcome: StepOutcome,
) -> Value {
    match outcome {
        StepOutcome::Done { pending, .. } => engine.take(pending).expect("done root"),
        StepOutcome::Escaped(request) => panic!("escaped {:?}", request.class),
        StepOutcome::Signaled(flow) => panic!("signaled {}", signal_key(&flow).0),
        StepOutcome::MissingCore { name } => panic!("missing core {name}"),
        StepOutcome::Suspended { .. } => panic!("suspended"),
        StepOutcome::Fault(_) => panic!("fault"),
    }
}

fn samples() -> Vec<Value> {
    vec![
        Value::NIL,
        Value::T,
        Value::fixnum(1),
        Value::make_int(-1),
        Value::make_int(0x3F_FFFF),
        Value::make_int(0x40_0000),
        Value::make_int(Value::MOST_POSITIVE_FIXNUM + 1),
        Value::make_float(1.5),
        Value::string("m"),
        Value::multibyte_string("héllo"),
        Value::cons(Value::fixnum(1), Value::fixnum(2)),
        Value::vector(vec![Value::fixnum(9)]),
        Value::symbol("sym"),
        make_marker_value(None, None, false),
    ]
}

/// `(name 'arg ...)`. Quote keeps the serial evaluator from treating a
/// symbol argument as a variable.
fn quoted_call(name: &'static str, args: &[Value]) -> Value {
    let mut cells = Vec::with_capacity(1 + args.len());
    cells.push(Value::symbol(name));
    for arg in args {
        cells.push(quote(*arg));
    }
    Value::list(cells)
}

#[test]
fn consp_null_and_identity_do_not_escape() {
    let mut ctx = context();
    let mut stepper = engine(TestSlots::empty());
    let forms = body(&[
        call("consp", &[Value::NIL]),
        call("null", &[Value::NIL]),
        call("identity", &[Value::fixnum(7)]),
    ]);
    let outcome = stepper.start(&mut ctx, forms, wide());
    assert_eq!(done_value(&mut stepper, outcome), Value::fixnum(7));
    assert!(stepper.escape_handler().requests.is_empty());
}

#[test]
fn car_of_a_harness_owned_cons_does_not_escape() {
    let mut ctx = context();
    let mut stepper = engine(TestSlots::empty());
    let cell = Value::cons(Value::symbol("head"), Value::fixnum(4));
    let outcome = stepper.start(&mut ctx, body(&[call("car", &[quote(cell)])]), wide());
    assert_eq!(done_value(&mut stepper, outcome), Value::symbol("head"));
    assert!(stepper.escape_handler().requests.is_empty());
}

#[test]
fn car_of_a_fixnum_signals_the_serial_wrong_type_argument() {
    let mut ctx = context();
    let mut stepper = engine(TestSlots::empty());
    let outcome = stepper.start(&mut ctx, body(&[call("car", &[Value::fixnum(1)])]), wide());
    let StepOutcome::Signaled(flow) = outcome else {
        panic!("(car 1) must signal");
    };
    let serial = builtin_car_1(&mut ctx, Value::fixnum(1)).expect_err("serial (car 1)");
    let (name, data) = signal_key(&flow);
    assert_eq!(name, "wrong-type-argument");
    assert_eq!(data[0], Value::symbol("listp"));
    assert_eq!(data[1], Value::fixnum(1));
    assert_eq!(signal_key(&flow), signal_key(&serial));
    assert!(stepper.escape_handler().requests.is_empty());
}

#[test]
fn an_unknown_name_escapes_once_and_does_not_run_the_argument() {
    let mut ctx = context();
    let mut stepper = engine(TestSlots::empty());
    let form = call(
        "no-such-stage1-primitive",
        &[call("car", &[Value::fixnum(1)])],
    );
    let outcome = stepper.start(&mut ctx, body(&[form]), wide());
    let StepOutcome::Escaped(request) = outcome else {
        panic!("unknown name must escape");
    };
    assert_eq!(request.class, EscapeClass::UnknownName);
    assert!(eq_value(&request.form, &form));
    assert_eq!(stepper.arena_depth(), 1);
    assert_eq!(stepper.live_root_count(), 0);
    assert_eq!(stepper.escape_handler().requests.len(), 1);
    assert_eq!(
        request.form.cons_cdr().cons_car().cons_car(),
        Value::symbol("car")
    );
}

#[test]
fn a_world_primitive_escapes_once_and_does_not_run_its_body() {
    let mut ctx = context();
    let mut stepper = engine(TestSlots::empty());
    let form = call("+", &[call("car", &[Value::fixnum(1)])]);
    let outcome = stepper.start(&mut ctx, body(&[form]), wide());
    let StepOutcome::Escaped(request) = outcome else {
        panic!("+ is a World arm");
    };
    assert!(eq_value(&request.form, &form));
    assert!(matches!(
        request.class,
        EscapeClass::World(WorldSplit::Arith(_))
    ));
    assert_eq!(stepper.escape_handler().requests.len(), 1);
    assert_eq!(
        request.form.cons_cdr().cons_car().cons_car(),
        Value::symbol("car")
    );

    let name = inventory()
        .iter()
        .find(|entry| {
            entry.kind == CatalogKind::Primitive
                && matches!(
                    primitive_slot_by_name(entry.name).map(|slot| slot.arm),
                    Some(ParallelArm::World(WorldSplit::Escape))
                )
        })
        .expect("one World escape primitive")
        .name;
    let mut stepper = engine(TestSlots::empty());
    let outcome = stepper.start(
        &mut ctx,
        body(&[call(name, &[call("car", &[Value::fixnum(1)])])]),
        wide(),
    );
    let StepOutcome::Escaped(request) = outcome else {
        panic!("{name} must escape");
    };
    assert_eq!(request.class, EscapeClass::World(WorldSplit::Escape));
    assert_eq!(stepper.escape_handler().requests.len(), 1);
}

#[test]
fn an_unimplemented_sealed_authority_escapes_before_arguments() {
    let mut ctx = context();
    let mut stepper = engine(TestSlots::empty());
    let form = call(
        "setcar",
        &[call("car", &[Value::fixnum(1)]), Value::fixnum(2)],
    );
    let outcome = stepper.start(&mut ctx, body(&[form]), wide());
    let StepOutcome::Escaped(request) = outcome else {
        panic!("setcar is ConsFieldWrite");
    };
    assert_eq!(
        request.class,
        EscapeClass::UnimplementedSealed(SealedAuthority::ConsFieldWrite)
    );
    assert!(eq_value(&request.form, &form));
    assert_eq!(stepper.escape_handler().requests.len(), 1);
    assert_eq!(
        request.form.cons_cdr().cons_car().cons_car(),
        Value::symbol("car")
    );

    let mut stepper = engine(TestSlots::empty());
    let form = call("cons", &[Value::fixnum(1), Value::fixnum(2)]);
    let outcome = stepper.start(&mut ctx, body(&[form]), wide());
    let StepOutcome::Escaped(request) = outcome else {
        panic!("cons is ConsAllocation");
    };
    assert_eq!(
        request.class,
        EscapeClass::UnimplementedSealed(SealedAuthority::ConsAllocation)
    );
    assert!(eq_value(&request.form, &form));
}

#[test]
fn poll_budget_one_suspends_a_two_form_body_and_resumes_the_same_frame() {
    let mut ctx = context();
    let mut stepper = engine(TestSlots::empty());
    let outcome = stepper.start(
        &mut ctx,
        body(&[Value::fixnum(1), Value::fixnum(2)]),
        NonZeroU32::new(1).expect("interval"),
    );
    let StepOutcome::Suspended { frame, pending } = outcome else {
        panic!("the first form must suspend");
    };
    assert!(stepper.frame_is_live(frame));
    assert_eq!(stepper.arena_depth(), 1);
    assert_eq!(stepper.live_root_count(), 1);
    assert_eq!(stepper.lookup_root(pending), Some(Value::fixnum(1)));
    let busy = stepper.start(&mut ctx, body(&[Value::fixnum(9)]), wide());
    assert!(matches!(busy, StepOutcome::Fault(EngineFault::Busy)));
    assert_eq!(stepper.arena_depth(), 1);
    assert_eq!(stepper.live_root_count(), 1);
    let resumed = stepper.resume(&mut ctx);
    let StepOutcome::Done {
        frame: done,
        pending: done_root,
    } = resumed
    else {
        panic!("resume must finish the body");
    };
    assert_eq!(frame, done);
    assert_eq!(frame.generation(), done.generation());
    assert_eq!(
        stepper.take(done_root).expect("done root"),
        Value::fixnum(2)
    );
    assert_clear(&stepper);
    assert!(stepper.escape_handler().requests.is_empty());
}

#[test]
fn progn_returns_its_last_form() {
    let mut ctx = context();
    let mut stepper = engine(TestSlots::empty());
    let progn = call("progn", &[Value::fixnum(1), Value::fixnum(2)]);
    let outcome = stepper.start(&mut ctx, body(&[progn]), wide());
    assert_eq!(done_value(&mut stepper, outcome), Value::fixnum(2));
}

#[test]
fn a_stale_frame_id_pop_is_still_refused() {
    let mut arena = FrameArena::new(4);
    let frame = InterpFrame::new(
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        0,
        HandlerMark::EMPTY,
        OwnerRecordMark::EMPTY,
        PollBudget::new(wide()),
    );
    let id = arena.push_interp(frame).expect("push");
    assert!(arena.pop(id).is_some());
    assert!(arena.pop(id).is_none());
    assert!(arena.interp(id).is_none());
}

#[test]
fn effective_read_write_and_bound_use_the_pure_rule() {
    let mut stepper = engine(TestSlots::bound(Value::fixnum(5)));
    let called = Cell::new(false);
    let read = stepper
        .read_plain(&key(), || {
            called.set(true);
            Value::fixnum(99)
        })
        .expect("valid key");
    assert_eq!(read, EffectiveRead::OwnerBound(Value::fixnum(5)));
    assert!(!called.get());

    let mut stepper = engine(TestSlots::empty());
    let called = Cell::new(0);
    let read = stepper
        .read_plain(&key(), || {
            called.set(called.get() + 1);
            Value::fixnum(41)
        })
        .expect("valid key");
    assert_eq!(read, EffectiveRead::Canonical(Value::fixnum(41)));
    assert_eq!(called.get(), 1);

    let mut stepper = engine(TestSlots::bound(Value::UNBOUND));
    let called = Cell::new(false);
    let read = stepper
        .read_plain(&key(), || {
            called.set(true);
            Value::fixnum(1)
        })
        .expect("valid key");
    assert_eq!(read, EffectiveRead::OwnerVoid);
    assert!(!called.get());

    let mut stepper = engine(TestSlots::bound(Value::fixnum(5)));
    let called = Cell::new(false);
    let write = stepper
        .write_plain(&key(), Value::fixnum(6), |_| called.set(true))
        .expect("valid key");
    assert_eq!(write, EffectiveWrite::OwnerSlot);
    assert!(!called.get());

    let mut stepper = engine(TestSlots::empty());
    let called = Cell::new(false);
    let bound = stepper
        .bound_plain(&key(), || {
            called.set(true);
            true
        })
        .expect("valid key");
    assert!(bound);
    assert!(called.get());

    let mut stepper = engine(TestSlots::bound(Value::fixnum(5)));
    let called = Cell::new(false);
    let stale = stepper.read_plain(&stale_key(), || {
        called.set(true);
        Value::fixnum(1)
    });
    assert_eq!(
        stale.expect_err("stale era").class,
        EscapeClass::StaleEffectiveKey
    );
    assert!(!called.get());
    assert_eq!(stepper.escape_handler().requests.len(), 1);
}

#[test]
fn every_outcome_reports_depth_and_live_roots() {
    let mut ctx = context();

    let mut stepper = engine(TestSlots::empty());
    let outcome = stepper.start(&mut ctx, body(&[Value::fixnum(1)]), wide());
    let StepOutcome::Done { pending, .. } = outcome else {
        panic!("a value must finish");
    };
    assert_eq!(stepper.arena_depth(), 0);
    assert_eq!(stepper.live_root_count(), 1);
    assert!(matches!(
        stepper.start(&mut ctx, body(&[Value::fixnum(2)]), wide()),
        StepOutcome::Fault(EngineFault::Busy)
    ));
    assert_eq!(stepper.live_root_count(), 1);
    assert_eq!(stepper.take(pending).expect("take"), Value::fixnum(1));
    assert_eq!(
        stepper.take(pending),
        Err(RootRevokeError::Stale),
        "the done root is popped"
    );
    assert_clear(&stepper);
    let again = stepper.start(&mut ctx, body(&[Value::fixnum(2)]), wide());
    assert_eq!(done_value(&mut stepper, again), Value::fixnum(2));
    assert_clear(&stepper);

    let mut stepper = engine(TestSlots::empty());
    let outcome = stepper.start(&mut ctx, body(&[call("car", &[Value::fixnum(1)])]), wide());
    assert!(matches!(outcome, StepOutcome::Signaled(_)));
    assert_clear(&stepper);
    let again = stepper.start(&mut ctx, body(&[Value::fixnum(4)]), wide());
    assert_eq!(done_value(&mut stepper, again), Value::fixnum(4));

    let mut stepper = Stage1::new(
        TestSlots::empty(),
        RecordingEscape::new(),
        MissingSealed,
        EraId::new(3),
        ShapeGeneration::new(7),
    );
    let outcome = stepper.start(&mut ctx, body(&[call("car", &[Value::NIL])]), wide());
    assert!(matches!(outcome, StepOutcome::MissingCore { .. }));
    assert_clear(&stepper);
    let again = stepper.start(&mut ctx, body(&[Value::fixnum(5)]), wide());
    assert_eq!(done_value(&mut stepper, again), Value::fixnum(5));

    let mut stepper = engine(TestSlots::empty());
    assert!(matches!(
        stepper.resume(&mut ctx),
        StepOutcome::Fault(EngineFault::NotSuspended)
    ));
    assert_clear(&stepper);
    let again = stepper.start(&mut ctx, body(&[Value::fixnum(6)]), wide());
    assert_eq!(done_value(&mut stepper, again), Value::fixnum(6));

    let mut stepper = engine(TestSlots::empty());
    let form = call("cons", &[Value::fixnum(1), Value::fixnum(2)]);
    let outcome = stepper.start(&mut ctx, body(&[form]), wide());
    let StepOutcome::Escaped(request) = outcome else {
        panic!("cons must escape");
    };
    assert!(eq_value(&request.form, &form));
    assert_eq!(stepper.arena_depth(), 1);
    assert_eq!(stepper.live_root_count(), 0);
    assert!(matches!(
        stepper.start(&mut ctx, body(&[Value::fixnum(7)]), wide()),
        StepOutcome::Fault(EngineFault::Busy)
    ));
    assert_eq!(stepper.arena_depth(), 1);
    assert_eq!(stepper.live_root_count(), 0);
}

#[test]
fn every_scalar_primitive_matches_serial_eval_on_the_sample_matrix() {
    let mut ctx = context();
    let mut scalars = Vec::new();
    for entry in inventory() {
        if entry.kind != CatalogKind::Primitive {
            continue;
        }
        let Some(slot) = primitive_slot_by_name(entry.name) else {
            continue;
        };
        if slot.arm != ParallelArm::Scalar {
            continue;
        }
        scalars.push(entry.name);
    }
    assert_eq!(scalars.len(), 14, "the 14 catalog Scalar primitives");
    let mut rows = vec![Vec::new()];
    rows.extend(samples().into_iter().map(|value| vec![value]));
    rows.push(vec![Value::fixnum(1), Value::T]);
    for name in scalars {
        for args in &rows {
            let form = quoted_call(name, args);
            let mut stepper = engine(TestSlots::empty());
            let outcome = stepper.start(&mut ctx, body(&[form]), wide());
            let engine_result = match outcome {
                StepOutcome::Done { pending, .. } => {
                    let value = stepper.take(pending).expect("done root");
                    assert_clear(&stepper);
                    Ok(value)
                }
                StepOutcome::Signaled(flow) => {
                    assert_clear(&stepper);
                    Err(signal_key(&flow))
                }
                StepOutcome::Escaped(request) => panic!("{name} escaped {:?}", request.class),
                StepOutcome::MissingCore { .. } => panic!("{name} has no shared core"),
                StepOutcome::Suspended { .. } => panic!("{name} suspended"),
                StepOutcome::Fault(_) => panic!("{name} faulted"),
            };
            let serial = match ctx.eval_sub(form) {
                Ok(value) => Ok(value),
                Err(flow) => Err(signal_key(&flow)),
            };
            match (engine_result, serial) {
                (Ok(left), Ok(right)) => assert_eq!(left, right, "{name}"),
                (Err(left), Err(right)) => assert_eq!(left, right, "{name}"),
                (Ok(_), Err(right)) => panic!("{name}: engine value, serial signal {right:?}"),
                (Err(left), Ok(_)) => panic!("{name}: engine signal {left:?}, serial value"),
            }
        }
    }
}

#[test]
fn every_admitted_sealed_read_has_a_shared_core_arm() {
    let mut ctx = context();
    let mut ops = HarnessSealed;
    for entry in inventory() {
        if entry.kind != CatalogKind::Primitive {
            continue;
        }
        let Some(slot) = primitive_slot_by_name(entry.name) else {
            continue;
        };
        let outcome = match slot.arm {
            ParallelArm::Sealed(SealedAuthority::ConsFieldRead) => {
                ops.read_cons_field(&mut ctx, entry.name, &[Value::NIL])
            }
            ParallelArm::Sealed(SealedAuthority::HeaderWord) => {
                ops.read_header_word(&mut ctx, entry.name, &[Value::NIL])
            }
            _ => continue,
        };
        assert!(
            !matches!(outcome, CoreOutcome::MissingCore),
            "{} has no shared core",
            entry.name
        );
    }
}

#[test]
fn header_word_integerp_does_not_escape() {
    let mut ctx = context();
    let mut stepper = engine(TestSlots::empty());
    let outcome = stepper.start(
        &mut ctx,
        body(&[call("integerp", &[Value::fixnum(1)])]),
        wide(),
    );
    assert_eq!(done_value(&mut stepper, outcome), Value::T);
    assert!(stepper.escape_handler().requests.is_empty());
}
