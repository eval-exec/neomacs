;;; explicit-size-after-font-oracle.el --- Explicit size after a font change -*- lexical-binding: t -*-
;;;
;;; A font height change runs, then an explicit 120x40 is applied.
;;; N827_WHEN:
;;;   unset            — `window-setup-hook', before the command-loop redisplay
;;;   redisplay        — a timer, after the first redisplay
;;;   before-redisplay — hook applies the size, waits until that configure
;;;                      is applied, and samples before the tool bar is built
;;;   init-redisplay   — `(redisplay)' and `(sit-for 0)' during load, then the hook
;;;   reenable         — after the first redisplay, one timer runs
;;;                      `(tool-bar-mode -1)', `(tool-bar-mode 1)', and the
;;;                      explicit size before another redisplay
;;; N827_INHIBIT selects t, tool-bar-lines, or default. The default is t.
;;; Reading `frame-width' before the sample would flush a queued resize.
(require 'subr-x)
(setq inhibit-startup-screen t
      frame-title-format "oracleb827")
(let ((inhibit (getenv "N827_INHIBIT")))
  (cond
   ((equal inhibit "tool-bar-lines")
    (setq frame-inhibit-implied-resize '(tool-bar-lines)))
   ((equal inhibit "default") nil)
   (t (setq frame-inhibit-implied-resize t))))
(modify-frame-parameters nil '((name . "oracleb827") (title . "oracleb827")))
(set-face-attribute 'default nil :height 160)
(defun n827-apply-size ()
  (modify-frame-parameters nil '((width . 120) (height . 40))))
(defun n827-lisp-grid ()
  "Frame grid, text pixels, bar sizes, and the root window's total width."
  (let ((geometry (frame-geometry)))
    (list (frame-width)
          (frame-height)
          (frame-char-width)
          (frame-char-height)
          (frame-text-height)
          (cdr (cdr (assq 'tool-bar-size geometry)))
          (cdr (cdr (assq 'menu-bar-size geometry)))
          (window-total-width (frame-root-window)))))
(defun n827-sample (&optional locked)
  "Print one EXPLICIT-SIZE line and exit.
LOCKED reads the Lisp grid before xdotool. `call-process' redisplays
while it reads subprocess output (callproc.c:902-904), and that
redisplay builds the tool bar."
  (let* ((grid (and locked (n827-lisp-grid)))
         (sizes (mapcar
                 #'n827-wh
                 (split-string
                  (shell-command-to-string "xdotool search --name oracleb827")
                  "\n" t)))
         (best (car (sort sizes
                          (lambda (a b)
                            (> (* (nth 1 a) (nth 2 a))
                               (* (nth 1 b) (nth 2 b)))))))
         (width (if best (nth 1 best) 0))
         (height (if best (nth 2 best) 0))
         (live (or grid (n827-lisp-grid))))
    ;; `frame-text-height` does not include the tool bar. Each editor's
    ;; own `tool-bar-size` is printed so a row can recover the shared text
    ;; grid without comparing GTK widget pixels. xwin may be 0 when the
    ;; sample runs before the window is mapped.
    (princ (format "EXPLICIT-SIZE xwin=%sx%s frame=%sx%s char=%sx%s text=%s tool=%s menu=%s root=%s\n"
                   width
                   height
                   (nth 0 live)
                   (nth 1 live)
                   (nth 2 live)
                   (nth 3 live)
                   (nth 4 live)
                   (nth 5 live)
                   (nth 6 live)
                   (nth 7 live))
           'external-debugging-output)
    (kill-emacs 0)))
