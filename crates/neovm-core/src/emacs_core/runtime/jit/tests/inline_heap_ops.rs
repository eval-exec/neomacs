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

/// Evaluate `srcs` into values kept live by a global (`inline-heap-keep`)
/// until the next call: under `NEOVM_GC_STRESS` every evaluation collects,
/// and a value held only in a Rust local would be swept.
fn keep(eval: &mut Context, srcs: &[&str]) -> Vec<Value> {
    let form = format!("(setq inline-heap-keep (list {}))", srcs.join(" "));
    let list = eval.eval_str(&form).expect("operands");
    crate::emacs_core::value::list_to_vec(&list).expect("list")
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
                let fresh = |eval: &mut Context| keep(eval, &[cell_src, value_src]);
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
    let args = keep(&mut eval, &["(cons 1 2)", "(list 'new)"]);
    let (cell, value) = (args[0], args[1]);

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
    let args = keep(&mut eval, &["(list 'old)", "(cons nil 2)"]);
    let (old, cell) = (args[0], args[1]);
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
    let mut eval = Context::new();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    let leaf = compile_bytecode_function(&store_fn(Op::Setcdr)).expect("compiles");
    let image =
        crate::tagged::gc::fake_image::FakeImage::leak(false).register_cons(&mut eval.tagged_heap);
    let args = keep(&mut eval, &["(cons 1 nil)", "(list 'young)"]);
    let (heap_cell, child) = (args[0], args[1]);

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
    let cell = keep(&mut eval, &["(cons 1 2)"])[0];
    let before = shim_calls();
    assert_eq!(native(ctx_ptr, &leaf, &[cell, Value::T], "knob off"), "t");
    assert_eq!(shim_calls() - before, 1);
}

// ---- `aset` of a plain vector or record ----

/// `(lambda (a i v) (aset a i v))`
fn aset_fn() -> ByteCodeFunction {
    lexical_fn(
        3,
        vec![
            Op::StackRef(2),
            Op::StackRef(2),
            Op::StackRef(2),
            Op::Aset,
            Op::Return,
        ],
        vec![],
    )
}

fn aset_shim_calls() -> usize {
    super::dispatch::ASET_SHIM_CALLS.with(|c| c.get())
}

/// The storage probe finds the owned/mapped discriminant on this toolchain:
/// if a compiler change moves it, inline `aset` silently stays off, and this
/// is what notices.
#[test]
fn the_owned_storage_probe_answers_on_this_toolchain() {
    use crate::tagged::header::LispValueVec;
    assert!(LispValueVec::jit_slice_offsets().is_some());
    let (offset, word) = LispValueVec::jit_owned_probe().expect("the niche is found");
    assert_eq!(offset % std::mem::size_of::<usize>(), 0);
    let backing = [Value::T; 3];
    let mapped = unsafe { LispValueVec::mapped(backing.as_ptr(), 3) };
    let owned = LispValueVec::owned(vec![Value::NIL; 4]);
    let read = |v: &LispValueVec| unsafe {
        (v as *const LispValueVec as *const u8)
            .add(offset)
            .cast::<usize>()
            .read_unaligned()
    };
    assert_eq!(read(&mapped), word);
    assert_ne!(read(&owned), word);
}

