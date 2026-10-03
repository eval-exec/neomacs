//! C2.8's generated cons barrier, including the cons cells behind BLVs.
//!
//! These tests execute native leaves and use the evaluator's full collection
//! and safe-point entries. Values kept across an allocation or collection are
//! rooted in the evaluator, never just in the native caller's argument array.

use super::*;
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::intern;
use crate::emacs_core::symbol::{LispBufferLocalValue, SymbolRedirect};
use crate::emacs_core::value::LambdaParams;

fn context(generational: bool) -> Context {
    // Nextest runs each test in a separate process. Select the constructor
    // knob before creating the heap, and restore the environment afterward:
    // compilation must read this heap's immutable setting, not the environment.
    struct RestoreKnob(Option<std::ffi::OsString>);
    impl Drop for RestoreKnob {
        fn drop(&mut self) {
            unsafe {
                match self.0.take() {
                    Some(previous) => std::env::set_var("NEOVM_GC_GENERATIONAL", previous),
                    None => std::env::remove_var("NEOVM_GC_GENERATIONAL"),
                }
            }
        }
    }
    crate::test_utils::init_test_tracing();
    let mut context = {
        let _restore = RestoreKnob(std::env::var_os("NEOVM_GC_GENERATIONAL"));
        unsafe {
            if generational {
                std::env::set_var("NEOVM_GC_GENERATIONAL", "1");
            } else {
                std::env::remove_var("NEOVM_GC_GENERATIONAL");
            }
        }
        Context::new()
    };
    context.gc_stress = false;
    context.tagged_heap.set_gc_threshold(usize::MAX);
    assert_eq!(context.tagged_heap.generational_enabled(), generational);
    context.gc_collect_exact();
    context
}

fn global(context: &Context, name: &str) -> Value {
    context.obarray().symbol_value(name).copied().expect(name)
}

fn minor(context: &mut Context) {
    assert!(context.tagged_heap.should_run_minor(false, false));
    let before = context.gc_count;
    context.tagged_heap.set_gc_threshold(1);
    // Restoring a binding need not allocate after the body's collection.
    // Charge a disposable cell so this real safe point starts the next cycle.
    let _ = Value::cons(Value::T, Value::NIL);
    for _ in 0..10_000 {
        context.gc_safe_point();
        assert!(!context.tagged_heap.concurrent_mark_running());
        if context.gc_count != before {
            assert_eq!(context.gc_count, before + 1);
            assert!(!context.tagged_heap.sweep_in_progress());
            context.tagged_heap.set_gc_threshold(usize::MAX);
            return;
        }
    }
    panic!("safe-point minor did not finish");
}

fn store_function(op: Op) -> ByteCodeFunction {
    let mut function = ByteCodeFunction::new(LambdaParams {
        required: vec![
            crate::emacs_core::intern::SymId(1),
            crate::emacs_core::intern::SymId(2),
        ],
        optional: Vec::new(),
        rest: None,
    });
    function.lexical = true;
    function.ops = vec![Op::StackRef(1), Op::StackRef(1), op, Op::Return];
    function.max_stack = 8;
    function
}

fn native(context: &mut Context, leaf: &CompiledLeaf, args: &[Value]) -> Value {
    match leaf.call(context as *mut Context as *mut u8, args) {
        NativeRun::Ok(bits) => Value::from_bits(bits),
        other => panic!("the leaf must complete natively: {other:?}"),
    }
}

fn cons_shims() -> usize {
    super::dispatch::LIST_STORE_SHIM_CALLS.with(|count| count.get())
}

