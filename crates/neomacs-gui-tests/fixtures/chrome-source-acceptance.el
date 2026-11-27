;;; chrome-source-acceptance.el --- Late chrome source mutation -*- lexical-binding: t; -*-
(require 'json)
(unless (display-graphic-p) (error "#832 requires a graphical frame"))
(setq debug-on-error t)
(blink-cursor-mode -1)
(menu-bar-mode -1)
(when (fboundp 'tool-bar-mode) (tool-bar-mode -1))
(setq cursor-type 'box)
(set-background-color "white")
(set-foreground-color "black")
(switch-to-buffer (get-buffer-create "*832-chrome-source*"))
(erase-buffer)
(insert "agent shell body\nsecond line\n")
(goto-char 1)
(defvar neo-chrome-source-window (selected-window))
(defvar neo-chrome-source-evals 0)
(defvar neo-chrome-source-overlay (make-overlay 1 6))
(defvar neo-chrome-source-samples nil)
(defvar neo-chrome-source-timers nil)
(setq-local mode-line-format nil)
(setq-local tab-line-format nil)
(setq-local header-line-format
            '((:eval (progn
                       (setq neo-chrome-source-evals (1+ neo-chrome-source-evals))
                       (overlay-put neo-chrome-source-overlay 'face
                                    (if (= (% neo-chrome-source-evals 2) 0)
                                        'bold 'italic))
                       "Agent shell"))))

(defun neo-chrome-source-bool (value) (if value t :json-false))

(defun neo-chrome-source-write (&optional failure)
  (with-temp-file (getenv "NEOMACS_CHROME_SOURCE_RESULT")
    (insert (json-encode
             `((error . ,failure)
               (samples . ,(vconcat (reverse neo-chrome-source-samples))))))))

(defun neo-chrome-source-finish (code)
  (dolist (timer neo-chrome-source-timers) (cancel-timer timer))
  (delete-overlay neo-chrome-source-overlay)
  (kill-emacs code))

(defun neo-chrome-source-sample ()
  (condition-case err
      (progn
        (unless (window-live-p neo-chrome-source-window)
          (error "Owned fixture window died"))
        (with-selected-window neo-chrome-source-window
          (redisplay t)
          (let* ((before neo-chrome-source-evals)
                 (cursor (window-cursor-info))
                 (line-height (window-line-height))
                 ;; GNU's VPOS/YPOS count the header row (window.c:2153-2156).
                 (line-heights (list line-height (window-line-height 0)
                                     (window-line-height 1)
                                     (window-line-height 'header-line)))
                 (dimensions (window-lines-pixel-dimensions))
                 (end-cached (window-end nil nil))
                 (end-update (window-end nil t))
                 (after-matrix neo-chrome-source-evals)
                 (visible (pos-visible-in-window-p 1))
                 (after-fresh neo-chrome-source-evals)
                 ;; Do not round-trip posn-at-point's text-relative Y through
                 ;; posn-at-x-y: that function's Y includes header/tab chrome.
                 (position (posn-at-point 8))
                 (hit (posn-at-x-y 1 (+ (window-header-line-height)
                                          (window-tab-line-height) 1))))
            (push
             `((survived . t)
               (window_system . ,(symbol-name window-system))
               (header_evaluations . ,before)
               (body_seen . ,(neo-chrome-source-bool
                              (equal (buffer-string) "agent shell body\nsecond line\n")))
               (cursor_present . ,(neo-chrome-source-bool
                                   (and (vectorp cursor) (> (length cursor) 0)
                                        (aref cursor 0))))
               (line_height_present . ,(neo-chrome-source-bool line-height))
               (line_dimensions_present . ,(neo-chrome-source-bool dimensions))
               (end_cached . ,end-cached) (end_update . ,end-update)
               (ends_match . ,(neo-chrome-source-bool
                               (and (numberp end-cached) (> end-cached 1)
                                    (equal end-cached end-update))))
               (matrix_query_evaluations . ,(- after-matrix before))
               (fresh_query_visible . ,(neo-chrome-source-bool visible))
               (fresh_query_evaluations . ,(- after-fresh after-matrix))
               (position_visible . ,(neo-chrome-source-bool position))
               (body_hit . ,(neo-chrome-source-bool
                             (and hit (equal (posn-point hit) 1))))
               (cursor_raw . ,(prin1-to-string cursor))
               (line_height_raw . ,(prin1-to-string line-height))
               (line_heights . ,(prin1-to-string line-heights))
               (dimensions_raw . ,(prin1-to-string dimensions))
               (position_raw . ,(prin1-to-string position))
               (hit_raw . ,(prin1-to-string hit)))
             neo-chrome-source-samples)))
        (neo-chrome-source-write))
    (error
     (neo-chrome-source-write (prin1-to-string err))
     (neo-chrome-source-finish 1))))

(defun neo-chrome-source-complete ()
  (condition-case err
      (progn
        ;; GNU GTK's PNG export is the oracle paint observation. Neo's actual
        ;; surface readback is written by the existing GUI harness/runtime.
        (when (and (eq window-system 'x) (fboundp 'x-export-frames))
          (let ((coding-system-for-write 'no-conversion))
            (with-temp-file (getenv "NEOMACS_DEBUG_SURFACE_READBACK_PNG")
              (set-buffer-multibyte nil)
              (insert (x-export-frames nil 'png)))))
        (neo-chrome-source-write)
        (neo-chrome-source-finish 0))
    (error
     (neo-chrome-source-write (prin1-to-string err))
     (neo-chrome-source-finish 1))))

;; The first sample is taken while this file loads, straight after its own
;; `(redisplay t)': GNU's current matrix is complete then, so no sample may
;; depend on when later redisplays happen to land.
(neo-chrome-source-sample)
(push (run-at-time 0.5 nil #'neo-chrome-source-sample) neo-chrome-source-timers)
(push (run-at-time 1.0 nil #'neo-chrome-source-sample) neo-chrome-source-timers)
(push (run-at-time 1.5 nil #'neo-chrome-source-complete) neo-chrome-source-timers)
(push (run-at-time 15 nil (lambda ()
                          (neo-chrome-source-write "fixture deadline")
                          (neo-chrome-source-finish 2)))
      neo-chrome-source-timers)
