use expect_test::expect;

use super::ParityBatchCase;

fn agda_editor_tactics_indent_counts_leading_spaces_across_real_declarations() -> ParityBatchCase {
    ParityBatchCase::value(
        "agda_editor_tactics_indent_counts_leading_spaces_across_real_declarations",
        r##"(mapcar
         (lambda (line)
           (cons line (agda-editor-tactics-indent line)))
         '("record R : Set where"
           " field"
           "  field"
           "    value : Set"
           "        proof = refl"))"##,
        expect![[
            r#"OK (("record R : Set where" . 1) (" field" . 1) ("  field" . 2) ("    value : Set" . 4) ("        proof = refl" . 8))"#
        ]],
    )
}

fn agda_editor_tactics_indent_distinguishes_spaces_tabs_and_empty_lines() -> ParityBatchCase {
    ParityBatchCase::value(
        "agda_editor_tactics_indent_distinguishes_spaces_tabs_and_empty_lines",
        r##"(mapcar
         (lambda (line)
           (list
            (prin1-to-string line)
            (agda-editor-tactics-indent line)))
         '("" "value : Set" "\tvalue : Set" " \tvalue : Set"
           "   " "\t" "  λ x → x"))"##,
        expect![[
            r#"OK (("\"\"" 0) ("\"value : Set\"" 1) ("\"\\11value : Set\"" 1) ("\" \\11value : Set\"" 1) ("\"   \"" 3) ("\"\\11\"" 0) ("\"  λ x → x\"" 2))"#
        ]],
    )
}

fn agda_editor_tactics_indent_handles_unicode_and_internal_whitespace() -> ParityBatchCase {
    ParityBatchCase::value(
        "agda_editor_tactics_indent_handles_unicode_and_internal_whitespace",
        r##"(mapcar
         (lambda (line)
           (list line (agda-editor-tactics-indent line)))
         '("  Σ-value : Set ℓ"
           "      _∙_ : Carrier → Carrier → Carrier"
           "  law : ∀ x → x ∙ ε ≡ x"
           "    spaced   internally"
           "  "))"##,
        expect![[
            r#"OK (("  Σ-value : Set ℓ" 2) ("      _∙_ : Carrier → Carrier → Carrier" 6) ("  law : ∀ x → x ∙ ε ≡ x" 2) ("    spaced   internally" 4) ("  " 2))"#
        ]],
    )
}

pub(super) fn indentation_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        agda_editor_tactics_indent_counts_leading_spaces_across_real_declarations(),
        agda_editor_tactics_indent_distinguishes_spaces_tabs_and_empty_lines(),
        agda_editor_tactics_indent_handles_unicode_and_internal_whitespace(),
    ]
}
