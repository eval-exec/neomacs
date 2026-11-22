;;; desktop-font-startup.el --- Desktop default font regression -*- lexical-binding: t -*-
(setq native-comp-jit-compilation nil native-comp-enable-subr-trampolines nil)
(dolist (variable '("NEOMACS_GUI_SVG_LIB_DIR" "NEOMACS_GUI_SVG_TAG_MODE_DIR"))
  (let ((directory (getenv variable)))
    (unless directory (error "Set %s to the installed package directory" variable))
    (add-to-list 'load-path directory)))
(require 'svg-tag-mode)

(defun desktop-font-startup-receipt ()
  (let ((file (getenv "NEOMACS_GUI_PRESENTATION_RECEIPT")))
    (when (and file (file-exists-p file))
      (with-temp-buffer (insert-file-contents file) (read (current-buffer))))))

(defun desktop-font-startup-presented (after)
  (or (not (getenv "NEOMACS_GUI_PRESENTATION_RECEIPT"))
      (let ((receipt (desktop-font-startup-receipt)))
        (and receipt
             (eq (plist-get receipt :outcome) 'presented)
             (> (plist-get receipt :submission) after)
             (= (plist-get receipt :width) (frame-outer-width))
             (= (plist-get receipt :height) (frame-outer-height))
             (integerp (plist-get receipt :clock-id))
             (natnump (plist-get receipt :seconds))
             (natnump (plist-get receipt :nanoseconds))
             (< (plist-get receipt :nanoseconds) 1000000000)))))

(run-at-time
 1 nil
 (lambda ()
   (condition-case err
       (progn
         (switch-to-buffer (get-buffer-create "*desktop-font-startup*"))
         (unless (equal (font-get-system-font) "Ubuntu Mono 13")
           (error "Desktop monospace query: expected Ubuntu Mono 13, got %S"
                  (font-get-system-font)))
         (unless (equal (font-get-system-normal-font) "Ubuntu 10")
           (error "Desktop application font was not kept separate"))
         ;; GNU chooses the initial font before gui_figure_window_size. A
         ;; desktop preference must not reduce the default 80-column frame.
         (unless (= (frame-width) 80)
           (error "Initial geometry used a different font: expected 80 columns, got %S"
                  (frame-width)))
         (unless (and (null face-remapping-alist)
                      (equal (face-attribute 'default :family) "Ubuntu Mono")
                      (= (window-font-width) 10) (= (window-font-height) 20))
           (error "Startup ignored desktop font: %S, cell %S, remapping %S"
                  (face-attribute 'default :family)
                  (list (window-font-width) (window-font-height)) face-remapping-alist))
         (let* ((tag (svg-tag-make "TODO" :margin 0 :radius 8 :padding 0.07
                                   :height 0.71 :ascent 79
                                   :font-family "DejaVu Sans Mono" :font-size 7))
                (data (plist-get (cdr tag) :data)))
           ;; Independently captured from GNU at 96 DPI with the GUI job's
           ;; fonts-ubuntu package (0.869+git20240321, noble): Ubuntu Mono 13pt
           ;; resolves to a 10x20 cell, so the 0.71-height tag is 14.2px tall
           ;; (the 9x18 Ubuntu Mono of the pre-0.869 font gave 12.78px).
           (unless (string-match "<svg width=\"40\\.7\" height=\"14\\.2\"" data)
             (error "Startup SVG differs from GNU: %S" data))
           (insert-image tag)
           (insert " Desktop monospace font selected before user configuration.\n"))
         (when (fboundp 'neomacs--write-frame-snapshot)
           (neomacs--write-frame-snapshot
            (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON") t 'json))
         ;; Observe the settled native resize through the presentation
         ;; receipt, not just the pending Lisp request: a software Wayland
         ;; compositor can take arbitrarily long to acknowledge the configure,
         ;; so the receipt for the 91-column frame is the barrier and the
         ;; harness's 60 s process cap is the only wall-clock bound.
         (let ((original-height (frame-pixel-height))
               (submission
                (or (plist-get (desktop-font-startup-receipt) :submission) 0))
               (timer nil))
           (set-frame-width nil 91)
           (setq timer
                 (run-at-time
                  0 0.02
                  (lambda ()
                    (when (and (= (frame-width) 91)
                               (= (frame-pixel-height) original-height)
                               (desktop-font-startup-presented submission))
                      (cancel-timer timer)
                      (kill-emacs 0)))))))
     (error
      (princ (format "DESKTOP-FONT-STARTUP-FAIL %S\n" err) 'external-debugging-output)
      (message "Desktop font startup regression: %S" err)
      (kill-emacs 1)))))
