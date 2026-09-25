//! Heap writes (and allocation) inline in JIT code (`compile::heap_inline`):
//! every case must match the interpreter's opcode arm — result, signal, and
//! the object afterwards — and the stores the barrier must see (a concurrent
//! mark, owner tracking, an image owner) must still reach it.

use super::*;
use crate::emacs_core::bytecode::Vm;
use crate::emacs_core::eval::Context;
use crate::emacs_core::print::print_value;
use crate::emacs_core::value::LambdaParams;

fn lexical_fn(nargs: u32, ops: Vec<Op>, constants: Vec<Value>) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: (1..=nargs).map(crate::emacs_core::intern::SymId).collect(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = ops;
    f.constants = constants.into();
    f.max_stack = 8;
    f
}

/// `(lambda (c v) (setcar c v))` or `setcdr`.
fn store_fn(op: Op) -> ByteCodeFunction {
    lexical_fn(
        2,
        vec![Op::StackRef(1), Op::StackRef(1), op, Op::Return],
        vec![],
    )
}

fn flow_text(flow: crate::emacs_core::error::Flow) -> String {
    match flow {
        crate::emacs_core::error::Flow::Signal(sig) => format!(
            "signal {} {:?}",
            sig.symbol_name(),
            sig.data.iter().map(print_value).collect::<Vec<_>>()
        ),
        other => format!("{other:?}"),
    }
}

fn interpret(eval: &mut Context, f: &ByteCodeFunction, args: Vec<Value>) -> String {
    let mut vm = Vm::from_context(eval);
    match vm.execute(f, args) {
        Ok(v) => print_value(&v),
        Err(flow) => flow_text(flow),
    }
}

fn native(ctx_ptr: *mut u8, leaf: &CompiledLeaf, args: &[Value], what: &str) -> String {
    match leaf.call(ctx_ptr, args) {
        NativeRun::Ok(bits) => print_value(&Value::from_bits(bits)),
        NativeRun::Signal => flow_text(take_pending_flow().expect("flow stashed")),
        other => panic!("{what} must not leave native code: {other:?}"),
    }
}

fn shim_calls() -> usize {
    super::dispatch::LIST_STORE_SHIM_CALLS.with(|c| c.get())
}

const CELLS: &[&str] = &[
    "(cons 1 2)",
    "(list 1 2 3)",
    "(cons (cons 'a 'b) nil)",
    "nil",
    "'sym",
    "42",
    "(vector 1 2)",
    "\"str\"",
    "1.5",
];

const VALUES: &[&str] = &["0", "'x", "nil", "(cons 3 4)", "\"s\"", "2.5", "(vector)"];

/// Every cell shape × value through the compiled `setcar`/`setcdr` site and
/// the interpreter's opcode arm, each on a fresh cell: the same result or
/// signal and the same cell afterwards. A cons is stored inline — the shim
/// only sees the non-conses, whose signal it raises.
#[test]
fn cons_stores_match_the_interpreter_natively() {
    let mut eval = Context::new();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    for op in [Op::Setcar, Op::Setcdr] {
        let f = store_fn(op.clone());
        let leaf = compile_bytecode_function(&f).expect("store compiles");
        assert!(leaf.needs_vmctx, "{op:?}: an inline site reads the vmctx");
        for cell_src in CELLS {
            for value_src in VALUES {
                let what = format!("({op:?} {cell_src} {value_src})");
                let fresh = |eval: &mut Context| {
                    let pair = eval
                        .eval_str(&format!("(cons {cell_src} {value_src})"))
                        .expect("operands");
                    vec![pair.cons_car(), pair.cons_cdr()]
                };
                let args = fresh(&mut eval);
                let cell = args[0];
                let want = interpret(&mut eval, &f, args);
                let want_cell = print_value(&cell);
                let args = fresh(&mut eval);
                let cell = args[0];
                let before = shim_calls();
                let got = native(ctx_ptr, &leaf, &args, &what);
                assert_eq!(got, want, "{what}");
                assert_eq!(print_value(&cell), want_cell, "{what}: the cell afterwards");
                assert_eq!(
                    shim_calls() - before,
                    usize::from(!cell.is_cons()),
                    "{what}: only a non-cons reaches the shim"
                );
            }
        }
    }
}

