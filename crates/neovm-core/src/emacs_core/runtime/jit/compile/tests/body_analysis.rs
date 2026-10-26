use super::*;
use crate::emacs_core::eval::{
    Context, push_scratch_gc_root, restore_scratch_gc_roots, save_scratch_gc_roots,
};
use crate::emacs_core::jit::{NumericFeedback, inline, mir};
use crate::emacs_core::value::{HashTableTest, HashTableWeakness, LambdaParams};

struct Roots(usize);
impl Roots {
    fn enter() -> Self {
        Self(save_scratch_gc_roots())
    }
}
impl Drop for Roots {
    fn drop(&mut self) {
        restore_scratch_gc_roots(self.0);
    }
}

fn table() -> Value {
    let table = Value::hash_table(HashTableTest::Eq);
    push_scratch_gc_root(table);
    table
}

fn put(table: Value, target: Value) {
    let result = table.with_hash_table_mut(|ht| {
        let key = Value::fixnum(7);
        ht.insert(key.to_hash_key(&ht.test), key, target);
    });
    assert!(result.is_some());
}

fn lexical(ops: Vec<Op>, constants: Vec<Value>) -> ByteCodeFunction {
    let mut function = ByteCodeFunction::new(LambdaParams {
        required: Vec::new(),
        optional: Vec::new(),
        rest: None,
    });
    function.lexical = true;
    function.ops = ops;
    function.constants = constants.into();
    function.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(16);
    function
}

#[test]
fn successful_cfg_is_shared_across_mir_fuser_and_lower_view() {
    let _ctx = Context::new();
    let ops = [Op::Constant(0), Op::Return];
    let constants = [Value::fixnum(9)];
    let body = CompileBody::from_legacy_parts(&ops, &constants, 0, None, 0);
    assert!(body.stable.get().is_none());
    let first = body.source_cfg().unwrap();
    let built = mir::build_mir_from_source(&first, super::super::MirReach::OFF, &|_| {
        NumericFeedback::FixnumOnly
    })
    .expect("MIR from the source lease");
    assert_eq!(built.constants.as_ref(), constants);
    assert!(inline::fuse_calls_from_body(&body, &[NumericFeedback::FixnumOnly; 2]).is_none());
    let lower = body.for_lowering();
    let second = lower.as_ref().source_cfg().unwrap();
    assert!(std::ptr::eq(first.as_ref(), second.as_ref()));
    assert!(matches!(second, CfgLease::Shared(_)));
    assert_eq!(first.as_ref().leaders, [0]);
    let leaf = super::super::lower_leaf_body_with_plan(
        &body,
        EntryPoint::Function,
        None,
        None,
        None,
        None,
    )
    .expect("unchanged baseline uses the same successful source");
    assert!(matches!(
        leaf.call(std::ptr::null_mut(), &[]),
        super::super::NativeRun::Ok(bits) if bits == Value::fixnum(9).bits()
    ));
    assert!(std::ptr::eq(
        first.as_ref(),
        body.source_cfg().unwrap().as_ref()
    ));
}

#[test]
fn mir_refusal_preserves_successful_cfg_for_fuser_and_baseline() {
    let _ctx = Context::new();
    let ops = [Op::Constant(0), Op::Return, Op::Constant(0), Op::Return];
    let constants = [Value::fixnum(9)];
    let body = CompileBody::from_legacy_parts(&ops, &constants, 0, None, 0);
    let source = body.source_cfg().unwrap();
    assert!(matches!(
        mir::build_mir_from_source(&source, super::super::MirReach::OFF, &|_| {
            NumericFeedback::FixnumOnly
        },),
        Err(CompileError::UnsupportedOp("mir-unreachable-block"))
    ));
    assert!(inline::fuse_calls_from_body(&body, &[NumericFeedback::FixnumOnly; 4]).is_none());
    let leaf = super::super::lower_leaf_body_with_plan(
        &body,
        EntryPoint::Function,
        None,
        None,
        None,
        None,
    )
    .expect("the existing baseline accepts the dead leader");
    assert!(matches!(
        leaf.call(std::ptr::null_mut(), &[]),
        super::super::NativeRun::Ok(bits) if bits == Value::fixnum(9).bits()
    ));
    assert!(std::ptr::eq(
        source.as_ref(),
        body.source_cfg().unwrap().as_ref()
    ));
}

