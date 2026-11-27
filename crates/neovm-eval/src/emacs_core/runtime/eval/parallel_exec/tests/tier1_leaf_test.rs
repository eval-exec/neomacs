//! E7-P.1 `VmExec` Tier-1 bounded leaf kernel entry differential tests against
//! the serial bytecode VM (including non-immediate, retired, debug-on-next-call,
//! and `--no-default-features` fallbacks).
use super::super::owner_bindings::{
    Channel, OwnerBindingRecord, OwnerRecordId, SlotState, UndoTarget,
};
use super::effective::{
    EraId, OwnerSlots, OwnerSlotsMut, PlainAccess, PlainAccessError, PlainAdmission, PlainRefusal,
    ShapeGeneration,
};
use super::engine::CoreOutcome;
use super::frames::FrameArena;
use super::vm::{
    ArithOps, ArithRefusal, CallClass, CallEnv, CallEscape, CodeRefusal, EscapeCompletion,
    NumericClass, SymbolCallLookup, UnwindCompletion, VmCode, VmEntry, VmEscape, VmEscapeRequest,
    VmExec, VmOutcome, VmPlain, VmPoll, VmProgram, VmSealed, classify_call_with_symbols,
    classify_tier1_scalar_leaf, program_from_bytecode,
};
use crate::emacs_core::bytecode::arith_kind::ArithGenericKind;
use crate::emacs_core::bytecode::value_api::TaggedValueBytecodeExt;
use crate::emacs_core::bytecode::{ByteCodeFunction, ByteCodeFunctionExt, Op, StackDepth};
use crate::emacs_core::eval::Context;
use crate::emacs_core::eval::world::poll::PollOutcome as VmPollOutcome;
use crate::emacs_core::eval::world::{ParallelHeapEraWorldExt, WorldEra};
use crate::emacs_core::jit::parallel::{NmjitCodeRegistry, NmjitLeafId, ScalarProgram};
use crate::test_support::execution_fixtures::ExecutionTestHomes;
use neovm_value::forward::TlsIndex;
use neovm_value::intern::{SymId, intern};
use neovm_value::tagged::gc::{
    Gen1Admission, ImmediateResult, OwnerRootSlot, ParallelExecutionAdmission, PlainSymbolBacking,
    RendezvousError, RunAccess,
};
use neovm_value::tagged::value::TaggedValue;
use neovm_value::test_support::authority::{nil_root_for_test, take_legacy_roots_for_test};
use neovm_value::value::{LambdaParams, Value};
use std::cell::RefCell;
use std::collections::HashMap;

struct Gen1ModePin {
    previous: Option<std::ffi::OsString>,
}

impl Gen1ModePin {
    fn new() -> Self {
        let previous = std::env::var_os("NEOVM_GC_GENERATIONAL");
        // SAFETY: lib-test executes one test per process; restored on drop.
        unsafe { std::env::set_var("NEOVM_GC_GENERATIONAL", "1") };
        Self { previous }
    }
}

impl Drop for Gen1ModePin {
    fn drop(&mut self) {
        // SAFETY: see `new`.
        unsafe {
            match self.previous.take() {
                Some(value) => std::env::set_var("NEOVM_GC_GENERATIONAL", value),
                None => std::env::remove_var("NEOVM_GC_GENERATIONAL"),
            }
        }
    }
}

