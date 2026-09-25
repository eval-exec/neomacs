//! `make-closure` (GNU `Fmake_closure`, alloc.c): a FRESH closure object per
//! call whose constant vector is a fresh copy of the prototype's with the
//! first N slots replaced by the captured values, and every other slot the
//! prototype's. Neomacs builds the instance without an owned argument
//! vector and without a whole-function temporary; these tests pin the
//! observable result.
use crate::emacs_core::bytecode::{ByteCodeFunction, Op};
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::intern;
use crate::emacs_core::value::{LambdaParams, Value};
use crate::tagged::header::SubrFn;

fn eval(ctx: &mut Context, src: &str) -> Value {
    let v = ctx.eval_str(src).unwrap_or_else(|e| panic!("{src}: {e:?}"));
    crate::emacs_core::eval::push_scratch_gc_root(v);
    v
}

/// A GNU-shaped prototype: `(lambda () V0)` over `[V0 TAIL]`, with a
/// docstring, an interactive spec and two extra slots.
const PROTOTYPE: &str = "(setq mci-proto (make-byte-code 0 \"\\300\\207\" \
                         [V0 mci-tail] 1 \"Doc.\" \"p\" 'mci-x1 'mci-x2))";

#[test]
fn make_closure_receives_its_arguments_as_a_slice() {
    let _ctx = Context::new();
    let entry = crate::emacs_core::eval::lookup_global_subr_entry(intern("make-closure"))
        .expect("make-closure is a builtin subr");
    assert!(
        matches!(entry.function, Some(SubrFn::ManySlice(_))),
        "make-closure must not copy its arguments into an owned Vec per call"
    );
}

#[test]
fn make_closure_is_reachable_through_funcall_apply_and_bytecode() {
    let mut ctx = Context::new();
    eval(&mut ctx, PROTOTYPE);
    assert_eq!(
        eval(
            &mut ctx,
            "(funcall (funcall #'make-closure mci-proto 'by-funcall))"
        ),
        Value::symbol("by-funcall")
    );
    assert_eq!(
        eval(
            &mut ctx,
            "(funcall (apply #'make-closure mci-proto '(by-apply)))"
        ),
        Value::symbol("by-apply")
    );
    assert_eq!(
        eval(&mut ctx, "(funcall (make-closure mci-proto 'by-call))"),
        Value::symbol("by-call")
    );
    // The bytecode `call` op: `(make-closure mci-proto 'by-bytecode)`.
    let proto = eval(&mut ctx, "mci-proto");
    let mut caller = ByteCodeFunction::new(LambdaParams {
        required: Vec::new(),
        optional: Vec::new(),
        rest: None,
    });
    caller.lexical = true;
    caller.constants = vec![
        Value::symbol("make-closure"),
        proto,
        Value::symbol("by-bytecode"),
    ]
    .into();
    caller.ops = vec![
        Op::Constant(0),
        Op::Constant(1),
        Op::Constant(2),
        Op::Call(2),
        Op::Return,
    ];
    caller.max_stack = 3;
    caller.seal_hand_assembled_ops();
    let closure = {
        let mut vm = crate::emacs_core::bytecode::Vm::from_context(&mut ctx);
        vm.execute(&caller, vec![]).expect("bytecode make-closure")
    };
    crate::emacs_core::eval::push_scratch_gc_root(closure);
    ctx.set_variable("mci-by-bytecode", closure);
    assert_eq!(
        eval(&mut ctx, "(funcall mci-by-bytecode)"),
        Value::symbol("by-bytecode")
    );
    // Arity and type errors keep GNU's shape.
    assert_eq!(
        eval(
            &mut ctx,
            "(condition-case err (make-closure) (error (car err)))"
        ),
        Value::symbol("wrong-number-of-arguments")
    );
    assert_eq!(
        eval(
            &mut ctx,
            "(condition-case err (make-closure 'not-a-prototype 1) (error err))"
        ),
        eval(
            &mut ctx,
            "'(wrong-type-argument byte-code-function-p not-a-prototype)"
        )
    );
}

fn make_closure(proto: Value, vars: &[Value]) -> Value {
    let mut args = vec![proto];
    args.extend_from_slice(vars);
    let v = super::symbols::builtin_make_closure(&args).expect("make-closure");
    crate::emacs_core::eval::push_scratch_gc_root(v);
    v
}

