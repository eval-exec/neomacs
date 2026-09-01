//! Complex combo divergence probes batch 3.
//!
//! string-make-multibyte of UTF-8 bytes, secure-hash/md5 over multibyte,
//! base64 round-trip, compare-buffer-substrings, cl-defstruct print/read,
//! filter-buffer-substring + invisible, nested non-local exit, positional
//! format, char-fold search + replace, text-property-search-forward, closure
//! print/read/funcall, display property + column.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx3_string_make_multibyte_utf8_bytes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 (4194243 4194217 4194274 4194178 4194220))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((m (string-make-multibyte (unibyte-string 195 169 226 130 172))))
  (list (length m) (append m nil)))
"##,
        expect,
    );
}

#[test]
fn div_cx3_secure_hash_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"dfda11df060666bcc681d3c5086b4000b9153dda2f5d7918811e9bb571b88135\" \"d742e25bd2f38b17eca47d22363776a1\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (secure-hash 'sha256 "café世界")
      (secure-hash 'md5 "café世界"))
"##,
        expect,
    );
}

#[test]
fn div_cx3_base64_roundtrip_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"Multibyte character in data for base64 encoding\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((s "café世界😀")
       (enc (base64-encode-string s))
       (dec (base64-decode-string enc)))
  (list enc (equal s dec)))
"##,
        expect,
    );
}

#[test]
fn div_cx3_compare_buffer_substrings_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((b1 (generate-new-buffer " *cbs1*"))
      (b2 (generate-new-buffer " *cbs2*")))
  (with-current-buffer b1 (insert "Café"))
  (with-current-buffer b2 (insert "café"))
  (prog1 (list (compare-buffer-substrings b1 nil nil b2 nil nil)
               (let ((case-fold-search t))
                 (compare-buffer-substrings b1 nil nil b2 nil nil)))
    (kill-buffer b1) (kill-buffer b2)))
"##,
        expect,
    );
}

#[test]
fn div_cx3_cl_defstruct_print_read() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (cl-defstruct neo-cx3-pt x y)
  (let* ((p (make-neo-cx3-pt :x 3 :y 4))
         (printed (prin1-to-string p))
         (back (car (read-from-string printed))))
    (list (neo-cx3-pt-x back) (neo-cx3-pt-y back) (neo-cx3-pt-p back))))
"##,
        expect,
    );
}

#[test]
fn div_cx3_filter_buffer_substring_invisible() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK #(\"visible INVISIBLE visibl\" 7 16 (invisible t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "visible INVISIBLE visible")
      (put-text-property 8 17 'invisible t)
      (filter-buffer-substring 1 25))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx3_nested_condition_unwind_throw() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:outer-clean :caught :inner-clean) :inner2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (log)
  (catch 'outer
    (unwind-protect
        (condition-case e
            (unwind-protect
                (throw 'inner :inner-val)
              (push :inner-clean log))
          (error (push :caught log)))
      (push :outer-clean log)))
  (list log (catch 'inner
              (unwind-protect
                  (throw 'inner :inner2)
                (push :inner2-clean log))
              :not-reached)))
"##,
        expect,
    );
}

#[test]
fn div_cx3_positional_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"hello world\" \"3 + 3 = 6\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (format "%2$s %1$s" "world" "hello")
      (format "%1$d + %1$d = %2$d" 3 6))
"##,
        expect,
    );
}

#[test]
fn div_cx3_char_fold_search_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument sequencep 233)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((search-default-mode #'char-fold-to-regexp))
  (replace-regexp-in-string (char-fold-to-regexp ?é) "E" "café"))
"##,
        expect,
    );
}

#[test]
fn div_cx3_text_property_search_forward() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 5 9)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "aaaaBBBBcccc")
  (put-text-property 5 9 'face 'bold)
  (goto-char 1)
  (let ((match (text-property-search-forward 'face 'bold t)))
    (list (if match t nil)
          (prop-match-beginning match)
          (prop-match-end match))))
