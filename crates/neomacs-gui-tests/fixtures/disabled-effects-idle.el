;;; disabled-effects-idle.el --- #857 disabled visual effects idle-loop fixture -*- lexical-binding: t -*-

(setq inhibit-startup-screen t)

(defun idle-test-receipt ()
  (let ((file (getenv "NEOMACS_GUI_PRESENTATION_RECEIPT")))
    (when (and file (file-exists-p file))
      (with-temp-buffer
        (insert-file-contents file)
        (when (> (buffer-size) 0)
          (ignore-errors (read (current-buffer))))))))

(defun idle-test-valid-presented-p (receipt after)
  (and receipt
       (eq (plist-get receipt :outcome) 'presented)
       (> (plist-get receipt :submission) after)))

(defun idle-test-write-silent (name text)
  "Write TEXT to NAME without emitting echo-area messages or redisplays."
  (let ((inhibit-message t)
        (message-log-max nil)
        (path (expand-file-name name (getenv "NEOMACS_IDLE_TEST_DIR"))))
    (with-temp-buffer
      (insert text)
      (write-region (point-min) (point-max) path nil 'nomessage))))

(defun idle-test-flag-p (name)
  (file-exists-p (expand-file-name name (getenv "NEOMACS_IDLE_TEST_DIR"))))

(defvar idle-test-stage 'init)
(defvar idle-test-last-seen-sub 0)
(defvar idle-test-stable-ticks 0)
(defvar idle-test-moves-done 0)
(defvar idle-test-timer nil)

(let ((buf (get-buffer-create "*disabled-effects-idle-test*")))
  (switch-to-buffer buf)
  (dotimes (i 120)
    (insert (format "Line %03d: disabled effects idle-loop acceptance fixture.\n" i)))
  (goto-char (point-min))
  (set-window-start (selected-window) (point-min))
  (when (fboundp 'blink-cursor-mode) (blink-cursor-mode -1))
  (when (fboundp 'neomacs--effect-set)
    (neomacs--effect-set 'cursor-motion :enabled nil)
    (neomacs--effect-set 'cursor-color-cycle :enabled nil)
    (neomacs--effect-set 'cursor-comet :enabled nil))
  (redisplay t))

(defun idle-test-step ()
  (condition-case err
      (let ((receipt (idle-test-receipt)))
        (pcase idle-test-stage
          ('init
           (when (idle-test-valid-presented-p receipt 0)
             (let ((sub (plist-get receipt :submission)))
               (if (> sub idle-test-last-seen-sub)
                   (setq idle-test-last-seen-sub sub
                         idle-test-stable-ticks 0)
                 (setq idle-test-stable-ticks (1+ idle-test-stable-ticks)))
               (when (>= idle-test-stable-ticks 5)
                 (setq idle-test-stage 'wait-enable)
                 (idle-test-write-silent "ready" "ok\n")))))
          ('wait-enable
           (cond
            ((idle-test-flag-p "quit")
             (cancel-timer idle-test-timer)
             (kill-emacs 0))
            ((idle-test-flag-p "enable-go")
             (unless (boundp 'neomacs-effect-cursor-comet)
               (error "neomacs-effect-cursor-comet option is not defined"))
             ;; Use a long fade (5000 ms) so the comet trail is guaranteed to
             ;; remain non-empty while the test disables effects mid-animation.
             (customize-set-variable 'neomacs-effect-cursor-comet
                                     '(:enabled t :fade-ms 5000 :trail-length 8))
             (customize-set-variable 'neomacs-cursor-motion-enabled t)
             (customize-set-variable 'neomacs-cursor-motion-duration 0.4)
             (unless (plist-get (neomacs-effect-get 'cursor-comet) :enabled)
               (error "cursor-comet did not enable: %S"
                      (neomacs-effect-get 'cursor-comet)))
             (unless (plist-get (neomacs-effect-get 'cursor-motion) :enabled)
               (error "cursor-motion did not enable: %S"
                      (neomacs-effect-get 'cursor-motion)))
             (setq idle-test-moves-done 0
                   idle-test-stage 'move-cursor)
             (forward-line 8))))
          ('move-cursor
           ;; Perform 3 spaced cursor jumps so cursor_comet.positions fills
           ;; with multiple trail points before we signal the harness.
           (when (idle-test-valid-presented-p receipt idle-test-last-seen-sub)
             (setq idle-test-last-seen-sub (plist-get receipt :submission)
                   idle-test-moves-done (1+ idle-test-moves-done))
             (if (< idle-test-moves-done 3)
                 (forward-line 8)
               (setq idle-test-stage 'wait-disable)
               (idle-test-write-silent "effects-active" "ok\n"))))
          ('wait-disable
           (cond
            ((idle-test-flag-p "quit")
             (cancel-timer idle-test-timer)
             (kill-emacs 0))
            ((idle-test-flag-p "disable-go")
             ;; Disable every effect we enabled mid-animation while the comet
             ;; trail is still non-empty.
             (customize-set-variable 'neomacs-effect-cursor-comet '(:enabled nil))
             (customize-set-variable 'neomacs-cursor-motion-enabled nil)
             (when (fboundp 'neomacs--effect-set)
               (neomacs--effect-set 'cursor-color-cycle :enabled nil))
             (when (plist-get (neomacs-effect-get 'cursor-comet) :enabled)
               (error "cursor-comet did not disable: %S"
                      (neomacs-effect-get 'cursor-comet)))
             (when (plist-get (neomacs-effect-get 'cursor-motion) :enabled)
               (error "cursor-motion did not disable: %S"
                      (neomacs-effect-get 'cursor-motion)))
             (setq idle-test-stage 'wait-quit)
             (idle-test-write-silent "disabled" "ok\n"))))
          ('wait-quit
           (when (idle-test-flag-p "quit")
             (cancel-timer idle-test-timer)
             (kill-emacs 0)))))
    (error
     (princ (format "IDLE-TEST-ERROR %S\n" err) 'external-debugging-output)
     (cancel-timer idle-test-timer)
     (kill-emacs 1))))

(setq idle-test-timer (run-at-time 0 0.02 #'idle-test-step))
