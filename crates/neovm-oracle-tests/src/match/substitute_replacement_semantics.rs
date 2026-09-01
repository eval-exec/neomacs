//! Oracle parity tests for GNU `match-substitute-replacement`.
//!
//! GNU implements this helper in `lisp/subr.el` by translating the current
//! match data to the matched substring and then delegating to `replace-match`.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_match_substitute_basic_backrefs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((s "name=alice age=42"))
  (string-match "\\([a-z]+\\)=\\([a-z0-9]+\\)" s)
  (list
   (match-substitute-replacement "\\2/\\1" nil nil s)
   (match-substitute-replacement "<\\&>" nil nil s)
   (match-substitute-replacement "\\1=\\1;\\2=\\2" nil nil s)))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"alice/name\" \"<name=alice>\" \"name=name;alice=alice\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_match_substitute_literal_backslashes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((s "a-10 b-20"))
  (string-match "\\([a-z]\\)-\\([0-9]+\\)" s)
  (list
   (match-substitute-replacement "\\2:\\1" t nil s)
   (match-substitute-replacement "\\2:\\1" t t s)
   (match-substitute-replacement "\\&" t nil s)
   (match-substitute-replacement "\\&" t t s)
   (match-substitute-replacement "x\\\\y" t nil s)
   (match-substitute-replacement "x\\\\y" t t s)))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"10:a\" \"\\\\2:\\\\1\" \"a-10\" \"\\\\&\" \"x\\\\y\" \"x\\\\\\\\y\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_match_substitute_fixedcase_case_conversion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((cases '("hello" "Hello" "HELLO" "HeLLo")))
  (mapcar
   (lambda (s)
     (string-match "\\([[:alpha:]]+\\)" s)
     (list s
           (match-substitute-replacement "world" nil nil s)
           (match-substitute-replacement "world" t nil s)))
   cases))
"#;

    let expect = expect_test::expect![[
        r#""OK ((\"hello\" \"world\" \"world\") (\"Hello\" \"World\" \"world\") (\"HELLO\" \"WORLD\" \"world\") (\"HeLLo\" \"World\" \"world\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_match_substitute_subexp_replacement() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((s "prefix [alpha:beta:gamma] suffix"))
  (string-match "\\[\\([^:]+\\):\\([^:]+\\):\\([^]]+\\)\\]" s)
  (list
   (match-substitute-replacement "WHOLE" t t s 0)
   (match-substitute-replacement "ONE" t t s 1)
   (match-substitute-replacement "\\3/\\2/\\1" t nil s 2)
   (match-substitute-replacement "" t t s 3)))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"WHOLE\" \"[ONE:beta:gamma]\" \"[alpha:gamma/beta/alpha:gamma]\" \"[alpha:beta:]\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_match_substitute_unmatched_optional_group() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((s "key="))
  (string-match "\\([^=]+\\)=\\([[:alpha:]]+\\)?" s)
  (list
   (match-string 1 s)
   (match-string 2 s)
   (match-substitute-replacement "<\\1>|<\\2>|<\\&>" t nil s)
   (condition-case err
       (match-substitute-replacement "\\9" t nil s)
     (error (list (car err) (cadr err))))))
"#;

    let expect = expect_test::expect![[r#""OK (\"key\" nil \"<key>|<>|<key=>\" \"\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_match_substitute_preserves_outer_match_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((s "foo=10 bar=20"))
  (string-match "\\([a-z]+\\)=\\([0-9]+\\)" s)
  (let ((before (match-data))
        (replacement (match-substitute-replacement "\\2/\\1" t nil s))
        (after (match-data)))
    (list replacement before after (equal before after))))
"#;

    let expect = expect_test::expect![[r#""OK (\"10/foo\" (0 6 0 3 4 6) (0 6 0 3 4 6) t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