/// Owner tracking makes the barrier window ALL: every compiled store goes to
/// the shim, whose barrier records the owner; switched off, the stores are
/// inline again.
#[test]
fn a_covering_window_sends_every_store_to_the_barrier() {
    use crate::tagged::gc::WriteTrackingMode;
    let mut eval = Context::new();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    let leaf = compile_bytecode_function(&store_fn(Op::Setcar)).expect("compiles");
    let cell = eval.eval_str("(cons 1 2)").expect("cell");
    let value = eval.eval_str("(list 'new)").expect("value");

    eval.tagged_heap
        .set_write_tracking_mode(WriteTrackingMode::OwnersAndRecords);
    let before = shim_calls();
    assert_eq!(native(ctx_ptr, &leaf, &[cell, value], "tracked"), "(new)");
    assert_eq!(shim_calls() - before, 1, "the window covers every owner");
    assert!(
        eval.tagged_heap.is_dirty_owner(cell),
        "the shim's barrier recorded the owner"
    );
    eval.tagged_heap
        .set_write_tracking_mode(WriteTrackingMode::Disabled);

    let before = shim_calls();
    assert_eq!(native(ctx_ptr, &leaf, &[cell, Value::T], "untracked"), "t");
    assert_eq!(shim_calls(), before, "an empty window stores inline");
    assert_eq!(print_value(&cell), "(t . 2)");
}

/// While a concurrent mark runs the window is ALL, so a compiled store logs
/// the overwritten car in the SATB buffer (the pre-image the mark must keep)
/// exactly as the interpreter's store does; after the mark, stores are
/// inline and log nothing.
#[test]
fn a_store_during_a_concurrent_mark_logs_its_pre_image() {
    let mut eval = Context::new();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    let leaf = compile_bytecode_function(&store_fn(Op::Setcar)).expect("compiles");
    let old = eval.eval_str("(list 'old)").expect("old");
    let cell = eval.eval_str("(cons nil 2)").expect("cell");
    crate::tagged::mutate::set_cons_car(cell, old);

    eval.tagged_heap.set_concurrent_active_for_test(true);
    let before = shim_calls();
    assert_eq!(native(ctx_ptr, &leaf, &[cell, Value::T], "marking"), "t");
    assert_eq!(
        shim_calls() - before,
        1,
        "a mark sends the store to the barrier"
    );
    let logged = eval.tagged_heap.take_satb_shared_for_test();
    eval.tagged_heap.set_concurrent_active_for_test(false);
    assert!(
        logged.iter().any(|v| v.bits() == old.bits()),
        "the overwritten car is logged: {logged:?}"
    );

    let before = shim_calls();
    assert_eq!(native(ctx_ptr, &leaf, &[cell, Value::NIL], "quiet"), "nil");
    assert_eq!(shim_calls(), before);
    assert!(eval.tagged_heap.take_satb_shared_for_test().is_empty());
}

/// An image cons (inside the dump span, the steady-state window) goes to the
/// barrier, which remembers it; a heap cons beside it is stored inline.
#[test]
fn an_image_owner_is_remembered_and_a_heap_owner_is_not() {
    use crate::tagged::header::{ConsCdrOrNext, ConsCell};
    let mut eval = Context::new();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    let leaf = compile_bytecode_function(&store_fn(Op::Setcdr)).expect("compiles");
    let image_cell = Box::into_raw(Box::new([ConsCell {
        car: Value::make_int(1),
        cdr_or_next: ConsCdrOrNext { cdr: Value::NIL },
    }])) as *mut ConsCell;
    unsafe { eval.tagged_heap.register_mapped_cons_range(image_cell, 1) };
    let image = unsafe { Value::from_cons_ptr(image_cell) };
    let heap_cell = eval.eval_str("(cons 1 nil)").expect("cell");
    let child = eval.eval_str("(list 'young)").expect("child");

    let before = shim_calls();
    assert_eq!(native(ctx_ptr, &leaf, &[image, child], "image"), "(young)");
    assert_eq!(shim_calls() - before, 1, "the image span is in the window");
    assert!(eval.tagged_heap.is_remembered_for_test(image));

    let before = shim_calls();
    assert_eq!(
        native(ctx_ptr, &leaf, &[heap_cell, child], "heap"),
        "(young)"
    );
    assert_eq!(shim_calls(), before, "a heap cons is stored inline");
    assert!(!eval.tagged_heap.is_remembered_for_test(heap_cell));
}

