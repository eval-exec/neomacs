//! Static inlining at OSR keeps source-header identities while lowering a
//! fused instruction stream and attributing deopt feedback to its callee.

use super::*;
use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::jit::NumericFeedback;
use crate::emacs_core::jit::compile::{self, Inline2Mode};
use crate::emacs_core::jit::inline::force_inline_for_test;
use crate::emacs_core::jit::reopt::{self, ReoptKnobs, ReoptVerdict};
use crate::emacs_core::value::LambdaParams;

struct Knobs;

impl Knobs {
    fn enter(mode: Inline2Mode) -> Self {
        force_inline_for_test(Some(true));
        compile::force_inline2_for_test(Some(mode));
        compile::force_deopt_for_test(false);
        Self
    }
}

impl Drop for Knobs {
    fn drop(&mut self) {
        force_inline_for_test(None);
        compile::force_inline2_for_test(None);
        reopt::force_reopt_for_test(None);
    }
}

fn function(ops: Vec<Op>, constants: Vec<Value>) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams::simple(vec![
        crate::emacs_core::intern::intern("osr-inline-arg"),
    ]));
    f.lexical = true;
    f.ops = ops;
    f.constants = constants.into();
    f.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(16);
    f.seal_hand_assembled_ops_for_test();
    f.jit_runtime().set_hot_for_test();
    f
}

fn caller_and_callee() -> (ByteCodeFunction, Value) {
    let inner = Value::make_bytecode(function(
        vec![Op::StackRef(0), Op::Add1, Op::Return],
        vec![],
    ));
    let physical = function(
        vec![
            Op::Constant(0),
            Op::StackRef(1),
            Op::Call(1),
            Op::Pop,
            Op::Constant(0),
            Op::StackRef(1),
            Op::Call(1),
            Op::Pop,
            Op::StackRef(0),
            Op::GotoIfNotNil(4),
            Op::StackRef(0),
            Op::Return,
        ],
        vec![inner],
    );
    (physical, inner)
}

#[test]
fn osr_inline_disabled_keeps_the_original_header_and_no_chain_metadata() {
    let _knobs = Knobs::enter(Inline2Mode::Off);
    let ctx = Context::new();
    let (physical, _) = caller_and_callee();
    let id = physical.jit_runtime().compiled_id_or_assign();
    let entry = compile_osr_leaf(&ctx.obarray, &physical, 4, id, None).unwrap();
    assert_eq!(entry.stack_depth, 1);
    assert_eq!(entry.bind_depth, 0);
    assert_eq!(entry.leaf.obs.osr_pc, Some(4));
    assert!(entry.leaf.chains.is_empty());
}

#[test]
fn osr_inline_chain_maps_a_shifted_header_and_resumes_inside_the_callee() {
    let _knobs = Knobs::enter(Inline2Mode::Closure);
    let mut ctx = Context::new();
    let (physical, inner) = caller_and_callee();
    let id = physical.jit_runtime().compiled_id_or_assign();
    let entry = compile_osr_leaf(&ctx.obarray, &physical, 4, id, None).unwrap();
    assert_eq!(entry.stack_depth, 1);
    assert_eq!(entry.leaf.obs.osr_pc, Some(4));
    assert!(!entry.leaf.chains.is_empty(), "OSR lowers the v2 body");
    let maximum = Value::fixnum(Value::MOST_POSITIVE_FIXNUM);
    let arguments = [maximum.bits() as i64];
    let run = entry.leaf.invoke_osr(
        &mut ctx as *mut Context as *mut u8,
        arguments.as_ptr(),
        physical.jit_constant_base(),
        None,
    );
    let NativeRun::DeoptAt(resume) = run else {
        panic!("callee overflow must yield a precise chain deopt");
    };
    assert_eq!(resume.pc, 6, "the original physical Call pc");
    assert_eq!(resume.stack, [maximum, inner, maximum]);
    let inlined = resume.inlined.as_ref().expect("owned OSR chain readback");
    assert_eq!(inlined.frames.len(), 1);
    assert_eq!(inlined.frames[0].function, inner);
    assert_eq!(inlined.frames[0].pc, 1, "the callee Add1 overflow pc");
}

