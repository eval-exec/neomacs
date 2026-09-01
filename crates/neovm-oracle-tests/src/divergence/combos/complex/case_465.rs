/// Batch 465: process-lines, make-temp-file deep, backup ops, signal-process.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx465_process_lines_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"hello\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (process-lines "echo" "hello")
  (error (car e)))"##,
        expect,
    );
}

#[test]
fn div_cx465_process_lines_ignore() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (process-lines-ignore-status "sh" "-c" "exit 0")
  (error (car e)))"##,
        expect,
    );
}

#[test]
fn div_cx465_signal_process_self() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (signal-process (emacs-pid) 0)
  (error (car e)))"##,
        expect,
    );
}

#[test]
fn div_cx465_backup_file_ops_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 4 52)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (backup-file-name-p "/tmp/test.el~")
      (backup-file-name-p "/tmp/test.el")
      (file-name-sans-versions "/tmp/test.el~")
      (file-name-sans-versions "/tmp/test.el.~1~")))"##,
        expect,
    );
}

#[test]
fn div_cx465_file_name_split_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"\" \"a\" \"b\" \"c.d\") (\"a\" \"b\" \"c\") (\"\" \"\" \"\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (file-name-split "/a/b/c.d")
      (file-name-split "a/b/c")
      (file-name-split "/"))
"##,
        expect,
    );
}

#[test]
fn div_cx465_file_name_concat() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"/tmp/sub/file.txt\" \"/tmp/file.txt\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (file-name-concat "/tmp" "sub" "file.txt")
      (file-name-concat "/tmp" "file.txt"))"##,
        expect,
    );
}

#[test]
fn div_cx465_make_temp_file_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t \"content\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((f (make-temp-file "neo-cx465-mtf-" nil ".txt" "content")))
  (unwind-protect
      (list (file-exists-p f)
            (string-suffix-p ".txt" f)
            (with-temp-buffer (insert-file-contents f) (buffer-string)))
    (delete-file f)))"##,
        expect,
    );
}

#[test]
fn div_cx465_file_writable_directory() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (file-writable-p "/tmp")
      (file-writable-p "/nonexistent-dir-cx465"))"##,
        expect,
    );
}

#[test]
fn div_cx465_directory_files_no_dots() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((d (make-temp-file "neo-cx465-dir-" t)))
  (with-temp-file (expand-file-name ".hidden" d) (insert "x"))
  (unwind-protect
      (length (directory-files d nil "^[^.]"))
    (delete-directory d t)))"##,
        expect,
    );
}

#[test]
fn div_cx465_format_time_string_fmt() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"2024-06-16 14:30:00\" \"Sunday, 16 June 2024\" \"1718562600\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((t1 (encode-time 0 30 14 16 6 2024 nil)))
  (list (format-time-string "%Y-%m-%d %H:%M:%S" t1)
        (format-time-string "%A, %d %B %Y" t1)
        (format-time-string "%s" t1)))"##,
        expect,
    );
}

#[test]
fn div_cx465_decode_time_extended() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2024 6 16 14 30 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((dt (decode-time (encode-time 0 30 14 16 6 2024 nil))))
  (list (decoded-time-year dt) (decoded-time-month dt)
        (decoded-time-day dt) (decoded-time-hour dt)
        (decoded-time-minute dt) (decoded-time-second dt)))"##,
        expect,
    );
}

#[test]
fn div_cx465_time_elapse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function time-elapse)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((t1 (current-time)))
  (cadr (time-elapse t1)))"##,
        expect,
    );
}

#[test]
fn div_cx465_process_file_dest() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"hello world\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (process-file "echo" nil '(t nil) nil "hello world")
  (string-trim-right (buffer-string)))"##,
        expect,
    );
}

#[test]
fn div_cx465_call_process_to_string_dest() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"call-process-test\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (call-process "echo" nil '(t nil) nil "call-process-test")
  (string-trim-right (buffer-string)))"##,
        expect,
    );
}

#[test]
fn div_cx465_rename_buffer_no_unique() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \" *cx465-rn-new*\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((b (get-buffer-create " *cx465-rn*")))
  (rename-buffer " *cx465-rn-new*" t)
  (buffer-name))"##,
        expect,
    );
}
