//! Full-check work at the fold/cleanup boundary, without production counters.
//! Threading: each test owns its IR and captures only this compiler thread's
//! test-only verifier entries. No environment or Lisp state is retained.

use super::{CleanupValidation, run_with_validation};
use crate::emacs_core::bytecode::Op;
use crate::emacs_core::jit::compile::{CompileError, analyze_cfg};
use crate::emacs_core::jit::opt::{
    build::{BuildInput, build},
    ir::{Func, ParamShape, Term, ValueBits},
    native_verify_observer::{self, Calls, NativeRegion},
    verify::VerifyError,
};
use crate::emacs_core::value::Value as LispValue;

/// Compiler-test failure domains retain their source, rather than erasing it.
/// Threading: owned diagnostics without IR, runtime pointers or Lisp state.
#[derive(Debug, thiserror::Error)]
enum PlanError {
    #[error(transparent)]
    Compile(#[from] CompileError),
    #[error(transparent)]
    Verify(#[from] VerifyError),
}

fn source_plan() -> Result<Func, PlanError> {
    let ops = [
        Op::Constant(0),
        Op::Consp,
        Op::GotoIfNil(5),
        Op::Constant(1),
        Op::Return,
        Op::Constant(2),
        Op::Return,
    ];
    let constants = [
        LispValue::fixnum(7),
        LispValue::fixnum(11),
        LispValue::fixnum(17),
    ];
    let params = ParamShape::default();
    let cfg = analyze_cfg(&ops, &constants, None, params.native_arity())?;
    let bits = constants.map(ValueBits::from_value);
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

#[test]
fn opt_fold_fast_reuses_cleanup_validation_without_changing_plan() -> Result<(), PlanError> {
    let mut repeated = source_plan()?;
    let mut verified = repeated.clone();
    let (old_stats, old_calls) = native_verify_observer::capture(|| {
        let _region = NativeRegion::enter();
        run_with_validation(&mut repeated, CleanupValidation::RepeatCleanup)
    });
    let (fast_stats, fast_calls) = native_verify_observer::capture(|| {
        let _region = NativeRegion::enter();
        run_with_validation(&mut verified, CleanupValidation::ReuseCleanup)
    });
    assert_eq!(old_stats?, fast_stats?);
    assert_eq!(
        old_calls,
        Calls {
            ordinary: 3,
            arrays: 0,
            recipes: 0
        }
    );
    assert_eq!(
        fast_calls,
        Calls {
            ordinary: 2,
            arrays: 0,
            recipes: 0
        }
    );
    assert_eq!(
        repeated.display().to_string(),
        verified.display().to_string()
    );
    assert_eq!(repeated.frames, verified.frames);
    assert_eq!(repeated.entry_stacks, verified.entry_stacks);
    let source_states = |func: &Func| {
        func.source_states
            .iter()
            .map(|state| {
                state.as_ref().map(|state| {
                    (
                        state.block,
                        state.frame,
                        state.pre.clone(),
                        state.post.clone(),
                    )
                })
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(source_states(&repeated), source_states(&verified));
    assert!(
        verified
            .blocks
            .iter()
            .all(|block| !matches!(block.term, Term::Branch { .. }))
    );
    verified.verify()?;
    Ok(())
}

#[test]
fn opt_fold_fast_checks_malformed_input_before_cleanup() -> Result<(), PlanError> {
    let mut input = source_plan()?;
    input.blocks[input.entry.index()]
        .params
        .push(crate::emacs_core::jit::opt::ir::Value(u32::MAX));
    let (result, calls) = native_verify_observer::capture(|| {
        let _region = NativeRegion::enter();
        run_with_validation(&mut input, CleanupValidation::ReuseCleanup)
    });
    assert!(result.is_err());
    assert_eq!(
        calls,
        Calls {
            ordinary: 1,
            arrays: 0,
            recipes: 0
        }
    );
    Ok(())
}
