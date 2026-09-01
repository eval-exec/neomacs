//! Strict combo oracle probes, batch 102: regex RE_DUP_MAX boundary and nested
//! quantifiers (a**, a*+).
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_r6_regex_dupmax_boundary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (nil nil nil \"Invalid content of \\\\{\\\\}\" \"Invalid content of \\\\{\\\\}\")""#
    ]];
    // Divergence surfaced 2026-06-27:
    // GNU Emacs: OK (nil nil nil "Invalid content of \\{\\}" "Invalid content of \\{\\}")
    // Neomacs:   OK (nil "Regular expression too big" "Regular expression too big" "Regular expression too big" "Regular expression too big")
    // RE_DUP_MAX boundary differs: Neomacs rejects \\{32768\\} as "Regular
    // expression too big" (RE_DUP_MAX = 32767), while GNU Emacs accepts up to
    // \\{65535\\} (RE_DUP_MAX = 65535). \\{32767\\} agrees (nil = accepted).
    // At \\{65536\\} both reject but with different messages.
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case err (string-match "a\\{32767\\}" "text") (invalid-regexp (cadr err)) (error 'other))
      (condition-case err (string-match "a\\{32768\\}" "text") (invalid-regexp (cadr err)) (error 'other))
      (condition-case err (string-match "a\\{65535\\}" "text") (invalid-regexp (cadr err)) (error 'other))
      (condition-case err (string-match "a\\{65536\\}" "text") (invalid-regexp (cadr err)) (error 'other))
      (condition-case err (string-match "a\\{100000\\}" "text") (invalid-regexp (cadr err)) (error 'other)))
"##,
        expect,
    );
}

#[test]
fn div_r6_regex_nested_quantifiers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 0 nil 0 0 nil)""#]];
    // Divergence surfaced 2026-06-27:
    // GNU Emacs: OK (0 0 nil 0 0 nil)
    // Neomacs:   OK (nil nil nil 0 0 nil)
    // Nested quantifiers a** and a*+ match differently: GNU treats a** as
    // matching at position 0 (zero-width, like a*), Neomacs returns nil (no
    // match, likely treats the second * as literal). a++/a??/\\{2\\}*/\\{2,3\\}+
    // agree.
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case err (string-match "a**" "text") (invalid-regexp (cadr err)) (error 'other))
      (condition-case err (string-match "a*+" "text") (invalid-regexp (cadr err)) (error 'other))
      (condition-case err (string-match "a++" "text") (invalid-regexp (cadr err)) (error 'other))
      (condition-case err (string-match "a??" "text") (invalid-regexp (cadr err)) (error 'other))
      (condition-case err (string-match "a\\{2\\}*" "text") (invalid-regexp (cadr err)) (error 'other))
      (condition-case err (string-match "a\\{2,3\\}+" "text") (invalid-regexp (cadr err)) (error 'other)))
"##,
        expect,
    );
}
