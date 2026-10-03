//! Completed collection-read certificates must survive unrelated native
//! stores and reject mutations of their owners with generations disabled.

use super::*;
use crate::tagged::collection_reads::capture;
use crate::tagged::mutate::LispCollectionRevision;

struct JournalOverride;

impl JournalOverride {
    fn enabled() -> Self {
        super::super::force_gen0_collection_journal_for_test(Some(true));
        Self
    }
}

impl Drop for JournalOverride {
    fn drop(&mut self) {
        super::super::force_gen0_collection_journal_for_test(None);
    }
}

fn check_cons_store(op: Op) {
    let _journal = JournalOverride::enabled();
    let mut context = context(false);
    let roots = context.save_specpdl_roots();
    let owner = Value::cons(Value::NIL, Value::NIL);
    let unrelated = Value::cons(Value::NIL, Value::NIL);
    context.push_specpdl_root(owner);
    context.push_specpdl_root(unrelated);
    let leaf = compile_bytecode_function(&store_function(op.clone())).expect("cons store compiles");
    let read = || {
        if matches!(op, Op::Setcar) {
            owner.cons_car()
        } else {
            owner.cons_cdr()
        }
    };
    let (_, interpreted_reads) = capture(read);
    if matches!(op, Op::Setcar) {
        owner.set_car(Value::make_int(1));
    } else {
        owner.set_cdr(Value::make_int(1));
    }
    assert!(
        !interpreted_reads
            .expect("interpreter certificate")
            .unchanged()
    );

    let (_, compiled_reads) = capture(read);
    let compiled_reads = compiled_reads.expect("native certificate");
    let before = cons_shims();
    native(&mut context, &leaf, &[unrelated, Value::make_int(2)]);
    assert!(
        compiled_reads.unchanged(),
        "an unrelated owner keeps reuse valid"
    );
    let revision = LispCollectionRevision::current();
    assert_eq!(
        native(&mut context, &leaf, &[owner, Value::make_int(3)]),
        Value::make_int(3)
    );
    assert_eq!(cons_shims(), before, "the native store remains inline");
    assert!(
        !compiled_reads.unchanged(),
        "GEN0 native {op:?} must invalidate the observed owner like the interpreter"
    );
    assert_eq!(
        LispCollectionRevision::current().steps_since_for_test(revision),
        1
    );

    let (_, projected) = capture(|| native(&mut context, &leaf, &[owner, Value::make_int(4)]));
    owner.set_cdr(Value::NIL);
    assert!(
        !projected
            .expect("the native setter observes its owner")
            .unchanged(),
        "a setter-only capture retains the projected owner"
    );
    let (_, incoherent) = capture(|| {
        read();
        native(&mut context, &leaf, &[owner, Value::make_int(5)])
    });
    assert!(
        incoherent.is_none(),
        "a read before the native cons store cannot certify reuse"
    );
    context.restore_specpdl_roots(roots);
}

#[test]
fn gen0_inline_setcar_invalidates_collection_read_certificates() {
    check_cons_store(Op::Setcar);
}

#[test]
fn gen0_inline_setcdr_invalidates_collection_read_certificates() {
    check_cons_store(Op::Setcdr);
}

#[test]
fn gen0_constant_fixnum_store_invalidates_collection_read_certificates() {
    let _journal = JournalOverride::enabled();
    let mut context = context(false);
    let roots = context.save_specpdl_roots();
    let owner = Value::cons(Value::NIL, Value::NIL);
    context.push_specpdl_root(owner);
    let leaf = lower_leaf(
        &[Op::StackRef(0), Op::Constant(0), Op::Setcar, Op::Return],
        &[Value::make_int(12)],
        1,
    )
    .expect("constant store compiles");
    let (_, reads) = capture(|| owner.cons_car());
    let before = cons_shims();
    assert_eq!(native(&mut context, &leaf, &[owner]), Value::make_int(12));
    assert_eq!(cons_shims(), before);
    assert!(!reads.expect("observed cons").unchanged());
    context.restore_specpdl_roots(roots);
}

fn aset_leaf() -> CompiledLeaf {
    lower_leaf(
        &[
            Op::StackRef(2),
            Op::StackRef(2),
            Op::StackRef(2),
            Op::Aset,
            Op::Return,
        ],
        &[],
        3,
    )
    .expect("aset compiles")
}

