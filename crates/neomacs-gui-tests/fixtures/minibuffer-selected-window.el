;;; minibuffer-selected-window.el --- Issue #560 -*- lexical-binding: t -*-

(defun issue560-affixate (candidates)
  ;; Public completion protocol, using Marginalia's window fallback.
  (with-selected-window (or (minibuffer-selected-window) (selected-window))
    (mapcar (lambda (candidate) (list candidate "" " checked")) candidates)))

(defun issue560-capture (suffix)
  (when (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON")
    (redisplay t)
    (neomacs--write-frame-snapshot
     (concat (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON") suffix) nil 'json)))

(defun issue560-main ()
  (condition-case err
      (let* ((origin (split-window))
             (completion-extra-properties
              '(:affixation-function issue560-affixate))
             (choice nil))
        (select-window origin)
        (setq choice
              (minibuffer-with-setup-hook
                  (lambda ()
                    (run-at-time
                     0.1 nil
                     (lambda ()
                       (condition-case err
                           (progn
                             (unless (eq (minibuffer-selected-window) origin)
                               (error "Missing live caller before deletion"))
                             (delete-window origin)
                             (when (minibuffer-selected-window)
                               (error "Deleted minibuffer caller was returned"))
                             (minibuffer-completion-help)
                             (issue560-capture ".candidates")
                             (with-temp-file (getenv "NEOMACS_TEST_COMPLETION_READY")
                               (insert "ready")))
                         (error (message "Issue 560 completion: %S" err) (kill-emacs 1))))))
                (completing-read "Choose: " '("Alpha" "Beta") nil t)))
        (unless (equal choice "Beta") (error "Unexpected selection: %S" choice))
        (when (minibuffer-selected-window) (error "Caller leaked after minibuffer exit"))
        (switch-to-buffer (get-buffer-create "minibuffer-result"))
        (erase-buffer)
        (insert (format "Selected %s\n" choice))
        (goto-char (point-min))
        (issue560-capture "")
        (when (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON")
          (run-at-time
           1 nil
           (lambda ()
             (copy-file (getenv "NEOMACS_DEBUG_SURFACE_READBACK_PNG")
                        (concat (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_JSON") ".png") t)
             (kill-emacs 0)))))
    (error (message "Issue 560: %S" err) (kill-emacs 1))))

(run-at-time 1 nil #'issue560-main)
(run-at-time 25 nil (lambda () (message "Issue 560 watchdog expired") (kill-emacs 2)))
