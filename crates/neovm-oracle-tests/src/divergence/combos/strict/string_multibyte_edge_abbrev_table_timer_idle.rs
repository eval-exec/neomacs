//! Strict combo oracle probes, batch 133: string multibyte edge cases
//! (surrogate pairs, invalid sequences), abbrev table operations, idle
//! timer, and with-current-buffer on killed buffer edge.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_u7_string_multibyte_surrogate_and_invalid() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (condition-case err (string 55296) (error (car err)))
      (condition-case err (string 1114112) (error (car err)))
      (condition-case err (string 1114111) (error (car err)))
      (length (string 128578))
      (string-bytes (string 128578))
      (condition-case err (char-to-string 1114112) (error (car err)))
      (condition-case err (format "%c" 55296) (error (car err)))
      (condition-case err (aref "ab" 5) (args-out-of-range 'caught) (error 'other))
      (condition-case err (aset "abc" nil ?x) (wrong-type-argument 'caught) (error 'other)))
"##;
    let expect = expect_test::expect![[
        r#""OK (\"���\" \"����\" \"\u{10ffff}\" 1 4 \"����\" \"���\" caught caught)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_u7_abbrev_table_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((table (make-abbrev-table)))
  (define-abbrev table "foo" "bar")
  (define-abbrev table "baz" "qux" nil :count 0)
  (list (abbrev-table-p table)
        (symbol-function (abbrev-symbol "foo" table))
        (abbrev-symbol "missing" table)
        (abbrev-expansion "foo" table)
        (with-temp-buffer
          (setq-local local-abbrev-table table)
          (insert "foo")
          (expand-abbrev)
          (buffer-string))))
"##;
    let expect = expect_test::expect![[r#""OK (t nil nil \"bar\" \"bar\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_u7_idle_timer_and_timer_idle_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((tm (run-with-idle-timer 1000 nil (lambda () nil))))
  (list (timerp tm)
        (memq tm timer-idle-list)
        (not (memq tm timer-list))
        (timer--idle-delay tm)
        (progn (cancel-timer tm)
               (not (memq tm timer-idle-list)))))
"##;
    let expect = expect_test::expect![[
        r#""OK (t ([nil 0 1000 0 nil (closure (t) nil nil) nil idle 0 nil]) t idle t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_u7_with_current_buffer_killed_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((b (generate-new-buffer " *probe-wcbk*")))
  (with-current-buffer b (insert "test"))
  (kill-buffer b)
  (list (buffer-live-p b)
        (condition-case err (with-current-buffer b (buffer-string))
          (error (car err)))
        (condition-case err (set-buffer b)
          (error (car err)))
        (condition-case err (buffer-name b)
          (error (car err)))))
"##;
    let expect = expect_test::expect![[r#""OK (nil error error nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_u7_map_concat_mapcan_filter_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (mapcan (lambda (x) (if (cl-evenp x) (list x) nil)) '(1 2 3 4 5 6))
      (mapcan #'list '((1 2) (3 4) (5 6)))
      (mapcon (lambda (l) (list (car l))) '(a b c))
      (mapl (lambda (l) nil) '(1 2 3))
      (cl-remove-if (lambda (x) (cl-oddp x)) '(1 2 3 4 5))
      (cl-remove-if-not #'cl-evenp '(1 2 3 4 5))
      (cl-substitute 'X 2 '(1 2 3 2 1))
      (cl-substitute 'X 2 '(1 2 3 2 1) :count 1)
      (cl-position 2 '(1 2 3 2 1) :from-end t))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function cl-evenp)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
