;;; desktop-font-startup.el --- Desktop default font regression -*- lexical-binding: t -*-
(setq native-comp-jit-compilation nil native-comp-enable-subr-trampolines nil)
(dolist (variable '("NEOMACS_GUI_SVG_LIB_DIR" "NEOMACS_GUI_SVG_TAG_MODE_DIR"))
  (let ((directory (getenv variable)))
    (unless directory (error "Set %s to the installed package directory" variable))
    (add-to-list 'load-path directory)))
(require 'svg-tag-mode)

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
         (unless (and (null face-remapping-alist)
                      (equal (face-attribute 'default :family) "Ubuntu Mono")
                      (= (window-font-width) 9) (= (window-font-height) 18))
           (error "Startup ignored desktop font: %S, cell %S, remapping %S"
                  (face-attribute 'default :family)
                  (list (window-font-width) (window-font-height)) face-remapping-alist))
         (let* ((tag (svg-tag-make "TODO" :margin 0 :radius 8 :padding 0.07
                                   :height 0.71 :ascent 79
                                   :font-family "DejaVu Sans Mono" :font-size 7))
                (data (plist-get (cdr tag) :data)))
           ;; Independently captured from GNU at 96 DPI, Ubuntu Mono 13pt.
           (unless (string-match "<svg width=\"36\\.63\" height=\"12\\.78\"" data)
             (error "Startup SVG differs from GNU: %S" data))
           (insert-image tag)
           (insert " Desktop monospace font selected before user configuration.\n"))
         (when (fboundp 'neomacs--write-frame-snapshot)
           (neomacs--write-frame-snapshot
            (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON") t 'json))
         (run-at-time 1 nil (lambda () (kill-emacs 0))))
     (error (message "Desktop font startup regression: %S" err) (kill-emacs 1)))))
