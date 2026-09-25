//! Byte-code closure slot identity (P3.2 L4a). GNU keeps a closure as a
//! pseudovector (alloc.c `Fmake_byte_code`, `Fmake_closure`), so `aref` on
//! slot 1 (the code string) and slot 2 (the constants vector) returns the
//! closure's own objects: `eq` on every read, the code string shared by
//! `make-closure` instances and their prototype, a fresh constants vector
//! per instance. Neomacs keeps those objects beside the function it executes
//! (`ByteCodeObj::slot_objects`); these tests pin the Lisp view, the GC
//! reachability of the objects, and that they are never read by code (the
//! L4b divergence, identical on both tiers).
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;
use crate::tagged::header::ByteCodeSlotObject;

fn eval(ctx: &mut Context, src: &str) -> Value {
    let v = ctx.eval_str(src).unwrap_or_else(|e| panic!("{src}: {e:?}"));
    crate::emacs_core::eval::push_scratch_gc_root(v);
    v
}

fn printed(ctx: &mut Context, src: &str) -> String {
    crate::emacs_core::print::print_value(&eval(ctx, src))
}

/// `(lambda (x) (+ x V0))` as GNU compiles it: dup, constant 0, plus, return.
const ADDER: &str = "(make-byte-code 257 \"\\211\\300\\\\\\207\" [3] 3)";
/// `(lambda () (list V0 V1 tail))` over `[V0 V1 tail]`, with a docstring.
const PROTO: &str = "(make-byte-code 0 \"\\300\\301\\302E\\207\" [V0 V1 csi-tail] 3 \"Doc.\")";

#[test]
fn aref_slots_1_and_2_are_the_same_objects_on_every_read() {
    let mut ctx = Context::new();
    eval(&mut ctx, &format!("(setq csi-f {ADDER})"));
    assert_eq!(
        printed(
            &mut ctx,
            "(list (eq (aref csi-f 1) (aref csi-f 1)) (eq (aref csi-f 2) (aref csi-f 2)) \
             (aref csi-f 2) (append (aref csi-f 1) nil) \
             (multibyte-string-p (aref csi-f 1)) (aref csi-f 0) (aref csi-f 3))"
        ),
        "(t t [3] (137 192 92 135) nil 257 3)"
    );
    // The objects are the ones the function carries.
    let f = eval(&mut ctx, "csi-f");
    assert_eq!(
        f.bytecode_slot_object_if_created(ByteCodeSlotObject::Constants),
        Some(eval(&mut ctx, "(aref csi-f 2)"))
    );
    assert_eq!(
        f.bytecode_slot_object_if_created(ByteCodeSlotObject::Code),
        Some(eval(&mut ctx, "(aref csi-f 1)"))
    );
    // Out of range and negative indices still signal.
    assert_eq!(
        printed(
            &mut ctx,
            "(list (condition-case e (aref csi-f 4) (error (car e))) \
                   (condition-case e (aref csi-f -1) (error (car e))))"
        ),
        "(args-out-of-range args-out-of-range)"
    );
}

#[test]
fn slot_objects_are_created_lazily() {
    let mut ctx = Context::new();
    let f = eval(&mut ctx, ADDER);
    assert_eq!(
        f.bytecode_slot_object_if_created(ByteCodeSlotObject::Code),
        None
    );
    assert_eq!(
        f.bytecode_slot_object_if_created(ByteCodeSlotObject::Constants),
        None
    );
    // Calling and printing create nothing.
    ctx.set_variable("csi-lazy", f);
    assert_eq!(eval(&mut ctx, "(funcall csi-lazy 1)"), Value::fixnum(4));
    let _ = printed(&mut ctx, "(prin1-to-string csi-lazy)");
    assert_eq!(
        f.bytecode_slot_object_if_created(ByteCodeSlotObject::Constants),
        None
    );
    assert_eq!(
        f.bytecode_slot_object_if_created(ByteCodeSlotObject::Code),
        None
    );
}

#[test]
fn make_closure_instances_share_the_prototype_code_string() {
    for in_place in [true, false] {
        super::symbols::force_make_closure_in_place_for_test(in_place);
        let mut ctx = Context::new();
        eval(&mut ctx, &format!("(setq csi-proto {PROTO})"));
        // An instance made BEFORE anything read the prototype's slot 1 must
        // still share it, as must one made after, and an instance of an
        // instance.
        eval(&mut ctx, "(setq csi-early (make-closure csi-proto 1 'one))");
        eval(&mut ctx, "(setq csi-proto-code (aref csi-proto 1))");
        eval(&mut ctx, "(setq csi-late (make-closure csi-proto 2 'two))");
        assert_eq!(
            printed(
                &mut ctx,
                "(list (eq (aref csi-early 1) csi-proto-code) \
                       (eq (aref csi-late 1) csi-proto-code) \
                       (eq (aref (make-closure csi-early 3) 1) csi-proto-code) \
                       (eq (aref csi-early 2) (aref csi-early 2)) \
                       (eq (aref csi-early 2) (aref csi-late 2)) \
                       (eq (aref csi-early 2) (aref csi-proto 2)) \
                       (aref csi-early 2) (aref csi-late 2) (aref csi-proto 2) \
                       (funcall csi-early) (funcall csi-late))"
            ),
            "(t t t t nil nil [1 one csi-tail] [2 two csi-tail] [V0 V1 csi-tail] \
             (1 one csi-tail) (2 two csi-tail))",
            "in_place={in_place}"
        );
    }
    super::symbols::force_make_closure_in_place_for_test(true);
}

