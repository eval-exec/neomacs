//! Optimization equivalence and explicit refusal at the real fontification seam.
//! No process environment or TLS is mutated. GNU-derived property semantics
//! remain covered by textprop_test's fallback/LIMIT oracle; these cases compare
//! the new admitted path to that unchanged implementation in the same Context.

use super::*;
use crate::emacs_core::Value;
use crate::emacs_core::error::EvalResult;
use crate::emacs_core::xdisp::{
    ensure_fontified_for_redisplay_with_mode, next_fontified_property_change,
    next_unfontified_position,
};

fn generic(ctx: &Context, pos: i64, end: i64) -> EvalResult {
    crate::emacs_core::textprop::builtin_next_single_property_change_in_state(
        ctx.variables(),
        ctx.buffer_manager(),
        &[
            Value::fixnum(pos),
            Value::symbol("fontified"),
            Value::NIL,
            Value::fixnum(end),
        ],
    )
}

fn assert_admitted(ctx: &mut Context, pos: i64, end: i64) {
    let expected = generic(ctx, pos, end).expect("unchanged generic boundary");
    let FontificationBoundaryAdmission::Raw(actual) =
        admit(ctx, pos, end, FontificationBoundaryMode::On).expect("raw query")
    else {
        panic!("expected a proven exact raw boundary at {pos}..{end}");
    };
    assert_eq!(Value::fixnum(actual.as_i64()), expected);
    assert_eq!(
        next_fontified_property_change(ctx, pos, end, FontificationBoundaryMode::On)
            .expect("admitted caller"),
        expected,
    );
    assert_eq!(
        admit(ctx, pos, end, FontificationBoundaryMode::Off).expect("OFF refuses"),
        FontificationBoundaryAdmission::UseGeneric,
    );
}

