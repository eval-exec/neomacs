/// Batch 508: error signaling characterization — different error types and conditions.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx508_error_wrong_type_arg() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (wrong-type-argument listp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (car 1) (error (list (car e) (cadr e))))
"##,
        expect,
    );
}

#[test]
fn div_cx508_error_wrong_number_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (wrong-number-of-arguments car)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (car 1 2 3) (error (list (car e) (cadr e))))
"##,
        expect,
    );
}

#[test]
fn div_cx508_error_void_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (void-function nonexistent-fn-12345)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (nonexistent-fn-12345) (error (list (car e) (cadr e))))
"##,
        expect,
    );
}

#[test]
fn div_cx508_error_void_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (void-variable nonexistent-var-12345)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e nonexistent-var-12345 (error (list (car e) (cadr e))))
"##,
        expect,
    );
}

#[test]
fn div_cx508_error_args_out_of_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (args-out-of-range [1])""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (aref [1] 5) (error (list (car e) (cadr e))))
"##,
        expect,
    );
}

#[test]
fn div_cx508_error_beginning_of_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (goto-char 0) (error (list (car e) (cadr e))))
"##,
        expect,
    );
}

#[test]
fn div_cx508_error_end_of_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (end-of-buffer nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (forward-char 99999) (error (list (car e) (cadr e))))
"##,
        expect,
    );
}

#[test]
fn div_cx508_error_text_read_only() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (text-read-only nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (let ((inhibit-read-only nil))
      (with-temp-buffer
        (insert "hello")
        (put-text-property 1 6 'read-only t)
        (delete-region 1 3)))
  (error (list (car e) (cadr e))))
"##,
        expect,
    );
}

#[test]
fn div_cx508_error_coding_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (coding-system-error nonexistent-coding)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (encode-coding-string "hello" 'nonexistent-coding)
  (error (list (car e) (cadr e))))
"##,
        expect,
    );
}

#[test]
fn div_cx508_error_file_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (file-missing \"Opening input file\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (insert-file-contents "/nonexistent-file-12345")
  (error (list (car e) (cadr e))))
"##,
        expect,
    );
}

#[test]
fn div_cx508_error_process_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (file-missing \"Searching for program\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (make-process :name "nonexistent-proc" :command '("nonexistent-command-xyzzy"))
  (error (list (car e) (cadr e))))
"##,
        expect,
    );
}

#[test]
fn div_cx508_error_scan_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (void-function scan-error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (scan-error "test") (error (list (car e) (cadr e))))
"##,
        expect,
    );
}

#[test]
fn div_cx508_error_user_signal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (error \"Invalid error symbol\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (signal 'my-error '(test-data))
  (error (list (car e) (cadr e))))
"##,
        expect,
    );
}

#[test]
fn div_cx508_error_overflow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 2305843009213693952""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (/ most-negative-fixnum -1) (error (list (car e) (cadr e))))
"##,
        expect,
    );
}

#[test]
fn div_cx508_error_range_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"����\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (char-to-string #x110000) (error (list (car e) (cadr e))))
"##,
        expect,
    );
}