struct NoEscape;
impl VmEscape for NoEscape {
    fn escape(
        &mut self,
        request: VmEscapeRequest,
        _arena: &FrameArena,
        _plain: &mut dyn PlainAccess,
    ) -> EscapeCompletion {
        EscapeCompletion::fall_through(&request.op)
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
}

struct NoSealed;
impl VmSealed for NoSealed {
    fn read_cons_field(&mut self, _: &mut Context, _: &'static str, _: &[Value]) -> CoreOutcome {
        CoreOutcome::MissingCore
    }
    fn write_cons_field(&mut self, _: &mut Context, _: &'static str, _: &[Value]) -> CoreOutcome {
        CoreOutcome::MissingCore
    }
    fn allocate_cons(&mut self, _: &mut Context, _: &'static str, _: &[Value]) -> CoreOutcome {
        CoreOutcome::MissingCore
    }
}

struct SimpleArith;
impl ArithOps for SimpleArith {
    fn arith(&mut self, kind: ArithGenericKind, operands: &[Value]) -> Result<Value, ArithRefusal> {
        match (kind, operands) {
            (ArithGenericKind::Add1, [v]) if v.is_fixnum() => {
                Ok(Value::fixnum(v.as_fixnum().unwrap() + 1))
            }
            _ => Err(ArithRefusal::MarkerInvolved),
        }
    }
}

struct NoPlain;
impl OwnerSlots for NoPlain {
    fn slot_state(&self, _slot: TlsIndex, _channel: Channel) -> SlotState {
        unreachable!("NoPlain does not access slots")
    }
}
impl OwnerSlotsMut for NoPlain {
    fn set_slot_state(&mut self, _slot: TlsIndex, _channel: Channel, _state: SlotState) {
        unreachable!("NoPlain does not mutate slots")
    }
}
impl PlainAccess for NoPlain {
    fn admission(&self, _sym: SymId) -> PlainAdmission {
        PlainAdmission::Refuse(PlainRefusal::NotPlain)
    }
    fn canonical_read(&mut self, _sym: SymId) -> Result<Value, PlainAccessError> {
        Ok(Value::NIL)
    }
    fn canonical_write(&mut self, _sym: SymId, _value: Value) -> Result<(), PlainAccessError> {
        Ok(())
    }
    fn canonical_bound(&mut self, _sym: SymId) -> Result<bool, PlainAccessError> {
        Ok(false)
    }
    fn push_binding(
        &mut self,
        _sym: SymId,
        _slot: TlsIndex,
        _channel: Channel,
        _undo: UndoTarget,
    ) -> OwnerRecordId {
        unreachable!("NoPlain does not bind")
    }
    fn pop_binding(&mut self, _id: OwnerRecordId) -> Option<OwnerBindingRecord> {
        None
    }
    fn unbind_needs_serial(&self, _sym: SymId) -> bool {
        false
    }
    fn record_live(&self, _id: OwnerRecordId) -> bool {
        false
    }
    fn top_record(&self) -> Option<OwnerRecordId> {
        None
    }
}
impl VmPlain for NoPlain {
    fn generation(&self) -> (EraId, ShapeGeneration) {
        (EraId::new(1), ShapeGeneration::new(1))
    }
}

struct NoopPoll;
impl VmPoll for NoopPoll {
    fn vm_back_edge(&mut self, _: &FrameArena) -> Result<VmPollOutcome, RendezvousError> {
        Ok(VmPollOutcome::Continue)
    }
    fn vm_call(&mut self, _: &FrameArena) -> Result<VmPollOutcome, RendezvousError> {
        Ok(VmPollOutcome::Continue)
    }
    fn serial_request(&mut self, _: &FrameArena) -> Result<VmPollOutcome, RendezvousError> {
        Ok(VmPollOutcome::Continue)
    }
    fn vm_refill(
        &mut self,
        _: &FrameArena,
        _: NumericClass,
    ) -> Result<VmPollOutcome, RendezvousError> {
        Ok(VmPollOutcome::Continue)
    }
}

struct Tier1LeafVmCode<'a, 'era, 'owner> {
    entry: VmProgram<'a>,
    registry: &'a NmjitCodeRegistry,
    admission: &'a ParallelExecutionAdmission<'era>,
    run: RefCell<&'a mut RunAccess<'owner, 'era>>,
    destination: &'a OwnerRootSlot<'era>,
    leaves: HashMap<usize, (NmjitLeafId, ScalarProgram)>,
}

impl<'a, 'era, 'owner> VmCode for Tier1LeafVmCode<'a, 'era, 'owner> {
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

    fn try_tier1_leaf(&self, code: Value, args: &[Value]) -> Option<Value> {
        let &(leaf_id, plan) = self.leaves.get(&code.bits())?;
        let input = match plan {
            ScalarProgram::Constant(_) => ImmediateResult::Nil,
            ScalarProgram::Argument => {
                let arg = *args.first()?;
                ImmediateResult::from_word(arg.bits()).ok()?
            }
            ScalarProgram::HasSlotBacking(_) => return None,
        };
        let mut run = self.run.borrow_mut();
        match self
            .registry
            .run_scalar(leaf_id, self.admission, &mut run, self.destination, input)
        {
            Ok(()) => {
                let out = ExecutionTestHomes::read(self.destination, &run);
                // SAFETY: `ImmediateResult::word()` is a validated immediate (`nil`, `t`, or fixnum).
                Some(unsafe { Value::from_bits(out.word()) })
            }
            Err(_) => None,
        }
    }
}

fn make_leaf_bc(required: usize, ops: Vec<Op>, constants: Vec<Value>) -> Value {
    let req: Vec<SymId> = (0..required)
        .map(|i| intern(&format!("e7p1-req-{i}")))
        .collect();
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: req,
        optional: Vec::new(),
        rest: None,
    });
    f.set_lexical(true);
    f.set_ops(ops);
    f.set_constants(constants.into());
    f.set_max_stack(StackDepth::for_test(4));
    Value::make_bytecode(f)
}

