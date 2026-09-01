use expect_test::expect;

use super::ParityBatchCase;

/// The round trip a user makes when trying a theme out: `load-theme`, look at
/// it, `disable-theme`.  All eighteen faces amber-glow sets are resolved before,
/// during and after, and the last element asserts the restored report is `equal'
/// to the baseline rather than merely plausible.
///
/// `frame-background-mode` is in the report because amber-glow, unlike most dark
/// themes, makes no `custom-theme-set-variables` call at all - it never declares
/// itself dark, and the variable stays nil across the whole round trip even
/// though the background it paints is nearly black.
fn enabling_the_theme_repaints_its_eighteen_faces_and_disabling_restores_them() -> ParityBatchCase {
    ParityBatchCase::value(
        "enabling_the_theme_repaints_its_eighteen_faces_and_disabling_restores_them",
        r##"(let ((baseline (amber-test-face-report amber-test-themed-faces)))
  (load-theme 'amber-glow t)
  (let ((enabled (list (copy-sequence custom-enabled-themes)
                       (and (custom-theme-enabled-p 'amber-glow) t)
                       (amber-test-face-count)
                       frame-background-mode
                       (amber-test-face-report amber-test-themed-faces))))
    (disable-theme 'amber-glow)
    (let ((restored (amber-test-face-report amber-test-themed-faces)))
      (list baseline
            enabled
            (copy-sequence custom-enabled-themes)
            frame-background-mode
            restored
            (equal baseline restored)))))"##,
        expect![[
            r##"OK (((default (:foreground . "unspecified-fg") (:background . "unspecified-bg")) (cursor (:background . "white")) (fringe (:background . "gray") (:foreground . unspecified)) (region (:background . unspecified) (:foreground . unspecified)) (highlight (:background . unspecified) (:foreground . unspecified)) (vertical-border (:background . unspecified) (:foreground . unspecified)) (font-lock-builtin-face (:foreground . unspecified)) (font-lock-comment-face (:foreground . unspecified)) (font-lock-constant-face (:foreground . unspecified)) (font-lock-function-name-face (:foreground . unspecified)) (font-lock-keyword-face (:foreground . unspecified)) (font-lock-string-face (:foreground . unspecified)) (font-lock-type-face (:foreground . unspecified)) (font-lock-variable-name-face (:foreground . unspecified)) (font-lock-warning-face (:foreground . unspecified) (:weight . bold) (:inherit . error)) (mode-line (:background . unspecified) (:foreground . unspecified)) (mode-line-inactive (:background . unspecified) (:foreground . unspecified)) (minibuffer-prompt (:foreground . "cyan"))) ((amber-glow) t 18 nil ((default (:foreground . "#EDE6D6") (:background . "#15130C")) (cursor (:background . "#EDE6D6")) (fringe (:background . "#15130C") (:foreground . unspecified)) (region (:background . "#362F21") (:foreground . unspecified)) (highlight (:background . "#EDE6D6") (:foreground . "#15130C")) (vertical-border (:background . "#15130C") (:foreground . "#EDE6D6")) (font-lock-builtin-face (:foreground . "#B28E63")) (font-lock-comment-face (:foreground . "#7D6C4B")) (font-lock-constant-face (:foreground . "#D19A66")) (font-lock-function-name-face (:foreground . "#C87850")) (font-lock-keyword-face (:foreground . "#5E3724")) (font-lock-string-face (:foreground . "#93655E")) (font-lock-type-face (:foreground . "#506948")) (font-lock-variable-name-face (:foreground . "#6AC24E")) (font-lock-warning-face (:foreground . "#EDE6D6") (:weight . bold) (:inherit . unspecified)) (mode-line (:background . "#362F21") (:foreground . "#EDE6D6")) (mode-line-inactive (:background . "#15130C") (:foreground . "#EDE6D6")) (minibuffer-prompt (:foreground . "#945738")))) nil nil ((default (:foreground . "unspecified-fg") (:background . "unspecified-bg")) (cursor (:background . "white")) (fringe (:background . "gray") (:foreground . unspecified)) (region (:background . unspecified) (:foreground . unspecified)) (highlight (:background . unspecified) (:foreground . unspecified)) (vertical-border (:background . unspecified) (:foreground . unspecified)) (font-lock-builtin-face (:foreground . unspecified)) (font-lock-comment-face (:foreground . unspecified)) (font-lock-constant-face (:foreground . unspecified)) (font-lock-function-name-face (:foreground . unspecified)) (font-lock-keyword-face (:foreground . unspecified)) (font-lock-string-face (:foreground . unspecified)) (font-lock-type-face (:foreground . unspecified)) (font-lock-variable-name-face (:foreground . unspecified)) (font-lock-warning-face (:foreground . unspecified) (:weight . bold) (:inherit . error)) (mode-line (:background . unspecified) (:foreground . unspecified)) (mode-line-inactive (:background . unspecified) (:foreground . unspecified)) (minibuffer-prompt (:foreground . "cyan"))) t)"##
        ]],
    )
}

fn a_font_locked_elisp_and_c_buffer_are_painted_in_the_amber_palette() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_font_locked_elisp_and_c_buffer_are_painted_in_the_amber_palette",
        r##"(unwind-protect
    (progn
      (load-theme 'amber-glow t)
      (list
       (with-temp-buffer
         (emacs-lisp-mode)
         (insert ";; Amber demo\n"
                 "(defun amber-demo (path)\n"
                 "  \"Read PATH; return its contents.\"\n"
                 "  (let ((limit 10))\n"
                 "    (message \"read %s\" path)\n"
                 "    (car limit)))\n")
         (font-lock-ensure)
         (list (amber-test-token-faces
                '(";; Amber demo" "defun" "amber-demo"
                  "\"Read PATH; return its contents.\"" "let"
                  "\"read %s\"" "car"))
               (buffer-substring-no-properties (point-min) (point-max))))
       (with-temp-buffer
         (c-mode)
         (insert "/* amber */\n"
                 "#include <stdio.h>\n"
                 "int main(void) {\n"
                 "  const char *s = \"hi\";\n"
                 "  return 0;\n"
                 "}\n")
         (font-lock-ensure)
         (list (amber-test-token-faces
                '("/* amber */" "#include" "stdio.h" "int" "main"
                  "const" "\"hi\"" "return"))
               (buffer-substring-no-properties (point-min) (point-max))))))
  (disable-theme 'amber-glow))"##,
        expect![[
            r##"OK ((((";; Amber demo" font-lock-comment-delimiter-face "#7D6C4B" unspecified) ("defun" font-lock-keyword-face "#5E3724" unspecified) ("amber-demo" font-lock-function-name-face "#C87850" unspecified) ("\"Read PATH; return its contents.\"" font-lock-doc-face "#93655E" unspecified) ("let" font-lock-keyword-face "#5E3724" unspecified) ("\"read %s\"" font-lock-string-face "#93655E" unspecified) ("car" nil nil nil)) ";; Amber demo\n(defun amber-demo (path)\n  \"Read PATH; return its contents.\"\n  (let ((limit 10))\n    (message \"read %s\" path)\n    (car limit)))\n") ((("/* amber */" font-lock-comment-delimiter-face "#7D6C4B" unspecified) ("#include" font-lock-preprocessor-face "#B28E63" unspecified) ("stdio.h" font-lock-string-face "#93655E" unspecified) ("int" font-lock-type-face "#506948" unspecified) ("main" font-lock-function-name-face "#C87850" unspecified) ("const" font-lock-keyword-face "#5E3724" unspecified) ("\"hi\"" font-lock-string-face "#93655E" unspecified) ("return" font-lock-keyword-face "#5E3724" unspecified)) "/* amber */\n#include <stdio.h>\nint main(void) {\n  const char *s = \"hi\";\n  return 0;\n}\n"))"##
        ]],
    )
}

fn the_faces_the_theme_never_names_move_only_where_they_inherit_from_one_it_sets() -> ParityBatchCase
{
    ParityBatchCase::value(
        "the_faces_the_theme_never_names_move_only_where_they_inherit_from_one_it_sets",
        r##"(progn
  (require 'paren)
  (let* ((inheriting '((header-line :background :foreground)
                       (line-number :foreground :inherit)
                       (line-number-current-line :foreground :inherit)
                       (font-lock-preprocessor-face :foreground :inherit)))
         (warning '((font-lock-warning-face :foreground :weight :inherit)))
         (before-unreached (amber-test-face-report amber-test-unreached-faces))
         (before-inheriting (amber-test-face-report inheriting))
         (before-warning (amber-test-face-report warning)))
    (load-theme 'amber-glow t)
    (let ((during (list (amber-test-face-report amber-test-unreached-faces)
                        (amber-test-face-report inheriting)
                        (amber-test-face-report warning))))
      (disable-theme 'amber-glow)
      (list before-unreached
            before-inheriting
            before-warning
            during
            (equal before-unreached (car during))
            (amber-test-face-report warning)
            (equal before-warning (amber-test-face-report warning))))))"##,
        expect![[
            r##"OK (((isearch (:background . unspecified) (:foreground . unspecified)) (lazy-highlight (:background . unspecified) (:foreground . unspecified)) (link (:foreground . unspecified) (:underline . t)) (show-paren-match (:background . unspecified) (:foreground . unspecified)) (secondary-selection (:background . unspecified)) (trailing-whitespace (:background . unspecified)) (tab-bar (:background . "grey") (:foreground . unspecified)) (tooltip (:background . unspecified) (:foreground . unspecified)) (match (:background . unspecified)) (shadow (:foreground . unspecified))) ((header-line (:background . unspecified) (:foreground . unspecified)) (line-number (:foreground . "unspecified-fg") (:inherit shadow default)) (line-number-current-line (:foreground . "unspecified-fg") (:inherit . line-number)) (font-lock-preprocessor-face (:foreground . unspecified) (:inherit . font-lock-builtin-face))) ((font-lock-warning-face (:foreground . unspecified) (:weight . bold) (:inherit . error))) (((isearch (:background . unspecified) (:foreground . unspecified)) (lazy-highlight (:background . unspecified) (:foreground . unspecified)) (link (:foreground . unspecified) (:underline . t)) (show-paren-match (:background . unspecified) (:foreground . unspecified)) (secondary-selection (:background . unspecified)) (trailing-whitespace (:background . unspecified)) (tab-bar (:background . "grey") (:foreground . unspecified)) (tooltip (:background . unspecified) (:foreground . unspecified)) (match (:background . unspecified)) (shadow (:foreground . unspecified))) ((header-line (:background . "#362F21") (:foreground . "#EDE6D6")) (line-number (:foreground . "#EDE6D6") (:inherit shadow default)) (line-number-current-line (:foreground . "#EDE6D6") (:inherit . line-number)) (font-lock-preprocessor-face (:foreground . "#B28E63") (:inherit . font-lock-builtin-face))) ((font-lock-warning-face (:foreground . "#EDE6D6") (:weight . bold) (:inherit . unspecified)))) t ((font-lock-warning-face (:foreground . unspecified) (:weight . bold) (:inherit . error))) t)"##
        ]],
    )
}

fn a_customized_face_outranks_the_theme_but_a_plain_attribute_does_not() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_customized_face_outranks_the_theme_but_a_plain_attribute_does_not",
        r##"(let (mine customized with-theme after-later-customization reloaded)
  (set-face-attribute 'font-lock-comment-face nil :foreground "#00FF00")
  (setq mine (amber-test-face-report '((font-lock-comment-face :foreground))))
  (custom-set-faces '(font-lock-string-face ((t (:foreground "#FF00FF")))))
  (setq customized (amber-test-face-report '((font-lock-string-face :foreground))))
  (load-theme 'amber-glow t)
  (setq with-theme (list (copy-sequence custom-enabled-themes)
                         (amber-test-face-report
                          '((font-lock-comment-face :foreground)
                            (font-lock-string-face :foreground)
                            (font-lock-type-face :foreground)))))
  (custom-set-faces '(font-lock-type-face ((t (:foreground "#00FFFF")))))
  (setq after-later-customization
        (amber-test-face-report '((font-lock-type-face :foreground))))
  (load-theme 'amber-glow t)
  (setq reloaded (list (copy-sequence custom-enabled-themes)
                       (amber-test-face-count)
                       (amber-test-face-report
                        '((font-lock-keyword-face :foreground)))))
  (disable-theme 'amber-glow)
  (list mine
        customized
        with-theme
        after-later-customization
        reloaded
        (copy-sequence custom-enabled-themes)
        (amber-test-face-report
         '((font-lock-comment-face :foreground)
           (font-lock-string-face :foreground)
           (font-lock-type-face :foreground)
           (font-lock-keyword-face :foreground)))))"##,
        expect![[
            r##"OK (((font-lock-comment-face (:foreground . "#00FF00"))) ((font-lock-string-face (:foreground . "#FF00FF"))) ((amber-glow) ((font-lock-comment-face (:foreground . "#7D6C4B")) (font-lock-string-face (:foreground . "#FF00FF")) (font-lock-type-face (:foreground . "#506948")))) ((font-lock-type-face (:foreground . "#00FFFF"))) ((amber-glow) 18 ((font-lock-keyword-face (:foreground . "#5E3724")))) nil ((font-lock-comment-face (:foreground . "#00FF00")) (font-lock-string-face (:foreground . "#FF00FF")) (font-lock-type-face (:foreground . "#00FFFF")) (font-lock-keyword-face (:foreground . unspecified))))"##
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        enabling_the_theme_repaints_its_eighteen_faces_and_disabling_restores_them(),
        a_font_locked_elisp_and_c_buffer_are_painted_in_the_amber_palette(),
        the_faces_the_theme_never_names_move_only_where_they_inherit_from_one_it_sets(),
        a_customized_face_outranks_the_theme_but_a_plain_attribute_does_not(),
    ]
}
