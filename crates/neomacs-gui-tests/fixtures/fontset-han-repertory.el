;;; fontset-han-repertory.el --- Actual CJK fontset selection -*- lexical-binding: t; -*-
(require 'json)
(defvar neomacs-han-control (getenv "NEOMACS_HAN_CONTROL"))

(defun neomacs-han-selected-file ()
  (redisplay t)
  (let* ((selected (internal-char-font (point-min) ?中))
         (font (car selected))
         (info (and font (font-info font))))
    (unless (and (consp selected) (integerp (cdr selected)) (> (cdr selected) 0)
                 (vectorp info) (stringp (aref info 12)))
      (error "No real glyph/font information for CJK character: %S %S" selected info))
    (file-name-nondirectory (aref info 12))))

(defun neomacs-han-paint-ack ()
  (if (file-exists-p (expand-file-name "painted" neomacs-han-control))
      (kill-emacs 0)
    (run-at-time 0.05 nil #'neomacs-han-paint-ack)))

(defun neomacs-han-run ()
  (condition-case err
      (progn
        ;; The primary face cannot render CJK. A separate explicit fallback
        ;; makes a rejected script rule observable instead of accidentally
        ;; finding the requested font through an automatic system fallback.
        (set-frame-font "Spleen-16")
        (set-fontset-font t nil
                          (font-spec :family "M PLUS 1 Code" :registry "iso10646-1"))
        (set-fontset-font t 'han
                          (font-spec :family "M PLUS 1 Code" :registry "iso10646-1"))
        (switch-to-buffer (get-buffer-create "*fontset-han-repertory*"))
        (erase-buffer)
        (insert "中\n")
        (goto-char (point-min))
        (let* ((baseline (neomacs-han-selected-file))
               (absent nil)
               (explicit nil))
          (set-fontset-font t 'han (font-spec :family "LXGWWenKai Nerd Font"))
          (setq absent (neomacs-han-selected-file))
          (set-fontset-font t 'han
                            (font-spec :family "LXGWWenKai Nerd Font" :registry "iso10646-1"))
          (setq explicit (neomacs-han-selected-file))
          (let* ((passed (and (equal baseline "MPLUS1Code-Thin.ttf")
                              (equal absent "LXGWWenKaiNerdFont-Regular.ttf")
                              (equal explicit absent)))
                 (state `((passed . ,(if passed t :json-false))
                          (baseline-file . ,baseline)
                          (absent-registry-file . ,absent)
                          (explicit-registry-file . ,explicit))))
            (with-temp-file (expand-file-name "result.json" neomacs-han-control)
              (insert (json-encode state)))
            (insert (if passed "FONTSET-HAN-PASSED\n" "FONTSET-HAN-FAILED\n"))
            (goto-char (point-min))
            (if (fboundp 'neomacs--write-frame-snapshot)
                (progn
                  (set-face-attribute 'default nil :foreground "#000000"
                                      :background (if passed "#00ff00" "#ff0000"))
                  (neomacs--write-frame-snapshot
                   (expand-file-name "final.json" neomacs-han-control) nil 'json)
                  (neomacs-han-paint-ack))
              (kill-emacs (if passed 0 1))))))
    (error
     (with-temp-file (expand-file-name "error.el" neomacs-han-control)
       (prin1 err (current-buffer)))
     (kill-emacs 1))))

(setq inhibit-startup-screen t)
(blink-cursor-mode -1)
(menu-bar-mode -1)
(tool-bar-mode -1)
(run-at-time 0.5 nil #'neomacs-han-run)
(run-at-time 30 nil (lambda () (kill-emacs 2)))
