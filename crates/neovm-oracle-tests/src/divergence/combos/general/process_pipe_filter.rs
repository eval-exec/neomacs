//! Divergence tests: process + pipe + filter + output combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_process_output_to_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " test-proc-xxx")))
    (with-current-buffer buf
      (let ((proc (start-process "test-proc-xxx" buf "echo" "hello world")))
        (set-process-query-on-exit-flag proc nil)
        (while (process-live-p proc) (accept-process-output proc 1))
        (let ((output (with-current-buffer buf (buffer-string))))
          (kill-buffer buf)
          (list (string-match "hello world" output)
                (>= (length output) 11)
                (stringp output))))))) "#,
        expect,
    );
}

#[test]
fn divergence_process_exit_status() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((proc (start-process "test-exit-xxx" nil "sh" "-c" "exit 42")))
    (set-process-query-on-exit-flag proc nil)
    (while (process-live-p proc) (accept-process-output proc 1))
    (list (process-exit-status proc)
          (= (process-exit-status proc) 42)
          (eq (process-status proc) 'exit)
          (null (process-live-p proc)))) "#,
        expect,
    );
}

#[test]
fn divergence_process_arg_parsing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " test-args-xxx")))
    (with-current-buffer buf
      (let ((proc (start-process "test-args-xxx" buf "printf" "%s-%s" "hello" "world")))
        (set-process-query-on-exit-flag proc nil)
        (while (process-live-p proc) (accept-process-output proc 1))
        (let ((output (with-current-buffer buf (buffer-string))))
          (kill-buffer buf)
          (list (string= output "hello-world")
                (= (length output) 11)
                (stringp output)))))) "#,
        expect,
    );
}

#[test]
fn divergence_shell_command_output() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((out (shell-command-to-string "echo test123")))
    (list (string= out "test123\n")
          (= (length out) 8)
          (string-match "test123" out)))) "#,
        expect,
    );
}

#[test]
fn divergence_process_environment_var() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t 0 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((out (shell-command-to-string "echo $HOME")))
    (list (> (length out) 1)
          (string-match "^/" out)
          (= (string-match "^/" out) 0)
          (not (null (string-match "\n\\'" out)))))) "#,
        expect,
    );
}

#[test]
fn divergence_process_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function every)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list (listp (process-list))
        (every (lambda (p) (processp p)) (process-list))
        (listp (process-names)))) "#,
        expect,
    );
}

#[test]
fn divergence_call_process_output() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " test-call-xxx")))
    (call-process "echo" nil buf nil "call-test")
    (let ((output (with-current-buffer buf (buffer-string))))
      (kill-buffer buf)
      (list (string= output "call-test\n")
            (= (length output) 10))))) "#,
        expect,
    );
}

#[test]
fn divergence_two_sequential_processes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf1 (generate-new-buffer " test-seq1-xxx"))
        (buf2 (generate-new-buffer " test-seq2-xxx")))
    (let ((p1 (start-process "test-seq1-xxx" buf1 "echo" "first-output")))
      (set-process-query-on-exit-flag p1 nil)
      (while (process-live-p p1) (accept-process-output p1 1)))
    (let ((p2 (start-process "test-seq2-xxx" buf2 "echo" "second-output")))
      (set-process-query-on-exit-flag p2 nil)
      (while (process-live-p p2) (accept-process-output p2 1)))
    (let ((out1 (with-current-buffer buf1 (buffer-string)))
          (out2 (with-current-buffer buf2 (buffer-string))))
      (kill-buffer buf1)
      (kill-buffer buf2)
      (list (string= out1 "first-output\n")
            (string= out2 "second-output\n")
            (= (length out1) 13)
            (= (length out2) 14))))) "#,
        expect,
    );
}

#[test]
fn divergence_call_process_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "INPUT DATA FOR CALL")
  (let ((buf (generate-new-buffer " test-cpr-xxx")))
    (call-process-region 1 19 "cat" nil buf)
    (let ((output (with-current-buffer buf (buffer-string))))
      (kill-buffer buf)
      (list (string= output "INPUT DATA FOR CALL")
            (= (length output) 19))))) "#,
        expect,
    );
}

#[test]
fn divergence_process_multiline_output() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " test-ml-xxx")))
    (let ((proc (start-process "test-ml-xxx" buf "sh" "-c"
                               "echo line1; echo line2; echo line3")))
      (set-process-query-on-exit-flag proc nil)
      (while (process-live-p proc) (accept-process-output proc 1))
      (let ((output (with-current-buffer buf (buffer-string)))
            (lines nil))
        (with-current-buffer buf
          (goto-char 1)
          (while (not (eobp))
            (push (buffer-substring (line-beginning-position)
                                    (line-end-position))
                  lines)
            (forward-line 1)))
        (kill-buffer buf)
        (list (= (length (nreverse lines)) 4)
              (string= output "line1\nline2\nline3\n")))))) "#,
        expect,
    );
}
