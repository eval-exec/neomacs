;;; cursor-focus.el --- the cursor of a frame with and without focus (#882 B1) -*- lexical-binding: t; -*-

;; GNU `get_window_cursor_type' (xdisp.c:34670) gives the non-selected cursor
;; to every window that is not its frame's selected window AND to the
;; selected window of a frame that does not hold the display's focus
;; (`f != highlight_frame').  Under `cursor-in-non-selected-windows' t that
;; makes a box hollow and a bar one pixel narrower.
;;
;; The test display has no window manager, so a frame is UNFOCUSED unless
;; this fixture gives it focus with `xdotool windowfocus'.  Scenarios that do
;; not call `neo-cf-focus' are the unfocused arm on purpose: do not "fix"
;; them by focusing.
;;
;; NEOMACS_CF_SCENARIO:
;;   focus-change  bar cursor; record (window-cursor-info) unfocused, then
;;                 focus the frame and record it again
;;   unfocused     box cursor on an unfocused frame; paint
;;   split         focus the frame, split the window; paint both cursors
;;
;; Records go to NEOMACS_COMMIT_RESULT_FILE as "CURSOR-FOCUS NAME VALUE".
;; Paint scenarios write the frame image to NEOMACS_DEBUG_SURFACE_READBACK_PNG
;; (GNU with `x-export-frames'; Neomacs's runtime writes its own readback).
(setq inhibit-startup-screen t)
(menu-bar-mode -1)
(when (fboundp 'tool-bar-mode) (tool-bar-mode -1))
(blink-cursor-mode -1)
(set-face-attribute 'default nil :height 300)
(set-face-background 'cursor "#ff0000")
(switch-to-buffer (get-buffer-create "*cursor-focus*"))
(setq-local mode-line-format nil)
(insert "MMMM    MMMM\n")
(goto-char 7)

(defun neo-cf-emit (name value)
  (let ((line (format "CURSOR-FOCUS %s %S\n" name value))
        (coding-system-for-write 'utf-8-unix))
    (princ line 'external-debugging-output)
    (write-region line nil (getenv "NEOMACS_COMMIT_RESULT_FILE") 'append 'silent)))

(defun neo-cf-fail (err)
  (ignore-errors (neo-cf-emit "FAIL" err))
  (kill-emacs 1))

(defun neo-cf-export ()
  (when (and (eq window-system 'x) (fboundp 'x-export-frames))
    (let ((coding-system-for-write 'no-conversion))
      (with-temp-file (getenv "NEOMACS_DEBUG_SURFACE_READBACK_PNG")
        (set-buffer-multibyte nil)
        (insert (x-export-frames nil 'png))))))

(defun neo-cf-focus ()
  "Give this Emacs's X window the input focus, then wait for the focus-in."
  ;; The window is found by _NET_WM_PID (Neomacs sets it) or else by a
  ;; unique title (GNU's window carries no _NET_WM_PID without a window
  ;; manager here).  Either way only this process's window is focused.
  (let* ((title (format "neo-cursor-focus-%d" (emacs-pid)))
         (pid (number-to-string (emacs-pid)))
         (deadline (+ (float-time) 10))
         (window-id
          (lambda (&rest search)
            ;; A separate search: `search --pid N windowfocus' would read
            ;; `windowfocus' as the name pattern.
            (with-temp-buffer
              (and (zerop (apply #'call-process "xdotool" nil t nil "search" "--onlyvisible"
                                 search))
                   (car (split-string (buffer-string)))))))
         window)
    (set-frame-parameter nil 'title title)
    ;; Poll: the window is viewable only once mapped.
    (while (not (setq window (or (funcall window-id "--pid" pid)
                                 (funcall window-id "--name" (concat "^" title "$")))))
      (when (> (float-time) deadline)
        (error "xdotool found no viewable window for this frame"))
      (sit-for 0.1))
    (unless (zerop (call-process "xdotool" nil nil nil "windowfocus" "--sync" window))
      (error "xdotool could not focus window %s" window)))
  (let ((deadline (+ (float-time) 10)))
    (while (not (eq (frame-focus-state) t))
      (when (> (float-time) deadline)
        (error "No focus-in after xdotool: %S" (frame-focus-state)))
      (sit-for 0.05))))

(defun neo-cf-settle ()
  "Redisplay for 1 s (20 polls 50 ms apart) before the frame is captured.
Neomacs's surface readback is written when a frame is presented, which can
trail `redisplay' under load; the last image must show the settled frame.
It is a settle, not a tolerance: every assertion compares exact pixels."
  (dotimes (_ 20)
    (redisplay t)
    (sit-for 0.05)))

(defun neo-cf-run ()
  (condition-case err
      (pcase (getenv "NEOMACS_CF_SCENARIO")
        ("focus-change"
         (setq cursor-type 'bar)
         (redisplay t)
         (neo-cf-emit "unfocused" (list (frame-focus-state) (window-cursor-info)))
         (neo-cf-focus)
         (redisplay t)
         (neo-cf-emit "focused" (list (frame-focus-state) (window-cursor-info)))
         (kill-emacs 0))
        ("unfocused"
         (neo-cf-settle)
         (neo-cf-emit "unfocused" (frame-focus-state))
         (neo-cf-export)
         (kill-emacs 0))
        ("split"
         (neo-cf-focus)
         (split-window-right)
         (neo-cf-settle)
         (neo-cf-emit "split" (frame-focus-state))
         (neo-cf-export)
         (kill-emacs 0))
        (other (error "Unknown NEOMACS_CF_SCENARIO %S" other)))
    (error (neo-cf-fail err))))

(run-at-time 1.0 nil #'neo-cf-run)
