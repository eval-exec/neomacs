//! Opt adapter counters belong to the leaf being emitted, even after a rooted
//! body on the same compiler thread. Threading: each test owns its native leaves
//! and context; the backend mode scope restores the exact previous override.

use crate::emacs_core::jit::compile::opt_census::SelectedTier;

use super::compile_pipeline_tests::{captured_clif, function};
use super::*;
use crate::emacs_core::jit::opt::ir::ParamShape;

fn root_counts(clif: &str) -> (u32, u32) {
    let header = clif.lines().next().expect("the emitted leaf has a header");
    let field = |name: &str| {
        header
            .split_whitespace()
            .find_map(|part| part.strip_prefix(name))
            .expect("root-window diagnostics are present")
            .parse::<u32>()
            .expect("root-window diagnostics are counts")
    };
    (field("rw_stores="), field("rw_elided="))
}

fn lower(f: &ByteCodeFunction, params: Option<ParamShape>) -> (CompiledLeaf, String) {
    let mut leaf = None;
    let clif = captured_clif(|| {
        leaf = Some(
            opt_backend::lower_best(
                f.executable_ops(),
                &f.constants,
                f.params.required.len(),
                f.executable_gnu_byte_offset_map(),
                None,
                None,
                0,
                params,
            )
            .expect("the production adapter emits this body"),
        );
    });
    assert_eq!(clif.len(), 1, "one completed leaf is emitted");
    (leaf.unwrap(), clif.into_iter().next().unwrap())
}

fn seed_root_stores() -> CompiledLeaf {
    // Two generic calls require a hoisted root window. Its prologue resets the
    // old counters, and its stores produce actual nonzero counts for this leaf.
    let rooted = function(
        vec![
            Op::Constant(0),
            Op::StackRef(2),
            Op::StackRef(2),
            Op::Call(1),
            Op::Add,
            Op::StackRef(2),
            Op::StackRef(2),
            Op::Call(1),
            Op::Add,
            Op::Return,
        ],
        vec![Value::make_int(1)],
        2,
    );
    let (leaf, clif) = lower(&rooted, None);
    assert_eq!(leaf.selected_tier(), SelectedTier::Baseline);
    assert!(
        root_counts(&clif).0 > 0,
        "the preceding leaf emitted root stores"
    );
    leaf
}

fn pure() -> ByteCodeFunction {
    function(
        vec![Op::StackRef(0), Op::Constant(0), Op::Add, Op::Return],
        vec![Value::make_int(1)],
        1,
    )
}

fn assert_native_parity(ctx: &mut Context, f: &ByteCodeFunction, leaf: &CompiledLeaf) {
    let args = [Value::make_int(41)];
    let expected = {
        let mut vm = Vm::from_context(ctx);
        vm.force_interpreter_only_for_test();
        vm.execute(f, args.to_vec()).expect("Tier-0 answer")
    };
    let NativeRun::Ok(bits) =
        leaf.call_consts(ctx as *mut Context as *mut u8, f.constants.as_ptr(), &args)
    else {
        panic!("the emitted leaf must finish natively")
    };
    assert_eq!(Value::from_bits(bits), expected);
}

#[test]
fn opt_t1_root_counts_do_not_inherit_the_preceding_leaf() {
    let _mode = opt_mode_scope_for_test(OptMode::Opt);
    let mut ctx = Context::new();
    let _rooted = seed_root_stores();
    let f = pure();
    let (leaf, clif) = lower(&f, None);
    assert_eq!(leaf.selected_tier(), SelectedTier::Baseline);
    assert_eq!(root_counts(&clif), (0, 0));
    assert_native_parity(&mut ctx, &f, &leaf);
}

#[test]
fn opt_success_root_counts_do_not_inherit_the_preceding_leaf() {
    let _mode = opt_mode_scope_for_test(OptMode::Opt);
    let mut ctx = Context::new();
    let _rooted = seed_root_stores();
    let f = pure();
    let (leaf, clif) = lower(
        &f,
        Some(ParamShape {
            required: 1,
            ..ParamShape::default()
        }),
    );
    assert_eq!(leaf.selected_tier(), SelectedTier::Opt);
    assert_eq!(root_counts(&clif), (0, 0));
    assert_native_parity(&mut ctx, &f, &leaf);
}

#[test]
fn opt_refused_root_counts_do_not_inherit_the_preceding_leaf() {
    let _mode = opt_mode_scope_for_test(OptMode::Opt);
    let mut ctx = Context::new();
    let _rooted = seed_root_stores();
    // The real opt operation budget rejects this valid pure body independently
    // of admission/pass knobs. The production adapter then emits the baseline.
    let mut ops = Vec::new();
    for _ in 0..500 {
        ops.extend([Op::StackRef(0), Op::Pop]);
    }
    ops.extend([Op::StackRef(0), Op::Return]);
    let f = function(ops, vec![], 1);
    let params = ParamShape {
        required: 1,
        ..ParamShape::default()
    };
    assert!(matches!(
        opt_backend::admission(f.executable_ops(), params, 0),
        Err(CompileError::UnsupportedOp("opt-budget:ops"))
    ));
    let (leaf, clif) = lower(&f, Some(params));
    assert_eq!(leaf.selected_tier(), SelectedTier::Baseline);
    assert_eq!(root_counts(&clif), (0, 0));
    assert_native_parity(&mut ctx, &f, &leaf);
}
