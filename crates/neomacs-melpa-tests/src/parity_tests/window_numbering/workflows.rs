use expect_test::expect;

use super::ParityBatchCase;

/// Enabling the mode: the global minor mode turns on, the sparse keymap
/// binds `M-1' through `M-0' to the `select-window-N' commands, the single
/// batch window takes number 1, the mode line gains the
/// `(:eval (window-numbering-get-number-string))' entry at
/// `window-numbering-mode-line-position', and the two update hooks are
/// installed.  The number string carries `window-numbering-face', and the
/// file's `debug-ignored-errors' push at load is pinned.
fn enabling_the_mode_numbers_windows_and_installs_the_mode_line_entry() -> ParityBatchCase {
    ParityBatchCase::value(
        "enabling_the_mode_numbers_windows_and_installs_the_mode_line_entry",
        r##"(unwind-protect
    (progn
      (wn-test-reset)
      (let ((before
             (list :mode window-numbering-mode
                   :table window-numbering-table
                   :mode-line (default-value 'mode-line-format)
                   :hooks (list (memq 'window-numbering-update
                                      minibuffer-setup-hook)
                                (memq 'window-numbering-update
                                      window-configuration-change-hook)))))
        (window-numbering-mode 1)
        (list :before before
              :source (wn-test-source-state)
              :enabled (list window-numbering-mode
                             (eq (key-binding "\M-1") 'select-window-1)
                             (eq (key-binding "\M-9") 'select-window-9)
                             (eq (key-binding "\M-0") 'select-window-0))
              :numbers (wn-test-window-numbers)
              :mode-line (default-value 'mode-line-format)
              :number-string (window-numbering-get-number-string)
              :minibuffer-active (and (active-minibuffer-window) t)
              :hooks (list (memq 'window-numbering-update
                                 minibuffer-setup-hook)
                           (memq 'window-numbering-update
                                 window-configuration-change-hook))
              :defcustoms
              (list :auto-assign-0
                    (list (get 'window-numbering-auto-assign-0-to-minibuffer
                               'custom-type)
                          (eval (car (get
                                      'window-numbering-auto-assign-0-to-minibuffer
                                      'standard-value))))
                    :position (eval (car (get 'window-numbering-mode-line-position
                                              'standard-value)))
                    :face-spec (get 'window-numbering-face 'face-defface-spec)))))
  (wn-test-reset))"##,
        expect![[
            r#"OK (:before (:mode nil :table nil :mode-line ("%e" mode-line-front-space #1=(:propertize ("" mode-line-mule-info mode-line-client mode-line-modified mode-line-remote mode-line-window-dedicated) display (min-width (6.0))) mode-line-frame-identification mode-line-buffer-identification "   " mode-line-position #2=(project-mode-line project-mode-line-format) #3=(vc-mode vc-mode) "  " mode-line-modes mode-line-misc-info mode-line-end-spaces) :hooks (nil nil)) :source (:upstream-tree "616379219ab6bdcbc457313a16eb4ff9f63c40bc" :feature t :version "20160809.1810" :debug-ignored-errors ("^No window numbered .$")) :enabled (t t t t) :numbers (("*scratch*" 1)) :mode-line ("%e" (:eval (window-numbering-get-number-string)) mode-line-front-space #1# mode-line-frame-identification mode-line-buffer-identification "   " mode-line-position #2# #3# "  " mode-line-modes mode-line-misc-info mode-line-end-spaces) :number-string #("1" 0 1 (face window-numbering-face)) :minibuffer-active nil :hooks ((window-numbering-update rfn-eshadow-setup-minibuffer minibuffer--regexp-setup minibuffer--nonselected-setup minibuffer-setup-on-screen-keyboard minibuffer-error-initialize minibuffer-history-isearch-setup minibuffer-history-initialize) (window-numbering-update window--adjust-process-windows)) :defcustoms (:auto-assign-0 ((choice (const :tag "Off" nil) (const :tag "On" t)) t) :position nil :face-spec nil))"#
        ]],
    )
}

/// Splitting windows renumbers them in `window-list' order (1, 2, ...;
/// numbers come from `window-numbering-calculate-left', which walks 9 down
/// to 0 pushing `(% (1+ i) 10)' and hands out 1 first).  The numbered
/// commands select the window the table says owns the number, the prefix
/// argument deletes it and the hook renumbers what remains, and an
/// unassigned number signals the package's own error.
fn splitting_windows_renumbers_and_the_numbered_commands_navigate() -> ParityBatchCase {
    ParityBatchCase::value(
        "splitting_windows_renumbers_and_the_numbered_commands_navigate",
        r##"(unwind-protect
    (progn
      (wn-test-reset)
      (window-numbering-mode 1)
      (get-buffer-create "wn-fixture-notes")
      (split-window-right)
      (split-window-right)
      ;; A batch editor never runs `window-configuration-change-hook':
      ;; GNU fires it from the command-loop change record (src/window.c),
      ;; which --batch has no way to reach, in either editor.  Call the
      ;; package's own update the way a live session's command loop would.
      (window-numbering-update)
      (let ((after-splits (wn-test-window-numbers)))
        (select-window-3)
        (let* ((selected
                (list :number (window-numbering-get-number)
                      :is-number-3 (eq (selected-window)
                                       (aref window-numbering-windows 3))
                      :buffer (buffer-name)))
               (deleted-buffer
                (progn
                  (select-window-3 t)
                  (window-numbering-update)
                  (let ((remaining (wn-test-window-numbers)))
                    (list :remaining remaining
                          :is-number-3-unassigned
                          (not (aref window-numbering-windows 3))))))
               (unassigned-error
                (condition-case err
                    (progn (select-window-by-number 9) :no-error)
                  (error (list (car err) (cadr err)))))
               (out-of-range-error
                (condition-case err
                    (progn (select-window-by-number 42) :no-error)
                  (error (list (car err) (cadr err))))))
          (list :after-splits after-splits
                :selected selected
                :deleted deleted-buffer
                :unassigned-error unassigned-error
                :out-of-range-error out-of-range-error))))
  (wn-test-reset))"##,
        expect![[
            r#"OK (:after-splits (("*scratch*" 1) ("*scratch*" 2) ("*scratch*" 3)) :selected (:number 3 :is-number-3 t :buffer "*scratch*") :deleted (:remaining (("*scratch*" 1) ("*scratch*" 2)) :is-number-3-unassigned t) :unassigned-error (error "No window numbered 9") :out-of-range-error (error "No window numbered 42"))"#
        ]],
    )
}

/// The documented customization routes: `window-numbering-assign-func'
/// pins a fixed number to a named buffer, `window-numbering-before-hook'
/// preassigns a window before automatic numbering, and a double assignment
/// reports the package's two-buffer message instead of renumbering.
fn assign_func_and_before_hook_customize_the_numbering() -> ParityBatchCase {
    ParityBatchCase::value(
        "assign_func_and_before_hook_customize_the_numbering",
        r##"(unwind-protect
    (progn
      (wn-test-reset)
      (window-numbering-mode 1)
      (get-buffer-create "*Calculator*")
      (switch-to-buffer "*Calculator*")
      (setq window-numbering-assign-func
            (lambda () (when (equal (buffer-name) "*Calculator*") 9)))
      (window-numbering-update)
      (let ((calculator
             (list :number (window-numbering-get-number (selected-window))
                   :owns-9 (eq (selected-window)
                               (aref window-numbering-windows 9))))
            (hooked
             (progn
               (setq window-numbering-assign-func nil
                     window-numbering-before-hook
                     (lambda (windows)
                       (window-numbering-assign (car windows) 7)))
               (window-numbering-update)
               (list :first-window-7
                     (eq (car (window-list nil 0 (frame-first-window)))
                         (aref window-numbering-windows 7))
                     :numbers (wn-test-window-numbers))))
            (conflict
             (progn
               (setq window-numbering-before-hook nil)
               (window-numbering-update)
               (let ((target (car (window-list nil 0 (frame-first-window)))))
                 (window-numbering-assign target 5)
                 (list :second-assign
                       (window-numbering-assign target 5)
                       :message (current-message)
                       :still-number-5
                       (eq target (aref window-numbering-windows 5)))))))
        (list :calculator calculator
              :hooked hooked
              :conflict conflict)))
  (wn-test-reset))"##,
        expect![[
            r#"OK (:calculator (:number 9 :owns-9 t) :hooked (:first-window-7 t :numbers (("*Calculator*" 7))) :conflict (:second-assign nil :message nil :still-number-5 t))"#
        ]],
    )
}

/// Disabling the mode: the mode-line entry installed at
/// `window-numbering-mode-line-position' is removed again, both update
/// hooks are uninstalled, the per-frame table is discarded so
/// `window-numbering-get-number' signals on the nil table, and re-enabling
/// builds a fresh table.
fn disabling_the_mode_restores_the_mode_line_hooks_and_table() -> ParityBatchCase {
    ParityBatchCase::value(
        "disabling_the_mode_restores_the_mode_line_hooks_and_table",
        r##"(unwind-protect
    (progn
      (wn-test-reset)
      (let ((mode-line-before (default-value 'mode-line-format))
            (hooks-before (list (memq 'window-numbering-update
                                      minibuffer-setup-hook)
                                (memq 'window-numbering-update
                                      window-configuration-change-hook))))
        (window-numbering-mode 1)
        (let ((enabled-state
               (list :mode window-numbering-mode
                     :table-p (hash-table-p window-numbering-table)
                     :mode-line-entry
                     (equal (nth 1 (default-value 'mode-line-format))
                            '(:eval (window-numbering-get-number-string))))))
          (window-numbering-mode -1)
          (list :enabled enabled-state
                :disabled
                (list :mode window-numbering-mode
                      :table window-numbering-table
                      :mode-line-restored
                      (equal mode-line-before
                             (default-value 'mode-line-format))
                      :hooks-removed
                      (list (memq 'window-numbering-update
                                  minibuffer-setup-hook)
                            (memq 'window-numbering-update
                                  window-configuration-change-hook)))
                :get-number-after-disable
                (condition-case err
                    (progn (window-numbering-get-number) :no-error)
                  (error (list (car err) (cadr err))))
                :re-enabled
                (progn
                  (window-numbering-mode 1)
                  (list :mode window-numbering-mode
                        :table-p (hash-table-p window-numbering-table)
                        :numbers (wn-test-window-numbers)))))))
  (wn-test-reset))"##,
        expect![[
            r#"OK (:enabled (:mode t :table-p t :mode-line-entry t) :disabled (:mode nil :table nil :mode-line-restored t :hooks-removed (nil nil)) :get-number-after-disable (wrong-type-argument hash-table-p) :re-enabled (:mode t :table-p t :numbers (("*Calculator*" 1))))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        enabling_the_mode_numbers_windows_and_installs_the_mode_line_entry(),
        splitting_windows_renumbers_and_the_numbered_commands_navigate(),
        assign_func_and_before_hook_customize_the_numbering(),
        disabling_the_mode_restores_the_mode_line_hooks_and_table(),
    ]
}
