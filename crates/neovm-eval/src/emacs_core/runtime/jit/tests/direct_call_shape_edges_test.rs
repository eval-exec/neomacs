//! Boundary cases for optional/rest direct sites, using the parent module's
//! GNU fixture helper and fresh-thread harness. This module is a test-only
//! child of `direct_call_shape_parity`.

use super::*;

/// A noneligible site still calls its native callee through the spec shim.
/// Both runs use the same caller body and differ only in the one shape bit.
fn assert_shape_falls_back(
    name: &str,
    shape: &str,
    program: &'static str,
    observe: &'static str,
    fixture: &str,
) {
    let expected = gnu_expect(name, program, observe, fixture);
    let baseline = run_in_with(Mode::Direct, DirectShapesKnob::OFF, program, observe);
    let shaped = run_in_with(
        Mode::Direct,
        DirectShapesKnob::parse(Some(shape)),
        program,
        observe,
    );
    assert_eq!(baseline.out, expected, "GNU {name}, exact-only");
    assert_eq!(shaped.out, expected, "GNU {name}, {shape}");
    assert!(baseline.shim_calls > 0, "fallback calls the spec shim");
    assert_eq!(
        shaped.shim_calls, baseline.shim_calls,
        "{name} keeps its shim fallback with {shape} on"
    );
}

/// Pin each eligible boundary separately: a combined observation could
/// otherwise bypass the shim for zero arguments while missing eight.
fn assert_rest_boundary_hits(
    name: &str,
    program: &'static str,
    observe: &'static str,
    fixture: &str,
) {
    let expected = gnu_expect(name, program, observe, fixture);
    let baseline = run_in_with(Mode::Direct, DirectShapesKnob::OFF, program, observe);
    let shaped = run_in_with(
        Mode::Direct,
        DirectShapesKnob::parse(Some("rest")),
        program,
        observe,
    );
    assert_eq!(baseline.out, expected, "GNU {name}, exact-only");
    assert_eq!(shaped.out, expected, "GNU {name}, rest");
    assert!(
        shaped.shim_calls < baseline.shim_calls,
        "{name} independently bypasses the spec shim"
    );
}

#[test]
fn gnu_zero_argument_optional_calls_keep_zero_nargs_and_wide_callees_fall_back() {
    const PROGRAM: &str = r#"(progn
      (defvar neovm--sz-arm nil)
      (defun neovm--sz-show ()
        (let (mapped)
          (mapbacktrace
           (lambda (evald f args flags)
             (when (eq f 'neovm--sz-opt)
               (setq mapped (list evald f args flags))))
           'neovm--sz-show)
          (list mapped (backtrace-frame 1 'neovm--sz-show))))
      (defun neovm--sz-opt (&optional a b)
        (if neovm--sz-arm (neovm--sz-show) (list a b)))
      (defun neovm--sz-opt-caller () (neovm--sz-opt))
      (defun neovm--sz-wide (&optional a b c d e f g) (list a b c d e f g))
      (defun neovm--sz-wide-caller () (neovm--sz-wide))
      (dolist (f '(neovm--sz-show neovm--sz-opt neovm--sz-opt-caller
                   neovm--sz-wide neovm--sz-wide-caller)) (byte-compile f))
      (dotimes (_ neovm--dc-warm)
        (neovm--sz-opt-caller) (neovm--sz-wide-caller)))"#;
    assert_gnu_shape(
        "optional-zero",
        "optional",
        PROGRAM,
        r#"(list (neovm--sz-opt-caller)
          (let ((neovm--sz-arm t)) (neovm--sz-opt-caller))
          (neovm--sz-wide-caller) (neovm--sz-opt-caller))"#,
        include_str!("direct_call_shape_parity/optional-zero.expect"),
        true,
    );
    assert_shape_falls_back(
        "optional-wide",
        "optional",
        PROGRAM,
        "(neovm--sz-wide-caller)",
        include_str!("direct_call_shape_parity/optional-wide.expect"),
    );
}

#[test]
fn gnu_rest_zero_and_eight_args_enter_directly_but_nine_and_wide_callees_fall_back() {
    const PROGRAM: &str = r#"(progn
      (defvar neovm--se-arm nil)
      (defun neovm--se-show ()
        (let (mapped)
          (mapbacktrace
           (lambda (evald f args flags)
             (when (eq f 'neovm--se-rest)
               (setq mapped (list evald f args flags))))
           'neovm--se-show)
          (list mapped (backtrace-frame 1 'neovm--se-show))))
      (defun neovm--se-rest (&rest tail)
        (if neovm--se-arm (neovm--se-show)
          (when tail (setcar tail (list (car tail)))) tail))
      (defun neovm--se-zero () (neovm--se-rest))
      (defun neovm--se-eight () (neovm--se-rest 1 2 3 4 5 6 7 8))
      (defun neovm--se-nine () (neovm--se-rest 1 2 3 4 5 6 7 8 9))
      (defun neovm--se-wide (a b c d e f &rest tail) (list a b c d e f tail))
      (defun neovm--se-wide-caller () (neovm--se-wide 1 2 3 4 5 6 7 8))
      (dolist (f '(neovm--se-show neovm--se-rest neovm--se-zero
                   neovm--se-eight neovm--se-nine neovm--se-wide
                   neovm--se-wide-caller)) (byte-compile f))
      (dotimes (_ neovm--dc-warm)
        (neovm--se-zero) (neovm--se-eight) (neovm--se-nine)
        (neovm--se-wide-caller)))"#;
    assert_gnu_shape(
        "rest-edges",
        "rest",
        PROGRAM,
        r#"(list (neovm--se-zero) (neovm--se-eight) (neovm--se-nine)
          (let ((neovm--se-arm t))
            (list (neovm--se-zero) (neovm--se-eight) (neovm--se-nine)))
          (let ((first (neovm--se-eight)) (second (neovm--se-eight)))
            (list first second (eq first second)))
          (neovm--se-wide-caller))"#,
        include_str!("direct_call_shape_parity/rest-edges.expect"),
        true,
    );
    assert_shape_falls_back(
        "rest-nine",
        "rest",
        PROGRAM,
        "(neovm--se-nine)",
        include_str!("direct_call_shape_parity/rest-nine.expect"),
    );
    assert_rest_boundary_hits(
        "rest-zero",
        PROGRAM,
        "(neovm--se-zero)",
        include_str!("direct_call_shape_parity/rest-zero.expect"),
    );
    assert_rest_boundary_hits(
        "rest-eight",
        PROGRAM,
        "(neovm--se-eight)",
        include_str!("direct_call_shape_parity/rest-eight.expect"),
    );
    assert_shape_falls_back(
        "rest-wide",
        "rest",
        PROGRAM,
        "(neovm--se-wide-caller)",
        include_str!("direct_call_shape_parity/rest-wide.expect"),
    );
}
