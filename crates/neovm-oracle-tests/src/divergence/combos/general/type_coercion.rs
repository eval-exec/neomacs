//! Divergence tests: complex type coercion + conversion chains.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_number_string_conversion_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (42 \"42\" 42 \"2a\" \"52\" \"101010\" 42 t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let* ((n 42)
        (s (number-to-string n))
        (n2 (string-to-number s))
        (hex (format \"%x\" n))
        (oct (format \"%o\" n))
        (bin (format \"%b\" n))
        (n3 (string-to-number hex 16)))
  (list n s n2 hex oct bin n3
        (= n n2)
        (= n n3)
        (string= hex \"2a\")
        (string= oct \"52\"))) ",
        expect,
    );
}

#[test]
fn divergence_char_string_number_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let* ((char ?A)
        (s (char-to-string char))
        (n (aref s 0))
        (back (char-to-string n)))
  (list (= char 65)
        (string= s \"A\")
        (= n char)
        (string= s back)
        (= (length s) 1)
        (= (aref s 0) 65))) ",
        expect,
    );
}

#[test]
fn divergence_sequence_type_conversions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument characterp \"A\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((lst '(1 2 3 4 5))
        (vec [1 2 3 4 5])
        (str \"abcde\"))
  (list (equal lst (append vec nil))
        (equal (append vec nil) lst)
        (vconcat lst)
        (equal (vconcat lst) vec)
        (string-to-list str)
        (equal (string-to-list str) '(97 98 99 100 101))
        (concat (mapcar #'char-to-string '(65 66 67)))
        (string= (concat (mapcar #'char-to-string '(65 66 67))) \"ABC\"))) ",
        expect,
    );
}

#[test]
fn divergence_float_integer_boundary() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 4 4 3 -4 -3 -4 -3 t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (floor 3.7)
  (ceiling 3.7)
  (round 3.7)
  (truncate 3.7)
  (floor -3.7)
  (ceiling -3.7)
  (round -3.7)
  (truncate -3.7)
  (= (float 5) 5.0)
  (= (float 5) 5)
  (equal (float 5) 5.0)
  (not (equal (float 5) 5))) ",
        expect,
    );
}

#[test]
fn divergence_symbol_string_conversion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t hello-world t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let* ((sym 'hello-world)
        (name (symbol-name sym))
        (back (intern name)))
  (list (string= name \"hello-world\")
        (eq sym back)
        (equal sym back)
        (intern-soft name obarray)
        (eq (intern-soft name obarray) sym))) ",
        expect,
    );
}

#[test]
fn divergence_format_specs_all_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"symbol\" \"(a b c)\" \"42\" \"ff\" \"10\" \"3.140000\" \"1.000000e+03\" \"A\" \"%\" \"         7\" \"0000000007\" \"+7\" \"-7\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (format \"%s\" 'symbol)
  (format \"%S\" '(a b c))
  (format \"%d\" 42)
  (format \"%x\" 255)
  (format \"%o\" 8)
  (format \"%f\" 3.14)
  (format \"%e\" 1000.0)
  (format \"%c\" 65)
  (format \"%%\")
  (format \"%10d\" 7)
  (format \"%010d\" 7)
  (format \"%+d\" 7)
  (format \"%+d\" -7)) ",
        expect,
    );
}

#[test]
fn divergence_bool_coercion_in_conditionals() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (yes yes no yes yes no 3 nil 3 nil nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (if 0 'yes 'no)
  (if \"\" 'yes 'no)
  (if nil 'yes 'no)
  (if t 'yes 'no)
  (if 'symbol 'yes 'no)
  (if () 'yes 'no)
  (and 1 2 3)
  (and 1 nil 3)
  (or nil nil 3)
  (or nil nil nil)
  (not 0)
  (not \"\")
  (not nil)) ",
        expect,
    );
}

#[test]
fn divergence_list_vector_hash_conversion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""ERR (wrong-type-argument listp #s(hash-table test equal))""#]];
    crate::common::assert_oracle_parity_expect(
        "(let* ((alist '((a . 1) (b . 2) (c . 3)))
        (ht (alist-get 'hash-table (make-hash-table :test 'equal))))
  (dolist (p alist) (puthash (car p) (cdr p) ht))
  (let ((back nil))
    (maphash (lambda (k v) (push (cons k v) back)) ht)
    (list (length back)
          (= (length back) 3)
          (gethash 'b ht)
          (= (gethash 'b ht) 2)))) ",
        expect,
    );
}

#[test]
fn divergence_string_bytes_chars_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((ascii \"hello\")
        (utf8 \"\\u4e16\\u754c\")
        (mixed \"hi\\u4e16\"))
  (list (= (length ascii) 5)
        (= (string-bytes ascii) 5)
        (= (length utf8) 2)
        (>= (string-bytes utf8) 4)
        (= (length mixed) 3)
        (>= (string-bytes mixed) 5)
        (multibyte-string-p utf8)
        (multibyte-string-p mixed)
        (string= (substring utf8 0 1) \"\\u4e16\"))) ",
        expect,
    );
}

#[test]
fn divergence_propertized_string_conversion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (5 t bold t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let* ((s (propertize \"hello\" 'face 'bold 'data 42))
        (len (length s))
        (has-face (get-text-property 1 'face s))
        (has-data (get-text-property 1 'data s))
        (plain (substring-no-properties s 0 5)))
  (list len
        (= len 5)
        has-face
        (= has-data 42)
        (string= plain \"hello\")
        (not (get-text-property 1 'face plain)))) ",
        expect,
    );
}
