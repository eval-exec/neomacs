;;; early-init.el --- Initial GUI frame lifecycle regression -*- lexical-binding: t; -*-

;; Capture the first user-visible state before configuring anything.
(defvar issue-417-first-state
  (list (framep (selected-frame))
        (frame-parameter nil 'background-color)
        (mapcar #'framep (frame-list))
        (and (boundp 'terminal-frame) terminal-frame
             (framep terminal-frame))
        (if (display-graphic-p) "gui" "tty")
        (if window-system "gui" "tty")))

(setq package-enable-at-startup nil
      native-comp-jit-compilation nil
      native-comp-enable-subr-trampolines nil
      inhibit-startup-screen t)
(defvar issue-417-events nil)

(defun issue-417-kind (kind)
  (cond ((eq kind t) "tty") (kind "gui") (t "absent")))

(defun issue-417-record (tag &optional frame saved)
  (let ((state (or saved
                   (list (framep (selected-frame))
                         (frame-parameter nil 'background-color)
                         (mapcar #'framep (frame-list))
                         (and (boundp 'terminal-frame) terminal-frame
                              (framep terminal-frame))
                         (if (display-graphic-p) "gui" "tty")
                         (if window-system "gui" "tty")))))
    (push `((tag . ,tag)
            (selected_kind . ,(issue-417-kind (nth 0 state)))
            (background . ,(format "%s" (nth 1 state)))
            (frame_kinds . ,(vconcat (mapcar #'issue-417-kind (nth 2 state))))
            (terminal_kind . ,(issue-417-kind (nth 3 state)))
            (display_kind . ,(nth 4 state))
            (window_system_kind . ,(nth 5 state))
            (display_pixels . ,(if (equal (nth 4 state) "gui")
                                   (vector (display-pixel-width)
                                           (display-pixel-height))
                                 [0 0]))
            (created_kind . ,(if frame (issue-417-kind (framep frame)) "absent"))
            (created_background . ,(if frame
                                      (frame-parameter frame 'background-color)
                                    "absent")))
          issue-417-events)))

(issue-417-record "early-init" nil issue-417-first-state)

(defun issue-417-write-json (path value)
  (let ((temporary (concat path ".next")))
    (with-temp-file temporary
      (insert (json-serialize value)))
    (rename-file temporary path t)))

;; Exiting here must not allocate a GUI frame or lose the requested status.
(when-let* ((status (getenv "NEOMACS_ISSUE_417_EARLY_EXIT")))
  (issue-417-write-json (getenv "NEOMACS_ISSUE_417_TRACE")
                        (vconcat (reverse issue-417-events)))
  (kill-emacs (string-to-number status)))

;; This owned file barrier lets Rust inspect the actual X11 window tree while
;; early-init is still executing, independently of Lisp frame introspection.
(when-let* ((ready (getenv "NEOMACS_ISSUE_417_EARLY_READY"))
            (release (getenv "NEOMACS_ISSUE_417_EARLY_RELEASE")))
  (issue-417-write-json ready
                        `((pid . ,(emacs-pid))
                          (events . ,(vconcat (reverse issue-417-events)))))
  (let ((deadline (+ (float-time) 20)))
    (while (not (file-exists-p release))
      (when (> (float-time) deadline)
        (error "Early-init native-window barrier was not released"))
      (sleep-for 0.01))))

(setq default-frame-alist
      (if (getenv "NEOMACS_ISSUE_417_INVALID_INITIAL_FRAME")
          '((background-color . "#00ff00") (alpha . 999))
        '((background-color . "#00ff00"))))
;; An invisible opening frame still runs normal startup and frame hooks.
;; The native backend must preserve this creation-time parameter as well.
(when (getenv "NEOMACS_ISSUE_417_INVISIBLE_INITIAL")
  (push '(visibility . nil) initial-frame-alist))

(when (getenv "NEOMACS_ISSUE_417_INVALID_INITIAL_FRAME")
  ;; Do not catch the frame creation error: the real startup error boundary
  ;; must report it and terminate while native startup is still pending.
  (issue-417-write-json (getenv "NEOMACS_ISSUE_417_TRACE")
                        (vconcat (reverse issue-417-events)))
  ;; GNU startup.el runs this hook after terminal/window-system preload,
  ;; immediately before frame-initialize.  Measure constructor failure from
  ;; here so cold source preload does not consume the prompt-exit budget.
  (add-hook
   'before-init-hook
   (lambda ()
     (issue-417-write-json
      (getenv "NEOMACS_ISSUE_417_INVALID_FRAME_READY")
      `((pid . ,(emacs-pid))
        (events . ,(vconcat (reverse issue-417-events))))))
   t))
(when (getenv "NEOMACS_ISSUE_417_RECOVERABLE_FRAME_ERROR")
  ;; An error caught by Lisp is not a fatal startup failure.  Use the actual
  ;; constructor after window-system initialization, before the opening
  ;; frame is created. Malformed parameters fail before allocating a frame
  ;; in both GNU and NeoMacs, independently of native parameter ordering.
  (add-hook
   'before-init-hook
   (lambda ()
     (condition-case err
         (progn
           (make-frame 42)
           (error "Malformed frame parameters unexpectedly created a frame"))
       (wrong-type-argument
        (issue-417-record "caught-frame-error")
        (push (cons 'error_condition (symbol-name (car err)))
              (car issue-417-events)))))
   t))
(when (getenv "NEOMACS_ISSUE_417_RETIRE_RAW_INVISIBLE")
  ;; A raw invisible frame can be retired before Lisp creates the opening
  ;; frame.  Neither constructor hooks nor the later lifecycle trace should
  ;; observe that discarded frame.
  (add-hook
   'before-init-hook
   (lambda ()
     (let ((frame (x-create-frame '((visibility . nil)))))
       (unless (and (display-graphic-p frame)
                    (null (frame-visible-p frame)))
         (error "Raw frame was not an invisible graphical frame"))
       (when-let* ((progress (getenv "NEOMACS_ISSUE_417_FLOOD_PROGRESS")))
         ;; These public parameter updates occur before the first native
         ;; readiness publication. A bounded render queue must not strand the
         ;; evaluator before it can publish or retire this logical frame.
         (issue-417-write-json
          progress `((pid . ,(emacs-pid))
                     (phase . "before-alpha-updates") (count . 0)))
         (dotimes (index 256)
           (set-frame-parameter frame 'alpha (if (= (% index 2) 0) 80 90)))
         (issue-417-write-json
          progress `((pid . ,(emacs-pid))
                     (phase . "after-alpha-updates") (count . 256))))
       ;; Pending native intents belong to this constructor.  Its replacement
       ;; must keep its own visibility and ordinary window defaults.
       (iconify-frame frame)
       (set-frame-parameter frame 'fullscreen 'maximized)
       (set-frame-parameter frame 'undecorated t)
       (delete-frame frame))
     (unless (and (eq (framep (selected-frame)) t)
                  (equal (mapcar #'framep (frame-list)) '(t)))
       (error "Retiring raw invisible frame did not restore terminal-only state")))
   t))
(add-hook 'before-init-hook (lambda () (issue-417-record "before-init")))
(add-hook 'before-make-frame-hook
          (lambda () (issue-417-record "before-make-frame")))
(add-hook 'after-make-frame-functions
          (lambda (frame) (issue-417-record "after-make-frame" frame)))
(add-hook 'after-init-hook (lambda () (issue-417-record "after-init")))
(add-hook 'emacs-startup-hook (lambda () (issue-417-record "emacs-startup")))

(defun issue-417-await-initial-visibility-file (file)
  (let ((deadline (+ (float-time) 20)))
    (while (not (file-exists-p file))
      (when (> (float-time) deadline)
        (error "Invisible initial-frame native observation was not released"))
      (sleep-for 0.01))))

(defun issue-417-observe-invisible-initial-frame ()
  (let ((frame (selected-frame)))
    (issue-417-write-json
     (getenv "NEOMACS_ISSUE_417_INVISIBLE_READY")
     `((pid . ,(emacs-pid))
       (selected_kind . ,(issue-417-kind (framep frame)))
       (logical_visibility . ,(format "%S" (frame-visible-p frame)))))
    (issue-417-await-initial-visibility-file
     (getenv "NEOMACS_ISSUE_417_INVISIBLE_RELEASE"))
    (make-frame-visible frame)
    (issue-417-write-json
     (getenv "NEOMACS_ISSUE_417_VISIBLE_READY")
     `((pid . ,(emacs-pid))
       (logical_visibility . ,(format "%S" (frame-visible-p frame)))))
    ;; Keep the editor alive until Rust observes actual native mapping.
    (issue-417-await-initial-visibility-file
     (getenv "NEOMACS_ISSUE_417_VISIBLE_ACK"))))

(when (getenv "NEOMACS_ISSUE_417_INVISIBLE_INITIAL")
  (add-hook 'after-init-hook #'issue-417-observe-invisible-initial-frame t))

(defun issue-417-finish ()
  (condition-case err
      (progn
        (issue-417-record "settled")
        (issue-417-write-json (getenv "NEOMACS_ISSUE_417_TRACE")
                              (vconcat (reverse issue-417-events)))
        (kill-emacs 0))
    (error (message "ISSUE-417-FAIL: %S" err) (kill-emacs 1))))

(add-hook 'window-setup-hook
          (lambda ()
            (issue-417-record "window-setup")
            (run-at-time 0.15 nil #'issue-417-finish)))
