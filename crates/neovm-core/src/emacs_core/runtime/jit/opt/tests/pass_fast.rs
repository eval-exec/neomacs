//! FAST parity includes full observation stacks and independently verified
//! identity recipes. Flags and plans are invocation-owned; no env/TLS mutation.

use crate::emacs_core::bytecode::Op;
use crate::emacs_core::jit::opt::{
    build::{BuildInput, build},
    ir::*,
    passes::{bools, licm, sink},
};
use crate::emacs_core::jit::{NumericFeedback, compile::analyze_cfg};
use crate::emacs_core::value::Value as LispValue;

fn plan(ops: &[Op], constants: &[LispValue], required: usize) -> Func {
    let params = ParamShape {
        required,
        ..Default::default()
    };
    let cfg = analyze_cfg(ops, constants, None, required).unwrap();
    let constants = constants
        .iter()
        .copied()
        .map(ValueBits::from_value)
        .collect::<Vec<_>>();
    let func = build(BuildInput {
        ops,
        constants: &constants,
        cfg: &cfg,
        params,
        dynamic_prefix: 0,
        fused: None,
        osr: None,
    })
    .unwrap();
    func.verify().unwrap();
    func
}

fn observations(func: &Func) -> Vec<Option<(Block, Box<[Value]>, Box<[Value]>, FrameId)>> {
    func.source_states
        .iter()
        .map(|source| {
            source.as_ref().map(|source| {
                (
                    source.block,
                    source.pre.clone(),
                    source.post.clone(),
                    source.frame,
                )
            })
        })
        .collect()
}

fn parity(old: &Func, fast: &Func) {
    old.verify().unwrap();
    fast.verify().unwrap();
    assert_eq!(old.display().to_string(), fast.display().to_string());
    assert_eq!(format!("{:?}", old.values), format!("{:?}", fast.values));
    assert_eq!(format!("{:?}", old.insts), format!("{:?}", fast.insts));
    assert_eq!(format!("{:?}", old.census), format!("{:?}", fast.census));
    assert_eq!(old.frames, fast.frames);
    assert_eq!(old.entry_stacks, fast.entry_stacks);
    assert_eq!(observations(old), observations(fast));
    assert_eq!(old.sink_recipes.uses, fast.sink_recipes.uses);
    assert_eq!(
        old.sink_recipes.source_sqrt_sites,
        fast.sink_recipes.source_sqrt_sites
    );
    assert_eq!(
        old.sink_recipes.owners.len(),
        fast.sink_recipes.owners.len()
    );
    for (owner, recipe) in &old.sink_recipes.owners {
        assert_eq!(
            format!("{recipe:?}"),
            format!("{:?}", fast.sink_recipes.owners[owner])
        );
    }
    assert_eq!(
        format!("{:?}", old.sink_recipes.versions),
        format!("{:?}", fast.sink_recipes.versions)
    );
    assert_eq!(
        format!("{:?}", old.sink_recipes.edges),
        format!("{:?}", fast.sink_recipes.edges)
    );
    assert_eq!(
        old.sink_recipes.frames.len(),
        fast.sink_recipes.frames.len()
    );
    for (key, frame) in &old.sink_recipes.frames {
        assert_eq!(frame.versions, fast.sink_recipes.frames[key].versions);
    }
    assert!(old.array_reads.reads.is_empty() && fast.array_reads.reads.is_empty());
}

#[test]
fn opt_fast_bool_empty_web_preserves_source_and_census() {
    let mut old = plan(&[Op::Constant(0), Op::Return], &[LispValue::fixnum(7)], 0);
    let mut fast = old.clone();
    let old_stats = bools::run_fast_for_test(&mut old, false).unwrap();
    let fast_stats = bools::run_fast_for_test(&mut fast, true).unwrap();
    assert_eq!(old_stats, fast_stats);
    assert_eq!(fast_stats, Default::default());
    parity(&old, &fast);
}

