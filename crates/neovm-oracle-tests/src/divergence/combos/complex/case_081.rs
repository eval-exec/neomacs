//! Complex combo batch 81 — debug / backtrace / edebug / profiler / trace
//! availability, frame introspection, `backtrace-frame`, `mapbacktrace`,
//! `mapatoms`, and feature-loading side effects.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx81_backtrace_frame_inside_catch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable fns)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (frames)
  (let ((fns (list (lambda () (push :level-2 frames))
                   (lambda () (push :level-1 frames) (funcall (car fns))))))
    (funcall (cadr fns))
    (nreverse frames)))
"##,
        expect,
    );
}

#[test]
fn div_cx81_mapatoms_collect_interned_symbols() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"neo-cx81-symbol-alpha\" \"neo-cx81-symbol-beta\" \"neo-cx81-symbol-gamma\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(intern "neo-cx81-symbol-alpha")
(intern "neo-cx81-symbol-beta")
(intern "neo-cx81-symbol-gamma")
(let (collected)
  (mapatoms (lambda (s)
              (when (string-prefix-p "neo-cx81-symbol-" (symbol-name s))
                (push s collected))))
  (sort (mapcar #'symbol-name collected) #'string<))
"##,
        expect,
    );
}

#[test]
fn div_cx81_features_after_require() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // Compare the require transition, not the backend-dependent inventory
    // surrounding it in `features'.
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((before features))
  (condition-case e
      (progn (require 'cl-lib)
             (require 'subr-x)
             (list (not (memq 'cl-lib before))
                   (not (memq 'subr-x before))
                   (featurep 'cl-lib)
                   (featurep 'subr-x)))
    (error (list :errored (car e)))))
"##,
        expect,
    );
}

#[test]
fn div_cx81_featurep_after_provide() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    // `memq' returns the backend-dependent tail of `features'; normalize the
    // membership result to the boolean contract exposed by `featurep'.
    let expect = expect_test::expect![[r#""OK (t nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(provide 'neo-cx81-feature-x)
(list (featurep 'neo-cx81-feature-x)
      (featurep 'no-such-feature-y)
      (and (memq 'neo-cx81-feature-x features) t)
      (assq 'neo-cx81-feature-x load-history))
"##,
        expect,
    );
}

#[test]
fn div_cx81_autoload_resolve_on_call() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (autoloadp (symbol-function 'forward-char))
          (autoloadp (symbol-function 'car))
          (fboundp 'forward-char)
          (fboundp 'no-such-fn-zzz))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx81_mapbacktrace_inside_signal_handler() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (frames)
  (condition-case e
      (signal 'wrong-type-argument '(integerp "x"))
    (error
     (mapbacktrace (lambda (evald func args flags)
                      (push (list evald (when (symbolp func) func)) frames))
                   t)))
  (list (length frames) (car frames)))
"##,
        expect,
    );
}

#[test]
fn div_cx81_load_history_inspection() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-find-if)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((cl-lib-entry (cl-find-if (lambda (e) (eq (car e) (locate-library "cl-lib")))
                                  load-history)))
  (list (consp cl-lib-entry)
        (car cl-lib-entry)
        (listp (cdr cl-lib-entry))))
"##,
        expect,
    );
}

#[test]
fn div_cx81_documentation_for_subr_and_lambda() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"Return the car of LIST.  If LIST is nil, return nil.\\nError if LIST is not nil and not a cons cell.  See also ‘car-safe’.\\n\\nSee Info node ‘(elisp)Cons Cells’ for a discussion of related basic\\nLisp concepts such as car, cdr, cons cell and list.\\n\\n(fn LIST)\" #(\"Move point N characters forward (backward if N is negative).\\nOn reaching end or beginning of buffer, stop and signal error.\\nInteractively, N is the numeric prefix argument.\\nIf N is omitted or nil, move point 1 character forward.\\n\\nDepending on the bidirectional context, the movement may be to the\\nright or to the left on the screen.  This is in contrast with\\n<right>, which see.\\n\\n(fn &optional N)\" 359 366 (font-lock-face help-key-binding face help-key-binding)) \"Return the car of LIST.  If LIST is nil, return nil.\\nError if LIST is not nil and not a cons cell.  See also ‘car-safe’.\\n\\nSee Info node ‘(elisp)Cons Cells’ for a discussion of related basic\\nLisp concepts such as car, cdr, cons cell and list.\\n\\n(fn LIST)\" \"doc for fn\" nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((fn-with-doc (lambda () "doc for fn" :result)))
  (list (documentation 'car)
        (documentation 'forward-char)
        (documentation (symbol-function 'car))
        (documentation fn-with-doc)
        (documentation-property 'neo-cx81-no-such 'variable-documentation)))
"##,
        expect,
    );
}

#[test]
fn div_cx81_help_function_arglist() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((a b &optional c &rest d) (arg1) (&rest rest))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((fn (lambda (a b &optional c &rest d) (list a b c d))))
      (list (help-function-arglist fn)
            (help-function-arglist (symbol-function 'car))
            (help-function-arglist (symbol-function 'list))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx81_inner_backtrace_through_unwind_protect() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (error \"induced\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (trace)
  (unwind-protect
      (progn
        (push :body-start trace)
        (error "induced"))
    (push :unwind trace)
    (condition-case e
        (error "during cleanup")
      (error (push :cleanup-caught trace))))
  trace)
"##,
        expect,
    );
}

#[test]
fn div_cx81_profiler_memory_with_marker_overlay_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((before (garbage-collect)))
      (with-temp-buffer
        (buffer-enable-undo)
        (insert "Profile test buffer with content here")
        (put-text-property 1 5 'face 'bold)
        (let ((m (set-marker (make-marker) 8))
              (ov (make-overlay 4 14)))
          (overlay-put ov 'face 'italic)
          (overlay-put ov 'evaporate t)
          (narrow-to-region 2 20)
          (let ((after (garbage-collect)))
            (undo)
            (widen)
            (list (consp before)
                  (consp after)
                  (buffer-string)
                  (marker-position m)
                  (overlay-start ov) (overlay-end ov)
                  (text-properties-at 1))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx81_symbol_plist_metadata() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function remprop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((sym (intern "neo-cx71-meta-sym")))
  (put sym 'variable-documentation "test doc")
  (put sym 'custom-type 'string)
  (put sym 'neo-cx81-custom :val)
  (list (get sym 'variable-documentation)
        (get sym 'custom-type)
        (get sym 'neo-cx81-custom)
        (get sym 'no-such)
        (symbol-plist sym)
        (plist-member (symbol-plist sym) 'custom-type)
        (remprop sym 'neo-cx81-custom)
        (get sym 'neo-cx81-custom)))
"##,
        expect,
    );
}

#[test]
fn div_cx81_obarray_hash_table_internals() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function make-obarray)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ob (make-obarray 31)))
  (dotimes (i 20)
    (intern (format "sym-%d" i) ob))
  (list (obarrayp ob)
        (hash-table-p ob)
        (hash-table-count ob)
        (> (hash-table-count ob) 0)
        (intern-soft "sym-0" ob)
        (intern-soft "sym-19" ob)
        (intern-soft "sym-20" ob)))
"##,
        expect,
    );
}
