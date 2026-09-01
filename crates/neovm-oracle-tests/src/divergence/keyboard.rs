//! Keyboard pure-function coverage (thin area: ~3 prior files).
//!
//! Deterministic, non-blocking keyboard ops: kbd parsing, key-description,
//! single-key-description, event-modifiers/event-basic-type/event-convert-list,
//! key-valid-p, kmacro construction/keys/counter/format. Avoids read-*
//! (blocks on EOF) and interactive input.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_kb_kbd_parse_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil nil nil 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (equal (kbd "C-c") (kbd "C-c"))
      (stringp (kbd "abc"))
      (vectorp (kbd "C-c"))
      (equal (kbd "RET") (kbd "<return>"))
      (equal (kbd "C-m") [13])
      (length (kbd "C-c C-c")))
"##,
        expect,
    );
}

#[test]
fn div_kb_key_description() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"C-c C-x\" \"a RET\" \"M-x\" \"p r e f i x C-c C-c\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (key-description (kbd "C-c C-x"))
      (key-description [?a 13])
      (key-description (kbd "M-x"))
      (key-description (kbd "C-c C-c") "prefix"))
"##,
        expect,
    );
}

#[test]
fn div_kb_single_key_description() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"a\" \"C-a\" \"M-a\" \"RET\" \"C-M-a\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (single-key-description ?a)
      (single-key-description 1)
      (single-key-description ?\M-a)
      (single-key-description 13)
      (single-key-description ?\C-\M-a))
"##,
        expect,
    );
}

#[test]
fn div_kb_event_modifiers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((control) (meta) (control meta) nil (click))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (event-modifiers ?\C-a)
      (event-modifiers ?\M-a)
      (event-modifiers ?\C-\M-a)
      (event-modifiers ?a)
      (event-modifiers 'mouse-1))
"##,
        expect,
    );
}

#[test]
fn div_kb_event_basic_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (97 97 97 mouse-1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (event-basic-type ?\C-a)
      (event-basic-type ?\M-a)
      (event-basic-type ?\S-a)
      (event-basic-type 'mouse-1))
"##,
        expect,
    );
}

#[test]
fn div_kb_event_convert_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable control)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (event-convert-list (list 'control ?a))
      (event-convert-list (list 'meta control ?a))
      (event-convert-list (list 'shift 'mouse-1)))
"##,
        expect,
    );
}

#[test]
fn div_kb_key_valid_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (key-valid-p "C-c") (key-valid-p "abc") (key-valid-p "<f5>")
      (key-valid-p "C-x C-c") (key-valid-p "C-xyz") (key-valid-p "M-<"))
"##,
        expect,
    );
}

#[test]
fn div_kb_kmacro_construct() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'kmacro)
           (let ((km (kmacro "abc")))
             (list (kmacro-p km) (kmacro-keys km))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_kb_kmacro_counter_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'kmacro)
           (let ((km (kmacro (kbd "C-a") 5 "d")))
             (list (kmacro-counter km) (kmacro-format km))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_kb_kmacro_definition_and_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'kmacro)
           (let ((km (kmacro "xyz")))
             (list (car (kmacro-definition km))
                   (length (kmacro-definition km))
                   (kmacro-single-p km))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_kb_event_symbol_and_mouse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((down) (shift click) mouse-1 mouse-3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (event-modifiers 'down-mouse-1)
      (event-modifiers 'S-mouse-3)
      (event-basic-type 'down-mouse-1)
      (event-basic-type 'S-mouse-3))
"##,
        expect,
    );
}

#[test]
fn div_kb_describe_bindings_structure() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (let ((s (describe-buffer-bindings (current-buffer))))
        (if (stringp s) (length s) s)))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}