fn data(v: Value) -> &'static ByteCodeFunction {
    v.get_bytecode_data().expect("a byte-code function")
}

#[test]
fn make_closure_instance_patches_the_prefix_and_keeps_every_other_slot() {
    let mut ctx = Context::new();
    let proto = eval(
        &mut ctx,
        "(make-byte-code 0 \"\\300\\301\\302E\\207\" [V0 V1 mci-tail] 3 \
         \"Doc.\" \"p\" 'mci-x1 'mci-x2)",
    );
    let before: Vec<Value> = data(proto).constants.to_vec();
    let a = make_closure(proto, &[Value::fixnum(1), Value::symbol("one")]);
    let b = make_closure(proto, &[Value::fixnum(2), Value::symbol("two")]);

    for (instance, first, second) in [
        (a, Value::fixnum(1), Value::symbol("one")),
        (b, Value::fixnum(2), Value::symbol("two")),
    ] {
        let (p, i) = (data(proto), data(instance));
        assert_eq!(
            i.constants.as_slice(),
            &[first, second, Value::symbol("mci-tail")]
        );
        // A fresh object with a fresh constant vector.
        assert_ne!(instance.bits(), proto.bits());
        assert_ne!(
            i.constants.as_slice().as_ptr(),
            p.constants.as_slice().as_ptr()
        );
        // Every other slot is the prototype's.
        assert_eq!(i.source_id, p.source_id);
        assert_eq!(i.arglist, p.arglist);
        assert_eq!(i.lexical, p.lexical);
        assert_eq!(i.max_stack, p.max_stack);
        assert_eq!(i.params.required, p.params.required);
        assert_eq!(i.params.optional, p.params.optional);
        assert_eq!(i.params.rest, p.params.rest);
        assert_eq!(i.env, p.env);
        assert_eq!(
            i.gnu_bytecode_bytes.as_deref(),
            p.gnu_bytecode_bytes.as_deref()
        );
        assert_eq!(
            i.docstring.as_ref().map(|d| d.as_bytes().to_vec()),
            p.docstring.as_ref().map(|d| d.as_bytes().to_vec())
        );
        assert_eq!(i.doc_form, p.doc_form);
        assert_eq!(i.interactive, p.interactive);
        assert_eq!(i.closure_slot_count, p.closure_slot_count);
        assert_eq!(i.extra_slots, p.extra_slots);
        assert_eq!(i.ops_sealed, p.ops_sealed);
        assert_eq!(i.stack_verified, p.stack_verified);
        assert_eq!(i.lazy_gnu_code.is_some(), p.lazy_gnu_code.is_some());
        if let (Some(il), Some(pl)) = (&i.lazy_gnu_code, &p.lazy_gnu_code) {
            assert!(std::sync::Arc::ptr_eq(il, pl), "one deferred decode");
        }
        #[cfg(feature = "jit")]
        assert!(
            std::ptr::eq(&**i.jit_runtime(), &**p.jit_runtime()),
            "one shared tiering state"
        );
    }
    assert_ne!(a.bits(), b.bits());
    // The prototype is untouched.
    assert_eq!(data(proto).constants.to_vec(), before);
    #[cfg(feature = "jit")]
    assert_eq!(data(proto).jit_runtime().patched_prefix(), 2);

    // The Lisp view: the instances run with their own captures, and the
    // slots GNU exposes are the prototype's.
    ctx.set_variable("mci-proto", proto);
    ctx.set_variable("mci-a", a);
    ctx.set_variable("mci-b", b);
    assert_eq!(
        eval(&mut ctx, "(list (funcall mci-a) (funcall mci-b))"),
        eval(&mut ctx, "'((1 one mci-tail) (2 two mci-tail))")
    );
    assert_eq!(
        eval(
            &mut ctx,
            "(list (equal (aref mci-a 1) (aref mci-proto 1)) (aref mci-a 2) \
             (length mci-a) (aref mci-a 4) (aref mci-a 5) (aref mci-a 6) \
             (aref mci-a 7) (func-arity mci-a) (equal mci-a mci-b) \
             (equal mci-a (make-closure mci-proto 1 'one)))"
        ),
        eval(
            &mut ctx,
            "'(t [1 one mci-tail] 8 \"Doc.\" \"p\" mci-x1 mci-x2 (0 . 0) nil t)"
        )
    );
}

