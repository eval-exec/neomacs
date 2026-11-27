;;; tab-bar-hide.el --- Tab-bar geometry when tab-bar-format is nil (#856) -*- lexical-binding: t; -*-

;; The #856 stimulus, without its 2 s startup timer: `tab-bar-mode' on,
;; then `tab-bar-format' nil plus `tab-bar--update-tab-bar-lines' (the
;; reporter's `my/tab-bar-hide'), then a one-item format, then the original
;; format back.  After each
;; step has held still for 1 s (20 polls 50 ms apart) it records
;;
;;   TAB-BAR-HIDE STEP TAB-BAR-LINES TAB-BAR-HEIGHT TEXT-HEIGHT ROOT-TOP ROOT-HEIGHT BODY-LINES
;;
;; TAB-BAR-LINES is the frame parameter, TAB-BAR-HEIGHT is (tab-bar-height
;; nil t), TEXT-HEIGHT is `frame-text-height', ROOT-TOP and ROOT-HEIGHT are
;; the root window's pixel top edge and height, BODY-LINES is
;; `window-body-height'.
;;
;; Markers go to stderr first, then to NEOMACS_COMMIT_RESULT_FILE, which is
;; the copy the harness reads.
(defun neo-tbh-emit (line)
  "Write LINE to stderr, then append it to NEOMACS_COMMIT_RESULT_FILE."
  (princ (concat line "\n") 'external-debugging-output)
  (let ((result-file (getenv "NEOMACS_COMMIT_RESULT_FILE")))
    (unless result-file (error "NEOMACS_COMMIT_RESULT_FILE is unset"))
    (let ((coding-system-for-write 'utf-8-unix)
          (create-lockfiles nil))
      (write-region (concat line "\n") nil result-file 'append 'silent))))

(defun neo-tbh-fail (err)
  "Report ERR and terminate; a failing report still reaches `kill-emacs'."
  (ignore-errors (neo-tbh-emit (format "COMMIT-EDITING-FAIL %S" err)))
  (kill-emacs 1))

(defun neo-tbh-geometry ()
  "The geometry this probe records, as a list."
  (let ((root (frame-root-window)))
    (list (or (frame-parameter nil 'tab-bar-lines) 0)
          (tab-bar-height nil t)
          (frame-text-height)
          (nth 1 (window-pixel-edges root))
          (window-pixel-height root)
          (window-body-height (frame-first-window)))))

(defun neo-tbh-as-after-a-command ()
  "Make the next redisplay recompute the tab bar, as any command would.
`tab-bar--update-tab-bar-lines' leaves `tab-bar-lines' at 1, so by itself
it changes nothing redisplay checks: GNU's `update_tab_bar' recomputes the
items only when a window, buffer or mode line needs redisplay.  In the
#856 report a later command does that; here it is explicit, so both
editors are recorded in the state the user sees."
  (force-mode-line-update t))

(defvar neo-tbh-format-backup nil "The `tab-bar-format' to restore.")
(defvar neo-tbh-deadline nil "The `float-time' after which a wait fails.")
(defvar neo-tbh-last nil "The geometry seen at the previous poll.")
(defvar neo-tbh-stable 0 "Consecutive polls with unchanged geometry.")

(defun neo-tbh-settle (step steps)
  "Record STEP once the geometry holds for 1 s, then run STEPS."
  (condition-case err
      (let ((geometry (progn (redisplay t) (neo-tbh-geometry))))
        (if (equal geometry neo-tbh-last)
            (setq neo-tbh-stable (1+ neo-tbh-stable))
          (setq neo-tbh-last geometry neo-tbh-stable 0))
        (cond
         ((>= neo-tbh-stable 20)
          (neo-tbh-emit (format "TAB-BAR-HIDE %s %s" step
                                (mapconcat #'number-to-string geometry " ")))
          (neo-tbh-run steps))
         ((> (float-time) neo-tbh-deadline)
          (error "Readiness expired: %s geometry never held still: %S" step geometry))
         (t (run-at-time 0.05 nil #'neo-tbh-settle step steps))))
    (error (neo-tbh-fail err))))

(defun neo-tbh-run (steps)
  "Apply the first of STEPS, then settle and record it."
  (condition-case err
      (if (null steps)
          (progn (neo-tbh-emit "COMMIT-EDITING-PASS") (kill-emacs 0))
        (pcase-let ((`(,step . ,action) (car steps)))
          (funcall action)
          (setq neo-tbh-deadline (+ (float-time) 30)
                neo-tbh-last nil
                neo-tbh-stable 0)
          (run-at-time 0.05 nil #'neo-tbh-settle step (cdr steps))))
    (error (neo-tbh-fail err))))

(condition-case err
    (progn
      (setq create-lockfiles nil)
      (run-at-time
       0.05 nil #'neo-tbh-run
       (list (cons 'shown
                   (lambda ()
                     (tab-bar-mode 1)
                     (setq neo-tbh-format-backup tab-bar-format)))
             (cons 'hidden
                   (lambda ()
                     (setq tab-bar-format nil)
                     (tab-bar--update-tab-bar-lines)
                     (neo-tbh-as-after-a-command)))
             ;; A one-item format measures more than 0 px, so the bar
             ;; comes back at that item's height: the shrink follows the
             ;; measured string, not a special case for a nil format.
             (cons 'one-item
                   (lambda ()
                     (setq tab-bar-format '(tab-bar-format-add-tab))
                     (tab-bar--update-tab-bar-lines)
                     (neo-tbh-as-after-a-command)))
             (cons 'reshown
                   (lambda ()
                     (setq tab-bar-format neo-tbh-format-backup)
                     (tab-bar--update-tab-bar-lines)
                     (neo-tbh-as-after-a-command))))))
  (error (neo-tbh-fail err)))
