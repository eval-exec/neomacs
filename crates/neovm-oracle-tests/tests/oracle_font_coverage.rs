//! Font coverage validates Emacs characters before optional frame dispatch.
#[path = "../src/common.rs"]
mod common;
use common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn font_coverage_accepts_emacs_character_domain_and_preserves_error_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(let ((font (font-spec)))
          (mapcar (lambda (ch)
                    (condition-case err (font-has-char-p font ch 1)
                      (wrong-type-argument (cdr err))))
                  '(#xd800 #x110000 #x3fffff -1 #x400000 4294967361 65.0 nil)))"#,
        expect_test::expect![[
            r#""OK ((framep 1) (framep 1) (framep 1) (characterp -1) (characterp 4194304) (characterp 4294967361) (characterp 65.0) (characterp nil))""#
        ]],
    );
}
