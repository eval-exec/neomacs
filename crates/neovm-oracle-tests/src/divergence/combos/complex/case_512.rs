/// Batch 512: further key description divergence characterization.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx512_key_desc_meta() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"M-x\" \"M-c\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (single-key-description ?\M-x) (single-key-description ?\M-c))
"##,
        expect,
    );
}

#[test]
fn div_cx512_key_desc_ctrl() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"C-x\" \"C-c\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (single-key-description ?\C-x) (single-key-description ?\C-c))
"##,
        expect,
    );
}

#[test]
fn div_cx512_key_desc_shift() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"S-a\" \"S-z\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (single-key-description ?\S-a) (single-key-description ?\S-z))
"##,
        expect,
    );
}

#[test]
fn div_cx512_key_desc_meta_ctrl() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"C-M-x\" \"C-M-c\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (single-key-description ?\M-\C-x) (single-key-description ?\M-\C-c))
"##,
        expect,
    );
}

#[test]
fn div_cx512_key_desc_hyper() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"H-x\" \"H-a\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (single-key-description ?\H-x) (single-key-description ?\H-a))
"##,
        expect,
    );
}

#[test]
fn div_cx512_key_desc_super() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"s-x\" \"s-a\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (single-key-description ?\s-x) (single-key-description ?\s-a))
"##,
        expect,
    );
}

#[test]
fn div_cx512_key_desc_alt() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"A-x\" \"A-a\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (single-key-description ?\A-x) (single-key-description ?\A-a))
"##,
        expect,
    );
}

#[test]
fn div_cx512_key_desc_punctuation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 2 38)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (single-key-description ?\M-!)
      (single-key-description ?\M-?)))
"##,
        expect,
    );
}

#[test]
fn div_cx512_key_desc_function_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"KEY must be an integer, cons, symbol, or string\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (single-key-description [f1])
      (single-key-description [f12])
      (single-key-description [return])
      (single-key-description [tab]))
"##,
        expect,
    );
}

#[test]
fn div_cx512_key_desc_mouse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"KEY must be an integer, cons, symbol, or string\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (single-key-description [mouse-1])
      (single-key-description [down-mouse-2])
      (single-key-description [double-mouse-1]))
"##,
        expect,
    );
}

#[test]
fn div_cx512_key_desc_combos() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"KEY must be an integer, cons, symbol, or string\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (single-key-description [C-M-a])
      (single-key-description [C-M-S-f1])
      (single-key-description [H-s-C-return]))
"##,
        expect,
    );
}

#[test]
fn div_cx512_char_to_string_modifiers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument characterp 134217848)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (char-to-string ?\M-x)
      (char-to-string ?\C-x)
      (char-to-string ?\s-x))
"##,
        expect,
    );
}

#[test]
fn div_cx512_text_char_desc_modifiers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument characterp 134217848)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (text-char-description ?\M-x)
      (text-char-description ?\C-x)
      (text-char-description ?\S-a))
"##,
        expect,
    );
}

#[test]
fn div_cx512_key_binding_event() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (120 (meta) 134217848)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (event-basic-type ?\M-x)
      (event-modifiers ?\M-x)
      (event-convert-list '(meta ?x)))
"##,
        expect,
    );
}

#[test]
fn div_cx512_event_convert() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (6 134217848 134217734)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (event-convert-list '(control ?f))
      (event-convert-list '(meta ?x))
      (event-convert-list '(control meta ?f)))
"##,
        expect,
    );
}
