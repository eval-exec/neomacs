;;; child-frame-animation-test.el --- Test child frame animations -*- lexical-binding: t; -*-

;; Test child-frame lifecycle animation: open fade/slide, close fade/slide,
;; slowdown observation, and the off master switch.
;; Usage: emacs -Q -l test/neomacs/child-frame-animation-test.el
;;
;; Tests 1-4 are visually driven; the messages log what to watch for.
;; `neomacs-child-frame-animations-slowdown' stretches everything to
;; watchable lengths, exactly as it does for window animations.

;;; Code:

(require 'neomacs-effects)

(defvar child-frame-animation-test--frames nil
  "List of child frames created during the test.")

(defvar child-frame-animation-test--step 0
  "Current test step for the automated sequence.")

(defun child-frame-animation-test--cleanup ()
  "Delete all test child frames and force a redisplay."
  (dolist (f child-frame-animation-test--frames)
    (when (frame-live-p f)
      (delete-frame f)))
  (setq child-frame-animation-test--frames nil)
  (redisplay t))

(defun child-frame-animation-test--make-child (name x y w h)
  "Create a child frame at (X, Y) with size (W, H) and some text."
  (let* ((buf (get-buffer-create (format "*anim-%s*" name)))
         (frame (make-frame
                 `((parent-frame . ,(selected-frame))
                   (left . ,x) (top . ,y)
                   (width . ,w) (height . ,h)
                   (minibuffer . nil)
                   (no-accept-focus . t)
                   (child-frame-border-width . 2)
                   (internal-border-width . 4)
                   (undecorated . t)
                   (visibility . t)))))
    (with-selected-frame frame
      (switch-to-buffer buf)
      (erase-buffer)
      (insert (format "%s\n" name))
      (insert "Watch this popup's\n")
      (insert "appear and disappear.\n"))
    (push frame child-frame-animation-test--frames)
    frame))

;; ============================================================================
;; Test 1: Appear — fade + 8px rise
;; ============================================================================

(defun child-frame-animation-test--appear ()
  "Create child frames and watch them fade/slide in."
  (child-frame-animation-test--cleanup)
  (setq child-frame-animation-test--frames nil)
  (child-frame-animation-test--make-child "appear-1" 60 120 40 8)
  (sit-for 0.2)
  (child-frame-animation-test--make-child "appear-2" 420 240 40 8)
  (message "Test 1: Appear — two popups should fade in, rising ~8px onto their placement"))

;; ============================================================================
;; Test 2: Disappear — fade out
;; ============================================================================

(defun child-frame-animation-test--disappear ()
  "Delete one of two child frames and watch the fade-out."
  (child-frame-animation-test--cleanup)
  (setq child-frame-animation-test--frames nil)
  (let ((doomed (child-frame-animation-test--make-child "doomed" 60 120 40 8)))
    (child-frame-animation-test--make-child "survivor" 420 240 40 8)
    (sit-for 0.4)
    (delete-frame doomed)
    (setq child-frame-animation-test--frames
          (remq doomed child-frame-animation-test--frames))
    (message "Test 2: Disappear — the doomed popup should fade out; the survivor stays")))

;; ============================================================================
;; Test 3: Slowdown — everything at a watchable pace
;; ============================================================================

(defun child-frame-animation-test--slowdown ()
  "Stretch every child-frame animation to a watchable length."
  (setq neomacs-child-frame-animations-slowdown 20.0)
  (child-frame-animation-test--cleanup)
  (setq child-frame-animation-test--frames nil)
  (child-frame-animation-test--make-child "slow-appear" 60 120 40 8)
  (run-at-time 4.0 nil
               (lambda ()
                 (child-frame-animation-test--cleanup)
                 (setq child-frame-animation-test--frames nil)
                 (let ((doomed (child-frame-animation-test--make-child "slow-close" 60 120 40 8)))
                   (sit-for 4.0)
                   (delete-frame doomed)
                   (setq child-frame-animation-test--frames
                         (remq doomed child-frame-animation-test--frames))
                   (message "Test 3: Slowdown — appear and close each took ~3s"))))
  (message "Test 3: Slowdown — watch the 20x popup appear, then the 20x fade-out"))

;; ============================================================================
;; Test 4: Master off switch
;; ============================================================================

(defun child-frame-animation-test--off ()
  "With the master switch off, popups must appear and vanish instantly."
  (setq neomacs-child-frame-animations-off t)
  (child-frame-animation-test--cleanup)
  (setq child-frame-animation-test--frames nil)
  (child-frame-animation-test--make-child "instant" 60 120 40 8)
  (sit-for 0.2)
  (let ((doomed (car child-frame-animation-test--frames)))
    (delete-frame doomed)
    (setq child-frame-animation-test--frames
          (remq doomed child-frame-animation-test--frames)))
  (message "Test 4: Off — popup appeared and vanished with no fade")
  ;; Restore.
  (setq neomacs-child-frame-animations-off nil))

;; ============================================================================
;; Automated sequence
;; ============================================================================

(defun child-frame-animation-test--run-all ()
  "Run every step in sequence."
  (child-frame-animation-test--appear)
  (run-at-time 3 nil #'child-frame-animation-test--disappear)
  (run-at-time 6 nil #'child-frame-animation-test--slowdown)
  (run-at-time 16 nil #'child-frame-animation-test--off))

(child-frame-animation-test--run-all)

(provide 'child-frame-animation-test)

;;; child-frame-animation-test.el ends here