#[test]
fn osr_inline_chain_feedback_retires_the_osr_leaf_and_widens_the_inner_source() {
    let _knobs = Knobs::enter(Inline2Mode::Off);
    reopt::force_reopt_for_test(Some(ReoptKnobs::stress()));
    let ctx = Context::new();
    sync_cache_to_current_heap();
    let physical = function(vec![Op::StackRef(0), Op::Add1, Op::Return], vec![]);
    let inner = function(vec![Op::StackRef(0), Op::Add1, Op::Return], vec![]);
    let id = physical.jit_runtime().compiled_id_or_assign();
    let entry = compile_osr_leaf(&ctx.obarray, &physical, 0, id, None).unwrap();
    let leaf = Rc::clone(&entry.leaf);
    OSR_CACHE.with(|cache| cache.borrow_mut().insert((id, 0), Some(entry)));
    let snapshot = [Value::fixnum(1)];
    assert_eq!(
        reopt::note_deopt_chain(
            &ctx,
            &physical,
            &inner,
            &leaf,
            LeafOrigin::Osr {
                header_pc: 0,
                snapshot: &snapshot,
            },
            2,
            DeoptEvent::Precise {
                pc: 1,
                stack: &[],
                cause: Some(reopt::DeoptCause::ArithOperands(NumericFeedback::Float)),
            },
        ),
        ReoptVerdict::Invalidated
    );
    assert_eq!(
        inner.jit_runtime().numeric_feedback(1),
        NumericFeedback::Float
    );
    assert_eq!(
        physical.jit_runtime().numeric_feedback(1),
        NumericFeedback::FixnumOnly
    );
    assert!(!osr_entry_cached(&physical, 0));
    assert!(leaf.retired.get());
}

#[test]
fn osr_inline_dependencies_evict_a_source_whose_entry_leaf_inlines_nothing() {
    let _knobs = Knobs::enter(Inline2Mode::All);
    compile::force_profit_gate_for_test(false);
    let ctx = Context::new();
    sync_cache_to_current_heap();
    let physical = function(vec![Op::StackRef(0), Op::Return], vec![]);
    let id = compile_and_cache_jit_leaf(&physical, None).unwrap();
    let ptr = compiled_leaf_ptr_for_test(id).unwrap();
    // Cache retirement keeps this allocation alive throughout the test.
    let entry_leaf = unsafe { &*ptr };
    assert!(entry_leaf.inline_deps().is_empty());
    let symbol = crate::emacs_core::intern::intern("osr-only-inline-dependency");
    let mut osr = compile_osr_leaf(&ctx.obarray, &physical, 0, id, None).unwrap();
    Rc::get_mut(&mut osr.leaf).unwrap().inline_deps = Box::from([symbol]);
    register_osr_inline_deps(id, 0, &osr.leaf);
    OSR_CACHE.with(|cache| cache.borrow_mut().insert((id, 0), Some(osr)));
    assert!(INLINE_DEPS.with(|deps| deps.borrow().get(&symbol).unwrap().contains(&id)));
    evict_inline_dependents(symbol);
    assert_eq!(cache_entry_kind_for_test(id), "none");
    assert!(!osr_entry_cached(&physical, 0));
    assert!(entry_leaf.retired.get());
    assert!(INLINE_DEPS.with(|deps| deps.borrow().values().all(|ids| !ids.contains(&id))));
}

#[test]
fn osr_inline_dependency_union_survives_rebuilding_the_entry_leaf() {
    let _knobs = Knobs::enter(Inline2Mode::All);
    let ctx = Context::new();
    sync_cache_to_current_heap();
    let physical = function(vec![Op::StackRef(0), Op::Return], vec![]);
    let id = physical.jit_runtime().compiled_id_or_assign();
    let osr_symbol = crate::emacs_core::intern::intern("osr-union-dependency");
    let entry_symbol = crate::emacs_core::intern::intern("entry-union-dependency");
    let obsolete = crate::emacs_core::intern::intern("obsolete-entry-union-dependency");
    let mut osr = compile_osr_leaf(&ctx.obarray, &physical, 0, id, None).unwrap();
    Rc::get_mut(&mut osr.leaf).unwrap().inline_deps = Box::from([osr_symbol]);
    register_osr_inline_deps(id, 0, &osr.leaf);
    OSR_CACHE.with(|cache| cache.borrow_mut().insert((id, 0), Some(osr)));
    INLINE_DEPS.with(|deps| deps.borrow_mut().entry(obsolete).or_default().insert(id));
    let mut rebuilt = compile_osr_leaf(&ctx.obarray, &physical, 0, id, None)
        .unwrap()
        .leaf;
    Rc::get_mut(&mut rebuilt).unwrap().inline_deps = Box::from([entry_symbol]);
    register_inline_deps(id, &rebuilt);
    COMPILED.with(|cache| cache.borrow_mut().insert(id, CacheEntry::Compiled(rebuilt)));
    for symbol in [osr_symbol, entry_symbol] {
        assert!(INLINE_DEPS.with(|deps| deps.borrow().get(&symbol).unwrap().contains(&id)));
    }
    assert!(INLINE_DEPS.with(|deps| !deps.borrow().contains_key(&obsolete)));
    evict_inline_dependents(osr_symbol);
    assert_eq!(cache_entry_kind_for_test(id), "none");
    assert!(!osr_entry_cached(&physical, 0));
    assert!(INLINE_DEPS.with(|deps| deps.borrow().values().all(|ids| !ids.contains(&id))));
}

