;;; issue-582-fullscreen.el --- Native WM geometry regression -*- lexical-binding: t -*-

;; Rust drives native WM operations independently of the editor.  These files
;; only coordinate public font changes with the native geometry observations.
(setq native-comp-jit-compilation nil
      native-comp-enable-subr-trampolines nil)
(defvar issue-582-sequence 0)

(defun issue-582-acknowledge (sequence)
  (let* ((destination (getenv "NEOMACS_ISSUE_582_ACK"))
         (temporary (concat destination ".next")))
    (with-temp-file temporary
      (prin1 (list sequence
                   (face-attribute 'default :family)
                   (face-attribute 'default :height)
                   (frame-char-width) (frame-char-height)
                   (frame-width) (frame-text-width) (frame-pixel-width))
             (current-buffer)))
    (rename-file temporary destination t)))

(defun issue-582-await-columns (sequence columns deadline)
  (condition-case err
      (cond
       ((and (= (frame-width) columns)
             (= (frame-text-width) (* columns (frame-char-width))))
        (issue-582-acknowledge sequence))
       ((> (float-time) deadline)
        (error "Character resize did not settle: %S columns, %S pixels"
               (frame-width) (frame-text-width)))
       (t (run-at-time 0.05 nil #'issue-582-await-columns
                       sequence columns deadline)))
    (error (message "ISSUE-582-FAIL: %S" err) (kill-emacs 1))))

(defun issue-582-observe-request ()
  (condition-case err
      (let ((request (getenv "NEOMACS_ISSUE_582_REQUEST")))
        (when (file-exists-p request)
          (let* ((form (with-temp-buffer
                         (insert-file-contents request)
                         (read (current-buffer))))
                 (sequence (car form))
                 (command (cadr form))
                 (value (caddr form)))
            (when (> sequence issue-582-sequence)
              (setq issue-582-sequence sequence)
              (cond
               ((eq command 'font)
                (set-face-attribute 'default nil :family "Times New Roman"
                                    :height value)
                (issue-582-acknowledge sequence))
               ((eq command 'columns)
                (set-frame-width nil value)
                (issue-582-await-columns sequence value (+ (float-time) 10)))
               ((eq command 'observe)
                (issue-582-acknowledge sequence))
               (t (error "Unknown issue-582 request: %S" form)))))))
    (error (message "ISSUE-582-FAIL: %S" err) (kill-emacs 1))))

(run-at-time 0.05 0.05 #'issue-582-observe-request)
