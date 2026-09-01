use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, EVIL_ESCAPE_MELPA_PIN, EVIL_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'evil-escape)

(defun neomacs-evil-escape-test-with-buffer (mode text function)
  "Run FUNCTION in a live displayed Evil buffer using MODE and TEXT."
  (let ((buffer (generate-new-buffer " *evil-escape-workflow*"))
        (saved-pre-command-hook (default-value 'pre-command-hook))
        (was-enabled evil-escape-mode)
        (this-command nil)
        (last-command nil)
        (unread-command-events nil)
        (unread-post-input-method-events nil))
    (unwind-protect
        (progn
          (set-window-buffer (selected-window) buffer)
          (set-buffer buffer)
          (funcall mode)
          (insert text)
          (goto-char (point-min))
          (set-buffer-modified-p nil)
          (evil-local-mode 1)
          (evil-normal-state)
          (evil-escape-mode 1)
          (funcall function))
      (unless was-enabled
        (evil-escape-mode -1))
      (set-default 'pre-command-hook saved-pre-command-hook)
      (when (buffer-live-p buffer)
        (with-current-buffer buffer
          (when (bound-and-true-p evil-local-mode)
            (evil-local-mode -1)))
        (kill-buffer buffer)))))

(defun neomacs-evil-escape-test-keys (&rest parts)
  "Execute PARTS as one real keyboard macro."
  (execute-kbd-macro (apply #'vconcat parts)))

(defun neomacs-evil-escape-test-state ()
  "Capture visible editing, input queue, and mode lifecycle state."
  (list
   :text (buffer-substring-no-properties (point-min) (point-max))
   :point (point)
   :line (line-number-at-pos)
   :column (current-column)
   :evil-state evil-state
   :modified (buffer-modified-p)
   :escape-mode evil-escape-mode
   :hook-installed
   (not (null (memq #'evil-escape-pre-command-hook
                    (default-value 'pre-command-hook))))
   :unread-command-events unread-command-events
   :unread-post-input-method-events unread-post-input-method-events))
"####;

fn insert_chord_exits_immediately_without_typing_the_chord() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-evil-escape-test-with-buffer
 #'text-mode ""
 (lambda ()
   (let ((evil-escape-key-sequence "jk")
         (evil-escape-delay 0.1))
     (neomacs-evil-escape-test-keys (kbd "i") "deploy now" "jk")
     (neomacs-evil-escape-test-state))))
"####;
    let expected = expect![[
        r#"OK (:text "deploy now" :point 10 :line 1 :column 9 :evil-state normal :modified t :escape-mode t :hook-installed t :unread-command-events nil :unread-post-input-method-events nil)"#
    ]];
    ParityBatchCase::value(
        "insert_chord_exits_immediately_without_typing_the_chord",
        elisp_form,
        expected,
    )
}

fn timeout_and_mismatch_keep_all_typed_characters_in_order() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-evil-escape-test-with-buffer
 #'text-mode ""
 (lambda ()
   (let ((evil-escape-key-sequence "jk")
         (evil-escape-delay 0.001))
     (evil-insert-state)
     (neomacs-evil-escape-test-keys "j")
     (let ((after-timeout (neomacs-evil-escape-test-state)))
       (neomacs-evil-escape-test-keys "jz")
       (list :after-timeout after-timeout
             :after-mismatch (neomacs-evil-escape-test-state))))))
"####;
    let expected = expect![[
        r#"OK (:after-timeout (:text "j" :point 2 :line 1 :column 1 :evil-state insert :modified t :escape-mode t :hook-installed t :unread-command-events nil :unread-post-input-method-events nil) :after-mismatch (:text "jjz" :point 4 :line 1 :column 3 :evil-state insert :modified t :escape-mode t :hook-installed t :unread-command-events nil :unread-post-input-method-events nil))"#
    ]];
    ParityBatchCase::value(
        "timeout_and_mismatch_keep_all_typed_characters_in_order",
        elisp_form,
        expected,
    )
}

fn unordered_case_insensitive_reverse_chord_exits_insert_state() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-evil-escape-test-with-buffer
 #'text-mode "release "
 (lambda ()
   (let ((evil-escape-key-sequence "fd")
         (evil-escape-delay 0.1)
         (evil-escape-unordered-key-sequence t)
         (evil-escape-case-insensitive-key-sequence t))
     (goto-char (point-max))
     (evil-insert-state)
     (neomacs-evil-escape-test-keys "DF")
     (neomacs-evil-escape-test-state))))
"####;
    let expected = expect![[
        r#"OK (:text "release " :point 8 :line 1 :column 7 :evil-state normal :modified nil :escape-mode t :hook-installed t :unread-command-events nil :unread-post-input-method-events nil)"#
    ]];
    ParityBatchCase::value(
        "unordered_case_insensitive_reverse_chord_exits_insert_state",
        elisp_form,
        expected,
    )
}

fn exclusions_and_predicate_inhibition_make_chords_literal_until_enabled() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-evil-escape-test-with-buffer
 #'text-mode ""
 (lambda ()
   (let ((evil-escape-key-sequence "jk")
         (evil-escape-delay 0.1)
         (evil-escape-excluded-major-modes '(text-mode)))
     (evil-insert-state)
     (neomacs-evil-escape-test-keys "jk")
     (let ((excluded (neomacs-evil-escape-test-state)))
       (setq evil-escape-excluded-major-modes nil
             evil-escape-inhibit-functions (list (lambda () t)))
       (neomacs-evil-escape-test-keys "jk")
       (let ((inhibited (neomacs-evil-escape-test-state)))
         (setq evil-escape-inhibit-functions nil)
         (neomacs-evil-escape-test-keys "jk")
         (list :excluded excluded
               :inhibited inhibited
               :enabled (neomacs-evil-escape-test-state)))))))
"####;
    let expected = expect![[
        r#"OK (:excluded (:text "jk" :point 3 :line 1 :column 2 :evil-state insert :modified t :escape-mode t :hook-installed t :unread-command-events nil :unread-post-input-method-events nil) :inhibited (:text "jkjk" :point 5 :line 1 :column 4 :evil-state insert :modified t :escape-mode t :hook-installed t :unread-command-events nil :unread-post-input-method-events nil) :enabled (:text "jkjk" :point 4 :line 1 :column 3 :evil-state normal :modified t :escape-mode t :hook-installed t :unread-command-events nil :unread-post-input-method-events nil))"#
    ]];
    ParityBatchCase::value(
        "exclusions_and_predicate_inhibition_make_chords_literal_until_enabled",
        elisp_form,
        expected,
    )
}

fn one_chord_exits_visual_replace_and_emacs_states_without_editing() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-evil-escape-test-with-buffer
 #'text-mode "alpha beta\n"
 (lambda ()
   (let ((evil-escape-key-sequence "jk")
         (evil-escape-delay 0.1)
         visual replace emacs)
     (goto-char (point-min))
     (set-mark (point))
     (forward-word 1)
     (evil-visual-state)
     (neomacs-evil-escape-test-keys "jk")
     (setq visual (neomacs-evil-escape-test-state))
     (evil-replace-state)
     (neomacs-evil-escape-test-keys "jk")
     (setq replace (neomacs-evil-escape-test-state))
     (evil-emacs-state)
     (neomacs-evil-escape-test-keys "jk")
     (setq emacs (neomacs-evil-escape-test-state))
     (list :visual visual :replace replace :emacs emacs))))
"####;
    let expected = expect![[
        r#"OK (:visual (:text "alpha beta\n" :point 6 :line 1 :column 5 :evil-state normal :modified nil :escape-mode t :hook-installed t :unread-command-events nil :unread-post-input-method-events nil) :replace (:text "alpha beta\n" :point 5 :line 1 :column 4 :evil-state normal :modified nil :escape-mode t :hook-installed t :unread-command-events nil :unread-post-input-method-events nil) :emacs (:text "alpha beta\n" :point 5 :line 1 :column 4 :evil-state normal :modified nil :escape-mode t :hook-installed t :unread-command-events nil :unread-post-input-method-events nil))"#
    ]];
    ParityBatchCase::value(
        "one_chord_exits_visual_replace_and_emacs_states_without_editing",
        elisp_form,
        expected,
    )
}

fn temporary_probe_does_not_dirty_an_unmodified_buffer_and_mode_cleans_hook() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-evil-escape-test-with-buffer
 #'text-mode "stable\n"
 (lambda ()
   (let ((evil-escape-key-sequence "jk")
         (evil-escape-delay 0.1))
     (goto-char (point-max))
     (evil-insert-state)
     (neomacs-evil-escape-test-keys "jk")
     (let ((escaped (neomacs-evil-escape-test-state)))
       (evil-escape-mode -1)
       (list :escaped escaped
             :disabled (neomacs-evil-escape-test-state))))))
"####;
    let expected = expect![[
        r#"OK (:escaped (:text "stable\n" :point 8 :line 2 :column 0 :evil-state normal :modified nil :escape-mode t :hook-installed t :unread-command-events nil :unread-post-input-method-events nil) :disabled (:text "stable\n" :point 8 :line 2 :column 0 :evil-state normal :modified nil :escape-mode nil :hook-installed nil :unread-command-events nil :unread-post-input-method-events nil))"#
    ]];
    ParityBatchCase::value(
        "temporary_probe_does_not_dirty_an_unmodified_buffer_and_mode_cleans_hook",
        elisp_form,
        expected,
    )
}

fn evil_escape_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(EVIL_ESCAPE_MELPA_PIN, "evil-escape.el")
        .expect("prepare pinned Evil Escape source below ./tmp")
        .with_melpa_dependency(EVIL_MELPA_PIN)
        .expect("prepare pinned Evil dependency")
        .with_timeout(Duration::from_secs(240))
        .with_prelude(PRELUDE)
}

#[test]
fn evil_escape_practical_workflows_batch() {
    let cases = vec![
        insert_chord_exits_immediately_without_typing_the_chord(),
        timeout_and_mismatch_keep_all_typed_characters_in_order(),
        unordered_case_insensitive_reverse_chord_exits_insert_state(),
        exclusions_and_predicate_inhibition_make_chords_literal_until_enabled(),
        one_chord_exits_visual_replace_and_emacs_states_without_editing(),
        temporary_probe_does_not_dirty_an_unmodified_buffer_and_mode_cleans_hook(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("evil-escape parity batch");
    assert_oracle_batch_cases(
        evil_escape_oracle(),
        test_name,
        "evil-escape parity",
        &cases,
    );
}
