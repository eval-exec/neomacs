//! Oracle parity tests for GNU `process-lines*` helper semantics.
//!
//! GNU implements these helpers in `lisp/subr.el` on top of `call-process`.
//! `process-lines-handling-status` deliberately calls STATUS-HANDLER while the
//! temporary output buffer is current and before the buffer is split into
//! lines; handlers can inspect or mutate that buffer.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_prop_gnu_process_lines_splits_trailing_newlines_like_temp_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (process-lines "sh" "-c" "printf 'alpha\nbeta\n'")
 (process-lines "sh" "-c" "printf 'alpha\nbeta'")
 (process-lines-ignore-status "sh" "-c" "printf 'kept\n'; exit 7"))
"#;

    let expect =
        expect_test::expect![[r#""OK ((\"alpha\" \"beta\") (\"alpha\" \"beta\") (\"kept\"))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_gnu_process_lines_status_handler_sees_output_buffer_before_collection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let (handler-current-buffer handler-buffer-string handler-point)
  (list
   (process-lines-handling-status
    "sh"
    (lambda (status)
      (setq handler-current-buffer (buffer-name))
      (setq handler-buffer-string (buffer-string))
      (setq handler-point (point))
      (goto-char (point-max))
      (insert (format "status=%s\n" status)))
    "-c" "printf 'one\ntwo\n'; exit 7")
   handler-current-buffer
   handler-buffer-string
   handler-point))
"#;

    let expect = expect_test::expect![[
        r#""OK ((\"one\" \"two\" \"status=7\") \" *temp*\" \"one\\ntwo\\n\" 9)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_gnu_process_lines_nil_status_handler_errors_after_output() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(condition-case err
    (process-lines-handling-status "sh" nil "-c" "printf 'bad\n'; exit 9")
  (error err))
"#;

    let expect = expect_test::expect![[r#""OK (error \"sh exited with status 9\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
