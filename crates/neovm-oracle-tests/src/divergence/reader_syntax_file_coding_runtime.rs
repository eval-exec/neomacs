//! Obscure reader syntax (#N=/#N# circular + shared refs, #[...] bytecode
//! literal, #& bool-vector literal, \^?/\C-? DEL, #@ skip, \M-\C- meta
//! combos, \x/\u/\U string escapes) and file-coding I/O (write-region/
//! insert-file-contents roundtrip, utf-8 detection, dos eol bytes,
//! set-buffer-file-coding-system, find-operation-coding-system) parity.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn read_bool_vector_literal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function bool-vectorp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((bv (read "#&5\"\\25\"")))
  (list (bool-vectorp bv) (length bv)))"##,
        expect,
    );
}

#[test]
fn read_bytecode_literal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (let ((f (read "#[257 \"\\300\\001\\\\\" [1+] 3]"))) (byte-code-function-p f)) (error (cons (quote ERR) (car e))))"##,
        expect,
    );
}

#[test]
fn read_circular_refs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil (a . #0))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((x (read "(#1=(a . #1#))")))
  (list (eq x (cdr x)) (car x)))"##,
        expect,
    );
}

#[test]
fn read_del_escape() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (127 127 t 127)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (read "?\\^?") (read "?\\C-?") (= (read "?\\^?") 127) (read "?\\d"))"##,
        expect,
    );
}

#[test]
fn read_hash_skip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (255 3 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (read "#xFF") (read "#b11") (car (read-from-string "1 #@2 ab3" 0)))"##,
        expect,
    );
}

#[test]
fn read_meta_combos() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (134217729 134217729 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (read "?\\M-\\C-a") (read "?\\C-\\M-a") (= (read "?\\M-a") (+ ?a (ash 1 27))))"##,
        expect,
    );
}

#[test]
fn read_shared_refs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((x (read "(#1=(1 2) #1# #1#)")))
  (list (eq (nth 0 x) (nth 1 x)) (eq (nth 1 x) (nth 2 x)) (car (nth 0 x))))"##,
        expect,
    );
}

#[test]
fn read_string_hex_unicode() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (2 \"AB\" \"A\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (length (read "\"\\x41\\x42\"")) (read "\"\\u0041\\u0042\"") (read "\"\\U00000041\""))"##,
        expect,
    );
}

#[test]
fn coding_dos_eol_file() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (9 (97 13 10 98 13 10 99 13 10))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((f (make-temp-file "neo-dos-")))
  (unwind-protect
      (progn
        (let ((coding-system-for-write 'utf-8-dos))
          (with-temp-buffer (insert "a\nb\nc\n") (write-region (point-min) (point-max) f)))
        (with-temp-buffer
          (let ((coding-system-for-read 'binary)) (insert-file-contents f))
          (list (buffer-size) (append (string-to-unibyte (buffer-string)) nil))))
    (delete-file f)))"##,
        expect,
    );
}

#[test]
fn find_op_coding_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument symbolp (undecided))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (coding-system-base (or (find-operation-coding-system 'write-region 0 0 "test.txt") 'undecided))
        (booleanp (coding-system-p 'prefer-utf-8)))"##,
        expect,
    );
}

#[test]
fn insert_file_detect() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"test ünïcödé\" 12)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((f (make-temp-file "neo-ifd-")))
  (unwind-protect
      (progn
        (let ((coding-system-for-write 'utf-8-unix))
          (with-temp-buffer (insert "test ünïcödé") (write-region (point-min) (point-max) f)))
        (with-temp-buffer (insert-file-contents f)
          (list (buffer-string) (buffer-size))))
    (delete-file f)))"##,
        expect,
    );
}

#[test]
fn set_buffer_coding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (iso-latin-1 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (set-buffer-file-coding-system 'latin-1-unix)
  (insert "abc")
  (list (coding-system-base buffer-file-coding-system)
        (coding-system-eol-type buffer-file-coding-system)))"##,
        expect,
    );
}

#[test]
fn write_read_coding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"héllo\\nwörld\\n\" utf-8-unix)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((f (make-temp-file "neo-wc-")))
  (unwind-protect
      (progn
        (let ((coding-system-for-write 'utf-8-unix))
          (with-temp-buffer (insert "héllo\nwörld\n") (write-region (point-min) (point-max) f)))
        (with-temp-buffer
          (let ((coding-system-for-read 'utf-8-unix)) (insert-file-contents f))
          (list (buffer-string) buffer-file-coding-system)))
    (delete-file f)))"##,
        expect,
    );
}
