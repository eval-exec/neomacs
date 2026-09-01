use expect_test::expect;

use super::ParityBatchCase;

fn prefix_font_lock_features_preserves_language_override_and_rule_payload() -> ParityBatchCase {
    ParityBatchCase::value(
        "prefix_font_lock_features_preserves_language_override_and_rule_payload",
        r##"(let* ((capture '((identifier) @font-lock-variable-name-face))
              (settings
               (list
                (list 'tsx
                      '((identifier) @font-lock-variable-name-face)
                      'identifier
                      capture)
                (list 'tsx :override 'keyword 'keep))))
          (list
           settings
           (astro-ts-mode--prefix-font-lock-features
            "embedded" settings)
           settings))"##,
        expect![
            "OK (#3=((tsx #1=((identifier) @font-lock-variable-name-face) identifier #2=((identifier) @font-lock-variable-name-face)) (tsx :override keyword keep)) ((tsx #1# embedded-identifier #2#) (tsx :override embedded-keyword keep)) #3#)"
        ],
    )
}

fn prefix_font_lock_features_handles_empty_and_partial_settings_exactly() -> ParityBatchCase {
    ParityBatchCase::value(
        "prefix_font_lock_features_handles_empty_and_partial_settings_exactly",
        r##"(list
          (astro-ts-mode--prefix-font-lock-features "tsx" nil)
          (astro-ts-mode--prefix-font-lock-features
           "x" '((astro query feature)
                 (css nil nil nil)
                 (tsx one two three four))))"##,
        expect!["OK (nil ((astro query x-feature nil) (css nil x-nil nil) (tsx one x-two three)))"],
    )
}

fn real_typescript_settings_gain_only_tsx_feature_prefixes() -> ParityBatchCase {
    ParityBatchCase::value(
        "real_typescript_settings_gain_only_tsx_feature_prefixes",
        r##"(let* ((original
                (typescript-ts-mode--font-lock-settings 'tsx))
               (prefixed
                (astro-ts-mode--prefix-font-lock-features
                 "tsx" original)))
          (list
           (length original)
           (length prefixed)
           (seq-take
            (mapcar
             (lambda (pair)
               (list (nth 2 (car pair))
                     (nth 2 (cdr pair))
                     (equal (nth 0 (car pair))
                            (nth 0 (cdr pair)))
                     (equal (nth 1 (car pair))
                            (nth 1 (cdr pair)))
                     (equal (nth 3 (car pair))
                            (nth 3 (cdr pair)))))
             (cl-mapcar #'cons original prefixed))
            12)
           (seq-every-p
            (lambda (setting)
              (string-prefix-p
               "tsx-"
               (symbol-name (nth 2 setting))))
            prefixed)))"##,
        expect![
            "OK (16 16 ((comment tsx-comment t t t) (constant tsx-constant t t t) (keyword tsx-keyword t t t) (string tsx-string t t t) (declaration tsx-declaration t t t) (identifier tsx-identifier t t t) (property tsx-property t t t) (expression tsx-expression t t t) (function tsx-function t t t) (pattern tsx-pattern t t t) (jsx tsx-jsx t t t) (number tsx-number t t t)) t)"
        ],
    )
}

fn real_css_settings_gain_only_css_feature_prefixes() -> ParityBatchCase {
    ParityBatchCase::value(
        "real_css_settings_gain_only_css_feature_prefixes",
        r##"(let ((prefixed
                (astro-ts-mode--prefix-font-lock-features
                 "css" css--treesit-settings)))
          (list
           (length css--treesit-settings)
           (length prefixed)
           (seq-take
            (mapcar
             (lambda (setting)
               (list (type-of (nth 0 setting))
                     (nth 2 setting)))
             prefixed)
            15)
           (seq-every-p
            (lambda (setting)
              (string-prefix-p
               "css-"
               (symbol-name (nth 2 setting))))
            prefixed)))"##,
        expect![
            "OK (12 12 ((treesit-compiled-query css-comment) (treesit-compiled-query css-string) (treesit-compiled-query css-keyword) (treesit-compiled-query css-variable) (treesit-compiled-query css-operator) (treesit-compiled-query css-selector) (treesit-compiled-query css-property) (treesit-compiled-query css-function) (treesit-compiled-query css-constant) (treesit-compiled-query css-query) (treesit-compiled-query css-bracket) (treesit-compiled-query css-error)) t)"
        ],
    )
}

fn distinct_prefixes_keep_colliding_embedded_feature_names_disjoint() -> ParityBatchCase {
    ParityBatchCase::value(
        "distinct_prefixes_keep_colliding_embedded_feature_names_disjoint",
        r##"(let ((settings
                '((tsx query comment override)
                  (tsx query string override)
                  (tsx query bracket override))))
          (list
           (mapcar
            (lambda (setting) (nth 2 setting))
            (astro-ts-mode--prefix-font-lock-features
             "tsx" settings))
           (mapcar
            (lambda (setting) (nth 2 setting))
            (astro-ts-mode--prefix-font-lock-features
             "css" settings))
           settings))"##,
        expect![
            "OK ((tsx-comment tsx-string tsx-bracket) (css-comment css-string css-bracket) ((tsx query comment override) (tsx query string override) (tsx query bracket override)))"
        ],
    )
}

fn repeated_prefixing_is_explicit_compositional_and_nonmutating() -> ParityBatchCase {
    ParityBatchCase::value(
        "repeated_prefixing_is_explicit_compositional_and_nonmutating",
        r##"(let* ((settings
                '((astro query definition override)
                  (astro query bracket override)))
               (once
                (astro-ts-mode--prefix-font-lock-features
                 "one" settings))
               (twice
                (astro-ts-mode--prefix-font-lock-features
                 "two" once)))
          (list settings once twice
                (equal settings
                       '((astro query definition override)
                         (astro query bracket override)))))"##,
        expect![
            "OK (((astro query definition override) (astro query bracket override)) ((astro query one-definition override) (astro query one-bracket override)) ((astro query two-one-definition override) (astro query two-one-bracket override)) t)"
        ],
    )
}

pub(super) fn prefix_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        prefix_font_lock_features_preserves_language_override_and_rule_payload(),
        prefix_font_lock_features_handles_empty_and_partial_settings_exactly(),
        real_typescript_settings_gain_only_tsx_feature_prefixes(),
        real_css_settings_gain_only_css_feature_prefixes(),
        distinct_prefixes_keep_colliding_embedded_feature_names_disjoint(),
        repeated_prefixing_is_explicit_compositional_and_nonmutating(),
    ]
}
