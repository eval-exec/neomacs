;;; wasm-startup-test.el --- Browser startup behavior -*- lexical-binding: t; -*-

(require 'ert)
(require 'neomacs-wasm-startup)

(defconst neomacs-wasm-test--root
  (expand-file-name "../../" (file-name-directory (or load-file-name buffer-file-name))))

(defmacro neomacs-wasm-test--with-site (&rest body)
  "Exercise the shipped site with an isolated writable playground."
  (declare (indent 0))
  `(progn
     (require 'neomacs-wasm-landing)
     (when (seq-some #'get-buffer
                     '("*NEO Emacs*" "*Playgorund*" "*About*"))
       (ert-skip "Do not change an existing interactive landing session"))
     (make-directory (expand-file-name "tmp/" neomacs-wasm-test--root) t)
     (let* ((directory (make-temp-file
                        (expand-file-name "tmp/landing-ert-" neomacs-wasm-test--root) t))
            (data-directory (expand-file-name "etc/" neomacs-wasm-test--root))
            (neomacs-wasm-landing-playground-file (expand-file-name "playground.el" directory)))
       (unwind-protect (progn ,@body)
         (delete-directory directory t)))))

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
  (neomacs-wasm-test--with-site
   (save-window-excursion
     (unwind-protect
         (progn
           (neomacs-wasm-landing-open)
           (should (get-buffer "*About*"))
           (with-current-buffer "*Playgorund*"
             (should (eq major-mode 'emacs-lisp-mode))
             (should (string-match-p (regexp-quote "(+ 1 2)") (buffer-string)))
             (should (equal (eval (preceding-sexp) t) 3))
             (erase-buffer)
             (insert "(+ 1 2)"))
           (neomacs-wasm-landing-open)
           (with-current-buffer "*Playgorund*"
             (should (equal (buffer-string) "(+ 1 2)")))
           (should (get-buffer "*NEO Emacs*")))
       (dolist (name '("*NEO Emacs*" "*Playgorund*" "*About*"))
         (when-let* ((buffer (get-buffer name)))
           (with-current-buffer buffer (set-buffer-modified-p nil))
           (kill-buffer buffer)))))))

(ert-deftest neomacs-wasm-welcome-is-a-readable-org-tour ()
  (neomacs-wasm-test--with-site
   (save-window-excursion
     (unwind-protect
         (progn
           (neomacs-wasm-landing-open)
           (with-current-buffer "*NEO Emacs*"
             (should (derived-mode-p 'org-mode))
             (should buffer-read-only)
             (should (eq buffer-face-mode-face 'neomacs-wasm-landing-body))
             (dolist (face '(org-level-1 org-level-2 org-level-3))
               (should (assq face face-remapping-alist)))
             (should (facep 'neomacs-wasm-landing-body))
             (should (facep 'neomacs-wasm-welcome-heading))
             (should-not (equal (face-attribute 'neomacs-wasm-landing-body :family)
                                (face-attribute 'neomacs-wasm-welcome-heading :family)))
             (should (string-match-p "C-x C-e" (buffer-string)))
             (should (string-match-p "Save before reloading" (buffer-string))))
           (with-current-buffer "*About*"
             (should (eq buffer-face-mode-face 'neomacs-wasm-landing-body))))
       (dolist (name '("*NEO Emacs*" "*Playgorund*" "*About*"))
         (when-let* ((buffer (get-buffer name)))
           (with-current-buffer buffer (set-buffer-modified-p nil))
           (kill-buffer buffer)))))))

(ert-deftest neomacs-wasm-starter-forms-are-runnable ()
  (require 'neomacs-wasm-landing)
  (let ((previous-command (and (fboundp 'neo-greet) (symbol-function 'neo-greet))))
    (save-window-excursion
      (unwind-protect
          (with-temp-buffer
            (emacs-lisp-mode)
            (insert-file-contents
             (expand-file-name "etc/neomacs-landing/playground.el" neomacs-wasm-test--root))
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
