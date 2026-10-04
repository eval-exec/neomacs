//! Admission differences run actual opt native code against an interpreter
//! forced to Tier-0. Threading: every test owns its context and compiled leaf;
//! settings affect compiler configuration on the test's existing scoped TLS.

use crate::emacs_core::jit::compile::opt_census::SelectedTier;

use super::compile_pipeline_tests::function;
use super::*;
use crate::emacs_core::bytecode::chunk::GnuByteOffsetMapEntry;
use crate::emacs_core::eval::{
    push_scratch_gc_roots, restore_scratch_gc_roots, save_scratch_gc_roots,
};
use crate::emacs_core::jit::NumericFeedback;
use crate::emacs_core::value::HashTableTest;

struct Settings;
impl Settings {
    fn enter() -> Self {
        force_opt_for_test(Some(OptMode::Opt), Some(OptAdmit::ALL));
        force_opt_profit_for_test(Some(OptProfitMode::Off));
        force_opt_passes_for_test(Some(OptPasses::default()));
        force_deopt_for_test(false);
        force_flonum_mode_for_test(Some(FlonumMode::OpLocal));
        Self
    }
}
impl Drop for Settings {
    fn drop(&mut self) {
        force_opt_for_test(None, None);
        force_opt_profit_for_test(None);
        force_opt_passes_for_test(None);
        force_deopt_for_test(false);
        force_flonum_mode_for_test(None);
    }
}

/// A test-owned lifetime in the active mutator's existing root stack.
struct Roots(usize);
impl Roots {
    fn new(values: &[Value]) -> Self {
        let saved = save_scratch_gc_roots();
        push_scratch_gc_roots(values);
        Self(saved)
    }
}
impl Drop for Roots {
    fn drop(&mut self) {
        restore_scratch_gc_roots(self.0);
    }
}

fn compile(ctx: &Context, f: &ByteCodeFunction) -> CompiledLeaf {
    let leaf = compile_bytecode_function_requested(
        f,
        Some(&ctx.obarray),
        CompileRequest {
            regalloc: RegallocPolicy::Full,
            bypass_profit_gate: true,
            origin: crate::emacs_core::jit::stats::CompileOrigin::Direct,
            tier: crate::emacs_core::jit::tier2::CompileTier::Upgrade(
                crate::emacs_core::jit::tier2::T2Upgrade::Feedback,
            ),
        },
    )
    .expect("the admitted body compiles");
    assert_eq!(leaf.selected_tier(), SelectedTier::Opt);
    leaf
}

fn tier0(ctx: &mut Context, f: &ByteCodeFunction, args: &[Value]) -> Value {
    let mut vm = Vm::from_context(ctx);
    vm.force_interpreter_only_for_test();
    vm.execute(f, args.to_vec()).unwrap()
}

fn native(ctx: &mut Context, leaf: &CompiledLeaf, f: &ByteCodeFunction, args: &[Value]) -> Value {
    match leaf.call_consts(ctx as *mut Context as *mut u8, f.constants.as_ptr(), args) {
        NativeRun::Ok(bits) => Value::from_bits(bits),
        other => panic!("admitted body must finish natively: {other:?}"),
    }
}

#[test]
fn opt_admits_optional_and_rest_with_gnu_argument_seeding() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    // GNU bytecode.c pushes non-rest arguments, nil-pads missing optionals,
    // and allocates the extra arguments' list in the final parameter slot.
    let mut f = function(vec![Op::List(3), Op::Return], vec![], 3);
    let mut params = f.params.named().expect("named fixture parameters").clone();
    params.required.truncate(1);
    f.params = params.into();
    let mut params = f.params.named().expect("named fixture parameters").clone();
    params.optional = vec![SymId(2)];
    f.params = params.into();
    let mut params = f.params.named().expect("named fixture parameters").clone();
    params.rest = Some(SymId(3));
    f.params = params.into();
    let leaf = compile(&ctx, &f);
    assert_eq!(leaf.required, 1);
    assert!(leaf.has_rest);
    for count in [1, 2, 5] {
        let args: Vec<_> = (1..=count).map(Value::make_int).collect();
        let expected = crate::emacs_core::print::print_value(&tier0(&mut ctx, &f, &args));
        let actual = native(&mut ctx, &leaf, &f, &args);
        assert_eq!(crate::emacs_core::print::print_value(&actual), expected);
    }
}

