//! VM tests whose harness is the runtime-startup image.
//!
//! `runtime_startup_context` installs the canonical eval hooks. That proof
//! is minted in core, so eval cannot construct this harness. The three
//! bootstrap helpers move with their callers. Tests that only need the
//! unit harness stay in eval, except the three that also call core's
//! `load_gnu_undo_auto_runtime` / `load_minimal_gnu_help_runtime`: those
//! use `new_context` here because eval's unit-runtime module stays
//! `cfg(test)`. Assertions are unchanged.
//!
//! The lexical-binding macroexpand test sets `Context::lexenv` through
//! `set_lexenv_for_test`. That field stays private. The signed setter is
//! why the test lives here.
//! The save-window-excursion opcode test moved with the other window-surface
//! tests; `Vm` is public and the split trait lives in display.

use neovm_value::error::EvalError;
use neovm_value::value::Value;

use crate::emacs_core::eval::Context;

fn with_vm_eval_in_context<R>(
    mut eval: Context,
    src: &str,
    lexical: bool,
    f: impl FnOnce(Result<Value, EvalError>, &Context) -> R,
) -> R {
    eval.set_lexical_binding(lexical);
    let result = eval.eval_str(src);
    f(result, &eval)
}

/// Unit-harness stand-in for the three moved tests that load a GNU Lisp
/// slice onto a bare context. Core's `new_context` is the legal bare
/// constructor; eval's unit-runtime module stays `cfg(test)`.
fn vm_eval_with_init_str(src: &str, init: impl FnOnce(&mut Context)) -> String {
    let mut eval = crate::test_utils::new_context();
    init(&mut eval);
    let result = eval.eval_str(src);
    crate::emacs_core::format_eval_result(&result)
}

fn with_vm_eval_bootstrap_context_state<R>(
    src: &str,
    lexical: bool,
    f: impl FnOnce(Result<Value, EvalError>, &Context) -> R,
) -> R {
    with_vm_eval_in_context(
        crate::test_utils::runtime_startup_context(),
        src,
        lexical,
        f,
    )
}

/// Variant of vm_eval_str that uses the cached runtime-startup
/// evaluator. Required for tests that exercise Lisp-defined
/// macros (with-temp-buffer, with-current-buffer, dolist,
/// dotimes, etc.) which only exist after subr.el is loaded.
fn vm_bootstrap_eval_str(src: &str) -> String {
    let mut eval = crate::test_utils::runtime_startup_context();
    let result = eval.eval_str(src);
    crate::emacs_core::format_eval_result(&result)
}
fn vm_bootstrap_eval_with_init_str(src: &str, init: impl FnOnce(&mut Context)) -> String {
    let mut eval = crate::test_utils::runtime_startup_context();
    init(&mut eval);
    let result = eval.eval_str(src);
    crate::emacs_core::format_eval_result(&result)
}