/// `(lambda (cell n) (while (> n 0) (setcar cell (cons n (car cell)))
/// (inline-heap-probe (car cell)) (setq n (1- n))) cell)`: a store loop
/// whose call is a GC safe point. Under exact-GC stress on a dump-less
/// heap every safe point collects — the first cycle stop-the-world, the
/// rest concurrent (marks and sweep slices land between the stores) — and
/// the list the stores build must come out whole.
fn store_loop_fn() -> ByteCodeFunction {
    lexical_fn(
        2,
        vec![
            Op::StackRef(0),   // 0: n              [cell n n]
            Op::Constant(0),   // 1: 0              [cell n n 0]
            Op::Gtr,           // 2                 [cell n b]
            Op::GotoIfNil(19), // 3                 [cell n]
            Op::StackRef(1),   // 4: cell           [cell n cell]
            Op::StackRef(1),   // 5: n              [cell n cell n]
            Op::StackRef(3),   // 6: cell           [cell n cell n cell]
            Op::Car,           // 7                 [cell n cell n c]
            Op::Cons,          // 8                 [cell n cell (n . c)]
            Op::Setcar,        // 9                 [cell n x]
            Op::Constant(1),   // 10: probe         [cell n x f]
            Op::StackRef(1),   // 11: x             [cell n x f x]
            Op::Call(1),       // 12                [cell n x r]
            Op::Pop,           // 13                [cell n x]
            Op::Pop,           // 14                [cell n]
            Op::StackRef(0),   // 15: n             [cell n n]
            Op::Sub1,          // 16                [cell n n-1]
            Op::StackSet(1),   // 17                [cell n-1]
            Op::Goto(0),       // 18
            Op::StackRef(1),   // 19: cell          [cell n cell]
            Op::Return,        // 20
        ],
        vec![Value::make_int(0), Value::symbol("inline-heap-probe")],
    )
}

fn check_counted_list(list: Value, n: i64, what: &str) {
    let items = crate::emacs_core::value::list_to_vec(&list).expect("a proper list");
    assert_eq!(items.len() as i64, n, "{what}: length");
    for (i, item) in items.iter().enumerate() {
        assert_eq!(*item, Value::make_int(i as i64 + 1), "{what}: element {i}");
    }
}

#[test]
fn a_store_loop_survives_collections_at_every_safe_point() {
    let mut eval = Context::new();
    eval.eval_str("(fset (quote inline-heap-probe) (lambda (x) (list x (cons x x))))")
        .expect("probe");
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    let f = store_loop_fn();
    let leaf = compile_bytecode_function_with(&f, Some(&eval.obarray)).expect("compiles");
    assert!(leaf.needs_vmctx);
    const N: i64 = 400;
    let collections_before = eval.tagged_heap.gc_collections();
    eval.gc_stress = true;
    for round in 0..4 {
        let cell = eval.eval_str("(cons nil nil)").expect("cell");
        match leaf.call(ctx_ptr, &[cell, Value::make_int(N)]) {
            NativeRun::Ok(bits) => {
                let cell = Value::from_bits(bits);
                check_counted_list(cell.cons_car(), N, &format!("round {round}"));
            }
            other => panic!("the loop must run natively: {other:?}"),
        }
    }
    eval.gc_stress = false;
    assert!(
        eval.tagged_heap.gc_collections() > collections_before + 10,
        "the stress collected ({} cycles)",
        eval.tagged_heap.gc_collections() - collections_before
    );
    assert_eq!(eval.jit_root_stack_top, 0);
}

/// `NEOVM_JIT_INLINE_HEAP_WRITE=off`: no inline store is emitted and every
/// store reaches the shim (the single-build A/B). Nextest runs each test in
/// its own process, so the knob is read fresh here.
#[test]
fn the_inline_heap_write_knob_turns_the_inline_stores_off() {
    unsafe { std::env::set_var("NEOVM_JIT_INLINE_HEAP_WRITE", "off") };
    assert!(!super::jit_inline_heap_write_on());
    let mut eval = Context::new();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    #[cfg(debug_assertions)]
    let emitted =
        super::heap_inline::INLINE_HEAP_STORES_EMITTED.load(std::sync::atomic::Ordering::Relaxed);
    let leaf = compile_bytecode_function(&store_fn(Op::Setcar)).expect("compiles");
    #[cfg(debug_assertions)]
    assert_eq!(
        super::heap_inline::INLINE_HEAP_STORES_EMITTED.load(std::sync::atomic::Ordering::Relaxed),
        emitted,
        "no inline store under the knob"
    );
    assert!(!leaf.needs_vmctx);
    let cell = eval.eval_str("(cons 1 2)").expect("cell");
    let before = shim_calls();
    assert_eq!(native(ctx_ptr, &leaf, &[cell, Value::T], "knob off"), "t");
    assert_eq!(shim_calls() - before, 1);
}
