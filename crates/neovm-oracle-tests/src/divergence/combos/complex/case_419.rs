//! Complex combo batch 419 — 20 probes into remaining fundamental areas:
//! eldoc, show-paren, electric modes, display-line-numbers, visual-line-mode,
//! auto-fill, sort-pages/reverse-register, register config-to-register,
//! secure-hash/base64 deeper, zlib, format %S circular structures,
//! prin1 with print-circle/print-length/level, and csv/tsv utilities.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// eldoc-mode: documentation display in echo area.
#[test]
fn div_cx419_eldoc_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'eldoc)
  (with-temp-buffer
    (emacs-lisp-mode)
    (eldoc-mode 1)
    (list eldoc-mode
          (functionp eldoc-documentation-strategy))))
"##,
        expect,
    );
}

/// show-paren-mode: parenthesis highlighting.
#[test]
fn div_cx419_show_paren_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (t [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified])""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'paren)
  (with-temp-buffer
    (insert "(hello)")
    (show-paren-mode 1)
    (list show-paren-mode
          (facep 'show-paren-match))))
"##,
        expect,
    );
}

/// electric-pair-mode: automatic pairing.
#[test]
fn div_cx419_electric_pair_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t \"(\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'elec-pair)
  (with-temp-buffer
    (electric-pair-mode 1)
    (insert "(")
    (list electric-pair-mode
          (buffer-string))))
"##,
        expect,
    );
}

/// display-line-numbers-mode: line number display.
#[test]
fn div_cx419_display_line_numbers_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'display-line-numbers)
  (with-temp-buffer
    (insert "a\nb\nc")
    (display-line-numbers-mode 1)
    (display-line-numbers-mode)))
"##,
        expect,
    );
}

/// visual-line-mode: visual line wrapping.
#[test]
fn div_cx419_visual_line_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function visual-line-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "aaa bbb ccc ddd eee fff ggg")
    (visual-line-mode 1)
    (list visual-line-mode
          (visual-line-p))))
"##,
        expect,
    );
}

/// auto-fill-mode / fill-paragraph deeper.
#[test]
fn div_cx419_auto_fill_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"This is a long sentence that should be broken at the fill column.\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (text-mode)
  (setq fill-column 20)
  (auto-fill-mode 1)
  (insert "This is a long sentence that should be broken at the fill column.")
  (buffer-string))
"##,
        expect,
    );
}

/// reverse-region / sort-pages.
#[test]
fn div_cx419_reverse_region_sort_pages() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"a\\nb\\nc\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "c\nb\na")
  (reverse-region (point-min) (point-max))
  (buffer-string))
"##,
        expect,
    );
}

/// register: window-configuration-to-register / point-to-register.
#[test]
fn div_cx419_register_config_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 3""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (goto-char 3)
  (point-to-register ?a)
  (goto-char 1)
  (jump-to-register ?a)
  (point))
"##,
        expect,
    );
}

/// secure-hash with different algorithms.
#[test]
fn div_cx419_secure_hash_algorithms() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"5d41402abc4b2a76b9719d911017c592\" \"aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d\" \"ea09ae9cc6768c50fcee903ed054556e5bfc8347907f12598aa24193\" \"2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824\" \"59e1748777448c69de6b800d7a33bbfb9ff1b463e44354c3553bcdb9c666fa90125a3c79f90397bdf5f6a13de828684f\" \"9b71d224bd62f3785d96d46ad3ea3d73319bfbc2890caadae2dff72519673ca72323c3d99ba5c11d7c7acc6e14b8c5da0c4663475c2e5c3adef46f73bcdec043\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s "hello"))
  (list (secure-hash 'md5 s)
        (secure-hash 'sha1 s)
        (secure-hash 'sha224 s)
        (secure-hash 'sha256 s)
        (secure-hash 'sha384 s)
        (secure-hash 'sha512 s)))
"##,
        expect,
    );
}

/// base64-encode / base64-decode with multibyte.
#[test]
fn div_cx419_base64_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"Multibyte character in data for base64 encoding\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s "café世界"))
  (let ((enc (base64-encode-string s)))
    (list enc
          (base64-decode-string enc))))
"##,
        expect,
    );
}

/// zlib-available-p / gzip/decompress.
#[test]
fn div_cx419_zlib_gzip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case e (zlib-available-p) (error (car e)))
      (condition-case e
          (with-temp-buffer
            (insert "test data")
            (gzip-region (point-min) (point-max))
            (buffer-size))
        (error (car e))))
"##,
        expect,
    );
}

/// format %S on circular structures (no print-circle).
#[test]
fn div_cx419_format_circular() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"(a b a b . #2)\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lst '(a b)))
  (setcdr (cdr lst) lst)
  (condition-case e (format "%S" lst) (error (car e))))
"##,
        expect,
    );
}

/// prin1 with print-circle: handling circular lists.
#[test]
fn div_cx419_prin1_print_circle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r##""OK \"#1=(a b . #1#)\"""##]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((lst '(a b))
       (print-circle t))
  (setcdr (cdr lst) lst)
  (prin1-to-string lst))
"##,
        expect,
    );
}

/// prin1 with print-length / print-level limits.
#[test]
fn div_cx419_prin1_print_length_level() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"((... ... ...))\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((print-length 3)
      (print-level 2))
  (prin1-to-string '(((1 2 3 4) (5 6) (7 8 9 10)))))
"##,
        expect,
    );
}

/// csv utilities: csv-sort-fields / csv-sort-columns.
#[test]
fn div_cx419_csv_utilities() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"csv-mode\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'csv-mode)
  (with-temp-buffer
    (insert "b,2\na,1\nc,3")
    (csv-sort-fields 1 (point-min) (point-max))
    (buffer-string)))
"##,
        expect,
    );
}

/// tsv / csv: parsing simple delimited data.
#[test]
fn div_cx419_csv_delim_parsing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"a,b,c\" \"1,2,3\" \"4,5,6\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((csv "a,b,c\n1,2,3\n4,5,6"))
  (split-string csv "\n"))
"##,
        expect,
    );
}

/// file-checksum / md5 with different file contents.
#[test]
fn div_cx419_file_checksum_md5() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function file-checksum)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (make-temp-file "neo-cx419-md5-")))
  (with-temp-file f (insert "test content"))
  (unwind-protect
      (list (md5 f)
            (file-checksum f 'md5))
    (delete-file f)))
"##,
        expect,
    );
}

/// gzip / gunzip with buffer content.
#[test]
fn div_cx419_gzip_gunzip_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK void-function""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "data to compress")
      (let ((orig-size (buffer-size)))
        (gzip-region (point-min) (point-max))
        (let ((compressed-size (buffer-size)))
          (gunzip-region (point-min) (point-max))
          (list orig-size compressed-size (buffer-size)))))
  (error (car e)))
"##,
        expect,
    );
}

/// yank / yank-pop / rotate-yank-pointer.
#[test]
fn div_cx419_yank_rotate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"
(with-temp-buffer
  (insert "first ")
  (kill-region 1 (point-max))
  (insert "second ")
  (kill-region 1 (point-max))
  (yank)
  (let ((first-yank (buffer-string)))
    (delete-region 1 (point-max))
    (yank-pop -1)
    (list first-yank (buffer-string))))
"##,
    );
}
