//! Divergence tests: subprocess + buffer + text property combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_call_process_propertize_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (10 nil (#(\"10\" 0 2 (parity even)) even) nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (require 'cl-lib)
  (with-temp-buffer
  (call-process \"seq\" nil t nil \"1\" \"10\")
  (goto-char 1)
  (while (re-search-forward \"[0-9]+\" nil t)
    (let ((n (string-to-number (match-string 0))))
      (put-text-property (match-beginning 0) (match-end 0)
                         'parity (if (cl-evenp n) 'even 'odd))))
  (let ((result nil))
    (goto-char 1)
    (while (re-search-forward \"[0-9]+\" nil t)
      (push (list (match-string 0)
                  (get-text-property (match-beginning 0) 'parity)) result))
    (list (length (nreverse result))
          (= (length (nreverse result)) 10)
          (nth 0 (nreverse result))
          (nth 1 (nreverse result)))))) ",
        expect,
    );
}

#[test]
fn divergence_shell_parse_into_hash() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 \"val1\" \"val2\" \"val3\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let* ((output (shell-command-to-string \"echo key1:val1; echo key2:val2; echo key3:val3\"))
        (lines (split-string output \"\\n\" t))
        (ht (make-hash-table :test 'equal)))
  (dolist (l lines)
    (let ((parts (split-string l \":\" t)))
      (when (= (length parts) 2)
        (puthash (nth 0 parts) (nth 1 parts) ht))))
  (list (hash-table-count ht)
        (gethash \"key1\" ht)
        (gethash \"key2\" ht)
        (gethash \"key3\" ht)
        (= (hash-table-count ht) 3))) ", expect);
}

#[test]
fn divergence_temp_file_write_read_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"Hello World\" nil 11)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((tmp (make-temp-file \"test-io-\")))
  (with-temp-buffer
    (insert \"Hello World\")
    (put-text-property 1 6 'face 'bold)
    (write-region (point-min) (point-max) tmp nil 'silent))
  (let ((result
         (with-temp-buffer
           (insert-file-contents tmp)
           (list (buffer-string)
                 (get-text-property 1 'face)
                 (length (buffer-string))))))
    (delete-file tmp)
    result)) ",
        expect,
    );
}

#[test]
fn divergence_multi_call_process_accumulate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (7 14 \"first\\nsecond\\nthird\\n\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(with-temp-buffer
  (call-process \"echo\" nil t nil \"first\")
  (let ((m1 (set-marker (make-marker) (point-max))))
    (call-process \"echo\" nil t nil \"second\")
    (let ((m2 (set-marker (make-marker) (point-max))))
      (call-process \"echo\" nil t nil \"third\")
      (list (marker-position m1) (marker-position m2)
            (buffer-string)
            (>= (marker-position m2) (marker-position m1)))))) ",
        expect,
    );
}

#[test]
fn divergence_call_process_region_transform() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"HELLO WORLD\" nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(with-temp-buffer
  (insert \"hello world\")
  (call-process-region (point-min) (point-max) \"tr\" t t nil \"a-z\" \"A-Z\")
  (list (buffer-string)
        (string= (buffer-string) \"HELLO WORLD\\n\")
        (string= (string-trim (buffer-string)) \"HELLO WORLD\"))) ",
        expect,
    );
}

#[test]
fn divergence_env_var_subprocess() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let* ((result (shell-command-to-string \"echo $HOME\"))
        (trimmed (string-trim result)))
  (list (stringp trimmed)
        (> (length trimmed) 1)
        (string-prefix-p \"/\" trimmed))) ",
        expect,
    );
}

#[test]
fn divergence_process_list_with_temp_buffers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"test\\n\" t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((before (length (process-list))))
  (with-temp-buffer
    (call-process \"echo\" nil t nil \"test\")
    (list (buffer-string)
          (= (length (process-list)) before)
          (process-list)))) ",
        expect,
    );
}

#[test]
fn divergence_shell_command_output_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let* ((raw (shell-command-to-string \"echo line1; echo line2; echo line3\"))
        (lines (split-string raw \"\\n\" t)))
  (list (length lines)
        (>= (length lines) 3)
        (string= (nth 0 lines) \"line1\")
        (string= (nth 2 lines) \"line3\"))) ",
        expect,
    );
}

#[test]
fn divergence_insert_file_contents_with_encoding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"café\\n\" nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((tmp (make-temp-file \"test-enc-\")))
  (write-region \"caf\\u00e9\\n\" nil tmp nil 'silent)
  (let ((result
         (with-temp-buffer
           (insert-file-contents tmp)
           (list (buffer-string)
                 (= (length (buffer-string)) 6)
                 (string= (buffer-string) \"caf\\u00e9\\n\")))))
    (delete-file tmp)
    result)) ",
        expect,
    );
}

#[test]
fn divergence_write_read_large_temp() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (100 t 1 t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((tmp (make-temp-file \"test-large-\")))
  (with-temp-buffer
    (dotimes (i 100) (insert (format \"Line %04d\\n\" i)))
    (write-region (point-min) (point-max) tmp nil 'silent))
  (let ((result
         (with-temp-buffer
           (insert-file-contents tmp)
           (list (count-lines 1 (point-max))
                 (= (count-lines 1 (point-max)) 100)
                 (goto-char 1)
                 (string= (buffer-substring (line-beginning-position)
                                            (line-end-position)) \"Line 0000\")))))
    (delete-file tmp)
    result)) ",
        expect,
    );
}
