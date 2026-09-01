//! Divergence tests: buffer-local variables, defaults, and kill ring.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_buffer_local_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (42 0 t 42)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar my-bl-test 0)
  (set (make-local-variable 'my-bl-test) 42)
  (list my-bl-test
        (default-value 'my-bl-test)
        (local-variable-p 'my-bl-test)
        (buffer-local-value 'my-bl-test (current-buffer))))"#,
        expect,
    );
}

#[test]
fn divergence_kill_buffer_local() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0 nil 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar my-bl-kill 0)
  (set (make-local-variable 'my-bl-kill) 99)
  (kill-local-variable 'my-bl-kill)
  (list my-bl-kill
        (local-variable-p 'my-bl-kill)
        (default-value 'my-bl-kill)))"#,
        expect,
    );
}

#[test]
fn divergence_default_toplevel() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (20 20)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar my-dt-var 10)
  (setq-default my-dt-var 20)
  (list my-dt-var
        (default-value 'my-dt-var)))"#,
        expect,
    );
}

#[test]
fn divergence_buffer_local_value_across_buffers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0 55 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar my-cross-buf-var 0)
  (let ((buf (generate-new-buffer " *cross-buf-test*")))
    (with-current-buffer buf
      (set (make-local-variable 'my-cross-buf-var) 55))
    (prog1
        (list my-cross-buf-var
              (buffer-local-value 'my-cross-buf-var buf)
              (default-value 'my-cross-buf-var))
      (kill-buffer buf))))"#,
        expect,
    );
}

#[test]
fn divergence_kill_ring_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"third\" \"second\" \"first\" 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((kill-ring nil))
  (kill-new "first")
  (kill-new "second")
  (kill-new "third")
  (list (car kill-ring)
        (nth 1 kill-ring)
        (nth 2 kill-ring)
        (length kill-ring)))"#,
        expect,
    );
}

#[test]
fn divergence_kill_ring_append() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"b\" \"b\" \"b\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((kill-ring nil))
  (kill-new "a")
  (kill-new "b" t)
  (list (car kill-ring)
        (current-kill 0)
        (current-kill 1)))"#,
        expect,
    );
}

#[test]
fn divergence_kill_ring_max_size() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 \"4\" \"3\" \"2\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((kill-ring nil)
        (kill-ring-max 3))
  (dotimes (i 5)
    (kill-new (number-to-string i)))
  (list (length kill-ring)
        (car kill-ring)
        (nth 1 kill-ring)
        (nth 2 kill-ring)))"#,
        expect,
    );
}

#[test]
fn divergence_with_temp_file() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t \"hello\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((tmp (make-temp-file "neovm-test-")))
  (unwind-protect
      (progn
        (write-region "hello" nil tmp nil 'silent)
        (list (file-exists-p tmp)
              (file-readable-p tmp)
              (file-writable-p tmp)
              (with-temp-buffer
                (insert-file-contents tmp)
                (buffer-string))))
    (delete-file tmp)))"#,
        expect,
    );
}

#[test]
fn divergence_expand_file_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (\"/foo/bar/\" \"baz.el\" \"gz\" \"test\" \"file\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (file-name-directory "/foo/bar/baz.el")
  (file-name-nondirectory "/foo/bar/baz.el")
  (file-name-extension "test.tar.gz")
  (file-name-sans-extension "test.el")
  (file-name-base "/path/to/file.el"))"#,
        expect,
    );
}

#[test]
fn divergence_directory_files() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"a.txt\" \"b.txt\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((tmp (make-temp-file "neovm-dir-test-" t)))
  (unwind-protect
      (progn
        (write-region "a" nil (expand-file-name "a.txt" tmp) nil 'silent)
        (write-region "b" nil (expand-file-name "b.txt" tmp) nil 'silent)
        (sort (directory-files tmp nil "\\.txt$") #'string<))
    (delete-directory tmp t)))"#,
        expect,
    );
}

#[test]
fn divergence_env_vars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"value\" nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((process-environment (list "NEOVM_TEST_ENV=value")))
  (list
   (getenv "NEOVM_TEST_ENV")
   (getenv "NEOVM_TEST_MISSING")
   (stringp (getenv "NEOVM_TEST_ENV"))
   (booleanp (getenv "NEOVM_TEST_ENV"))))"#,
        expect,
    );
}