fn check_array_store(source: &str, index: usize) {
    let _journal = JournalOverride::enabled();
    let mut context = context(false);
    let leaf = aset_leaf();
    let owner = context.eval_str(source).expect("array owner");
    let roots = context.save_specpdl_roots();
    context.push_specpdl_root(owner);
    let slot = Value::make_int(index as i64);
    // The first outlined call validates aset's function epoch; subsequent
    // stores must exercise its native owned-vector/record path.
    native(&mut context, &leaf, &[owner, slot, Value::make_int(1)]);
    let (_, interpreted_reads) = capture(|| owner.veclike_type());
    let set = |value| {
        if owner.as_vector_data().is_some() {
            owner.set_vector_slot(index, value)
        } else {
            owner.set_record_slot(index, value)
        }
    };
    assert!(set(Value::make_int(2)));
    assert!(
        !interpreted_reads
            .expect("interpreter certificate")
            .unchanged()
    );

    let (_, compiled_reads) = capture(|| owner.veclike_type());
    let before = super::super::dispatch::ASET_SHIM_CALLS.with(|count| count.get());
    let revision = LispCollectionRevision::current();
    native(&mut context, &leaf, &[owner, slot, Value::make_int(3)]);
    assert_eq!(
        super::super::dispatch::ASET_SHIM_CALLS.with(|count| count.get()),
        before,
        "the epoch-armed owned array store remains inline"
    );
    assert!(
        !compiled_reads.expect("native certificate").unchanged(),
        "GEN0 native aset must invalidate its array like the interpreter"
    );
    assert_eq!(
        LispCollectionRevision::current().steps_since_for_test(revision),
        1
    );

    let (_, projected) =
        capture(|| native(&mut context, &leaf, &[owner, slot, Value::make_int(4)]));
    assert!(set(Value::NIL));
    assert!(
        !projected
            .expect("the native setter observes its array")
            .unchanged()
    );
    let (_, incoherent) = capture(|| {
        owner.veclike_type();
        native(&mut context, &leaf, &[owner, slot, Value::make_int(5)])
    });
    assert!(
        incoherent.is_none(),
        "a read before the native array store cannot certify reuse"
    );
    context.restore_specpdl_roots(roots);
}

#[test]
fn gen0_inline_vector_aset_invalidates_collection_read_certificates() {
    check_array_store("(vector nil)", 0);
}

#[test]
fn gen0_inline_record_aset_invalidates_collection_read_certificates() {
    check_array_store("(record 'journal nil)", 1);
}

#[test]
fn gen0_inline_string_aset_invalidates_and_observes_collection_reads() {
    let _journal = JournalOverride::enabled();
    let mut context = context(false);
    let leaf = aset_leaf();
    for owner in [Value::unibyte_string("a"), Value::multibyte_string("a")] {
        let roots = context.save_specpdl_roots();
        context.push_specpdl_root(owner);
        let args = [owner, Value::make_int(0), Value::make_int(i64::from(b'b'))];
        // Arm the function epoch before taking the certificate.
        native(&mut context, &leaf, &args);
        let (_, interpreted_reads) = capture(|| owner.as_str_owned());
        assert!(owner.set_string_byte_same_char_count(0, b'c'));
        assert!(
            !interpreted_reads
                .expect("interpreter string certificate")
                .unchanged()
        );

        let (_, compiled_reads) = capture(|| owner.as_str_owned());
        let before = super::super::dispatch::ASET_SHIM_CALLS.with(|count| count.get());
        assert_eq!(native(&mut context, &leaf, &args), args[2]);
        assert_eq!(owner.as_str_owned().as_deref(), Some("b"));
        assert_eq!(
            super::super::dispatch::ASET_SHIM_CALLS.with(|count| count.get()),
            before
        );
        assert!(
            !compiled_reads
                .expect("native string certificate")
                .unchanged(),
            "inline string aset must invalidate a completed certificate"
        );
        let (_, projected) = capture(|| native(&mut context, &leaf, &args));
        assert!(owner.set_string_byte_same_char_count(0, b'd'));
        assert!(
            !projected
                .expect("native string setter certificate")
                .unchanged(),
            "inline string aset must project its owner into setter-only captures"
        );
        let (_, incoherent) = capture(|| {
            owner.as_str_owned();
            native(&mut context, &leaf, &args)
        });
        assert!(
            incoherent.is_none(),
            "a read before native string aset cannot certify reuse"
        );
        context.restore_specpdl_roots(roots);
    }
}

