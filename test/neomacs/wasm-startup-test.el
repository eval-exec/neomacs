;;; wasm-startup-test.el --- Browser startup behavior -*- lexical-binding: t; -*-

(require 'ert)
(require 'neomacs-wasm-startup)

(ert-deftest neomacs-wasm-build-defaults-to-landing ()
  (should (eq (default-value 'neomacs-wasm-startup-profile) 'landing)))

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
            (should (string-match-p (regexp-quote "(+ 1 2)") (buffer-string)))
            (should (equal (eval (preceding-sexp) t) 3))
            (erase-buffer)
            (insert "(+ 1 2)"))
          (neomacs-wasm-landing-open)
          (with-current-buffer "*NEO Emacs Playground*"
            (should (equal (buffer-string) "(+ 1 2)")))
          (should (get-buffer "*NEO Emacs*")))
      (dolist (name '("*NEO Emacs*" "*NEO Emacs Playground*" "*NEO Emacs About*"))
        (when-let* ((buffer (get-buffer name)))
          (with-current-buffer buffer (set-buffer-modified-p nil))
          (kill-buffer buffer))))))

(ert-deftest neomacs-wasm-welcome-is-a-readable-org-tour ()
  (require 'neomacs-wasm-landing)
  (save-window-excursion
    (unwind-protect
        (progn
          (neomacs-wasm-landing-open)
          (with-current-buffer "*NEO Emacs*"
            (should (derived-mode-p 'org-mode))
            (should buffer-read-only)
            (should (string-match-p "C-x C-e" (buffer-string)))
            (should (string-match-p "Unsaved buffers" (buffer-string)))))
      (dolist (name '("*NEO Emacs*" "*NEO Emacs Playground*" "*NEO Emacs About*"))
        (when-let* ((buffer (get-buffer name)))
          (with-current-buffer buffer (set-buffer-modified-p nil))
          (kill-buffer buffer))))))

(ert-deftest neomacs-wasm-starter-forms-are-runnable ()
  (require 'neomacs-wasm-landing)
  (let ((previous-command (and (fboundp 'neo-greet) (symbol-function 'neo-greet))))
    (save-window-excursion
      (unwind-protect
          (with-temp-buffer
            (emacs-lisp-mode)
            (insert neomacs-wasm-landing--examples)
            (goto-char (point-min))
            (let (form)
              (while (setq form (condition-case nil (read (current-buffer))
                                 (end-of-file nil)))
                (eval form t)))
            (should (commandp 'neo-greet))
            (should (get-buffer "*NEO Notes*")))
        (if previous-command (fset 'neo-greet previous-command) (fmakunbound 'neo-greet))
        (when-let* ((buffer (get-buffer "*NEO Notes*")))
          (with-current-buffer buffer (set-buffer-modified-p nil))
          (kill-buffer buffer))))))

;;; wasm-startup-test.el ends here
