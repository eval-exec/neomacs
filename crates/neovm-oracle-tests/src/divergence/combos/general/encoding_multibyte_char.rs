//! Divergence tests: encoding + coding-system + multibyte + char combo.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_coding_system_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 12 54)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list (coding-system-p 'utf-8)
        (eq (coding-system-p 'utf-8) t)
        (coding-system-p 'latin-1)
        (eq (coding-system-p 'latin-1) t)
        (coding-system-p 'binary)
        (eq (coding-system-p 'binary) t)
        (not (coding-system-p 'nonexistent-cs-xxx))
        (coding-system-base 'utf-8)
        (eq (coding-system-base 'utf-8) 'utf-8)
        (coding-system-base 'utf-8-dos)
        (eq (coding-system-base 'utf-8-dos) 'utf-8)))) "#,
        expect,
    );
}

#[test]
fn divergence_multibyte_char_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function char-lowercase-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list (char-to-string ?A)
        (string= (char-to-string ?A) "A")
        (string-to-char "ABC")
        (= (string-to-char "ABC") ?A)
        (char-width ?A)
        (= (char-width ?A) 1)
        (char-width ?\x3B1)
        (>= (char-width ?\x3B1) 1)
        (char-width ?\x4E00)
        (>= (char-width ?\x4E00) 2)
        (char-category-set ?A)
        (char-category-set ?0)
        (char-uppercase-p ?A)
        (char-uppercase-p ?a)
        (not (char-uppercase-p ?a))
        (char-lowercase-p ?a)
        (char-lowercase-p ?A)
        (not (char-lowercase-p ?A))
        (upcase ?a)
        (= (upcase ?a) ?A)
        (downcase ?A)
        (= (downcase ?A) ?a)))) "#,
        expect,
    );
}

#[test]
fn divergence_string_multibyte_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (nil t 5 t 5 t \"hello\" t \"hello\" t \"hello\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((str "hello"))
    (list (multibyte-string-p str)
          (not (multibyte-string-p str))
          (string-bytes str)
          (= (string-bytes str) 5)
          (length str)
          (= (length str) 5)
          (string-as-unibyte str)
          (= (length (string-as-unibyte str)) 5)
          (string-to-multibyte str)
          (= (length (string-to-multibyte str)) 5)
          (string-as-unibyte (string-to-multibyte str))
          (= (length (string-as-unibyte (string-to-multibyte str))) 5)))) "#,
        expect,
    );
}

#[test]
fn divergence_string_with_unicode() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 t t 945 t 946 t 947 t \"αβ\" t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((str "\x03B1\x03B2\x03B3"))
    (list (length str)
          (= (length str) 3)
          (multibyte-string-p str)
          (aref str 0)
          (= (aref str 0) ?\x3B1)
          (aref str 1)
          (= (aref str 1) ?\x3B2)
          (aref str 2)
          (= (aref str 2) ?\x3B3)
          (substring str 0 2)
          (= (length (substring str 0 2)) 2)
          (string= (substring str 1 2) "\x03B2")))) "#,
        expect,
    );
}

