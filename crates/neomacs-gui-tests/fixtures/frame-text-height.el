;;; frame-text-height.el --- Frame text height after each sizing -*- lexical-binding: t; -*-

;; Records the frame's text-area geometry, each shape after the geometry has
;; held still for 1 s (20 polls 50 ms apart).
;;
;; NEOMACS_FTH_SCENARIO=settled records, in turn:
;;   default    no size requested (GNU: 80x36 characters, frame.c
;;              `gui_figure_window_size')
;;   pixelwise  (set-frame-size nil 640 612 t)
;;   chars      (set-frame-size nil 80 30)
;;
;; NEOMACS_FTH_SCENARIO=startup requests (set-frame-size nil 640 612 t)
;; while this file loads, before the first frame has settled, as an init
;; file would, and records
;;   startup-pixelwise
;;
;;   FRAME-TEXT-HEIGHT SHAPE CHAR-HEIGHT TEXT-HEIGHT FRAME-HEIGHT ROOT MINI BODY
;;
;; TEXT-HEIGHT is `frame-text-height', FRAME-HEIGHT is `frame-height' (lines),
;; ROOT and MINI are the pixel heights of the root and minibuffer windows,
;; BODY is `window-body-height' (lines) of the frame's first window.
;; GNU's text area holds both windows, so ROOT + MINI = TEXT-HEIGHT there.
;;
;; Markers go to stderr first, then to NEOMACS_COMMIT_RESULT_FILE, which is
;; the copy the harness reads.
(defun neo-fth-emit (line)
  "Write LINE to stderr, then append it to NEOMACS_COMMIT_RESULT_FILE."
  (princ (concat line "\n") 'external-debugging-output)
  (let ((result-file (getenv "NEOMACS_COMMIT_RESULT_FILE")))
    (unless result-file (error "NEOMACS_COMMIT_RESULT_FILE is unset"))
    (let ((coding-system-for-write 'utf-8-unix)
          (create-lockfiles nil))
      (write-region (concat line "\n") nil result-file 'append 'silent))))

(defun neo-fth-fail (err)
  "Report ERR and terminate; a failing report still reaches `kill-emacs'."
  (ignore-errors (neo-fth-emit (format "COMMIT-EDITING-FAIL %S" err)))
  (kill-emacs 1))

(defun neo-fth-geometry ()
  "The geometry this probe records, as a list."
  (list (frame-char-height) (frame-text-height) (frame-height)
        (window-pixel-height (frame-root-window))
        (window-pixel-height (minibuffer-window))
        (window-body-height (frame-first-window))))

(defvar neo-fth-deadline nil "The `float-time' after which a wait fails.")
(defvar neo-fth-last nil "The geometry seen at the previous poll.")
(defvar neo-fth-stable 0 "Consecutive polls with unchanged geometry.")

(defun neo-fth-settle (shape shapes)
  "Record SHAPE once the geometry holds for 1 s, then run SHAPES."
  (condition-case err
      (let ((geometry (progn (redisplay t) (neo-fth-geometry))))
        (if (equal geometry neo-fth-last)
            (setq neo-fth-stable (1+ neo-fth-stable))
          (setq neo-fth-last geometry neo-fth-stable 0))
        (cond
         ((>= neo-fth-stable 20)
          (neo-fth-emit (format "FRAME-TEXT-HEIGHT %s %s" shape
                                (mapconcat #'number-to-string geometry " ")))
          (neo-fth-run shapes))
         ((> (float-time) neo-fth-deadline)
          (error "Readiness expired: %s geometry never held still: %S" shape geometry))
         (t (run-at-time 0.05 nil #'neo-fth-settle shape shapes))))
    (error (neo-fth-fail err))))

(defun neo-fth-run (shapes)
  "Apply the first of SHAPES, then settle and record it."
  (condition-case err
      (if (null shapes)
          (progn (neo-fth-emit "COMMIT-EDITING-PASS") (kill-emacs 0))
        (pcase-let ((`(,shape . ,action) (car shapes)))
          (when action (funcall action))
          (setq neo-fth-deadline (+ (float-time) 30)
                neo-fth-last nil
                neo-fth-stable 0)
          (run-at-time 0.05 nil #'neo-fth-settle shape (cdr shapes))))
    (error (neo-fth-fail err))))

(condition-case err
    (progn
      (setq create-lockfiles nil)
      (pcase (getenv "NEOMACS_FTH_SCENARIO")
        ("settled"
         (run-at-time 0.05 nil #'neo-fth-run
                      (list (cons 'default nil)
                            (cons 'pixelwise
                                  (lambda () (set-frame-size (selected-frame) 640 612 t)))
                            (cons 'chars
                                  (lambda () (set-frame-size (selected-frame) 80 30))))))
        ("startup"
         ;; The size is requested while the file loads, before the first
         ;; frame has settled: the sizing an init file makes.
         (set-frame-size (selected-frame) 640 612 t)
         (run-at-time 0.05 nil #'neo-fth-run (list (cons 'startup-pixelwise nil))))
        (other (error "Unknown NEOMACS_FTH_SCENARIO %S" other))))
  (error (neo-fth-fail err)))
