;;; tab-bar-height.el --- Tab-bar height and box-width probe (#862) -*- lexical-binding: t; -*-

;; Records the tab-bar pixel height, root window pixel top, tab-bar font pixel
;; size and font height, and default frame-char-height across five steps, each
;; settled for 1 s (20 polls 50 ms apart):
;;
;;   default        untouched -Q (tab-bar-mode 1) with default :box (:line-width 1)
;;   no-box         tab-bar-tab :box nil (isolates bare variable-pitch font height)
;;   box-2          tab-bar-tab :box (:line-width 2 :style released-button) (+4 px)
;;   box-neg-2      tab-bar-tab :box (:line-width -2 :style released-button) (+0 px)
;;   explicit-14px  tab-bar explicit 14px Sans Serif + :box (:line-width 1) (DPI-independent)
;;
;; Output record format:
;;   TAB-BAR-HEIGHT STEP LINES HEIGHT ROOT-TOP FONT-PX FONT-HEIGHT CHAR-HEIGHT

(defun neo-tbh-emit (line)
  "Write LINE to stderr, then append it to NEOMACS_COMMIT_RESULT_FILE."
  (princ (concat line "\n") 'external-debugging-output)
  (let ((result-file (getenv "NEOMACS_COMMIT_RESULT_FILE")))
    (unless result-file (error "NEOMACS_COMMIT_RESULT_FILE is unset"))
    (let ((coding-system-for-write 'utf-8-unix)
          (create-lockfiles nil))
      (write-region (concat line "\n") nil result-file 'append 'silent))))

(defun neo-tbh-fail (err)
  "Report ERR and terminate cleanly with exit status 1."
  (ignore-errors (neo-tbh-emit (format "COMMIT-EDITING-FAIL %S" err)))
  (kill-emacs 1))

(defun neo-tbh-geometry ()
  "Return (LINES HEIGHT ROOT-TOP FONT-PX FONT-HEIGHT CHAR-HEIGHT)."
  (let* ((font (face-font 'tab-bar))
         (info (and font (font-info font)))
         (font-px (if info (aref info 2) 0))
         (font-height (if info (aref info 3) 0)))
    (list (or (frame-parameter nil 'tab-bar-lines) 0)
          (tab-bar-height nil t)
          (nth 1 (window-pixel-edges (frame-root-window)))
          font-px
          font-height
          (frame-char-height))))

(defvar neo-tbh-deadline nil)
(defvar neo-tbh-last nil)
(defvar neo-tbh-stable 0)

(defun neo-tbh-settle (step steps)
  "Record STEP once geometry holds still for 1 s, then advance to STEPS."
  (condition-case err
      (let ((geometry (progn (redisplay t) (neo-tbh-geometry))))
        (if (equal geometry neo-tbh-last)
            (setq neo-tbh-stable (1+ neo-tbh-stable))
          (setq neo-tbh-last geometry neo-tbh-stable 0))
        (cond
         ((>= neo-tbh-stable 20)
          (neo-tbh-emit (format "TAB-BAR-HEIGHT %s %s" step
                                (mapconcat #'number-to-string geometry " ")))
          (neo-tbh-run steps))
         ((> (float-time) neo-tbh-deadline)
          (error "Readiness expired: %s geometry never held still: %S" step geometry))
         (t (run-at-time 0.05 nil #'neo-tbh-settle step steps))))
    (error (neo-tbh-fail err))))

(defun neo-tbh-run (steps)
  "Apply the first step in STEPS, then settle and record it."
  (condition-case err
      (if (null steps)
          (progn (neo-tbh-emit "COMMIT-EDITING-PASS") (kill-emacs 0))
        (pcase-let ((`(,step . ,action) (car steps)))
          (funcall action)
          (force-mode-line-update t)
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
       (list
        (cons 'default
              (lambda () (tab-bar-mode 1)))
        (cons 'no-box
              (lambda ()
                (set-face-attribute 'tab-bar-tab nil :box nil)))
        (cons 'box-2
              (lambda ()
                (set-face-attribute 'tab-bar-tab nil
                                    :box '(:line-width 2 :style released-button))))
        (cons 'box-neg-2
              (lambda ()
                (set-face-attribute 'tab-bar-tab nil
                                    :box '(:line-width -2 :style released-button))))
        (cons 'explicit-14px
              (lambda ()
                (set-face-attribute 'tab-bar nil
                                    :font (font-spec :family "Sans Serif" :size 14))
                (set-face-attribute 'tab-bar-tab nil
                                    :box '(:line-width 1 :style released-button)))))))
  (error (neo-tbh-fail err)))
