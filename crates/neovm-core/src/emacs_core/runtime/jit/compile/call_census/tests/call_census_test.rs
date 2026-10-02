//! The call-shape census (`NEOVM_JIT_CALL_CENSUS`): every shape a JIT call
//! site meets lands in its own cell, and the knob off counts nothing.

use super::*;

/// Callees of every shape, called from a compiled caller: a named exact,
/// `&optional` (short), `&rest` and framed callee, a `cl-flet` local (a
/// constant byte-code callee, called twice so the optimizer keeps it a
/// function; its body calls, so the fuser leaves it a call), and an `apply`
/// of a compiled function (its list a variable, which the optimizer cannot
/// spread at compile time).
const PROGRAM: &str = r#"(progn
  (require 'cl-lib)
  (defvar neovm--cc-special nil)
  (defvar neovm--cc-tail '(3))
  (defun neovm--cc-exact (a b) (if (> a b) (- a b) (+ a b)))
  (defun neovm--cc-opt (a &optional b) (list a b))
  (defun neovm--cc-rest (a &rest r) (cons a r))
  (defun neovm--cc-framed (a) (let ((neovm--cc-special a)) (1+ neovm--cc-special)))
  (defun neovm--cc-applied (a b) (if a (list a b) b))
  (defun neovm--cc-caller (x)
    (list (neovm--cc-exact x 1) (neovm--cc-opt x) (neovm--cc-rest x 1 2)
          (neovm--cc-framed x)
          (cl-flet ((g (y) (list (neovm--cc-exact y 2))))
            (list (g x) (g (1+ x))))
          (apply #'neovm--cc-applied x neovm--cc-tail)))
  (dolist (f '(neovm--cc-exact neovm--cc-opt neovm--cc-rest neovm--cc-framed
               neovm--cc-applied neovm--cc-caller))
    (byte-compile f))
  (dotimes (i 1500) (neovm--cc-caller i)))"#;

/// The census delta of running PROGRAM's caller 100 times once warm, with
/// the census knob as given.
fn census_delta(on: bool) -> Vec<(&'static str, &'static str, u64)> {
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            crate::test_utils::init_test_tracing();
            force_profit_gate_for_test(false);
            crate::emacs_core::jit::inline::force_inline_for_test(Some(true));
            crate::emacs_core::jit::force_profit_defer_for_test(Some(1));
            force_direct_call_for_test(Some(false));
            force_direct_shapes_for_test(Some(DirectShapesKnob::OFF));
            force_call_census_for_test(Some(on));
            let mut ev = crate::test_utils::runtime_startup_context();
            ev.eval_str(PROGRAM).expect("warmed");
            let before = census_counts();
            ev.eval_str("(dotimes (i 100) (neovm--cc-caller i))")
                .expect("runs");
            let after = census_counts();
            force_call_census_for_test(None);
            force_direct_shapes_for_test(None);
            force_direct_call_for_test(None);
            crate::emacs_core::jit::inline::force_inline_for_test(None);
            crate::emacs_core::jit::force_profit_defer_for_test(None);
            after
                .into_iter()
                .map(|(site, shape, n)| {
                    let was = before
                        .iter()
                        .find(|(s, h, _)| *s == site && *h == shape)
                        .map_or(0, |(_, _, n)| *n);
                    (site, shape, n - was)
                })
                .filter(|(_, _, n)| *n > 0)
                .collect()
        })
        .expect("spawn")
        .join()
        .expect("no crash")
}

fn calls(delta: &[(&str, &str, u64)], site: &str, shape: &str) -> u64 {
    delta
        .iter()
        .find(|(s, h, _)| *s == site && *h == shape)
        .map_or(0, |(_, _, n)| *n)
}

#[test]
fn the_census_counts_each_callee_shape_in_its_own_cell() {
    let delta = census_delta(true);
    // The caller's named sites, once each per call of the caller.
    assert!(
        calls(&delta, "named", "exact") + calls(&delta, "named", "exact_direct") >= 100,
        "{delta:?}"
    );
    assert!(calls(&delta, "named", "optional") >= 100, "{delta:?}");
    assert!(calls(&delta, "named", "rest") >= 100, "{delta:?}");
    assert!(calls(&delta, "named", "framed") >= 100, "{delta:?}");
    // The `cl-flet` local is a constant callee the fuser leaves a call.
    assert!(calls(&delta, "const", "exact") >= 200, "{delta:?}");
    // `apply` of a compiled exact-arity function.
    assert!(calls(&delta, "apply", "exact") >= 100, "{delta:?}");
}

#[test]
fn the_census_knob_off_counts_nothing() {
    assert_eq!(census_delta(false), Vec::new());
}

/// Op::Apply's function slot may be a symbol or an object. A Call of
/// `apply` instead keeps the applied function in its first argument.
#[test]
fn the_census_counts_apply_opcode_and_builtin_targets() {
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            crate::test_utils::init_test_tracing();
            force_profit_gate_for_test(false);
            crate::emacs_core::jit::inline::force_inline_for_test(Some(true));
            crate::emacs_core::jit::force_profit_defer_for_test(Some(1));
            force_direct_call_for_test(Some(false));
            force_direct_shapes_for_test(Some(DirectShapesKnob::OFF));
            force_call_census_for_test(Some(true));
            let mut ev = crate::test_utils::runtime_startup_context();
            ev.eval_str(PROGRAM).expect("warmed");
            let symbol = Value::symbol("neovm--cc-applied");
            let object = ev
                .obarray
                .symbol_function_id(symbol.as_symbol_id().unwrap())
                .unwrap();
            let tail = Value::list_from_slice(&[Value::make_int(2), Value::make_int(3)]);
            let before = calls(&census_counts(), "apply", "exact");
            // Exercise the actual emission, not only the classification shim.
            for callee in [symbol, object] {
                let leaf = lower_nullary_leaf(
                    &[Op::Constant(0), Op::Constant(1), Op::Apply(1), Op::Return],
                    &[callee, tail],
                )
                .expect("apply opcode compiles");
                assert!(matches!(
                    leaf.call(core::ptr::from_mut(&mut ev).cast(), &[]),
                    NativeRun::Ok(_)
                ));
            }
            assert_eq!(calls(&census_counts(), "apply", "exact") - before, 2);

            let before = calls(&census_counts(), "apply", "exact");
            for callee in [symbol, object] {
                let leaf = lower_nullary_leaf(
                    &[
                        Op::Constant(0),
                        Op::Constant(1),
                        Op::Constant(2),
                        Op::Call(2),
                        Op::Return,
                    ],
                    &[Value::symbol("apply"), callee, tail],
                )
                .expect("apply builtin compiles");
                assert!(matches!(
                    leaf.call(core::ptr::from_mut(&mut ev).cast(), &[]),
                    NativeRun::Ok(_)
                ));
            }
            assert_eq!(calls(&census_counts(), "apply", "exact") - before, 2);
            force_call_census_for_test(None);
            force_direct_shapes_for_test(None);
            force_direct_call_for_test(None);
            crate::emacs_core::jit::inline::force_inline_for_test(None);
            crate::emacs_core::jit::force_profit_defer_for_test(None);
        })
        .expect("spawn")
        .join()
        .expect("no crash");
}