#[test]
fn cfg_errors_remain_uncached_and_preserve_legacy_lower_failure() {
    let ops = [Op::Return];
    let body = CompileBody::from_legacy_parts(&ops, &[], 0, None, 0);
    for _ in 0..2 {
        assert!(matches!(
            body.source_cfg(),
            Err(CompileError::StackUnderflow)
        ));
        assert!(body.stable.get().is_none());
    }
    assert!(matches!(
        super::super::lower_leaf_body_with_plan(
            &body,
            EntryPoint::Function,
            None,
            None,
            None,
            None,
        ),
        Err(CompileError::StackUnderflow)
    ));
    assert!(matches!(
        super::super::lower_leaf_full_osr(&ops, &[], 0, None, None, None, 0),
        Err(CompileError::StackUnderflow)
    ));
    assert!(body.stable.get().is_none());
}

#[test]
fn empty_and_unreachable_switches_are_fresh_even_without_targets() {
    let _ctx = Context::new();
    let _roots = Roots::enter();
    let constants = [Value::fixnum(7), table()];
    let cases = [
        (
            vec![
                Op::Constant(0),
                Op::Constant(1),
                Op::Switch,
                Op::Constant(0),
                Op::Return,
            ],
            2,
        ),
        (
            vec![
                Op::Constant(0),
                Op::Return,
                Op::Constant(0),
                Op::Constant(1),
                Op::Switch,
                Op::Constant(0),
                Op::Return,
            ],
            4,
        ),
    ];
    for (ops, pc) in cases {
        let body = CompileBody::from_legacy_parts(&ops, &constants, 0, None, 0);
        let first = body.source_cfg().expect("empty table CFG");
        let second = body.source_cfg().expect("fresh second observation");
        assert!(matches!(&first, CfgLease::FreshSwitch(_)));
        assert!(matches!(&second, CfgLease::FreshSwitch(_)));
        assert!(first.as_ref().switch_targets[&pc].is_empty());
        assert!(!std::ptr::eq(first.as_ref(), second.as_ref()));
        assert!(body.stable.get().is_none());
        // The existing index-addressed Switch guard precedes any fuser lease.
        assert!(
            inline::fuse_calls_from_body(&body, &vec![NumericFeedback::FixnumOnly; ops.len()],)
                .is_none()
        );
        assert!(body.stable.get().is_none());
    }
}

#[test]
fn weak_switch_observes_changed_removed_and_invalid_targets_each_time() {
    let _ctx = Context::new();
    let _roots = Roots::enter();
    let table = table();
    assert!(
        table
            .with_hash_table_mut(|ht| {
                ht.weakness = Some(HashTableWeakness::Key);
            })
            .is_some()
    );
    let constants = [Value::fixnum(7), table];
    let ops = [
        Op::Constant(0),
        Op::Constant(1),
        Op::Switch,
        Op::Constant(0),
        Op::Return,
        Op::Constant(0),
        Op::Return,
    ];
    let body = CompileBody::from_legacy_parts(&ops, &constants, 0, None, 0);
    put(table, Value::fixnum(3));
    let first = body.source_cfg().unwrap();
    assert_eq!(first.as_ref().switch_targets[&2], [(3, 3)]);
    put(table, Value::fixnum(5));
    let second = body.source_cfg().unwrap();
    assert_eq!(second.as_ref().switch_targets[&2], [(5, 5)]);
    assert_eq!(first.as_ref().switch_targets[&2], [(3, 3)]);
    assert!(table.with_hash_table_mut(|ht| ht.data.clear()).is_some());
    assert!(body.source_cfg().unwrap().as_ref().switch_targets[&2].is_empty());
    put(table, Value::NIL);
    assert!(matches!(
        body.source_cfg(),
        Err(CompileError::UnsupportedOp("switch-dynamic"))
    ));
    put(table, Value::fixnum(3));
    assert_eq!(
        body.source_cfg().unwrap().as_ref().switch_targets[&2],
        [(3, 3)]
    );
    assert!(body.stable.get().is_none());
}

