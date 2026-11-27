;;; frame-native-title.el --- the native window title after title/name changes (#890) -*- lexical-binding: t; -*-

;; GNU `x_set_name' (xfns.c:2309-2349) puts the frame's `title' parameter on
;; the native window when it is set, and the name otherwise; an explicit
;; `name' stops `frame-title-format' from renaming the frame.  `x-create-frame'
;; leaves a frame created without a `name' parameter implicit
;; (xfns.c:5115-5125).
;;
;; Each step records "FRAME-TITLE STEP EXPLICIT TITLE-PARAM NATIVE": NATIVE
;; is the name of every viewable top-level window on the test display, read
;; with xdotool (the display holds only this editor's frame).  Records go to
;; NEOMACS_COMMIT_RESULT_FILE.
(setq inhibit-startup-screen t)

(defun neo-fnt-native-names ()
  (with-temp-buffer
    (unless (zerop (call-process "xdotool" nil t nil "search" "--onlyvisible" "--name" "."))
      (error "xdotool found no viewable window"))
    (mapcar (lambda (id)
              (with-temp-buffer
                (call-process "xdotool" nil t nil "getwindowname" id)
                (string-trim (buffer-string))))
            (split-string (buffer-string)))))

(defun neo-fnt-record (step)
  "Record STEP once the native title has held still for 0.5 s."
  (let ((deadline (+ (float-time) 10)) last (stable 0))
    (while (< stable 10)
      (redisplay t)
      (let ((names (neo-fnt-native-names)))
        (if (equal names last) (setq stable (1+ stable)) (setq last names stable 0)))
      (when (> (float-time) deadline)
        (error "Native title never held still at %s: %S" step last))
      (sit-for 0.05))
    (let ((line (format "FRAME-TITLE %s %S %S %S\n" step
                        (frame-parameter nil 'explicit-name)
                        (frame-parameter nil 'title) last))
          (coding-system-for-write 'utf-8-unix))
      (princ line 'external-debugging-output)
      (write-region line nil (getenv "NEOMACS_COMMIT_RESULT_FILE") 'append 'silent))))

(run-at-time
 1.0 nil
 (lambda ()
   (condition-case err
       (progn
         ;; Not the editor's own branding: the default `frame-title-format'
         ;; text differs between the editors, so it is fixed here.
         (setq frame-title-format "fmt-%b")
         ;; As a command would: GNU re-renders the title only when mode
         ;; lines need updating.
         (force-mode-line-update t)
         (neo-fnt-record "start")
         (set-frame-parameter nil 'title "my-title")
         (neo-fnt-record "title-set")
         (set-frame-parameter nil 'title nil)
         (neo-fnt-record "title-nil")
         (set-frame-parameter nil 'name "my-name")
         (neo-fnt-record "name-set")
         (set-frame-parameter nil 'title "t2")
         (neo-fnt-record "title-over-name")
         (kill-emacs 0))
     (error
      (ignore-errors
        (write-region (format "FRAME-TITLE FAIL %S\n" err) nil
                      (getenv "NEOMACS_COMMIT_RESULT_FILE") 'append 'silent))
      (kill-emacs 1)))))