/// A sidecar changes the native entry protocol; it does not add dynamic
/// bindings or handlers, so it must not inflate Stage 2c's framed share.
#[test]
fn the_census_does_not_count_a_sidecar_as_a_lisp_frame() {
    crate::test_utils::init_test_tracing();
    force_direct_call_for_test(Some(false));
    force_direct_shapes_for_test(Some(DirectShapesKnob::OFF));
    let mut leaf = lower_nullary_leaf(&[Op::Nil, Op::Return], &[]).expect("leaf");
    leaf.sidecar = Some(Box::new(LeafSidecar {
        reloc_base: core::ptr::null(),
        spill_base: core::ptr::null(),
        meta_pc: core::ptr::null(),
        meta_depth: core::ptr::null(),
        meta_handlers: core::ptr::null(),
        spec_slot_base: core::ptr::null(),
        spec_expected_base: core::ptr::null(),
    }));
    assert!(!leaf.direct_call_eligible());
    let slot = SpecSlot::at_epoch(0);
    let consts = [Value::NIL];
    slot.arm_leaf(&leaf, consts.as_ptr(), false, true, false);
    assert_eq!(named_shape(&slot), CallShape::Exact);
    leaf.has_binds = true;
    assert_eq!(named_shape(&slot), CallShape::Framed);
    leaf.has_binds = false;
    leaf.has_handlers = true;
    assert_eq!(named_shape(&slot), CallShape::Framed);
    force_direct_shapes_for_test(None);
    force_direct_call_for_test(None);
}
