//! Oracle parity tests for GNU `subr.el` `shell-quote-argument`.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_prop_gnu_subr_shell_quote_argument_posix_contracts() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU subr.el:shell-quote-argument POSIX quoting leaves only POSIX
    // filename characters unescaped, maps empty string to "''", and replaces
    // embedded newlines with the shell-safe quoted newline sequence.
    let form = r#"(mapcar
 (lambda (s)
   (list s
         (shell-quote-argument s t)
         (shell-quote-argument s)))
 (list ""
       "plain"
       "has space"
       "quote'single"
       "dollar$semi;pipe|"
       "line\nbreak"
       "[glob]*?"
       "back\\slash"
       "two\\\\slashes"
       "tab\tchar"
       "ümlaut"))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"\" \"''\" \"''\") (\"plain\" \"plain\" \"plain\") (\"has space\" \"has\\\\ space\" \"has\\\\ space\") (\"quote'single\" \"quote\\\\'single\" \"quote\\\\'single\") (\"dollar$semi;pipe|\" \"dollar\\\\$semi\\\\;pipe\\\\|\" \"dollar\\\\$semi\\\\;pipe\\\\|\") (\"line\\nbreak\" \"line'\\n'break\" \"line'\\n'break\") (\"[glob]*?\" \"\\\\[glob\\\\]\\\\*\\\\?\" \"\\\\[glob\\\\]\\\\*\\\\?\") (\"back\\\\slash\" \"back\\\\\\\\slash\" \"back\\\\\\\\slash\") (\"two\\\\\\\\slashes\" \"two\\\\\\\\\\\\\\\\slashes\" \"two\\\\\\\\\\\\\\\\slashes\") (\"tab\tchar\" \"tab\\\\\tchar\" \"tab\\\\\tchar\") (\"ümlaut\" \"\\\\ümlaut\" \"\\\\ümlaut\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