#[test]
fn gen0_aset_vector_shim_invalidates_collection_read_certificates() {
    let _journal = JournalOverride::enabled();
    let mut context = context(false);
    let owner = Value::vector(vec![Value::NIL]);
    let roots = context.save_specpdl_roots();
    context.push_specpdl_root(owner);
    let (_, reads) = capture(|| owner.as_vector_data().map(|items| items[0]));
    let value = Value::make_int(7);
    let result = super::super::dispatch::neovm_jit_aset(
        &mut context as *mut Context as *mut u8,
        owner.bits() as i64,
        Value::make_int(0).bits() as i64,
        value.bits() as i64,
    );
    assert_eq!(result, value.bits() as i64);
    assert!(
        !reads.expect("vector shim certificate").unchanged(),
        "the owned-vector shim fast path must journal its store"
    );
    let mut store = || {
        super::super::dispatch::neovm_jit_aset(
            &mut context as *mut Context as *mut u8,
            owner.bits() as i64,
            Value::make_int(0).bits() as i64,
            value.bits() as i64,
        )
    };
    let (_, projected) = capture(&mut store);
    assert!(owner.set_vector_slot(0, Value::NIL));
    assert!(
        !projected
            .expect("the vector shim projects its owner")
            .unchanged(),
        "a setter-only shim capture must retain the array dependency"
    );
    let (_, incoherent) = capture(|| {
        owner.as_vector_data();
        store()
    });
    assert!(
        incoherent.is_none(),
        "a prior read followed by the shim store is incoherent"
    );
    context.restore_specpdl_roots(roots);
}

fn gen0_blv_fixture(local: bool) -> Context {
    let mut context = context(false);
    context.specpdl.reserve(16);
    context.jit_bind_stack.reserve(16);
    let setup = if local {
        "(progn (defvar u34-inline-blv 43)
                (make-local-variable 'u34-inline-blv)
                (setq u34-inline-blv 47))"
    } else {
        "(progn (defvar u34-inline-blv 43)
                (save-current-buffer
                  (set-buffer (get-buffer-create \" u34-inline-other\"))
                  (make-local-variable 'u34-inline-blv))
                u34-inline-blv)"
    };
    context.eval_str(setup).expect("GEN0 BLV fixture");
    context
}

#[test]
fn gen0_inline_blv_set_invalidates_default_and_local_collection_reads() {
    let _journal = JournalOverride::enabled();
    use super::super::shims::VARSET_SHIM_CALLS;
    for local in [false, true] {
        let mut context = gen0_blv_fixture(local);
        let owner = blv_cell(&context, local);
        let leaf = blv_set_leaf(&context);
        let (_, interpreted_reads) = capture(|| owner.cons_cdr());
        context
            .eval_str("(setq u34-inline-blv 19)")
            .expect("interpreter BLV set");
        assert!(
            !interpreted_reads
                .expect("interpreter BLV certificate")
                .unchanged()
        );
        let (_, reads) = capture(|| owner.cons_cdr());
        let before = VARSET_SHIM_CALLS.with(|count| count.get());
        assert_eq!(
            native(&mut context, &leaf, &[Value::make_int(27)]),
            Value::make_int(27)
        );
        assert_eq!(VARSET_SHIM_CALLS.with(|count| count.get()), before);
        assert!(
            !reads.expect("native BLV certificate").unchanged(),
            "inline GEN0 BLV set must invalidate the loaded cell, local={local}"
        );
    }
}

#[test]
fn gen0_inline_blv_bind_and_restore_invalidate_collection_reads() {
    let _journal = JournalOverride::enabled();
    use super::super::shims::{UNBIND_SHIM_CALLS, VARBIND_SHIM_CALLS};
    for local in [false, true] {
        let mut context = gen0_blv_fixture(local);
        let owner = blv_cell(&context, local);
        let restored = owner.cons_cdr();
        let leaf = compile_blv(
            &context,
            &[
                Op::StackRef(0),
                Op::VarBind(0),
                Op::VarRef(0),
                Op::Unbind(1),
                Op::Return,
            ],
            &[Value::symbol("u34-inline-blv")],
            1,
        );
        let (_, interpreted_reads) = capture(|| owner.cons_cdr());
        context
            .eval_str("(let ((u34-inline-blv 19)) u34-inline-blv)")
            .expect("interpreter BLV bind and restore");
        assert!(
            !interpreted_reads
                .expect("interpreter bind certificate")
                .unchanged()
        );
        let (_, reads) = capture(|| owner.cons_cdr());
        let bind_before = VARBIND_SHIM_CALLS.with(|count| count.get());
        let unbind_before = UNBIND_SHIM_CALLS.with(|count| count.get());
        assert_eq!(
            native(&mut context, &leaf, &[Value::make_int(31)]),
            Value::make_int(31)
        );
        assert_eq!(owner.cons_cdr(), restored, "the binding is restored");
        assert_eq!(VARBIND_SHIM_CALLS.with(|count| count.get()), bind_before);
        assert_eq!(UNBIND_SHIM_CALLS.with(|count| count.get()), unbind_before);
        assert!(
            !reads.expect("native bind certificate").unchanged(),
            "inline GEN0 bind and restore must journal even when the final value is unchanged"
        );
    }
}
