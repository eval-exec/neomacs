use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, WINUM_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'winum)

(defun neomacs-winum-test-user-terminal-name (&optional _terminal)
  "Model a normal named terminal instead of batch-only initial_terminal."
  "tty-neomacs-parity")

;; Winum deliberately excludes `initial_terminal'.  Both parity executables
;; run headlessly on that special bootstrap terminal, unlike an interactive
;; user's TTY or graphical frame, so replace only this environmental boundary.
(advice-add 'terminal-name :override #'neomacs-winum-test-user-terminal-name)

(defun neomacs-winum-test-reset ()
  "Restore Winum and the selected frame to an empty one-window layout."
  (when winum-mode (winum-mode -1))
  (remove-hook 'minibuffer-setup-hook 'winum--update)
  (remove-hook 'window-configuration-change-hook 'winum--update)
  (setq winum--window-count nil
        winum--remaining nil
        winum--window-vector nil
        winum--numbers-table nil
        winum--frames-table nil
        winum--last-used-scope winum-scope)
  (delete-other-windows))

(defun neomacs-winum-test-layout (&rest names)
  "Create a deterministic one-, two-, three-, or four-window layout for NAMES."
  (delete-other-windows)
  (let* ((buffers (mapcar #'get-buffer-create names))
         (top-left (selected-window))
         top-right bottom-left bottom-right)
    (set-window-buffer top-left (nth 0 buffers))
    (when (nth 1 buffers)
      (setq top-right (split-window-right))
      (set-window-buffer top-right (nth 1 buffers)))
    (when (nth 2 buffers)
      (setq bottom-left (split-window-below nil top-left))
      (set-window-buffer bottom-left (nth 2 buffers)))
    (when (nth 3 buffers)
      (setq bottom-right (split-window-below nil top-right))
      (set-window-buffer bottom-right (nth 3 buffers)))
    (select-window top-left)
    buffers))

(defun neomacs-winum-test-kill-buffers (buffers)
  "Kill BUFFERS without retaining shared-process state."
  (dolist (buffer buffers)
    (when (buffer-live-p buffer) (kill-buffer buffer))))

(defun neomacs-winum-test-snapshot ()
  "Describe every live window and Winum's public bidirectional mapping."
  (mapcar
   (lambda (window)
     (let ((number (winum-get-number window)))
       (list :buffer (buffer-name (window-buffer window))
             :number number
             :number-string
             (let ((string (winum-get-number-string window)))
               (list (substring-no-properties string)
                     (get-text-property 0 'face string)))
             :lookup-round-trip
             (and number (eq window (winum-get-window-by-number number))))))
   (window-list)))
"####;

fn numbers_a_three_pane_release_workspace_bidirectionally() -> ParityBatchCase {
    let elisp_form = r####"
(save-window-excursion
  (neomacs-winum-test-reset)
  (let ((winum-scope 'global)
        (winum-auto-assign-0-to-minibuffer nil)
        (winum-auto-setup-mode-line nil)
        (winum-ignored-buffers nil)
        buffers)
    (unwind-protect
        (progn
          (setq buffers
                (neomacs-winum-test-layout
                 "*winum source*" "*winum tests*" "*winum logs*"))
          (winum-mode 1)
          (list :selected (buffer-name)
                :windows (neomacs-winum-test-snapshot)
                :lookup
                (mapcar
                 (lambda (number)
                   (let ((window (winum-get-window-by-number number)))
                     (cons number
                           (and window (buffer-name (window-buffer window))))))
                 '(0 1 2 3 4))))
      (neomacs-winum-test-reset)
      (neomacs-winum-test-kill-buffers buffers))))
"####;
    let expected = expect![[
        r#"OK (:selected "*winum source*" :windows ((:buffer "*winum source*" :number 1 :number-string ("1" winum-face) :lookup-round-trip t) (:buffer "*winum logs*" :number 2 :number-string ("2" winum-face) :lookup-round-trip t) (:buffer "*winum tests*" :number 3 :number-string ("3" winum-face) :lookup-round-trip t)) :lookup ((0) (1 . "*winum source*") (2 . "*winum logs*") (3 . "*winum tests*") (4)))"#
    ]];
    ParityBatchCase::value(
        "numbers_a_three_pane_release_workspace_bidirectionally",
        elisp_form,
        expected,
    )
}

fn selects_and_deletes_numbered_windows_then_renumbers_the_layout() -> ParityBatchCase {
    let elisp_form = r####"
(save-window-excursion
  (neomacs-winum-test-reset)
  (let ((winum-scope 'global)
        (winum-auto-assign-0-to-minibuffer nil)
        (winum-auto-setup-mode-line nil)
        (winum-ignored-buffers nil)
        buffers)
    (unwind-protect
        (progn
          (setq buffers
                (neomacs-winum-test-layout
                 "*winum editor*" "*winum preview*" "*winum terminal*"))
          (winum-mode 1)
          (winum-select-window-by-number 2)
          (let ((selected-two (buffer-name)))
            (winum-select-window-3)
            (let ((selected-three (buffer-name)))
              (winum-select-window-by-number -2)
              (run-hooks 'window-configuration-change-hook)
              (list :selected-two selected-two
                    :selected-three selected-three
                    :after-delete (neomacs-winum-test-snapshot)
                    :selected-after-delete (buffer-name)
                    :missing
                    (condition-case error
                        (progn (winum-select-window-by-number 9) :no-error)
                      (error (cons (car error) (cdr error))))))))
      (neomacs-winum-test-reset)
      (neomacs-winum-test-kill-buffers buffers))))
"####;
    let expected = expect![[
        r#"OK (:selected-two "*winum terminal*" :selected-three "*winum preview*" :after-delete ((:buffer "*winum preview*" :number 2 :number-string ("2" winum-face) :lookup-round-trip t) (:buffer "*winum editor*" :number 1 :number-string ("1" winum-face) :lookup-round-trip t)) :selected-after-delete "*winum preview*" :missing (error "No window numbered 9"))"#
    ]];
    ParityBatchCase::value(
        "selects_and_deletes_numbered_windows_then_renumbers_the_layout",
        elisp_form,
        expected,
    )
}

fn honors_custom_numbers_and_ignored_tool_windows() -> ParityBatchCase {
    let elisp_form = r####"
(save-window-excursion
  (neomacs-winum-test-reset)
  (let ((winum-scope 'global)
        (winum-auto-assign-0-to-minibuffer nil)
        (winum-auto-setup-mode-line nil)
        (winum-ignored-buffers '("*winum completion*"))
        (winum-ignored-buffers-regexp '("\\*winum build-[^*]+\\*"))
        (winum-assign-functions
         (list
          (lambda ()
            (cond
             ((equal (buffer-name) "*winum dashboard*") 10)
             ((equal (buffer-name) "*winum diagnostics*") 8)))))
        buffers)
    (unwind-protect
        (progn
          (setq buffers
                (neomacs-winum-test-layout
                 "*winum dashboard*" "*winum diagnostics*"
                 "*winum completion*" "*winum build-output*"))
          (winum-mode 1)
          (let ((mapping (neomacs-winum-test-snapshot)))
            (select-window (car (window-list)))
            (winum-select-window-0-or-10)
            (list :mapping mapping
                  :selected-by-zero-fallback (buffer-name)
                  :number-8
                  (buffer-name (window-buffer (winum-get-window-by-number 8)))
                  :number-10
                  (buffer-name (window-buffer (winum-get-window-by-number 10)))
                  :ignored
                  (mapcar
                   (lambda (name)
                     (let ((window (get-buffer-window name)))
                       (cons name (and window (winum-get-number window)))))
                   '("*winum completion*" "*winum build-output*")))))
      (neomacs-winum-test-reset)
      (neomacs-winum-test-kill-buffers buffers))))
"####;
    let expected = expect![[
        r#"OK (:mapping ((:buffer "*winum dashboard*" :number 10 :number-string ("10" winum-face) :lookup-round-trip t) (:buffer "*winum completion*" :number nil :number-string ("" nil) :lookup-round-trip nil) (:buffer "*winum diagnostics*" :number 8 :number-string ("8" winum-face) :lookup-round-trip t) (:buffer "*winum build-output*" :number nil :number-string ("" nil) :lookup-round-trip nil)) :selected-by-zero-fallback "*winum dashboard*" :number-8 "*winum diagnostics*" :number-10 "*winum dashboard*" :ignored (("*winum completion*") ("*winum build-output*")))"#
    ]];
    ParityBatchCase::value(
        "honors_custom_numbers_and_ignored_tool_windows",
        elisp_form,
        expected,
    )
}

fn remaps_the_prefix_and_dispatches_a_real_number_key() -> ParityBatchCase {
    let elisp_form = r####"
(save-window-excursion
  (neomacs-winum-test-reset)
  (let ((original-keymap winum-keymap)
        (winum-scope 'global)
        (winum-auto-assign-0-to-minibuffer nil)
        (winum-auto-setup-mode-line nil)
        (winum-ignored-buffers nil)
        buffers)
    (unwind-protect
        (progn
          (setq buffers
                (neomacs-winum-test-layout "*winum left*" "*winum right*"))
          (winum-set-keymap-prefix (kbd "C-c n"))
          (winum-mode 1)
          (let ((bindings
                 (list :new-2 (key-binding (kbd "C-c n 2"))
                       :new-prompt (key-binding (kbd "C-c n `"))
                       :old-1 (key-binding (kbd "C-x w 1")))))
            (execute-kbd-macro (kbd "C-c n 2"))
            (list :bindings bindings
                  :selected (buffer-name)
                  :number (winum-get-number))))
      (when winum-mode (winum-mode -1))
      (setq winum-keymap original-keymap)
      (setcdr (assoc 'winum-mode minor-mode-map-alist) winum-keymap)
      (neomacs-winum-test-reset)
      (neomacs-winum-test-kill-buffers buffers))))