/// The shapes `aset` stores inline never reach the shim; everything the
/// shim must decide — strings, bool-vectors, tagged char-table vectors,
/// out-of-range or non-fixnum indices, non-arrays — still does, with the
/// interpreter's answer (the full shape matrix is `array_shims`'
/// `array_sites_match_the_interpreter_natively`, run with this inline path).
#[test]
fn plain_vector_and_record_stores_stay_inline() {
    let mut eval = Context::new();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    let f = aset_fn();
    // Without the string intrinsic (`NEOVM_JIT_LEAF=string`), whatever the
    // environment says: a string store here must reach the shim.
    force_leaf_knob_for_test(Some(LeafKnob::OFF));
    let leaf = compile_bytecode_function(&f).expect("aset compiles");
    force_leaf_knob_for_test(None);
    assert!(leaf.needs_vmctx);
    let cases: &[(&str, &str, &str, bool)] = &[
        ("(vector 1 2 3)", "2", "'z", true),
        ("(vector 1 2 3)", "0", "(cons 1 2)", true),
        ("(record 'foo 1 2)", "1", "\"s\"", true),
        ("(record 'foo 1 2)", "0", "'bar", true),
        ("(make-vector 1 nil)", "0", "1.5", true),
        ("(vector 1 2 3)", "3", "'z", false),
        ("(vector 1 2 3)", "-1", "'z", false),
        ("(vector 1 2 3)", "'x", "'z", false),
        ("(vector)", "0", "'z", false),
        ("(make-bool-vector 5 nil)", "1", "t", false),
        ("(make-char-table 'foo 7)", "1", "'z", false),
        (
            "(let ((v (make-vector 80 nil))) (aset v 0 '--char-table--) v)",
            "3",
            "'d",
            false,
        ),
        ("(make-string 3 ?a)", "1", "98", false),
        ("(cons 1 2)", "0", "'z", false),
        ("nil", "0", "'z", false),
    ];
    // The first call arms the context's `aset` epoch cell through the shim.
    let warm = keep(&mut eval, &["(vector 0)"])[0];
    native(
        ctx_ptr,
        &leaf,
        &[warm, Value::make_int(0), Value::T],
        "warm",
    );
    for (array_src, index_src, value_src, inline) in cases {
        let what = format!("(aset {array_src} {index_src} {value_src})");
        let fresh = |eval: &mut Context| keep(eval, &[array_src, index_src, value_src]);
        let args = fresh(&mut eval);
        let array = args[0];
        let want = interpret(&mut eval, &f, args);
        let want_array = print_value(&array);
        let args = fresh(&mut eval);
        let array = args[0];
        let before = aset_shim_calls();
        let got = native(ctx_ptr, &leaf, &args, &what);
        assert_eq!(got, want, "{what}");
        assert_eq!(
            print_value(&array),
            want_array,
            "{what}: the array afterwards"
        );
        assert_eq!(
            aset_shim_calls() - before,
            usize::from(!inline),
            "{what}: inline={inline}"
        );
    }
}

/// With the string intrinsic on (`NEOVM_JIT_LEAF=string`, I2) an `aset`
/// site tries the inline vector store first, the inline string store on
/// its miss path, then the shim: a vector or record store stays inline, a
/// same-width string store is still answered inline by the string store,
/// and everything else reaches the shim, all as the interpreter answers.
#[test]
fn a_string_store_still_reaches_the_string_inline_behind_the_vector_store() {
    #[cfg(debug_assertions)]
    use std::sync::atomic::Ordering;
    let mut eval = Context::new();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    let f = aset_fn();
    #[cfg(debug_assertions)]
    let (heap0, string0) = (
        super::heap_inline::INLINE_HEAP_STORES_EMITTED.load(Ordering::Relaxed),
        super::lowering::STRING_ASET_INLINE_EMITTED.load(Ordering::Relaxed),
    );
    force_leaf_knob_for_test(Some(LeafKnob {
        string: true,
        ..LeafKnob::OFF
    }));
    let leaf = compile_bytecode_function(&f).expect("aset compiles");
    force_leaf_knob_for_test(None);
    #[cfg(debug_assertions)]
    {
        assert!(
            super::heap_inline::INLINE_HEAP_STORES_EMITTED.load(Ordering::Relaxed) > heap0,
            "the vector store was emitted"
        );
        assert!(
            super::lowering::STRING_ASET_INLINE_EMITTED.load(Ordering::Relaxed) > string0,
            "the string store was emitted"
        );
    }
    let cases: &[(&str, &str, &str, bool)] = &[
        ("(vector 1 2 3)", "2", "'z", true),
        ("(record 'foo 1 2)", "1", "\"s\"", true),
        ("(make-string 3 ?a)", "1", "98", true),
        ("(string-to-multibyte (make-string 3 ?a))", "2", "127", true),
        // A width change, a non-array, a bad index: the shim.
        (
            "(string-to-multibyte (make-string 3 ?a))",
            "1",
            "200",
            false,
        ),
        ("(cons 1 2)", "0", "'z", false),
        ("(vector 1 2 3)", "3", "'z", false),
    ];
    // The first call arms the context's `aset` epoch cell through the shim.
    let warm = keep(&mut eval, &["(vector 0)"])[0];
    native(
        ctx_ptr,
        &leaf,
        &[warm, Value::make_int(0), Value::T],
        "warm",
    );
    for (array_src, index_src, value_src, inline) in cases {
        let what = format!("(aset {array_src} {index_src} {value_src})");
        let fresh = |eval: &mut Context| keep(eval, &[array_src, index_src, value_src]);
        let args = fresh(&mut eval);
        let array = args[0];
        let want = interpret(&mut eval, &f, args);
        let want_array = print_value(&array);
        let args = fresh(&mut eval);
        let array = args[0];
        let before = aset_shim_calls();
        let got = native(ctx_ptr, &leaf, &args, &what);
        assert_eq!(got, want, "{what}");
        assert_eq!(
            print_value(&array),
            want_array,
            "{what}: the array afterwards"
        );
        assert_eq!(
            aset_shim_calls() - before,
            usize::from(!inline),
            "{what}: inline={inline}"
        );
    }
}