#[test]
fn make_closure_widening_the_prefix_is_monotone() {
    let mut ctx = Context::new();
    let proto = eval(&mut ctx, "(make-byte-code 0 \"\\300\\207\" [V0 V1 V2] 1)");
    make_closure(proto, &[Value::fixnum(1)]);
    #[cfg(feature = "jit")]
    assert_eq!(data(proto).jit_runtime().patched_prefix(), 1);
    make_closure(
        proto,
        &[Value::fixnum(1), Value::fixnum(2), Value::fixnum(3)],
    );
    #[cfg(feature = "jit")]
    assert_eq!(data(proto).jit_runtime().patched_prefix(), 3);
    let narrow = make_closure(proto, &[Value::fixnum(9)]);
    #[cfg(feature = "jit")]
    assert_eq!(data(proto).jit_runtime().patched_prefix(), 3);
    assert_eq!(
        data(narrow).constants.as_slice(),
        &[Value::fixnum(9), Value::symbol("V1"), Value::symbol("V2")]
    );
    // No captures at all: a fresh copy of the prototype.
    let bare = make_closure(proto, &[]);
    assert_ne!(bare.bits(), proto.bits());
    assert_eq!(
        data(bare).constants.as_slice(),
        data(proto).constants.as_slice()
    );
}

#[test]
fn make_closure_rejects_more_captures_than_constants() {
    let mut ctx = Context::new();
    eval(
        &mut ctx,
        "(setq mci-small (make-byte-code 0 \"\\300\\207\" [V0] 1))",
    );
    assert_eq!(
        eval(
            &mut ctx,
            "(condition-case err (make-closure mci-small 1 2) (error err))"
        ),
        eval(&mut ctx, "'(error \"Closure vars do not fit in constvec\")")
    );
}

/// A prototype whose constant pool aliases a mapped (pdump) image: the
/// instance gets owned storage and the mapping is never written.
#[test]
fn make_closure_copies_a_mapped_prototype_pool() {
    let _ctx = Context::new();
    let image: &'static [Value] = Box::leak(
        vec![
            Value::symbol("V0"),
            Value::symbol("mci-mapped-1"),
            Value::symbol("mci-mapped-2"),
        ]
        .into_boxed_slice(),
    );
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: Vec::new(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = vec![Op::Constant(0), Op::Return];
    f.max_stack = 1;
    // SAFETY: `image` is leaked, so it outlives the function, and nothing
    // writes through a mapped pool.
    f.constants = unsafe { crate::tagged::header::LispValueVec::mapped(image.as_ptr(), 3) };
    f.seal_hand_assembled_ops();
    let proto = Value::make_bytecode(f);
    crate::emacs_core::eval::push_scratch_gc_root(proto);
    assert!(!data(proto).constants.is_owned());
    let instance = make_closure(proto, &[Value::fixnum(5)]);
    let i = data(instance);
    assert!(i.constants.is_owned());
    assert_eq!(
        i.constants.as_slice(),
        &[
            Value::fixnum(5),
            Value::symbol("mci-mapped-1"),
            Value::symbol("mci-mapped-2")
        ]
    );
    assert_eq!(image[0], Value::symbol("V0"));
    assert!(!data(proto).constants.is_owned());
}