#[test]
fn divergence_coding_system_alias_and_priority() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t utf-8 t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (define-coding-system-alias 'test-csa-xxx 'utf-8)
  (let ((aliases (coding-system-aliases 'utf-8)))
    (list (and (memq 'test-csa-xxx aliases) t)
          (coding-system-p 'test-csa-xxx)
          (eq (coding-system-p 'test-csa-xxx) t)
          (coding-system-base 'test-csa-xxx)
          (eq (coding-system-base 'test-csa-xxx) 'utf-8)
          (consp (coding-system-priority-list))
          (>= (length (coding-system-priority-list)) 1)
          (and (memq 'utf-8 (coding-system-priority-list)) t)))) "#,
        expect,
    );
}

#[test]
fn divergence_char_tables_and_categories() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function char-table-type)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((ct (make-char-table 'category-table t)))
    (set-char-table-range ct ?A 'letter)
    (set-char-table-range ct ?0 'digit)
    (list (char-table-p ct)
          (eq (char-table-type ct) 'category-table)
          (char-table-range ct ?A)
          (eq (char-table-range ct ?A) 'letter)
          (char-table-range ct ?0)
          (eq (char-table-range ct ?0) 'digit)
          (char-table-range ct ?!)
          (eq (char-table-range ct ?!) t)
          (char-table-extra-slot ct 0)
          (null (char-table-extra-slot ct 0))
          (set-char-table-extra-slot ct 0 'test-val)
          (char-table-extra-slot ct 0)
          (eq (char-table-extra-slot ct 0) 'test-val)))) "#,
        expect,
    );
}

#[test]
fn divergence_string_bytes_vs_length() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((ascii "abcde")
        (mixed "abc\x03B1\x03B2")
        (cjk "\x4E00\x4E01\x4E02"))
    (list (= (length ascii) 5)
          (= (string-bytes ascii) 5)
          (= (length mixed) 5)
          (>= (string-bytes mixed) (length mixed))
          (= (length cjk) 3)
          (>= (string-bytes cjk) (length cjk))
          (= (length (substring ascii 1 3)) 2)
          (= (length (substring mixed 1 4)) 3)
          (string= (substring ascii 0 2) "ab")
          (= (aref mixed 3) ?\x3B1)
          (= (aref cjk 0) ?\x4E00)
          (= (aref cjk 1) ?\x4E01)))) "#,
        expect,
    );
}

#[test]
fn divergence_unicode_property_lookup() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function char-script-table)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list (get-char-code-property ?A 'general-category)
        (equal (get-char-code-property ?A 'general-category) "Lu")
        (get-char-code-property ?a 'general-category)
        (equal (get-char-code-property ?a 'general-category) "Ll")
        (get-char-code-property ?0 'general-category)
        (equal (get-char-code-property ?0 'general-category) "Nd")
        (get-char-code-property ?  'general-category)
        (equal (get-char-code-property ?  'general-category) "Zs")
        (get-char-code-property ?$ 'general-category)
        (equal (get-char-code-property ?$ 'general-category) "Sc")
        (char-table-p (char-script-table)))) "#,
        expect,
    );
}

#[test]
fn divergence_case_conversion_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"HELLO WORLD 123\" t \"hello world 123\" t \"Hello World 123\" t \"Hello World 123\" t \"ABCΑΒ\" t \"abcαβ\" t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((str "Hello World 123"))
    (list (upcase str)
          (string= (upcase str) "HELLO WORLD 123")
          (downcase str)
          (string= (downcase str) "hello world 123")
          (capitalize str)
          (string= (capitalize str) "Hello World 123")
          (upcase-initials str)
          (string= (upcase-initials str) "Hello World 123")
          (upcase "abc\x03B1\x03B2")
          (= (length (upcase "abc\x03B1\x03B2")) 5)
          (downcase "ABC\x0391\x0392")
          (= (length (downcase "ABC\x0391\x0392")) 5)))) "#,
        expect,
    );
}

#[test]
fn divergence_string_conversion_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"42\" t \"3.14\" t 42 t 3.14 t 255 t 10 t 63 t \"ff\" t \"10\" t \"1010\" t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list (number-to-string 42)
        (string= (number-to-string 42) "42")
        (number-to-string 3.14)
        (string= (number-to-string 3.14) "3.14")
        (string-to-number "42")
        (= (string-to-number "42") 42)
        (string-to-number "3.14")
        (= (string-to-number "3.14") 3.14)
        (string-to-number "ff" 16)
        (= (string-to-number "ff" 16) 255)
        (string-to-number "1010" 2)
        (= (string-to-number "1010" 2) 10)
        (string-to-number "77" 8)
        (= (string-to-number "77" 8) 63)
        (format "%x" 255)
        (string= (format "%x" 255) "ff")
        (format "%o" 8)
        (string= (format "%o" 8) "10")
        (format "%b" 10)
        (string= (format "%b" 10) "1010"))) "#,
        expect,
    );
}
