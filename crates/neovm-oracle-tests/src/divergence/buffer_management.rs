//! Divergence tests: buffer naming, get-buffer, buried buffers, buffer-list.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_buffer_list_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((bufs (buffer-list)))
  (list (> (length bufs) 0)
        (bufferp (car bufs))
        (eq (car bufs) (current-buffer))))"#,
        expect,
    );
}

#[test]
fn divergence_get_buffer_create() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t \" *test-gbc*\" t nil t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((buf (get-buffer-create " *test-gbc*")))
  (list (bufferp buf)
        (buffer-name buf)
        (buffer-live-p buf)
        (bury-buffer buf)
        (eq buf (get-buffer " *test-gbc*"))
        (kill-buffer buf)
        (buffer-live-p buf)))"#,
        expect,
    );
}

#[test]
fn divergence_generate_new_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (\"*test-gnb*\" \"*test-gnb*<2>\" t \" *test-gnb*\" 0 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((visible1 (generate-new-buffer "*test-gnb*"))
        (visible2 (generate-new-buffer "*test-gnb*"))
        (hidden1 (generate-new-buffer " *test-gnb*"))
        (hidden2 (generate-new-buffer " *test-gnb*")))
  (unwind-protect
      (list (buffer-name visible1)
            (buffer-name visible2)
            (not (eq visible1 visible2))
            (buffer-name hidden1)
            (string-match-p "\\` \\*test-gnb\\*-[0-9]+\\'" (buffer-name hidden2))
            (not (eq hidden1 hidden2)))
    (mapc (lambda (buf)
            (when (buffer-live-p buf)
              (kill-buffer buf)))
          (list visible1 visible2 hidden1 hidden2))))"#,
        expect,
    );
}

#[test]
fn divergence_buffer_name_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"test-name-edge\" t #<killed buffer> t \"renamed-edge\" \"test-name-edge\" t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((buf (get-buffer-create "test-name-edge")))
  (list (buffer-name buf)
        (string= (buffer-name buf) "test-name-edge")
        (get-buffer "test-name-edge")
        (eq (get-buffer "test-name-edge") buf)
        (rename-buffer "renamed-edge")
        (buffer-name buf)
        (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn divergence_buffer_file_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (null (buffer-file-name))
  (null (buffer-file-name (current-buffer)))
  (fboundp 'set-visited-file-name)
  (fboundp 'write-file))"#,
        expect,
    );
}

#[test]
fn divergence_buffer_modified() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil nil t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (booleanp (buffer-modified-p))
  (buffer-modified-p)
  (set-buffer-modified-p t)
  (buffer-modified-p)
  (set-buffer-modified-p nil)
  (buffer-modified-p))"#,
        expect,
    );
}

#[test]
fn divergence_buffer_size_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (5 t 6 t 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello")
  (list (buffer-size)
        (= (buffer-size) 5)
        (point-max)
        (= (point-max) 6)
        (buffer-chars-modified-tick)))"#,
        expect,
    );
}

#[test]
fn divergence_buffer_multibyte_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function enable-multibyte-characters)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (buffer-live-p (current-buffer))
  (multibyte-string-p (buffer-string))
  (enable-multibyte-characters)
  (bufferp (current-buffer)))"#,
        expect,
    );
}

#[test]
fn divergence_with_current_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"\" \"in-other\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((buf (generate-new-buffer " *wcb*")))
  (unwind-protect
      (progn
        (with-current-buffer buf
          (insert "in-other"))
        (list (buffer-string)
              (with-current-buffer buf (buffer-string))
              (eq (current-buffer) buf)))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn divergence_buffer_swap_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"original\" \"swapped\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((buf (generate-new-buffer " *swap*")))
  (unwind-protect
      (progn
        (insert "original")
        (with-current-buffer buf (insert "swapped"))
        (list (buffer-string)
              (with-current-buffer buf (buffer-string))))
    (kill-buffer buf)))"#,
        expect,
    );
}
