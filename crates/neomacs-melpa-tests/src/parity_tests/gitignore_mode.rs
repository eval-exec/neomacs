use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, GIT_MODES_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r###"
(require 'cl-lib)
(require 'git-modes)

(defun neomacs-gitignore-test-face-spans ()
  "Return all semantically fontified spans in buffer order."
  (font-lock-ensure)
  (let ((position (point-min))
        spans)
    (while (< position (point-max))
      (let* ((face (get-text-property position 'face))
             (next (next-single-property-change
                    position 'face nil (point-max))))
        (when face
          (push (list :range (list position next)
                      :text (buffer-substring-no-properties position next)
                      :face face)
                spans))
        (setq position next)))
    (nreverse spans)))

(defun neomacs-gitignore-test-syntax-at (needle offset)
  "Return parse state OFFSET characters into NEEDLE."
  (goto-char (point-min))
  (search-forward needle)
  (let* ((start (- (point) (length needle)))
         (position (+ start offset))
         (state (syntax-ppss position)))
    (list :needle needle
          :position position
          :comment (nth 4 state)
          :string (nth 3 state)
          :start (nth 8 state))))
"###;

fn successor_package_exposes_gitignore_mode_and_its_conf_parent_contract() -> ParityBatchCase {
    let elisp_form = r###"
(let ((descriptor (cadr (assq 'git-modes package-alist))))
  (list
   :package
   (list :name (package-desc-name descriptor)
         :version (package-version-join (package-desc-version descriptor))
         :requirements (package-desc-reqs descriptor)
         :features
         (mapcar (lambda (feature) (and (featurep feature) t))
                 '(git-modes gitignore-mode gitconfig-mode gitattributes-mode)))
   :surface
   (list (fboundp 'gitignore-mode)
         (boundp 'gitignore-mode-font-lock-keywords))
   :mode
   (with-temp-buffer
     (gitignore-mode)
     (list :major major-mode
           :name mode-name
           :parents (list (and (derived-mode-p 'conf-unix-mode) t)
                          (and (derived-mode-p 'conf-mode) t))
           :comments (list comment-start comment-start-skip comment-end)
           :font-lock font-lock-defaults
           :assignment-sign conf-assignment-sign
           :binding (key-binding (kbd "C-c C-a"))))))
"###;
    let expected = expect![[
        r##"OK (:package (:name git-modes :version "20260601.1550" :requirements ((emacs (28 1)) (compat (31 0))) :features (t t t t)) :surface (t t) :mode (:major gitignore-mode :name "Gitignore" :parents (t t) :comments ("#" "#+\\s *" "") :font-lock (gitignore-mode-font-lock-keywords t t) :assignment-sign nil :binding conf-align-assignments))"##
    ]];
    ParityBatchCase::value(
        "successor_package_exposes_gitignore_mode_and_its_conf_parent_contract",
        elisp_form,
        expected,
    )
}

fn repository_global_and_information_exclude_paths_select_gitignore_mode() -> ParityBatchCase {
    let elisp_form = r###"
(mapcar
 (lambda (path)
   (with-temp-buffer
     (setq buffer-file-name path)
     (set-auto-mode)
     (list path major-mode mode-name)))
 '("/work/project/.gitignore"
   "/work/project/packages/api/.gitignore"
   "/work/project/.git/info/exclude"
   "/work/home/.config/git/ignore"
   "/work/project/info/exclude"
   "/work/project/.ignore"))
"###;
    let expected = expect![[
        r#"OK (("/work/project/.gitignore" gitignore-mode "Gitignore") ("/work/project/packages/api/.gitignore" gitignore-mode "Gitignore") ("/work/project/.git/info/exclude" gitignore-mode "Gitignore") ("/work/home/.config/git/ignore" gitignore-mode "Gitignore") ("/work/project/info/exclude" gitignore-mode "Gitignore") ("/work/project/.ignore" fundamental-mode "Fundamental"))"#
    ]];
    ParityBatchCase::value(
        "repository_global_and_information_exclude_paths_select_gitignore_mode",
        elisp_form,
        expected,
    )
}

fn monorepo_ignore_rules_fontify_comments_negation_directories_and_globs() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (gitignore-mode)
  (insert "# Build products\n"
          "/target/\n"
          "**/node_modules/\n"
          "*.log\n"
          "!important.log\n"
          "docs/generated/*.html\n"
          "reports/report-??.json\n"
          "artifacts/[0-9][0-9]/*.zip\n")
  (list :faces (neomacs-gitignore-test-face-spans)
        :lines (line-number-at-pos (point-max))))
"###;
    let expected = expect![[
        r##"OK (:faces ((:range (1 17) :text "# Build products" :face font-lock-comment-face) (:range (18 19) :text "/" :face font-lock-constant-face) (:range (25 26) :text "/" :face font-lock-constant-face) (:range (27 29) :text "**" :face font-lock-keyword-face) (:range (29 30) :text "/" :face font-lock-constant-face) (:range (42 43) :text "/" :face font-lock-constant-face) (:range (44 45) :text "*" :face font-lock-keyword-face) (:range (50 51) :text "!" :face font-lock-negation-char-face) (:range (69 70) :text "/" :face font-lock-constant-face) (:range (79 80) :text "/" :face font-lock-constant-face) (:range (80 81) :text "*" :face font-lock-keyword-face) (:range (94 95) :text "/" :face font-lock-constant-face) (:range (102 104) :text "??" :face font-lock-keyword-face) (:range (119 120) :text "/" :face font-lock-constant-face) (:range (120 130) :text "[0-9][0-9]" :face font-lock-keyword-face) (:range (130 131) :text "/" :face font-lock-constant-face) (:range (131 132) :text "*" :face font-lock-keyword-face)) :lines 9)"##
    ]];
    ParityBatchCase::value(
        "monorepo_ignore_rules_fontify_comments_negation_directories_and_globs",
        elisp_form,
        expected,
    )
}

fn comment_fontification_follows_git_line_rules_while_syntax_state_remains_explicit()
-> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (gitignore-mode)
  (insert "# release comment\n"
          "  # indented pattern\n"
          "artifact#fragment\n"
          "\\#literal-leading-hash\n"
          "!keep#fragment\n")
  (list
   :faces (neomacs-gitignore-test-face-spans)
   :states
   (mapcar (lambda (probe)
             (neomacs-gitignore-test-syntax-at (car probe) (cdr probe)))
           '(("release comment" . 2)
             ("indented pattern" . 2)
             ("artifact#fragment" . 10)
             ("\\#literal-leading-hash" . 3)
             ("keep#fragment" . 6)))))
"###;
    let expected = expect![[
        r##"OK (:faces ((:range (1 18) :text "# release comment" :face font-lock-comment-face) (:range (81 82) :text "!" :face font-lock-negation-char-face)) :states ((:needle "release comment" :position 5 :comment t :string nil :start 1) (:needle "indented pattern" :position 25 :comment t :string nil :start 21) (:needle "artifact#fragment" :position 50 :comment t :string nil :start 48) (:needle "\\#literal-leading-hash" :position 61 :comment nil :string nil :start nil) (:needle "keep#fragment" :position 88 :comment t :string nil :start 86)))"##
    ]];
    ParityBatchCase::value(
        "comment_fontification_follows_git_line_rules_while_syntax_state_remains_explicit",
        elisp_form,
        expected,
    )
}

fn editing_rules_incrementally_updates_negation_directory_and_recursive_glob_faces()
-> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (gitignore-mode)
  (insert "cache/\n*.log\n")
  (let ((initial (neomacs-gitignore-test-face-spans)))
    (goto-char (point-min))
    (insert "!")
    (forward-line 1)
    (delete-region (line-beginning-position) (line-end-position))
    (insert "logs/**/*.log")
    (font-lock-flush)
    (list :initial initial
          :edited (neomacs-gitignore-test-face-spans)
          :buffer (buffer-substring-no-properties
                   (point-min) (point-max)))))
"###;
    let expected = expect![[
        r#"OK (:initial ((:range (6 7) :text "/" :face font-lock-constant-face) (:range (8 9) :text "*" :face font-lock-keyword-face)) :edited ((:range (1 2) :text "!" :face font-lock-negation-char-face) (:range (7 8) :text "/" :face font-lock-constant-face) (:range (13 14) :text "/" :face font-lock-constant-face) (:range (14 16) :text "**" :face font-lock-keyword-face) (:range (16 17) :text "/" :face font-lock-constant-face) (:range (17 18) :text "*" :face font-lock-keyword-face)) :buffer "!cache/\nlogs/**/*.log\n")"#
    ]];
    ParityBatchCase::value(
        "editing_rules_incrementally_updates_negation_directory_and_recursive_glob_faces",
        elisp_form,
        expected,
    )
}

fn commenting_and_uncommenting_selected_rules_round_trips_the_deployment_policy() -> ParityBatchCase
{
    let elisp_form = r###"
(with-temp-buffer
  (gitignore-mode)
  (insert "/dist/\n"
          "*.map\n"
          "!dist/manifest.json\n"
          "docs/private/\n")
  (goto-char (point-min))
  (forward-line 1)
  (let ((start (point)))
    (forward-line 2)
    (comment-region start (point)))
  (let ((commented (buffer-substring-no-properties
                    (point-min) (point-max))))
    (goto-char (point-min))
    (forward-line 1)
    (let ((start (point)))
      (forward-line 2)
      (uncomment-region start (point)))
    (list :commented commented
          :restored (buffer-substring-no-properties
                     (point-min) (point-max))
          :faces (neomacs-gitignore-test-face-spans))))
"###;
    let expected = expect![[
        r#"OK (:commented "/dist/\n# *.map\n# !dist/manifest.json\ndocs/private/\n" :restored "/dist/\n*.map\n!dist/manifest.json\ndocs/private/\n" :faces ((:range (1 2) :text "/" :face font-lock-constant-face) (:range (6 7) :text "/" :face font-lock-constant-face) (:range (8 9) :text "*" :face font-lock-keyword-face) (:range (14 15) :text "!" :face font-lock-negation-char-face) (:range (19 20) :text "/" :face font-lock-constant-face) (:range (38 39) :text "/" :face font-lock-constant-face) (:range (46 47) :text "/" :face font-lock-constant-face)))"#
    ]];
    ParityBatchCase::value(
        "commenting_and_uncommenting_selected_rules_round_trips_the_deployment_policy",
        elisp_form,
        expected,
    )
}

fn escaped_and_edge_patterns_reveal_the_exact_highlighting_boundaries() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (gitignore-mode)
  (insert "!important.log\n"
          "\\!literal-bang\n"
          "\\#literal-hash\n"
          "build/file?.o\n"
          "docs/[[:digit:]]?.pdf\n"
          "vendor/**/generated/\n"
          "trailing-space \\ \n")
  (neomacs-gitignore-test-face-spans))
"###;
    let expected = expect![[
        r#"OK ((:range (1 2) :text "!" :face font-lock-negation-char-face) (:range (51 52) :text "/" :face font-lock-constant-face) (:range (56 57) :text "?" :face font-lock-keyword-face) (:range (64 65) :text "/" :face font-lock-constant-face) (:range (65 75) :text "[[:digit:]" :face font-lock-keyword-face) (:range (76 77) :text "?" :face font-lock-keyword-face) (:range (88 89) :text "/" :face font-lock-constant-face) (:range (89 91) :text "**" :face font-lock-keyword-face) (:range (91 92) :text "/" :face font-lock-constant-face) (:range (101 102) :text "/" :face font-lock-constant-face))"#
    ]];
    ParityBatchCase::value(
        "escaped_and_edge_patterns_reveal_the_exact_highlighting_boundaries",
        elisp_form,
        expected,
    )
}

#[test]
fn gitignore_mode_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(GIT_MODES_MELPA_PIN, "git-modes.el")
            .expect("reuse revision-pinned Git Modes source below ./tmp")
            .with_timeout(Duration::from_secs(240))
            .with_prelude(PRELUDE),
        "gitignore-mode-package-batch",
        "Gitignore Mode (from Git Modes)",
        &[
            successor_package_exposes_gitignore_mode_and_its_conf_parent_contract(),
            repository_global_and_information_exclude_paths_select_gitignore_mode(),
            monorepo_ignore_rules_fontify_comments_negation_directories_and_globs(),
            comment_fontification_follows_git_line_rules_while_syntax_state_remains_explicit(),
            editing_rules_incrementally_updates_negation_directory_and_recursive_glob_faces(),
            commenting_and_uncommenting_selected_rules_round_trips_the_deployment_policy(),
            escaped_and_edge_patterns_reveal_the_exact_highlighting_boundaries(),
        ],
    );
}
