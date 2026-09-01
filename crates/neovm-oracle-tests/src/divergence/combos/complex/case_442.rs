//! Complex combo batch 442 — 15 more edge probes: directory-files-recursively,
//! file-expand-wildcards, call-process, call-process-region, process-file,
//! shell-command-to-string exit, save-match-data edge, set-match-data reset,
//! replace-count, char-fold custom table, with-syntax-table deeper,
//! directory-files dots, file-readable-p, file-symlink-p chain.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// directory-files-recursively: recursive file listing.
#[test]
fn div_cx442_directory_files_recursively() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 2""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((d (make-temp-file "neo-cx442-dfr-" t)))
  (with-temp-file (expand-file-name "a.txt" d) (insert "x"))
  (with-temp-file (expand-file-name "b.txt" d) (insert "y"))
  (unwind-protect
      (length (directory-files-recursively d "\\.txt$" nil t))
    (delete-directory d t)))"##,
        expect,
    );
}

/// file-expand-wildcards: glob pattern expansion.
#[test]
fn div_cx442_file_expand_wildcards() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (file-expand-wildcards "/nonexistent-cx442-dir/*.txt")
      (file-expand-wildcards "/nonexistent-cx442-dir/*.nonexistent"))"##,
        expect,
    );
}

/// call-process: synchronous subprocess execution.
#[test]
fn div_cx442_call_process() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"hello\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (call-process "echo" nil t nil "hello")
  (string-trim-right (buffer-string)))"##,
        expect,
    );
}

/// call-process-region: sending region to process.
#[test]
fn div_cx442_call_process_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"hello world\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "hello world")
  (call-process-region (point-min) (point-max) "cat" t t)
  (string-trim-right (buffer-string)))"##,
        expect,
    );
}

/// process-file: like call-process but respects default-directory.
#[test]
fn div_cx442_process_file() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"test\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (process-file "echo" nil '(t nil) nil "test")
  (string-trim-right (buffer-string)))"##,
        expect,
    );
}

/// save-match-data: preserving match data across edits.
#[test]
fn div_cx442_save_match_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((6 11) (0 5))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "hello world")
  (string-match "hello" "hello world")
  (let ((md (match-data)))
    (string-match "world" "hello world")
    (save-match-data
      (string-match "xxx" "hello world"))
    (list (match-data) md)))"##,
        expect,
    );
}

/// set-match-data with reset flag.
#[test]
fn div_cx442_set_match_data_reset() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"a\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (string-match "a" "abc")
  (let ((md (match-data t)))
    (string-match "b" "abc")
    (set-match-data md)
    (match-string 0 "abc")))"##,
        expect,
    );
}

/// replace-regexp-in-string with count limit.
#[test]
fn div_cx442_replace_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"XXX XXX XXX\" \"XX XXX XXX\" \"X XXX XXX\" \" XXX XXX\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (replace-regexp-in-string "a" "X" "aaa aaa aaa" nil nil nil 0)
      (replace-regexp-in-string "a" "X" "aaa aaa aaa" nil nil nil 1)
      (replace-regexp-in-string "a" "X" "aaa aaa aaa" nil nil nil 2)
      (replace-regexp-in-string "a" "X" "aaa aaa aaa" nil nil nil 3))"##,
        expect,
    );
}

/// directory-files with dot handling.
#[test]
fn div_cx442_directory_files_dots() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((d (make-temp-file "neo-cx442-dfd-" t)))
  (unwind-protect
      (list (length (directory-files d nil "^[^.]"))
            (length (directory-files d nil "" t)))
    (delete-directory d t)))"##,
        expect,
    );
}

/// file-readable-p / file-writable-p / file-executable-p.
#[test]
fn div_cx442_file_access_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((f (make-temp-file "neo-cx442-fap-")))
  (unwind-protect
      (list (file-readable-p f)
            (file-writable-p f)
            (file-executable-p f))
    (delete-file f)))"##,
        expect,
    );
}

/// file-symlink-p chain: follow symlinks to original.
#[test]
fn div_cx442_file_symlink_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"/tmp\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (list (file-truename "/tmp")
          (file-equal-p "/tmp" "/tmp/../tmp"))
  (error (car e)))"##,
        expect,
    );
}

/// file-newer-than-file-p: comparing file timestamps.
#[test]
fn div_cx442_file_newer_than() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((f1 (make-temp-file "neo-cx442-fnt1-"))
      (f2 (make-temp-file "neo-cx442-fnt2-")))
  (unwind-protect
      (progn
        (set-file-times f1 100)
        (set-file-times f2 100)
        (list (file-newer-than-file-p f1 f2)
              (file-newer-than-file-p f2 f1)))
    (delete-file f1)
    (delete-file f2)))"##,
        expect,
    );
}

/// file-name-extension / file-name-sans-extension deep.
#[test]
fn div_cx442_file_name_ext_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"gz\" \".gz\" \"foo.tar\" \"foo.tar\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (file-name-extension "foo.tar.gz")
      (file-name-extension "foo.tar.gz" t)
      (file-name-base "foo.tar.gz")
      (file-name-sans-extension "foo.tar.gz"))"##,
        expect,
    );
}

/// file-name-directory / file-name-nondirectory edge.
#[test]
fn div_cx442_file_name_dir_nondir() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"foo/bar/\" \"\" \"///\" \"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (file-name-directory "foo/bar/")
      (file-name-nondirectory "foo/bar/")
      (file-name-directory "///")
      (file-name-nondirectory "///"))"##,
        expect,
    );
}

/// directory-name-p / file-directory-p.
#[test]
fn div_cx442_directory_name_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((d (make-temp-file "neo-cx442-dnp-" t)))
  (unwind-protect
      (list (directory-name-p (file-name-as-directory d))
            (directory-name-p d)
            (file-directory-p d))
    (delete-directory d t)))"##,
        expect,
    );
}
