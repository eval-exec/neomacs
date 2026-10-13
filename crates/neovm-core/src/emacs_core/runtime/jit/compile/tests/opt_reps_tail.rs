//! Terminal Reps full-check work and exact source/IR preservation.
//!
//! Threading: each test owns its IR and captures only this compiler thread's
//! test-only checker entries. Scoped scalar policies restore their exact prior
//! values; no process environment, heap object or runtime pointer is retained.

use super::{build_plan, ir};
use crate::emacs_core::bytecode::Op;
use crate::emacs_core::jit::NumericFeedback;
use crate::emacs_core::jit::compile::{
    CompileError, analyze_cfg,
    knobs::{
        FastWorkSchedule, OptPasses, jit_opt_fast, jit_opt_passes, opt_fast_scope_for_test,
        opt_passes_scope_for_test,
    },
    opt_profile::{Profile, scope_for_test},
    publish_numeric_feedback_vec,
};
use crate::emacs_core::jit::opt::{
    build::{BuildInput, build},
    mem::{AliasClass, Effects},
    native_verify_observer::{self, Calls, NativeRegion},
    passes::{
        array_reads::{ArrayReadProof, BoundsWitness},
        reps,
    },
    sink_recipes::{OwnerRecipe, RecipeKind, RecipeOrigin, RecipeVersionId},
    sink_shape,
    types::TypeSet,
    verify::VerifyError,
};
use crate::emacs_core::value::Value as LispValue;

