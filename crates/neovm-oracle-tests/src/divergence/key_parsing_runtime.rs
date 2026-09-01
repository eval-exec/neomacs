//! Key parsing/description parity: kbd of chords/function/mouse/modifier keys,
//! key-description roundtrip, key-valid-p, key-parse, listify-key-sequence,
//! single-key-description, kbd edge (DEL/ESC/C-?/C-SPC), event-modifiers/
//! basic-type, global key-binding lookup.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn event_modifiers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((control) (control meta) 97 98)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (event-modifiers ?\C-a) (event-modifiers ?\M-\C-a)
        (event-basic-type ?\C-a) (event-basic-type ?\M-b))"##,
        expect,
    );
}

#[test]
fn kbd_edge_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (\"\u{7f}\" \"\u{1b}\" [67108927] [67108896] [backspace])""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (kbd "DEL") (kbd "ESC") (kbd "C-?") (kbd "C-SPC") (kbd "<backspace>"))"##,
        expect,
    );
}

#[test]
fn kbd_function_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ([mouse-1] [down] [C-up] [M-f7] [S-tab])""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (kbd "<mouse-1>") (kbd "<down>") (kbd "<C-up>")
        (kbd "M-<f7>") (kbd "S-<tab>"))"##,
        expect,
    );
}

#[test]
fn kbd_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"\u{18}\u{3}\" [134217848] [f5] [134217729] \"\\r\" \"\t\" \" \" \"\u{3}\u{18}\u{16}\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (kbd "C-x C-c") (kbd "M-x") (kbd "<f5>") (kbd "C-M-a")
        (kbd "RET") (kbd "TAB") (kbd "SPC") (kbd "C-c C-x C-v"))"##,
        expect,
    );
}

#[test]
fn key_binding_global() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (eq (key-binding (kbd "C-f")) 'forward-char)
        (eq (key-binding (kbd "C-x C-f")) 'find-file)
        (commandp (key-binding (kbd "C-a"))))"##,
        expect,
    );
}

#[test]
fn key_description_prefix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"C-c C-x\" \"C-a\" \"M-x\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (key-description [?\C-x] [?\C-c])
        (single-key-description ?\C-a) (single-key-description ?\M-x))"##,
        expect,
    );
}

#[test]
fn key_description_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"C-x C-c\" \"M-RET\" \"<f1>\" \"C-a M-b\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (key-description (kbd "C-x C-c")) (key-description (kbd "M-RET"))
        (key-description (kbd "<f1>")) (key-description [?\C-a ?\M-b]))"##,
        expect,
    );
}

#[test]
fn key_parse() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ([24 3] [134217848] [13])""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (list (key-parse "C-x C-c") (key-parse "M-x") (key-parse "RET")) (error (cons (quote ERR) (car e))))"##,
        expect,
    );
}

#[test]
fn key_valid_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (list (key-valid-p "C-x C-c") (key-valid-p "C-xC-c")
        (key-valid-p "<f5>") (key-valid-p "RET")) (error (cons (quote ERR) (car e))))"##,
        expect,
    );
}

#[test]
fn listify_key_sequence() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((1) (97 98 99) (134217825))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (listify-key-sequence (kbd "C-a")) (listify-key-sequence (kbd "abc"))
        (listify-key-sequence [?\M-a]))"##,
        expect,
    );
}
