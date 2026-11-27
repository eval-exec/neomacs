;;; cursor-row-walk.el --- Cursor row across a C-n walk -*- lexical-binding: t; -*-

;; The #855 report: on line 98, C-n to line 99 draws the cursor at the start
;; of line 100; a second C-n puts it on line 100 at the right column. Each case
;; builds a 120-line buffer whose line 99 has one shape a Doom buffer can give
;; it, then walks 98 -> 99 -> 100 with `next-line' and records, after each
;; settled redisplay, the cursor GNU and Neomacs both expose to Lisp:
;;
;;   CURSOR-ROW-WALK CASE STEP LINE COLUMN START-LINE LINE-HEIGHT Y-POSN INFO
;;
;; START-LINE is the line of `window-start', LINE-HEIGHT is
;; `default-line-height', Y-POSN is the posn-at-point Y and INFO is
;; `window-cursor-info' (`[TYPE X Y W H ASCENT]'); both Ys are relative to
;; the text area. The same stimulus runs on both editors, and the harness
;; compares the lines in rows of LINE-HEIGHT, so a font that differs between
;; the editors' hosts does not decide the comparison.
;;
;; Markers go to stderr first, then to NEOMACS_COMMIT_RESULT_FILE, which is
;; the copy the harness reads. `create-lockfiles' is nil so two concurrent
;; runs can never meet a lock.
(defun neo-row-walk-emit (line)
  "Write LINE to stderr, then append it to NEOMACS_COMMIT_RESULT_FILE."
  (princ (concat line "\n") 'external-debugging-output)
  (let ((result-file (getenv "NEOMACS_COMMIT_RESULT_FILE")))
    (unless result-file (error "NEOMACS_COMMIT_RESULT_FILE is unset"))
    (let ((coding-system-for-write 'utf-8-unix)
          (create-lockfiles nil))
      (write-region (concat line "\n") nil result-file 'append 'silent))))

(defun neo-row-walk-fail (err)
  "Report ERR and terminate; a failing report still reaches `kill-emacs'."
  (ignore-errors (neo-row-walk-emit (format "COMMIT-EDITING-FAIL %S" err)))
  (kill-emacs 1))