(defun n827-native-size ()
  (cdr (assq 'native-size (frame-geometry))))
(defun n827-explicit-size-ready-p ()
  "The requested 120x40 grid, including its text pixels.
`frame-parameters' reports `new_width' / column width while
`new_size_p' is set (frame.c:4141-4147). That is the queued
configure, not `FRAME_COLS'. The text height matches the reported
rows only after `change_frame_size' applies it (dispnew.c:6720-6728)."
  (and (= (frame-width) 120)
       (= (frame-height) 40)
       (= (frame-text-height) (* 40 (frame-char-height)))))
(defun n827-configure-settled-p (before previous)
  "The toolkit native size moved, and the reported grid stopped moving.
Two identical reads after the configure acknowledgement are GNU's
settled size when that size is not the requested 120x40."
  (let ((now (n827-native-size))
        (sample (list (frame-width) (frame-height) (frame-text-height))))
    (and (consp now)
         (not (equal now before))
         (equal sample previous)
         (= (nth 2 sample) (* (nth 1 sample) (frame-char-height))))))
(defun n827-wait-for-explicit-size ()
  "Apply the queued configure without building the tool bar.
`xg_frame_set_char_size' waits `x-wait-for-event-timeout' (0.1s,
xterm.c:32794). Under load that misses, and `xg_frame_resized'
only queues `change_frame_size' (gtkutil.c:1161, dispnew.c:6710-6713).
`frame-width' then reports `new_width' / column (frame.c:4145-4147).
`sit-for' returns immediately when `input-pending-p' (subr.el:3872)
and never reaches `do_pending_window_change'. `discard-input' clears
that Lisp input. `read-event' blocks in `wait_reading_process_output'
(keyboard.c:4128, process.c:5841), which applies the queue.
`accept-process-output' applies a queue that `read-event' returned
before processing, and its wait does not redisplay (process.c:4958).
The caller binds `inhibit-redisplay', so a garbaged frame does not
run `prepare_menu_bars' (xdisp.c:17377). Stop on the requested
120x40 grid, or when the native size has moved and the grid is stable."
  (let ((before (n827-native-size))
        (deadline (+ (float-time) 3.0))
        (previous nil))
    (while (and (< (float-time) deadline)
                (not (n827-explicit-size-ready-p))
                (not (n827-configure-settled-p before previous)))
      (discard-input)
      (let ((input-method-function nil))
        (read-event nil t 0.05))
      (unless (or (n827-explicit-size-ready-p)
                  (n827-configure-settled-p before previous))
        (accept-process-output nil 0.05))
      (setq previous (list (frame-width) (frame-height) (frame-text-height))))))
(defun n827-sample-before-redisplay ()
  ;; Bound for the wait and for xdotool. `redisplay_internal' returns
  ;; before `prepare_menu_bars' (xdisp.c:17377), so the tool bar stays
  ;; unallocated while the configure is applied.
  (let ((inhibit-redisplay t))
    (n827-apply-size)
    (n827-wait-for-explicit-size)
    (n827-sample t)))
(let ((when (getenv "N827_WHEN")))
  (cond
   ((equal when "redisplay")
    (run-with-timer 1 nil #'n827-apply-size)
    (run-with-timer 3 nil #'n827-sample))
   ((equal when "before-redisplay")
    (add-hook 'window-setup-hook #'n827-sample-before-redisplay))
   ((equal when "init-redisplay")
    (redisplay)
    (sit-for 0)
    (add-hook 'window-setup-hook #'n827-apply-size)
    (run-with-timer 2 nil #'n827-sample))
   ((equal when "reenable")
    ;; One timer, after the first redisplay has packed the bar. Turning the
    ;; mode off and on calls `x_change_tool_bar_height` before the explicit
    ;; size, and that size runs before the next redisplay (xfns.c:1902-1908).
    (run-with-timer 1 nil
                    (lambda ()
                      (tool-bar-mode -1)
                      (tool-bar-mode 1)
                      (n827-apply-size)))
    (run-with-timer 3 nil #'n827-sample))
   (t
    (add-hook 'window-setup-hook #'n827-apply-size)
    (run-with-timer 2 nil #'n827-sample))))

(defun n827-wh (id)
  "Return (ID WIDTH HEIGHT) for an X window. Width and height are 0 when
xwininfo has no stats."
  (let ((info (shell-command-to-string (format "xwininfo -id %s -stats" id))))
    (list id
          (if (string-match "Width: \\([0-9]+\\)" info)
              (string-to-number (match-string 1 info))
            0)
          (if (string-match "Height: \\([0-9]+\\)" info)
              (string-to-number (match-string 1 info))
            0))))
