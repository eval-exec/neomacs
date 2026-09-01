//! Unicode character-property parity (get-char-code-property over the unidata
//! tables): general-category, name, numeric-value, bidi-class, decomposition,
//! uppercase/lowercase, mirroring, canonical-combining-class; char-width across
//! scripts, char-script-table, char-category-set/mnemonics.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn bidi_class() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (L R EN)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (get-char-code-property ?A 'bidi-class)
        (get-char-code-property ?א 'bidi-class)
        (get-char-code-property ?5 'bidi-class))"##,
        expect,
    );
}

#[test]
fn category_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#&128\"\\0\\0\\0\\0\\0@\\0\\0\\0\u{10}\\0\\0\u{2}\u{10}\u{4}\\0\" \".Lalr\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (char-category-set ?A) (category-set-mnemonics (char-category-set ?A)))"##,
        expect,
    );
}

#[test]
fn char_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"LATIN CAPITAL LETTER A\" \"GREEK SMALL LETTER LAMDA\" \"EURO SIGN\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (get-char-code-property ?A 'name)
        (get-char-code-property ?λ 'name)
        (get-char-code-property ?€ 'name))"##,
        expect,
    );
}

#[test]
fn char_script() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (latin han greek)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (aref char-script-table ?A) (aref char-script-table ?日)
        (aref char-script-table ?α))"##,
        expect,
    );
}

#[test]
fn char_width_scripts() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 2 1 0 1 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (char-width ?A) (char-width ?日) (char-width ?ｱ)
        (char-width ?́) (string-width "á") (string-width "日本"))"##,
        expect,
    );
}

#[test]
fn decomposition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((101 769) (65) (compat 102 105))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (get-char-code-property ?é 'decomposition)
        (get-char-code-property ?A 'decomposition)
        (get-char-code-property ?ﬁ 'decomposition))"##,
        expect,
    );
}

#[test]
fn general_category() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (Lu Ll Nd Zs Po)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (get-char-code-property ?A 'general-category)
        (get-char-code-property ?a 'general-category)
        (get-char-code-property ?5 'general-category)
        (get-char-code-property ?\s 'general-category)
        (get-char-code-property ?. 'general-category))"##,
        expect,
    );
}

#[test]
fn mirroring_canonical() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (41 0 230)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (get-char-code-property ?\( 'mirroring)
        (get-char-code-property ?A 'canonical-combining-class)
        (get-char-code-property ?́ 'canonical-combining-class))"##,
        expect,
    );
}

#[test]
fn numeric_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (5 12 0.5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (get-char-code-property ?5 'numeric-value)
        (get-char-code-property ?Ⅻ 'numeric-value)
        (get-char-code-property ?½ 'numeric-value))"##,
        expect,
    );
}

#[test]
fn uppercase_lowercase_prop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (65 97 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (get-char-code-property ?a 'uppercase)
        (get-char-code-property ?A 'lowercase)
        (get-char-code-property ?5 'uppercase))"##,
        expect,
    );
}
