//! UTF-8 / multibyte *charset & coding-system infrastructure* divergence probes.
//!
//! Probes construction APIs (`define-charset`, `make-coding-system`) and
//! metadata accessors (`charset-plist`, `charset-code-space`,
//! `coding-system-aliases`, `coding-system-type`, `charset-chars`, the `block`
//! property). A UTF-8-internal reimpl often lacks the full charset/coding
//! registry machinery, so these are likely divergence points.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_utf8_define_charset_custom() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored wrong-type-argument)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(condition-case err
    (progn
      (define-charset 'neo-test-charset-1
        "Test charset"
        :dimension 1
        :code-space [0 127]
        :superset 'ascii)
      (list (charset-p 'neo-test-charset-1)
            (charset-dimension 'neo-test-charset-1)))
  (error (list 'errored (car err))))
"#,
        expect,
    );
}

#[test]
fn div_utf8_make_coding_system_custom() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(condition-case err
    (progn
      (make-coding-system 'neo-cs-1 0 ?T "Test coding system")
      (coding-system-p 'neo-cs-1))
  (error (list 'errored (car err))))
"#,
        expect,
    );
}

#[test]
fn div_utf8_charset_plist_builtins() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:name ascii :dimension 1 :code-space [0 127 0 0 0 0 0 0] :iso-final-char 66 :emacs-mule-id 0 :ascii-compatible-p t :code-offset 0 :docstring \"ASCII (ISO646 IRV)\" :short-name \"ASCII\" :long-name \"ASCII (ISO646 IRV)\") (:name unicode :dimension 3 :code-space [0 255 0 255 0 16 0 0] :iso-final-char nil :emacs-mule-id nil :ascii-compatible-p t :code-offset 0 :docstring \"Unicode (ISO10646)\" :short-name \"Unicode\" :long-name \"Unicode (ISO10646)\") (:name eight-bit :dimension 1 :code-space [128 255 0 0 0 0 0 0] :iso-final-char nil :emacs-mule-id nil :ascii-compatible-p nil :code-offset 4194176 :docstring \"Raw bytes 128-255\" :short-name \"Raw bytes\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (charset-plist 'ascii)
      (charset-plist 'unicode)
      (charset-plist 'eight-bit))
"#,
        expect,
    );
}

#[test]
fn div_utf8_charset_code_space_builtins() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function charset-code-space)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (charset-code-space 'ascii)
      (charset-code-space 'unicode)
      (charset-code-space 'japanese-jisx0208))
"#,
        expect,
    );
}

#[test]
fn div_utf8_coding_system_aliases() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((utf-8 mule-utf-8 cp65001) (iso-latin-1 iso-8859-1 latin-1) (iso-latin-1 iso-8859-1 latin-1) (emacs-mule))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (coding-system-aliases 'utf-8)
      (coding-system-aliases 'latin-1)
      (coding-system-aliases 'iso-8859-1)
      (coding-system-aliases 'emacs-mule))
"#,
        expect,
    );
}

#[test]
fn div_utf8_charset_chars_counts() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (128 256 128)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (charset-chars 'ascii)
      (charset-chars 'unicode)
      (charset-chars 'eight-bit))
"#,
        expect,
    );
}

#[test]
fn div_utf8_block_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (get-char-code-property ?a 'block)
      (get-char-code-property ?\x3042 'block)
      (get-char-code-property ?\x1f600 'block)
      (get-char-code-property ?\x5d0 'block)
      (get-char-code-property ?é 'block))
"#,
        expect,
    );
}

#[test]
fn div_utf8_coding_system_type_and_mnemonic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (utf-8 utf-16 charset 85 49)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (coding-system-type 'utf-8)
      (coding-system-type 'utf-16)
      (coding-system-type 'latin-1)
      (coding-system-mnemonic 'utf-8)
      (coding-system-mnemonic 'latin-1))
"#,
        expect,
    );
}
