use std::time::Duration;

use expect_test::expect;

use crate::{
    COMPAT_GNU_ELPA_PIN, CachedMelpaOracle, JS2_MODE_MELPA_PIN, LIVID_MODE_MELPA_PIN, S_MELPA_PIN,
    SIMPLE_HTTPD_MELPA_PIN, SKEWER_MODE_MELPA_PIN,
};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'js2-mode)
(require 'livid-mode)

(defun livid369-test-activate-javascript ()
  (js-mode))

(defun livid369-test-mode-state ()
  (list :mode livid-mode
        :lighter (copy-tree (assq 'livid-mode minor-mode-alist))
        :key (lookup-key livid-mode-map (kbd "C-c C-p"))
        :hook-count (cl-count 'livid-tick after-change-functions :test #'eq)
        :locals
        (mapcar (lambda (symbol) (list symbol (local-variable-p symbol)))
                '(livid-timer livid-last-seen livid-paused))
        :last-seen (copy-sequence livid-last-seen)
        :paused livid-paused
        :skewer skewer-mode))

(defun livid369-test-buffer-state ()
  (list :text (buffer-substring-no-properties (point-min) (point-max))
        :point (point)
        :mark (mark t)
        :mark-active mark-active
        :modified (buffer-modified-p)
        :mode (livid369-test-mode-state)))

(defun livid369-test-file-bytes (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (buffer-string)))

(defun livid369-test-last-message ()
  (with-current-buffer (messages-buffer)
    (save-excursion
      (goto-char (point-max))
      (skip-chars-backward "\n")
      (buffer-substring-no-properties
       (line-beginning-position) (line-end-position)))))

(defun livid369-test-delete-tree (root)
  (when (and (stringp root)
             (file-name-absolute-p root)
             (file-directory-p root)
             (not (file-symlink-p root)))
    (delete-directory root t)))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(LIVID_MODE_MELPA_PIN, "livid-mode.el")
        .expect("prepare pinned livid-mode source below ./tmp")
        .with_melpa_dependency(SKEWER_MODE_MELPA_PIN)
        .expect("prepare pinned skewer-mode dependency below ./tmp")
        .with_melpa_dependency(S_MELPA_PIN)
        .expect("prepare pinned s dependency below ./tmp")
        .with_melpa_dependency(JS2_MODE_MELPA_PIN)
        .expect("prepare pinned js2-mode dependency below ./tmp")
        .with_melpa_dependency(SIMPLE_HTTPD_MELPA_PIN)
        .expect("prepare pinned simple-httpd dependency below ./tmp")
        .with_gnu_elpa_dependency(COMPAT_GNU_ELPA_PIN)
        .expect("prepare pinned Compat dependency below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn activation_runs_the_initial_trimmed_evaluation_and_owns_local_mode_state() -> ParityBatchCase {
    let form = r####"
(with-temp-buffer
  (insert " \nconst release = { id: 'REL-417', total: 49.95 };\n \t")
  (livid369-test-activate-javascript)
  (set-buffer-modified-p nil)
  (goto-char 12)
  (set-mark 3)
  (setq mark-active t)
  (let ((livid-validate-javascript nil)
        evaluations)
    (cl-letf (((symbol-function 'skewer-eval)
               (lambda (code &optional callback &rest arguments)
                 (push (list (copy-sequence code) callback (copy-tree arguments))
                       evaluations)
                 '((id . "livid-initial")))))
      (let ((before (livid369-test-buffer-state)))
        (livid-mode 1)
        (let ((enabled (livid369-test-buffer-state)))
          (livid-mode -1)
          (list :before before
                :enabled enabled
                :evaluations (nreverse evaluations)
                :disabled (livid369-test-buffer-state)))))))
"####;
    ParityBatchCase::value(
        "activation_runs_the_initial_trimmed_evaluation_and_owns_local_mode_state",
        form,
        expect![[
            r#"OK (:before (:text " \nconst release = { id: 'REL-417', total: 49.95 };\n \11" :point 12 :mark 3 :mark-active t :modified nil :mode (:mode nil :lighter (livid-mode " livid") :key livid-toggle-pause :hook-count 0 :locals ((livid-timer nil) (livid-last-seen nil) (livid-paused nil)) :last-seen "" :paused nil :skewer nil)) :enabled (:text " \nconst release = { id: 'REL-417', total: 49.95 };\n \11" :point 12 :mark 3 :mark-active t :modified nil :mode (:mode t :lighter (livid-mode " livid") :key livid-toggle-pause :hook-count 1 :locals ((livid-timer t) (livid-last-seen t) (livid-paused t)) :last-seen "const release = { id: 'REL-417', total: 49.95 };" :paused nil :skewer nil)) :evaluations (("const release = { id: 'REL-417', total: 49.95 };" nil nil)) :disabled (:text " \nconst release = { id: 'REL-417', total: 49.95 };\n \11" :point 12 :mark 3 :mark-active t :modified nil :mode (:mode nil :lighter (livid-mode " livid") :key livid-toggle-pause :hook-count 0 :locals ((livid-timer t) (livid-last-seen t) (livid-paused t)) :last-seen "" :paused nil :skewer nil)))"#
        ]],
    )
}

fn real_edits_trim_dedupe_and_send_only_semantically_changed_programs() -> ParityBatchCase {
    let form = r####"
(with-temp-buffer
  (insert "const cart = { subtotal: 45, tax: 4.95 };\n")
  (livid369-test-activate-javascript)
  (let ((livid-validate-javascript nil)
        evaluations phases)
    (cl-letf (((symbol-function 'skewer-eval)
               (lambda (code &optional callback &rest arguments)
                 (push (list (copy-sequence code) callback (copy-tree arguments))
                       evaluations)
                 '((id . "livid-edit")))))
      (livid-mode 1)
      (push (list :initial (copy-tree (nreverse (copy-tree evaluations)))
                  :last (copy-sequence livid-last-seen))
            phases)
      (goto-char (point-max))
      (insert " \t\n")
      (push (list :edge-whitespace (length evaluations)
                  :last (copy-sequence livid-last-seen))
            phases)
      (delete-region (- (point-max) 3) (point-max))
      (push (list :remove-whitespace (length evaluations)
                  :last (copy-sequence livid-last-seen))
            phases)
      (goto-char (point-max))
      (insert "const total = cart.subtotal + cart.tax;\n")
      (push (list :semantic-edit (length evaluations)
                  :last (copy-sequence livid-last-seen))
            phases)
      (livid-tick nil nil nil)
      (push (list :repeat-tick (length evaluations)
                  :last (copy-sequence livid-last-seen))
            phases)
      (livid-mode -1)
      (list :phases (nreverse phases)
            :evaluations (nreverse evaluations)
            :final (livid369-test-buffer-state)))))
"####;
    ParityBatchCase::value(
        "real_edits_trim_dedupe_and_send_only_semantically_changed_programs",
        form,
        expect![[
            r#"OK (:phases ((:initial (("const cart = { subtotal: 45, tax: 4.95 };" nil nil)) :last "const cart = { subtotal: 45, tax: 4.95 };") (:edge-whitespace 1 :last "const cart = { subtotal: 45, tax: 4.95 };") (:remove-whitespace 1 :last "const cart = { subtotal: 45, tax: 4.95 };") (:semantic-edit 2 :last "const cart = { subtotal: 45, tax: 4.95 };\nconst total = cart.subtotal + cart.tax;") (:repeat-tick 2 :last "const cart = { subtotal: 45, tax: 4.95 };\nconst total = cart.subtotal + cart.tax;")) :evaluations (("const cart = { subtotal: 45, tax: 4.95 };" nil nil) ("const cart = { subtotal: 45, tax: 4.95 };\nconst total = cart.subtotal + cart.tax;" nil nil)) :final (:text "const cart = { subtotal: 45, tax: 4.95 };\nconst total = cart.subtotal + cart.tax;\n" :point 83 :mark nil :mark-active nil :modified t :mode (:mode nil :lighter (livid-mode " livid") :key livid-toggle-pause :hook-count 0 :locals ((livid-timer t) (livid-last-seen t) (livid-paused t)) :last-seen "" :paused nil :skewer nil)))"#
        ]],
    )
}

fn owned_validator_rejects_syntax_error_and_retries_corrected_shell_quoted_code() -> ParityBatchCase
{
    let form = r####"
(let* ((root (make-temp-file "livid369-validator-" t))
       (script (expand-file-name "validator" root))
       (count-file (expand-file-name "count" root))
       (argument-file (expand-file-name "argument" root))
       evaluations result)
  (unwind-protect
      (progn
        (with-temp-file script
          (insert "#!/bin/sh\n"
                  "set -eu\n"
                  "printf '%s' \"$#\" >\"$LIVID369_COUNT\"\n"
                  "printf '%s' \"$1\" >\"$LIVID369_ARGUMENT\"\n"
                  "case \"$1\" in\n"
                  "  *BROKEN*) printf '%s\\n' 'SyntaxError: expected expression' ;;\n"
                  "  *) printf '%s\\n' 'validator-ok' ;;\n"
                  "esac\n"))
        (set-file-modes script #o700)
        (unless (and (file-regular-p script) (not (file-symlink-p script)))
          (error "Livid validator fixture is not direct and regular"))
        (with-temp-buffer
          (insert "const quote = \"O'Reilly\";\n"
                  "const literal = \"$HOME $(touch forbidden)\";\n"
                  "const state = BROKEN;\n")
          (livid369-test-activate-javascript)
          (let ((livid-validate-javascript t)
                (livid-validate-javascript-command (shell-quote-argument script))
                (shell-file-name "/bin/sh")
                (process-environment
                 (list "LC_ALL=C"
                       (concat "LIVID369_COUNT=" count-file)
                       (concat "LIVID369_ARGUMENT=" argument-file))))
            (cl-letf (((symbol-function 'skewer-eval)
                       (lambda (code &optional callback &rest arguments)
                         (push (list (copy-sequence code) callback
                                     (copy-tree arguments))
                               evaluations)
                         '((id . "validated")))))
              (livid-mode 1)
              (let ((rejected
                     (list :evaluations (length evaluations)
                           :last (copy-sequence livid-last-seen)
                           :argc (livid369-test-file-bytes count-file)
                           :argument (livid369-test-file-bytes argument-file)
                           :forbidden (file-exists-p
                                       (expand-file-name "forbidden"
                                                         default-directory)))))
                (goto-char (point-min))
                (search-forward "BROKEN")
                (replace-match "ready" t t)
                (setq result
                      (list :rejected rejected
                            :recovered
                            (list :evaluations (nreverse evaluations)
                                  :last (copy-sequence livid-last-seen)
                                  :argc (livid369-test-file-bytes count-file)
                                  :argument (livid369-test-file-bytes argument-file)
                                  :forbidden (file-exists-p
                                              (expand-file-name
                                               "forbidden" default-directory)))
                            :buffer (livid369-test-buffer-state))))))))
    (livid369-test-delete-tree root))
  result)
"####;
    ParityBatchCase::value(
        "owned_validator_rejects_syntax_error_and_retries_corrected_shell_quoted_code",
        form,
        expect![[
            r#"OK (:rejected (:evaluations 0 :last "" :argc "1" :argument "const quote = \"O'Reilly\";\nconst literal = \"$HOME $(touch forbidden)\";\nconst state = BROKEN;" :forbidden nil) :recovered (:evaluations (("const quote = \"O'Reilly\";\nconst literal = \"$HOME $(touch forbidden)\";\nconst state = ready;" nil nil)) :last "const quote = \"O'Reilly\";\nconst literal = \"$HOME $(touch forbidden)\";\nconst state = ready;" :argc "1" :argument "const quote = \"O'Reilly\";\nconst literal = \"$HOME $(touch forbidden)\";\nconst state = ready;" :forbidden nil) :buffer (:text "const quote = \"O'Reilly\";\nconst literal = \"$HOME $(touch forbidden)\";\nconst state = ready;\n" :point 90 :mark nil :mark-active nil :modified t :mode (:mode t :lighter (livid-mode " livid") :key livid-toggle-pause :hook-count 1 :locals ((livid-timer t) (livid-last-seen t) (livid-paused t)) :last-seen "const quote = \"O'Reilly\";\nconst literal = \"$HOME $(touch forbidden)\";\nconst state = ready;" :paused nil :skewer nil)))"#
        ]],
    )
}

fn pause_key_suppresses_edits_and_unpause_evaluates_the_latest_program_once() -> ParityBatchCase {
    let form = r####"
(with-temp-buffer
  (insert "let release = 1;\n")
  (livid369-test-activate-javascript)
  (let ((livid-validate-javascript nil)
        evaluations phases)
    (cl-letf (((symbol-function 'skewer-eval)
               (lambda (code &optional callback &rest arguments)
                 (push (list (copy-sequence code) callback (copy-tree arguments))
                       evaluations)
                 '((id . "livid-pause")))))
      (livid-mode 1)
      (call-interactively (key-binding (kbd "C-c C-p")))
      (push (list :pause-message (livid369-test-last-message)
                  :paused livid-paused :count (length evaluations))
            phases)
      (goto-char (point-max))
      (insert "release += 2;\n")
      (insert "release *= 3;\n")
      (push (list :while-paused livid-paused :count (length evaluations)
                  :last (copy-sequence livid-last-seen))
            phases)
      (call-interactively (key-binding (kbd "C-c C-p")))
      (push (list :unpause-message (livid369-test-last-message)
                  :paused livid-paused :count (length evaluations)
                  :last (copy-sequence livid-last-seen))
            phases)
      (goto-char (point-max))
      (insert "release -= 4;\n")
      (push (list :after-unpause livid-paused :count (length evaluations)
                  :last (copy-sequence livid-last-seen))
            phases)
      (livid-mode -1)
      (list :phases (nreverse phases)
            :evaluations (nreverse evaluations)
            :final (livid369-test-buffer-state)))))
"####;
    ParityBatchCase::value(
        "pause_key_suppresses_edits_and_unpause_evaluates_the_latest_program_once",
        form,
        expect![[
            r#"OK (:phases ((:pause-message "Paused livid-mode" :paused t :count 1) (:while-paused t :count 1 :last "let release = 1;") (:unpause-message "Unpaused livid-mode" :paused nil :count 2 :last "let release = 1;\nrelease += 2;\nrelease *= 3;") (:after-unpause nil :count 3 :last "let release = 1;\nrelease += 2;\nrelease *= 3;\nrelease -= 4;")) :evaluations (("let release = 1;" nil nil) ("let release = 1;\nrelease += 2;\nrelease *= 3;" nil nil) ("let release = 1;\nrelease += 2;\nrelease *= 3;\nrelease -= 4;" nil nil)) :final (:text "let release = 1;\nrelease += 2;\nrelease *= 3;\nrelease -= 4;\n" :point 60 :mark nil :mark-active nil :modified t :mode (:mode nil :lighter (livid-mode " livid") :key livid-toggle-pause :hook-count 0 :locals ((livid-timer t) (livid-last-seen t) (livid-paused t)) :last-seen "" :paused nil :skewer nil)))"#
        ]],
    )
}

fn disable_reenable_and_two_buffers_preserve_exact_local_state_and_isolation() -> ParityBatchCase {
    let form = r####"
(let ((first (generate-new-buffer " *livid369-first*"))
      (second (generate-new-buffer " *livid369-second*"))
      evaluations result)
  (unwind-protect
      (let ((livid-validate-javascript nil))
        (cl-letf (((symbol-function 'skewer-eval)
                   (lambda (code &optional callback &rest arguments)
                     (push (list (buffer-name) (copy-sequence code) callback
                                 (copy-tree arguments))
                           evaluations)
                     '((id . "livid-isolation")))))
          (with-current-buffer first
            (insert "const first = 1;\n")
            (livid369-test-activate-javascript)
            (livid-mode 1)
            (livid-toggle-pause))
          (with-current-buffer second
            (insert "const second = 2;\n")
            (livid369-test-activate-javascript)
            (livid-mode 1))
          (let ((enabled
                 (list :first (with-current-buffer first
                                (livid369-test-buffer-state))
                       :second (with-current-buffer second
                                 (livid369-test-buffer-state))))
                (count-before-disable (length evaluations)))
            (with-current-buffer first
              (livid-mode -1)
              (goto-char (point-max))
              (insert "const disabledEdit = 3;\n"))
            (let ((disabled
                   (with-current-buffer first (livid369-test-buffer-state)))
                  (count-after-disabled-edit (length evaluations)))
              (unless (= count-before-disable count-after-disabled-edit)
                (error "Disabled Livid buffer evaluated an edit"))
              (with-current-buffer first
                (livid-mode 1)
                (unless livid-paused
                  (error "Livid re-enable unexpectedly cleared pause state"))
                (livid-toggle-pause))
              (setq result
                    (list :enabled enabled
                          :disabled disabled
                          :disabled-evaluation-count-stable
                          (= count-after-disabled-edit (1- (length evaluations)))
                          :reenabled
                          (with-current-buffer first
                            (livid369-test-buffer-state))
                          :second-final
                          (with-current-buffer second
                            (livid369-test-buffer-state))
                          :evaluations (nreverse evaluations)))))))
    (when (buffer-live-p first) (kill-buffer first))
    (when (buffer-live-p second) (kill-buffer second)))
  result)
"####;
    ParityBatchCase::value(
        "disable_reenable_and_two_buffers_preserve_exact_local_state_and_isolation",
        form,
        expect![[
            r#"OK (:enabled (:first (:text "const first = 1;\n" :point 18 :mark nil :mark-active nil :modified t :mode (:mode t :lighter (livid-mode " livid") :key livid-toggle-pause :hook-count 1 :locals ((livid-timer t) (livid-last-seen t) (livid-paused t)) :last-seen "const first = 1;" :paused t :skewer nil)) :second (:text "const second = 2;\n" :point 19 :mark nil :mark-active nil :modified t :mode (:mode t :lighter (livid-mode " livid") :key livid-toggle-pause :hook-count 1 :locals ((livid-timer t) (livid-last-seen t) (livid-paused t)) :last-seen "const second = 2;" :paused nil :skewer nil))) :disabled (:text "const first = 1;\nconst disabledEdit = 3;\n" :point 42 :mark nil :mark-active nil :modified t :mode (:mode nil :lighter (livid-mode " livid") :key livid-toggle-pause :hook-count 0 :locals ((livid-timer t) (livid-last-seen t) (livid-paused t)) :last-seen "" :paused t :skewer nil)) :disabled-evaluation-count-stable t :reenabled (:text "const first = 1;\nconst disabledEdit = 3;\n" :point 42 :mark nil :mark-active nil :modified t :mode (:mode t :lighter (livid-mode " livid") :key livid-toggle-pause :hook-count 1 :locals ((livid-timer t) (livid-last-seen t) (livid-paused t)) :last-seen "const first = 1;\nconst disabledEdit = 3;" :paused nil :skewer nil)) :second-final (:text "const second = 2;\n" :point 19 :mark nil :mark-active nil :modified t :mode (:mode t :lighter (livid-mode " livid") :key livid-toggle-pause :hook-count 1 :locals ((livid-timer t) (livid-last-seen t) (livid-paused t)) :last-seen "const second = 2;" :paused nil :skewer nil)) :evaluations ((" *livid369-first*" "const first = 1;" nil nil) (" *livid369-second*" "const second = 2;" nil nil) (" *livid369-first*" "const first = 1;\nconst disabledEdit = 3;" nil nil)))"#
        ]],
    )
}

#[test]
fn livid_mode_package_batch() {
    assert_oracle_batch_cases(
        oracle(),
        "livid-mode-package-batch",
        "livid_mode_parity",
        &[
            activation_runs_the_initial_trimmed_evaluation_and_owns_local_mode_state(),
            real_edits_trim_dedupe_and_send_only_semantically_changed_programs(),
            owned_validator_rejects_syntax_error_and_retries_corrected_shell_quoted_code(),
            pause_key_suppresses_edits_and_unpause_evaluates_the_latest_program_once(),
            disable_reenable_and_two_buffers_preserve_exact_local_state_and_isolation(),
        ],
    );
}