#[test]
fn opt_admits_patched_prefix_for_two_instances_of_one_source() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let proto = function(
        vec![Op::StackRef(0), Op::Constant(0), Op::Add, Op::Return],
        vec![Value::make_int(1)],
        1,
    );
    proto.jit_runtime().note_patched_prefix(1);
    let leaf = compile(&ctx, &proto);
    for captured in [Value::make_int(7), Value::make_int(41)] {
        let mut instance = proto.clone();
        instance.constants.ensure_owned()[0] = captured;
        for arg in [Value::make_int(-3), Value::make_int(19)] {
            let expected = tier0(&mut ctx, &instance, &[arg]);
            assert_eq!(native(&mut ctx, &leaf, &instance, &[arg]), expected);
        }
    }
}

#[test]
fn opt_admits_switch_with_raw_gnu_targets_and_equal_keys() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let symbol = Value::symbol("opt-native-switch-case");
    let list = Value::list(vec![symbol, Value::make_int(7)]);
    let table = Value::hash_table(HashTableTest::Equal);
    let _roots = Roots::new(&[list, table]);
    table
        .with_hash_table_mut(|ht| {
            ht.insert(symbol.to_hash_key(&ht.test), symbol, Value::make_int(50));
            ht.insert(list.to_hash_key(&ht.test), list, Value::make_int(70));
        })
        .unwrap();
    let mut f = function(
        vec![
            Op::StackRef(0),
            Op::Constant(0),
            Op::Switch,
            Op::Constant(1),
            Op::Return,
            Op::Constant(2),
            Op::Return,
            Op::Constant(3),
            Op::Return,
        ],
        vec![
            table,
            Value::make_int(-1),
            Value::make_int(10),
            Value::make_int(20),
        ],
        1,
    );
    // The table stores byte addresses 50/70, while native blocks use source
    // instruction indexes 5/7. The shared switch shim returns the raw address.
    f.gnu_byte_offset_map = Some(
        [0, 1, 2, 30, 40, 50, 60, 70, 80]
            .into_iter()
            .enumerate()
            .map(|(instruction_index, byte_offset)| {
                GnuByteOffsetMapEntry::new(byte_offset, instruction_index)
            })
            .collect(),
    );
    let leaf = compile(&ctx, &f);
    for arg in [
        symbol,
        Value::list(vec![symbol, Value::make_int(7)]),
        Value::make_int(7),
        Value::NIL,
    ] {
        let _arg_root = Roots::new(&[arg]);
        let expected = tier0(&mut ctx, &f, &[arg]);
        assert_eq!(native(&mut ctx, &leaf, &f, &[arg]), expected);
    }
}

#[test]
fn opt_admits_float_sites_preserving_signed_zero_and_object_identity() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let floats = |ops: Vec<Op>, pcs: &[usize]| {
        let f = function(ops, vec![], 2);
        for &pc in pcs {
            f.jit_runtime()
                .record_numeric(pc, f.ops.len(), NumericFeedback::Float);
        }
        f
    };
    let arithmetic = floats(
        vec![Op::StackRef(1), Op::StackRef(1), Op::Mul, Op::Return],
        &[2],
    );
    let leaf = compile(&ctx, &arithmetic);
    for (a, b) in [(-0.0, 1.0), (0.0, -1.0), (2.5, -2.0)] {
        let args = [Value::make_float(a), Value::make_float(b)];
        let _args_root = Roots::new(&args);
        let expected = tier0(&mut ctx, &arithmetic, &args).xfloat().to_bits();
        assert_eq!(
            native(&mut ctx, &leaf, &arithmetic, &args)
                .xfloat()
                .to_bits(),
            expected
        );
    }
    // One GNU arithmetic result is one object: Dup keeps identity, while
    // two executions of the same arithmetic operation allocate distinct ones.
    for (ops, pcs) in [
        (
            vec![
                Op::StackRef(1),
                Op::StackRef(1),
                Op::Mul,
                Op::Dup,
                Op::Eq,
                Op::Return,
            ],
            vec![2],
        ),
        (
            vec![
                Op::StackRef(1),
                Op::StackRef(1),
                Op::Mul,
                Op::StackRef(2),
                Op::StackRef(2),
                Op::Mul,
                Op::Eq,
                Op::Return,
            ],
            vec![2, 5],
        ),
    ] {
        let f = floats(ops, &pcs);
        let leaf = compile(&ctx, &f);
        let args = [Value::make_float(1.5), Value::make_float(2.0)];
        let _args_root = Roots::new(&args);
        let expected = tier0(&mut ctx, &f, &args);
        assert_eq!(native(&mut ctx, &leaf, &f, &args), expected);
    }
}