#[test]
fn opt_fast_bool_selected_web_keeps_original_lisp_return_view() {
    let mut old = plan(&[Op::Constant(0), Op::Return], &[LispValue::T], 0);
    let mut fast = old.clone();
    let old_stats = bools::run_fast_for_test(&mut old, false).unwrap();
    let fast_stats = bools::run_fast_for_test(&mut fast, true).unwrap();
    assert_eq!(old_stats, fast_stats);
    assert_eq!(fast_stats.constant_producers, 1);
    assert_eq!(fast_stats.tagged_views, 1);
    parity(&old, &fast);
}

#[test]
fn opt_fast_licm_empty_loops_preserves_observations() {
    let mut old = plan(&[Op::Constant(0), Op::Return], &[LispValue::fixnum(7)], 0);
    let mut fast = old.clone();
    assert_eq!(
        licm::run_fast_for_test(&mut old, false),
        licm::run_fast_for_test(&mut fast, true)
    );
    parity(&old, &fast);
}

#[test]
fn opt_fast_licm_selected_motion_keeps_same_preheader_and_replay() {
    let ops = [
        Op::Constant(0),
        Op::StackRef(0),
        Op::StackRef(2),
        Op::Lss,
        Op::GotoIfNil(11),
        Op::StackRef(0),
        Op::Constant(1),
        Op::Add,
        Op::StackSet(1),
        Op::Goto(1),
        Op::Return,
        Op::StackRef(0),
        Op::Return,
    ];
    let mut old = plan(&ops, &[LispValue::fixnum(0), LispValue::fixnum(1)], 1);
    let mut fast = old.clone();
    let old_stats = licm::run_fast_for_test(&mut old, false).unwrap();
    let fast_stats = licm::run_fast_for_test(&mut fast, true).unwrap();
    assert_eq!(old_stats, fast_stats);
    assert!(fast_stats.pure_hoisted > 0);
    parity(&old, &fast);
}

#[test]
fn opt_fast_sink_empty_selection_preserves_all_source_identities() {
    let mut old = plan(&[Op::Constant(0), Op::Return], &[LispValue::fixnum(7)], 0);
    let mut fast = old.clone();
    let feedback = [NumericFeedback::Float; 2];
    assert_eq!(
        sink::run_fast_for_test(&mut old, &feedback, false),
        sink::run_fast_for_test(&mut fast, &feedback, true)
    );
    parity(&old, &fast);
}

#[test]
fn opt_fast_sink_selected_numeric_and_cons_recipes_preserve_aliases() {
    for numeric in [true, false] {
        let ops = if numeric {
            vec![
                Op::StackRef(0),
                Op::Constant(0),
                Op::Mul,
                Op::Dup,
                Op::Eq,
                Op::Return,
            ]
        } else {
            vec![
                Op::Constant(0),
                Op::Constant(0),
                Op::Cons,
                Op::Dup,
                Op::Eq,
                Op::Return,
            ]
        };
        let mut old = plan(&ops, &[LispValue::fixnum(2)], usize::from(numeric));
        let mut fast = old.clone();
        let feedback = vec![NumericFeedback::Float; ops.len()];
        let old_stats = sink::run_fast_for_test(&mut old, &feedback, false).unwrap();
        let fast_stats = sink::run_fast_for_test(&mut fast, &feedback, true).unwrap();
        assert_eq!(old_stats, fast_stats);
        assert_eq!(fast_stats.numeric_sources, usize::from(numeric));
        assert_eq!(fast_stats.cons_sources, usize::from(!numeric));
        parity(&old, &fast);
    }
}

#[test]
fn opt_fast_empty_shortcuts_still_reject_malformed_ssa_before_discovery() {
    let mut invalid = plan(&[Op::Constant(0), Op::Return], &[LispValue::fixnum(7)], 0);
    invalid.values[0].def = ValueDef::Alias(Value(0));
    let expected = invalid.verify().unwrap_err();
    for fast in [false, true] {
        assert_eq!(
            bools::run_fast_for_test(&mut invalid.clone(), fast).unwrap_err(),
            expected
        );
        assert_eq!(
            licm::run_fast_for_test(&mut invalid.clone(), fast).unwrap_err(),
            expected
        );
        assert_eq!(
            sink::run_fast_for_test(&mut invalid.clone(), &[], fast).unwrap_err(),
            expected
        );
    }
}