#[test]
fn heapless_runtime_lowering_does_not_install_a_fallback_heap() {
    let mut heap = crate::tagged::gc::TaggedHeap::new();
    crate::tagged::gc::set_tagged_heap(&mut heap);
    crate::tagged::gc::clear_tagged_heap_if_installed(&heap);
    // Each store reaches RtCtx's lazy generational-mode capture. With no
    // installed heap it must choose the legacy gate without creating one.
    for op in [Op::Setcar, Op::Setcdr] {
        #[cfg(debug_assertions)]
        let before = crate::tagged::gc::heap_generational_mode_reads_for_test();
        #[cfg(debug_assertions)]
        let gates_before = super::heap_inline::GENERATIONAL_CONS_TESTS_EMITTED
            .load(std::sync::atomic::Ordering::Relaxed);
        let leaf = lower_leaf(&[Op::StackRef(1), Op::StackRef(1), op, Op::Return], &[], 2)
            .expect("heapless store lowering preserves the legacy shape");
        assert!(leaf.needs_vmctx, "the heap-store gate must be emitted");
        #[cfg(debug_assertions)]
        {
            assert_eq!(
                crate::tagged::gc::heap_generational_mode_reads_for_test(),
                before + 1,
                "the store gate must query the heapless generational mode"
            );
            assert_eq!(
                super::heap_inline::GENERATIONAL_CONS_TESTS_EMITTED
                    .load(std::sync::atomic::Ordering::Relaxed),
                gates_before,
                "heapless lowering must select the legacy store gate"
            );
        }
        assert!(!crate::tagged::gc::tagged_heap_is_installed());
        assert_eq!(crate::tagged::gc::current_tagged_heap_identity(), None);
    }
}

#[cfg(debug_assertions)]
#[test]
fn pure_runtime_lowering_does_not_read_a_moved_and_dropped_context_heap() {
    // Keep live replacement storage ready before the source TLS becomes stale.
    // Cleanup overwrites the raw slot without ever reading its old allocation,
    // including if compilation or an assertion panics.
    struct ClearSourceHeap(Box<crate::tagged::gc::TaggedHeap>);
    impl Drop for ClearSourceHeap {
        fn drop(&mut self) {
            crate::tagged::gc::set_tagged_heap(&mut self.0);
            crate::tagged::gc::clear_tagged_heap_if_installed(&self.0);
        }
    }
    let cleanup = ClearSourceHeap(Box::new(crate::tagged::gc::TaggedHeap::new()));
    let context = Context::new();
    let identity = context.tagged_heap.identity();
    std::thread::spawn(move || drop(context)).join().unwrap();
    // Drop retracts only the worker's installation. These metadata queries
    // deliberately do not dereference the freed source-thread heap pointer.
    assert!(crate::tagged::gc::tagged_heap_is_installed());
    assert_eq!(
        crate::tagged::gc::current_tagged_heap_identity(),
        Some(identity)
    );
    let before = crate::tagged::gc::heap_generational_mode_reads_for_test();
    let leaf = lower_leaf(&[Op::Constant(0), Op::Length, Op::Return], &[Value::NIL], 0);
    let after = crate::tagged::gc::heap_generational_mode_reads_for_test();
    drop(cleanup);
    assert!(
        leaf.is_ok(),
        "pure runtime lowering remains heap-independent"
    );
    assert_eq!(after, before, "pure lowering must not query the stale heap");
    assert!(!crate::tagged::gc::tagged_heap_is_installed());
}

#[test]
fn old_unlogged_cons_stores_take_one_shim_per_cycle() {
    let mut context = context(true);
    for (index, op) in [Op::Setcar, Op::Setcdr].into_iter().enumerate() {
        let owner = Value::cons(Value::NIL, Value::NIL);
        context.assign("u34-inline-owner", owner);
        context.gc_collect_exact();
        assert!(context.tagged_heap.value_is_old_for_test(owner));
        let leaf = compile_bytecode_function(&store_function(op)).expect("cons store compiles");

        for cycle in 0..2 {
            let roots = context.save_specpdl_roots();
            let child = Value::cons(Value::make_int(71 + index as i64 + cycle), Value::NIL);
            context.push_specpdl_root(child);
            assert!(!context.tagged_heap.value_is_old_for_test(child));
            let before = cons_shims();
            assert_eq!(native(&mut context, &leaf, &[owner, child]), child);
            assert_eq!(cons_shims() - before, 1, "the first store claims the owner");
            assert!(context.tagged_heap.is_remembered_for_test(owner));
            assert_eq!(context.tagged_heap.remembered_log_len_for_test(), 1);
            assert_eq!(native(&mut context, &leaf, &[owner, child]), child);
            assert_eq!(cons_shims() - before, 1, "the second store stays inline");
            context.restore_specpdl_roots(roots);

            minor(&mut context);
            let stored = if index == 0 {
                owner.cons_car()
            } else {
                owner.cons_cdr()
            };
            assert_eq!(stored, child);
            assert_eq!(
                stored.cons_car(),
                Value::make_int(71 + index as i64 + cycle)
            );
            assert!(context.tagged_heap.value_is_old_for_test(stored));
            assert_eq!(context.tagged_heap.remembered_log_len_for_test(), 0);
        }
    }
}