#[test]
fn source_lease_preserves_gnu_byte_target_resolution_and_mir_error() {
    let _ctx = Context::new();
    let _roots = Roots::enter();
    let table = table();
    put(table, Value::fixnum(8));
    let constants = [table, Value::fixnum(10), Value::fixnum(20)];
    let ops = [
        Op::StackRef(0),
        Op::Constant(0),
        Op::Switch,
        Op::Constant(1),
        Op::Return,
        Op::Constant(2),
        Op::Return,
    ];
    let map = [GnuByteOffsetMapEntry::new(8, 5)];
    let body = CompileBody::from_legacy_parts(&ops, &constants, 1, Some(&map), 0);
    let source = body.source_cfg().unwrap();
    assert_eq!(source.as_ref().switch_targets[&2], [(8, 5)]);
    assert!(matches!(
        mir::build_mir_from_source(&source, super::super::MirReach::OFF, &|_| {
            NumericFeedback::FixnumOnly
        },),
        Err(CompileError::UnsupportedOp("mir-unmodelled-control:Switch"))
    ));
    let wrong_units = CompileBody::from_legacy_parts(&ops, &constants, 1, None, 0);
    assert!(matches!(
        wrong_units.source_cfg(),
        Err(CompileError::BadOperand)
    ));
    assert!(body.stable.get().is_none());
    assert!(wrong_units.stable.get().is_none());
}

#[test]
fn source_replacement_and_prefix_remask_never_import_an_existing_cell() {
    assert!(matches!(ConstantPrefix::from(0), ConstantPrefix::Unpatched));
    assert!(matches!(
        ConstantPrefix::from(1),
        ConstantPrefix::Captured(slots) if usize::from(slots) == 1
    ));
    assert_eq!(Option::<usize>::from(EntryPoint::from(Some(7))), Some(7));
    assert_eq!(Option::<usize>::from(EntryPoint::from(None)), None);
    assert_eq!(usize::from(SourceOpIndex::from(9)), 9);
    assert_eq!(usize::from(EntryStackDepth::from(3)), 3);
    let function = lexical(vec![Op::Constant(0), Op::Return], vec![Value::fixnum(9)]);
    let shape = JitParamShape::try_from(&function).unwrap();
    let body = CompileBody::for_function(FunctionView::new(&function), shape);
    let first = body.source_cfg().unwrap();
    let fused = FusedBody {
        v2: None,
        ops: vec![Op::Constant(0), Op::Add1, Op::Return],
        constants: vec![Value::fixnum(20)],
        feedback: vec![NumericFeedback::FixnumOnly; 3],
        offset_map: None,
        regions: Vec::new(),
        region_of: vec![None; 3],
        caller_of_fused: vec![0, 0, 1],
    };
    let replacement = CompileBody::for_fused(&body, &fused);
    assert!(replacement.stable.get().is_none());
    let fresh = replacement.source_cfg().unwrap();
    assert!(!std::ptr::eq(first.as_ref(), fresh.as_ref()));
    assert_eq!(fresh.ops(), fused.ops);
    assert_eq!(fresh.constants(), fused.constants);

    function.jit_runtime().note_patched_prefix(1);
    let patched = CompileBody::for_function(FunctionView::new(&function), shape);
    assert_eq!(patched.constants(), [Value::NIL]);
    let prior = patched.source_cfg().unwrap();
    let lower = patched.for_lowering();
    assert!(matches!(&lower, LowerBody::Remasked(_)));
    assert!(lower.as_ref().stable.get().is_none());
    let remasked = lower.as_ref().source_cfg().unwrap();
    assert!(!std::ptr::eq(prior.as_ref(), remasked.as_ref()));
    assert_eq!(remasked.constants(), [Value::NIL]);
}

#[test]
fn real_classic_fusion_gets_new_owner_and_matches_legacy_fuser() {
    let _ctx = Context::new();
    let _roots = Roots::enter();
    let callee = lexical(vec![Op::Constant(0), Op::Return], vec![Value::fixnum(19)]);
    callee.jit_runtime().set_hot_for_test();
    let callee = Value::make_bytecode(callee);
    push_scratch_gc_root(callee);
    let ops = [Op::Constant(0), Op::Call(0), Op::Return];
    let constants = [callee];
    let feedback = [NumericFeedback::FixnumOnly; 3];
    let body = CompileBody::from_legacy_parts(&ops, &constants, 0, None, 0);
    let prior = body.source_cfg().unwrap();
    let fused = inline::fuse_calls_from_body(&body, &feedback).expect("real static callee");
    let legacy = inline::fuse_calls(&ops, &constants, None, 0, &feedback).unwrap();
    assert_eq!(fused.ops, legacy.ops);
    assert_eq!(fused.constants, legacy.constants);
    assert_eq!(fused.regions, legacy.regions);
    let final_body = CompileBody::for_fused(&body, &fused);
    assert!(final_body.stable.get().is_none());
    let final_cfg = final_body.source_cfg().unwrap();
    assert!(!std::ptr::eq(prior.as_ref(), final_cfg.as_ref()));
    assert_eq!(final_cfg.ops(), fused.ops);
}
