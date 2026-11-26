;;; presentation-pacing-receipts.el --- Receipt-driven presentation & pacing test -*- lexical-binding: t -*-

(setq inhibit-startup-screen t)

(defun pacing-test-receipt ()
  (let ((file (getenv "NEOMACS_GUI_PRESENTATION_RECEIPT")))
    (when (and file (file-exists-p file))
      (with-temp-buffer
        (insert-file-contents file)
        (when (> (buffer-size) 0)
          (ignore-errors (read (current-buffer))))))))

(defun pacing-test-valid-presented-p (receipt after)
  (and receipt
       (eq (plist-get receipt :outcome) 'presented)
       (> (plist-get receipt :submission) after)
       (integerp (plist-get receipt :clock-id))
       (natnump (plist-get receipt :seconds))
       (natnump (plist-get receipt :nanoseconds))
       (< (plist-get receipt :nanoseconds) 1000000000)))

(defun pacing-test-write-silent (name text)
  "Write TEXT to NAME without emitting echo-area messages or redisplays."
  (let ((inhibit-message t)
        (message-log-max nil)
        (path (expand-file-name name (getenv "NEOMACS_PACING_TEST_DIR"))))
    (with-temp-buffer
      (insert text)
      (write-region (point-min) (point-max) path nil 'nomessage))))

(defun pacing-test-flag-p (name)
  (file-exists-p (expand-file-name name (getenv "NEOMACS_PACING_TEST_DIR"))))

(defvar pacing-test-stage 'init)
(defvar pacing-test-last-sub 0)
(defvar pacing-test-moves-done 0)
(defvar pacing-test-stable-ticks 0)
(defvar pacing-test-last-seen-sub 0)
(defvar pacing-test-phase1-receipts nil)
(defvar pacing-test-phase2-receipts nil)
(defvar pacing-test-timer nil)

(let ((buf (get-buffer-create "*pacing-test*")))
  (switch-to-buffer buf)
  (dotimes (i 120)
    (insert (format "Line %03d: presentation pacing acceptance fixture.\n" i)))
  (goto-char (point-min))
  (set-window-start (selected-window) (point-min))
  (when (fboundp 'blink-cursor-mode) (blink-cursor-mode -1))
  (when (fboundp 'neomacs-effects--set)
    (neomacs-effects--set 'cursor-motion :enabled nil)
    (neomacs-effects--set 'cursor-color-cycle :enabled nil))
  (redisplay t))

(defun pacing-test-step ()
  (condition-case err
      (let ((receipt (pacing-test-receipt)))
        (pcase pacing-test-stage
          ('init
           (when (pacing-test-valid-presented-p receipt 0)
             (let ((sub (plist-get receipt :submission)))
               (if (> sub pacing-test-last-seen-sub)
                   (setq pacing-test-last-seen-sub sub
                         pacing-test-stable-ticks 0)
                 (setq pacing-test-stable-ticks (1+ pacing-test-stable-ticks)))
               ;; Wait until initial startup commits settle so no in-flight
               ;; startup commit leaks into the test's counter window.
               (when (>= pacing-test-stable-ticks 5)
                 (setq pacing-test-last-sub sub
                       pacing-test-stage 'wait-command)
                 (pacing-test-write-silent "ready" "ok\n")))))
          ('wait-command
           (cond
            ((pacing-test-flag-p "quit")
             (cancel-timer pacing-test-timer)
             (kill-emacs 0))
            ((and (pacing-test-flag-p "phase1-go")
                  (not (pacing-test-flag-p "phase1-done")))
             (setq pacing-test-moves-done 0
                   pacing-test-phase1-receipts nil
                   pacing-test-stage 'phase1-move)
             (forward-line 1))
            ((and (pacing-test-flag-p "phase2-setup")
                  (not (pacing-test-flag-p "phase2-done")))
             (setq pacing-test-last-seen-sub pacing-test-last-sub
                   pacing-test-stable-ticks 0
                   pacing-test-stage 'wait-phase2-config)
             (when (fboundp 'neomacs-effects--set)
               (neomacs-effects--set 'cursor-motion :enabled t)
               (neomacs-effects--set 'cursor-motion :duration 0.35)
               (neomacs-effects--set 'cursor-color-cycle :enabled nil)))))
          ('phase1-move
           (when (pacing-test-valid-presented-p receipt pacing-test-last-sub)
             (push receipt pacing-test-phase1-receipts)
             (setq pacing-test-last-sub (plist-get receipt :submission)
                   pacing-test-moves-done (1+ pacing-test-moves-done))
             (if (< pacing-test-moves-done 10)
                 (forward-line 1)
               (pacing-test-write-silent
                "receipts-phase1.sexp"
                (mapconcat (lambda (r) (format "%S" r))
                           (nreverse pacing-test-phase1-receipts)
                           "\n"))
               (setq pacing-test-stage 'wait-command)
               (pacing-test-write-silent "phase1-done" "ok\n"))))
          ('wait-phase2-config
           ;; Wait for the neomacs-effects--set config-sync redraw to present
           ;; and settle before signaling phase2-ready, so the config-sync
           ;; submission cannot satisfy Phase 2 ahead of (forward-line 20).
           (when (pacing-test-valid-presented-p receipt pacing-test-last-sub)
             (let ((sub (plist-get receipt :submission)))
               (if (> sub pacing-test-last-seen-sub)
                   (setq pacing-test-last-seen-sub sub
                         pacing-test-stable-ticks 0)
                 (setq pacing-test-stable-ticks (1+ pacing-test-stable-ticks)))
               (when (>= pacing-test-stable-ticks 5)
                 (setq pacing-test-last-sub sub
                       pacing-test-stage 'wait-phase2-go)
                 (pacing-test-write-silent "phase2-ready" "ok\n")))))
          ('wait-phase2-go
           (when (pacing-test-flag-p "phase2-go")
             (setq pacing-test-stage 'phase2-wait-settle
                   pacing-test-stable-ticks 0
                   pacing-test-phase2-receipts nil
                   pacing-test-last-seen-sub pacing-test-last-sub)
             (forward-line 20)))
          ('phase2-wait-settle
           (when (pacing-test-valid-presented-p receipt pacing-test-last-sub)
             (let ((sub (plist-get receipt :submission)))
               (if (> sub pacing-test-last-seen-sub)
                   (progn
                     (push receipt pacing-test-phase2-receipts)
                     (setq pacing-test-last-seen-sub sub
                           pacing-test-stable-ticks 0))
                 (setq pacing-test-stable-ticks (1+ pacing-test-stable-ticks)))
               (when (>= pacing-test-stable-ticks 12)
                 (pacing-test-write-silent
                  "receipts-phase2.sexp"
                  (mapconcat (lambda (r) (format "%S" r))
                             (nreverse pacing-test-phase2-receipts)
                             "\n"))
                 (setq pacing-test-stage 'wait-command)
                 (pacing-test-write-silent "phase2-done" "ok\n")))))))
    (error
     (princ (format "PACING-TEST-ERROR %S\n" err) 'external-debugging-output)
     (cancel-timer pacing-test-timer)
     (kill-emacs 1))))

(setq pacing-test-timer (run-at-time 0 0.02 #'pacing-test-step))
