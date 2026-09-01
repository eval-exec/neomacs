//! Complex combo batch 257 — core Elisp type semantics deep:
//! `vconcat`/`append`/`copy-sequence` type preservation, `mapcar`/
//! `mapc` return values, `identity`, `sxhash` distribution,
//! `type-of` accuracy for all built-in types.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx257_type_of_all_builtin_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((integer 42) (float 3.14) (string \"string\") (symbol symbol) (cons (1 2 3)) (vector [1 2 3]) (integer 97) (symbol 1/3) (cons (expt 2 128)) (cons (cons 1 2)) (symbol nil) (cons (make-hash-table)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (v) (list (type-of v) v))
        '(42 3.14 "string" symbol
          (1 2 3) [1 2 3]
          ?a 1/3 (expt 2 128)
          (cons 1 2) nil
          (make-hash-table)))
"##,
        expect,
    )
}

#[test]
fn div_cx257_vconcat_append_copy_sequence_type_preservation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (vector cons cons vector string nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lst '(1 2 3))
      (vec [1 2 3])
      (str "abc"))
  (list (type-of (vconcat lst))
        (type-of (append vec nil))
        (type-of (copy-sequence lst))
        (type-of (copy-sequence vec))
        (type-of (copy-sequence str))
        (eq (vconcat lst) (vconcat lst))
        (eq lst (copy-sequence lst))))
"##,
        expect,
    )
}

#[test]
fn div_cx257_mapcar_mapc_return_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((2 3 4 5 6) (1 2 3 4 5) (1 2 3 4 5) (1 2 3 4 5) t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lst '(1 2 3 4 5)))
  (list (mapcar #'1+ lst)
        (mapc #'1+ lst)
        lst
        (identity lst)
        (eq (identity lst) lst)))
"##,
        expect,
    )
}

#[test]
fn div_cx257_sxhash_distribution() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-remove-duplicates)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((vals '("alpha" "beta" "gamma" "delta" "epsilon")))
  (list (mapcar #'sxhash-equal vals)
        (cl-remove-duplicates (mapcar #'sxhash-equal vals))
        (= (sxhash-equal "test") (sxhash-equal (copy-sequence "test")))
        (integerp (sxhash-eq 'symbol))))
"##,
        expect,
    )
}

#[test]
fn div_cx257_append_with_strings_and_vectors() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((97 98 99) (1 2 3) (97 98 99 d e f) (97 98 99 . 120) [97 98 99 100] 3 6)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (append "abc" nil)
      (append [1 2 3] nil)
      (append "abc" '(d e f))
      (append "abc" ?x)
      (vconcat "abc" [100])
      (length (append "abc" nil))
      (length (append "abc" '(1 2 3))))
"##,
        expect,
    )
}

#[test]
fn div_cx257_copy_sequence_deep_vs_shallow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (((99 2 3) (99 2 3)) ((99 2 3) (99 2 3)) t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((inner (list 1 2 3))
       (outer (list inner inner))
       (copy (copy-sequence outer)))
  (setcar (car copy) 99)
  (list outer copy
        (eq (car outer) (cadr outer))
        (eq (car copy) (cadr copy))))
"##,
        expect,
    )
}

#[test]
fn div_cx257_char_to_string_byte_to_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"a\" \"à\" \"世\" \"😀\" \"A\" \"�\" 97 99 19990)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (char-to-string ?a)
      (char-to-string ?à)
      (char-to-string ?世)
      (char-to-string ?😀)
      (byte-to-string 65)
      (byte-to-string 255)
      (string-to-char "abc")
      (string-to-char "café")
      (string-to-char "世界"))
"##,
        expect,
    )
}

#[test]
fn div_cx257_number_to_string_string_to_number_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable 1/3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (number-to-string 42)
      (number-to-string -42)
      (number-to-string 3.14)
      (number-to-string 1/3)
      (number-to-string (expt 2 64))
      (string-to-number "42")
      (string-to-number "3.14")
      (string-to-number "1/3")
      (string-to-number "0x1A")
      (string-to-number "not-a-number")
      (string-to-number "42abc")
      (string-to-number ""))
"##,
        expect,
    )
}

#[test]
fn div_cx257_format_type_round_trip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable 1/3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (format "%S" 42)
      (format "%S" "string")
      (format "%S" '(1 2 3))
      (format "%S" [1 2 3])
      (format "%S" 'symbol)
      (format "%S" ?A)
      (format "%S" 1/3)
      (format "%s" "plain")
      (format "%s" 42)
      (format "%d" 42)
      (format "%c" 946))
"##,
        expect,
    )
}

#[test]
fn div_cx257_type_semantics_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((data '((name . "alpha") (value . 42) (tags . (a b c))))
       (data-copy (copy-tree data))
       (data-type (type-of data)))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert (format "Type mega: %S" data))
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 10))
          (ov (make-overlay 4 18)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 25)
      (let ((state (list data-type
                         (eq data data-copy)
                         (equal data data-copy)
                         (type-of (car data))
                         (type-of (cdar data))
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen)
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1))))))
"##,
        expect,
    )
}
