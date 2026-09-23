//! `Op::VarRef` (GNU `Bvarref`) reads a plain symbol's value cell inline and
//! calls `neovm_jit_varref` for everything else. Each symbol shape must read
//! what the interpreter's opcode arm reads — value or signal — and the inline
//! read must be the one taken for a plain bound symbol.

use super::shims::VARREF_SHIM_CALLS;
use super::*;
use crate::emacs_core::bytecode::Vm;
use crate::emacs_core::eval::Context;
use crate::emacs_core::print::print_value;
use crate::emacs_core::value::LambdaParams;

fn body(ops: Vec<Op>, constants: Vec<Value>) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: Vec::new(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = ops;
    f.constants = constants.into();
    f.max_stack = 8;
    f
}

/// `(lambda () SYM)`
fn reader(sym: Value) -> ByteCodeFunction {
    body(vec![Op::VarRef(0), Op::Return], vec![sym])
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

fn interpret(eval: &mut Context, f: &ByteCodeFunction) -> String {
    let mut vm = Vm::from_context(eval);
    match vm.execute(f, vec![]) {
        Ok(v) => print_value(&v),
        Err(flow) => flow_text(flow),
    }
}

/// Run natively; `(answer, shim calls)`.
fn native(ctx_ptr: *mut u8, f: &ByteCodeFunction) -> (String, usize) {
    let leaf = compile_bytecode_function(f).expect("compiles");
    VARREF_SHIM_CALLS.with(|c| c.set(0));
    let answer = match leaf.call(ctx_ptr, &[]) {
        NativeRun::Ok(bits) => print_value(&Value::from_bits(bits)),
        NativeRun::Signal => flow_text(take_pending_flow().expect("flow stashed")),
        other => panic!("must not leave native code: {other:?}"),
    };
    (answer, VARREF_SHIM_CALLS.with(|c| c.get()))
}

fn eval_ok(eval: &mut Context, src: &str) {
    eval.eval_str(src)
        .unwrap_or_else(|e| panic!("{src}: {e:?}"));
}

#[test]
fn every_symbol_shape_reads_as_the_interpreter_reads_it() {
    let mut eval = Context::new();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    eval_ok(
        &mut eval,
        "(progn
           (defvar vri-plain 42)
           (defvar vri-nil nil)
           (defvar vri-unbound)
           (defvar vri-string \"s\")
           (defvaralias 'vri-alias 'vri-plain)
           (defvar vri-local 1)
           (make-local-variable 'vri-local)
           (setq vri-local 2)
           (setq buffer-undo-list nil)
           (insert \"undoable\"))",
    );
    // (symbol, inline read expected)
    let cases: &[(&str, bool)] = &[
        ("vri-plain", true),
        ("vri-nil", true),
        ("vri-string", true),
        ("vri-unbound", false),
        ("vri-alias", false),
        ("vri-local", false),
        ("fill-column", false),
        ("case-fold-search", false),
        ("gc-cons-threshold", false),
        ("buffer-undo-list", false),
        ("t", true),
        ("nil", true),
        // Made by `Value::symbol`, never interned into this obarray: an empty
        // slot, so the shim answers (with the keyword itself).
        (":vri-keyword", false),
        ("vri-never-defined", false),
    ];
    for &(name, inline) in cases {
        let f = reader(Value::symbol(name));
        let want = interpret(&mut eval, &f);
        let (got, shim_calls) = native(ctx_ptr, &f);
        assert_eq!(got, want, "{name}");
        assert_eq!(
            shim_calls == 0,
            inline,
            "{name}: inline read expected {inline}, shim called {shim_calls}x"
        );
    }
    // A let binding changes the cell the inline read sees, and the unbind
    // restores it.
    let f = reader(Value::symbol("vri-plain"));
    let depth = eval.specpdl.len();
    eval.try_specbind(
        crate::emacs_core::intern::intern("vri-plain"),
        Value::fixnum(7),
    )
    .expect("let");
    assert_eq!(native(ctx_ptr, &f), ("7".to_string(), 0));
    eval.unbind_to(depth);
    assert_eq!(native(ctx_ptr, &f), ("42".to_string(), 0));
    // `buffer-undo-list`'s cell is nil while the buffer holds the history:
    // the read must reach the buffer.
    assert_ne!(
        native(ctx_ptr, &reader(Value::symbol("buffer-undo-list"))).0,
        "nil"
    );
}

/// A symbol never interned into this context's obarray reads through the
/// shim, and signals as the interpreter does; a leaf compiled before the
/// obarray grew by several chunks still reads its symbol's cell.
#[test]
fn reads_survive_obarray_growth_and_absent_symbols() {
    let mut eval = Context::new();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    let absent = Value::from_sym_id(crate::emacs_core::intern::intern(
        "vri-interned-globally-only",
    ));
    let f = reader(absent);
    let want = interpret(&mut eval, &f);
    assert_eq!(native(ctx_ptr, &f).0, want);
    // An id past every chunk this obarray has: only the bounds check keeps
    // the read off the end of the spine.
    let mut far = absent;
    for i in 0..20_000 {
        far = Value::from_sym_id(crate::emacs_core::intern::intern(&format!(
            "vri-far-global-{i}"
        )));
    }
    let len_slots = far.as_symbol_id().expect("symbol").0 as usize;
    assert!(
        len_slots >= eval.obarray.symbol_count_upper_bound_for_test(),
        "the far id is outside this obarray"
    );
    let f = reader(far);
    let want = interpret(&mut eval, &f);
    assert_eq!(native(ctx_ptr, &f), (want, 1));

    eval_ok(&mut eval, "(defvar vri-grow 'before)");
    let f = reader(Value::symbol("vri-grow"));
    let leaf = compile_bytecode_function(&f).expect("compiles");
    for i in 0..12_000 {
        eval.obarray.intern(&format!("vri-growth-filler-{i}"));
    }
    eval_ok(&mut eval, "(setq vri-grow 'after)");
    VARREF_SHIM_CALLS.with(|c| c.set(0));
    match leaf.call(ctx_ptr, &[]) {
        NativeRun::Ok(bits) => assert_eq!(print_value(&Value::from_bits(bits)), "after"),
        other => panic!("{other:?}"),
    }
    assert_eq!(VARREF_SHIM_CALLS.with(|c| c.get()), 0, "still inline");
}

/// `(lambda () (condition-case err vri-void (void-variable (list 'caught err))))`
#[test]
fn a_void_variable_reaches_a_leaf_local_handler() {
    let mut eval = Context::new();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    eval_ok(&mut eval, "(defvar vri-void)");
    let f = body(
        vec![
            Op::PushConditionCase(4), // 0
            Op::VarRef(0),            // 1
            Op::PopHandler,           // 2
            Op::Return,               // 3
            Op::Constant(1),          // 4: [err caught]
            Op::StackRef(1),          // 5
            Op::List(2),              // 6
            Op::Return,               // 7
        ],
        vec![Value::symbol("vri-void"), Value::symbol("caught")],
    );
    let leaf = compile_bytecode_function(&f).expect("compiles");
    match leaf.call(ctx_ptr, &[]) {
        NativeRun::Ok(bits) => assert_eq!(
            print_value(&Value::from_bits(bits)),
            "(caught (void-variable vri-void))"
        ),
        other => panic!("{other:?}"),
    }
    eval_ok(&mut eval, "(setq vri-void 5)");
    match leaf.call(ctx_ptr, &[]) {
        NativeRun::Ok(bits) => assert_eq!(print_value(&Value::from_bits(bits)), "5"),
        other => panic!("{other:?}"),
    }
}

/// The shim path roots the residual stack; the inline read stores nothing.
/// A later call site must root `h` itself, so the continuation has to meet
/// the two store records: on the inline path, taken here, `h` is otherwise
/// unrooted across the collection.
///
///     (lambda () (let ((h (cons 1 2))) vri-meet (collect-and-allocate) h))
#[test]
fn a_later_call_site_roots_what_only_the_varref_shim_stored() {
    super::force_profit_gate_for_test(false);
    let mut eval = Context::new();
    let ctx_ptr = &mut eval as *mut Context as *mut u8;
    eval_ok(
        &mut eval,
        "(progn (defvar vri-meet 3)
                (fset 'vri-collect (lambda () (garbage-collect) (make-list 4096 (cons 0 0)) nil)))",
    );
    let f = body(
        vec![
            Op::Constant(0), // [1]
            Op::Constant(1), // [1 2]
            Op::Cons,        // [h]
            Op::VarRef(2),   // [h v]   residual [h]
            Op::Pop,         // [h]
            Op::Constant(3), // [h f]
            Op::Call(0),     // [h r]   residual [h]
            Op::Pop,         // [h]
            Op::Return,
        ],
        vec![
            Value::fixnum(1),
            Value::fixnum(2),
            Value::symbol("vri-meet"),
            Value::symbol("vri-collect"),
        ],
    );
    let leaf = compile_bytecode_function(&f).expect("compiles");
    for _ in 0..3 {
        match leaf.call(ctx_ptr, &[]) {
            NativeRun::Ok(bits) => {
                let h = Value::from_bits(bits);
                assert!(h.is_cons(), "h survived the collection (got {h:?})");
                assert_eq!(h.cons_car(), Value::fixnum(1));
                assert_eq!(h.cons_cdr(), Value::fixnum(2));
            }
            other => panic!("{other:?}"),
        }
    }
}

#[test]
fn raw_varref_preserves_integer_values_across_inline_and_fallback_reads() {
    let mut eval = Context::new();
    eval_ok(
        &mut eval,
        "(progn (defvar raw-vri-plain 7)
                (defvaralias 'raw-vri-alias 'raw-vri-plain)
                (defvar raw-vri-local 8)
                (make-local-variable 'raw-vri-local)
                (setq raw-vri-local 9))",
    );
    for (name, inline) in [
        ("raw-vri-plain", true),
        ("raw-vri-alias", false),
        ("raw-vri-local", false),
    ] {
        let mut f = body(
            vec![Op::Add1, Op::VarRef(0), Op::List(2), Op::Return],
            vec![Value::symbol(name)],
        );
        f.params
            .required
            .push(crate::emacs_core::intern::intern("x"));
        let leaf = compile_bytecode_function(&f).expect("raw reader compiles");
        for n in [
            Value::MOST_NEGATIVE_FIXNUM,
            -1,
            0,
            Value::MOST_POSITIVE_FIXNUM - 1,
        ] {
            let arg = Value::make_int(n);
            let want = Vm::from_context(&mut eval)
                .execute(&f, vec![arg])
                .expect("interpreter");
            let want = print_value(&want);
            VARREF_SHIM_CALLS.with(|c| c.set(0));
            let result = leaf.call(&mut eval as *mut Context as *mut u8, &[arg]);
            let NativeRun::Ok(bits) = result else {
                panic!("{name} {n}: {result:?}")
            };
            assert_eq!(print_value(&Value::from_bits(bits)), want, "{name} {n}");
            assert_eq!(VARREF_SHIM_CALLS.with(|c| c.get()), usize::from(!inline));
            assert_eq!(eval.jit_root_stack_top, 0);
        }
    }
}

#[test]
fn raw_varref_signal_handler_receives_tagged_residual_values() {
    let mut eval = Context::new();
    eval_ok(&mut eval, "(defvar raw-vri-void)");
    let mut f = body(
        vec![
            Op::PushConditionCase(6), // handler keeps the live argument
            Op::Add1,                 // raw integer replaces that argument
            Op::VarRef(0),
            Op::PopHandler,
            Op::List(2),
            Op::Return,
            Op::List(2), // [updated integer, condition data]
            Op::Return,
        ],
        vec![Value::symbol("raw-vri-void")],
    );
    f.params
        .required
        .push(crate::emacs_core::intern::intern("x"));
    let leaf = compile_bytecode_function(&f).expect("raw signal reader compiles");
    let result = leaf.call(&mut eval as *mut Context as *mut u8, &[Value::make_int(-1)]);
    let NativeRun::Ok(bits) = result else {
        panic!("{result:?}")
    };
    assert_eq!(
        print_value(&Value::from_bits(bits)),
        "(0 (void-variable raw-vri-void))"
    );
    assert_eq!(eval.jit_root_stack_top, 0);
    eval_ok(&mut eval, "(setq raw-vri-void 5)");
    let result = leaf.call(&mut eval as *mut Context as *mut u8, &[Value::make_int(-1)]);
    let NativeRun::Ok(bits) = result else {
        panic!("{result:?}")
    };
    assert_eq!(print_value(&Value::from_bits(bits)), "(0 5)");
    assert_eq!(eval.jit_root_stack_top, 0);
}

#[test]
fn raw_varref_keeps_heap_roots_live_at_a_later_collecting_call() {
    super::force_profit_gate_for_test(false);
    let mut eval = Context::new();
    eval_ok(
        &mut eval,
        "(progn (defvar raw-vri-gc 3)
                (defvaralias 'raw-vri-gc-alias 'raw-vri-gc)
                (fset 'raw-vri-collect (lambda () (garbage-collect) (make-list 4096 (cons 0 0)) nil)))",
    );
    for name in ["raw-vri-gc", "raw-vri-gc-alias"] {
        let mut f = body(
            vec![
                Op::Constant(0),
                Op::Constant(1),
                Op::Cons, // [x heap]
                Op::StackRef(1),
                Op::Add1,
                Op::StackSet(2), // [raw heap]
                Op::VarRef(2),
                Op::Pop,
                Op::Constant(3),
                Op::Call(0),
                Op::Pop,
                Op::List(2),
                Op::Return,
            ],
            vec![
                Value::make_int(1),
                Value::make_int(2),
                Value::symbol(name),
                Value::symbol("raw-vri-collect"),
            ],
        );
        f.params
            .required
            .push(crate::emacs_core::intern::intern("x"));
        let leaf = compile_bytecode_function(&f).expect("raw collecting reader compiles");
        let before = eval.tagged_heap.gc_collections();
        for _ in 0..3 {
            let result = leaf.call(&mut eval as *mut Context as *mut u8, &[Value::make_int(41)]);
            let NativeRun::Ok(bits) = result else {
                panic!("{name}: {result:?}")
            };
            assert_eq!(print_value(&Value::from_bits(bits)), "(42 (1 . 2))");
            assert_eq!(eval.jit_root_stack_top, 0);
        }
        assert!(eval.tagged_heap.gc_collections() >= before + 3);
    }
}