#[test]
fn vm_handler_bind_1_runs_inside_signal_dynamic_extent() {
    crate::test_utils::init_test_tracing();
    // user-error is defined in subr.el → use bootstrap context.
    with_vm_eval_bootstrap_context_state(
        "(catch 'tag
           (handler-bind-1
             (lambda ()
               (list 'inner-catch
                     (catch 'tag
                       (user-error \"hello\"))))
             '(error)
             (lambda (_err) (throw 'tag 'err))))",
        false,
        |result, _eval| {
            assert_eq!(
                crate::emacs_core::format_eval_result(&result),
                "OK (inner-catch err)"
            );
        },
    );
}

#[test]
fn vm_handler_bind_1_mutes_lower_condition_handlers() {
    crate::test_utils::init_test_tracing();
    // user-error is defined in subr.el → use bootstrap context.
    with_vm_eval_bootstrap_context_state(
        "(condition-case nil
           (handler-bind-1
             (lambda ()
               (list 'result
                     (condition-case nil
                         (user-error \"hello\")
                       (wrong-type-argument 'inner-handler))))
             '(error)
             (lambda (_err) (signal 'wrong-type-argument nil)))
         (wrong-type-argument 'wrong-type-argument))",
        false,
        |result, _eval| {
            assert_eq!(
                crate::emacs_core::format_eval_result(&result),
                "OK wrong-type-argument"
            );
        },
    );
}

#[test]
fn vm_handler_bind_1_handlers_do_not_apply_within_handlers() {
    crate::test_utils::init_test_tracing();
    // user-error is defined in subr.el → use bootstrap context.
    with_vm_eval_bootstrap_context_state(
        "(condition-case nil
           (handler-bind-1
             (lambda () (user-error \"hello\"))
             '(error)
             (lambda (_err) (signal 'wrong-type-argument nil))
             '(wrong-type-argument)
             (lambda (_err) (user-error \"wrong-type-argument\")))
         (wrong-type-argument 'wrong-type-argument)
         (error 'plain-error))",
        false,
        |result, _eval| {
            assert_eq!(
                crate::emacs_core::format_eval_result(&result),
                "OK wrong-type-argument"
            );
        },
    );
}

#[test]
fn vm_compiled_loaddefs_docstring_print_form_preserves_point() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_bootstrap_eval_str(
            r#"(progn
                 (require 'bytecomp)
                 (funcall
                  (byte-compile
                   (lambda ()
                     (with-temp-buffer
                       (let ((def (list 'autoload
                                        (list 'quote 'ebrowse-tags-find-declaration)
                                        "ebrowse"
                                        "Find declaration of member at point."
                                        t))
                             (n 0))
                         (insert "(")
                         (while (< n 3)
                           (prin1 (car def) (current-buffer)
                                  '(t (escape-newlines . t)
                                      (escape-control-characters . t)))
                           (setq def (cdr def))
                           (insert " ")
                           (setq n (1+ n)))
                         (let ((start (point)))
                           (prin1 (car def) (current-buffer) t)
                           (setq def (cdr def))
                           (save-excursion
                             (goto-char (1+ start))
                             (insert "\\
")))
                         (while def
                           (insert " ")
                           (prin1 (car def) (current-buffer)
                                  '(t (escape-newlines . t)
                                      (escape-control-characters . t)))
                           (setq def (cdr def)))
                         (insert ")")
                         (buffer-string)))))))"#
        ),
        "OK \"(autoload 'ebrowse-tags-find-declaration \\\"ebrowse\\\" \\\"\\\\\nFind declaration of member at point.\\\" t)\""
    );
}

#[test]
fn vm_primitive_bytecode_ops_ignore_later_function_cell_overrides() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_bootstrap_eval_str(
            r#"(progn
                 (require 'bytecomp)
                 (let* ((f-list (byte-compile (lambda () (list 1 2 3))))
                        (f-concat (byte-compile (lambda () (concat "a" "b"))))
                        (f-nth (byte-compile (lambda () (nth 0 '(a b)))))
                        (f-memq (byte-compile (lambda () (memq 'a '(a b)))))
                        (orig-list (symbol-function 'list))
                        (orig-concat (symbol-function 'concat))
                        (orig-nth (symbol-function 'nth))
                        (orig-memq (symbol-function 'memq)))
                   (unwind-protect
                       (progn
                         (fset 'list (lambda (&rest _) 'override))
                         (fset 'concat (lambda (&rest _) 'override))
                         (fset 'nth (lambda (&rest _) 'override))
                         (fset 'memq (lambda (&rest _) 'override))
                         (funcall orig-list
                                  (funcall f-list)
                                  (funcall f-concat)
                                  (funcall f-nth)
                                  (funcall f-memq)))
                     (fset 'list orig-list)
                     (fset 'concat orig-concat)
                     (fset 'nth orig-nth)
                     (fset 'memq orig-memq))))"#
        ),
        "OK ((1 2 3) \"ab\" a (a b))"
    );
}

#[test]
fn vm_compiled_maphash_closure_mutates_captured_accumulator_like_face_list() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_bootstrap_eval_str(
            r#"(progn
                 (require 'bytecomp)
                 (funcall
                  (byte-compile
                   (lambda ()
                     (let ((table (make-hash-table :test 'eq))
                           faces)
                       (puthash 'face '(1) table)
                       (maphash (lambda (face spec)
                                  (push `(,(car spec) . ,face) faces))
                                table)
                       faces)))))"#
        ),
        "OK ((1 . face))"
    );
}

#[test]
fn vm_varset_refreshes_gc_runtime_settings_after_compiled_startup_timer() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    eval.eval_str_each(
        "(progn
           (setq gc-cons-percentage nil)
           (setq gc-cons-threshold 268435456)
           (setq neomacs--startup-gc-ceiling-active t))",
    );
    assert_eq!(
        eval.gc_threshold(),
        4 * 1024 * 1024,
        "startup must temporarily cap the internal collector threshold"
    );

    eval.eval_str(
        "(funcall
           (byte-compile
             (lambda ()
               (setq neomacs--startup-gc-ceiling-active nil))))",
    )
    .expect("compiled startup timer should run");

    assert_eq!(
        eval.gc_threshold(),
        268_435_456,
        "compiled varset must release the internal startup GC ceiling"
    );
}

#[test]
fn vm_unbind_propagates_restore_watcher_signal() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_bootstrap_eval_str(
            r#"(progn
                 (defvar vm-unbind-error-target 9)
                 (setq vm-unbind-error-body-ran nil)
                 (fset 'vm-unbind-error-watcher
                       (lambda (_symbol _new-value operation _where)
                         (if (eq operation 'unlet)
                             (signal 'error '("restore"))
                           nil)))
                 (add-variable-watcher 'vm-unbind-error-target
                                       'vm-unbind-error-watcher)
                 (condition-case error
                     (funcall
                      (byte-compile
                       (lambda ()
                         (let ((vm-unbind-error-target 1))
                           (setq vm-unbind-error-body-ran t)
                           'body))))
                   (error
                    (list (car error) vm-unbind-error-body-ran))))"#
        ),
        "OK (error t)"
    );
}

#[test]
fn vm_string_builtin_calls_preserve_aliases_redefinition_and_arity() {
    crate::test_utils::init_test_tracing();
    // GNU Emacs returns (70 nil) with either compiler binding mode. Exercise
    // actual Bcall instructions, including the same symbol after repeated fset.
    let form = r#"(let* ((call (byte-compile
              '(lambda (function string item)
                 (let* ((alias string) (result (funcall function string item)))
                   (list result string alias (eq result string) (eq string alias))))))
       (wrong-arity (byte-compile '(lambda (function string) (funcall function string))))
       (fill (symbol-function 'fillarray))
       (concat-subr (symbol-function 'concat))
       (count 0) failures)
  (fset 'vm-wb-direct fill)
  (fset 'vm-wb-alias 'fillarray)
  (fset 'vm-wb-chain 'vm-wb-alias)
  (dolist (original '("abc" "éßø" "漢字文" "😀😃😄" ""))
    (let ((character (if (> (length original) 0) (aref original 0) ?x)))
      (dolist (function (list 'fillarray fill 'vm-wb-direct 'vm-wb-alias 'vm-wb-chain))
        (let* ((string (copy-sequence original))
               (filled (make-string (length string) character))
               (actual (funcall call function string character))
               (expected (list filled filled filled t t)))
          (setq count (1+ count))
          (unless (equal actual expected)
            (push (list 'fill original function actual) failures))))
      ;; Reuse the same compiled call and symbol across function-epoch changes.
      (dotimes (_ 3)
        (fset 'vm-wb-changing concat-subr)
        (let* ((string (copy-sequence original))
               (actual (funcall call 'vm-wb-changing string "!")))
          (setq count (1+ count))
          (unless (equal actual (list (concat original "!") original original nil t))
            (push (list 'concat original actual) failures)))
        (fset 'vm-wb-changing fill)
        (let* ((string (copy-sequence original))
               (filled (make-string (length string) character))
               (actual (funcall call 'vm-wb-changing string character)))
          (setq count (1+ count))
          (unless (equal actual (list filled filled filled t t))
            (push (list 'changed original actual) failures))))
      (dolist (function (list 'fillarray fill 'vm-wb-alias))
        (setq count (1+ count))
        (unless (eq (condition-case err
                        (funcall wrong-arity function (copy-sequence original))
                      (error (car err)))
                    'wrong-number-of-arguments)
          (push (list 'arity original function) failures)))))
  (list count (nreverse failures)))"#;
    for lexical in ["nil", "t"] {
        assert_eq!(
            vm_bootstrap_eval_str(&format!("(let ((lexical-binding {lexical})) {form})")),
            "OK (70 nil)",
            "lexical-binding={lexical}"
        );
    }
}

#[test]
fn vm_bootstrap_comparisons_and_fixnump_handle_bignums() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_bootstrap_eval_str(
            "(let ((x (1+ most-positive-fixnum))) (list (<= x most-positive-fixnum) (>= x most-positive-fixnum) (fixnump x) (bignump x)))"
        ),
        "OK (nil t nil t)"
    );
    assert_eq!(
        vm_bootstrap_eval_str(
            "(let ((x 1267650600228229401496703205376)) (list (<= x most-positive-fixnum) (>= x most-positive-fixnum) (fixnump x) (bignump x)))"
        ),
        "OK (nil t nil t)"
    );
}

/// Byte-compiled `pcase`/`cond` forms dispatch through real jump tables
/// (`byte-compile-cond-jump-table`, lisp/emacs-lisp/bytecomp.el), each corpus
/// many times over so the table is dispatched cold, repeatedly and (with the
/// JIT on) from native code. The expected strings are GNU Emacs 31.1's answers
/// for the same forms, recorded by the `pcase_switch_jump_table_semantics`
/// oracle file; this runs them in-process so every nextest run checks them.
#[test]
fn vm_switch_byte_compiled_jump_tables_answer_like_gnu() {
    crate::test_utils::init_test_tracing();
    const RUNNER: &str = "(run (lambda (f inputs)
              (let ((first (mapcar f inputs)) (stable t))
                (dotimes (_ 150)
                  (unless (equal (mapcar f inputs) first) (setq stable nil)))
                (list stable first))))";
    let cases: [(String, &str); 9] = [
        (
            format!(
                "(let* ({RUNNER} (f (byte-compile (lambda (x) (pcase x ('a 1) ('b 2) (:k 3) ('nil 4) ('t 5) ('c 6) (_ 0))))))
                   (funcall run f (list 'a 'b :k nil t 'c 'd :other 0 \"a\" (list 'a) 1.0 (intern \"a\") (make-symbol \"a\"))))"
            ),
            "OK (t (1 2 3 4 5 6 0 0 0 0 0 0 1 0))",
        ),
        (
            format!(
                "(let* ({RUNNER}
                        (dense (byte-compile (lambda (x) (pcase x (0 'a0) (1 'a1) (2 'a2) (3 'a3) (4 'a4) (5 'a5) (6 'a6) (7 'a7) (8 'a8) (9 'a9) (_ 'no)))))
                        (sparse (byte-compile (lambda (x) (cond ((eql x -4) 'neg) ((eql x ?a) 'ch) ((eql x 1000) 'k) ((eql x 2305843009213693951) 'mpf) ((eql x -2305843009213693952) 'mnf) ((eql x 0) 'zero) (t 'no)))))
                        (mixed (byte-compile (lambda (x) (pcase x (1 'one) (2 'two) ('a 'sym) (?\\C-c 'ctl) (_ 'no)))))
                        (inputs (list -5 -4 0 1 2 3 5 8 9 10 97 ?b 1000 2305843009213693951 -2305843009213693952 1.0 'a nil (expt 2 70))))
                   (list (funcall run dense inputs) (funcall run sparse inputs) (funcall run mixed inputs)))"
            ),
            "OK ((t (no no a0 a1 a2 a3 a5 a8 a9 no no no no no no no no no no)) (t (no neg zero no no no no no no no ch no k mpf mnf no no no no)) (t (no no no one two ctl no no no no no no no no no no sym no no)))",
        ),
        (
            format!(
                "(let* ({RUNNER}
                        (by-eql (byte-compile (lambda (x) (cond ((eql x 0.0) 'pz) ((eql x -0.0) 'nz) ((eql x 1.0) 'one) ((eql x 1) 'int) ((eql x 0.0e+NaN) 'nan) (t 'no)))))
                        (by-equal (byte-compile (lambda (x) (cond ((equal x 0.0) 'pz) ((equal x -0.0) 'nz) ((equal x 1.5) 'x15) ((equal x 1) 'int) (t 'no)))))
                        (inputs (list 0.0 -0.0 1.0 1 (/ 2.0 2) 0.0e+NaN (- 0.0e+NaN) 1.5 (* 1.5 1) 2.0 'a nil (expt 2 64))))
                   (list (funcall run by-eql inputs) (funcall run by-equal inputs)))"
            ),
            "OK ((t (pz nz one int one nan no no no no no no no)) (t (pz nz no int no no no x15 x15 no no no no)))",
        ),
        (
            format!(
                r#"(let* ({RUNNER} (f (byte-compile (lambda (x) (pcase x ("alpha" 1) ("beta" 2) ("" 3) ("\377" 4) ("é" 5) ("abc" 6) (_ 0))))))
                   (funcall run f (list "alpha" (copy-sequence "alpha") (propertize "beta" 'face 'bold) "" (make-string 0 ?x)
                                        "\377" (string-to-multibyte "\377") (string ?é) (encode-coding-string (string ?é) 'utf-8)
                                        (encode-coding-string (string ?é) 'latin-1) (string-to-multibyte "abc") (string-to-unibyte "abc")
                                        "ALPHA" 'alpha nil 1)))"#
            ),
            "OK (t (1 1 2 3 3 4 0 5 0 0 6 6 0 0 0 0))",
        ),
        (
            format!(
                r#"(let* ({RUNNER}
                        (f (byte-compile (lambda (x) (pcase x (`(a b) 1) (`(a . b) 2) (`((a) b) 3) (`(a) 4) (`(1 "s" 2.5) 5) (_ 0)))))
                        (circular (let ((c (list 'a 'b))) (setcdr (cdr c) c) c))
                        (deep (let ((d nil)) (dotimes (_ 300) (setq d (list d))) d))
                        (shared (let ((tail (list 'b))) (list (list 'a) (car tail))))
                        (dag (let ((a (list 'a))) (list a a))))
                   (funcall run f (list (list 'a 'b) (cons 'a 'b) (list (list 'a) 'b) (list 'a) (list 'a 'b 'c) (list 'b) 'a nil
                                        (list 1 "s" 2.5) (list 1 (copy-sequence "s") 2.5) (list 1 "s" 2.50001) (list 1 "s" 2.5 nil)
                                        circular deep shared dag (vector 'a 'b) (list 'a (list 'b)))))"#
            ),
            "OK (t (1 2 3 4 0 0 0 0 5 5 0 0 0 0 3 0 0 0))",
        ),
        (
            format!(
                "(let* ({RUNNER} (f (byte-compile (lambda (x) (pcase x (18446744073709551616 'bn) ('[1 2] 'vec) ('(1 [2]) 'lv) ('a 'sym) (_ 'no))))))
                   (funcall run f (list (expt 2 64) (1+ (expt 2 64)) (vector 1 2) (vector 1 3) [1 2] (list 1 (vector 2)) 'a 18446744073709551616.0 nil)))"
            ),
            "OK (t (bn no vec no vec lv sym no no))",
        ),
        (
            r#"(let* ((f (byte-compile (lambda (x) (pcase x ('a 1) ('b 2) (_ 0)))))
                      (g (byte-compile (lambda (x) (pcase x (`(a b) 1) (`(c . d) 2) (_ 0)))))
                      (h (byte-compile (lambda (x) (cond ((eql x 'a) 1) ((eql x 'b) 2) ((eql x 5) 3) (t 0)))))
                      (forms (read-positioning-symbols "(a b (a b) (c . d) (x a b))"))
                      (pa (nth 0 forms)) (pb (nth 1 forms)) (pab (nth 2 forms)) (pcd (nth 3 forms))
                      (probe (lambda ()
                               (list (mapcar f (list pa pb 'a 'b pab))
                                     (mapcar g (list pab pcd (list 'a 'b) (list 'a pb) pa))
                                     (mapcar h (list pa pb 'a 5 pab))))))
                 (list (symbol-with-pos-p pa)
                       (funcall probe)
                       (let ((symbols-with-pos-enabled t)) (funcall probe))
                       (let ((symbols-with-pos-enabled t)) (dotimes (_ 400) (funcall probe)) (funcall probe))
                       (funcall probe)))"#
                .to_string(),
            "OK (t ((0 0 1 2 0) (0 0 1 0 0) (0 0 1 3 0)) ((1 2 1 2 0) (1 2 1 1 0) (1 2 1 3 0)) ((1 2 1 2 0) (1 2 1 1 0) (1 2 1 3 0)) ((0 0 1 2 0) (0 0 1 0 0) (0 0 1 3 0)))",
        ),
        (
            r#"(let* ((f (byte-compile (lambda (x) (pcase x ('a 1) ('b 2) ('c 3) ('d 4) ('e 5) ('f 6) ('g 7) (_ 0)))))
                      (g (byte-compile (lambda (x) (pcase x (`(a b) 1) (`(c) 2) ("s" 3) (_ 0)))))
                      (ftbl nil) (gtbl nil))
                 (mapc (lambda (c) (when (hash-table-p c) (setq ftbl c))) (aref f 2))
                 (mapc (lambda (c) (when (hash-table-p c) (setq gtbl c))) (aref g 2))
                 (list (mapcar f '(a b c d e f g h z))
                       (mapcar g (list (list 'a 'b) (list 'c) "s" (list 'e) 'a))
                       (progn (puthash 'z (gethash 'a ftbl) ftbl) (funcall f 'z))
                       (progn (remhash 'a ftbl) (list (funcall f 'a) (funcall f 'z) (funcall f 'g)))
                       (progn (puthash 'b (gethash 'c ftbl) ftbl) (list (funcall f 'b) (funcall f 'c)))
                       (progn (puthash (list 'e) (gethash "s" gtbl) gtbl) (funcall g (list 'e)))
                       (progn (remhash (list 'a 'b) gtbl) (funcall g (list 'a 'b)))
                       (progn (puthash "s" (gethash (list 'c) gtbl) gtbl) (list (funcall g "s") (funcall g (list 'c))))))"#
                .to_string(),
            "OK ((1 2 3 4 5 6 7 0 0) (1 2 3 0 0) 1 (0 1 7) (3 3) 3 0 (2 2))",
        ),
        (
            r#"(let* ((f (byte-compile (lambda (x) (pcase x (`(a b) 1) (`(c) 2) ('d 3) (_ 0)))))
                      (tbl nil) (warm nil))
                 (mapc (lambda (c) (when (hash-table-p c) (setq tbl c))) (aref f 2))
                 (dotimes (i 1200)
                   (setq warm (list (funcall f (list 'a 'b)) (funcall f (list 'c)) (funcall f 'd) (funcall f i))))
                 (list warm
                       (progn (puthash (list 'e) (gethash (list 'c) tbl) tbl) (funcall f (list 'e)))
                       (progn (puthash 'd (gethash (list 'a 'b) tbl) tbl) (funcall f 'd))
                       (progn (remhash (list 'a 'b) tbl) (funcall f (list 'a 'b)))
                       (let (r)
                         (dotimes (_ 1200)
                           (setq r (list (funcall f (list 'a 'b)) (funcall f (list 'c)) (funcall f 'd) (funcall f (list 'e)))))
                         r)))"#
                .to_string(),
            "OK ((1 2 3 0) 2 1 0 (0 2 1 2))",
        ),
    ];
    let mut eval = crate::test_utils::runtime_startup_context();
    eval.set_lexical_binding(true);
    eval.eval_str("(require 'bytecomp)")
        .expect("bytecomp loads");
    for (form, gnu) in &cases {
        let result = eval.eval_str(form);
        assert_eq!(
            crate::emacs_core::format_eval_result(&result),
            *gnu,
            "{form}"
        );
    }
}

#[test]
fn vm_window_state_accessors_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_bootstrap_eval_str(
            r#"(let ((w (selected-window)))
                 (with-current-buffer (window-buffer w)
                   (erase-buffer)
                   (insert (make-string 200 ?x)))
                 (set-window-start w 7)
                 (set-window-point w 9)
                 (list (window-start w)
                       (window-group-start w)
                       (window-point w)
                       (integerp (window-use-time w))
                       (window-old-point w)
                       (window-old-buffer w)
                       (window-prev-buffers w)
                       (window-next-buffers w)))"#
        ),
        "OK (7 7 9 t 1 nil nil nil)"
    );
    assert_eq!(
        vm_bootstrap_eval_str(
            r#"(list (condition-case err (window-start 999999) (error err))
                     (condition-case err (window-group-start 999999) (error err))
                     (condition-case err (window-point 999999) (error err))
                     (condition-case err (window-use-time 999999) (error err))
                     (condition-case err (window-old-point 999999) (error err))
                     (condition-case err (window-old-buffer 999999) (error err))
                     (condition-case err (window-prev-buffers 999999) (error err))
                     (condition-case err (window-next-buffers 999999) (error err)))"#
        ),
        // `window-old-buffer` is the odd one out, and deliberately so: GNU
        // decodes it with `decode_any_window` rather than
        // `decode_live_window` ("WINDOW can be any window", window.c), so a
        // non-window argument fails the weaker `windowp` predicate. Every
        // other accessor here demands a live window. Verified against GNU
        // 31.1, which returns exactly this list.
        "OK ((wrong-type-argument window-live-p 999999) (wrong-type-argument window-live-p 999999) (wrong-type-argument window-live-p 999999) (wrong-type-argument window-live-p 999999) (wrong-type-argument window-live-p 999999) (wrong-type-argument windowp 999999) (wrong-type-argument window-live-p 999999) (wrong-type-argument window-live-p 999999))"
    );
}

#[test]
fn vm_scroll_and_recenter_builtins_use_shared_window_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_bootstrap_eval_str(
            r#"(let ((w (selected-window)))
                 (with-current-buffer (window-buffer w)
                   (erase-buffer)
                   (insert "a\nb\nc\nd\ne\nf\ng\nh\n"))
                 (set-window-point w 1)
                 (list (condition-case err
                           (progn (scroll-up 2) (scroll-down 1) (window-point w))
                         (error (car err)))
                       (progn (scroll-left 3) (window-hscroll w))
                       (progn (scroll-right 1) (window-hscroll w))
                       (progn (set-window-point w 9) (recenter 1) (window-start w))))"#
        ),
        // GNU 31.0.90 --batch: the bootstrap frame is the INITIAL frame, where
        // `pos_visible_p` is always false, so `scroll-down` recenters around
        // point, lands on BEGV and signals (window.c window_scroll_line_based
        // `lose = n < 0 && PT == BEGV`).
        "OK (beginning-of-buffer 3 2 7)"
    );
}

#[test]
fn vm_window_geometry_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_bootstrap_eval_str(
            r#"(let* ((w (selected-window))
                      (m (minibuffer-window)))
                 (with-current-buffer (window-buffer w)
                   (erase-buffer)
                   (insert (make-string 200 ?x)))
                 (list (window-total-height w)
                       (window-body-width w)
                       (window-body-height w)
                       (window-body-width w)
                       (window-total-height w)
                       (window-total-width w)
                       (window-left-column w)
                       (window-top-line m)
                       (window-pixel-left w)
                       (window-pixel-top m)
                       (> (window-end w) (window-start w))
                       (window-mode-line-height w)
                       (window-mode-line-height m)
                       (window-header-line-height w)
                       (window-pixel-height w)
                       (window-pixel-height m)
                       (window-pixel-width w)
                       (window-pixel-width m)
                       (window-text-height w)
                       (window-text-height m)
                       (window-text-width w)
                       (window-text-width m)
                       (window-edges w)
                       (window-edges m)
                       (window-edges w t)
                       (window-edges m t)))"#
        ),
        "OK (24 80 23 80 24 80 0 24 0 24 t 1 0 0 24 1 80 80 23 1 80 80 (0 0 80 24) (0 24 80 25) (0 0 80 23) (0 24 80 25))"
    );
}

#[test]
fn vm_call_interactively_builtin_forward_char_uses_default_prefix_arg() {
    crate::test_utils::init_test_tracing();
    // forward-char is (interactive "^p"); GNU callint.c calls the Lisp
    // `handle-shift-selection` from simple.el unconditionally, so the bare
    // harness (no Lisp loaded) cannot run it.
    assert_eq!(
        vm_bootstrap_eval_with_init_str(
            r#"(progn
                 (call-interactively 'forward-char)
                 (point))"#,
            |eval| {
                let current = eval
                    .buffer_manager_mut()
                    .current_buffer_id()
                    .expect("current buffer");
                let _ = eval
                    .buffer_manager_mut()
                    .replace_buffer_contents(current, "ab");
                let _ = eval
                    .buffer_manager_mut()
                    .goto_buffer_emacs_byte_pos(current, neovm_edit::buffer::EmacsBytePos::new(0));
            },
        ),
        "OK 2"
    );
}

/// The `N' code letter without a prefix argument calls `read-number', which is
/// defined in subr.el (lisp/subr.el:3725) -- GNU reaches it through the
/// function cell at src/callint.c:645 -- so this needs the bootstrap context
/// the same way `user-error' does above (DIVERGENCES.md 152).
#[test]
fn vm_call_interactively_number_specs_need_subr_el() {
    crate::test_utils::init_test_tracing();
    with_vm_eval_bootstrap_context_state(
        r#"(list
              (let ((current-prefix-arg '(4))
                    (prefix-arg nil))
                (call-interactively '(lambda (n) (interactive "NNumber: ") n)))
              (let ((current-prefix-arg nil)
                    (prefix-arg nil))
                (condition-case err
                    (call-interactively '(lambda (n) (interactive "NNumber: ") n))
                  (error (car err))))
              (condition-case err
                  (call-interactively '(lambda (x) (interactive "nNumber: ") x))
                (error (car err))))"#,
        false,
        |result, _eval| {
            assert_eq!(
                crate::emacs_core::format_eval_result(&result),
                "OK (4 end-of-file end-of-file)"
            );
        },
    );
}

#[test]
fn vm_mapatoms_and_maphash_use_shared_runtime_callbacks() {
    crate::test_utils::init_test_tracing();
    // `when` is a macro defined in subr.el, so this test needs the
    // bootstrap context.
    assert_eq!(
        vm_bootstrap_eval_str(
            "(list
               (let ((h (make-hash-table :test 'eq))
                     (acc 0))
                 (puthash 'a 1 h)
                 (puthash 'b 2 h)
                 (maphash (lambda (_k v) (setq acc (+ acc v))) h)
                 acc)
               (let ((seen nil))
                 (intern \"vm-mapatoms-default\")
                 (mapatoms (lambda (sym)
                             (when (eq sym 'vm-mapatoms-default)
                               (setq seen t))))
                 seen)
               (let* ((ob (make-vector 7 0))
                      (target (intern \"vm-mapatoms-custom\" ob))
                     (seen nil))
                 (mapatoms (lambda (sym)
                             (when (eq sym target)
                               (setq seen t)))
                           ob)
                 seen))"
        ),
        "OK (3 t t)"
    );
}

#[test]
fn vm_casefiddle_region_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_bootstrap_eval_str(
            r#"(progn
                 (insert "heLLo woRLD")
                 (list
                  (progn
                    (downcase-region 1 6)
                    (buffer-string))
                  (progn
                    (upcase-region 7 12)
                    (buffer-string))
                  (progn
                    (capitalize-region 1 12)
                    (buffer-string))
                  (progn
                    (upcase-initials-region 1 12)
                    (buffer-string))))"#
        ),
        r#"OK ("hello woRLD" "hello WORLD" "Hello World" "Hello World")"#
    );
}

#[test]
fn vm_casefiddle_word_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_bootstrap_eval_str(
            r#"(progn
                 (insert "heLLo woRLD")
                 (list
                  (progn
                    (goto-char 1)
                    (downcase-word 1)
                    (buffer-string))
                  (progn
                    (goto-char 7)
                    (upcase-word 1)
                    (buffer-string))
                  (progn
                    (goto-char 1)
                    (capitalize-word 2)
                    (buffer-string))))"#
        ),
        r#"OK ("hello woRLD" "hello WORLD" "Hello World")"#
    );
}

#[test]
fn vm_completion_builtins_use_shared_runtime_callbacks() {
    crate::test_utils::init_test_tracing();
    // `defun` is defined in subr.el → bootstrap context required.
    assert_eq!(
        vm_bootstrap_eval_with_init_str(
            r#"(progn
                 (defun neo-vm-completion-target () nil)
                 (let ((items '("alpha" "alps" "beta"))
                       (pred (lambda (candidate)
                               (not (equal candidate "beta")))))
                   (let ((collection
                          (lambda (string predicate action)
                            (cond
                             ((eq action nil)
                              (try-completion string items predicate))
                             ((eq action t)
                              (all-completions string items predicate))
                             ((eq action 'lambda)
                              (test-completion string items predicate))
                             (t nil)))))
                     (list
                      (try-completion
                       "neo-vm-completion-target"
                       obarray
                       (lambda (sym) (eq sym 'neo-vm-completion-target)))
                     (not
                       (null
                        (member "neo-vm-completion-target"
                                (all-completions
                                 "neo-vm"
                                 obarray
                                 (lambda (sym)
                                   (eq sym 'neo-vm-completion-target))))))
                      (try-completion "al" collection pred)))))"#,
            |eval| {
                let obarray_proxy = Value::vector(vec![Value::NIL]);
                eval.obarray_mut()
                    .set_symbol_value("obarray", obarray_proxy);
                eval.obarray_mut()
                    .set_symbol_value("neovm--obarray-object", obarray_proxy);
            }
        ),
        r#"OK (t t "alp")"#
    );
}

#[test]
fn vm_marker_print_targets_insert_and_restore_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_bootstrap_eval_str(
            r#"(let* ((orig (current-buffer))
                      (obuf (get-buffer-create "*vm-marker-print*")))
                 (with-current-buffer obuf
                   (erase-buffer)
                   (insert "xy")
                   (goto-char 2))
                 (let ((m (with-current-buffer obuf (point-marker))))
                   (list
                    (progn
                      (princ "ab" m)
                      (with-current-buffer obuf
                        (list (buffer-string) (point) (marker-position m))))
                    (progn
                      (write-char 67 m)
                      (with-current-buffer obuf
                        (list (buffer-string) (point) (marker-position m))))
                    (progn
                      (terpri m)
                      (with-current-buffer obuf
                        (list (buffer-string) (point) (marker-position m))))
                    (eq (current-buffer) orig))))"#
        ),
        "OK ((\"xaby\" 4 4) (\"xabCy\" 5 5) (\"xabC\ny\" 6 6) t)"
    );
}

#[test]
fn vm_with_current_buffer_restores_outer_point_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_bootstrap_eval_str(
            r#"(let* ((orig (current-buffer))
                      (orig-point (point))
                      (obuf (get-buffer-create "*vm-wcb*")))
                 (with-current-buffer obuf
                   (erase-buffer)
                   (insert "xy")
                   (goto-char 2))
                 (list (eq (current-buffer) orig)
                       orig-point
                       (point)
                       (with-current-buffer obuf (buffer-string))))"#
        ),
        "OK (t 1 1 \"xy\")"
    );
}

#[test]
fn vm_save_current_buffer_restores_outer_point_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_bootstrap_eval_str(
            r#"(let* ((orig (current-buffer))
                      (orig-point (point))
                      (obuf (get-buffer-create "*vm-save-current-buffer*")))
                 (save-current-buffer
                   (set-buffer obuf)
                   (erase-buffer)
                   (insert "xy")
                   (goto-char 2))
                 (list (eq (current-buffer) orig)
                       orig-point
                       (point)
                       (with-current-buffer obuf (buffer-string))))"#
        ),
        "OK (t 1 1 \"xy\")"
    );
}

#[test]
fn vm_make_process_builtin_uses_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    // ignore-errors is a macro from subr.el → bootstrap context required.
    assert_eq!(
        vm_bootstrap_eval_str(
            r#"(let ((p (make-process
                          :name "vm-make-process"
                          :buffer "vm-make-process-buffer"
                          :command '("cat")
                          :filter 'ignore
                          :sentinel 'ignore)))
                 (unwind-protect
                     (list
                      (processp p)
                      (equal (process-name p) "vm-make-process")
                      (equal (process-command p) '("cat"))
                      (let ((b (process-buffer p)))
                        (and (bufferp b)
                             (equal (buffer-name b) "vm-make-process-buffer")))
                      (eq (process-filter p) 'ignore)
                      (eq (process-sentinel p) 'ignore))
                   (ignore-errors (delete-process p))))"#
        ),
        "OK (t t t t t t)"
    );
}

#[test]
fn vm_base64_region_and_json_buffer_builtins_use_shared_current_buffer_state() {
    crate::test_utils::init_test_tracing();
    // with-current-buffer is a macro in subr.el → bootstrap required.
    assert_eq!(
        vm_bootstrap_eval_str(
            r#"(with-current-buffer (get-buffer-create " *vm-base64-json*")
                 (erase-buffer)
                 (insert "Hi")
                 (let ((encoded-len (base64-encode-region (point-min) (point-max)))
                       (encoded (buffer-string)))
                   (goto-char (point-min))
                   (let ((decoded-len (base64-decode-region (point-min) (point-max)))
                         (decoded (buffer-string)))
                     (erase-buffer)
                     (insert "{\"a\":1} tail")
                     (goto-char (point-min))
                     (let ((parsed (json-parse-buffer :object-type 'alist))
                           (parse-point (point)))
                       (erase-buffer)
                       (json-insert [1 2 3])
                       (list encoded-len
                             encoded
                             decoded-len
                             decoded
                             parsed
                             parse-point
                             (buffer-string))))))"#
        ),
        r#"OK (4 "SGk=" 2 "Hi" ((a . 1)) 8 "[1,2,3]")"#
    );
}

#[test]
fn vm_internal_utility_builtins_use_direct_and_shared_state_paths() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_bootstrap_eval_str(
            r#"(with-current-buffer (get-buffer-create " *vm-internal-utils*")
                 (erase-buffer)
                 (insert "abc")
                 (list
                  (equal (field-string-no-properties) "abc")
                  (let ((table (make-hash-table :test 'eq)))
                    (puthash 'a 1 table)
                    (list
                     (consp (internal--hash-table-buckets table))
                     (consp (internal--hash-table-histogram table))
                     (integerp (internal--hash-table-index-size table))))
                  (let ((ob (make-vector 4 nil)))
                    (listp (internal--obarray-buckets ob)))
                  (null (internal--set-buffer-modified-tick 1 (current-buffer)))
                  (progn
                    (defvar vm-bytecode-internal-nonspecial nil)
                    (let ((before (special-variable-p 'vm-bytecode-internal-nonspecial)))
                      (internal-make-var-non-special 'vm-bytecode-internal-nonspecial)
                      (list before
                            (special-variable-p 'vm-bytecode-internal-nonspecial))))
                  (eq
                   (internal-set-lisp-face-attribute-from-resource
                    'default :weight "bold")
                   'default)
                  (eq (internal-get-lisp-face-attribute 'default :weight)
                      'bold)))"#
        ),
        r#"OK (t (t t t) t t (t nil) t t)"#
    );
}

#[test]
fn vm_composition_and_compute_motion_builtins_use_direct_dispatch() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_bootstrap_eval_str(
            r#"(list
                 (equal (compose-string-internal "abc" 0 2 nil nil) "abc")
                 (null (find-composition-internal 1 nil nil nil))
                 (vectorp (composition-get-gstring 0 1 nil "ab"))
                 (null (clear-composition-cache))
                 (equal (composition-sort-rules '((1 . 2))) '((1 . 2)))
                 (with-current-buffer (get-buffer-create "*vm-compute-motion*")
                   (erase-buffer)
                   (insert "\tX")
                   (setq tab-width 4)
                   (equal (compute-motion 1 '(0 . 0) 3 nil 80 nil nil)
                          '(3 5 0 4 nil))))"#
        ),
        "OK (t t t t t t)"
    );
}

#[test]
fn vm_delete_char_uses_shared_read_only_and_narrowing_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_with_init_str(
            r#"(list
                 (let ((buffer-read-only t))
                   (condition-case err
                       (delete-char 1)
                     (error (car err))))
                 (let ((buffer-read-only t)
                       (inhibit-read-only t))
                   (delete-char 1)
                   (buffer-string))
                 (progn
                   (narrow-to-region 1 2)
                   (goto-char (point-max))
                   (condition-case err
                       (delete-char 1)
                     (error (car err)))))"#,
            |eval| {
                crate::test_utils::load_gnu_undo_auto_runtime(eval);
                let current = eval
                    .buffer_manager_mut()
                    .current_buffer_id()
                    .expect("scratch buffer");
                {
                    let buffers = eval.buffer_manager_mut();
                    let buffer = buffers.get_mut(current).expect("scratch buffer");
                    buffer.insert("abc");
                    buffer.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(0));
                }
            },
        ),
        r#"OK (buffer-read-only "bc" end-of-buffer)"#
    );
}

#[test]
fn vm_search_builtins_use_shared_runtime_state_and_match_data() {
    crate::test_utils::init_test_tracing();
    // Use bootstrap context throughout — mixing bare and bootstrap
    // vm_eval inside one #[test] divergences the global interner with
    // the cached pdump.
    assert_eq!(
        vm_bootstrap_eval_str(
            r#"(with-temp-buffer
                 (insert "ab")
                 (let ((end (copy-marker (point-max) t)))
                   (goto-char (point-min))
                   (insert "X")
                   (goto-char (point-min))
                   (list (search-forward "b" end t)
                         (point)
                         (marker-position end)
                         (match-beginning 0)
                         (match-end 0))))"#
        ),
        "OK (4 4 4 3 4)"
    );

    // search-forward-regexp is an alias defined in subr.el, so the
    // bootstrap context is needed.
    assert_eq!(
        vm_bootstrap_eval_str(
            r#"(with-temp-buffer
                 (insert "ab12")
                 (goto-char 1)
                 (list (re-search-forward "[0-9]+" nil t)
                       (match-beginning 0)
                       (match-end 0)
                       (progn
                         (goto-char 1)
                         (search-forward-regexp "[a-z]+" nil t))
                       (progn
                         (goto-char 1)
                         (posix-search-forward "[0-9]+" nil t))))"#
        ),
        "OK (5 3 5 3 5)"
    );
}

#[test]
fn vm_when_unless() {
    crate::test_utils::init_test_tracing();
    // when/unless are macros defined in subr.el, so they require the
    // runtime-startup bootstrap context to be available — the bare
    // bytecode compiler does not know about them as native syntax.
    assert_eq!(vm_bootstrap_eval_str("(when t 1 2 3)"), "OK 3");
    assert_eq!(vm_bootstrap_eval_str("(when nil 1 2 3)"), "OK nil");
    assert_eq!(vm_bootstrap_eval_str("(unless nil 1 2 3)"), "OK 3");
    assert_eq!(vm_bootstrap_eval_str("(unless t 1 2 3)"), "OK nil");
}

#[test]
fn vm_syntax_table_accessors_use_shared_current_buffer_state() {
    crate::test_utils::init_test_tracing();
    // syntax-after is defined in subr.el → bootstrap context required.
    assert_eq!(
        vm_bootstrap_eval_str(
            r#"(let ((primary (current-buffer))
                     (other (get-buffer-create "vm-syntax-other")))
                 (set-syntax-table (copy-syntax-table (standard-syntax-table)))
                 (modify-syntax-entry ?\; "<")
                 (erase-buffer)
                 (insert ";")
                 (list (syntax-table-p (syntax-table))
                       (= (char-syntax ?\;) ?<)
                       (consp (syntax-after 1))
                       (= (matching-paren ?\() ?\))
                       (not (eq (syntax-table) (standard-syntax-table)))
                       (progn
                         (set-buffer other)
                         (list (= (char-syntax ?\;) ?.)
                               (eq (syntax-table) (standard-syntax-table))))
                       (progn
                         (set-buffer primary)
                         (= (char-syntax ?\;) ?<))))"#
        ),
        "OK (t t t t t (t t) t)"
    );
}

#[test]
fn vm_dotimes() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_bootstrap_eval_str("(let ((sum 0)) (dotimes (i 5) (setq sum (+ sum i))) sum)"),
        "OK 10"
    );
}

#[test]
fn vm_dolist() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_bootstrap_eval_str(
            "(let ((result nil)) (dolist (x '(a b c)) (setq result (cons x result))) result)"
        ),
        "OK (c b a)"
    );
}

#[test]
fn vm_documentation_and_help_builtins_use_shared_runtime_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_with_init_str(
            "(progn
               (put 'vm-doc-shared 'variable-documentation '(identity \"doc\"))
               ;; `describe-buffer-bindings' now really walks the translation
               ;; maps, and `help--describe-command' names each translated
               ;; character with `char-to-name' (mule-cmds.el, which this
               ;; deliberately minimal runtime does not load). Nil means \"no
               ;; name known\", which is the branch GNU takes for most keys.
               (fset 'char-to-name (lambda (_char) nil))
               (list
                 (stringp (documentation 'car))
                 (documentation-property 'vm-doc-shared 'variable-documentation)
                 (documentation-stringp '(\"DOC\" . 7))
                 (describe-buffer-bindings (current-buffer))
                 (condition-case err
                     ;; DESCRIBER is funcalled on each element; `car' is a
                     ;; C DEFUN and signals the same condition on a fixnum that
                     ;; `display-buffer' did.  Measured on GNU 31.0.90:
                     ;; (describe-vector [1] 'car) => (wrong-type-argument listp 1).
                     (describe-vector [1] 'car)
                   (wrong-type-argument (car err)))
                 (condition-case err
                     (help--describe-vector nil nil nil nil nil nil nil)
                   (wrong-type-argument (car err)))))",
            crate::test_utils::load_minimal_gnu_help_runtime,
        ),
        // GNU `Fhelp__describe_vector` starts with CHECK_VECTOR_OR_CHAR_TABLE,
        // so nil signals rather than returning nil (verified against 31.0.90).
        "OK (t \"doc\" t nil wrong-type-argument wrong-type-argument)"
    );
}

#[test]
fn vm_documentation_and_property_respect_raw_substitute_command_keys_semantics() {
    crate::test_utils::init_test_tracing();
    // The VM runtime harness (`Context::new_vm_runtime_harness`) is
    // a near-bare context — it has the C-level subrs but does not
    // run loadup.el, so `substitute-command-keys' (defined in
    // `lisp/help.el') is not bound. The test asserts that
    // (documentation X) substitutes `\[command]', which requires
    // help.el to have provided substitute-command-keys. Use the
    // init helper to load enough of the GNU runtime for the
    // substitute path to be reachable, mirroring R1's fix for the
    // analogous doc_test.rs failures.
    assert_eq!(
        vm_eval_with_init_str(
            r#"(progn
                 (fset 'vm-doc-fn (lambda () t))
                 (put 'vm-doc-fn 'function-documentation "Press \\[save-buffer] to save.")
                 (put 'vm-doc-prop 'variable-documentation "Press \\[save-buffer] to save.")
                 (let ((doc (documentation 'vm-doc-fn))
                       (raw-doc (documentation 'vm-doc-fn t))
                       (prop (documentation-property 'vm-doc-prop 'variable-documentation))
                       (raw-prop (documentation-property 'vm-doc-prop 'variable-documentation t)))
                   (list (not (eq ?\\ (aref doc 6)))
                         (eq ?\\ (aref raw-doc 6))
                         (not (eq ?\\ (aref prop 6)))
                         (eq ?\\ (aref raw-prop 6)))))"#,
            |eval| {
                crate::test_utils::load_minimal_gnu_help_runtime(eval);
            },
        ),
        "OK (t t t t)"
    );
}

#[test]
fn vm_macroexpand_preserves_visible_lexical_binding_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::runtime_startup_context();
    eval.eval_str(
        "(progn
           (require 'bytecomp)
           (defalias 'vm-lb-macro
             (cons 'macro
                   (byte-compile
                    (lambda (&rest _)
                      lexical-binding)))))",
    )
    .expect("install compiled macro expander");

    let _ = eval
        .try_set_runtime_binding_by_id(neovm_value::intern::intern("lexical-binding"), Value::T);
    neovm_eval::test_support::authority::set_lexenv_for_test(&mut eval, Value::NIL);

    let result = eval.eval_str("(list (macroexpand '(vm-lb-macro)) lexical-binding)");
    assert_eq!(crate::emacs_core::format_eval_result(&result), "OK (t t)");
}
