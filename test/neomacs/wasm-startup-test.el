;;; wasm-startup-test.el --- Browser startup behavior -*- lexical-binding: t; -*-

(require 'ert)
(require 'neomacs-wasm-startup)

(ert-deftest neomacs-wasm-command-available-in-editor-profile ()
  (should (commandp 'neomacs-wasm-landing-open)))

(ert-deftest neomacs-wasm-profile-respects-user-buffer-choice ()
  (save-window-excursion
    (let ((neomacs-wasm-startup-profile 'landing)
          (initial-buffer-choice t)
          (window-setup-hook '(neomacs-wasm-startup-finish)))
      (delete-other-windows)
      (switch-to-buffer "*scratch*")
      (run-hooks 'window-setup-hook)
      (should (equal (buffer-name) "*scratch*"))
      (should (one-window-p t))
      (should-not (memq 'neomacs-wasm-startup-finish window-setup-hook)))))

(ert-deftest neomacs-wasm-profile-respects-user-window-layout ()
  (save-window-excursion
    (let ((neomacs-wasm-startup-profile 'landing)
          (initial-buffer-choice nil)
          (window-setup-hook '(neomacs-wasm-startup-finish)))
      (delete-other-windows)
      (switch-to-buffer "*scratch*")
      (let ((user-window (split-window-below)))
        (run-hooks 'window-setup-hook)
        (should (window-live-p user-window))
        (should (= (length (window-list)) 2))))))

(ert-deftest neomacs-wasm-landing-keeps-playground-edits ()
  (require 'neomacs-wasm-landing)
  (save-window-excursion
    (unwind-protect
        (progn
          (neomacs-wasm-landing-open)
          (with-current-buffer "*NEO Emacs Playground*"
            (should (eq major-mode 'emacs-lisp-mode))
            (should (= (buffer-size) 0))
            (insert "(+ 1 2)"))
          (neomacs-wasm-landing-open)
          (with-current-buffer "*NEO Emacs Playground*"
            (should (equal (buffer-string) "(+ 1 2)")))
          (should (get-buffer "*NEO Emacs*")))
      (dolist (name '("*NEO Emacs*" "*NEO Emacs Playground*"))
        (when-let* ((buffer (get-buffer name)))
          (with-current-buffer buffer (set-buffer-modified-p nil))
          (kill-buffer buffer))))))

;;; wasm-startup-test.el ends here
