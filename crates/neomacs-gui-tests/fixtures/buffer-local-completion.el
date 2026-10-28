;;; Buffer-local completion in a live minibuffer. -*- lexical-binding: t; -*-
(require 'timer)
(defvar completion-fixture-presentation-target nil)
(defvar completion-fixture-last-receipt nil)
(defvar completion-fixture-png-ready nil)

(defun completion-fixture-await-result-presentation (passed)
  ;; A diagnostic snapshot owns a renderer-inert ticket: it is discarded,
  ;; never presented.  Redisplay AFTER taking its ID, then require a newer
  ;; presentation of the same frame while the final result stays unchanged.
  (let* ((document (json-parse-string
                    (neomacs--frame-snapshot nil 'json)
                    :object-type 'alist :array-type 'list))
         (frame (car (alist-get 'frames document)))
         (frame-id (alist-get 'frame (alist-get 'frame_placement frame)))
         (after (alist-get 'presentation_id frame))
         (receipt-file (getenv "NEOMACS_GUI_PRESENTATION_RECEIPT"))
         (png (getenv "NEOMACS_DEBUG_SURFACE_READBACK_PNG"))
         (timer nil))
    (unless (and (integerp frame-id) (integerp after) receipt-file png)
      (error "Missing completion presentation diagnostics: %S" frame))
    (setq completion-fixture-presentation-target (cons frame-id after))
    (redisplay t)
    (setq timer
          (run-at-time
           0 0.02
           (lambda ()
             (setq completion-fixture-last-receipt
                   (when (file-exists-p receipt-file)
                     (with-temp-buffer
                       (insert-file-contents receipt-file)
                       (read (current-buffer))))
                   ;; Surface captures publish atomically before presentation;
                   ;; a later matching receipt therefore also fences this PNG.
                   completion-fixture-png-ready
                   (and (file-exists-p png)
                        (> (file-attribute-size (file-attributes png)) 0)))
             (let ((receipt completion-fixture-last-receipt))
               (when (and receipt
                          (eq (plist-get receipt :outcome) 'presented)
                          (equal (plist-get receipt :frame) frame-id)
                          (integerp (plist-get receipt :presentation))
                          (> (plist-get receipt :presentation) after)
                          completion-fixture-png-ready)
                 (cancel-timer timer)
                 (kill-emacs (if passed 0 1)))))))))
(setq inhibit-startup-screen t)
(menu-bar-mode -1)
(when (fboundp 'tool-bar-mode) (tool-bar-mode -1))
(blink-cursor-mode -1)
(run-at-time
 0.5 nil
 (lambda ()
   (unless (display-graphic-p) (error "Expected a graphical frame"))
   (condition-case err
       (let* ((completion-ignore-case nil)
              (selected
               (minibuffer-with-setup-hook
                   (lambda ()
                     (setq-local completion-ignore-case nil)
                     (let ((completion-ignore-case t))
                       (insert "*mes")
                       (minibuffer-complete))
                     (run-at-time 0.3 nil #'exit-minibuffer))
                 (completing-read "Case: " '("*Messages*") nil t)))
              (passed (equal selected "*Messages*")))
         (switch-to-buffer (get-buffer-create "*completion result*"))
         (erase-buffer)
         (insert (format "Selected: %s\n" selected))
         (message nil)
         (redisplay t)
         (let ((snapshot (getenv "NEOMACS_GUI_FRAME_SNAPSHOT_TXT")))
           (when (and snapshot (fboundp 'neomacs--write-frame-snapshot))
             (neomacs--write-frame-snapshot snapshot t 'text-faces)))
         (with-temp-file (getenv "NEOMACS_GUI_STATE_JSON")
           (insert (format "{\"passed\":%s,\"selected\":\"%s\"}\n"
                           (if passed "true" "false") selected)))
         (completion-fixture-await-result-presentation passed))
     (error (message "Completion fixture: %S" err) (kill-emacs 1)))))
(run-at-time
 8 nil
 (lambda ()
   (princ (format
           "COMPLETION-PRESENTATION-TIMEOUT target=%S receipt=%S png-ready=%S\n"
           completion-fixture-presentation-target
           completion-fixture-last-receipt
           completion-fixture-png-ready)
          'external-debugging-output)
   (kill-emacs 2)))
