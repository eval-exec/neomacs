use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, EVIL_MELPA_PIN, EVIL_TUTOR_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'evil-tutor)

(defun neomacs-evil-tutor-test-line-state ()
  "Return the current line, its number, and point."
  (list :line (buffer-substring-no-properties
               (line-beginning-position) (line-end-position))
        :line-number (line-number-at-pos)
        :point (point)))

(defun neomacs-evil-tutor-test-session-state ()
  "Return stable state for the current tutorial session."
  (list :file (file-name-nondirectory buffer-file-name)
        :exists (file-exists-p buffer-file-name)
        :size (buffer-size)
        :first-line
        (buffer-substring-no-properties
         (point-min)
         (save-excursion (goto-char (point-min)) (line-end-position)))
        :lesson-markers
        (save-excursion
          (goto-char (point-min))
          (let ((count 0))
            (while (re-search-forward "^~.*~$" nil t)
              (setq count (1+ count)))
            count))
        :major-mode major-mode
        :mode-name mode-name
        :evil-local (and (boundp 'evil-local-mode) evil-local-mode)
        :evil-state (and (boundp 'evil-state) evil-state)
        :literal (and (boundp 'find-file-literally) find-file-literally)
        :multibyte enable-multibyte-characters))
"####;

fn a_tutorial_session_is_created_edited_saved_and_resumed_from_disk() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root (make-temp-file "neomacs-evil-tutor-session-" t))
       (evil-tutor-working-directory (file-name-as-directory root))
       first-state
       resumed-state
       saved-file
       files-after-create
       progress-preserved)
  (unwind-protect
      (save-window-excursion
        (cl-letf (((symbol-function 'format-time-string)
                   (lambda (&rest _arguments) "05082026")))
          (evil-tutor-start))
        (setq first-state (neomacs-evil-tutor-test-session-state))
        (setq saved-file buffer-file-name)
        (setq files-after-create
              (mapcar #'file-name-nondirectory
                      (directory-files root t "\\.txt\\'")))
        (goto-char (point-max))
        (insert "\nSESSION PROGRESS: practiced movement and deletion.\n")
        (save-buffer)
        (kill-buffer (current-buffer))
        (evil-tutor-resume)
        (setq resumed-state (neomacs-evil-tutor-test-session-state))
        (setq progress-preserved
              (save-excursion
                (goto-char (point-min))
                (re-search-forward
                 "^SESSION PROGRESS: practiced movement and deletion\\.$"
                 nil t)))
        (let ((result
               (list :first first-state
                     :files files-after-create
                     :resumed resumed-state
                     :same-file (equal saved-file buffer-file-name)
                     :progress-preserved (not (null progress-preserved))
                     :global-evil-mode evil-mode)))
          (kill-buffer (current-buffer))
          result))
    (dolist (buffer (buffer-list))
      (when (and (buffer-file-name buffer)
                 (file-in-directory-p (buffer-file-name buffer) root))
        (kill-buffer buffer)))
    (delete-directory root t)))
"####;
    let expected = expect![[
        r#"OK (:first (:file "evil-tutor-05082026.txt" :exists t :size 25352 :first-line "===============================================================================" :lesson-markers 32 :major-mode evil-tutor-mode :mode-name "evil-tutor" :evil-local t :evil-state normal :literal nil :multibyte t) :files ("evil-tutor-05082026.txt") :resumed (:file "evil-tutor-05082026.txt" :exists t :size 25404 :first-line "===============================================================================" :lesson-markers 32 :major-mode evil-tutor-mode :mode-name "evil-tutor" :evil-local t :evil-state normal :literal t :multibyte nil) :same-file t :progress-preserved t :global-evil-mode t)"#
    ]];
    ParityBatchCase::value(
        "a_tutorial_session_is_created_edited_saved_and_resumed_from_disk",
        elisp_form,
        expected,
    )
    .fresh_process()
}

fn lesson_navigation_moves_by_markers_in_both_directions() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (save-window-excursion
    (switch-to-buffer (current-buffer))
    (evil-tutor-mode)
    (insert
     "Preface\n~ lesson one ~\nAlpha exercise\nalpha detail\n~ lesson two ~\nBeta exercise\nbeta detail\n~ lesson three ~\nGamma exercise\ngamma detail\n")
    (goto-char (point-min))
    (evil-tutor-goto-next-lesson)
    (let ((first (neomacs-evil-tutor-test-line-state)))
      (evil-tutor-goto-next-lesson 2)
      (let ((third (neomacs-evil-tutor-test-line-state)))
        (evil-tutor-goto-previous-lesson)
        (let ((second (neomacs-evil-tutor-test-line-state)))
          (evil-tutor-goto-previous-lesson)
          (list :first first
                :third third
                :second second
                :back-to-first (neomacs-evil-tutor-test-line-state)))))))
"####;
    let expected = expect![[
        r#"OK (:first (:line "Alpha exercise" :line-number 3 :point 24) :third (:line "Gamma exercise" :line-number 9 :point 110) :second (:line "Beta exercise" :line-number 6 :point 67) :back-to-first (:line "Alpha exercise" :line-number 3 :point 24))"#
    ]];
    ParityBatchCase::value(
        "lesson_navigation_moves_by_markers_in_both_directions",
        elisp_form,
        expected,
    )
}

fn tutor_mode_inherits_text_editing_and_installs_only_navigation_overrides() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (evil-tutor-mode)
  (list :major-mode major-mode
        :mode-name mode-name
        :derived-text (derived-mode-p 'text-mode)
        :parent-is-text (eq (keymap-parent evil-tutor-mode-map)
                            text-mode-map)
        :next-binding (lookup-key evil-tutor-mode-map (kbd "C-j"))
        :previous-binding (lookup-key evil-tutor-mode-map (kbd "C-k"))
        :resume-is-start
        (eq (indirect-function 'evil-tutor-resume)
            (indirect-function 'evil-tutor-start))
        :commands
        (list (commandp 'evil-tutor-resume)
              (commandp 'evil-tutor-start))))
"####;
    let expected = expect![[
        r#"OK (:major-mode evil-tutor-mode :mode-name "evil-tutor" :derived-text text-mode :parent-is-text t :next-binding evil-tutor-goto-next-lesson :previous-binding evil-tutor-goto-previous-lesson :resume-is-start t :commands (t t))"#
    ]];
    ParityBatchCase::value(
        "tutor_mode_inherits_text_editing_and_installs_only_navigation_overrides",
        elisp_form,
        expected,
    )
}

fn working_file_selection_uses_the_first_txt_entry_and_rejects_other_extensions() -> ParityBatchCase
{
    let elisp_form = r####"
(mapcar
 (lambda (scenario)
   (let ((selected (evil-tutor--find-first-working-file (cdr scenario))))
     (list (car scenario)
           (and selected (file-name-nondirectory selected)))))
 '((nil-input)
   (no-txt "/project/." "/project/.." "/project/notes.org")
   (first-txt "/project/notes.org" "/project/evil-tutor-old.txt"
              "/project/evil-tutor-new.txt")
   (skips-uppercase "/project/README.TXT" "/project/session.txt")
   (period-extension "/project/archive.txt.bak" "/project/progress.txt")))
"####;
    let expected = expect![[
        r#"OK ((nil-input nil) (no-txt nil) (first-txt "evil-tutor-old.txt") (skips-uppercase "session.txt") (period-extension "progress.txt"))"#
    ]];
    ParityBatchCase::value(
        "working_file_selection_uses_the_first_txt_entry_and_rejects_other_extensions",
        elisp_form,
        expected,
    )
}

fn evil_tutor_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(EVIL_TUTOR_MELPA_PIN, "evil-tutor.el")
        .expect("prepare pinned Evil-Tutor source below ./tmp")
        .with_melpa_dependency(EVIL_MELPA_PIN)
        .expect("prepare pinned Evil dependency below ./tmp")
        .with_timeout(Duration::from_secs(240))
        .with_prelude(PRELUDE)
}

#[test]
fn evil_tutor_practical_workflows_batch() {
    let cases = vec![
        a_tutorial_session_is_created_edited_saved_and_resumed_from_disk(),
        lesson_navigation_moves_by_markers_in_both_directions(),
        tutor_mode_inherits_text_editing_and_installs_only_navigation_overrides(),
        working_file_selection_uses_the_first_txt_entry_and_rejects_other_extensions(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("evil-tutor parity batch");
    assert_oracle_batch_cases(evil_tutor_oracle(), test_name, "evil-tutor parity", &cases);
}