#[allow(clippy::too_many_arguments)]
fn check_tier1_call<'era, 'owner>(
    serial_ctx: &mut Context,
    registry: &NmjitCodeRegistry,
    admission: &ParallelExecutionAdmission<'era>,
    run_ref: &mut RunAccess<'owner, 'era>,
    dst: &OwnerRootSlot<'era>,
    compiled_leaves: &HashMap<usize, (NmjitLeafId, ScalarProgram)>,
    ops: Vec<Op>,
    constants: Vec<Value>,
    expected_hit: bool,
) {
    let era_tag = ();
    let caller_bc = make_leaf_bc(0, ops.clone(), constants.clone());
    let serial_res = serial_ctx
        .apply(caller_bc, Vec::new())
        .expect("serial VM execute");

    let entry_prog = VmProgram {
        function: Value::from_sym_id(intern("e7p1-caller")),
        code: caller_bc,
        ops: &ops,
        constants: &constants,
    };
    let code = Tier1LeafVmCode {
        entry: entry_prog,
        registry,
        admission,
        run: RefCell::new(run_ref),
        destination: dst,
        leaves: compiled_leaves.clone(),
    };
    let mut exec = VmExec::new(&era_tag, NoEscape, NoSealed, SimpleArith, NoPlain, NoopPoll);
    let outcome = exec.run_with_code(
        serial_ctx,
        &code,
        VmEntry {
            function: entry_prog.function,
            code: caller_bc,
            frame_bytes: 0,
        },
    );
    let VmOutcome::Done { pending } = outcome else {
        panic!("VmExec expected Done, got {outcome:?}");
    };
    let parallel_res = exec.take(pending).unwrap();
    assert_eq!(parallel_res.bits(), serial_res.bits());
    if cfg!(feature = "jit") && expected_hit {
        assert_eq!(exec.tier1_leaf_hits(), 1);
    } else {
        assert_eq!(exec.tier1_leaf_hits(), 0);
    }
}

