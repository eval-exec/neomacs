use expect_test::expect;

use super::ParityBatchCase;

fn mode_initializes_comment_syntax_and_auto_mode() -> ParityBatchCase {
    ParityBatchCase::value(
        "mode_initializes_comment_syntax_and_auto_mode",
        r####"
(with-temp-buffer
  (toml-mode)
  (list :mode major-mode
        :comment-start comment-start
        :tabs indent-tabs-mode
        :auto (cdr (assoc "\\.toml\\'" auto-mode-alist))
        :align-rules (and (assq 'toml-equals align-mode-rules-list) t)))
"####,
        expect![[
            r##"OK (:mode toml-mode :comment-start "#" :tabs nil :auto toml-mode :align-rules t)"##
        ]],
    )
}

fn font_lock_keywords_cover_booleans_and_tables() -> ParityBatchCase {
    ParityBatchCase::value(
        "font_lock_keywords_cover_booleans_and_tables",
        r####"
(with-temp-buffer
  (toml-mode)
  (insert "enabled = true\n[package]\nname = \"x\"\n")
  (font-lock-ensure)
  (goto-char (point-min))
  (search-forward "true")
  (backward-char 1)
  (let ((bool-face (get-text-property (point) 'face)))
    (goto-char (point-min))
    (search-forward "package")
    (backward-char 1)
    (let ((table-face (get-text-property (point) 'face)))
      (list :bool bool-face
            :table table-face
            :keywords-nonempty (and toml-mode-font-lock-keywords t)))))
"####,
        expect![[
            r#"OK (:bool font-lock-constant-face :table font-lock-type-face :keywords-nonempty t)"#
        ]],
    )
}

fn hash_starts_comment_and_semicolon_is_punctuation() -> ParityBatchCase {
    ParityBatchCase::value(
        "hash_starts_comment_and_semicolon_is_punctuation",
        r####"
(with-temp-buffer
  (toml-mode)
  (insert "a = 1 # note\nx = \"; not-comment\"\n")
  (goto-char (point-min))
  (search-forward "#")
  (list :hash-syntax (string (char-syntax (char-before)))
        :semi-syntax
        (progn
          (goto-char (point-min))
          (search-forward ";")
          (string (char-syntax (char-before))))))
"####,
        expect![[r#"OK (:hash-syntax "<" :semi-syntax ".")"#]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        mode_initializes_comment_syntax_and_auto_mode(),
        font_lock_keywords_cover_booleans_and_tables(),
        hash_starts_comment_and_semicolon_is_punctuation(),
    ]
}
