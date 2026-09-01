//! Strict combo oracle probes, batch 137: window pixel-to-text conversion,
//! indent-tabs-mode with tab-to-tab-stop, cl-loop with while+for+collect
//! combo, and string-bytes vs length on unusual codings.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v1_window_pixel_to_text_and_text_to_pixel() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((b (get-buffer-create " *probe-p2t*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (switch-to-buffer b)
        (with-current-buffer b (insert (make-string 500 ?x)))
        (let ((cw (frame-char-width))
              (ch (frame-char-height)))
          (list (condition-case err (posn-at-x-y (* 3 cw) (* 2 ch)) (error 'err))
                (condition-case err (posn-at-x-y 0 0) (error 'err))
                (condition-case err (posn-at-point 1) (error 'err))
                (condition-case err (posn-at-point 100) (error 'err)))))
    (kill-buffer b)
    (delete-other-windows)))
"##;
    let expect = expect_test::expect![[
        r#""OK ((#<window 1 on *scratch*> 162 (3 . 2) 0 nil 162 (3 . 2) nil (0 . 0) (0 . 0)) (#<window 1 on *scratch*> 1 (0 . 0) 0 nil 1 (0 . 0) nil (0 . 0) (0 . 0)) nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v1_indent_tabs_mode_tab_to_tab_stop() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (with-temp-buffer
        (setq-local indent-tabs-mode t)
        (insert "line1\n\tindented\nline3")
        (goto-char 1)
        (forward-line 1)
        (back-to-indentation)
        (current-column))
      (with-temp-buffer
        (setq-local indent-tabs-mode nil)
        (insert "line1\n    indented\n")
        (goto-char 1)
        (forward-line 1)
        (back-to-indentation)
        (current-column))
      (with-temp-buffer
        (insert "abc")
        (goto-char 1)
        (tab-to-tab-stop)
        (current-column)
        (buffer-string))
      (default-value 'indent-tabs-mode)
      (default-value 'tab-stop-list))
"##;
    let expect = expect_test::expect![[r#""OK (8 4 \"\tabc\" t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v1_cl_loop_while_for_collect_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((lst '(a b c d e))
      (idx 0))
  (list (cl-loop while lst
                for x = (pop lst)
                for i from 0
                collect (cons i x)
                when (= i 2) do (push 'found lst)
                while (< i 10))
        (cl-loop for i from 1
                 for j = (* i i)
                 while (< j 50)
                 collect j)
        (cl-loop for c across "hello"
                 for i from 0
                 if (cl-evenp i) collect c into evens
                 else collect c into odds
                 finally (return (list evens odds)))
        (cl-loop for x in '(1 2 3 4 5 6 7 8 9 10)
                 while (< x 6)
                 sum x)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v1_string_bytes_vs_length_unusual() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (length "abc")
      (string-bytes "abc")
      (length "日本語")
      (string-bytes "日本語")
      (length "café")
      (string-bytes "café")
      (length (string 128 200 255))
      (string-bytes (string 128 200 255))
      (length (encode-coding-string "café" 'utf-8))
      (string-bytes (encode-coding-string "café" 'utf-8))
      (length (encode-coding-string "日" 'shift_jis))
      (string-bytes (encode-coding-string "日" 'shift_jis)))
"##;
    let expect = expect_test::expect![[r#""OK (3 3 3 9 4 5 3 6 5 5 2 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v1_compare_strings_case_fold_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (compare-strings "abc" nil nil "ABC" nil nil t)
      (compare-strings "abc" nil nil "ABC" nil nil nil)
      (compare-strings "abc" 0 3 "abc" 0 3)
      (compare-strings "abc" 0 2 "abd" 0 2)
      (compare-strings "abc" 0 3 "abc" 0 2)
      (compare-strings "ABCdef" 0 3 "abcDEF" 0 3 t)
      (compare-strings "HELLO" nil nil "hello" nil nil t))
"##;
    let expect = expect_test::expect![[r#""OK (t 1 t t 3 t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
