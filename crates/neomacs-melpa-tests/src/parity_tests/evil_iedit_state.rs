use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, EVIL_IEDIT_STATE_MELPA_PIN, EVIL_MELPA_PIN, IEDIT_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'evil-iedit-state)

(defmacro neomacs-evil-iedit-test-with-buffer (mode text needle &rest body)
  "Run BODY in a live Evil/Iedit buffer containing TEXT at NEEDLE."
  `(let ((buffer (generate-new-buffer " *evil-iedit-workflow*"))
         (this-command nil)
         (last-command nil)
         (kill-ring '("existing clipboard"))
         (kill-ring-yank-pointer nil)
         (iedit-auto-buffering nil)
         (iedit-auto-narrow nil)
         (iedit-auto-save-occurrence-in-kill-ring t)
         (iedit-case-sensitive t)
         (iedit-case-sensitive-default t))
     (setq kill-ring-yank-pointer kill-ring)
     (unwind-protect
         (progn
           (set-window-buffer (selected-window) buffer)
           (set-buffer buffer)
           (funcall ,mode)
           (buffer-enable-undo)
           (insert ,text)
           (goto-char (point-min))
           (when ,needle
             (search-forward ,needle)
             (goto-char (match-beginning 0)))
           (evil-local-mode 1)
           (evil-normal-state)
           ,@body)
       (when (buffer-live-p buffer)
         (with-current-buffer buffer
           (when (bound-and-true-p iedit-mode)
             (iedit-done))
           (when (bound-and-true-p evil-local-mode)
             (evil-local-mode -1)))
         (kill-buffer buffer)))))

(defun neomacs-evil-iedit-test-overlays ()
  "Return active Iedit occurrences in stable buffer order."
  (mapcar
   (lambda (overlay)
     (list (overlay-start overlay)
           (overlay-end overlay)
           (buffer-substring-no-properties
            (overlay-start overlay) (overlay-end overlay))))
   (sort (copy-sequence iedit-occurrences-overlays)
         (lambda (left right)
           (< (overlay-start left) (overlay-start right))))))

(defun neomacs-evil-iedit-test-state ()
  "Capture the user-visible editing and lifecycle state."
  (list
   :text (buffer-substring-no-properties (point-min) (point-max))
   :point (point)
   :line (line-number-at-pos)
   :column (current-column)
   :evil-state evil-state
   :iedit-mode (not (null iedit-mode))
   :overlays (neomacs-evil-iedit-test-overlays)
   :kill-ring kill-ring
   :post-command-hook
   (not (null (memq #'iedit-update-occurrences post-command-hook)))
   :change-major-mode-hook
   (not (null (memq #'iedit-done change-major-mode-hook)))))

(defun neomacs-evil-iedit-test-keys (&rest parts)
  "Execute PARTS as one real keyboard macro."
  (execute-kbd-macro (apply #'vconcat parts)))
"####;

fn append_suffix_propagates_through_real_iedit_insert_state() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-evil-iedit-test-with-buffer
 #'emacs-lisp-mode
 "(let ((artifact \"api\"))\n  (publish artifact)\n  (verify artifact))\n"
 "artifact"
 (evil-iedit-state/iedit-mode)
 (let ((selected (neomacs-evil-iedit-test-state)))
   (neomacs-evil-iedit-test-keys (kbd "A") "-v2")
   (let ((inserting (neomacs-evil-iedit-test-state)))
     (neomacs-evil-iedit-test-keys [escape])
     (let ((editing (neomacs-evil-iedit-test-state)))
       (neomacs-evil-iedit-test-keys [escape])
       (list :selected selected
             :inserting inserting
             :editing editing
             :finished (neomacs-evil-iedit-test-state)
             :remembered iedit-last-occurrence-local)))))
"####;
    let expected = expect![[
        r#"OK (:selected (:text "(let ((artifact \"api\"))\n  (publish artifact)\n  (verify artifact))\n" :point 8 :line 1 :column 7 :evil-state iedit :iedit-mode t :overlays ((8 16 "artifact") (36 44 "artifact") (56 64 "artifact")) :kill-ring #1=("existing clipboard") :post-command-hook t :change-major-mode-hook t) :inserting (:text "(let ((artifact-v2 \"api\"))\n  (publish artifact-v2)\n  (verify artifact-v2))\n" :point 19 :line 1 :column 18 :evil-state iedit-insert :iedit-mode t :overlays ((8 19 "artifact-v2") (39 50 "artifact-v2") (62 73 "artifact-v2")) :kill-ring #1# :post-command-hook t :change-major-mode-hook t) :editing (:text "(let ((artifact-v2 \"api\"))\n  (publish artifact-v2)\n  (verify artifact-v2))\n" :point 19 :line 1 :column 18 :evil-state iedit :iedit-mode t :overlays ((8 19 "artifact-v2") (39 50 "artifact-v2") (62 73 "artifact-v2")) :kill-ring #1# :post-command-hook t :change-major-mode-hook t) :finished (:text "(let ((artifact-v2 \"api\"))\n  (publish artifact-v2)\n  (verify artifact-v2))\n" :point 19 :line 1 :column 18 :evil-state normal :iedit-mode nil :overlays nil :kill-ring #1# :post-command-hook nil :change-major-mode-hook nil) :remembered "artifact-v2")"#
    ]];
    ParityBatchCase::value(
        "append_suffix_propagates_through_real_iedit_insert_state",
        elisp_form,
        expected,
    )
}

fn toggled_occurrence_is_excluded_from_real_uppercase_edit() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-evil-iedit-test-with-buffer
 #'text-mode
 "release primary\nrelease fallback\nrelease legacy\nrelease emergency\n"
 "release"
 (evil-iedit-state/iedit-mode)
 (neomacs-evil-iedit-test-keys (kbd "n TAB U"))
 (let ((edited (neomacs-evil-iedit-test-state)))
   (evil-iedit-state/quit-iedit-mode)
   (list :edited edited
         :finished (neomacs-evil-iedit-test-state))))
"####;
    let expected = expect![[
        r#"OK (:edited (:text "RELEASE primary\nrelease fallback\nRELEASE legacy\nRELEASE emergency\n" :point 17 :line 2 :column 0 :evil-state iedit :iedit-mode t :overlays ((1 8 "RELEASE") (34 41 "RELEASE") (49 56 "RELEASE")) :kill-ring #1=("existing clipboard") :post-command-hook t :change-major-mode-hook t) :finished (:text "RELEASE primary\nrelease fallback\nRELEASE legacy\nRELEASE emergency\n" :point 17 :line 2 :column 0 :evil-state normal :iedit-mode nil :overlays nil :kill-ring #1# :post-command-hook nil :change-major-mode-hook nil))"#
    ]];
    ParityBatchCase::value(
        "toggled_occurrence_is_excluded_from_real_uppercase_edit",
        elisp_form,
        expected,
    )
}

fn paste_replace_consumes_the_clipboard_without_polluting_it_on_exit() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-evil-iedit-test-with-buffer
 #'text-mode
 "deploy to staging\ndeploy to canary\ndeploy to production\n"
 "deploy"
 (setq kill-ring '("publish")
       kill-ring-yank-pointer kill-ring)
 (evil-iedit-state/iedit-mode)
 (neomacs-evil-iedit-test-keys (kbd "p"))
 (let ((replaced (neomacs-evil-iedit-test-state)))
   (evil-iedit-state/quit-iedit-mode)
   (list :replaced replaced
         :finished (neomacs-evil-iedit-test-state)
         :remembered iedit-last-occurrence-local)))
"####;
    let expected = expect![[
        r#"OK (:replaced (:text "publish to staging\npublish to canary\npublish to production\n" :point 7 :line 1 :column 6 :evil-state iedit :iedit-mode t :overlays ((1 8 "publish") (20 27 "publish") (38 45 "publish")) :kill-ring #1=("publish") :post-command-hook t :change-major-mode-hook t) :finished (:text "publish to staging\npublish to canary\npublish to production\n" :point 7 :line 1 :column 6 :evil-state normal :iedit-mode nil :overlays nil :kill-ring #1# :post-command-hook nil :change-major-mode-hook nil) :remembered "publish")"#
    ]];
    ParityBatchCase::value(
        "paste_replace_consumes_the_clipboard_without_polluting_it_on_exit",
        elisp_form,
        expected,
    )
}

fn line_restriction_and_numbering_update_only_the_active_workflow() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-evil-iedit-test-with-buffer
 #'text-mode
 "TODO lint TODO format\nTODO build TODO package\nTODO deploy TODO notify\n"
 "TODO build"
 (evil-iedit-state/iedit-mode)
 (neomacs-evil-iedit-test-keys (kbd "L #"))
 (let ((numbered (neomacs-evil-iedit-test-state)))
   (evil-iedit-state/quit-iedit-mode)
   (list :numbered numbered
         :finished (neomacs-evil-iedit-test-state))))
"####;
    let expected = expect![[
        r#"OK (:numbered (:text "TODO lint TODO format\n001TODO build 002TODO package\nTODO deploy TODO notify\n" :point 23 :line 2 :column 0 :evil-state iedit :iedit-mode t :overlays ((23 30 "001TODO") (37 44 "002TODO")) :kill-ring #1=("existing clipboard") :post-command-hook t :change-major-mode-hook t) :finished (:text "TODO lint TODO format\n001TODO build 002TODO package\nTODO deploy TODO notify\n" :point 23 :line 2 :column 0 :evil-state normal :iedit-mode nil :overlays nil :kill-ring #1# :post-command-hook nil :change-major-mode-hook nil))"#
    ]];
    ParityBatchCase::value(
        "line_restriction_and_numbering_update_only_the_active_workflow",
        elisp_form,
        expected,
    )
}

fn substitute_and_first_undo_apply_to_every_occurrence() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-evil-iedit-test-with-buffer
 #'text-mode
 "draft report\ndraft invoice\ndraft announcement\n"
 "draft"
 (evil-iedit-state/iedit-mode)
 (setq buffer-undo-list nil)
 (neomacs-evil-iedit-test-keys (kbd "S") "final" [escape])
 (let ((edited (neomacs-evil-iedit-test-state)))
   (neomacs-evil-iedit-test-keys [escape] (kbd "u"))
   (list :edited edited
         :insertion-undone (neomacs-evil-iedit-test-state))))
"####;
    let expected = expect![[
        r#"OK (:edited (:text "final report\nfinal invoice\nfinal announcement\n" :point 6 :line 1 :column 5 :evil-state iedit :iedit-mode t :overlays ((1 6 "final") (14 19 "final") (28 33 "final")) :kill-ring #1=("existing clipboard") :post-command-hook t :change-major-mode-hook t) :insertion-undone (:text " report\n invoice\n announcement\n" :point 1 :line 1 :column 0 :evil-state normal :iedit-mode nil :overlays nil :kill-ring #1# :post-command-hook nil :change-major-mode-hook nil))"#
    ]];
    ParityBatchCase::value(
        "substitute_and_first_undo_apply_to_every_occurrence",
        elisp_form,
        expected,
    )
}

fn evil_iedit_state_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(EVIL_IEDIT_STATE_MELPA_PIN, "evil-iedit-state.el")
        .expect("prepare pinned Evil Iedit State source below ./tmp")
        .with_melpa_dependency(EVIL_MELPA_PIN)
        .expect("prepare pinned Evil dependency")
        .with_melpa_dependency(IEDIT_MELPA_PIN)
        .expect("prepare pinned Iedit dependency")
        .with_timeout(Duration::from_secs(240))
        .with_prelude(PRELUDE)
}

#[test]
fn evil_iedit_state_practical_workflows_batch() {
    let cases = vec![
        append_suffix_propagates_through_real_iedit_insert_state(),
        toggled_occurrence_is_excluded_from_real_uppercase_edit(),
        paste_replace_consumes_the_clipboard_without_polluting_it_on_exit(),
        line_restriction_and_numbering_update_only_the_active_workflow(),
        substitute_and_first_undo_apply_to_every_occurrence(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("evil-iedit-state parity batch");
    assert_oracle_batch_cases(
        evil_iedit_state_oracle(),
        test_name,
        "evil-iedit-state parity",
        &cases,
    );
}