#[test]
fn sequence_functions_hand_out_the_slot_objects() {
    let mut ctx = Context::new();
    eval(&mut ctx, &format!("(setq csi-f {ADDER})"));
    assert_eq!(
        printed(
            &mut ctx,
            "(list (eq (nth 1 (append csi-f nil)) (aref csi-f 1)) \
                   (eq (nth 2 (append csi-f nil)) (aref csi-f 2)) \
                   (eq (aref (vconcat csi-f) 2) (aref csi-f 2)) \
                   (eq (nth 2 (mapcar #'identity csi-f)) (aref csi-f 2)) \
                   (length csi-f) (append csi-f nil))"
        ),
        "(t t t t 4 (257 \"\\211\\300\\\\\\207\" [3] 3))"
    );
}

#[test]
fn print_circle_sees_the_shared_slot_objects() {
    let mut ctx = Context::new();
    assert_eq!(
        printed(
            &mut ctx,
            &format!(
                "(let* ((f {ADDER}) (v (aref f 2)) (print-circle t)) \
                   (prin1-to-string (list v f (aref f 1))))"
            )
        ),
        "\"(#1=[3] #[257 #2=\\\"\\\\211\\\\300\\\\\\\\\\\\207\\\" #1# 3] #2#)\""
    );
}

/// The slot objects are reachable only through the function once Lisp drops
/// them: a collection must keep them (and the next read returns the same,
/// intact objects), with and without a collection at every safepoint.
#[test]
fn slot_objects_survive_collections_through_the_function() {
    for gc_stress in [false, true] {
        let mut ctx = Context::new();
        eval(&mut ctx, &format!("(setq csi-f {ADDER})"));
        let f = eval(&mut ctx, "csi-f");
        // Create both objects, then drop every Lisp reference to them.
        eval(&mut ctx, "(progn (aref csi-f 1) (aref csi-f 2) nil)");
        let vector = f
            .bytecode_slot_object_if_created(ByteCodeSlotObject::Constants)
            .expect("created");
        let code = f
            .bytecode_slot_object_if_created(ByteCodeSlotObject::Code)
            .expect("created");
        ctx.gc_stress = gc_stress;
        for _ in 0..3 {
            ctx.gc_collect_exact();
            // Reuse whatever a sweep freed: a swept slot object would now
            // hold one of these.
            eval(
                &mut ctx,
                "(let ((i 0) (junk nil)) \
                   (while (< i 2000) \
                     (setq junk (cons (vector 'junk i) (cons (make-string 4 ?x) junk))) \
                     (setq i (1+ i))) \
                   nil)",
            );
        }
        ctx.gc_stress = false;
        assert_eq!(eval(&mut ctx, "(aref csi-f 2)"), vector);
        assert_eq!(eval(&mut ctx, "(aref csi-f 1)"), code);
        assert_eq!(
            printed(
                &mut ctx,
                "(list (aref csi-f 2) (append (aref csi-f 1) nil) (funcall csi-f 1))"
            ),
            "([3] (137 192 92 135) 4)",
            "gc_stress={gc_stress}"
        );
    }
}

/// Expected divergence until P3.2 L4b: GNU runs the function from the vector
/// `aref` returns, so an `aset` into it changes the result to 100. Neomacs
/// never reads the slot object, on either tier: the interpreter reads its
/// own pool and the JIT bakes constants (or, for a `make-closure` instance,
/// reads the captured prefix from that same pool). Both tiers must agree.
#[test]
fn aset_into_the_constants_vector_is_not_seen_by_either_tier() {
    for jit in [false, true] {
        for source in [
            ADDER,
            "(make-closure (make-byte-code 257 \"\\211\\300\\\\\\207\" [V0] 3) 3)",
        ] {
            let mut ctx = Context::new();
            eval(&mut ctx, &format!("(setq csi-f {source})"));
            let f = eval(&mut ctx, "csi-f");
            let bc = f.get_bytecode_data().expect("byte code");
            #[cfg(feature = "jit")]
            {
                if jit {
                    bc.jit_runtime().set_hot_for_test();
                } else {
                    bc.jit_runtime().set_cold_for_test();
                }
            }
            assert_eq!(eval(&mut ctx, "(funcall csi-f 1)"), Value::fixnum(4));
            #[cfg(feature = "jit")]
            assert_eq!(
                bc.jit_runtime()
                    .compiled_id()
                    .is_some_and(crate::emacs_core::jit::cache::is_compiled_for_test),
                jit,
                "{source}: the function must run on the tier under test"
            );
            assert_eq!(
                printed(
                    &mut ctx,
                    "(let ((c (aref csi-f 2))) \
                       (aset c 0 99) \
                       (list (aref (aref csi-f 2) 0) (eq c (aref csi-f 2)) (funcall csi-f 1)))"
                ),
                "(99 t 4)",
                "jit={jit} {source}"
            );
            #[cfg(not(feature = "jit"))]
            let _ = (jit, bc);
        }
    }
}
