//! A byte-code function's `aref` slot objects (`ByteCodeObj::slot_objects`,
//! P3.2 L4a) survive a pdump round trip with their identity: an object Lisp
//! held at dump time is the one the loaded function hands out, on both the
//! self-contained (baked, mapped extras) and the descriptor load paths. A
//! slot object created after the load in a mapped function is a young
//! object held by a dumped owner, so the write barrier must remember the
//! owner or the next collection frees it.
// API-drift shims (post-split re-home): the pre-split test reached `Context`,
// `ByteCodeFunction`, `LambdaParams`, `Op`, `dump_to_file` and
// `load_from_dump` through `use super::*`. The pdump constructor is now the
// core test facade; the seal goes through the signed AUTHORITY fence (the
// upper-crate `ByteCodeFunctionExt` trait stays crate-private); the obarray
// and heap readers use their public accessors.
use crate::emacs_core::bytecode::chunk::ByteCodeFunction;
use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::bytecode::value_api::TaggedValueBytecodeExt;
use crate::emacs_core::eval::Context;
use crate::emacs_core::load_boot::load_from_dump;
use crate::emacs_core::pdump::dump_to_file;
use neovm_value::value::{LambdaParams, Value};
fn eval(ctx: &mut Context, src: &str) -> Value {
    let v = ctx.eval_str(src).unwrap_or_else(|e| panic!("{src}: {e:?}"));
    crate::emacs_core::eval::push_scratch_gc_root(v);
    v
}

fn printed(ctx: &mut Context, src: &str) -> String {
    crate::emacs_core::print::print_value(&eval(ctx, src))
}

#[test]
fn bytecode_slot_objects_round_trip_with_their_identity() {
    crate::test_utils::init_test_tracing();
    let scratch_base = crate::emacs_core::eval::save_scratch_gc_roots();
    let mut ctx = crate::test_utils::new_context();
    // GNU bytecode (the reader literal): a self-contained mapped function
    // whose slot objects exist at dump time, and one whose do not.
    eval(
        &mut ctx,
        "(defvar psi-fn #[257 \"\\211\\300\\\\\\207\" [3] 3])",
    );
    eval(&mut ctx, "(defvar psi-code (aref psi-fn 1))");
    eval(&mut ctx, "(defvar psi-consts (aref psi-fn 2))");
    eval(&mut ctx, "(defvar psi-fresh #[0 \"\\300\\207\" [42] 1])");
    // Decoded instructions (no GNU byte string): the descriptor path.
    let mut decoded = ByteCodeFunction::new(LambdaParams::simple(Vec::new()));
    decoded.ops = vec![Op::Constant(0), Op::Return];
    decoded.constants = vec![Value::fixnum(7)].into();
    decoded.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(1);
    neovm_eval::test_support::authority::seal_hand_assembled_ops_for_test(&mut decoded);
    ctx.obarray_mut()
        .set_symbol_value("psi-decoded", Value::make_bytecode(decoded));
    eval(&mut ctx, "(defvar psi-decoded-consts (aref psi-decoded 2))");
    let mut decoded_fresh = ByteCodeFunction::new(LambdaParams::simple(Vec::new()));
    decoded_fresh.ops = vec![Op::Constant(0), Op::Return];
    decoded_fresh.constants = vec![Value::fixnum(8)].into();
    decoded_fresh.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(1);
    neovm_eval::test_support::authority::seal_hand_assembled_ops_for_test(&mut decoded_fresh);
    ctx.obarray_mut()
        .set_symbol_value("psi-decoded-fresh", Value::make_bytecode(decoded_fresh));

    let dir = tempfile::tempdir().unwrap();
    let dump_path = dir.path().join("slot-objects.pdump");
    dump_to_file(&ctx, &dump_path).expect("dump should succeed");
    // The dumping heap's values must not be scanned by the loaded heap.
    crate::emacs_core::eval::restore_scratch_gc_roots(scratch_base);
    let mut loaded = load_from_dump(&dump_path).expect("load should succeed");

    assert_eq!(
        printed(
            &mut loaded,
            "(list (eq psi-code (aref psi-fn 1)) (eq psi-consts (aref psi-fn 2)) \
                   (eq psi-decoded-consts (aref psi-decoded 2)) \
                   (aref psi-fn 2) (append (aref psi-fn 1) nil) (aref psi-decoded 2) \
                   (aref psi-decoded 1) (funcall psi-fn 1) (funcall psi-decoded))"
        ),
        "(t t t [3] (137 192 92 135) [7] nil 4 7)"
    );
    let consts = eval(&mut loaded, "psi-consts");
    assert!(
        loaded.tagged_heap().mapped_image_owns_for_test(consts),
        "a dumped slot object is image-resident"
    );

    // Young slot objects held only by an OLD owner, which the partitioned
    // collector never traces again unless the owner is remembered: a lazy
    // image stub, a descriptor-restored image function, and a function born
    // after the load that the first partition cycle tenures.
    eval(&mut loaded, "(defvar psi-young #[0 \"\\300\\207\" [43] 1])");
    let owners = [
        ("psi-fresh", true, "([42] (192 135) 42)"),
        ("psi-decoded-fresh", true, "([8] nil 8)"),
        ("psi-young", false, "([43] (192 135) 43)"),
    ];
    for (owner, mapped, _) in owners {
        let value = eval(&mut loaded, owner);
        assert_eq!(
            loaded.tagged_heap().mapped_image_owns_for_test(value),
            mapped,
            "{owner}"
        );
    }
    // Past the first partition cycle the image is permanently black and
    // the survivors are tenured: only remembered owners are traced again.
    loaded.gc_collect_exact();
    loaded.gc_collect_exact();
    for (owner, _, expected) in owners {
        let before_slot_objects = crate::emacs_core::eval::save_scratch_gc_roots();
        let vector = eval(&mut loaded, &format!("(aref {owner} 2)"));
        let code = eval(&mut loaded, &format!("(aref {owner} 1)"));
        assert!(!loaded.tagged_heap().mapped_image_owns_for_test(vector));
        // From here the function is their only holder.
        crate::emacs_core::eval::restore_scratch_gc_roots(before_slot_objects);
        for gc_stress in [false, true] {
            neovm_eval::test_support::authority::set_gc_stress_for_test(&mut loaded, gc_stress);
            for _ in 0..3 {
                loaded.gc_collect_exact();
                eval(
                    &mut loaded,
                    "(let ((i 0) (junk nil)) \
                       (while (< i 2000) \
                         (setq junk (cons (vector 'junk i) (cons (make-string 2 ?x) junk))) \
                         (setq i (1+ i))) \
                       nil)",
                );
            }
            neovm_eval::test_support::authority::set_gc_stress_for_test(&mut loaded, false);
            assert_eq!(
                eval(&mut loaded, &format!("(aref {owner} 2)")),
                vector,
                "{owner}"
            );
            assert_eq!(
                eval(&mut loaded, &format!("(aref {owner} 1)")),
                code,
                "{owner}"
            );
            assert_eq!(
                printed(
                    &mut loaded,
                    &format!(
                        "(list (aref {owner} 2) (append (aref {owner} 1) nil) (funcall {owner}))"
                    )
                ),
                expected,
                "{owner} gc_stress={gc_stress}"
            );
        }
    }
}
