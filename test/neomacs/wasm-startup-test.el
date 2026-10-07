;;; wasm-startup-test.el --- Browser startup behavior -*- lexical-binding: t; -*-

(require 'ert)

(defconst neomacs-wasm-test--root
  (expand-file-name "../../" (file-name-directory (or load-file-name buffer-file-name))))

;; The landing page is an example, outside the editor's own lisp tree.
(add-to-list 'load-path
             (expand-file-name "examples/neomacs-wasm-landing-page"
                               neomacs-wasm-test--root))
(require 'neomacs-wasm-startup)

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
             (expand-file-name "examples/neomacs-wasm-landing-page/site/playground.el"
                               neomacs-wasm-test--root))
            (should-not (string-match-p ";; 07 /" (buffer-string)))
            (goto-char (point-min))
            (let (form)
              (while (setq form (condition-case nil (read (current-buffer))
                                  (end-of-file nil)))
                (eval form t)))
            (should (commandp 'neo-greet)))
        (if previous-command (fset 'neo-greet previous-command) (fmakunbound 'neo-greet))))))

(ert-deftest neomacs-wasm-about-has-author-picture-and-profile-link ()
  (neomacs-wasm-test--with-site
   (save-window-excursion
     (unwind-protect
         (progn
           (neomacs-wasm-landing-open)
           (with-current-buffer "*About*"
             (let ((button (next-button (point-min)))
                   opened)
               (should button)
               (should (equal (button-label button) "Eval Exec"))
               ;; The browser/OS opener is the external boundary here.
               (let ((browse-url-browser-function
                      (lambda (url &rest _) (setq opened url))))
                 (button-activate button))
               (should (equal opened "https://github.com/eval-exec")))
             (should (string-match-p
                      (regexp-quote "https://github.com/eval-exec/neomacs")
                      (buffer-string)))
             (should (file-readable-p
                      (expand-file-name "author.jpg"
                                        (neomacs-wasm-landing--root))))
             (should buffer-read-only)))
       (dolist (name '("*NEO Emacs*" "*Playgorund*" "*About*"))
         (when-let* ((buffer (get-buffer name)))
           (with-current-buffer buffer (set-buffer-modified-p nil))
           (kill-buffer buffer)))))))

(defmacro neomacs-wasm-test--with-resizable-landing (&rest body)
  "Exercise frame resize hooks with a stand-in for the optional tree package."
  (declare (indent 0))
  `(neomacs-wasm-test--with-site
     (let* ((frame (selected-frame))
            (original-width (window-total-width (frame-root-window frame)))
            (original-layout (frame-parameter frame 'neomacs-wasm-landing-layout))
            (window-size-change-functions nil)
            (original-features features))
       (provide 'treemacs)
       (save-window-excursion
         (unwind-protect
             (cl-letf (((symbol-function 'neomacs-wasm-landing--tree)
                        (lambda ()
                          (with-current-buffer (get-buffer-create "*Landing test tree*")
                            (setq major-mode 'treemacs-mode)
                            (display-buffer-in-side-window
                             (current-buffer) '((side . left) (window-width . 24)))))))
               ,@body)
           (setq features original-features)
           (set-frame-width frame original-width)
           (set-frame-parameter frame 'neomacs-wasm-landing-layout original-layout)
           (dolist (name '("*NEO Emacs*" "*Playgorund*" "*About*"
                           "*Landing test tree*" "*Landing custom pane*"))
             (when-let* ((buffer (get-buffer name)))
               (with-current-buffer buffer (set-buffer-modified-p nil))
               (kill-buffer buffer))))))))

(ert-deftest neomacs-wasm-landing-restores-panes-on-width-changes ()
  (neomacs-wasm-test--with-resizable-landing
   (set-frame-width frame 80)
   (neomacs-wasm-landing-open)
   (should (one-window-p t))
   (with-current-buffer "*Playgorund*" (erase-buffer) (insert "(+ 20 22)"))
   ;; A real size-change notification must restore panes without reopening.
   (set-frame-width frame 150)
   (run-hook-with-args 'window-size-change-functions frame)
   (should (get-buffer-window "*Landing test tree*" frame))
   (should-not (get-buffer-window "*About*" frame))
   (set-frame-width frame 200)
   (run-hook-with-args 'window-size-change-functions frame)
   (should (get-buffer-window "*Landing test tree*" frame))
   (should (get-buffer-window "*About*" frame))
   (should (get-buffer-window "*Playgorund*" frame))
   (should (equal (buffer-name) "*NEO Emacs*"))
   (with-current-buffer "*Playgorund*" (should (equal (buffer-string) "(+ 20 22)")))
   (let ((windows (window-list frame 'no-minibuf)))
     (set-frame-width frame 210)
     (run-hook-with-args 'window-size-change-functions frame)
     (should (equal windows (window-list frame 'no-minibuf))))
   ;; Model the pane deletion performed when the frontend shrinks its frame.
   (set-frame-width frame 80)
   (dolist (window (window-list frame 'no-minibuf))
     (unless (equal (buffer-name (window-buffer window)) "*NEO Emacs*")
       (delete-window window)))
   (run-hook-with-args 'window-size-change-functions frame)
   (set-frame-width frame 200)
   (run-hook-with-args 'window-size-change-functions frame)
   (should (get-buffer-window "*Landing test tree*" frame))
   (should (get-buffer-window "*About*" frame))
   (with-current-buffer "*Playgorund*" (should (equal (buffer-string) "(+ 20 22)")))))

(ert-deftest neomacs-wasm-landing-resize-preserves-user-layout ()
  (neomacs-wasm-test--with-resizable-landing
   (set-frame-width frame 80)
   (neomacs-wasm-landing-open)
   (set-window-buffer (split-window-below) (get-buffer-create "*Landing custom pane*"))
   (run-hook-with-args 'window-size-change-functions frame)
   (set-frame-width frame 200)
   (run-hook-with-args 'window-size-change-functions frame)
   (should (get-buffer-window "*Landing custom pane*" frame))
   (should (= (length (window-list frame 'no-minibuf)) 2))
   (should-not (get-buffer-window "*About*" frame))))

(ert-deftest neomacs-wasm-landing-resize-respects-a-closed-pane ()
  (neomacs-wasm-test--with-resizable-landing
   (set-frame-width frame 200)
   (neomacs-wasm-landing-open)
   (delete-window (get-buffer-window "*About*" frame))
   (run-hook-with-args 'window-size-change-functions frame)
   (set-frame-width frame 210)
   (run-hook-with-args 'window-size-change-functions frame)
   (should-not (get-buffer-window "*About*" frame))))

(ert-deftest neomacs-wasm-landing-resize-preserves-a-new-buffer ()
  (neomacs-wasm-test--with-resizable-landing
   (set-frame-width frame 80)
   (neomacs-wasm-landing-open)
   (switch-to-buffer (get-buffer-create "*Landing custom pane*"))
   (set-frame-width frame 200)
   (run-hook-with-args 'window-size-change-functions frame)
   (should (one-window-p t))
   (should (equal (buffer-name) "*Landing custom pane*"))))

;;; wasm-startup-test.el ends here
