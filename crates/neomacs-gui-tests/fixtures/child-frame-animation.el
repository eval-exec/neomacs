;;; child-frame-animation.el --- Child-frame fade verification -*- lexical-binding: t -*-

;; Drives the child-frame lifecycle animation end to end for the GUI test:
;; a popup fades in over three seconds (slowdown 20, the clamp ceiling, on
;; the 150ms default), holds, is deleted, and fades out over another three
;; seconds. Phase transitions are published to a JSON state file the test
;; polls; the test samples the compositor's output at wall-clock instants
;; inside each fade.

(require 'json)

;; Long fades make the pixel series measurable from the harness: each 150ms
;; default becomes 3.0s at this multiplier — the clamp ceiling
;; (`MAX_SLOWDOWN'), so anything above 20.0 behaves as 20.0. The values are
;; declared here so the variables exist before any editor display exists, and
;; re-pushed inside `child-frame-animation-setup' — a push issued during
;; fixture load races the render thread's own startup applications and is
;; overwritten by them, so the effective values must be pushed once the
;; display host is live.
(defvar child-frame-animation-slowdown 20.0)
(defvar child-frame-animation-open-slide-pixels 0.0)
(defvar child-frame-animation-open-scale-from 0.6)
(defvar child-frame-animation-close-scale-from 0.6)
(defvar child-frame-animation-resize-enabled t)
(defvar child-frame-animation-movement-enabled t)

(defvar child-frame-animation-popup-size-before 0)
(defvar child-frame-animation-popup-size-after 0)
(defvar child-frame-animation-sample 0)
(defvar child-frame-animation-popup nil)
(defvar child-frame-animation-phases nil)

(defun child-frame-animation-write (phase)
  (setq child-frame-animation-sample (1+ child-frame-animation-sample)
        child-frame-animation-phases (cons phase child-frame-animation-phases))
  (with-temp-file (getenv "NEOMACS_GUI_ANIMATION_STATE_JSON")
    (insert (json-encode
             `((sample . ,child-frame-animation-sample)
               (phase . ,phase)
               ;; Phases accumulate: the test's capture cadence can outrun
               ;; the timers, and a phase overwritten before it is polled
               ;; must still count as reached.
               (phases . ,(reverse child-frame-animation-phases))
               (slowdown . ,(plist-get (neomacs-effect-get 'child-frame-animations)
                                       :slowdown))
               (popup-width-before . ,child-frame-animation-popup-size-before)
               (popup-width-after . ,child-frame-animation-popup-size-after)
               (snapshot-env . ,(getenv "NEOMACS_GUI_ANIMATION_SNAPSHOT_JSON")))))))

(defun child-frame-animation-setup ()
  (customize-set-variable
   'neomacs-child-frame-animations-slowdown child-frame-animation-slowdown)
  ;; Keep the popup exactly on its final rect while it fades, so every pixel
  ;; sample reads one fixed region. The slide itself is pinned by unit tests.
  (customize-set-variable
   'neomacs-child-frame-open-slide-pixels child-frame-animation-open-slide-pixels)
  ;; The pop: both fades scale the frame between 60% and 100% of its
  ;; settled size, anchored at its top-left, so the pixel test can watch
  ;; the frame's right edge sweep across a strip that the settled frame
  ;; covers and the start scale does not.
  (customize-set-variable
   'neomacs-child-frame-open-scale-from child-frame-animation-open-scale-from)
  (customize-set-variable
   'neomacs-child-frame-close-scale-from child-frame-animation-close-scale-from)
  ;; The resize content crossfade: a size-changing update crossfades the
  ;; previous presentation's picture into the new one instead of snapping.
  (customize-set-variable
   'neomacs-child-frame-resize-enabled child-frame-animation-resize-enabled)
  ;; The anchor-track drift: a re-anchored popup glides to its new
  ;; placement instead of jumping.
  (customize-set-variable
   'neomacs-child-frame-movement-enabled child-frame-animation-movement-enabled)
  (switch-to-buffer (get-buffer-create "*child-frame-animation-parent*"))
  (insert "parent content line\n")
  ;; Publish before any popup exists: the test takes its background
  ;; reference over the popup's future rectangle at this point.
  (child-frame-animation-write "parent-ready")
  (setq child-frame-animation-popup
        (make-frame `((parent-frame . ,(selected-frame))
                      (minibuffer . nil)
                      (visibility . t)
                      (left . (+ 240)) (top . (+ 180))
                      (width . (text-pixels . 280)) (height . (text-pixels . 140))
                      (background-color . "red")
                      (internal-border-width . 4)
                      (child-frame-border-width . 2)
                      (undecorated . t)
                      (menu-bar-lines . 0) (tool-bar-lines . 0) (tab-bar-lines . 0))))
  (set-window-buffer (frame-root-window child-frame-animation-popup)
                     (get-buffer-create "*child-frame-animation-popup*"))
  (with-current-buffer "*child-frame-animation-popup*"
    (erase-buffer)
    (insert "FADE POPUP\nwhite on red text\n"))
  (child-frame-animation-write "created")
  ;; At slowdown 20 both fades run 3s. Timer positions leave the settled
  ;; window (3.0-7.0s) wide enough for a ~1s compositor capture to land
  ;; inside it, and the pruned window wide enough for the final capture to
  ;; reach a live editor.
  (run-at-time 3.5 nil
               (lambda ()
                 (child-frame-animation-write "settled")
                 ;; Publish the placement too, so the test can verify the
                 ;; popup really is where the crop looks. Timer errors are
                 ;; demoted to the echo area and lost with the session, so
                 ;; land any failure in the state file instead.
                 (condition-case err
                     (neomacs--write-frame-snapshot
                      (getenv "NEOMACS_GUI_ANIMATION_SNAPSHOT_JSON") t 'json)
                   (error (child-frame-animation-write
                           (format "snapshot-error %S" err))))))
  ;; Grow the popup mid-life: the size-changing update crossfades the old
  ;; picture into the bigger one over the resize slot's 3s curve. Placed
  ;; after the drift settles so the two windows do not overlap.
  (run-at-time 12.0 nil
               (lambda ()
                 ;; The phase goes out first: the crossfade begins when the
                 ;; render thread ingests the resized payload, which races
                 ;; this write, and the test's first capture must land
                 ;; inside the tween rather than after it.
                 (child-frame-animation-write "resized")
                 (setq child-frame-animation-popup-size-before
                       (frame-pixel-width child-frame-animation-popup))
                 (set-frame-size child-frame-animation-popup 400 180 t)
                 ;; The resized payload reaches the render thread only
                 ;; through a redisplay that re-publishes the child frame.
                 (redisplay t)
                 (setq child-frame-animation-popup-size-after
                       (frame-pixel-width child-frame-animation-popup))))
  ;; Re-anchor mid-life: the movement slot's drift glides the popup from
  ;; (240,180) to (400,300) over the spring's slowed settle (~6.6s, most of
  ;; the travel in the first three).
  (run-at-time 5.0 nil
               (lambda ()
                 (child-frame-animation-write "reanchored")
                 (modify-frame-parameters
                  child-frame-animation-popup
                  '((left . (+ 400)) (top . (+ 300))))
                 ;; The re-anchored payload reaches the render thread only
                 ;; through a redisplay that re-publishes the child frame.
                 (redisplay t)))
  (run-at-time 16.0 nil
               (lambda ()
                 (delete-frame child-frame-animation-popup)
                 (child-frame-animation-write "deleted")))
  ;; The close fade ends at 19.0s; the dying entry is pruned right after.
  (run-at-time 19.5 nil (lambda () (child-frame-animation-write "pruned")))
  (run-at-time 21.0 nil (lambda () (kill-emacs 0))))

;; Let the initial native configure establish the parent dimensions first.
(run-at-time 1.5 nil #'child-frame-animation-setup)

;;; child-frame-animation.el ends here