"####;
    let expected = expect![[
        r#"OK (:bindings (:new-2 winum-select-window-2 :new-prompt winum-select-window-by-number :old-1 nil) :selected "*winum right*" :number 2)"#
    ]];
    ParityBatchCase::value(
        "remaps_the_prefix_and_dispatches_a_real_number_key",
        elisp_form,
        expected,
    )
}

fn installs_formats_and_removes_the_mode_line_number() -> ParityBatchCase {
    let elisp_form = r####"
(save-window-excursion
  (neomacs-winum-test-reset)
  (let ((original-default (default-value 'mode-line-format))
        (winum-scope 'global)
        (winum-auto-assign-0-to-minibuffer nil)
        (winum-auto-setup-mode-line t)
        (winum-mode-line-position 1)
        (winum-format "<pane:%s>")
        (winum-ignored-buffers nil)
        buffers)
    (unwind-protect
        (progn
          (setq-default mode-line-format '("[" mode-line-buffer-identification "]"))
          (setq mode-line-format (default-value 'mode-line-format))
          (setq buffers
                (neomacs-winum-test-layout "*winum code*" "*winum review*"))
          (dolist (buffer buffers)
            (with-current-buffer buffer
              (setq mode-line-format (default-value 'mode-line-format))))
          (winum-mode 1)
          (let ((enabled-structure (default-value 'mode-line-format))
                (rendered
                 (mapcar
                  (lambda (window)
                    (with-selected-window window
                      (cons (buffer-name)
                            (substring-no-properties (format-mode-line mode-line-format)))))
                  (window-list)))
                (segments
                 (mapcar
                  (lambda (window)
                    (with-selected-window window
                      (cons (buffer-name)
                            (substring-no-properties
                             (eval (cadr winum--mode-line-segment))))))
                  (window-list))))
            (winum-mode -1)
            (list :enabled enabled-structure
                  :rendered rendered
                  :segments segments
                  :disabled (default-value 'mode-line-format)
                  :window-hook
                  (memq 'winum--update window-configuration-change-hook)
                  :minibuffer-hook
                  (memq 'winum--update minibuffer-setup-hook))))
      (when winum-mode (winum-mode -1))
      (setq-default mode-line-format original-default)
      (setq mode-line-format original-default)
      (neomacs-winum-test-reset)
      (neomacs-winum-test-kill-buffers buffers))))
"####;
    let expected = expect![[
        r#"OK (:enabled ("[" (:eval (format winum-format (winum-get-number-string))) mode-line-buffer-identification "]") :rendered (("*winum code*" . "") ("*winum review*" . "")) :segments (("*winum code*" . "<pane:1>") ("*winum review*" . "<pane:2>")) :disabled ("[" mode-line-buffer-identification "]") :window-hook nil :minibuffer-hook nil)"#
    ]];
    ParityBatchCase::value(
        "installs_formats_and_removes_the_mode_line_number",
        elisp_form,
        expected,
    )
}

fn tracks_new_and_removed_panes_through_the_window_change_hook() -> ParityBatchCase {
    let elisp_form = r####"
(save-window-excursion
  (neomacs-winum-test-reset)
  (let ((winum-scope 'global)
        (winum-auto-assign-0-to-minibuffer nil)
        (winum-auto-setup-mode-line nil)
        (winum-ignored-buffers nil)
        buffers extra-window)
    (unwind-protect
        (progn
          (setq buffers
                (neomacs-winum-test-layout "*winum main*" "*winum docs*"))
          (winum-mode 1)
          (let ((initial (neomacs-winum-test-snapshot)))
            (setq extra-window (split-window-below nil (car (window-list))))
            (push (get-buffer-create "*winum shell*") buffers)
            (set-window-buffer extra-window (car buffers))
            (run-hooks 'window-configuration-change-hook)
            (let ((expanded (neomacs-winum-test-snapshot)))
              (delete-window extra-window)
              (run-hooks 'window-configuration-change-hook)
              (list :initial initial
                    :expanded expanded
                    :contracted (neomacs-winum-test-snapshot)
                    :window-count winum--window-count))))
      (neomacs-winum-test-reset)
      (neomacs-winum-test-kill-buffers buffers))))
"####;
    let expected = expect![[
        r#"OK (:initial ((:buffer "*winum main*" :number 1 :number-string ("1" winum-face) :lookup-round-trip t) (:buffer "*winum docs*" :number 2 :number-string ("2" winum-face) :lookup-round-trip t)) :expanded ((:buffer "*winum main*" :number 1 :number-string ("1" winum-face) :lookup-round-trip t) (:buffer "*winum shell*" :number 2 :number-string ("2" winum-face) :lookup-round-trip t) (:buffer "*winum docs*" :number 3 :number-string ("3" winum-face) :lookup-round-trip t)) :contracted ((:buffer "*winum main*" :number 1 :number-string ("1" winum-face) :lookup-round-trip t) (:buffer "*winum docs*" :number 2 :number-string ("2" winum-face) :lookup-round-trip t)) :window-count 2)"#
    ]];
    ParityBatchCase::value(
        "tracks_new_and_removed_panes_through_the_window_change_hook",
        elisp_form,
        expected,
    )
}

fn reports_an_invalid_runtime_scope_without_leaving_hooks_installed() -> ParityBatchCase {
    let elisp_form = r####"
(save-window-excursion
  (neomacs-winum-test-reset)
  (let ((winum-scope 'workspace)
        (winum-auto-setup-mode-line nil))
    (unwind-protect
        (let ((outcome
               (condition-case error
                   (progn (winum-mode 1) :no-error)
                 (error (cons (car error) (cdr error))))))
          (when winum-mode (winum-mode -1))
          (list :outcome outcome
                :mode winum-mode
                :window-hook
                (memq 'winum--update window-configuration-change-hook)
                :minibuffer-hook
                (memq 'winum--update minibuffer-setup-hook)))
      (neomacs-winum-test-reset))))
"####;
    let expected = expect![[
        r#"OK (:outcome (error "Invalid ‘winum-scope’: workspace") :mode nil :window-hook nil :minibuffer-hook nil)"#
    ]];
    ParityBatchCase::value(
        "reports_an_invalid_runtime_scope_without_leaving_hooks_installed",
        elisp_form,
        expected,
    )
}

fn winum_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(WINUM_MELPA_PIN, "winum.el")
        .expect("prepare pinned Winum and Dash below ./tmp")
        .with_timeout(Duration::from_secs(240))
        .with_prelude(PRELUDE)
}

#[test]
fn winum_practical_workflows_batch() {
    let cases = vec![
        numbers_a_three_pane_release_workspace_bidirectionally(),
        selects_and_deletes_numbered_windows_then_renumbers_the_layout(),
        honors_custom_numbers_and_ignored_tool_windows(),
        remaps_the_prefix_and_dispatches_a_real_number_key(),
        installs_formats_and_removes_the_mode_line_number(),
        tracks_new_and_removed_panes_through_the_window_change_hook(),
        reports_an_invalid_runtime_scope_without_leaving_hooks_installed(),
    ];
    let thread = std::thread::current();
    let test_name = thread
        .name()
        .unwrap_or("winum practical workflow parity batch");
    assert_oracle_batch_cases(winum_oracle(), test_name, "winum parity", &cases);
}
