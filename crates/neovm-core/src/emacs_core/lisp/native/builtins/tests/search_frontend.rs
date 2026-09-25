//! U2.8: the buffer-search front-end fast path answers exactly what the
//! general path answers -- value or signal, point, match data -- across
//! patterns that do and do not read syntax, bounds on both sides, counts,
//! NOERROR modes, `case-fold-search`, `inhibit-changing-match-data`, and
//! `syntax-propertize` states that force the general path.
use crate::emacs_core::eval::set_builtin_frontend_for_test;
use crate::emacs_core::value::Value;
use crate::test_utils::runtime_startup_context;

/// Every search and its observable aftermath, as one list per call.
const SEARCH_MATRIX: &str = r#"
(let ((out nil))
  (dolist (mode '(fundamental-mode emacs-lisp-mode))
    (dolist (text '("foo bar baz\n(defun x () \"str\" ; c\n  (car y))\n"
                    "\u00c0\u00c9 \u6f22\u5b57 abc_def-ghi\n(a \"b\" c)"))
      (dolist (propertized '(stale covered))
        (with-temp-buffer
          (insert text)
          (funcall mode)
          (dolist (re '("" "a" "b\\w+" "\\_<[a-z]+\\_>" "\\s-+" "[[:space:]]"
                        "(\\(\\w+\\)" "\\bba" "z\\'" "^(" "\\(?:x\\)\\{2\\}" "[" "\\s\""))
            (dolist (spec '((1 nil nil nil) (5 20 t nil) (3 nil t 2) (25 nil t -1)
                            (20 5 t nil) (10 nil 1 nil) (1 nil t 0) (30 nil t 3)
                            (7 nil nil nil)))
              (dolist (fn '(re-search-forward re-search-backward
                            posix-search-forward posix-search-backward))
                (dolist (fold '(nil t))
                  (dolist (inhibit '(nil t))
                    (if (eq propertized 'stale)
                        (setq syntax-propertize--done -1)
                      (setq syntax-propertize--done (1+ (point-max))))
                    (goto-char (min (point-max) (car spec)))
                    (set-match-data (list 1 1))
                    (let ((case-fold-search fold)
                          (inhibit-changing-match-data inhibit))
                      (push (list fn re spec fold inhibit
                                  (condition-case err (apply fn re (cdr spec))
                                    (error (list 'signal err)))
                                  (point)
                                  (match-data t))
                            out)))))))))))
  (nreverse out))
"#;

fn run_matrix(frontend: bool) -> Vec<String> {
    set_builtin_frontend_for_test(Some(frontend));
    let mut eval = runtime_startup_context();
    let result = eval
        .eval_str(SEARCH_MATRIX)
        .expect("the search matrix evaluates");
    set_builtin_frontend_for_test(None);
    crate::emacs_core::value::list_to_vec(&result)
        .expect("a list of rows")
        .iter()
        .map(|row: &Value| crate::emacs_core::print::print_value(row))
        .collect()
}

#[test]
fn fast_buffer_searches_answer_like_the_general_path() {
    crate::test_utils::init_test_tracing();
    let general = run_matrix(false);
    let before = super::FRONTEND_FAST_CALLS.with(|calls| calls.get());
    let fast = run_matrix(true);
    let fast_calls = super::FRONTEND_FAST_CALLS.with(|calls| calls.get()) - before;
    assert_eq!(general.len(), fast.len());
    assert!(general.len() > 5_000, "{} rows", general.len());
    for (index, (general, fast)) in general.iter().zip(&fast).enumerate() {
        assert_eq!(fast, general, "row {index} differs");
    }
    // The matrix must actually exercise the fast path, and also leave some
    // calls to the general path (stale `syntax-propertize--done` under
    // `emacs-lisp-mode`, COUNT 0, a malformed pattern).
    assert!(
        fast_calls > general.len() / 2,
        "only {fast_calls} of {} calls took the fast path",
        general.len()
    );
    assert!(fast_calls < general.len(), "every call took the fast path");
}

#[test]
fn the_knob_turns_the_fast_path_off() {
    crate::test_utils::init_test_tracing();
    let mut eval = runtime_startup_context();
    let form = r#"(with-temp-buffer (insert "abc abc") (goto-char 1)
                    (list (re-search-forward "b" nil t) (re-search-backward "a" nil t)))"#;
    for (frontend, expected_calls) in [(false, 0), (true, 2)] {
        set_builtin_frontend_for_test(Some(frontend));
        let before = super::FRONTEND_FAST_CALLS.with(|calls| calls.get());
        let result = eval.eval_str(form).expect("searches evaluate");
        let calls = super::FRONTEND_FAST_CALLS.with(|calls| calls.get()) - before;
        set_builtin_frontend_for_test(None);
        assert_eq!(crate::emacs_core::print::print_value(&result), "(3 1)");
        assert_eq!(calls, expected_calls, "frontend {frontend}");
    }
}
