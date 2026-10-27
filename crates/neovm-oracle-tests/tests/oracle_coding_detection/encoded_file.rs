//! Raw compiled-file loading must preserve Emacs character identity.
use crate::common;

#[test]
fn encoded_file_character_boundaries_match_live_gnu() {
    common::return_if_neovm_enable_oracle_proptest_not_set!();
    // A .el source file may go through coding conversion instead of FileBytes.
    // The compiled fixture has a GNU-recognized header and contains source
    // forms, so both editors exercise the public raw-file load boundary.
    common::assert_oracle_parity_expect(
        r#"(let ((file (expand-file-name
                        "crates/neovm-oracle-tests/fixtures/encoded-file/valid.elc"
                        (getenv "NEOVM_ORACLE_PROJECT_ROOT"))))
             (load file nil t t)
             (list neo-encoded-file-result neo-encoded-file-malformed
                   neo-encoded-file-after))"#,
        expect_test::expect![[
            r#""OK ((233 30333 128512 2097152 4194175 4194176 4194303) (195 40) loaded)""#
        ]],
    );
}

#[test]
fn supplied_name_delete_stops_at_declared_count_matches_live_gnu() {
    common::return_if_neovm_enable_oracle_proptest_not_set!();
    // This payload is fully initialized. GNU's logical SCHARS limit leaves
    // the physical z outside the declared name untouched by deletion.
    common::assert_oracle_parity_expect(
        r#"(let* ((read-symbol-shorthands
                    (list (cons (unibyte-string #xc3 #xa9) (string #x3bb))))
                   (symbol (intern (concat (string #xe9) "z") (obarray-make)))
                   (name (symbol-name symbol))
                   (result (delete ?z name)))
              (list (length name) (eq result name) (length result)
                    (append (string-as-unibyte result) nil)
                    (append (string-as-unibyte name) nil)))"#,
        expect_test::expect![[r#""OK (1 t 1 (206 187 122) (206 187 122))""#]],
    );
}