/// A NeoVM-compiled closure (captured environment, no GNU constants
/// prefix) replaces the leading environment values instead.
#[test]
fn make_closure_on_an_environment_closure_rebinds_the_environment() {
    let mut ctx = Context::new();
    let env = eval(&mut ctx, "(list (cons 'mci-a 1) (cons 'mci-b 2))");
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: Vec::new(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = vec![Op::Nil, Op::Return];
    f.max_stack = 1;
    f.constants = vec![Value::symbol("mci-k")].into();
    f.env = Some(env);
    f.seal_hand_assembled_ops();
    let proto = Value::make_bytecode(f);
    crate::emacs_core::eval::push_scratch_gc_root(proto);
    let instance = make_closure(proto, &[Value::fixnum(10)]);
    let i = data(instance);
    ctx.set_variable("mci-env", i.env.expect("environment"));
    assert_eq!(
        eval(&mut ctx, "mci-env"),
        eval(&mut ctx, "'((mci-a . 10) (mci-b . 2))")
    );
    assert_eq!(i.constants.as_slice(), &[Value::symbol("mci-k")]);
    assert_eq!(data(proto).env, Some(env));
}

/// Instances share the prototype's bytecode string storage instead of
/// copying it, and `(aref INSTANCE 1)` still reads the prototype's bytes.
#[test]
fn make_closure_instances_share_the_prototype_bytes() {
    let mut ctx = Context::new();
    let proto = eval(
        &mut ctx,
        "(make-byte-code 0 \"\\300\\207\" [V0 mci-shared] 1)",
    );
    let a = make_closure(proto, &[Value::fixnum(1)]);
    let b = make_closure(a, &[Value::fixnum(2)]);
    let proto_bytes = data(proto).gnu_bytecode_bytes.as_ref().expect("bytes");
    for instance in [a, b] {
        let bytes = data(instance).gnu_bytecode_bytes.as_ref().expect("bytes");
        assert!(bytes.shares_storage_with(proto_bytes));
        assert_eq!(bytes.as_slice(), b"\xC0\x87");
    }
    ctx.set_variable("mci-shared-proto", proto);
    ctx.set_variable("mci-shared-a", a);
    assert_eq!(
        eval(
            &mut ctx,
            "(list (equal (aref mci-shared-a 1) (aref mci-shared-proto 1)) \
             (multibyte-string-p (aref mci-shared-a 1)) (aref mci-shared-a 1))"
        ),
        eval(&mut ctx, "'(t nil \"\\300\\207\")")
    );
}

/// `NEOVM_MAKE_CLOSURE_IN_PLACE=off` (the A/B baseline: clone, patch, move)
/// builds the same instance as the in-place writer.
#[test]
fn make_closure_by_clone_and_in_place_build_the_same_instance() {
    let mut ctx = Context::new();
    let proto = eval(
        &mut ctx,
        "(make-byte-code 0 \"\\300\\301\\302E\\207\" [V0 V1 mci-tail] 3 \
         \"Doc.\" \"p\" 'mci-x1)",
    );
    let mut built = Vec::new();
    for in_place in [true, false] {
        super::symbols::force_make_closure_in_place_for_test(in_place);
        let v = make_closure(proto, &[Value::fixnum(1), Value::symbol("one")]);
        let (i, p) = (data(v), data(proto));
        assert_ne!(v.bits(), proto.bits());
        assert_eq!(
            i.constants.as_slice(),
            &[
                Value::fixnum(1),
                Value::symbol("one"),
                Value::symbol("mci-tail")
            ]
        );
        assert!(
            i.gnu_bytecode_bytes
                .as_ref()
                .unwrap()
                .shares_storage_with(p.gnu_bytecode_bytes.as_ref().unwrap())
        );
        #[cfg(feature = "jit")]
        assert!(std::ptr::eq(&**i.jit_runtime(), &**p.jit_runtime()));
        assert_eq!(i.extra_slots, p.extra_slots);
        assert_eq!(i.closure_slot_count, p.closure_slot_count);
        assert_eq!(i.interactive, p.interactive);
        let err = match super::symbols::builtin_make_closure(&[
            proto,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ]) {
            Err(crate::emacs_core::error::Flow::Signal(sig)) => format!(
                "{} {}",
                sig.symbol_name(),
                sig.data
                    .iter()
                    .map(crate::emacs_core::print::print_value)
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
            other => panic!("expected a signal: {other:?}"),
        };
        assert_eq!(err, "error \"Closure vars do not fit in constvec\"");
        built.push((v, err));
    }
    assert_eq!(built[0].1, built[1].1);
    ctx.set_variable("mci-in-place", built[0].0);
    ctx.set_variable("mci-by-clone", built[1].0);
    assert_eq!(
        eval(
            &mut ctx,
            "(list (equal mci-in-place mci-by-clone) (funcall mci-by-clone))"
        ),
        eval(&mut ctx, "'(t (1 one mci-tail))")
    );
}