#[test]
fn opt_admits_loop_heap_writes_varrefs_and_generic_calls() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    ctx.eval_str("(setq opt-native-delta 3) (fset 'opt-native-step (lambda (x) x))")
        .unwrap();
    // v,pair,n,i. Each iteration reads/writes v[i], writes both pair fields,
    // reloads a dynamic variable, and crosses a generic call before the edge.
    let mut ops = vec![
        Op::Constant(0),
        Op::StackRef(0),
        Op::StackRef(2),
        Op::Lss,
        Op::GotoIfNil(0),
        Op::StackRef(3),
        Op::StackRef(1),
        Op::StackRef(1),
        Op::StackRef(1),
        Op::Aref,
        Op::VarRef(1),
        Op::Add,
        Op::Aset,
        Op::Pop,
        Op::StackRef(2),
        Op::StackRef(1),
        Op::Setcar,
        Op::Pop,
        Op::StackRef(2),
        Op::StackRef(4),
        Op::StackRef(2),
        Op::Aref,
        Op::Setcdr,
        Op::Pop,
        Op::Constant(2),
        Op::StackRef(1),
        Op::Call(1),
        Op::Pop,
        Op::StackRef(0),
        Op::Add1,
        Op::StackSet(1),
        Op::Goto(1),
    ];
    let done = ops.len() as u32;
    ops[4] = Op::GotoIfNil(done);
    ops.extend([
        Op::StackRef(3),
        Op::StackRef(3),
        Op::StackRef(2),
        Op::List(3),
        Op::Return,
    ]);
    let f = function(
        ops,
        vec![
            Value::make_int(0),
            Value::symbol("opt-native-delta"),
            Value::symbol("opt-native-step"),
        ],
        3,
    );
    let leaf = compile(&ctx, &f);
    for n in [0, 1, 8] {
        let args = || {
            [
                Value::vector((0..n).map(Value::make_int).collect()),
                Value::cons(Value::NIL, Value::NIL),
                Value::make_int(n),
            ]
        };
        let before = args();
        let _before_roots = Roots::new(&before);
        let expected = crate::emacs_core::print::print_value(&tier0(&mut ctx, &f, &before));
        let after = args();
        let _after_roots = Roots::new(&after);
        let actual = native(&mut ctx, &leaf, &f, &after);
        assert_eq!(crate::emacs_core::print::print_value(&actual), expected);
        assert_eq!(ctx.jit_root_stack_top, 0);
    }
}

#[test]
fn opt_admits_generic_gc_call_with_a_live_new_heap_value() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let f = function(
        vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::Cons,
            Op::Constant(2),
            Op::Call(0),
            Op::Pop,
            Op::Return,
        ],
        vec![
            Value::make_int(7),
            Value::make_int(9),
            Value::symbol("garbage-collect"),
        ],
        0,
    );
    let leaf = compile(&ctx, &f);
    let expected = crate::emacs_core::print::print_value(&tier0(&mut ctx, &f, &[]));
    for _ in 0..3 {
        let before = ctx.tagged_heap.gc_collections();
        let actual = native(&mut ctx, &leaf, &f, &[]);
        assert_eq!(crate::emacs_core::print::print_value(&actual), expected);
        assert!(ctx.tagged_heap.gc_collections() > before);
        assert_eq!(ctx.jit_root_stack_top, 0);
    }
}
