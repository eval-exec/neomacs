;;; page-scrolling.el --- Org banner paging through the command loop -*- lexical-binding: t -*-

(require 'org)
(require 'image)
(require 'json)

(setq inhibit-startup-screen t
      scroll-preserve-screen-position nil
      auto-window-vscroll t)
(menu-bar-mode -1)
(tool-bar-mode -1)
(blink-cursor-mode -1)
(set-face-attribute 'default nil :family "DejaVu Sans Mono" :height 105
                    :foreground "#000000" :background "#ffffff")
(set-face-attribute 'variable-pitch nil :family "DejaVu Sans" :height 105)
;; Keep heading antialiasing out of the banner's orange pixel signature.
;; macOS's default Org heading colors otherwise overlap that signature.
(set-face-attribute 'org-level-1 nil :height 1.7 :foreground "#000000")
(set-face-attribute 'org-level-2 nil :height 1.4 :foreground "#000000")
(set-frame-size nil 680 700 t)

(defvar neomacs-scroll-control (getenv "NEOMACS_GUI_SCROLL_CONTROL"))
(defvar neomacs-scroll-trace nil)
(defvar neomacs-scroll-remaining 0)
(defvar neomacs-scroll-key nil)

(defun neomacs-scroll-record (key)
  (push `((key . ,key) (point . ,(point)) (start . ,(window-start))
          (vscroll . ,(window-vscroll nil t)))
        neomacs-scroll-trace)
  (with-temp-file (getenv "NEOMACS_GUI_STATE_JSON")
    (insert (json-encode (vconcat (reverse neomacs-scroll-trace))))))

(defun neomacs-scroll-capture (stage)
  (neomacs--write-frame-snapshot
   (expand-file-name (concat stage ".json") neomacs-scroll-control) nil 'json)
  (with-temp-file (expand-file-name (concat stage ".ready") neomacs-scroll-control)
    (insert "ready"))
  (run-at-time 0.05 nil #'neomacs-scroll-await-presentation stage))

(defun neomacs-scroll-await-presentation (stage)
  (cond
   ((file-exists-p (expand-file-name "stop" neomacs-scroll-control)) (kill-emacs 2))
   ((file-exists-p (expand-file-name (concat stage ".ack") neomacs-scroll-control))
    (if (equal stage "returned")
        (kill-emacs 0)
      (setq neomacs-scroll-key (if (equal stage "initial") "C-v" "M-v")
            neomacs-scroll-remaining 8)
      (neomacs-scroll-step)))
   (t (run-at-time 0.05 nil #'neomacs-scroll-await-presentation stage))))

(defun neomacs-scroll-step ()
  (condition-case err
      (progn
        ;; Execute actual key bindings, allowing ordinary command-loop
        ;; bookkeeping and redisplay between successive page commands.
        (condition-case nil
            (execute-kbd-macro (kbd neomacs-scroll-key))
          ((beginning-of-buffer end-of-buffer) nil))
        (setq neomacs-scroll-remaining (1- neomacs-scroll-remaining))
        (run-at-time 0.1 nil #'neomacs-scroll-after-command))
    (error (message "Page scrolling fixture failed: %S" err) (kill-emacs 1))))

(defun neomacs-scroll-after-command ()
  (neomacs-scroll-record neomacs-scroll-key)
  (if (> neomacs-scroll-remaining 0)
      (neomacs-scroll-step)
    (neomacs-scroll-capture (if (equal neomacs-scroll-key "C-v") "away" "returned"))))

(defun neomacs-scroll-start ()
  (condition-case err
      (progn
        (switch-to-buffer (get-buffer-create "*page-scrolling*"))
        (delete-other-windows)
        (org-mode)
        (variable-pitch-mode 1)
        (insert "[[file:banner.svg]]\n\n* Banner scrolling\n")
        (dotimes (index 80)
          (insert (format "Page scrolling row %02d: some variable-pitch text.\n" index))
          (when (= (% index 12) 0) (insert "** A larger heading\n")))
        (font-lock-ensure)
        (goto-char (point-min))
        (let ((overlay (make-overlay (point-min) (line-end-position)))
              ;; An in-memory SVG avoids packages, downloads and font-dependent
              ;; screenshots. Only the banner has this saturated orange fill.
              (image (create-image
                      (format "<svg xmlns='http://www.w3.org/2000/svg' width='620' height='%s'><rect width='100%%' height='100%%' fill='#ff7800'/></svg>"
                              (getenv "NEOMACS_GUI_SCROLL_IMAGE_HEIGHT"))
                      'svg t :scale 1 :ascent 'center)))
          (image-size image t)
          (overlay-put overlay 'display image))
        (set-window-start nil (point-min))
        (neomacs-scroll-record "initial")
        (neomacs-scroll-capture "initial"))
    (error (message "Page scrolling setup failed: %S" err) (kill-emacs 1))))

(run-at-time 1 nil #'neomacs-scroll-start)
