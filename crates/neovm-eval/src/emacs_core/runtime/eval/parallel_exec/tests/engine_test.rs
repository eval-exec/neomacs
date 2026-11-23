//! Stage-1 harness stepper. The sealed-ops impl reads only conses this
//! harness allocated. It does not read a shared object.

use super::super::owner_bindings::{
    BindingKind, Channel, OwnerBindingRecord, OwnerRecordId, OwnerSlab, SlotState, SlotValue,
    UndoTarget,
};
use super::dispatch::primitive_slot_by_name;
use super::effective::{
    EffectivePlainKey, EffectiveRead, EffectiveWrite, EraId, OwnerSlots, OwnerSlotsMut,
    PlainAccess, PlainAdmission, PlainRefusal, ShapeGeneration, take_top_binding,
};
use super::engine::{
    CoreOutcome, EngineFault, EscapeClass, EscapeRequest, OwnerPoll, PlainStop, SealedOps,
    SerialEscape, Stage1, StepOutcome, apply_one, apply_slice, apply_vec,
};
use super::frames::{
    FrameArena, HandlerMark, InterpFrame, OwnerRecordMark, OwnerScanJoin, PollBudget,
    RootRevokeError,
};
use super::poll_tests::{
    OWNER, activate, collect, end_epoch, finish, lend, recv, request_stop, retire, retry,
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
use crate::emacs_core::eval::world::poll::{
    PollOutcome, poll_interp_back_edge, poll_interp_call_entry, poll_serial_request,
};
use crate::emacs_core::eval::world::serial_gate::stop_for_test;
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
use neovm_value::tagged::SymbolMode;
use neovm_value::tagged::gc::{Mutator, RendezvousError, TaggedHeap};
use neovm_value::tagged::transport::SharedRoot;
use neovm_value::value::{Value, eq_value, symbol_id};
use std::cell::Cell;
use std::num::NonZeroU32;
use std::sync::mpsc;
use std::thread;

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

/// The wired poll edges. Order inside one stepper is observable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PollEdge {
    InterpCallEntry,
    InterpBackEdge,
    SerialRequest,
}

/// A poll double that records the edge and can refuse one call, either on the
/// next edge of any kind or on a chosen edge.
struct RecordingPoll {
    edges: Vec<PollEdge>,
    refusal: Option<RendezvousError>,
    refuse_on: Option<PollEdge>,
}

impl RecordingPoll {
    fn new() -> Self {
        Self {
            edges: Vec::new(),
            refusal: None,
            refuse_on: None,
        }
    }

    fn refusing(error: RendezvousError) -> Self {
        Self {
            edges: Vec::new(),
            refusal: Some(error),
            refuse_on: None,
        }
    }

    /// The refusal fires on the first call of `edge` only.
    fn refusing_on(edge: PollEdge, error: RendezvousError) -> Self {
        Self {
            edges: Vec::new(),
            refusal: Some(error),
            refuse_on: Some(edge),
        }
    }

    fn poll(&mut self, edge: PollEdge) -> Result<PollOutcome, RendezvousError> {
        self.edges.push(edge);
        if self.refuse_on.is_some_and(|wanted| wanted != edge) {
            return Ok(PollOutcome::Continue);
        }
        match self.refusal.take() {
            Some(error) => Err(error),
            None => Ok(PollOutcome::Continue),
        }
    }

    fn edge_count(&self, edge: PollEdge) -> usize {
        self.edges.iter().filter(|seen| **seen == edge).count()
    }
}

impl OwnerPoll for RecordingPoll {
    fn interp_call_entry(&mut self, _arena: &FrameArena) -> Result<PollOutcome, RendezvousError> {
        self.poll(PollEdge::InterpCallEntry)
    }

    fn interp_back_edge(&mut self, _arena: &FrameArena) -> Result<PollOutcome, RendezvousError> {
        self.poll(PollEdge::InterpBackEdge)
    }

    fn serial_request(&mut self, _arena: &FrameArena) -> Result<PollOutcome, RendezvousError> {
        self.poll(PollEdge::SerialRequest)
    }
}

/// The pre-era fixture. `forms` and `heap` cross to the owner thread as plain
/// numbers; `keep` roots the era-heap form list until the era finishes.
struct EraInput {
    forms: usize,
    heap: usize,
    _keep: Vec<SharedRoot>,
}

/// Build the form list in the era heap before the era is admitted, like
/// `hold_cons` in `poll_test`. Every value a frame can trace is then in the
/// heap the owner publishes to.
fn era_input(context: &mut Context, forms: Vec<Value>) -> EraInput {
    context.set_gc_stress_for_test(false);
    context.tagged_heap.set_gc_threshold(usize::MAX);
    context.tagged_heap.close_alloc_regions();
    let forms = Value::list(forms);
    // SAFETY: setup is the sole serial mutator; the root keeps the form list
    // reachable until the owner scan publishes or the era finishes.
    let root = unsafe { SharedRoot::new(&context.tagged_heap, forms) };
    EraInput {
        forms: forms.bits(),
        heap: context.tagged_heap.heap_root().as_ptr() as usize,
        _keep: vec![root],
    }
}

/// The honest chain: the engine calls the real wrapper, which calls
/// `Mutator::poll`, which publishes `arena.publish_scan` to the era keeper.
///
/// `gate` is test sequencing only: after the real wrapper returns on a chosen
/// edge (after `skip` earlier calls of that edge), the owner reports the edge
/// and waits for the keeper side to request a stop, so the next poll is
/// deterministically the parked one. The parked poll itself is the real
/// wrapper.
struct EraGate {
    edge: PollEdge,
    skip: usize,
    reached: mpsc::Sender<()>,
    go: mpsc::Receiver<()>,
}

struct EraPoll<'era> {
    owner: Mutator<'era>,
    heap: usize,
    edges: Vec<PollEdge>,
    build_calls: usize,
    call_entry_builds: usize,
    back_edge_builds: usize,
    serial_builds: usize,
    gate: Option<EraGate>,
}

impl<'era> EraPoll<'era> {
    fn new(owner: Mutator<'era>, heap: usize) -> Self {
        Self {
            owner,
            heap,
            edges: Vec::new(),
            build_calls: 0,
            call_entry_builds: 0,
            back_edge_builds: 0,
            serial_builds: 0,
            gate: None,
        }
    }