/// A stale `aset` epoch (a function-cell write since the cell was armed)
/// sends the next store to the shim, which re-arms the cell; the store
/// after it is inline again. A redefined `aset` keeps every store in the
/// shim, which answers the general call.
#[test]
fn a_function_cell_write_sends_the_next_aset_to_the_shim() {
    let mut eval = Context::new();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    let leaf = compile_bytecode_function(&aset_fn()).expect("aset compiles");
    let v = eval
        .eval_str("(setq aset-epoch-vector (vector 0 0))")
        .expect("v");
    let store = |eval: &mut Context, n: i64| {
        let _ = eval;
        native(
            ctx_ptr,
            &leaf,
            &[v, Value::make_int(1), Value::make_int(n)],
            "aset",
        )
    };
    store(&mut eval, 1);
    let before = aset_shim_calls();
    store(&mut eval, 2);
    assert_eq!(aset_shim_calls(), before, "an armed cell stores inline");

    eval.eval_str("(fset 'aset-epoch-mover (lambda () nil))")
        .expect("fset");
    let before = aset_shim_calls();
    store(&mut eval, 3);
    assert_eq!(aset_shim_calls() - before, 1, "a moved epoch re-validates");
    let before = aset_shim_calls();
    store(&mut eval, 4);
    assert_eq!(aset_shim_calls(), before, "re-armed");
    assert_eq!(print_value(&v), "[0 4]");

    eval.eval_str(
        "(progn (defvar aset-orig (symbol-function 'aset))
                (fset 'aset (lambda (a i v) (funcall aset-orig a i (list v)))))",
    )
    .expect("redefine");
    for n in 5..8 {
        let before = aset_shim_calls();
        store(&mut eval, n);
        assert_eq!(
            aset_shim_calls() - before,
            1,
            "a redefined aset never inlines"
        );
    }
    assert_eq!(print_value(&v), "[0 (7)]");
    eval.eval_str("(fset 'aset aset-orig)").expect("restore");
}

/// Tenured owners (a partitioned heap after its first cycle): an owner the
/// remembered set lacks goes to the shim once, whose barrier remembers it;
/// from then on its stores are inline. An image vector (in the window, and
/// mapped storage a store must copy) always goes to the shim.
#[test]
fn tenured_owners_store_inline_once_remembered() {
    let mut eval = Context::new();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    let leaf = compile_bytecode_function(&aset_fn()).expect("aset compiles");
    // The owners exist (and are reachable) before a fake image turns the
    // dump partition on, so whichever collection runs the partition's first
    // cycle — the explicit one below, or under GC stress an earlier safe
    // point — tenures them. The image vector's slots are mapped storage.
    eval.eval_str("(setq aset-old-a (vector 1 2 3) aset-old-b (record 'r 1 2))")
        .expect("owners");
    let image = crate::tagged::gc::fake_image::FakeImage::leak(true);
    image.register_cons(&mut eval.tagged_heap);
    let image_vector = image.register_vector(&mut eval.tagged_heap);
    eval.eval_str("(garbage-collect)")
        .expect("first partition cycle");
    let a = eval.eval_str("aset-old-a").expect("a");
    let b = eval.eval_str("aset-old-b").expect("b");
    assert!(eval.tagged_heap.is_tenured_for_test(a));
    assert!(eval.tagged_heap.is_tenured_for_test(b));
    // Arm the context's `aset` epoch cell.
    let young = keep(&mut eval, &["(vector 0)"])[0];
    native(
        ctx_ptr,
        &leaf,
        &[young, Value::make_int(0), Value::T],
        "arm",
    );

    for owner in [a, b] {
        assert!(!eval.tagged_heap.is_remembered_for_test(owner));
        let child = keep(&mut eval, &["(list 'young-child)"])[0];
        let before = aset_shim_calls();
        native(ctx_ptr, &leaf, &[owner, Value::make_int(1), child], "first");
        assert_eq!(
            aset_shim_calls() - before,
            1,
            "{owner:?}: the barrier remembers it"
        );
        assert!(eval.tagged_heap.is_remembered_for_test(owner));
        let before = aset_shim_calls();
        for n in 0..4 {
            let child = keep(&mut eval, &["(list 'another)"])[0];
            native(ctx_ptr, &leaf, &[owner, Value::make_int(2), child], "again");
            let _ = n;
        }
        assert_eq!(
            aset_shim_calls(),
            before,
            "{owner:?}: remembered owners store inline"
        );
    }
    // The young children stored into the old owners survive a collection
    // (the remembered set re-seeds them).
    eval.eval_str("(garbage-collect)").expect("second cycle");
    assert_eq!(print_value(&a), "[1 (young-child) (another)]");
    assert_eq!(print_value(&b), "#s(r (young-child) (another))");

    let before = aset_shim_calls();
    assert_eq!(
        native(
            ctx_ptr,
            &leaf,
            &[image_vector, Value::make_int(0), Value::T],
            "image"
        ),
        "t"
    );
    assert_eq!(
        aset_shim_calls() - before,
        1,
        "an image vector takes the shim"
    );
    assert_eq!(print_value(&image_vector), "[t 9 9]");
}

// ---- inline allocation: conses and float boxes ----

fn allocs_emitted() -> usize {
    #[cfg(debug_assertions)]
    {
        super::heap_inline::INLINE_ALLOCS_EMITTED.load(std::sync::atomic::Ordering::Relaxed)
    }
    #[cfg(not(debug_assertions))]
    {
        0
    }
}

fn conses_counted() -> i64 {
    Value::memory_use_counts_snapshot()[0]
}

fn floats_counted() -> i64 {
    Value::memory_use_counts_snapshot()[1]
}

/// `(lambda (n) (let ((acc nil)) (while (> n 0) (setq acc (cons n acc))
/// (when (= n HALF) (inline-alloc-probe)) (setq n (1- n))) acc))`: a cons
/// loop whose call (only when `n` is `half`) is a safe point.
fn cons_loop_fn(half: i64) -> ByteCodeFunction {
    lexical_fn(
        1,
        vec![
            Op::Nil,           // 0: acc              [n acc]
            Op::StackRef(1),   // 1: n                [n acc n]
            Op::Constant(0),   // 2: 0                [n acc n 0]
            Op::Gtr,           // 3                   [n acc b]
            Op::GotoIfNil(20), // 4                   [n acc]
            Op::StackRef(1),   // 5: n                [n acc n]
            Op::StackRef(1),   // 6: acc              [n acc n acc]
            Op::Cons,          // 7                   [n acc c]
            Op::StackSet(1),   // 8: acc = c          [n c]
            Op::StackRef(1),   // 9: n                [n acc n]
            Op::Constant(1),   // 10: half            [n acc n half]
            Op::Eqlsign,       // 11                  [n acc b]
            Op::GotoIfNil(16), // 12                  [n acc]
            Op::Constant(2),   // 13: probe           [n acc f]
            Op::Call(0),       // 14                  [n acc r]
            Op::Pop,           // 15                  [n acc]
            Op::StackRef(1),   // 16: n               [n acc n]
            Op::Sub1,          // 17                  [n acc n-1]
            Op::StackSet(2),   // 18: n = n-1         [n-1 acc]
            Op::Goto(1),       // 19
            Op::Return,        // 20: acc
        ],
        vec![
            Value::make_int(0),
            Value::make_int(half),
            Value::symbol("inline-alloc-probe"),
        ],
    )
}

/// A compiled cons loop allocates inline and counts exactly: 10 000 conses
/// are 10 000 in `memory-use-counts`, as in the interpreter; with a forced
/// collection in the middle (through a call — the safe point closes and
/// reopens the region) the list comes out whole, and the delta is the
/// interpreter's.
#[test]
fn a_cons_loop_allocates_inline_and_counts_exactly() {
    let mut eval = Context::new();
    eval.eval_str("(fset 'inline-alloc-probe (lambda () (garbage-collect) nil))")
        .expect("probe");
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    const N: i64 = 10_000;
    for (half, what) in [(-1, "no collection"), (N / 2, "a collection mid-loop")] {
        let f = cons_loop_fn(half);
        let emitted = allocs_emitted();
        let leaf = compile_bytecode_function_with(&f, Some(&eval.obarray)).expect("compiles");
        #[cfg(debug_assertions)]
        assert!(allocs_emitted() > emitted, "{what}: the cons is inline");
        assert!(leaf.needs_vmctx);

        let before = conses_counted();
        let want = interpret(&mut eval, &f, vec![Value::make_int(N)]);
        let interpreted = conses_counted() - before;

        let before = conses_counted();
        let got = match leaf.call(ctx_ptr, &[Value::make_int(N)]) {
            NativeRun::Ok(bits) => Value::from_bits(bits),
            other => panic!("{what}: the loop must run natively: {other:?}"),
        };
        let native_delta = conses_counted() - before;
        check_counted_list(got, N, what);
        assert_eq!(print_value(&got), want, "{what}");
        assert_eq!(
            native_delta, interpreted,
            "{what}: native and interpreted count alike"
        );
        if half < 0 {
            assert_eq!(native_delta, N, "{what}: exactly one count per cons");
        }
    }
    assert_eq!(eval.jit_root_stack_top, 0);
}

/// An on-stack replacement into a cons loop enters through the function
/// entry, which hoists the heap pointer the loop's inline cons reads.
#[test]
fn osr_into_a_cons_loop_allocates_inline() {
    let mut ctx = Context::new();
    let mut f = cons_loop_fn(-1);
    f.seal_hand_assembled_ops();
    let snapshot = [Value::make_int(500), Value::NIL];
    ctx.bc_buf.extend_from_slice(&snapshot);
    let before = conses_counted();
    let run = crate::emacs_core::jit::cache::try_run_osr(&mut ctx, &f, 1, &snapshot, &[]);
    let Some(NativeRun::Ok(bits)) = run else {
        panic!("the OSR transfer must run the loop natively: {run:?}");
    };
    check_counted_list(Value::from_bits(bits), 500, "osr");
    assert_eq!(conses_counted() - before, 500);
    assert_eq!(ctx.jit_root_stack_top, 0);
}

/// The MIR tier's escaping cons allocates inline; a scalar-replaced one
/// allocates nothing at all.
#[test]
fn mir_conses_allocate_inline_or_not_at_all() {
    let mut eval = Context::new();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    // (lambda (a b) (cons a (cons b nil))): both escape.
    let escaping = lexical_fn(
        2,
        vec![
            Op::StackRef(1),
            Op::StackRef(1),
            Op::Nil,
            Op::Cons,
            Op::Cons,
            Op::Return,
        ],
        vec![],
    );
    let emitted = allocs_emitted();
    let leaf = compile_bytecode_function(&escaping).expect("compiles");
    assert_eq!(leaf.tier(), leaf::LeafTier::Mir);
    #[cfg(debug_assertions)]
    assert_eq!(allocs_emitted() - emitted, 2, "two inline conses");
    let before = conses_counted();
    assert_eq!(
        native(ctx_ptr, &leaf, &[Value::make_int(1), Value::T], "escaping"),
        "(1 t)"
    );
    assert_eq!(conses_counted() - before, 2);

    // (lambda (a b) (car (cons a b))): the cons never escapes.
    let virtual_cons = lexical_fn(
        2,
        vec![
            Op::StackRef(1),
            Op::StackRef(1),
            Op::Cons,
            Op::Car,
            Op::Return,
        ],
        vec![],
    );
    let leaf = compile_bytecode_function(&virtual_cons).expect("compiles");
    assert_eq!(leaf.tier(), leaf::LeafTier::Mir);
    let before = conses_counted();
    assert_eq!(
        native(ctx_ptr, &leaf, &[Value::make_int(4), Value::T], "virtual"),
        "4"
    );
    assert_eq!(
        conses_counted() - before,
        0,
        "a scalar-replaced cons is never made"
    );
}

/// `(lambda (a b) (* a b))` with `Float` feedback: the product is boxed
/// inline — at the site (`NEOVM_JIT_FLONUM=off`) or where it escapes, at the
/// return (`resident`) — and counted once.
#[test]
fn float_boxes_are_inline_and_counted_once() {
    use crate::emacs_core::jit::NumericFeedback as NF;
    let mut eval = Context::new();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    for mode in [FlonumMode::Off, FlonumMode::Resident] {
        let mut f = lexical_fn(
            2,
            vec![Op::StackRef(1), Op::StackRef(1), Op::Mul, Op::Return],
            vec![],
        );
        f.seal_hand_assembled_ops();
        f.jit_runtime().record_numeric(2, f.ops.len(), NF::Float);
        force_flonum_mode_for_test(Some(mode));
        let emitted = allocs_emitted();
        let leaf = compile_bytecode_function(&f).expect("compiles");
        force_flonum_mode_for_test(None);
        #[cfg(debug_assertions)]
        assert!(allocs_emitted() > emitted, "{mode:?}: the box is inline");
        assert!(leaf.needs_vmctx);
        let a = Value::make_float(1.5);
        let b = Value::make_float(2.0);
        let before = floats_counted();
        let product = match leaf.call(ctx_ptr, &[a, b]) {
            NativeRun::Ok(bits) => Value::from_bits(bits),
            other => panic!("{mode:?}: must run natively: {other:?}"),
        };
        assert_eq!(product.xfloat(), 3.0, "{mode:?}");
        assert_eq!(
            floats_counted() - before,
            1,
            "{mode:?}: one float, counted once"
        );
    }
}

/// `NEOVM_JIT_INLINE_ALLOC=off`: no inline allocation is emitted; conses
/// and floats come from the shims, counted exactly the same.
#[test]
fn the_inline_alloc_knob_turns_inline_allocation_off() {
    unsafe { std::env::set_var("NEOVM_JIT_INLINE_ALLOC", "off") };
    assert!(!super::jit_inline_alloc_on());
    let mut eval = Context::new();
    eval.eval_str("(fset 'inline-alloc-probe (lambda () nil))")
        .expect("probe");
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    let f = cons_loop_fn(-1);
    let emitted = allocs_emitted();
    let leaf = compile_bytecode_function_with(&f, Some(&eval.obarray)).expect("compiles");
    assert_eq!(
        allocs_emitted(),
        emitted,
        "no inline allocation under the knob"
    );
    let before = conses_counted();
    let Some(bits) = (match leaf.call(ctx_ptr, &[Value::make_int(300)]) {
        NativeRun::Ok(bits) => Some(bits),
        _ => None,
    }) else {
        panic!("the loop must run natively");
    };
    check_counted_list(Value::from_bits(bits), 300, "knob off");
    assert_eq!(conses_counted() - before, 300);
}
