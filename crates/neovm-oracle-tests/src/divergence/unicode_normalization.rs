//! Divergence tests: character folding, unicode normalization, and bidi deep.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_char_fold_to_regexp_ascii() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t 0 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((rx (char-fold-to-regexp "a")))
  (list (stringp rx)
        (> (length rx) 1)
        (string-match rx "a")
        (string-match rx "á")))"#,
        expect,
    );
}

#[test]
fn divergence_char_fold_multi() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t 0 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((rx (char-fold-to-regexp "ss")))
  (list (stringp rx)
        (string-match rx "ss")
        (string-match rx "ß")))"#,
        expect,
    );
}

#[test]
fn divergence_unicode_collation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (string-collate-equalp "hello" "HELLO" nil t)
  (string-collate-equalp "hello" "hello")
  (string-collate-lessp "a" "b"))"#,
        expect,
    );
}

#[test]
fn divergence_get_unicode_property_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"LATIN CAPITAL LETTER A\" \"CJK IDEOGRAPH-4E2D\" nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (get-char-code-property ?A 'name)
  (get-char-code-property ?中 'name)
  (get-char-code-property ?a 'old-name)
  (get-char-code-property ?\n 'name))"#,
        expect,
    );
}

#[test]
fn divergence_unicode_general_categories() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (Lu Ll Nd Zs Cc Po Sc)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (get-char-code-property ?A 'general-category)
  (get-char-code-property ?a 'general-category)
  (get-char-code-property ?0 'general-category)
  (get-char-code-property ?  'general-category)
  (get-char-code-property ?\n 'general-category)
  (get-char-code-property ?! 'general-category)
  (get-char-code-property ?$ 'general-category))"#,
        expect,
    );
}

#[test]
fn divergence_decode_big5() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (coding-system-p 'big5)
  (coding-system-p 'euc-jp)
  (coding-system-p 'shift_jis)
  (coding-system-p 'koi8-r))"#,
        expect,
    );
}

#[test]
fn divergence_coding_system_priority_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((cs (find-coding-systems-string "Hello")))
  (list (consp cs)
        (memq 'utf-8 cs)
        (memq 'raw-text cs)
        (memq 'emacs-mule cs)))"#,
        expect,
    );
}

#[test]
fn divergence_string_width_cjk() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 4 7 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (string-width "ABC")
  (string-width "中文")
  (string-width "ABC中文")
  (= (string-width "ABC中文") (+ (string-width "ABC") (string-width "中文"))))"#,
        expect,
    );
}

#[test]
fn divergence_char_width_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 2 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (aref char-width-table ?A)
  (aref char-width-table ?中)
  (aref char-width-table ?a)
  (aref char-width-table ?\n))"#,
        expect,
    );
}

#[test]
fn divergence_composition_function_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(list
  (char-table-p composition-function-table)
  (aref composition-function-table ?a)
  (aref composition-function-table ?é))"#,
        expect,
    );
}

/// The two hand-derived flag blocks of `lisp/international/emoji-zwj.el`, asked
/// of the image rather than of the file.
///
/// Ledger 206: this port generated that file with a Rust reimplementation of
/// GNU's `admin/unidata/emoji-zwj.awk` (`neovm-core/build.rs`) as well as with
/// the awk itself (`cargo xtask fresh-build`), and the reimplementation doubled
/// the backslash on every `\U0001F1E6`-style character escape.  In Elisp source
/// `"\U0001F1E6"` is ONE character and `"\\U0001F1E6"` is ten, so the regexp
/// GNU builds for regional-indicator flags -- `[X-Y][X-Y]`, ten characters --
/// arrived here as a 46-character bracket of literal backslashes, and neither
/// country flags nor UK subdivision flags composed.
///
/// Measured on the shipped binaries before and after, with the same probe:
///
/// | | flag rules | flag regexp | matches AU | uk rules | uk regexp | matches Scotland |
/// | --- | --- | --- | --- | --- | --- | --- |
/// | GNU Emacs 31.0.90 | 1 | 10 | t | 2 | 23 | t |
/// | neomacs, before | 1 | **46** | **nil** | 2 | **140** | **nil** |
/// | neomacs, after | 1 | 10 | t | 2 | 23 | t |
///
/// The lengths are asserted rather than the regexps because the regexps are
/// made of astral emoji and a diff of them is unreadable; the `string-match`
/// answers are what makes the lengths mean something.
#[test]
fn divergence_emoji_flag_composition_regexps() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 10 t 2 23 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let* ((flag-rules (char-table-range composition-function-table ?\N{U+1F1E6}))
       (flag-re (aref (car (last flag-rules)) 0))
       (uk-rules (char-table-range composition-function-table ?\N{U+1F3F4}))
       (uk-re (aref (car (last uk-rules)) 0)))
  (list (length flag-rules)
        (length flag-re)
        (and (string-match flag-re (string ?\N{U+1F1E6} ?\N{U+1F1FA})) t)
        (length uk-rules)
        (length uk-re)
        (and (string-match uk-re (string ?\N{U+1F3F4} ?\N{U+E0067} ?\N{U+E0062}
                                         ?\N{U+E0073} ?\N{U+E0063} ?\N{U+E0074}
                                         ?\N{U+E007F}))
             t)))"#,
        expect,
    );
}