    fn with_gate(
        mut self,
        edge: PollEdge,
        skip: usize,
        reached: mpsc::Sender<()>,
        go: mpsc::Receiver<()>,
    ) -> Self {
        self.gate = Some(EraGate {
            edge,
            skip,
            reached,
            go,
        });
        self
    }

    fn edge_count(&self, edge: PollEdge) -> usize {
        self.edges.iter().filter(|seen| **seen == edge).count()
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
}

impl OwnerPoll for EraPoll<'_> {
    fn interp_call_entry(&mut self, arena: &FrameArena) -> Result<PollOutcome, RendezvousError> {
        self.edges.push(PollEdge::InterpCallEntry);
        let builds = &mut self.build_calls;
        let edge_builds = &mut self.call_entry_builds;
        let heap = self.heap;
        let outcome = poll_interp_call_entry(&mut self.owner, || {
            *builds += 1;
            *edge_builds += 1;
            // SAFETY: `heap` is the era provenance root captured before
            // admission. `publish_scan` only reads the arena, and the keeper
            // cannot sweep while this owner is inside `Mutator::poll`.
            let heap = unsafe { &*(heap as *const TaggedHeap) };
            let mut batch = Vec::new();
            arena.publish_scan(heap, &mut batch)?;
            Ok(batch)
        });
        self.run_gate(PollEdge::InterpCallEntry);
        outcome
    }

    fn interp_back_edge(&mut self, arena: &FrameArena) -> Result<PollOutcome, RendezvousError> {
        self.edges.push(PollEdge::InterpBackEdge);
        let builds = &mut self.build_calls;
        let edge_builds = &mut self.back_edge_builds;
        let heap = self.heap;
        let outcome = poll_interp_back_edge(&mut self.owner, || {
            *builds += 1;
            *edge_builds += 1;
            // SAFETY: see `interp_call_entry`.
            let heap = unsafe { &*(heap as *const TaggedHeap) };
            let mut batch = Vec::new();
            arena.publish_scan(heap, &mut batch)?;
            Ok(batch)
        });
        self.run_gate(PollEdge::InterpBackEdge);
        outcome
    }

    fn serial_request(&mut self, arena: &FrameArena) -> Result<PollOutcome, RendezvousError> {
        self.edges.push(PollEdge::SerialRequest);
        let builds = &mut self.build_calls;
        let serial_builds = &mut self.serial_builds;
        let heap = self.heap;
        poll_serial_request(&mut self.owner, || {
            *builds += 1;
            *serial_builds += 1;
            // SAFETY: see `interp_call_entry`.
            let heap = unsafe { &*(heap as *const TaggedHeap) };
            let mut batch = Vec::new();
            arena.publish_scan(heap, &mut batch)?;
            Ok(batch)
        })
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
    admission: PlainAdmission,
    canonical: Value,
    canonical_reads: Cell<usize>,
    canonical_writes: Cell<usize>,
    /// The binding-record LIFO, on a real slab: the stepper of this harness
    /// binds nothing, so the trait's record half is the slab's own API.
    records: OwnerSlab,
}

impl TestSlots {
    fn with_state(state: SlotState, admission: PlainAdmission) -> Self {
        Self {
            state,
            admission,
            canonical: Value::NIL,
            canonical_reads: Cell::new(0),
            canonical_writes: Cell::new(0),
            records: OwnerSlab::new(0),
        }
    }

    fn empty() -> Self {
        Self::with_state(
            SlotState::NoBinding,
            PlainAdmission::refuse(PlainRefusal::NotPlain),
        )
    }

    fn bound(value: Value) -> Self {
        Self::with_state(
            SlotState::Bound(SlotValue::new(value)),
            PlainAdmission::refuse(PlainRefusal::NotPlain),
        )
    }

    fn bound_with_admission(value: Value, admission: PlainAdmission) -> Self {
        Self::with_state(SlotState::Bound(SlotValue::new(value)), admission)
    }

    fn refused(admission: PlainAdmission) -> Self {
        Self::with_state(SlotState::NoBinding, admission)
    }

