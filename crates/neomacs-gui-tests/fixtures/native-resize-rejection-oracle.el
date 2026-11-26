;;; native-resize-rejection-oracle.el --- Paired native resize observation -*- lexical-binding: t -*-
(require 'json)
(setq inhibit-startup-screen t)
(blink-cursor-mode -1)
(defvar n607-phase 'baseline)
(defvar n607-initial nil)
(defvar n607-baseline nil)
(defvar n607-fullscreen nil)
(defvar n607-last nil)
(defvar n607-stable 0)
(defvar n607-requests-start nil)
(defvar n607-observations nil)
(defvar n607-results nil)
(defvar n607-timer nil)
(defvar n607-restored nil)
(defvar n607-font-start nil)

(defun n607-snapshot ()
  `((time . ,(float-time))
    (editor_version . ,emacs-version)
    (wayland_display . ,(or (getenv "WAYLAND_DISPLAY") "UNSET"))
    (wayland_socket . ,(or (getenv "WAYLAND_SOCKET") "UNSET"))
    (display . ,(or (getenv "DISPLAY") "UNSET"))
    (columns . ,(frame-width))
    (rows . ,(frame-height))
    (parameter_width . ,(frame-parameter nil 'width))
    (parameter_height . ,(frame-parameter nil 'height))
    (native_width . ,(frame-pixel-width))
    (native_height . ,(frame-pixel-height))
    (char_width . ,(frame-char-width))
    (char_height . ,(frame-char-height))
    (fullscreen . ,(prin1-to-string (frame-parameter nil 'fullscreen)))
    (font . ,(prin1-to-string (frame-parameter nil 'font)))
    (window_system . ,(prin1-to-string window-system))
    (inhibit_policy . ,(prin1-to-string frame-inhibit-implied-resize))))

(defun n607-write (phase data)
  (let ((output (cons (cons 'phase phase) data)))
    (push output n607-results)
    ;; Preserve every transition, including the actual native restoration
    ;; before the font mutation. The Rust comparison is allowed to fail only
    ;; after both editors have completed this same observation sequence.
    (with-temp-file (getenv "NEOMACS_GUI_STATE_JSON")
      (insert (json-encode `((observations . ,(vconcat (reverse n607-results)))))))
    (princ (concat "ASYNC-RESIZE-STATE " (json-encode output) "\n")
           'external-debugging-output)))

(defun n607-geometry (sample)
  (list (alist-get 'native_width sample)
        (alist-get 'native_height sample)
        (alist-get 'columns sample)
        (alist-get 'rows sample)))

(defun n607-tick ()
  (condition-case err
      (let* ((sample (n607-snapshot))
             (geometry (n607-geometry sample)))
        (if (equal geometry n607-last)
            (setq n607-stable (1+ n607-stable))
          (setq n607-last geometry n607-stable 0))
        (pcase n607-phase
          ('baseline
           (when (and (= (alist-get 'columns sample) 91)
                      (> (alist-get 'native_width sample)
                         (alist-get 'native_width n607-initial))
                      (>= n607-stable 3))
             (setq n607-baseline sample n607-phase 'fullscreen n607-stable 0)
             (n607-write "baseline" sample)
             (set-frame-parameter nil 'fullscreen 'fullboth)))
          ('fullscreen
           (when (and (> (alist-get 'native_width sample)
                         (alist-get 'native_width n607-baseline))
                      (>= n607-stable 3))
             (setq n607-fullscreen sample n607-phase 'requests
                   n607-requests-start (float-time) n607-observations nil)
             (n607-write "fullscreen" sample)
             (set-frame-width nil 101)
             (n607-write "request_101_first_immediate" (n607-snapshot))
             (set-frame-width nil 101)
             (n607-write "request_101_second_immediate" (n607-snapshot))))
          ('requests
           (push geometry n607-observations)
           (when (>= (- (float-time) n607-requests-start) 1.0)
             (n607-write "after_duplicate_requests"
                         (append sample
                                 `((native_geometry_unchanged .
                                    ,(if (cl-every
                                          (lambda (observed)
                                            (equal (seq-take observed 2)
                                                   (seq-take (n607-geometry n607-fullscreen) 2)))
                                          n607-observations)
                                         t :json-false)))))
             (setq n607-phase 'restored n607-stable 0)
             (set-frame-parameter nil 'fullscreen nil)))
          ('restored
           (when (and (null (frame-parameter nil 'fullscreen))
                      (= (alist-get 'native_width sample)
                         (alist-get 'native_width n607-baseline))
                      (= (alist-get 'native_height sample)
                         (alist-get 'native_height n607-baseline))
                      (>= n607-stable 3))
             (n607-write "restored_before_font" sample)
             (setq n607-restored sample n607-phase 'font-settle n607-stable 0
                   n607-font-start (float-time))
             (n607-write "font_change_form" '((form . "(set-frame-font \"DejaVu Sans Mono-20\" nil t)")))
             (set-frame-font "DejaVu Sans Mono-20" nil t)
             (n607-write "font_change_immediate" (n607-snapshot))))
          ('font-settle
           (when (and (not (= (alist-get 'char_width sample)
                              (alist-get 'char_width n607-restored)))
                      (>= (- (float-time) n607-font-start) 0.5)
                      (>= n607-stable 5))
             (n607-write "font_change_settled" sample)
             (cancel-timer n607-timer)
             (kill-emacs 0)))))
    (error
     (n607-write "error" `((error . ,(prin1-to-string err))))
     (kill-emacs 2))))

(require 'cl-lib)
(require 'seq)
;; GNU xterm.c:410-448 allows the WM to replace the requested size. In
;; dispnew.c:6710-6727 an applied configure replaces and consumes pending
;; dimensions; xterm.c:27179-27181 then preserves the observed grid on a font
;; change. Do not wait for frame-width=91 after restoration: that would hide
;; the stale-intent regression behind a timeout instead of recording it.
(run-at-time 45 nil (lambda () (n607-write "timeout" (n607-snapshot)) (kill-emacs 3)))
(run-at-time
 0.2 nil
 (lambda ()
   (setq n607-initial (n607-snapshot))
   (n607-write "initial" n607-initial)
   (set-frame-width nil 91)
   (setq n607-timer (run-at-time 0 0.05 #'n607-tick))))