#[test]
fn fontification_boundary_policy_defaults_on_and_keeps_explicit_off() {
    assert_eq!(parse(None), FontificationBoundaryMode::On);
    for value in ["on", "1", "true", "yes", " ON "] {
        assert_eq!(
            parse(Some(OsStr::new(value))),
            FontificationBoundaryMode::On
        );
    }
    for value in ["off", "0", "false", "no", "", "unknown"] {
        assert_eq!(
            parse(Some(OsStr::new(value))),
            FontificationBoundaryMode::Off
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        assert_eq!(
            parse(Some(OsStr::from_bytes(b"\xff"))),
            FontificationBoundaryMode::Off,
        );
    }
}

#[test]
fn fontification_boundary_dense_unrelated_splits_preserve_all_representations() {
    for (mode, text) in [
        ("nil", "(string-as-unibyte (make-string 64 ?a))"),
        ("nil", "(unibyte-string 233 233 233 233 233 233 233 233)"),
        ("t", "(string-to-multibyte (make-string 64 ?a))"),
        ("t", "\"aé中λbcdé中λ\""),
    ] {
        for with_props in [false, true] {
            let mut ctx = Context::new();
            ctx.eval_str(&format!(
                "(progn (set-buffer-multibyte {mode}) (insert {text}))"
            ))
            .expect("insert representation");
            if with_props {
                ctx.eval_str(
                    "(progn
                       (put-text-property 1 (point-max) 'fontified t)
                       (let ((p 1))
                         (while (< p (point-max))
                           (put-text-property p (1+ p) 'face
                                              (if (= (% p 2) 0) 'bold 'italic))
                           (setq p (1+ p)))))",
                )
                .expect("dense unrelated face splits");
            }
            let end = ctx
                .buffer_manager()
                .current_buffer()
                .expect("buffer")
                .point_max_lisp_char_pos()
                .as_i64();
            for pos in 1..end {
                for limit in [pos + 1, end] {
                    assert_admitted(&mut ctx, pos, limit);
                }
            }
        }
    }
}

#[test]
fn fontification_boundary_nil_holes_limits_narrowing_and_trailing_nil_match_generic() {
    let mut ctx = Context::new();
    ctx.eval_str(
        "(progn (insert \"abcdefghij\")
           (put-text-property 1 4 'fontified t)
           (put-text-property 4 6 'fontified nil)
           (put-text-property 6 9 'fontified t)
           (put-text-property 2 3 'face 'bold))",
    )
    .expect("property runs");
    for pos in 1..11 {
        for end in pos + 1..=11 {
            assert_admitted(&mut ctx, pos, end);
        }
    }
    ctx.eval_str("(narrow-to-region 3 9)")
        .expect("narrow source");
    for pos in 3..9 {
        for end in pos + 1..=9 {
            assert_admitted(&mut ctx, pos, end);
        }
    }
    for (pos, end) in [(0, 8), (2, 8), (3, 10), (4, 4), (9, 10), (10, 9)] {
        assert_eq!(
            admit(&ctx, pos, end, FontificationBoundaryMode::On).expect("range refuses"),
            FontificationBoundaryAdmission::UseGeneric
        );
    }
    ctx.eval_str("(narrow-to-region 4 4)")
        .expect("empty accessible region");
    assert_eq!(
        admit(&ctx, 4, 4, FontificationBoundaryMode::On).expect("empty refuses"),
        FontificationBoundaryAdmission::UseGeneric
    );
}

#[test]
fn fontification_boundary_fallbacks_and_conservative_category_presence_refuse() {
    for setup in [
        "(progn (put 'fb-cat 'fontified t) (put-text-property 2 5 'category 'fb-cat))",
        "(progn (put-text-property 2 5 'category 'fb-cat) (remove-text-properties 2 5 '(category nil)))",
        "(progn (set (make-local-variable 'char-property-alias-alist) '((fontified fb-alias))) (put-text-property 2 5 'fb-alias t))",
        "(set (make-local-variable 'default-text-properties) '(fontified t))",
        "(set (make-local-variable 'default-text-properties) '(fontified nil))",
    ] {
        let mut ctx = Context::new();
        ctx.eval_str("(insert \"abcdefgh\")").expect("source");
        ctx.eval_str(setup).expect("fallback source");
        let expected = generic(&ctx, 1, 9).expect("generic fallback");
        let FontificationBoundaryAdmission::Resolved(next) =
            admit(&ctx, 1, 9, FontificationBoundaryMode::On).expect("resolved query")
        else {
            panic!("expected raw refusal with shared resolved scan: {setup}");
        };
        assert_eq!(Value::fixnum(next.as_i64()), expected, "{setup}");
        assert_eq!(
            next_fontified_property_change(&mut ctx, 1, 9, FontificationBoundaryMode::On)
                .expect("refused caller"),
            expected,
            "{setup}"
        );
    }
}

fn observe_ensure(mode: FontificationBoundaryMode, setup: &str) -> (String, String, String) {
    let mut ctx = Context::new();
    ctx.eval_str("(progn (insert \"abcdefghij\") (setq fb-calls nil))")
        .expect("initial source");
    ctx.eval_str(setup).expect("fontification hooks");
    let target = ctx.buffer_manager().current_buffer_id().expect("target");
    let other = ctx.buffer_manager_mut().create_buffer("*fb-other*");
    ctx.set_current_buffer_unrecorded(other)
        .expect("saved current buffer");
    let result = ensure_fontified_for_redisplay_with_mode(&mut ctx, target, 0, 10, mode);
    let outcome = match result {
        Ok(outcome) => format!("{outcome:?}"),
        Err(flow) => crate::emacs_core::error::format_flow_with_eval(&mut ctx, &flow),
    };
    let current = ctx
        .buffer_manager()
        .current_buffer()
        .expect("restored buffer")
        .name_runtime_string_owned();
    ctx.set_current_buffer_unrecorded(target)
        .expect("inspect target");
    let value = ctx
        .eval_str(
            "(list fb-calls (point-min) (point-max)
           (let ((p (point-min)) (values nil))
             (while (< p (point-max))
               (setq values (cons (list (get-char-property p 'fontified)
                                        (get-char-property p 'face)) values))
               (setq p (1+ p)))
             values))",
        )
        .expect("observe callbacks and final source");
    (
        outcome,
        current,
        crate::emacs_core::print::print_value(&value),
    )
}

#[test]
fn fontification_overlay_boundary_ignores_endpoints_outside_live_restriction() {
    let mut ctx = Context::new();
    ctx.eval_str(
        "(progn (insert \"abcdefghij\")
           (put-text-property 1 11 'fontified t)
           (let ((o (make-overlay 8 10))) (overlay-put o 'fontified t))
           (narrow-to-region 1 5))",
    )
    .expect("callback-style narrowed source");
    for mode in [
        FontificationBoundaryMode::On,
        FontificationBoundaryMode::Off,
    ] {
        assert_eq!(
            next_fontified_property_change(&mut ctx, 1, 11, mode)
                .expect("stale limit retains generic exhaustion semantics"),
            Value::fixnum(11),
        );
    }
}

#[test]
fn fontification_boundary_real_ensure_preserves_marking_and_overlay_callback_order() {
    for setup in [
        "(setq fontification-functions
           (list (lambda (p) (setq fb-calls (cons p fb-calls))
                   (put-text-property p (min (point-max) (+ p 3)) 'fontified t)
                   (put-text-property p (min (point-max) (+ p 3)) 'face 'bold))))",
        "(progn (let ((o (make-overlay 1 5))) (overlay-put o 'fontified t))
           (setq fontification-functions
             (list (lambda (p) (setq fb-calls (cons p fb-calls))
                     (put-text-property p (min (point-max) (+ p 2)) 'fontified t)))))",
    ] {
        assert_eq!(
            observe_ensure(FontificationBoundaryMode::On, setup),
            observe_ensure(FontificationBoundaryMode::Off, setup),
            "{setup}"
        );
    }
}

#[test]
fn fontification_boundary_real_ensure_refreshes_proofs_after_callback_changes() {
    for setup in [
        "(setq fontification-functions
           (list (lambda (p) (setq fb-calls (cons p fb-calls))
                   (put-text-property p (min (point-max) (+ p 2)) 'fontified t)
                   (set (make-local-variable 'char-property-alias-alist) '((fontified fb-alias)))
                   (put-text-property 6 9 'fb-alias t))))",
        "(setq fontification-functions
           (list (lambda (p) (setq fb-calls (cons p fb-calls))
                   (put-text-property p (min (point-max) (+ p 2)) 'fontified t)
                   (set (make-local-variable 'default-text-properties) '(fontified t)))))",
        "(setq fontification-functions
           (list (lambda (p) (setq fb-calls (cons p fb-calls))
                   (put-text-property p (min (point-max) (+ p 2)) 'fontified t)
                   (put 'fb-cat 'fontified t)
                   (put-text-property 6 9 'category 'fb-cat))))",
        "(setq fontification-functions
           (list (lambda (p) (setq fb-calls (cons p fb-calls))
                   (put-text-property p (point-max) 'fontified t)
                   (narrow-to-region 1 7))))",
        "(setq fontification-functions
           (list (lambda (p) (setq fb-calls (cons p fb-calls))
                   (put-text-property p (min (point-max) (+ p 3)) 'fontified t)
                   (set-buffer (get-buffer-create \"*fb-switch*\")))))",
    ] {
        assert_eq!(
            observe_ensure(FontificationBoundaryMode::On, setup),
            observe_ensure(FontificationBoundaryMode::Off, setup),
            "{setup}"
        );
    }
}

#[test]
fn fontification_boundary_real_ensure_keeps_nonmarking_and_error_hook_observations() {
    for setup in [
        "(setq fontification-functions (list (lambda (p) (setq fb-calls (cons p fb-calls)))))",
        "(setq fontification-functions
           (list (lambda (p) (setq fb-calls (cons (cons 'error p) fb-calls))
                   (error \"fontifier probe\"))
                 (lambda (p) (setq fb-calls (cons (cons 'mark p) fb-calls))
                   (put-text-property p (min (point-max) (+ p 3)) 'fontified t))))",
    ] {
        assert_eq!(
            observe_ensure(FontificationBoundaryMode::On, setup),
            observe_ensure(FontificationBoundaryMode::Off, setup),
            "{setup}"
        );
    }
}

#[test]
fn fontification_scan_stops_at_effective_overlay_nil_islands() {
    // GNU 31.1: taskset -c 0-3,14-31 bash
    // tmp/vmcall/fontadm-oracle/run-gnu.sh --out
    // tmp/vmcall/fontadm-oracle/capture-r1 --release (main sandbox).
    // Captured stdout SHA256 f120875cc51de9fda48cf75aa025475d6cf0d8677743694428204093f2db5171:
    // truthy-overlay-prefix-nil-text-tail first nil=5; a nil overlay over
    // truthy text has no nil stop. These are literals, never refreshed here.
    for mode in [
        FontificationBoundaryMode::On,
        FontificationBoundaryMode::Off,
    ] {
        let mut ctx = Context::new();
        ctx.eval_str(
            "(progn (insert \"aé中λbcdefg\")
               (let ((a (make-overlay 1 5)) (b (make-overlay 7 9))
                     (face (make-overlay 2 4)))
                 (overlay-put a 'fontified t)
                 (overlay-put b 'fontified 'defer)
                 (overlay-put face 'face 'bold)))",
        )
        .unwrap();
        for (from, end, expected) in [(1, 11, 5), (2, 4, 4), (5, 11, 5), (7, 11, 9)] {
            assert_eq!(
                next_unfontified_position(&mut ctx, from, end, mode).unwrap(),
                expected,
                "{mode:?}, {from}..{end}"
            );
        }
        // Nil overlay values do not hide a truthy text value.
        ctx.eval_str(
            "(progn (put-text-property 1 (point-max) 'fontified t)
               (let ((o (make-overlay 3 8))) (overlay-put o 'fontified nil)))",
        )
        .unwrap();
        assert_eq!(
            next_unfontified_position(&mut ctx, 1, 11, mode).unwrap(),
            11
        );
    }
}

#[test]
fn fontification_scan_overlay_prefix_does_not_skip_hooks_in_nil_tail() {
    for mode in [
        FontificationBoundaryMode::On,
        FontificationBoundaryMode::Off,
    ] {
        let observed = observe_ensure(
            mode,
            "(progn
               (let ((o (make-overlay 1 5))) (overlay-put o 'fontified t))
               (setq fontification-functions
                 (list (lambda (p) (setq fb-calls (cons p fb-calls))
                         (put-text-property p (min (point-max) (+ p 2))
                                            'fontified t)))))",
        );
        assert_eq!(observed.0, "Fontified");
        assert!(observed.2.starts_with("((9 7 5) "), "{observed:?}");
    }
}

#[test]
fn fontification_scan_restarts_after_hook_replaces_a_later_island() {
    for mode in [
        FontificationBoundaryMode::On,
        FontificationBoundaryMode::Off,
    ] {
        let observed = observe_ensure(
            mode,
            "(progn (put-text-property 3 9 'fontified t)
               (setq fontification-functions
                 (list (lambda (p) (setq fb-calls (cons p fb-calls))
                         (put-text-property p (1+ p) 'fontified t)
                         (if (= p 1)
                           (progn (put-text-property 4 5 'fontified nil)
                             (let ((o (make-overlay 9 11)))
                               (overlay-put o 'fontified t))))))))",
        );
        assert_eq!(observed.0, "Fontified");
        assert!(observed.2.starts_with("((4 2 1) "), "{observed:?}");
    }
}

#[test]
fn fontification_overlay_first_stop_matches_independent_text_and_overlay_queries() {
    for text in [
        "(make-string 96 ?a)",
        "(apply #'concat (make-list 32 \"aé中\"))",
    ] {
        let mut ctx = Context::new();
        ctx.eval_str(&format!(
            "(progn (insert {text})
               (put-text-property 1 (point-max) 'fontified t)
               (put-text-property 77 83 'fontified nil)
               (let ((p 1))
                 (while (< p (point-max))
                   (put-text-property p (1+ p) 'face
                                      (if (= (% p 2) 0) 'bold 'italic))
                   (setq p (1+ p))))
               (let ((p 3))
                 (while (< p 90)
                   (let ((o (make-overlay p (+ p 3))))
                     (overlay-put o 'face 'bold))
                   (setq p (+ p 7)))))"
        ))
        .unwrap();
        for restriction in ["nil", "(narrow-to-region 5 91)"] {
            ctx.eval_str(restriction).unwrap();
            let buffer = ctx.buffer_manager().current_buffer().unwrap();
            let min = buffer.point_min_lisp_char_pos().as_i64();
            let max = buffer.point_max_lisp_char_pos().as_i64();
            for pos in min..max {
                for limit in [pos + 1, max, max + 7] {
                    // The public text-only query and independent public
                    // overlay query reproduce the original stop contract.
                    let text_stop = generic(&ctx, pos, limit).unwrap().as_int().unwrap();
                    let overlay_stop = ctx
                        .eval_str(&format!("(next-overlay-change {pos})"))
                        .unwrap()
                        .as_int()
                        .unwrap();
                    let expected = if overlay_stop < max {
                        text_stop.min(overlay_stop)
                    } else {
                        text_stop
                    };
                    for mode in [
                        FontificationBoundaryMode::On,
                        FontificationBoundaryMode::Off,
                    ] {
                        assert_eq!(
                            next_fontified_property_change(&mut ctx, pos, limit, mode).unwrap(),
                            Value::fixnum(expected),
                            "{text}: {pos}..{limit}"
                        );
                    }
                }
            }
        }
    }
}