    fn canonical_reads(&self) -> usize {
        self.canonical_reads.get()
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

impl PlainAccess for TestSlots {
    fn admission(&self, sym: neovm_value::intern::SymId) -> PlainAdmission {
        match self.admission {
            PlainAdmission::Mint { sym: admitted, .. } if admitted == sym => self.admission,
            refusal @ PlainAdmission::Refuse(_) => refusal,
            _ => PlainAdmission::refuse(PlainRefusal::StaleBacking),
        }
    }

    fn canonical_read(&self, _sym: neovm_value::intern::SymId) -> Value {
        self.canonical_reads.set(self.canonical_reads.get() + 1);
        self.canonical
    }

    fn canonical_write(&mut self, _sym: neovm_value::intern::SymId, value: Value) {
        self.canonical_writes.set(self.canonical_writes.get() + 1);
        self.canonical = value;
    }

    fn canonical_bound(&self, _sym: neovm_value::intern::SymId) -> bool {
        true
    }

    /// This double models no watchers, no retired projection and no dead
    /// channel: every owner binding is undone on the owner.
    fn unbind_needs_serial(&self, _sym: neovm_value::intern::SymId) -> bool {
        false
    }

    fn record_live(&self, record: super::super::owner_bindings::OwnerRecordId) -> bool {
        self.records.live_records().any(|(id, _)| id == record)
    }

    fn push_binding(
        &mut self,
        sym: neovm_value::intern::SymId,
        slot: TlsIndex,
        channel: Channel,
        undo: UndoTarget,
    ) -> OwnerRecordId {
        self.records
            .push_record(sym, slot, BindingKind::Let, channel, undo)
    }

    fn pop_binding(&mut self, id: OwnerRecordId) -> Option<OwnerBindingRecord> {
        take_top_binding(&mut self.records, id)
    }
}

fn multibyte_string_p(ctx: &mut Context, arg: Value) -> neovm_value::error::EvalResult {
    builtin_multibyte_string_p_1(ctx, arg)
}

fn context() -> Context {
    Context::new(unit_runtime())
}

fn engine(slots: TestSlots) -> Stage1<TestSlots, RecordingEscape, HarnessSealed, RecordingPoll> {
    Stage1::new(
        slots,
        RecordingEscape::new(),
        HarnessSealed,
        RecordingPoll::new(),
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

fn assert_clear<O: SealedOps, P: OwnerPoll>(engine: &Stage1<TestSlots, RecordingEscape, O, P>) {
    assert_eq!(engine.arena_depth(), 0, "arena depth");
    assert_eq!(engine.live_root_count(), 0, "live roots");
}

fn done_value<O: SealedOps, P: OwnerPoll>(
    engine: &mut Stage1<TestSlots, RecordingEscape, O, P>,
    outcome: StepOutcome,
) -> Value {
    match outcome {
        StepOutcome::Done { pending, .. } => engine.take(pending).expect("done root"),
        StepOutcome::Escaped(request) => panic!("escaped {:?}", request.class),
        StepOutcome::Signaled(flow) => panic!("signaled {}", signal_key(&flow).0),
        StepOutcome::VoidVariable { .. } => panic!("void variable"),
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
fn a_refused_unwind_poisons_the_engine_and_start_refuses() {
    let mut ctx = context();
    let mut stepper = engine(TestSlots::empty());
    let outcome = stepper.start(
        &mut ctx,
        body(&[Value::fixnum(1), Value::fixnum(2)]),
        NonZeroU32::new(1).expect("interval"),
    );
    let StepOutcome::Suspended { .. } = outcome else {
        panic!("the first form must suspend");
    };
    assert!(stepper.pop_top_frame_keep_note());
    assert_eq!(stepper.arena_depth(), 0);
    let resumed = stepper.resume(&mut ctx);
    assert!(matches!(
        resumed,
        StepOutcome::Fault(EngineFault::StaleFrame)
    ));
    assert_eq!(stepper.arena_depth(), 0);
    assert_eq!(stepper.live_root_count(), 0);
    let refused = stepper.start(&mut ctx, body(&[Value::fixnum(9)]), wide());
    assert!(
        matches!(refused, StepOutcome::Fault(EngineFault::Poisoned)),
        "a refused unwind must refuse the next start"
    );
    assert_eq!(stepper.arena_depth(), 0);
    assert_eq!(stepper.live_root_count(), 0);
    assert!(matches!(
        stepper.resume(&mut ctx),
        StepOutcome::Fault(EngineFault::Poisoned)
    ));
    assert_eq!(stepper.arena_depth(), 0);
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
    let PlainStop::Escape(request) = stale.expect_err("stale era") else {
        panic!("a stale key is an escape, not a poll refusal");
    };
    assert_eq!(request.class, EscapeClass::StaleEffectiveKey);
    assert!(!called.get());
    assert_eq!(stepper.escape_handler().requests.len(), 1);
    assert_eq!(
        stepper.poll_handler().edge_count(PollEdge::SerialRequest),
        1,
        "the plain access polls before the stale-key escape"
    );
}

/// A valid plain key is not a safepoint: the caller may hold an unrooted
/// value on the Rust stack (`write_plain`'s `value`). Only a stale key polls,
/// once, immediately before the escape it records.
#[test]
fn plain_access_polls_only_the_stale_key_path() {
    let mut stepper = engine(TestSlots::empty());
    stepper
        .read_plain(&key(), || Value::fixnum(1))
        .expect("a valid key reads");
    stepper
        .write_plain(&key(), Value::fixnum(2), |_| {})
        .expect("a valid key writes");
    assert!(
        stepper
            .bound_plain(&key(), || true)
            .expect("a valid key is bound")
    );
    assert_eq!(
        stepper.poll_handler().edge_count(PollEdge::SerialRequest),
        0,
        "a valid plain access must not reach a safepoint"
    );
    assert!(stepper.escape_handler().requests.is_empty());

    let mut stepper = engine(TestSlots::empty());
    let called = Cell::new(false);
    let stale_read = stepper.read_plain(&stale_key(), || {
        called.set(true);
        Value::fixnum(1)
    });
    assert!(matches!(stale_read, Err(PlainStop::Escape(_))));
    assert!(!called.get(), "a stale key does not read");
    assert_eq!(
        stepper.poll_handler().edge_count(PollEdge::SerialRequest),
        1,
        "the stale-key poll runs once, before the escape"
    );
    assert_eq!(stepper.escape_handler().requests.len(), 1);
    assert_eq!(
        stepper.escape_handler().requests[0].class,
        EscapeClass::StaleEffectiveKey
    );

    let mut stepper = engine(TestSlots::empty());
    let stale_write = stepper.write_plain(&stale_key(), Value::fixnum(3), |_| {
        panic!("a stale key does not write")
    });
    assert!(
        matches!(stale_write, Err(PlainStop::Escape(_))),
        "the unrooted write value is abandoned, never stored"
    );
    assert_eq!(
        stepper.poll_handler().edge_count(PollEdge::SerialRequest),
        1
    );
    assert_eq!(stepper.escape_handler().requests.len(), 1);

    let mut stepper = engine(TestSlots::empty());
    let stale_bound = stepper.bound_plain(&stale_key(), || panic!("a stale key does not bound"));
    assert!(matches!(stale_bound, Err(PlainStop::Escape(_))));
    assert_eq!(
        stepper.poll_handler().edge_count(PollEdge::SerialRequest),
        1
    );
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
        RecordingPoll::new(),
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
                StepOutcome::VoidVariable { .. } => panic!("{name} produced a void variable"),
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

/// Test 1: the wired return-boundary poll parks against the real keeper and
/// resumes the SAME frame, with the harness budget and its root untouched.
#[test]
fn wired_interp_call_entry_parks_and_resumes_the_same_frame() {
    Context::with_test_collection_era(
        |context| era_input(context, vec![Value::fixnum(1), Value::fixnum(2)]),
        |era, input| {
            let permit = era.register(OWNER, 0).expect("register");
            let heap = era.heap_identity();
            let rendezvous = era.rendezvous();
            let forms_bits = input.forms;
            let heap_addr = input.heap;
            let (suspended_tx, suspended_rx) = mpsc::channel();
            let (go_tx, go_rx) = mpsc::channel::<()>();
            let (gate_tx, gate_rx) = mpsc::channel::<()>();
            let (gate_go_tx, gate_go_rx) = mpsc::channel::<()>();
            let (done_tx, done_rx) = mpsc::channel();
            thread::scope(|scope| {
                scope.spawn(move || {
                    let owner = activate(permit, heap, OWNER);
                    let mut ctx = Context::new(unit_runtime());
                    let mut stepper = Stage1::new(
                        TestSlots::empty(),
                        RecordingEscape::new(),
                        HarnessSealed,
                        // B1 runs during `start`; gate B2, the body step that
                        // runs inside `resume`, so the stop lands before the
                        // second form-return poll.
                        EraPoll::new(owner, heap_addr).with_gate(
                            PollEdge::InterpBackEdge,
                            1,
                            gate_tx,
                            gate_go_rx,
                        ),
                        EraId::new(3),
                        ShapeGeneration::new(7),
                    );
                    let forms = unsafe { Value::from_bits(forms_bits) };
                    let first = stepper.start(&mut ctx, forms, NonZeroU32::MIN);
                    let StepOutcome::Suspended { frame, .. } = first else {
                        panic!("the completed first form must suspend through the budget");
                    };
                    suspended_tx
                        .send((frame, stepper.arena_depth(), stepper.frame_is_live(frame)))
                        .expect("suspended report");
                    recv(&go_rx);
                    // The stop was requested at the body step during this
                    // resume, so the after_form poll at the second form's
                    // return is the one that parks.
                    let finished = stepper.resume(&mut ctx);
                    let StepOutcome::Done {
                        frame: done_frame,
                        pending,
                    } = finished
                    else {
                        panic!("the parked run must finish");
                    };
                    let value = stepper.take(pending).expect("done root");
                    let value_ok = eq_value(&value, &Value::fixnum(2));
                    let depth = stepper.arena_depth();
                    let roots = stepper.live_root_count();
                    let poll = stepper.into_poll();
                    let interp_edges = poll.edge_count(PollEdge::InterpCallEntry);
                    let back_edges = poll.edge_count(PollEdge::InterpBackEdge);
                    let serial_edges = poll.edge_count(PollEdge::SerialRequest);
                    let call_entry_builds = poll.call_entry_builds;
                    let back_edge_builds = poll.back_edge_builds;
                    let builds = poll.build_calls;
                    done_tx
                        .send((
                            done_frame,
                            value_ok,
                            depth,
                            roots,
                            interp_edges,
                            back_edges,
                            serial_edges,
                            call_entry_builds,
                            back_edge_builds,
                            builds,
                        ))
                        .expect("done report");
                    retire(poll.owner);
                });
                let (frame, depth, live) = recv(&suspended_rx);
                assert!(live, "the suspended frame must stay live");
                assert_eq!(depth, 1, "one frame before the stop");
                go_tx.send(()).expect("resume");
                recv(&gate_rx);
                request_stop(rendezvous);
                gate_go_tx.send(()).expect("gate go");
                let stopped = retry("stop", || stop_for_test(&era));
                let mut ticket = lend(&era, stopped);
                collect(&mut ticket);
                end_epoch(ticket);
                let (
                    done_frame,
                    value_ok,
                    depth,
                    roots,
                    interp_edges,
                    back_edges,
                    serial_edges,
                    call_entry_builds,
                    back_edge_builds,
                    builds,
                ) = recv(&done_rx);
                assert_eq!(done_frame, frame, "resume must continue the same FrameId");
                assert!(value_ok, "the parked frame must keep its accumulator");
                assert_eq!(depth, 0, "the done frame is popped");
                assert_eq!(roots, 0, "the done root is revoked");
                assert_eq!(
                    interp_edges, 2,
                    "one return-boundary poll per completed form"
                );
                assert_eq!(back_edges, 2, "one body step per fetched form");
                assert_eq!(serial_edges, 0, "no serial escape on this path");
                assert_eq!(
                    call_entry_builds, 1,
                    "the parked poll is the after_form return boundary"
                );
                assert_eq!(
                    back_edge_builds, 0,
                    "the gated body step ran on the fast path"
                );
                assert!(builds >= 1, "the parked poll must publish a scan batch");
            });
            drop(input);
            finish(era);
        },
    );
}

/// Test 4: an admissible phase word keeps the batch closure unbuilt.
#[test]
fn wired_interp_call_entry_fast_path_builds_no_batch() {
    Context::with_test_collection_era(
        |context| era_input(context, vec![Value::fixnum(1), Value::fixnum(2)]),
        |era, input| {
            let permit = era.register(OWNER, 0).expect("register");
            let heap = era.heap_identity();
            let forms_bits = input.forms;
            let heap_addr = input.heap;
            thread::scope(|scope| {
                scope.spawn(move || {
                    let owner = activate(permit, heap, OWNER);
                    let mut ctx = Context::new(unit_runtime());
                    let mut stepper = Stage1::new(
                        TestSlots::empty(),
                        RecordingEscape::new(),
                        HarnessSealed,
                        EraPoll::new(owner, heap_addr),
                        EraId::new(3),
                        ShapeGeneration::new(7),
                    );
                    let forms = unsafe { Value::from_bits(forms_bits) };
                    let outcome = stepper.start(&mut ctx, forms, wide());
                    let StepOutcome::Done { pending, .. } = outcome else {
                        panic!("an admissible run must finish");
                    };
                    assert_eq!(stepper.take(pending).expect("done root"), Value::fixnum(2));
                    let poll = stepper.into_poll();
                    assert_eq!(
                        poll.build_calls, 0,
                        "the fast path must not call the roots closure"
                    );
                    assert_eq!(poll.edge_count(PollEdge::InterpCallEntry), 2);
                    retire(poll.owner);
                });
            });
            drop(input);
            finish(era);
        },
    );
}

/// Test 5: the serial-request poll runs before the escape, parks against the
/// real keeper, and the escape is recorded once after resume.
#[test]
fn wired_serial_request_parks_before_the_escape() {
    Context::with_test_collection_era(
        |context| {
            era_input(
                context,
                vec![Value::list(vec![
                    Value::symbol("+"),
                    Value::fixnum(1),
                    Value::fixnum(2),
                ])],
            )
        },
        |era, input| {
            let permit = era.register(OWNER, 0).expect("register");
            let heap = era.heap_identity();
            let rendezvous = era.rendezvous();
            let forms_bits = input.forms;
            let heap_addr = input.heap;
            let (ready_tx, ready_rx) = mpsc::channel();
            let (go_tx, go_rx) = mpsc::channel::<()>();
            let (gate_tx, gate_rx) = mpsc::channel::<()>();
            let (gate_go_tx, gate_go_rx) = mpsc::channel::<()>();
            let (done_tx, done_rx) = mpsc::channel();
            thread::scope(|scope| {
                scope.spawn(move || {
                    let owner = activate(permit, heap, OWNER);
                    let mut ctx = Context::new(unit_runtime());
                    let mut stepper = Stage1::new(
                        TestSlots::empty(),
                        RecordingEscape::new(),
                        HarnessSealed,
                        // Gate the compound-form entry poll so the stop lands
                        // after it and before `record`'s serial request.
                        EraPoll::new(owner, heap_addr).with_gate(
                            PollEdge::InterpCallEntry,
                            0,
                            gate_tx,
                            gate_go_rx,
                        ),
                        EraId::new(3),
                        ShapeGeneration::new(7),
                    );
                    let forms = unsafe { Value::from_bits(forms_bits) };
                    ready_tx.send(()).expect("ready");
                    recv(&go_rx);
                    // The stop was requested at the call-entry edge, so the
                    // poll inside `record` parks, then the escape is recorded.
                    let outcome = stepper.start(&mut ctx, forms, wide());
                    let StepOutcome::Escaped(request) = outcome else {
                        panic!("a World arm must escape after the poll");
                    };
                    let world = matches!(request.class, EscapeClass::World(_));
                    let recorded = stepper.escape_handler().requests.len();
                    let roots = stepper.live_root_count();
                    let depth = stepper.arena_depth();
                    let poll = stepper.into_poll();
                    let serial_edges = poll.edge_count(PollEdge::SerialRequest);
                    let interp_edges = poll.edge_count(PollEdge::InterpCallEntry);
                    let back_edges = poll.edge_count(PollEdge::InterpBackEdge);
                    let serial_builds = poll.serial_builds;
                    let call_entry_builds = poll.call_entry_builds;
                    let back_edge_builds = poll.back_edge_builds;
                    let builds = poll.build_calls;
                    done_tx
                        .send((
                            world,
                            recorded,
                            roots,
                            depth,
                            serial_edges,
                            interp_edges,
                            back_edges,
                            serial_builds,
                            call_entry_builds,
                            back_edge_builds,
                            builds,
                        ))
                        .expect("done report");
                    retire(poll.owner);
                });
                recv(&ready_rx);
                go_tx.send(()).expect("go");
                recv(&gate_rx);
                request_stop(rendezvous);
                gate_go_tx.send(()).expect("gate go");
                let stopped = retry("stop", || stop_for_test(&era));
                let mut ticket = lend(&era, stopped);
                collect(&mut ticket);
                end_epoch(ticket);
                let (
                    world,
                    recorded,
                    roots,
                    depth,
                    serial_edges,
                    interp_edges,
                    back_edges,
                    serial_builds,
                    call_entry_builds,
                    back_edge_builds,
                    builds,
                ) = recv(&done_rx);
                assert!(world, "the form reached the World arm");
                assert_eq!(recorded, 1, "exactly one escape is recorded");
                assert_eq!(roots, 0, "the escape pushes no root");
                assert_eq!(depth, 1, "the escape keeps its top frame");
                assert_eq!(serial_edges, 1, "one serial-request poll before the escape");
                assert_eq!(interp_edges, 1, "only the compound-form entry poll");
                assert_eq!(back_edges, 1, "one body step fetched the form");
                assert_eq!(serial_builds, 1, "the serial-request poll parked");
                assert_eq!(call_entry_builds, 0, "the entry poll ran on the fast path");
                assert_eq!(back_edge_builds, 0, "the body step ran on the fast path");
                assert!(builds >= 1, "the parked poll must publish a scan batch");
            });
            drop(input);
            finish(era);
        },
    );
}

/// Test 3: a local form crosses the wired edges without a serial escape; a
/// World form records exactly one escape after its poll.
#[test]
fn wired_poll_edges_do_not_dispatch_serially() {
    let mut ctx = context();

    let mut stepper = engine(TestSlots::empty());
    let outcome = stepper.start(
        &mut ctx,
        body(&[Value::fixnum(1), Value::fixnum(2)]),
        wide(),
    );
    assert_eq!(done_value(&mut stepper, outcome), Value::fixnum(2));
    assert!(stepper.escape_handler().requests.is_empty());
    assert_eq!(
        stepper.poll_handler().edge_count(PollEdge::InterpCallEntry),
        2
    );
    assert_eq!(
        stepper.poll_handler().edge_count(PollEdge::SerialRequest),
        0
    );

    let mut stepper = engine(TestSlots::empty());
    let form = call("+", &[Value::fixnum(1), Value::fixnum(2)]);
    let outcome = stepper.start(&mut ctx, body(&[form]), wide());
    let StepOutcome::Escaped(request) = outcome else {
        panic!("+ is a World arm");
    };
    assert!(matches!(request.class, EscapeClass::World(_)));
    assert!(eq_value(&request.form, &form));
    assert_eq!(stepper.escape_handler().requests.len(), 1);
    assert_eq!(
        stepper.poll_handler().edge_count(PollEdge::SerialRequest),
        1,
        "the poll runs before the escape"
    );
    assert_eq!(
        stepper.poll_handler().edge_count(PollEdge::InterpCallEntry),
        1,
        "one compound-form entry poll; the form never returned a value"
    );
    assert_eq!(
        stepper.poll_handler().edge_count(PollEdge::InterpBackEdge),
        1,
        "one body step fetched the form"
    );
}

/// Test 2: every terminal outcome leaves no live root.
#[test]
fn wired_outcomes_leave_no_live_roots() {
    let mut ctx = context();

    let mut stepper = engine(TestSlots::empty());
    let outcome = stepper.start(&mut ctx, body(&[Value::fixnum(1)]), wide());
    let StepOutcome::Done { pending, .. } = outcome else {
        panic!("a fixnum finishes");
    };
    assert_eq!(stepper.take(pending).expect("done root"), Value::fixnum(1));
    assert_clear(&stepper);

    let mut stepper = engine(TestSlots::empty());
    let outcome = stepper.start(&mut ctx, body(&[call("car", &[Value::fixnum(1)])]), wide());
    assert!(matches!(outcome, StepOutcome::Signaled(_)));
    assert_clear(&stepper);

    let mut stepper = Stage1::new(
        TestSlots::empty(),
        RecordingEscape::new(),
        MissingSealed,
        RecordingPoll::new(),
        EraId::new(3),
        ShapeGeneration::new(7),
    );
    let outcome = stepper.start(&mut ctx, body(&[call("car", &[Value::NIL])]), wide());
    assert!(matches!(outcome, StepOutcome::MissingCore { .. }));
    assert_clear(&stepper);

    let mut stepper = engine(TestSlots::empty());
    let outcome = stepper.start(
        &mut ctx,
        body(&[call("cons", &[Value::NIL, Value::NIL])]),
        wide(),
    );
    assert!(matches!(outcome, StepOutcome::Escaped(_)));
    assert_eq!(stepper.live_root_count(), 0, "an escape publishes no root");
    assert_eq!(stepper.arena_depth(), 1, "an escape keeps its top frame");
}

/// libeval's extra row: a poll refusal unwinds to Idle, leaves no root, and
/// the stepper accepts a fresh run.
#[test]
fn poll_fault_unwinds_to_idle_and_the_engine_restarts() {
    let mut ctx = context();

    let mut stepper = Stage1::new(
        TestSlots::empty(),
        RecordingEscape::new(),
        HarnessSealed,
        RecordingPoll::refusing(RendezvousError::Publication),
        EraId::new(3),
        ShapeGeneration::new(7),
    );
    let outcome = stepper.start(&mut ctx, body(&[Value::fixnum(1)]), wide());
    assert!(
        matches!(
            outcome,
            StepOutcome::Fault(EngineFault::Poll(RendezvousError::Publication))
        ),
        "a non-Busy refusal is a Poll fault"
    );
    assert_clear(&stepper);
    let again = stepper.start(&mut ctx, body(&[Value::fixnum(2)]), wide());
    assert_eq!(done_value(&mut stepper, again), Value::fixnum(2));
    assert_clear(&stepper);

    // The stale-key poll surfaces the same refusal without touching state and
    // without recording the escape it preempted.
    let mut stepper = Stage1::new(
        TestSlots::empty(),
        RecordingEscape::new(),
        HarnessSealed,
        RecordingPoll::refusing(RendezvousError::Capacity),
        EraId::new(3),
        ShapeGeneration::new(7),
    );
    let plain = stepper.read_plain(&stale_key(), || Value::fixnum(9));
    let PlainStop::Poll(RendezvousError::Capacity) = plain.expect_err("poll refusal") else {
        panic!("the stale-key poll refusal must surface");
    };
    assert!(
        stepper.escape_handler().requests.is_empty(),
        "a refused poll records no escape"
    );
    assert_clear(&stepper);
    let again = stepper.start(&mut ctx, body(&[Value::fixnum(3)]), wide());
    assert_eq!(done_value(&mut stepper, again), Value::fixnum(3));
}

/// Site A honest chain: the compound-form entry poll parks against the real
/// keeper, the keeper collects at that park, and the post-resume `car` reads
/// the era-heap call form (including its quoted payload) and returns 1.
#[test]
fn wired_call_entry_parks_and_resumes_the_same_form() {
    Context::with_test_collection_era(
        |context| {
            era_input(
                context,
                vec![Value::list(vec![
                    Value::symbol("car"),
                    Value::list(vec![
                        Value::symbol("quote"),
                        Value::list(vec![Value::fixnum(1), Value::fixnum(2)]),
                    ]),
                ])],
            )
        },
        |era, input| {
            let permit = era.register(OWNER, 0).expect("register");
            let heap = era.heap_identity();
            let rendezvous = era.rendezvous();
            let forms_bits = input.forms;
            let heap_addr = input.heap;
            let (gate_tx, gate_rx) = mpsc::channel::<()>();
            let (gate_go_tx, gate_go_rx) = mpsc::channel::<()>();
            let (done_tx, done_rx) = mpsc::channel();
            thread::scope(|scope| {
                scope.spawn(move || {
                    let owner = activate(permit, heap, OWNER);
                    let mut ctx = Context::new(unit_runtime());
                    let mut stepper = Stage1::new(
                        TestSlots::empty(),
                        RecordingEscape::new(),
                        HarnessSealed,
                        // Gate the body step that fetches the call form, so the
                        // stop lands before the compound-form entry poll.
                        EraPoll::new(owner, heap_addr).with_gate(
                            PollEdge::InterpBackEdge,
                            0,
                            gate_tx,
                            gate_go_rx,
                        ),
                        EraId::new(3),
                        ShapeGeneration::new(7),
                    );
                    let forms = unsafe { Value::from_bits(forms_bits) };
                    let outcome = stepper.start(&mut ctx, forms, wide());
                    let StepOutcome::Done { pending, .. } = outcome else {
                        panic!("the parked call must finish");
                    };
                    let value = stepper.take(pending).expect("done root");
                    let value_ok = eq_value(&value, &Value::fixnum(1));
                    let depth = stepper.arena_depth();
                    let roots = stepper.live_root_count();
                    let poll = stepper.into_poll();
                    let interp_edges = poll.edge_count(PollEdge::InterpCallEntry);
                    let back_edges = poll.edge_count(PollEdge::InterpBackEdge);
                    let call_entry_builds = poll.call_entry_builds;
                    let back_edge_builds = poll.back_edge_builds;
                    done_tx
                        .send((
                            value_ok,
                            depth,
                            roots,
                            interp_edges,
                            back_edges,
                            call_entry_builds,
                            back_edge_builds,
                        ))
                        .expect("done report");
                    retire(poll.owner);
                });
                recv(&gate_rx);
                request_stop(rendezvous);
                gate_go_tx.send(()).expect("gate go");
                let stopped = retry("stop", || stop_for_test(&era));
                let mut ticket = lend(&era, stopped);
                collect(&mut ticket);
                end_epoch(ticket);
                let (
                    value_ok,
                    depth,
                    roots,
                    interp_edges,
                    back_edges,
                    call_entry_builds,
                    back_edge_builds,
                ) = recv(&done_rx);
                assert!(value_ok, "the post-resume car must read the collected form");
                assert_eq!(depth, 0, "the done frame is popped");
                assert_eq!(roots, 0, "the done root is revoked");
                assert_eq!(
                    interp_edges, 2,
                    "the parked entry poll plus the return boundary"
                );
                assert_eq!(back_edges, 1, "one body step fetched the call form");
                assert_eq!(call_entry_builds, 1, "the compound-form entry poll parked");
                assert_eq!(back_edge_builds, 0, "the gated body step stayed fast");
            });
            drop(input);
            finish(era);
        },
    );
}

/// Site B honest chain: the `progn` body step parks, the keeper collects at
/// that park, and the post-resume path reads the rest list and its quoted
/// literal (`(car '(5 6))` -> 5).
#[test]
fn wired_back_edge_parks_and_resumes_the_same_frame() {
    Context::with_test_collection_era(
        |context| {
            era_input(
                context,
                vec![
                    Value::fixnum(1),
                    Value::list(vec![
                        Value::symbol("car"),
                        Value::list(vec![
                            Value::symbol("quote"),
                            Value::list(vec![Value::fixnum(5), Value::fixnum(6)]),
                        ]),
                    ]),
                ],
            )
        },
        |era, input| {
            let permit = era.register(OWNER, 0).expect("register");
            let heap = era.heap_identity();
            let rendezvous = era.rendezvous();
            let forms_bits = input.forms;
            let heap_addr = input.heap;
            let (ready_tx, ready_rx) = mpsc::channel();
            let (go_tx, go_rx) = mpsc::channel::<()>();
            let (done_tx, done_rx) = mpsc::channel();
            thread::scope(|scope| {
                scope.spawn(move || {
                    let owner = activate(permit, heap, OWNER);
                    let mut ctx = Context::new(unit_runtime());
                    let mut stepper = Stage1::new(
                        TestSlots::empty(),
                        RecordingEscape::new(),
                        HarnessSealed,
                        EraPoll::new(owner, heap_addr),
                        EraId::new(3),
                        ShapeGeneration::new(7),
                    );
                    let forms = unsafe { Value::from_bits(forms_bits) };
                    ready_tx.send(()).expect("ready");
                    recv(&go_rx);
                    // The stop is already active: the first body step parks.
                    let outcome = stepper.start(&mut ctx, forms, wide());
                    let StepOutcome::Done { pending, .. } = outcome else {
                        panic!("the parked run must finish");
                    };
                    let value = stepper.take(pending).expect("done root");
                    let value_ok = eq_value(&value, &Value::fixnum(5));
                    let depth = stepper.arena_depth();
                    let roots = stepper.live_root_count();
                    let poll = stepper.into_poll();
                    let interp_edges = poll.edge_count(PollEdge::InterpCallEntry);
                    let back_edges = poll.edge_count(PollEdge::InterpBackEdge);
                    let call_entry_builds = poll.call_entry_builds;
                    let back_edge_builds = poll.back_edge_builds;
                    done_tx
                        .send((
                            value_ok,
                            depth,
                            roots,
                            interp_edges,
                            back_edges,
                            call_entry_builds,
                            back_edge_builds,
                        ))
                        .expect("done report");
                    retire(poll.owner);
                });
                recv(&ready_rx);
                request_stop(rendezvous);
                go_tx.send(()).expect("go");
                let stopped = retry("stop", || stop_for_test(&era));
                let mut ticket = lend(&era, stopped);
                collect(&mut ticket);
                end_epoch(ticket);
                let (
                    value_ok,
                    depth,
                    roots,
                    interp_edges,
                    back_edges,
                    call_entry_builds,
                    back_edge_builds,
                ) = recv(&done_rx);
                assert!(
                    value_ok,
                    "the post-resume car must read the collected rest list"
                );
                assert_eq!(depth, 0, "the done frame is popped");
                assert_eq!(roots, 0, "the done root is revoked");
                assert_eq!(interp_edges, 3, "two return boundaries plus the car entry");
                assert_eq!(back_edges, 2, "one body step per fetched form");
                assert_eq!(back_edge_builds, 1, "the first body step parked");
                assert_eq!(call_entry_builds, 0, "no call-entry poll parked");
            });
            drop(input);
            finish(era);
        },
    );
}

/// Site A fast path: an admissible phase word never builds the batch.
#[test]
fn wired_call_entry_fast_path_builds_no_batch() {
    Context::with_test_collection_era(
        |context| {
            era_input(
                context,
                vec![Value::list(vec![
                    Value::symbol("car"),
                    Value::list(vec![
                        Value::symbol("quote"),
                        Value::list(vec![Value::fixnum(1), Value::fixnum(2)]),
                    ]),
                ])],
            )
        },
        |era, input| {
            let permit = era.register(OWNER, 0).expect("register");
            let heap = era.heap_identity();
            let forms_bits = input.forms;
            let heap_addr = input.heap;
            thread::scope(|scope| {
                scope.spawn(move || {
                    let owner = activate(permit, heap, OWNER);
                    let mut ctx = Context::new(unit_runtime());
                    let mut stepper = Stage1::new(
                        TestSlots::empty(),
                        RecordingEscape::new(),
                        HarnessSealed,
                        EraPoll::new(owner, heap_addr),
                        EraId::new(3),
                        ShapeGeneration::new(7),
                    );
                    let forms = unsafe { Value::from_bits(forms_bits) };
                    let outcome = stepper.start(&mut ctx, forms, wide());
                    let StepOutcome::Done { pending, .. } = outcome else {
                        panic!("an admissible run must finish");
                    };
                    assert_eq!(stepper.take(pending).expect("done root"), Value::fixnum(1));
                    let poll = stepper.into_poll();
                    assert!(
                        poll.edge_count(PollEdge::InterpCallEntry) >= 1,
                        "the call-entry edge ran"
                    );
                    assert_eq!(
                        poll.call_entry_builds, 0,
                        "the call-entry fast path must not build a batch"
                    );
                    assert_eq!(poll.back_edge_builds, 0);
                    retire(poll.owner);
                });
            });
            drop(input);
            finish(era);
        },
    );
}

/// Site B fast path: an admissible phase word never builds the batch.
#[test]
fn wired_back_edge_fast_path_builds_no_batch() {
    Context::with_test_collection_era(
        |context| era_input(context, vec![Value::fixnum(1), Value::fixnum(2)]),
        |era, input| {
            let permit = era.register(OWNER, 0).expect("register");
            let heap = era.heap_identity();
            let forms_bits = input.forms;
            let heap_addr = input.heap;
            thread::scope(|scope| {
                scope.spawn(move || {
                    let owner = activate(permit, heap, OWNER);
                    let mut ctx = Context::new(unit_runtime());
                    let mut stepper = Stage1::new(
                        TestSlots::empty(),
                        RecordingEscape::new(),
                        HarnessSealed,
                        EraPoll::new(owner, heap_addr),
                        EraId::new(3),
                        ShapeGeneration::new(7),
                    );
                    let forms = unsafe { Value::from_bits(forms_bits) };
                    let outcome = stepper.start(&mut ctx, forms, wide());
                    let StepOutcome::Done { pending, .. } = outcome else {
                        panic!("an admissible run must finish");
                    };
                    assert_eq!(stepper.take(pending).expect("done root"), Value::fixnum(2));
                    let poll = stepper.into_poll();
                    assert!(
                        poll.edge_count(PollEdge::InterpBackEdge) >= 2,
                        "every fetched form crossed the body step"
                    );
                    assert_eq!(
                        poll.back_edge_builds, 0,
                        "the body-step fast path must not build a batch"
                    );
                    assert_eq!(poll.call_entry_builds, 0);
                    retire(poll.owner);
                });
            });
            drop(input);
            finish(era);
        },
    );
}

/// Edge order for `(progn 1)`: back edge, entry, back edge, the child's return
/// boundary, the parent's Continued return boundary. The last two are both
/// return boundaries, so no back-edge poll fires after after_form or a
/// Continued return.
#[test]
fn wired_edge_order_is_call_entry_then_body_steps() {
    let mut ctx = context();
    let mut stepper = engine(TestSlots::empty());
    let forms = body(&[Value::list(vec![Value::symbol("progn"), Value::fixnum(1)])]);
    let outcome = stepper.start(&mut ctx, forms, wide());
    assert_eq!(done_value(&mut stepper, outcome), Value::fixnum(1));
    assert_eq!(
        stepper.poll_handler().edges,
        vec![
            PollEdge::InterpBackEdge,
            PollEdge::InterpCallEntry,
            PollEdge::InterpBackEdge,
            PollEdge::InterpCallEntry,
            PollEdge::InterpCallEntry,
        ],
        "the body step poll must not double the return-boundary polls"
    );
}

/// Site A refusal: the compound-form entry poll refuses, the run unwinds to
/// Idle with no escape, and a fresh run is accepted.
#[test]
fn poll_refusal_at_call_entry_unwinds_and_restarts() {
    let mut ctx = context();
    let mut stepper = Stage1::new(
        TestSlots::empty(),
        RecordingEscape::new(),
        HarnessSealed,
        RecordingPoll::refusing_on(PollEdge::InterpCallEntry, RendezvousError::Publication),
        EraId::new(3),
        ShapeGeneration::new(7),
    );
    let outcome = stepper.start(
        &mut ctx,
        body(&[call("car", &[quote(Value::list(vec![Value::fixnum(1)]))])]),
        wide(),
    );
    assert!(
        matches!(
            outcome,
            StepOutcome::Fault(EngineFault::Poll(RendezvousError::Publication))
        ),
        "the call-entry refusal is a Poll fault"
    );
    assert!(stepper.escape_handler().requests.is_empty());
    assert_clear(&stepper);
    let again = stepper.start(&mut ctx, body(&[Value::fixnum(9)]), wide());
    assert_eq!(done_value(&mut stepper, again), Value::fixnum(9));
}

/// Site B refusal: the body-step poll refuses, the run unwinds to Idle with no
/// escape, and a fresh run is accepted.
#[test]
fn poll_refusal_at_back_edge_unwinds_and_restarts() {
    let mut ctx = context();
    let mut stepper = Stage1::new(
        TestSlots::empty(),
        RecordingEscape::new(),
        HarnessSealed,
        RecordingPoll::refusing_on(PollEdge::InterpBackEdge, RendezvousError::Capacity),
        EraId::new(3),
        ShapeGeneration::new(7),
    );
    let outcome = stepper.start(
        &mut ctx,
        body(&[Value::fixnum(1), Value::fixnum(2)]),
        wide(),
    );
    assert!(
        matches!(
            outcome,
            StepOutcome::Fault(EngineFault::Poll(RendezvousError::Capacity))
        ),
        "the body-step refusal is a Poll fault"
    );
    assert!(stepper.escape_handler().requests.is_empty());
    assert_clear(&stepper);
    let again = stepper.start(&mut ctx, body(&[Value::fixnum(8)]), wide());
    assert_eq!(done_value(&mut stepper, again), Value::fixnum(8));
}

#[test]
fn engine_reads_a_bound_plain_symbol() {
    let mut ctx = context();
    let sym = Value::symbol("pentry_plain_foo");
    let id = symbol_id(&sym, SymbolMode::Bare).expect("interned test symbol");
    let mut stepper = engine(TestSlots::bound_with_admission(
        Value::fixnum(5),
        PlainAdmission::mint(id, TlsIndex::VOID),
    ));
    let outcome = stepper.start(&mut ctx, body(&[sym]), wide());
    assert_eq!(done_value(&mut stepper, outcome), Value::fixnum(5));
    assert!(stepper.escape_handler().requests.is_empty());
    assert_eq!(
        stepper.slots().canonical_reads(),
        0,
        "an owner-bound read never touches canonical storage"
    );
    assert_clear(&stepper);
}

#[test]
fn engine_owner_void_symbol_yields_a_typed_void_variable_outcome() {
    let mut ctx = context();
    let sym = Value::symbol("pentry_plain_void");
    let id = symbol_id(&sym, SymbolMode::Bare).expect("interned test symbol");
    let mut stepper = engine(TestSlots::bound_with_admission(
        Value::UNBOUND,
        PlainAdmission::mint(id, TlsIndex::VOID),
    ));
    let outcome = stepper.start(&mut ctx, body(&[sym]), wide());
    match outcome {
        StepOutcome::VoidVariable { symbol } => assert_eq!(symbol, sym),
        _other => panic!("a local void Plain binding must be a typed void-variable outcome"),
    }
    assert!(
        stepper.escape_handler().requests.is_empty(),
        "a void variable is a signal, not an unsupported-form escape"
    );
    assert_clear(&stepper);
}

#[test]
fn engine_localized_symbol_escapes_once_and_touches_no_slot() {
    let mut ctx = context();
    let sym = Value::symbol("pentry_plain_localized");
    let mut stepper = engine(TestSlots::refused(PlainAdmission::refuse(
        PlainRefusal::NotPlain,
    )));
    let outcome = stepper.start(&mut ctx, body(&[sym]), wide());
    match outcome {
        StepOutcome::Escaped(request) => assert_eq!(request.class, EscapeClass::UnsupportedForm),
        _other => panic!("expected one escape"),
    }
    assert_eq!(stepper.escape_handler().requests.len(), 1);
    assert_eq!(
        stepper.slots().canonical_reads(),
        0,
        "a refused admission does not read canonical storage"
    );
    // An escape keeps the top frame (settle's Escaped arm), so the arena is
    // not empty here; assert_clear applies to Done/Fault outcomes only.
}