#[test]
fn young_cons_and_fixnum_or_character_stores_stay_inline() {
    let mut context = context(true);
    for op in [Op::Setcar, Op::Setcdr] {
        let leaf = compile_bytecode_function(&store_function(op)).expect("cons store compiles");
        let roots = context.save_specpdl_roots();
        let owner = Value::cons(Value::NIL, Value::NIL);
        context.push_specpdl_root(owner);
        let child = Value::cons(Value::make_int(31), Value::NIL);
        context.push_specpdl_root(child);
        assert!(!context.tagged_heap.value_is_old_for_test(owner));
        let before = cons_shims();
        assert_eq!(native(&mut context, &leaf, &[owner, child]), child);
        assert_eq!(cons_shims(), before);
        assert_eq!(context.tagged_heap.remembered_log_len_for_test(), 0);

        context.gc_collect_exact();
        assert!(context.tagged_heap.value_is_old_for_test(owner));
        for immediate in [Value::make_int(-17), Value::char('λ')] {
            assert_eq!(native(&mut context, &leaf, &[owner, immediate]), immediate);
            assert_eq!(cons_shims(), before);
            assert_eq!(context.tagged_heap.remembered_log_len_for_test(), 0);
        }
        context.restore_specpdl_roots(roots);
    }
}

#[test]
fn bare_uninterned_symbol_store_logs_owner_once_before_a_minor() {
    let mut context = context(true);
    context
        .eval_str(
            "(setq u34-inline-owner (cons nil nil)
               u34-inline-weak (make-hash-table :weakness 'key))",
        )
        .expect("old roots");
    context.gc_collect_exact();
    let owner = global(&context, "u34-inline-owner");
    let leaf = compile_bytecode_function(&store_function(Op::Setcar)).expect("cons store compiles");
    context
        .eval_str(
            "(progn (setq u34-inline-key (make-symbol \"u34-inline-key\"))
                (puthash u34-inline-key 19 u34-inline-weak))",
        )
        .expect("weak key setup");
    let key = global(&context, "u34-inline-key");
    assert!(key.is_symbol());
    assert!(!key.is_heap_object(), "the symbol needs side-table marking");
    assert!(context.tagged_heap.value_is_old_for_test(owner));
    assert!(!context.tagged_heap.is_remembered_for_test(owner));
    let logs_before = context.tagged_heap.remembered_log_len_for_test();
    let before = cons_shims();
    assert_eq!(native(&mut context, &leaf, &[owner, key]), key);
    // A minor conservatively retains every symbol id, so the weak-key check
    // below cannot prove logging. Observe the owner's mutator log directly.
    assert!(
        context.tagged_heap.is_remembered_for_test(owner),
        "storing a bare symbol must log the old owner"
    );
    assert_eq!(
        context.tagged_heap.remembered_log_len_for_test(),
        logs_before + 1
    );
    assert_eq!(cons_shims() - before, 1);
    assert_eq!(native(&mut context, &leaf, &[owner, key]), key);
    assert_eq!(
        context.tagged_heap.remembered_log_len_for_test(),
        logs_before + 1
    );
    assert_eq!(
        cons_shims() - before,
        1,
        "an already logged owner stays inline"
    );
    context.assign("u34-inline-key", Value::NIL);
    minor(&mut context);
    assert_eq!(owner.cons_car(), key);
    assert_eq!(
        context
            .eval_str("(hash-table-count u34-inline-weak)")
            .expect("weak table"),
        Value::make_int(1),
    );
    context.gc_collect_exact();
    assert_eq!(
        context
            .eval_str("(gethash (car u34-inline-owner) u34-inline-weak)")
            .expect("weak key survives major"),
        Value::make_int(19),
    );
}

#[cfg(debug_assertions)]
#[test]
fn legacy_cons_store_compilation_emits_no_generational_test() {
    use std::sync::atomic::Ordering;
    let mut context = context(false);
    let before = super::heap_inline::GENERATIONAL_CONS_TESTS_EMITTED.load(Ordering::Relaxed);
    for op in [Op::Setcar, Op::Setcdr] {
        let leaf = compile_bytecode_function(&store_function(op)).expect("legacy store compiles");
        let roots = context.save_specpdl_roots();
        let owner = Value::cons(Value::NIL, Value::NIL);
        context.push_specpdl_root(owner);
        assert_eq!(
            native(&mut context, &leaf, &[owner, Value::make_int(8)]),
            Value::make_int(8)
        );
        context.restore_specpdl_roots(roots);
    }
    assert_eq!(
        super::heap_inline::GENERATIONAL_CONS_TESTS_EMITTED.load(Ordering::Relaxed),
        before
    );
}

