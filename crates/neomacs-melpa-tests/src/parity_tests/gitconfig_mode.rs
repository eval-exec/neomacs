use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, GIT_MODES_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r###"
(require 'cl-lib)
(require 'imenu)
(require 'git-modes)

(defun neomacs-gitconfig-test-face-spans ()
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

(defun neomacs-gitconfig-test-syntax-at (needle offset)
  "Return syntax state OFFSET characters into NEEDLE."
  (goto-char (point-min))
  (search-forward needle)
  (let* ((start (- (point) (length needle)))
         (position (+ start offset))
         (state (syntax-ppss position)))
    (list :needle needle
          :position position
          :char-syntax (char-syntax (char-after position))
          :string (nth 3 state)
          :comment (nth 4 state)
          :start (nth 8 state))))

(defun neomacs-gitconfig-test-imenu-entry (entry)
  "Normalize one recursive Imenu ENTRY to line-oriented data."
  (cond
   ((and (consp entry) (or (integerp (cdr entry)) (markerp (cdr entry))))
    (save-excursion
      (goto-char (cdr entry))
      (list (car entry)
            :line (line-number-at-pos)
            :text (buffer-substring-no-properties
                   (line-beginning-position) (line-end-position)))))
   ((consp entry)
    (cons (car entry)
          (mapcar #'neomacs-gitconfig-test-imenu-entry (cdr entry))))
   (t entry)))
"###;

fn successor_package_registers_the_gitconfig_mode_surface_and_parent_contract() -> ParityBatchCase {
    let elisp_form = r###"
(let ((descriptor (cadr (assq 'git-modes package-alist))))
  (list
   :package
   (list :name (package-desc-name descriptor)
         :version (package-version-join (package-desc-version descriptor))
         :requirements (package-desc-reqs descriptor)
         :features
         (mapcar (lambda (feature) (and (featurep feature) t))
                 '(git-modes gitconfig-mode gitattributes-mode gitignore-mode)))
   :surface
   (mapcar #'fboundp
           '(gitconfig-mode gitconfig-indent-line
             gitconfig-line-indented-p gitconfig-point-in-indentation-p
             gitconfig-indentation-string))
   :mode
   (with-temp-buffer
     (gitconfig-mode)
     (list :major major-mode
           :name mode-name
           :parents (list (and (derived-mode-p 'conf-unix-mode) t)
                          (and (derived-mode-p 'conf-mode) t))
           :comments (list comment-start comment-start-skip)
           :indent (list indent-tabs-mode tab-width indent-line-function)
           :text-conversion text-conversion-style
           :bindings
           (mapcar (lambda (key) (cons key (key-binding (kbd key))))
                   '("C-c C-a" "C-c C-u"))))))
"###;
    let expected = expect![[
        r##"OK (:package (:name git-modes :version "20260601.1550" :requirements ((emacs (28 1)) (compat (31 0))) :features (t t t t)) :surface (t t t t t) :mode (:major gitconfig-mode :name "Gitconfig" :parents (t t) :comments ("#" "#+\\s *") :indent (t 8 gitconfig-indent-line) :text-conversion t :bindings (("C-c C-a" . conf-align-assignments) ("C-c C-u" . conf-unix-mode))))"##
    ]];
    ParityBatchCase::value(
        "successor_package_registers_the_gitconfig_mode_surface_and_parent_contract",
        elisp_form,
        expected,
    )
}

fn real_git_configuration_paths_select_gitconfig_mode_without_content_guessing() -> ParityBatchCase
{
    let elisp_form = r###"
(mapcar
 (lambda (path)
   (with-temp-buffer
     (setq buffer-file-name path)
     (set-auto-mode)
     (list path major-mode mode-name)))
 '("/work/home/.gitconfig"
   "/work/project/.git/config"
   "/work/project/.git/modules/vendor/library/config"
   "/work/project/git/config"
   "/work/project/.gitmodules"
   "/etc/gitconfig"
   "/work/project/config"))
"###;
    let expected = expect![[
        r#"OK (("/work/home/.gitconfig" gitconfig-mode "Gitconfig") ("/work/project/.git/config" gitconfig-mode "Gitconfig") ("/work/project/.git/modules/vendor/library/config" gitconfig-mode "Gitconfig") ("/work/project/git/config" gitconfig-mode "Gitconfig") ("/work/project/.gitmodules" gitconfig-mode "Gitconfig") ("/etc/gitconfig" gitconfig-mode "Gitconfig") ("/work/project/config" conf-unix-mode "Conf[Unix]"))"#
    ]];
    ParityBatchCase::value(
        "real_git_configuration_paths_select_gitconfig_mode_without_content_guessing",
        elisp_form,
        expected,
    )
}

fn practical_user_remote_core_and_feature_configuration_fontifies_semantic_fields()
-> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (gitconfig-mode)
  (insert "# Workstation identity\n"
          "[user]\n"
          "\tname = Alice Example\n"
          "\temail = alice@example.com\n"
          "[remote \"origin\"]\n"
          "\turl = ssh://git@example.com/platform/app.git\n"
          "\tfetch = +refs/heads/*:refs/remotes/origin/*\n"
          "[core]\n"
          "\tbare = false\n"
          "\tcompression = 9\n"
          "\teditor = emacsclient\n"
          "[feature]\n"
          "\tenabled = on\n"
          "\tdisabled = no\n")
  (list :faces (neomacs-gitconfig-test-face-spans)
        :lines (line-number-at-pos (point-max))))
"###;
    let expected = expect![[
        r##"OK (:faces ((:range (1 3) :text "# " :face font-lock-comment-delimiter-face) (:range (3 24) :text "Workstation identity\n" :face font-lock-comment-face) (:range (25 29) :text "user" :face font-lock-type-face) (:range (32 36) :text "name" :face font-lock-variable-name-face) (:range (54 59) :text "email" :face font-lock-variable-name-face) (:range (81 87) :text "remote" :face font-lock-type-face) (:range (88 96) :text "\"origin\"" :face font-lock-function-name-face) (:range (99 102) :text "url" :face font-lock-variable-name-face) (:range (145 150) :text "fetch" :face font-lock-variable-name-face) (:range (190 194) :text "core" :face font-lock-type-face) (:range (197 201) :text "bare" :face font-lock-variable-name-face) (:range (204 209) :text "false" :face font-lock-keyword-face) (:range (211 222) :text "compression" :face font-lock-variable-name-face) (:range (225 226) :text "9" :face font-lock-constant-face) (:range (228 234) :text "editor" :face font-lock-variable-name-face) (:range (250 257) :text "feature" :face font-lock-type-face) (:range (260 267) :text "enabled" :face font-lock-variable-name-face) (:range (270 272) :text "on" :face font-lock-keyword-face) (:range (274 282) :text "disabled" :face font-lock-variable-name-face) (:range (285 287) :text "no" :face font-lock-keyword-face)) :lines 15)"##
    ]];
    ParityBatchCase::value(
        "practical_user_remote_core_and_feature_configuration_fontifies_semantic_fields",
        elisp_form,
        expected,
    )
}

fn syntax_distinguishes_both_comment_styles_subsection_strings_and_literal_apostrophes()
-> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (gitconfig-mode)
  (insert "; legacy setting\n"
          "# managed setting\n"
          "[remote \"origin\"]\n"
          "\turl = ssh://git@example.com/team/app.git\n"
          "\tname = O'Reilly\n"
          "\tcache-key.value = enabled\n")
  (font-lock-ensure)
  (list
   :states
   (mapcar (lambda (probe)
             (neomacs-gitconfig-test-syntax-at (car probe) (cdr probe)))
           '(("legacy" . 2)
             ("managed" . 2)
             ("origin" . 2)
             ("O'Reilly" . 1)
             ("ssh://git@example.com/team/app.git" . 8)))
   :syntax
   (mapcar (lambda (character)
             (cons character (char-syntax character)))
           '(?\; ?# ?' ?\" ?_ ?- ?.))))
"###;
    let expected = expect![[
        r#"OK (:states ((:needle "legacy" :position 5 :char-syntax 119 :string nil :comment t :start 1) (:needle "managed" :position 22 :char-syntax 119 :string nil :comment t :start 18) (:needle "origin" :position 47 :char-syntax 119 :string 34 :comment nil :start 44) (:needle "O'Reilly" :position 105 :char-syntax 46 :string nil :comment nil :start nil) (:needle "ssh://git@example.com/team/app.git" :position 69 :char-syntax 119 :string nil :comment nil :start nil)) :syntax ((59 . 60) (35 . 60) (39 . 46) (34 . 34) (95 . 95) (45 . 95) (46 . 95)))"#
    ]];
    ParityBatchCase::value(
        "syntax_distinguishes_both_comment_styles_subsection_strings_and_literal_apostrophes",
        elisp_form,
        expected,
    )
}

fn whole_file_indentation_canonicalizes_sections_settings_and_comments_idempotently()
-> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (gitconfig-mode)
  (insert "  [user]\n"
          "name = Alice Example\n"
          "    email = alice@example.com\n"
          "  # managed identity\n"
          "  ; legacy identity\n"
          "        [core]\n"
          "    editor = emacsclient\n"
          "bare = false\n")
  (let ((before
         (save-excursion
           (goto-char (point-min))
           (let (states)
             (while (not (eobp))
               (push (list (buffer-substring-no-properties
                            (line-beginning-position) (line-end-position))
                           (and (gitconfig-line-indented-p) t))
                     states)
               (forward-line 1))
             (nreverse states)))))
    (indent-region (point-min) (point-max))
    (let ((once (buffer-string)))
      (indent-region (point-min) (point-max))
      (list :before before
            :after once
            :idempotent (equal once (buffer-string))
            :all-lines-indented
            (save-excursion
              (goto-char (point-min))
              (let ((result t))
                (while (not (eobp))
                  (unless (gitconfig-line-indented-p)
                    (setq result nil))
                  (forward-line 1))
                result))))))
"###;
    let expected = expect![[
        r#"OK (:before (("  [user]" nil) ("name = Alice Example" nil) ("    email = alice@example.com" nil) ("  # managed identity" nil) ("  ; legacy identity" nil) ("        [core]" nil) ("    editor = emacsclient" nil) ("bare = false" nil)) :after "[user]\n\11name = Alice Example\n\11email = alice@example.com\n\11# managed identity\n\11; legacy identity\n[core]\n\11editor = emacsclient\n\11bare = false\n" :idempotent t :all-lines-indented t)"#
    ]];
    ParityBatchCase::value(
        "whole_file_indentation_canonicalizes_sections_settings_and_comments_idempotently",
        elisp_form,
        expected,
    )
}

fn indentation_policy_supports_space_only_projects_and_preserves_editing_position()
-> ParityBatchCase {
    let elisp_form = r###"
(list
 :strings
 (mapcar (lambda (settings)
           (with-temp-buffer
             (gitconfig-mode)
             (setq indent-tabs-mode (car settings)
                   tab-width (cdr settings))
             (list settings (gitconfig-indentation-string))))
         '((t . 8) (nil . 2) (nil . 4)))
 :space-project
 (with-temp-buffer
   (gitconfig-mode)
   (setq indent-tabs-mode nil
         tab-width 4)
   (insert "[user]\nname = Alice\nemail = alice@example.com\n")
   (indent-region (point-min) (point-max))
   (buffer-string))
 :point-preservation
 (with-temp-buffer
   (gitconfig-mode)
   (insert "name = Alice Example")
   (search-backward "Example")
   (forward-char 2)
   (let ((before (list :point (point)
                       :column (current-column)
                       :suffix (buffer-substring-no-properties
                                (point) (line-end-position)))))
     (gitconfig-indent-line)
     (list :before before
           :after (list :point (point)
                        :column (current-column)
                        :suffix (buffer-substring-no-properties
                                 (point) (line-end-position)))
           :buffer (buffer-string)))))
"###;
    let expected = expect![[
        r#"OK (:strings (((t . 8) "\11") ((nil . 2) "  ") ((nil . 4) "    ")) :space-project "[user]\n    name = Alice\n    email = alice@example.com\n" :point-preservation (:before (:point 16 :column 15 :suffix "ample") :after (:point 17 :column 23 :suffix "ample") :buffer "\11name = Alice Example"))"#
    ]];
    ParityBatchCase::value(
        "indentation_policy_supports_space_only_projects_and_preserves_editing_position",
        elisp_form,
        expected,
    )
}

fn comment_round_trip_and_assignment_alignment_support_real_configuration_maintenance()
-> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (gitconfig-mode)
  (insert "[user]\n"
          "\tname=Alice Example\n"
          "\temail = alice@example.com\n"
          "[core]\n"
          "\tbare=true\n")
  (goto-char (point-min))
  (forward-line 1)
  (let ((start (point)))
    (forward-line 2)
    (comment-region start (point)))
  (let ((commented (buffer-string)))
    (goto-char (point-min))
    (forward-line 1)
    (let ((start (point)))
      (forward-line 2)
      (uncomment-region start (point)))
    (let ((uncommented (buffer-string)))
      (conf-align-assignments 18)
      (list :commented commented
            :uncommented uncommented
            :aligned (buffer-string)
            :comment-vars
            (list comment-start comment-start-skip comment-end)))))
"###;
    let expected = expect![[
        r##"OK (:commented "[user]\n\11# name=Alice Example\n\11# email = alice@example.com\n[core]\n\11bare=true\n" :uncommented "[user]\n\11name=Alice Example\n\11email = alice@example.com\n[core]\n\11bare=true\n" :aligned "[user]\n\11name\11  = Alice Example\n\11email\11  = alice@example.com\n[core]\n\11bare\11  = true\n" :comment-vars ("#" "#+\\s *" ""))"##
    ]];
    ParityBatchCase::value(
        "comment_round_trip_and_assignment_alignment_support_real_configuration_maintenance",
        elisp_form,
        expected,
    )
}

fn imenu_indexes_sections_and_parameters_for_navigation_to_live_lines() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (gitconfig-mode)
  (insert "[user]\n"
          "\tname = Alice\n"
          "\temail = alice@example.com\n"
          "[remote \"origin\"]\n"
          "\turl = ssh://git@example.com/team/app.git\n"
          "\tfetch = +refs/heads/*:refs/remotes/origin/*\n"
          "[core]\n"
          "\teditor = emacsclient\n")
  (list :expression imenu-generic-expression
        :index
        (mapcar #'neomacs-gitconfig-test-imenu-entry
                (imenu--generic-function imenu-generic-expression))))
"###;
    let expected = expect![[
        r#"OK (:expression (("Parameters" "^[ \11]*\\(.+?\\)[ \11]*=" 1) (nil "^[ \11]*\\[[ \11]*\\(.+\\)[ \11]*\\]" 1) (nil "^[ \11]*\\([^=:{} \11\n][^=:{}\n]+\\)[ \11\n]*{" 1)) :index (("Parameters" ("name" :line 2 :text "\11name = Alice") ("email" :line 3 :text "\11email = alice@example.com") ("url" :line 5 :text "\11url = ssh://git@example.com/team/app.git") ("fetch" :line 6 :text "\11fetch = +refs/heads/*:refs/remotes/origin/*") ("editor" :line 8 :text "\11editor = emacsclient")) ("user" :line 1 :text "[user]") ("remote \"origin\"" :line 4 :text "[remote \"origin\"]") ("core" :line 7 :text "[core]")))"#
    ]];
    ParityBatchCase::value(
        "imenu_indexes_sections_and_parameters_for_navigation_to_live_lines",
        elisp_form,
        expected,
    )
}

#[test]
fn gitconfig_mode_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(GIT_MODES_MELPA_PIN, "git-modes.el")
            .expect("prepare revision-pinned Git Modes source below ./tmp")
            .with_timeout(Duration::from_secs(240))
            .with_prelude(PRELUDE),
        "gitconfig-mode-package-batch",
        "Gitconfig Mode (from Git Modes)",
        &[
            successor_package_registers_the_gitconfig_mode_surface_and_parent_contract(),
            real_git_configuration_paths_select_gitconfig_mode_without_content_guessing(),
            practical_user_remote_core_and_feature_configuration_fontifies_semantic_fields(),
            syntax_distinguishes_both_comment_styles_subsection_strings_and_literal_apostrophes(),
            whole_file_indentation_canonicalizes_sections_settings_and_comments_idempotently(),
            indentation_policy_supports_space_only_projects_and_preserves_editing_position(),
            comment_round_trip_and_assignment_alignment_support_real_configuration_maintenance(),
            imenu_indexes_sections_and_parameters_for_navigation_to_live_lines(),
        ],
    );
}
