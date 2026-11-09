//! Misc rows that load the minimal GNU backquote runtime.
//!
//! `load_minimal_gnu_backquote_runtime` is minted in core. The bare harness
//! is core's `new_context`: eval's unit runtime stays `cfg(test)`. Direct
//! builtin rows stay in `misc_test.rs`. Assertions are unchanged.

use neovm_value::value::{Value, eq_value};

fn bootstrap_eval(src: &str) -> Vec<String> {
    let mut ev = crate::test_utils::new_context();
    crate::test_utils::load_minimal_gnu_backquote_runtime(&mut ev);
    ev.eval_str_each(src)
        .iter()
        .map(crate::emacs_core::print::format_eval_result)
        .collect()
}

// ----- assoc-default -----

#[test]
fn assoc_default_bootstrap_matches_gnu_subr() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval(
        r#"
        (subrp (symbol-function 'assoc-default))
        (assoc-default "key" '(("key" . 42)))
        (assoc-default "missing" '(("key" . 42)) nil -1)
        (assoc-default 'foo '((foo . 10)) 'eq)
        (assoc-default 'foo '(foo) nil 'fallback)
        "#,
    );
    assert_eq!(results[0], "OK nil");
    assert_eq!(results[1], "OK 42");
    assert_eq!(results[2], "OK nil");
    assert_eq!(results[3], "OK 10");
    assert_eq!(results[4], "OK fallback");
}

#[test]
fn assoc_default_bootstrap_error_shapes_match_gnu_subr() {
    crate::test_utils::init_test_tracing();
    let results = bootstrap_eval(
        r#"
        (condition-case err
            (assoc-default 'foo 1)
          (wrong-type-argument (list (car err) (nth 1 err))))
        (condition-case err
            (assoc-default 'foo '((foo . 10)) 1)
          (error (car err)))
        "#,
    );
    assert_eq!(results[0], "OK (wrong-type-argument listp)");
    assert_eq!(results[1], "OK invalid-function");
}

// ----- with-syntax-table (Elisp macro in GNU subr.el:6394) -----

#[test]
fn sf_with_syntax_table_evaluates_body() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::new_context();
    crate::test_utils::load_minimal_gnu_backquote_runtime(&mut ev);
    let result = ev
        .eval_str("(with-syntax-table (make-syntax-table) 30)")
        .expect("eval");
    assert!(eq_value(&result, &Value::fixnum(30)));
}

#[test]
fn sf_with_syntax_table_restores_original_table_on_success() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::new_context();
    crate::test_utils::load_minimal_gnu_backquote_runtime(&mut ev);
    let original = neovm_text::emacs_core::syntax::builtin_syntax_table(&mut ev, vec![]).unwrap();
    ev.eval_str("(with-syntax-table (make-syntax-table) 1)")
        .expect("eval");
    let restored = neovm_text::emacs_core::syntax::builtin_syntax_table(&mut ev, vec![]).unwrap();
    assert!(eq_value(&restored, &original));
}

#[test]
fn with_syntax_table_restores_original_table_on_error() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::new_context();
    crate::test_utils::load_minimal_gnu_backquote_runtime(&mut ev);
    let original = neovm_text::emacs_core::syntax::builtin_syntax_table(&mut ev, vec![]).unwrap();
    let _ = ev.eval_str("(ignore-errors (with-syntax-table (make-syntax-table) missing-var))");
    let restored = neovm_text::emacs_core::syntax::builtin_syntax_table(&mut ev, vec![]).unwrap();
    assert!(eq_value(&restored, &original));
}

// ---------------------------------------------------------------------------
// Ledger 183: GNU's BASE resolution for the backtrace primitives, row by row.
//
// `get_backtrace_starting_at` (`src/eval.c:3958-3980`) is shared by
// `backtrace-frame`, `backtrace-debug`, `mapbacktrace` and
// `backtrace--frames-from-thread`, so probing it through `backtrace-frame`
// probes all four.  Ledger 172 audited it "only at its edges"; this is the
// frame-by-frame comparison it asked for -- 20 rows measured against GNU Emacs
// 31.0.90 `-Q --batch` (`tmp/l183-p13.el`), of which 19 already agreed and one
// did not.
// ---------------------------------------------------------------------------

/// The one divergent row of the 20.
///
/// GNU takes the offset out of `(OFFSET . FUNCTION)` with a bare `XFIXNUM`
/// (`src/eval.c:3966`) -- there is no `CHECK_FIXNAT` there, unlike the LEVEL
/// argument twenty lines further down (`:3986`) -- and spends it in
/// `while (backtrace_p (pdl) && offset-- > 0)` (`:3977`).  So a negative
/// offset is not an error, it is a loop that does not run.  Measured
/// (`tmp/l183-p14.el`), each row projected to the frame's function:
///
/// ```text
/// GNU                (l183-b2 l183-b2 l183-b1 nil wrong-type-argument)
/// this port, before  (wrong-type-argument l183-b2 l183-b1 nil wrong-type-argument)
/// ```
///
/// The asymmetry is the point: the same primitive checks one of its two
/// numbers and not the other, and copying the check onto both is how it got
/// wrong here.  The last row is the LEVEL argument, which GNU does check.
#[test]
fn a_negative_base_offset_is_not_an_error_like_gnu() {
    crate::test_utils::init_test_tracing();
    // `defun` is a macro from `byte-run.el`/`subr.el`, so a bare `Context`
    // cannot run this probe at all -- the first version of this pin failed
    // with `void-function l183-b1` rather than with the divergence.
    let out = bootstrap_eval(
        "(defun l183-b3 (probe) (funcall probe))
         (defun l183-b2 (probe) (l183-b3 probe))
         (defun l183-b1 (probe) (l183-b2 probe))
         (mapcar (lambda (x) (if (consp x) (nth 1 x) x))
           (l183-b1 (lambda ()
             (list (condition-case e (backtrace-frame 0 '(-1 . l183-b2)) (error (car e)))
                   (condition-case e (backtrace-frame 0 '(0 . l183-b2)) (error (car e)))
                   (condition-case e (backtrace-frame 0 '(1 . l183-b2)) (error (car e)))
                   (condition-case e (backtrace-frame 0 '(99 . l183-b2)) (error (car e)))
                   (condition-case e (backtrace-frame -1 'l183-b2) (error (car e)))))))",
    );
    assert_eq!(
        out.last().expect("the probe form must have run"),
        "OK (l183-b2 l183-b2 l183-b1 nil wrong-type-argument)"
    );
}