fn baseline_with_dependencies(
    ctx: &Context,
    physical: &ByteCodeFunction,
    id: u64,
    deps: &[SymId],
) -> Rc<CompiledLeaf> {
    let mut leaf = compile::lower_leaf_full(
        physical.executable_ops(),
        &physical.constants,
        1,
        physical.executable_gnu_byte_offset_map(),
        Some(&ctx.obarray),
        0,
    )
    .unwrap();
    leaf.obs.id = id;
    // Isolate cache policy from which builtin calls the emitter intrinsifies.
    leaf.inline_deps = deps.into();
    Rc::new(leaf)
}

#[test]
fn osr_inline_disabled_preserves_entry_only_dependency_eviction() {
    let _knobs = Knobs::enter(Inline2Mode::Off);
    let ctx = Context::new();
    sync_cache_to_current_heap();
    let physical = function(vec![Op::StackRef(0), Op::Return], vec![]);
    let id = physical.jit_runtime().compiled_id_or_assign();
    let entry_symbol = crate::emacs_core::intern::intern("off-entry-dependency");
    let other_entry_symbol = crate::emacs_core::intern::intern("off-other-entry-dependency");
    let osr_symbol = crate::emacs_core::intern::intern("off-osr-dependency");
    let entry_leaf =
        baseline_with_dependencies(&ctx, &physical, id, &[entry_symbol, other_entry_symbol]);
    register_inline_deps(id, &entry_leaf);
    COMPILED.with(|cache| {
        cache
            .borrow_mut()
            .insert(id, CacheEntry::Compiled(Rc::clone(&entry_leaf)))
    });
    let mut osr = compile_osr_leaf(&ctx.obarray, &physical, 0, id, None).unwrap();
    Rc::get_mut(&mut osr.leaf).unwrap().inline_deps = Box::from([osr_symbol]);
    register_osr_inline_deps(id, 0, &osr.leaf);
    let osr_leaf = Rc::clone(&osr.leaf);
    OSR_CACHE.with(|cache| cache.borrow_mut().insert((id, 0), Some(osr)));
    refresh_cached_inline_deps(id);
    register_inline_deps(id, &entry_leaf);
    assert!(INLINE_DEPS.with(|deps| !deps.borrow().contains_key(&osr_symbol)));

    evict_inline_dependents(entry_symbol);
    assert_eq!(cache_entry_kind_for_test(id), "none");
    assert!(entry_leaf.retired.get());
    assert!(osr_entry_cached(&physical, 0));
    assert!(!osr_leaf.retired.get());
    // The original entry-only policy leaves the other memberships untouched.
    assert!(INLINE_DEPS.with(|deps| {
        deps.borrow()
            .get(&other_entry_symbol)
            .is_some_and(|ids| ids.contains(&id))
    }));
    evict_compiled(id);
    assert!(INLINE_DEPS.with(|deps| {
        deps.borrow()
            .get(&other_entry_symbol)
            .is_some_and(|ids| ids.contains(&id))
    }));
}

#[test]
fn osr_inline_invalidation_preserves_dependencies_of_a_borrowed_entry() {
    let _knobs = Knobs::enter(Inline2Mode::All);
    reopt::force_reopt_for_test(Some(ReoptKnobs::stress()));
    let ctx = Context::new();
    sync_cache_to_current_heap();
    let physical = function(vec![Op::StackRef(0), Op::Return], vec![]);
    let id = physical.jit_runtime().compiled_id_or_assign();
    let entry_symbol = crate::emacs_core::intern::intern("retained-entry-dependency");
    let osr_symbol = crate::emacs_core::intern::intern("retired-osr-dependency");
    let entry_leaf = baseline_with_dependencies(&ctx, &physical, id, &[entry_symbol]);
    register_inline_deps(id, &entry_leaf);
    COMPILED.with(|cache| {
        cache
            .borrow_mut()
            .insert(id, CacheEntry::Compiled(Rc::clone(&entry_leaf)))
    });
    let mut osr = compile_osr_leaf(&ctx.obarray, &physical, 0, id, None).unwrap();
    Rc::get_mut(&mut osr.leaf).unwrap().inline_deps = Box::from([osr_symbol]);
    register_osr_inline_deps(id, 0, &osr.leaf);
    OSR_CACHE.with(|cache| cache.borrow_mut().insert((id, 0), Some(osr)));

    COMPILED.with(|cache| {
        let _borrowed = cache.borrow();
        invalidate_for_reopt(
            &physical,
            LeafOrigin::Osr {
                header_pc: 0,
                snapshot: &[Value::fixnum(1)],
            },
            ReoptLevel::Speculative,
            reopt::Reprofile::Immediate,
        );
    });
    assert_eq!(cache_entry_kind_for_test(id), "compiled");
    assert!(!entry_leaf.retired.get());
    assert!(!osr_entry_cached(&physical, 0));
    assert!(INLINE_DEPS.with(|deps| {
        deps.borrow()
            .get(&entry_symbol)
            .is_some_and(|ids| ids.contains(&id))
    }));
    assert!(INLINE_DEPS.with(|deps| !deps.borrow().contains_key(&osr_symbol)));
}