"##,
        expect,
    );
}

#[test]
fn div_cx3_closure_print_read_funcall() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t 42)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((lexical-binding t)
       (f (byte-compile (let ((x 42)) (lambda () x))))
       (printed (prin1-to-string f)))
  (list (functionp f)
        (stringp printed)
        (condition-case e (funcall (car (read-from-string printed))) (error (car e)))))
"##,
        expect,
    );
}

#[test]
fn div_cx3_display_property_column_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (end-of-buffer)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (put-text-property 2 4 'display (make-string 8 88))
  (list (current-column)
        (save-excursion (forward-char 3) (current-column))
        (string-width (buffer-substring 1 5))))
"##,
        expect,
    );
}

#[test]
fn div_cx3_char_fold_table_modify_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function char-fold-table)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((char-fold-table (char-fold-table)))
  (list (string-match (char-fold-to-regexp ?e) "café")
        (string-match (char-fold-to-regexp ?a) "abc")))
"##,
        expect,
    );
}

#[test]
fn div_cx3_overlays_in_narrowed_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range #<killed buffer> 5 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789ABCDEF")
  (let ((o1 (make-overlay 3 6)) (o2 (make-overlay 8 12)))
    (overlay-put o1 'face 'bold)
    (overlay-put o2 'face 'italic)
    (narrow-to-region 5 14)
    (delete-region (point-min) 3)
    (list (overlay-start o1) (overlay-end o1)
          (overlay-start o2) (overlay-end o2)
          (buffer-string))))
"##,
        expect,
    );
}

#[test]
fn div_cx3_buffer_hash_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"fd5a5c98b79d0a4eeebce6933dd52d4f6400611f\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "café世界")
  (buffer-hash))
"##,
        expect,
    );
}

#[test]
fn div_cx3_put_text_property_on_string_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((face bold) (face italic))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((s (copy-sequence "café"))
       (_ (put-text-property 0 2 'face 'bold s))
       (_ (put-text-property 2 4 'face 'italic s))
       (p (prin1-to-string s))
       (back (car (read-from-string p))))
  (list (text-properties-at 0 back)
        (text-properties-at 2 back)))
"##,
        expect,
    );
}

#[test]
fn div_cx3_string_match_multibyte_capture_groups() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range #<buffer *scratch*> 2 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (string-match "\\(café\\).*\\(世界\\)" "x café y 世界 z")
  (list (match-string 1) (match-string 2)
        (match-beginning 1) (match-end 1)
        (match-beginning 2) (match-end 2)))
"##,
        expect,
    );
}

#[test]
fn div_cx3_recursive_macro_expansion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 7""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defmacro neo-cx3-inc (n) (list '1+ n))
  (defmacro neo-cx3-double-inc (n) (list 'neo-cx3-inc (list 'neo-cx3-inc n)))
  (neo-cx3-double-inc 5))
"##,
        expect,
    );
}

#[test]
fn div_cx3_window_point_set_buffer_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-wp-*")))
  (with-current-buffer buf (insert "abcdef"))
  (set-window-buffer (selected-window) buf)
  (set-window-point (selected-window) 4)
  (prog1 (list (window-point) (eq (window-buffer (selected-window)) buf))
    (set-window-buffer (selected-window) (get-buffer "*scratch*"))
    (kill-buffer buf)))
"##,
        expect,
    );
}

#[test]
fn div_cx3_abbrev_expansion_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"dynamic \"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((tbl (make-abbrev-table)))
  (define-abbrev tbl "dyn" "" (lambda () (insert "dynamic")))
  (with-temp-buffer
    (set (make-local-variable 'local-abbrev-table) tbl)
    (abbrev-mode 1)
    (insert "dyn ")
    (expand-abbrev)
    (buffer-string)))
"##,
        expect,
    );
}