#[test]
fn e7p1_vm_exec_tier1_leaf_matches_serial_vm_and_falls_back_cleanly() {
    let _harness = crate::test_utils::unit_harness();
    let _gen1 = Gen1ModePin::new();

    let mut serial_ctx = Context::new(crate::test_utils::unit_runtime());
    let immediate_samples = [
        Value::NIL,
        Value::T,
        Value::fixnum(0),
        Value::fixnum(73),
        Value::fixnum(-97),
        Value::fixnum(TaggedValue::MOST_NEGATIVE_FIXNUM),
        Value::fixnum(TaggedValue::MOST_POSITIVE_FIXNUM),
    ];

    let const_leaves: Vec<(Value, Value)> = immediate_samples
        .iter()
        .map(|&v| {
            let ops = if v.is_nil() {
                vec![Op::Nil, Op::Return]
            } else if v.is_t() {
                vec![Op::True, Op::Return]
            } else {
                vec![Op::Constant(0), Op::Return]
            };
            let consts = if v.is_nil() || v.is_t() {
                vec![]
            } else {
                vec![v]
            };
            (v, make_leaf_bc(0, ops, consts))
        })
        .collect();
    let id_leaf = make_leaf_bc(1, vec![Op::StackRef(0), Op::Return], vec![]);
    let id_leaf_return_only = make_leaf_bc(1, vec![Op::Return], vec![]);
    let add1_non_leaf = make_leaf_bc(1, vec![Op::StackRef(0), Op::Add1, Op::Return], vec![]);

    for (expected, bc_val) in &const_leaves {
        let bc = bc_val.bytecode_data_if_materialized().unwrap();
        assert_eq!(
            classify_tier1_scalar_leaf(bc),
            Some(ScalarProgram::Constant(
                ImmediateResult::from_word(expected.bits()).unwrap()
            ))
        );
    }
    assert_eq!(
        classify_tier1_scalar_leaf(id_leaf.bytecode_data_if_materialized().unwrap()),
        Some(ScalarProgram::Argument)
    );
    assert_eq!(
        classify_tier1_scalar_leaf(id_leaf_return_only.bytecode_data_if_materialized().unwrap()),
        Some(ScalarProgram::Argument)
    );
    assert_eq!(
        classify_tier1_scalar_leaf(add1_non_leaf.bytecode_data_if_materialized().unwrap()),
        None
    );

    let cons_arg = Value::cons(Value::fixnum(1), Value::fixnum(2));

    Context::try_with_test_symbol_backing_init_era(
        PlainSymbolBacking::Unprojected,
        |_, _| {},
        |mut proof, _| {
            let frozen = take_legacy_roots_for_test(&mut proof).unwrap();
            let (collector, keeper) = proof.finish().unwrap();
            let admitted = Gen1Admission::check(collector).unwrap();
            let registry = NmjitCodeRegistry::new(&admitted);

            let mut compiled_leaves: HashMap<usize, (NmjitLeafId, ScalarProgram)> = HashMap::new();
            for (_, bc_val) in &const_leaves {
                let plan =
                    classify_tier1_scalar_leaf(bc_val.bytecode_data_if_materialized().unwrap())
                        .unwrap();
                if let Ok(id) = registry.compile_scalar(&admitted, plan) {
                    compiled_leaves.insert(bc_val.bits(), (id, plan));
                }
            }
            let id_leaf_id = registry
                .compile_scalar(&admitted, ScalarProgram::Argument)
                .ok();
            if let Some(lid) = id_leaf_id {
                compiled_leaves.insert(id_leaf.bits(), (lid, ScalarProgram::Argument));
                compiled_leaves.insert(id_leaf_return_only.bits(), (lid, ScalarProgram::Argument));
            }

            let era = WorldEra::admit_rooted(admitted, keeper, frozen).unwrap();
            let admission = era.execution_admission();
            let mut owner = era
                .register(1, 4)
                .unwrap()
                .activate(era.heap_identity(), 1)
                .unwrap();

            {
                let mut run = owner.run_access().unwrap();
                let dst = nil_root_for_test(&mut run).unwrap();
                let era_tag = ();

                for (_, bc_val) in &const_leaves {
                    check_tier1_call(
                        &mut serial_ctx,
                        &registry,
                        &admission,
                        &mut run,
                        &dst,
                        &compiled_leaves,
                        vec![Op::Constant(0), Op::Call(0), Op::Return],
                        vec![*bc_val],
                        true,
                    );
                }
                for &sample in &immediate_samples {
                    check_tier1_call(
                        &mut serial_ctx,
                        &registry,
                        &admission,
                        &mut run,
                        &dst,
                        &compiled_leaves,
                        vec![Op::Constant(0), Op::Constant(1), Op::Call(1), Op::Return],
                        vec![id_leaf, sample],
                        true,
                    );
                }
                check_tier1_call(
                    &mut serial_ctx,
                    &registry,
                    &admission,
                    &mut run,
                    &dst,
                    &compiled_leaves,
                    vec![Op::Constant(0), Op::Constant(1), Op::Call(1), Op::Return],
                    vec![id_leaf, cons_arg],
                    false,
                );
                check_tier1_call(
                    &mut serial_ctx,
                    &registry,
                    &admission,
                    &mut run,
                    &dst,
                    &compiled_leaves,
                    vec![Op::Constant(0), Op::Constant(1), Op::Call(1), Op::Return],
                    vec![add1_non_leaf, Value::fixnum(41)],
                    false,
                );
                if let Some(lid) = id_leaf_id {
                    registry.retire(lid).unwrap();
                    check_tier1_call(
                        &mut serial_ctx,
                        &registry,
                        &admission,
                        &mut run,
                        &dst,
                        &compiled_leaves,
                        vec![Op::Constant(0), Op::Constant(1), Op::Call(1), Op::Return],
                        vec![id_leaf, Value::fixnum(73)],
                        false,
                    );
                }
                {
                    let ops = vec![Op::Constant(0), Op::Call(0), Op::Return];
                    let constants = vec![const_leaves[0].1];
                    let caller_bc = make_leaf_bc(0, ops.clone(), constants.clone());
                    let entry_prog = VmProgram {
                        function: Value::from_sym_id(intern("e7p1-debug-caller")),
                        code: caller_bc,
                        ops: &ops,
                        constants: &constants,
                    };
                    let code = Tier1LeafVmCode {
                        entry: entry_prog,
                        registry: &registry,
                        admission: &admission,
                        run: RefCell::new(&mut run),
                        destination: &dst,
                        leaves: compiled_leaves.clone(),
                    };
                    let mut exec =
                        VmExec::new(&era_tag, NoEscape, NoSealed, SimpleArith, NoPlain, NoopPoll);
                    exec.set_call_env(CallEnv {
                        debug_on_next_call: true,
                        ..CallEnv::default()
                    });
                    let _ = exec.run_with_code(
                        &mut serial_ctx,
                        &code,
                        VmEntry {
                            function: entry_prog.function,
                            code: caller_bc,
                            frame_bytes: 0,
                        },
                    );
                    assert_eq!(exec.tier1_leaf_hits(), 0);
                    assert_eq!(exec.call_escapes(), &[CallEscape::DebugOnNextCall]);
                }

                run.clear_root_slots().unwrap();
            }

            owner.retire_rooted(&[]).unwrap();
            drop(admission);
            let mut stopped = era.stop().unwrap();
            let mut receipt = stopped.mint_grace_receipt().unwrap();
            let _ = registry.reclaim_with_grace(&mut receipt).unwrap();
            drop(receipt);
            stopped.trace(|_, _| Ok(())).unwrap();
            stopped.complete().unwrap();
            let (collector, keeper) = era.finish().unwrap();
            drop(collector);
            drop(keeper);
            drop(registry);
        },
    )
    .unwrap();
}