fn blv_fixture(local: bool) -> Context {
    let mut context = context(true);
    context.specpdl.reserve(16);
    context.jit_bind_stack.reserve(16);
    let setup = if local {
        "(progn (defvar u34-inline-blv (cons 43 nil))
                (make-local-variable 'u34-inline-blv)
                (setq u34-inline-blv (cons 47 nil)))"
    } else {
        // Localize in another buffer, leaving the default loaded here.
        "(progn (defvar u34-inline-blv (cons 43 nil))
                (save-current-buffer
                  (set-buffer (get-buffer-create \" u34-inline-other\"))
                  (make-local-variable 'u34-inline-blv))
                u34-inline-blv)"
    };
    context.eval_str(setup).expect("BLV fixture");
    context.gc_collect_exact();
    assert!(
        context
            .tagged_heap
            .value_is_old_for_test(blv_cell(&context, local))
    );
    context
}

fn blv(context: &Context) -> *mut LispBufferLocalValue {
    let symbol = context
        .obarray()
        .get_by_id(intern("u34-inline-blv"))
        .expect("BLV symbol");
    assert_eq!(symbol.redirect(), SymbolRedirect::Localized);
    // SAFETY: the checked localized symbol owns this record for its life.
    unsafe { symbol.val.blv }
}

fn blv_cell(context: &Context, local: bool) -> Value {
    // SAFETY: blv checked the record's live localized owner.
    let blv = unsafe { &*blv(context) };
    if local { blv.valcell } else { blv.defcell }
}

fn compile_blv(context: &Context, ops: &[Op], constants: &[Value], arity: usize) -> CompiledLeaf {
    force_inline_vars_for_test(Some(InlineVarsKnob::ALL));
    let compiled = super::inline_vars::with_compile_env_for_test(context, || {
        lower_leaf(ops, constants, arity)
    });
    force_inline_vars_for_test(None);
    compiled.expect("inline BLV program compiles")
}

fn blv_set_leaf(context: &Context) -> CompiledLeaf {
    compile_blv(
        context,
        &[Op::StackRef(0), Op::VarSet(0), Op::VarRef(0), Op::Return],
        &[Value::symbol("u34-inline-blv")],
        1,
    )
}

#[test]
fn blv_default_and_current_cell_stores_remember_young_children() {
    use super::shims::VARSET_SHIM_CALLS;
    for local in [false, true] {
        let mut context = blv_fixture(local);
        let owner = blv_cell(&context, local);
        let leaf = blv_set_leaf(&context);
        let roots = context.save_specpdl_roots();
        let child = Value::cons(Value::make_int(59), Value::NIL);
        context.push_specpdl_root(child);
        let before = VARSET_SHIM_CALLS.with(|count| count.get());
        assert_eq!(native(&mut context, &leaf, &[child]), child);
        assert_eq!(VARSET_SHIM_CALLS.with(|count| count.get()) - before, 1);
        assert!(context.tagged_heap.is_remembered_for_test(owner));
        assert_eq!(native(&mut context, &leaf, &[child]), child);
        assert_eq!(VARSET_SHIM_CALLS.with(|count| count.get()) - before, 1);
        context.restore_specpdl_roots(roots);

        minor(&mut context);
        assert_eq!(owner.cons_cdr(), child);
        assert_eq!(owner.cons_cdr().cons_car(), Value::make_int(59));
        assert!(context.tagged_heap.value_is_old_for_test(child));
        assert_eq!(context.tagged_heap.remembered_log_len_for_test(), 0);
    }
}

#[test]
fn blv_bind_and_restore_use_the_cons_gate_across_an_explicit_major() {
    use super::shims::{UNBIND_SHIM_CALLS, VARBIND_SHIM_CALLS};
    for local in [false, true] {
        let mut context = blv_fixture(local);
        context
            .eval_str("(fset 'u34-inline-body (lambda () (garbage-collect) 17))")
            .expect("GC body");
        let owner = blv_cell(&context, local);
        let restored = owner.cons_cdr();
        let leaf = compile_blv(
            &context,
            &[
                Op::StackRef(0),
                Op::VarBind(0),
                Op::Constant(1),
                Op::Call(0),
                Op::Pop,
                Op::Unbind(1),
                Op::VarRef(0),
                Op::Return,
            ],
            &[
                Value::symbol("u34-inline-blv"),
                Value::symbol("u34-inline-body"),
            ],
            1,
        );
        let roots = context.save_specpdl_roots();
        let child = Value::cons(Value::make_int(61), Value::NIL);
        context.push_specpdl_root(child);
        let bindings = context.specpdl.len();
        let bind_stack = context.jit_bind_stack.len();
        let collections = context.gc_count;
        let bind_before = VARBIND_SHIM_CALLS.with(|count| count.get());
        let unbind_before = UNBIND_SHIM_CALLS.with(|count| count.get());
        assert_eq!(native(&mut context, &leaf, &[child]), restored);
        assert_eq!(
            VARBIND_SHIM_CALLS.with(|count| count.get()) - bind_before,
            1
        );
        // The body's major reset the unlogged bit: restoring a pointer value
        // must log again, even though the earlier bind logged the same owner.
        assert_eq!(
            UNBIND_SHIM_CALLS.with(|count| count.get()) - unbind_before,
            1
        );
        assert_eq!(context.gc_count, collections + 1);
        assert_eq!(context.specpdl.len(), bindings);
        assert_eq!(context.jit_bind_stack.len(), bind_stack);
        assert_eq!(owner.cons_cdr(), restored);
        assert!(context.tagged_heap.is_remembered_for_test(owner));
        context.restore_specpdl_roots(roots);
        minor(&mut context);
        assert_eq!(
            owner.cons_cdr().cons_car(),
            Value::make_int(if local { 47 } else { 43 })
        );
    }
}

#[test]
fn mapped_blv_default_never_uses_the_owned_cons_trailer_or_a_baked_remembered_fact() {
    use super::shims::VARSET_SHIM_CALLS;
    use crate::tagged::header::{ConsCdrOrNext, ConsCell};
    let mut context = blv_fixture(false);
    let record = blv(&context);
    let old_default = blv_cell(&context, false);
    // One process-lifetime image cell keeps the mapped span exact. It has no
    // 64 KiB owned-block trailer and is registered like a pdump loader cell.
    let mapped = Box::leak(Box::new(ConsCell {
        car: Value::symbol("u34-inline-blv"),
        cdr_or_next: ConsCdrOrNext {
            cdr: old_default.cons_cdr(),
        },
    }));
    // SAFETY: leaked writable storage satisfies mapped range lifetime and
    // alignment. Both BLV pointers are updated while the collector is idle.
    let owner = unsafe {
        context.tagged_heap.register_mapped_cons_range(mapped, 1);
        let owner = Value::from_cons_ptr(mapped);
        (*record).defcell = owner;
        (*record).valcell = owner;
        owner
    };
    context.gc_collect_exact();
    let leaf = blv_set_leaf(&context);
    let roots = context.save_specpdl_roots();
    let child = Value::cons(Value::make_int(67), Value::NIL);
    context.push_specpdl_root(child);
    let before = VARSET_SHIM_CALLS.with(|count| count.get());
    assert_eq!(native(&mut context, &leaf, &[child]), child);
    let logged = context.tagged_heap.remembered_log_len_for_test();
    assert_eq!(native(&mut context, &leaf, &[child]), child);
    assert_eq!(VARSET_SHIM_CALLS.with(|count| count.get()) - before, 2);
    assert!(context.tagged_heap.is_remembered_for_test(owner));
    // Setup can already have logged the mapped default. Repeated stores
    // must use the shim while deduplicating the owner in the current log.
    assert_eq!(context.tagged_heap.remembered_log_len_for_test(), logged);
    context.restore_specpdl_roots(roots);
    minor(&mut context);
    assert_eq!(owner.cons_cdr().cons_car(), Value::make_int(67));
    context.gc_collect_exact();
    assert_eq!(owner.cons_cdr().cons_car(), Value::make_int(67));
}

#[path = "inline_heap_collection_revision.rs"]
mod collection_revision;

#[cfg(test)]
#[path = "inline_heap_gen0_collection_revision.rs"]
mod gen0_collection_revision;
