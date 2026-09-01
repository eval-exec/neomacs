//! Strict combo oracle probes, batch 141: advice reapply after fset
//! redefinition, condition-case + unwind-protect + catch deep combo,
//! abbrev system table interaction, and window-size-change-functions.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v5_advice_reapply_after_fset_redefinition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((log nil))
  (defun probe-adv-rd (x) (push 'orig log) (* x 2))
  (let ((around (lambda (fn x) (push 'around log) (funcall fn (+ x 1)))))
    (advice-add 'probe-adv-rd :around around)
    (let ((r1 (probe-adv-rd 5)))
      (fset 'probe-adv-rd (lambda (x) (push 'new log) (+ x 10)))
      (let ((r2 (probe-adv-rd 5)))
        (advice-remove 'probe-adv-rd around)
        (let ((r3 (probe-adv-rd 5)))
          (list r1 r2 r3 (nreverse log))))
    (fmakunbound 'probe-adv-rd)))
"##;
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v5_condition_case_unwind_catch_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((log nil))
  (catch 'outer
    (condition-case err
        (unwind-protect
            (dotimes (i 5)
              (push i log)
              (when (= i 2)
                (condition-case inner-err
                    (signal 'arith-error '("inner"))
                  (arith-error
                   (push 'caught-inner log)
                   (throw 'outer 'escaped)))
                (push 'unreachable log)))
          (push 'cleanup log))
        (error (push (cons 'caught (cdr err)) log))))
  (nreverse log))
"##;
    let expect = expect_test::expect![[r#""OK (0 1 2 caught-inner cleanup)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v5_abbrev_system_table_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((local-table (make-abbrev-table)))
  (define-abbrev local-table "exp1" "expanded1")
  (define-abbrev local-table "exp2" "expanded2" nil :count 0)
  (list (abbrev-table-p local-table)
        (abbrev-symbol "exp1" local-table)
        (abbrev-symbol "exp3" local-table)
        (abbrev-expansion "exp1" local-table)
        (abbrev-expansion "exp2" local-table)
        (with-temp-buffer
          (setq-local local-abbrev-table local-table)
          (insert "exp1")
          (expand-abbrev)
          (buffer-string))
        (with-temp-buffer
          (setq-local local-abbrev-table local-table)
          (insert "exp2")
          (expand-abbrev)
          (buffer-string))))
"##;
    let expect = expect_test::expect![[
        r#""OK (t exp1 nil \"expanded1\" \"expanded2\" \"expanded1\" \"expanded2\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v5_window_size_change_functions_hook() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((log nil))
  (let ((b (get-buffer-create " *probe-wscf*")))
    (unwind-protect
        (progn
          (delete-other-windows)
          (switch-to-buffer b)
          (setq window-size-change-functions nil)
          (add-hook 'window-size-change-functions
                    (lambda (frame) (push (framep frame) log)))
          (condition-case err
              (let ((w2 (split-window nil nil 'right)))
                (set-window-buffer w2 b)
                (delete-window w2))
            (error nil))
          (list (length log)
                (windowp (selected-window))
                (count-windows)))
      (kill-buffer b)
      (setq window-size-change-functions nil)
      (delete-other-windows))))
"##;
    let expect = expect_test::expect![[r#""OK (0 t 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v5_coding_system_priority_set_prefer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((orig (coding-system-priority-list)))
  (prefer-coding-system 'latin-1)
  (let ((after-prefer (coding-system-priority-list)))
    (prefer-coding-system 'utf-8)
    (let ((after-utf8 (coding-system-priority-list)))
      (list (car orig)
            (car after-prefer)
            (car after-utf8)
            (memq 'utf-8 after-utf8)
            (memq 'latin-1 after-utf8)))))
"##;
    let expect = expect_test::expect![[
        r#""OK (utf-8 iso-latin-1 utf-8 (utf-8 iso-latin-1 iso-2022-7bit iso-2022-7bit-lock iso-2022-8bit-ss2 emacs-mule raw-text iso-2022-jp in-is13194-devanagari chinese-iso-8bit utf-8-auto utf-8-with-signature utf-16 utf-16be-with-signature utf-16le-with-signature utf-16be utf-16le japanese-shift-jis chinese-big5 undecided) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
