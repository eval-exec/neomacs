//! Stage-1 harness stepper. The sealed-ops impl reads only conses this
//! harness allocated. It does not read a shared object.

use super::super::owner_bindings::{Channel, SlotState, SlotValue};
use super::dispatch::primitive_slot_by_name;
use super::effective::{
    EffectivePlainKey, EffectiveRead, EffectiveWrite, EraId, OwnerSlots, OwnerSlotsMut,
    ShapeGeneration,
};
use super::engine::{
    CoreOutcome, EscapeClass, EscapeRequest, SealedOps, SerialEscape, Stage1, StepOutcome,
    apply_one, apply_slice, apply_vec, scalar_core,
};
use super::frames::{FrameArena, HandlerMark, InterpFrame, OwnerRecordMark, PollBudget};
use crate::emacs_core::builtins::{
    builtin_arrayp, builtin_atom_1, builtin_car_1, builtin_car_safe_1, builtin_cdr_1,
    builtin_cdr_safe_1, builtin_characterp, builtin_consp_1, builtin_floatp_1,
    builtin_hash_table_p, builtin_identity, builtin_integerp_1, builtin_listp_1,
    builtin_module_function_p, builtin_native_comp_available_p, builtin_nlistp_1, builtin_null_1,
    builtin_numberp_1, builtin_sequencep, builtin_stringp_1, builtin_symbol_with_pos_p_1,
    builtin_treesit_available_p, builtin_user_ptrp, builtin_vector_or_char_table_p,
    builtin_vectorp_1,
};
use crate::emacs_core::builtins_extra::{
    builtin_byte_code_function_p, builtin_byteorder, builtin_closurep, builtin_subrp,
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
use neovm_text::emacs_core::xml::builtin_libxml_available_p;
use neovm_text::emacs_core::zlib::builtin_zlib_available_p;
use neovm_text::encoding::builtin_multibyte_string_p_1;
use neovm_value::boolvec::builtin_bool_vector_p;
use neovm_value::error::Flow;
use neovm_value::forward::TlsIndex;
use neovm_value::intern::NIL_SYM_ID;
use neovm_value::value::Value;
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

fn assert_cores(label: &str, engine_outcome: CoreOutcome, serial: CoreOutcome) {
    match (engine_outcome, serial) {
        (CoreOutcome::Value(left), CoreOutcome::Value(right)) => {
            assert_eq!(left, right, "{label}");
        }
        (CoreOutcome::Signal(left), CoreOutcome::Signal(right)) => {
            assert_eq!(signal_key(&left), signal_key(&right), "{label}");
        }
        (CoreOutcome::MissingCore, _) => panic!("{label}: engine has no shared core"),
        (_, CoreOutcome::MissingCore) => panic!("{label}: serial oracle has no shared core"),
        (CoreOutcome::Value(_), CoreOutcome::Signal(_)) => {
            panic!("{label}: engine returned a value and the builtin signaled")
        }
        (CoreOutcome::Signal(_), CoreOutcome::Value(_)) => {
            panic!("{label}: engine signaled and the builtin returned a value")
        }
    }
}

fn done_value(
    engine: &mut Stage1<TestSlots, RecordingEscape, HarnessSealed>,
    outcome: StepOutcome,
) -> Value {
    match outcome {
        StepOutcome::Done { pending, .. } => engine.lookup_root(pending).expect("done root"),
        StepOutcome::Escaped(request) => panic!("escaped {:?}", request.class),
        StepOutcome::Signaled(flow) => panic!("signaled {}", signal_key(&flow).0),
        StepOutcome::MissingCore { name } => panic!("missing core {name}"),
        StepOutcome::Suspended { .. } => panic!("suspended"),
        StepOutcome::Fault(_) => panic!("fault"),
    }
}

/// Serial builtin calls, written separately from [`scalar_core`].
fn serial_scalar(ctx: &mut Context, name: &str, args: &[Value]) -> CoreOutcome {
    match name {
        "atom" => apply_one(ctx, "atom", args, builtin_atom_1),
        "byteorder" => apply_vec(args, builtin_byteorder),
        "characterp" => apply_vec(args, builtin_characterp),
        "consp" => apply_one(ctx, "consp", args, builtin_consp_1),
        "floatp" => apply_one(ctx, "floatp", args, builtin_floatp_1),
        "identity" => apply_vec(args, builtin_identity),
        "libxml-available-p" => apply_vec(args, builtin_libxml_available_p),
        "listp" => apply_one(ctx, "listp", args, builtin_listp_1),
        "native-comp-available-p" => apply_vec(args, builtin_native_comp_available_p),
        "nlistp" => apply_one(ctx, "nlistp", args, builtin_nlistp_1),
        "null" => apply_one(ctx, "null", args, builtin_null_1),
        "stringp" => apply_one(ctx, "stringp", args, builtin_stringp_1),
        "treesit-available-p" => apply_vec(args, builtin_treesit_available_p),
        "zlib-available-p" => apply_vec(args, builtin_zlib_available_p),
        _ => CoreOutcome::MissingCore,
    }
}

fn samples() -> Vec<Value> {
    vec![
        Value::NIL,
        Value::T,
        Value::fixnum(1),
        Value::make_float(1.5),
        Value::string("m"),
        Value::cons(Value::fixnum(1), Value::fixnum(2)),
        Value::vector(vec![Value::fixnum(9)]),
        Value::symbol("sym"),
        make_marker_value(None, None, false),
    ]
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
    let outcome = stepper.start(
        &mut ctx,
        body(&[call(
            "no-such-stage1-primitive",
            &[call("car", &[Value::fixnum(1)])],
        )]),
        wide(),
    );
    let StepOutcome::Escaped(request) = outcome else {
        panic!("unknown name must escape");
    };
    assert_eq!(request.class, EscapeClass::UnknownName);
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
    let outcome = stepper.start(
        &mut ctx,
        body(&[call("+", &[call("car", &[Value::fixnum(1)])])]),
        wide(),
    );
    let StepOutcome::Escaped(request) = outcome else {
        panic!("+ is a World arm");
    };
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
    let outcome = stepper.start(
        &mut ctx,
        body(&[call(
            "setcar",
            &[call("car", &[Value::fixnum(1)]), Value::fixnum(2)],
        )]),
        wide(),
    );
    let StepOutcome::Escaped(request) = outcome else {
        panic!("setcar is ConsFieldWrite");
    };
    assert_eq!(
        request.class,
        EscapeClass::UnimplementedSealed(SealedAuthority::ConsFieldWrite)
    );
    assert_eq!(stepper.escape_handler().requests.len(), 1);
    assert_eq!(
        request.form.cons_cdr().cons_car().cons_car(),
        Value::symbol("car")
    );

    let mut stepper = engine(TestSlots::empty());
    let outcome = stepper.start(
        &mut ctx,
        body(&[call("cons", &[Value::fixnum(1), Value::fixnum(2)])]),
        wide(),
    );
    let StepOutcome::Escaped(request) = outcome else {
        panic!("cons is ConsAllocation");
    };
    assert_eq!(
        request.class,
        EscapeClass::UnimplementedSealed(SealedAuthority::ConsAllocation)
    );
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
    assert_eq!(stepper.lookup_root(pending), Some(Value::fixnum(1)));
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
    assert_eq!(stepper.lookup_root(done_root), Some(Value::fixnum(2)));
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
fn every_scalar_primitive_matches_the_serial_builtin_on_the_sample_matrix() {
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
    for name in scalars {
        for args in &rows {
            let engine_outcome = scalar_core(&mut ctx, name, args);
            let serial = serial_scalar(&mut ctx, name, args);
            assert_cores(name, engine_outcome, serial);
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