(defun neo-row-walk-line-99 (shape)
  "The text of line 99 for SHAPE."
  (let ((width (window-body-width)))
    (pcase shape
      ('empty "")
      ('short "ab")
      ;; One character, so a C-n with goal column 5 lands on its newline.
      ('one-char "x")
      ;; One space: trailing whitespace, which Doom shows by default.
      ('one-space " ")
      ;; A lone closing paren, as in the #855 reporter's workspace autosave.
      ('one-paren ")")
      ('exact-width (make-string width ?x))
      ('width-minus-one (make-string (1- width) ?x))
      ('wrapped (make-string (+ width (/ width 2)) ?w))
      ;; Point lands on column 5, a glyph taller or shorter than the font
      ;; the rest of the row uses: `baseline - ascent' must still be the row.
      ('tall-glyph (concat "abcde" (propertize "T" 'face '(:height 2.0)) "fgh"))
      ('small-glyph (concat "abcde" (propertize "s" 'face '(:height 0.5)) "fgh"))
      (_ (error "Unknown line-99 shape %S" shape)))))

(defconst neo-row-walk-cases
  '((empty-plain empty nil)
    (empty-numbers empty (display-line-numbers-mode hl-line-mode))
    (short-numbers short (display-line-numbers-mode hl-line-mode))
    (exact-plain exact-width nil)
    (exact-numbers exact-width (display-line-numbers-mode hl-line-mode))
    (minus-one-numbers width-minus-one (display-line-numbers-mode hl-line-mode))
    (wrapped-numbers wrapped (display-line-numbers-mode hl-line-mode))
    (wrapped-visual wrapped (display-line-numbers-mode visual-line-mode))
    (tall-glyph-numbers tall-glyph (display-line-numbers-mode hl-line-mode))
    (small-glyph-numbers small-glyph (display-line-numbers-mode hl-line-mode))
    ;; Shapes that move the text area's origin or its rows' pitch, where a
    ;; glyph-derived cursor is most likely to disagree with layout's row.
    (empty-tab-bar empty (tab-bar-mode display-line-numbers-mode hl-line-mode))
    (empty-header-line empty (neo-row-walk-header-line display-line-numbers-mode hl-line-mode))
    (empty-line-spacing empty (neo-row-walk-line-spacing display-line-numbers-mode hl-line-mode))
    (short-all-chrome short (tab-bar-mode neo-row-walk-header-line neo-row-walk-line-spacing
                                          display-line-numbers-mode hl-line-mode))
    ;; A mode line taller than a text line, like doom-modeline's: the body
    ;; height stops being a multiple of the line height.
    (empty-tall-mode-line empty (neo-row-walk-tall-mode-line display-line-numbers-mode
                                                             hl-line-mode))
    ;; #855: point lands on the newline of an empty or one-character line in
    ;; the MIDDLE of the window.  No step scrolls, so every move is redrawn
    ;; by the cursor-only path, which the bottom-row cases above never use.
    (mid-empty-plain empty nil middle)
    (mid-empty-hl-line empty (hl-line-mode) middle)
    (mid-empty-numbers empty (display-line-numbers-mode) middle)
    (mid-empty-tall-mode-line empty (neo-row-walk-tall-mode-line) middle)
    (mid-empty-doom empty (display-line-numbers-mode hl-line-mode
                                                     neo-row-walk-tall-mode-line)
                    middle)
    (mid-one-char-plain one-char nil middle)
    (mid-one-char-doom one-char (display-line-numbers-mode hl-line-mode
                                                           neo-row-walk-tall-mode-line)
                       middle)
    (mid-one-space-trailing one-space (neo-row-walk-trailing-whitespace) middle)
    (mid-one-space-doom one-space (neo-row-walk-trailing-whitespace
                                   display-line-numbers-mode hl-line-mode
                                   neo-row-walk-tall-mode-line)
                        middle)
    (mid-one-paren-elisp-doom one-paren (neo-row-walk-elisp
                                         neo-row-walk-trailing-whitespace
                                         display-line-numbers-mode hl-line-mode
                                         neo-row-walk-tall-mode-line)
                              middle))
  "(NAME LINE-99-SHAPE MODES [PLACEMENT]): one buffer per case.
Each MODE is called with 1 in the case's buffer.  PLACEMENT `middle' puts
line 98 in the middle of the window; otherwise it is on the last body row,
so the first C-n scrolls.")

(defun neo-row-walk-elisp (_arg)
  "Edit the case's buffer in `emacs-lisp-mode' with font-lock on."
  (emacs-lisp-mode)
  (font-lock-mode 1))

(defun neo-row-walk-trailing-whitespace (_arg)
  "Show trailing whitespace in the case's buffer, as Doom does."
  (setq show-trailing-whitespace t))

(defun neo-row-walk-header-line (_arg)
  "Give the case's buffer a header line."
  (setq header-line-format "cursor row walk"))

(defun neo-row-walk-tall-mode-line (_arg)
  "Make the mode line 1.4 times the default face's height."
  (set-face-attribute 'mode-line nil :height 1.4))

(defun neo-row-walk-line-spacing (_arg)
  "Give the case's buffer 4 px of extra line spacing."
  (setq line-spacing 4))

(defun neo-row-walk-setup (case)
  "Build CASE's buffer and leave point on line 98, column 5."
  (pcase-let ((`(,name ,shape ,modes ,placement) case))
    (let ((buffer (get-buffer-create (format "row-walk-%s" name))))
      (switch-to-buffer buffer)
      (delete-other-windows)
      (fundamental-mode)
      (setq truncate-lines nil)
      ;; `tab-bar-mode' and the mode-line face are global: each case starts
      ;; without them.
      (tab-bar-mode -1)
      (set-face-attribute 'mode-line nil :height 'unspecified)
      (dolist (mode modes) (funcall mode 1))
      (erase-buffer)
      (dotimes (index 97)
        (insert (format "line %d of the walk\n" (1+ index))))
      (insert "abcdefghij\n")
      (insert (neo-row-walk-line-99 shape) "\n")
      (insert "0123456789\n")
      (dotimes (index 20)
        (insert (format "tail %d\n" (1+ index))))
      (goto-char (point-min))
      (forward-line 97)
      (move-to-column 5)
      (if (eq placement 'middle)
          (recenter)
        ;; Line 98 on the last body row, so the first C-n has to scroll.
        (recenter -1))
      (setq last-command nil)
      (neo-row-walk-emit
       (format "CURSOR-ROW-WALK-WINDOW %s %d %d %d %S %S"
               name (window-body-height) (window-body-height nil t)
               (window-text-height nil t) (window-inside-pixel-edges)
               (list :window (window-pixel-height)
                     :mode-line (window-mode-line-height)
                     :header (window-header-line-height)
                     :tab-line (window-tab-line-height)
                     :frame (frame-pixel-height) :text (frame-text-height)
                     :mini (window-pixel-height (minibuffer-window))))))))

(defun neo-row-walk-record (name step)
  "Emit one settled observation for case NAME at STEP."
  (neo-row-walk-emit
   (format "CURSOR-ROW-WALK %s %s %d %d %d %d %S %S"
           name step (line-number-at-pos) (current-column)
           (line-number-at-pos (window-start))
           (default-line-height)
           (cdr (posn-x-y (posn-at-point)))
           (window-cursor-info))))

(defvar neo-row-walk-deadline nil "The `float-time' after which a wait fails.")

(defun neo-row-walk-after-redisplay (continuation)
  "Redisplay until the cursor is displayed, then call CONTINUATION.
The poll reads point in the selected window's buffer: a chained timer
starts in *scratch* (see `neo-row-walk-note-buffer')."
  (condition-case err
      (progn
        (set-buffer (window-buffer))
        (redisplay t)
        (cond
         ;; Continue in a fresh timer on both editors, so every step starts
         ;; the same way (see `neo-row-walk-note-buffer').
         ((and (window-cursor-info) (posn-at-point))
          (run-at-time 0.05 nil continuation))
         ((> (float-time) neo-row-walk-deadline)
          (error "Readiness expired: cursor geometry never became available"))
         (t (run-at-time 0.05 nil #'neo-row-walk-after-redisplay continuation))))
    (error (neo-row-walk-fail err))))

;; Neomacs also logs where the cursor is DRAWN (`neomacs::cursor_geometry',
;; stage=draw), which `window-cursor-info' cannot show: the renderer places
;; the cursor from the glyph in its slot, not from layout's rectangle.  Each
;; step waits for a draw line newer than the step's stimulus and copies the
;; latest one as CURSOR-ROW-WALK-DRAW CASE STEP LINE.  GNU has no such log.
(defun neo-row-walk-draw-lines ()
  "The stage=draw lines of NEOMACS_LOG_FILE, oldest first; nil on GNU."
  (let ((path (getenv "NEOMACS_LOG_FILE")))
    (when (and (fboundp 'neomacs--frame-snapshot) path (file-exists-p path))
      (with-temp-buffer
        (insert-file-contents path)
        (let (lines)
          (goto-char (point-min))
          (while (search-forward " stage=draw presentation=" nil t)
            (push (buffer-substring-no-properties
                   (line-beginning-position) (line-end-position))
                  lines)
            (forward-line 1))
          (nreverse lines))))))

(defun neo-row-walk-after-draw (name step before continuation)
  "On Neomacs, wait for a draw line past BEFORE and copy it; then CONTINUATION."
  (condition-case err
      (if (not (fboundp 'neomacs--frame-snapshot))
          (run-at-time 0.05 nil continuation)
        (let ((lines (neo-row-walk-draw-lines)))
          (cond
           ((> (length lines) before)
            (neo-row-walk-emit
             (format "CURSOR-ROW-WALK-DRAW %s %s %s" name step (car (last lines))))
            (run-at-time 0.05 nil continuation))
           ((> (float-time) neo-row-walk-deadline)
            (error "Readiness expired: no cursor_geometry draw line for %s %s" name step))
           (t (run-at-time 0.05 nil #'neo-row-walk-after-draw
                           name step before continuation)))))
    (error (neo-row-walk-fail err))))

;; Each phase of a step runs in a fresh timer callback on both editors.
;; GNU's `timer-event-handler' wraps every callback in `save-current-buffer'
;; (timer.el), and the command loop resets the current buffer from the
;; selected window only around reading input (keyboard.c `command_loop_1',
;; `read_key_sequence'), not between timers.  So a callback that follows the
;; one that ran `switch-to-buffer' still starts in *scratch* while the window
;; shows the case's buffer.  Each phase records that, as
;;   CURSOR-ROW-WALK-CURRENT-BUFFER CASE STEP PHASE CURRENT WINDOW
;; for the harness to compare, then works in the window's buffer as a
;; command would.
(defun neo-row-walk-note-buffer (name step phase)
  "Record the current and window buffers for NAME STEP PHASE, then select."
  (neo-row-walk-emit
   (format "CURSOR-ROW-WALK-CURRENT-BUFFER %s %s %s %s %s"
           name step phase (buffer-name) (buffer-name (window-buffer))))
  (set-buffer (window-buffer)))

(defun neo-row-walk-steps (name steps rest)
  "Run STEPS for case NAME, each after a settled redisplay, then REST."
  (if (null steps)
      (neo-row-walk-cases rest)
    (neo-row-walk-after-redisplay
     (lambda ()
       (condition-case err
           (pcase-let* ((`(,label . ,action) (car steps))
                        (_ (neo-row-walk-note-buffer name label 'act))
                        (count (length (neo-row-walk-draw-lines)))
                        ;; A step with no stimulus accepts the latest line.
                        (drawn (if action count (max 0 (1- count)))))
             (when action (funcall action))
             (neo-row-walk-after-redisplay
              (lambda ()
                (neo-row-walk-note-buffer name label 'record)
                (neo-row-walk-record name label)
                (neo-row-walk-after-draw
                 name label drawn
                 (lambda () (neo-row-walk-steps name (cdr steps) rest))))))
         (error (neo-row-walk-fail err)))))))

(defun neo-row-walk-next-line ()
  "C-n, as the command loop runs it: a repeat keeps the goal column.
A failing move reports the motion state it failed from."
  (setq this-command 'next-line)
  (let ((before (list (buffer-name) (buffer-name (window-buffer))
                      (point) (window-point) (line-number-at-pos) (current-column)
                      temporary-goal-column (window-start) (window-end nil t)
                      (save-excursion (vertical-motion 1)))))
    (condition-case err
        (call-interactively #'next-line)
      (error (signal (car err) (append (cdr err) (list :from before))))))
  (setq last-command 'next-line))

(defun neo-row-walk-cases (cases)
  "Walk each of CASES in turn, then report success."
  (condition-case err
      (if (null cases)
          (progn
            (neo-row-walk-emit "COMMIT-EDITING-PASS")
            (kill-emacs 0))
        (neo-row-walk-setup (car cases))
        (setq neo-row-walk-deadline (+ (float-time) 45))
        (neo-row-walk-steps
         (car (car cases))
         (list (cons 'start nil)
               (cons 'down-1 #'neo-row-walk-next-line)
               (cons 'down-2 #'neo-row-walk-next-line))
         (cdr cases)))
    (error (neo-row-walk-fail err))))

(defvar neo-row-walk-geometry nil "The last frame geometry seen while settling.")
(defvar neo-row-walk-stable-polls 0 "Consecutive polls with unchanged geometry.")

(defun neo-row-walk-settle ()
  "Start the cases once the frame geometry has held for 1 s (20 polls).
The first case must not see a frame that is still being resized."
  (condition-case err
      (let ((geometry (list (frame-pixel-width) (frame-pixel-height)
                            (window-inside-pixel-edges))))
        (redisplay t)
        (if (equal geometry neo-row-walk-geometry)
            (setq neo-row-walk-stable-polls (1+ neo-row-walk-stable-polls))
          (setq neo-row-walk-geometry geometry
                neo-row-walk-stable-polls 0))
        (cond
         ((>= neo-row-walk-stable-polls 20) (neo-row-walk-cases neo-row-walk-cases))
         ((> (float-time) neo-row-walk-deadline)
          (error "Readiness expired: frame geometry never held still: %S" geometry))
         (t (run-at-time 0.05 nil #'neo-row-walk-settle))))
    (error (neo-row-walk-fail err))))

(condition-case err
    (progn
      (setq create-lockfiles nil)
      (setq neo-row-walk-deadline (+ (float-time) 45))
      ;; No size is requested: the walk's scroll positions depend on the
      ;; window height, and both editors' default frame is GNU's 36 lines
      ;; (row frame_text_height_matches_gnu pins that).
      (run-at-time 0.05 nil #'neo-row-walk-settle))
  (error (neo-row-walk-fail err)))
