;;; scroll-bar-gui-oracle.el --- GNU/Neomacs GUI scroll-bar oracle -*- lexical-binding: t -*-
;;
;; One `(NAME . STATE)` line per case, written to the path named by
;; NEOMACS_SCROLL_BAR_ORACLE_RESULT.  The test runs this fixture under both
;; editors on the same display and compares the two files line by line, so
;; every case must be a pure function of the editor's scroll-bar model.
;;
;; STATE is
;;   (WINDOW-SCROLL-BAR-WIDTH WINDOW-SCROLL-BAR-HEIGHT (window-scroll-bars)
;;    FRAME-SCROLL-BAR-WIDTH FRAME-SCROLL-BAR-HEIGHT
;;    vertical-scroll-bars horizontal-scroll-bars
;;    scroll-bar-width scroll-bar-height
;;    FRAME-CHAR-WIDTH)
;;
;; The last element is diagnostic: the COLUMNS/LINES slots of
;; `window-scroll-bars' divide the pixel sizes by it, so a character-width
;; difference between the two runs must be visible rather than mysterious.

(defvar neo-scroll-bar-oracle-results nil)

(defun neo-scroll-bar-oracle--state ()
  (list (window-scroll-bar-width)
        (window-scroll-bar-height)
        (window-scroll-bars)
        (frame-scroll-bar-width)
        (frame-scroll-bar-height)
        (frame-parameter nil 'vertical-scroll-bars)
        (frame-parameter nil 'horizontal-scroll-bars)
        (frame-parameter nil 'scroll-bar-width)
        (frame-parameter nil 'scroll-bar-height)
        (frame-char-width)))

(defun neo-scroll-bar-oracle--record (name thunk)
  (push (cons name
              (condition-case error-data
                  (funcall thunk)
                (error error-data)))
        neo-scroll-bar-oracle-results))

;; Control: the opening state of a GUI frame.  The bar width is the toolkit
;; default (16), not the character cell, and `frame-parameter' reports that
;; configured width even though the frame never named one.
(neo-scroll-bar-oracle--record
 'default-gui
 (lambda () (neo-scroll-bar-oracle--state)))

;; Control: which side the bar is on must agree; only the width fields differ.
(neo-scroll-bar-oracle--record
 'vertical-left
 (lambda ()
   (set-frame-parameter nil 'vertical-scroll-bars 'left)
   (redisplay t)
   (neo-scroll-bar-oracle--state)))

;; GNU normalizes the frame parameter to `t' and sizes the horizontal bar
;; from the toolkit default (16), not the character height.
(neo-scroll-bar-oracle--record
 'horizontal-bottom
 (lambda ()
   (set-frame-parameter nil 'horizontal-scroll-bars 'bottom)
   (redisplay t)
   (neo-scroll-bar-oracle--state)))

;; The same height parameter, read back: `frame-scroll-bar-height' answers
;; from the effective horizontal area (7 here, FRAME_SCROLL_BAR_AREA_HEIGHT),
;; while the frame parameter keeps reporting the configured 7.
(neo-scroll-bar-oracle--record
 'scroll-bar-height-7
 (lambda ()
   (set-frame-parameter nil 'scroll-bar-height 7)
   (redisplay t)
   (neo-scroll-bar-oracle--state)))

;; Control: an explicit pixel width is stored and honored by both.  The
;; horizontal bar is cleared first so the two divergences above (its
;; normalization and the height stub) do not bleed into this case.
(neo-scroll-bar-oracle--record
 'scroll-bar-width-20
 (lambda ()
   (set-frame-parameter nil 'horizontal-scroll-bars nil)
   (set-frame-parameter nil 'scroll-bar-width 20)
   (redisplay t)
   (neo-scroll-bar-oracle--state)))

;; Control: the window-local override, GNU signature
;; (WINDOW WIDTH VERTICAL-TYPE HEIGHT HORIZONTAL-TYPE &optional PERSISTENT).
(neo-scroll-bar-oracle--record
 'window-local-width-4
 (lambda ()
   (set-window-scroll-bars (selected-window) 4 'right nil 'bottom)
   (redisplay t)
   (neo-scroll-bar-oracle--state)))

(let ((path (getenv "NEOMACS_SCROLL_BAR_ORACLE_RESULT")))
  (with-temp-file path
    (dolist (result (reverse neo-scroll-bar-oracle-results))
      (prin1 result (current-buffer))
      (terpri (current-buffer)))))

(kill-emacs 0)