/// Test failures preserve the compiler/verifier error domains.
/// Threading: owned diagnostics, with no IR, runtime pointers or Lisp state.
#[derive(Debug, thiserror::Error)]
enum PlanError {
    #[error(transparent)]
    Compile(#[from] CompileError),
    #[error(transparent)]
    Verify(#[from] VerifyError),
    #[error("the test fixture is missing {0:?}")]
    MissingFixture(FixturePart),
    #[error("the malformed test fixture unexpectedly verifies")]
    MalformedFixtureVerified,
}

#[derive(Clone, Copy, Debug)]
enum FixturePart {
    FramedInstruction,
    SourceState,
    RepsCensus,
}

/// An operation index in the compiler's source stream, never a byte offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SourcePc(u32);
impl From<u32> for SourcePc {
    fn from(pc: u32) -> Self {
        Self(pc)
    }
}

#[derive(Debug, PartialEq, Eq)]
struct BlockObservation {
    params: Vec<ir::Value>,
    insts: Vec<ir::Inst>,
    term: ir::Term,
    preds: Vec<ir::Block>,
    pc: SourcePc,
    loop_header: Option<ir::LoopId>,
    cold: bool,
}
impl From<&ir::BlockData> for BlockObservation {
    fn from(data: &ir::BlockData) -> Self {
        Self {
            params: data.params.clone(),
            insts: data.insts.clone(),
            term: data.term.clone(),
            preds: data.preds.clone(),
            pc: data.pc.into(),
            loop_header: data.loop_header,
            cold: data.cold,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct InstructionObservation {
    op: ir::Opcode,
    args: Vec<ir::Value>,
    result: Option<ir::Value>,
    eff: Effects,
    mem: AliasClass,
    frame: Option<ir::FrameId>,
    pc: SourcePc,
}
impl From<&ir::InstData> for InstructionObservation {
    fn from(data: &ir::InstData) -> Self {
        Self {
            op: data.op.clone(),
            args: data.args.clone(),
            result: data.result,
            eff: data.eff,
            mem: data.mem,
            frame: data.frame,
            pc: data.pc.into(),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ValueObservation {
    ty: TypeSet,
    rep: ir::Rep,
    def: ir::ValueDef,
}
impl From<&ir::ValueData> for ValueObservation {
    fn from(data: &ir::ValueData) -> Self {
        Self {
            ty: data.ty,
            rep: data.rep,
            def: data.def,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
struct SourceObservation {
    pre: Box<[ir::Value]>,
    post: Box<[ir::Value]>,
    frame: ir::FrameId,
    block: ir::Block,
}
impl From<&ir::SourceState> for SourceObservation {
    fn from(state: &ir::SourceState) -> Self {
        Self {
            pre: state.pre.clone(),
            post: state.post.clone(),
            frame: state.frame,
            block: state.block,
        }
    }
}

/// Typed, deterministic projection; private frame-intern HashMap order is not
/// an observable IR difference. These fixtures have no Array/Sink sidecars.
#[derive(Debug, PartialEq, Eq)]
struct PlanObservation {
    blocks: Vec<BlockObservation>,
    insts: Vec<InstructionObservation>,
    values: Vec<ValueObservation>,
    frames: Vec<ir::FrameState>,
    entry: ir::Block,
    osr: Option<ir::OsrEntry>,
    consts: Box<[ir::ValueBits]>,
    dynamic_prefix: usize,
    arity: ir::ParamShape,
    census: ir::OptCensus,
    source_states: Vec<Option<SourceObservation>>,
    entry_stacks: Vec<Box<[ir::Value]>>,
}
impl From<&ir::Func> for PlanObservation {
    fn from(func: &ir::Func) -> Self {
        Self {
            blocks: func.blocks.iter().map(BlockObservation::from).collect(),
            insts: func
                .insts
                .iter()
                .map(InstructionObservation::from)
                .collect(),
            values: func.values.iter().map(ValueObservation::from).collect(),
            frames: func.frames.clone(),
            entry: func.entry,
            osr: func.osr.clone(),
            consts: func.consts.clone(),
            dynamic_prefix: func.dynamic_prefix,
            arity: func.arity,
            census: func.census.clone(),
            source_states: func
                .source_states
                .iter()
                .map(|state| state.as_ref().map(SourceObservation::from))
                .collect(),
            entry_stacks: func.entry_stacks.clone(),
        }
    }
}

fn constant_pool() -> [LispValue; 1] {
    [LispValue::fixnum(7)]
}

/// Analyze outside the observer so the count covers only the actual plan
/// builder, including every selected pass and the final backend check.
fn captured_plan(
    ops: &[Op],
    passes: OptPasses,
    work: FastWorkSchedule,
) -> Result<(ir::Func, Calls), PlanError> {
    let _profile = scope_for_test(Profile::Lists48Osr);
    let _passes = opt_passes_scope_for_test(passes);
    let _work = opt_fast_scope_for_test(work);
    let _feedback = publish_numeric_feedback_vec(vec![NumericFeedback::FixnumOnly; ops.len()]);
    let constants = constant_pool();
    let params = ir::ParamShape::default();
    let cfg = analyze_cfg(ops, &constants, None, params.native_arity())?;
    let (plan, calls) = native_verify_observer::capture(|| {
        let _region = NativeRegion::enter();
        build_plan(ops, &constants, &cfg, params, 0, None)
    });
    Ok((plan?, calls))
}

fn assert_no_sidecars(func: &ir::Func) {
    assert!(func.array_reads.reads.is_empty());
    assert!(!sink_shape::has_metadata(func));
}

fn source_plan() -> Result<ir::Func, PlanError> {
    let ops = [Op::Constant(0), Op::Add1, Op::Return];
    let constants = constant_pool();
    let params = ir::ParamShape::default();
    let cfg = analyze_cfg(&ops, &constants, None, params.native_arity())?;
    let bits = constants.map(ir::ValueBits::from_value);
    Ok(build(BuildInput {
        ops: &ops,
        constants: &bits,
        cfg: &cfg,
        params,
        dynamic_prefix: 0,
        fused: None,
        osr: None,
    })?)
}

#[derive(Clone, Copy, Debug)]
enum MalformedInput {
    Entry,
    Frame,
    Source,
    Array,
    Sink,
}
impl MalformedInput {
    fn apply(self, func: &mut ir::Func) -> Result<(), PlanError> {
        match self {
            Self::Entry => func.entry = ir::Block(u32::MAX),
            Self::Frame => {
                let inst = func
                    .insts
                    .iter_mut()
                    .find(|inst| inst.frame.is_some())
                    .ok_or(PlanError::MissingFixture(FixturePart::FramedInstruction))?;
                inst.frame = Some(ir::FrameId(u32::MAX));
            }
            Self::Source => {
                let state = func
                    .source_states
                    .iter_mut()
                    .flatten()
                    .next()
                    .ok_or(PlanError::MissingFixture(FixturePart::SourceState))?;
                state.frame = ir::FrameId(u32::MAX);
            }
            Self::Array => {
                func.array_reads.reads.insert(
                    ir::Inst(u32::MAX),
                    ArrayReadProof {
                        guarded_base: ir::Value(0),
                        checked_index: ir::Value(0),
                        length_read: ir::Inst(0),
                        length: ir::Value(0),
                        bounds: ir::Inst(0),
                        bounds_result: ir::Value(0),
                        frame: ir::FrameId(0),
                        pc: 1,
                        witness: BoundsWitness::Checked,
                    },
                );
            }
            Self::Sink => {
                let owner = ir::Value(u32::MAX);
                func.sink_recipes.owners.insert(
                    owner,
                    OwnerRecipe {
                        owner,
                        kind: RecipeKind::Cons,
                        semantic_type: TypeSet::CONS,
                        origin: RecipeOrigin::Borrow {
                            inst: ir::Inst(0),
                            original: ir::Value(0),
                        },
                        definition_version: RecipeVersionId(0),
                    },
                );
            }
        }
        Ok(())
    }

    fn expected_calls(self) -> Calls {
        match self {
            Self::Entry | Self::Frame | Self::Source => Calls {
                ordinary: 1,
                arrays: 0,
                recipes: 0,
            },
            Self::Array => Calls {
                ordinary: 1,
                arrays: 1,
                recipes: 0,
            },
            Self::Sink => Calls {
                ordinary: 1,
                arrays: 0,
                recipes: 1,
            },
        }
    }
}

#[test]
fn opt_reps_tail_fast_reuses_complete_validation_without_changing_plan() -> Result<(), PlanError> {
    let ops = [Op::Constant(0), Op::Add1, Op::Return];
    let passes = OptPasses {
        reps: true,
        ..OptPasses::default()
    };
    let (repeated, repeated_calls) = captured_plan(&ops, passes, FastWorkSchedule::RepeatChecks)?;
    let (reused, reused_calls) = captured_plan(&ops, passes, FastWorkSchedule::ReuseChecks)?;
    assert_eq!(
        repeated_calls,
        Calls {
            ordinary: 5,
            arrays: 0,
            recipes: 0
        }
    );
    // RED on the old backend: it repeats the successful candidate check (5).
    assert_eq!(
        reused_calls,
        Calls {
            ordinary: 4,
            arrays: 0,
            recipes: 0
        }
    );
    assert_no_sidecars(&repeated);
    assert_no_sidecars(&reused);
    assert_eq!(
        PlanObservation::from(&repeated),
        PlanObservation::from(&reused)
    );
    let census = reused
        .census
        .reps
        .as_ref()
        .ok_or(PlanError::MissingFixture(FixturePart::RepsCensus))?;
    assert_eq!(census.lift.lifted_arithmetic, 1);
    // A single result returned to Lisp keeps its tagged-fix view: the raw
    // web cannot pay the return conversion. The lifted arithmetic is real.
    assert_eq!(census.selection.tagged_arithmetic, 1);
    assert_eq!(census.selection.raw_arithmetic, 0);
    reused.verify()?;
    Ok(())
}

#[test]
fn opt_reps_tail_reps_off_retains_backend_full_check() -> Result<(), PlanError> {
    let ops = [Op::Constant(0), Op::Add1, Op::Return];
    for work in [
        FastWorkSchedule::RepeatChecks,
        FastWorkSchedule::ReuseChecks,
    ] {
        let (plan, calls) = captured_plan(&ops, OptPasses::default(), work)?;
        assert_eq!(
            calls,
            Calls {
                ordinary: 1,
                arrays: 0,
                recipes: 0
            }
        );
        assert!(plan.census.reps.is_none());
        assert_no_sidecars(&plan);
        plan.verify()?;
    }
    Ok(())
}

#[test]
fn opt_reps_tail_sink_selected_no_change_retains_original_schedule() -> Result<(), PlanError> {
    let ops = [Op::Constant(0), Op::Return];
    let passes = OptPasses {
        reps: true,
        sink: true,
        ..OptPasses::default()
    };
    let (repeated, repeated_calls) = captured_plan(&ops, passes, FastWorkSchedule::RepeatChecks)?;
    let (fast, fast_calls) = captured_plan(&ops, passes, FastWorkSchedule::ReuseChecks)?;
    // FAST-off Sink still clones and checks its unchanged candidate.
    assert_eq!(
        repeated_calls,
        Calls {
            ordinary: 6,
            arrays: 0,
            recipes: 0
        }
    );
    // FAST-on Sink discovers no work, but the backend full check stays.
    assert_eq!(
        fast_calls,
        Calls {
            ordinary: 5,
            arrays: 0,
            recipes: 0
        }
    );
    assert_no_sidecars(&repeated);
    assert_no_sidecars(&fast);
    assert_eq!(
        PlanObservation::from(&repeated),
        PlanObservation::from(&fast)
    );
    assert_eq!(fast.census.sink, Some(Default::default()));
    fast.verify()?;
    Ok(())
}

#[test]
fn opt_reps_tail_original_reps_rejects_malformed_input_before_analysis() -> Result<(), PlanError> {
    for malformed in [
        MalformedInput::Entry,
        MalformedInput::Frame,
        MalformedInput::Source,
        MalformedInput::Array,
        MalformedInput::Sink,
    ] {
        let mut input = source_plan()?;
        malformed.apply(&mut input)?;
        let expected = input
            .verify()
            .err()
            .ok_or(PlanError::MalformedFixtureVerified)?;
        let before = PlanObservation::from(&input);
        let (result, calls) = native_verify_observer::capture(|| {
            let _region = NativeRegion::enter();
            reps::run(&mut input)
        });
        assert_eq!(result, Err(expected));
        assert_eq!(calls, malformed.expected_calls());
        assert_eq!(PlanObservation::from(&input), before);
    }
    Ok(())
}

#[test]
fn opt_reps_tail_test_policies_restore_enclosing_thread_scopes() {
    let _profile = scope_for_test(Profile::Lists48Osr);
    assert!(jit_opt_fast());
    let selected = OptPasses {
        reps: true,
        ..OptPasses::default()
    };
    {
        let _passes = opt_passes_scope_for_test(selected);
        let _work = opt_fast_scope_for_test(FastWorkSchedule::RepeatChecks);
        assert_eq!(jit_opt_passes(), selected);
        assert!(!jit_opt_fast());
        {
            let _inner_passes = opt_passes_scope_for_test(OptPasses::default());
            let _inner_work = opt_fast_scope_for_test(FastWorkSchedule::ReuseChecks);
            assert_eq!(jit_opt_passes(), OptPasses::default());
            assert!(jit_opt_fast());
        }
        assert_eq!(jit_opt_passes(), selected);
        assert!(!jit_opt_fast());
    }
    assert_eq!(jit_opt_passes(), Profile::Lists48Osr.defaults().passes);
    assert!(jit_opt_fast());
}

#[test]
fn opt_reps_tail_owned_terminal_rejects_malformed_input_before_analysis() -> Result<(), PlanError> {
    for malformed in [
        MalformedInput::Entry,
        MalformedInput::Frame,
        MalformedInput::Source,
        MalformedInput::Array,
        MalformedInput::Sink,
    ] {
        let mut input = source_plan()?;
        malformed.apply(&mut input)?;
        let expected = input
            .verify()
            .err()
            .ok_or(PlanError::MalformedFixtureVerified)?;
        let (result, calls) = native_verify_observer::capture(|| {
            let _region = NativeRegion::enter();
            reps::run_terminal(input)
        });
        assert_eq!(result.err(), Some(expected));
        assert_eq!(calls, malformed.expected_calls());
    }
    Ok(())
}
