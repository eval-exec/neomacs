;;; desktop-font-live.el --- Public live desktop-font oracle -*- lexical-binding: t -*-
(setq native-comp-jit-compilation nil native-comp-enable-subr-trampolines nil)

(defun desktop-font-live-log (format-string &rest args)
  (princ (concat (apply #'format format-string args) "\n")
         'external-debugging-output))

(defun desktop-font-live-state ()
  (list :monospace (font-get-system-font)
        :application (font-get-system-normal-font)
        :family (face-attribute 'default :family)
        :height (face-attribute 'default :height)
        :frame-font (frame-parameter nil 'font)
        :cell (list (window-font-width) (window-font-height))
        :columns (frame-width)
        :dynamic-setting (featurep 'dynamic-setting)
        :handler (lookup-key special-event-map [config-changed-event])
        :opt-in font-use-system-font
        :display (terminal-name)
        :graphical-display (display-graphic-p (terminal-name))
        :display-frames (frames-on-display-list (terminal-name))))

(defun desktop-font-live-await (predicate continuation deadline)
  (condition-case err
      (cond ((funcall predicate) (funcall continuation))
            ((> (float-time) deadline)
             (error "Live font update timed out: %S geometry=%S text=%S receipt=%S"
                    (desktop-font-live-state) (desktop-font-live-frame-state)
                    (list (frame-text-width) (frame-text-height))
                    (desktop-font-live-receipt)))
            (t (run-at-time 0.05 nil #'desktop-font-live-await
                            predicate continuation deadline)))
    (error (desktop-font-live-log "LIVE-FONT-FAIL %S" err) (kill-emacs 1))))

(defun desktop-font-live-write (key value)
  ;; This child inherits only this test's keyfile backend and config root.
  ;; No writes reach the user's desktop settings.
  (unless (eq 0 (call-process "gsettings" nil nil nil "set"
                             "org.gnome.desktop.interface" key value))
    (error "Isolated GSettings writer failed")))

(defun desktop-font-live-frame-state ()
  (list (face-attribute 'default :family) (face-attribute 'default :height)
        (frame-parameter nil 'font) (window-font-width) (window-font-height)
        (frame-width) (frame-height) (frame-pixel-width) (frame-pixel-height)))

(defun desktop-font-live-unchanged (before font)
  (unless (and (equal before (desktop-font-live-frame-state))
               (eq font (face-attribute 'default :font)))
    (error "Preference-only change reopened the font or changed the frame: %S -> %S"
           before (desktop-font-live-frame-state))))

(defun desktop-font-live-pass ()
  (desktop-font-live-log "LIVE-FONT-PASS %S" (desktop-font-live-state))
  (kill-emacs 0))

(defun desktop-font-live-opt-out ()
  (setq font-use-system-font nil)
  (let ((before (desktop-font-live-frame-state))
        (font (face-attribute 'default :font)))
    (desktop-font-live-write "monospace-font-name" "DejaVu Sans Mono 16")
    (desktop-font-live-await
     (lambda () (equal (font-get-system-font) "DejaVu Sans Mono 16"))
     (lambda ()
       (desktop-font-live-unchanged before font)
       ;; Enabling adoption alone must not replay the skipped preference.
       ;; A later application update gives an observed native-event barrier.
       (setq font-use-system-font t)
       (desktop-font-live-write "font-name" "DejaVu Sans 12")
       (desktop-font-live-await
        (lambda () (equal (font-get-system-normal-font) "DejaVu Sans 12"))
        (lambda ()
          (desktop-font-live-unchanged before font)
          (desktop-font-live-pass))
        (+ (float-time) 8)))
     (+ (float-time) 8))))

(defun desktop-font-live-opt-in (&optional continuation)
  (setq font-use-system-font t)
  (desktop-font-live-write "monospace-font-name" "DejaVu Sans Mono 16")
  (desktop-font-live-await
   (lambda ()
     (and (equal (font-get-system-font) "DejaVu Sans Mono 16")
          (equal (face-attribute 'default :family) "DejaVu Sans Mono")
          ;; Independently observed in GNU X11 at 96 logical DPI.
          (= (face-attribute 'default :height) 158)
          (= (window-font-width) 13)
          (= (window-font-height) 25)))
   (lambda ()
     (unless (equal (font-get-system-normal-font) "Ubuntu 10")
       (error "Monospace update replaced the application preference"))
     (funcall (or continuation #'desktop-font-live-pass)))
   (+ (float-time) 8)))

(defun desktop-font-live-explicit-and-future ()
  (set-frame-font "DejaVu Sans Mono 12")
  (let ((existing (make-frame '((font . "Ubuntu Mono 11")))))
    (desktop-font-live-opt-in
     (lambda ()
       (unless (and (equal (face-attribute 'default :family existing) "DejaVu Sans Mono")
                    (= (face-attribute 'default :height existing) 158))
         (error "Explicit font on another existing frame escaped opt-in: %S"
                (frame-parameters existing)))
       (let ((future (make-frame))
             (explicit (make-frame '((font . "Ubuntu Mono 11")))))
         (unless (and (equal (face-attribute 'default :family future) "DejaVu Sans Mono")
                      (= (face-attribute 'default :height future) 158))
           (error "Ordinary future frame did not inherit live defaults: %S"
                  (frame-parameters future)))
         (unless (equal (face-attribute 'default :family explicit) "Ubuntu Mono")
           (error "Explicit future-frame font lost precedence: %S"
                  (frame-parameters explicit)))
         (desktop-font-live-pass))))))

(defun desktop-font-live-receipt ()
  (let ((file (getenv "NEOMACS_GUI_PRESENTATION_RECEIPT")))
    (when (and file (file-exists-p file))
      (with-temp-buffer (insert-file-contents file) (read (current-buffer))))))

(defun desktop-font-live-presented (after)
  (or (not (getenv "NEOMACS_GUI_PRESENTATION_RECEIPT"))
      (let ((receipt (desktop-font-live-receipt)))
        (and receipt
             (eq (plist-get receipt :outcome) 'presented)
             (> (plist-get receipt :submission) after)
             (= (plist-get receipt :width) (frame-pixel-width))
             (= (plist-get receipt :height) (frame-pixel-height))
             (integerp (plist-get receipt :clock-id))
             (natnump (plist-get receipt :seconds))
             (natnump (plist-get receipt :nanoseconds))
             (< (plist-get receipt :nanoseconds) 1000000000)))))

(defun desktop-font-live-grid-ready ()
  (and (= (frame-width) 80) (= (frame-text-lines) 24) (= (frame-height) 24)
       (= (frame-text-width) (* 80 (frame-char-width)))
       (= (frame-text-height) (* 24 (frame-char-height)))))

(defun desktop-font-live-geometry (&optional continuation)
  ;; Isolate the font-owned text grid from platform-specific bar chrome.
  (menu-bar-mode -1)
  (tool-bar-mode -1)
  (set-frame-size nil 80 24)
  (desktop-font-live-await
   (lambda () (and (desktop-font-live-grid-ready)
                   (desktop-font-live-presented 0)))
   (lambda ()
     (let ((submission (or (plist-get (desktop-font-live-receipt) :submission) 0)))
       (desktop-font-live-log "LIVE-GEOMETRY-BEFORE %S" (desktop-font-live-frame-state))
       (desktop-font-live-opt-in
        (lambda ()
          (desktop-font-live-await
           (lambda () (and (desktop-font-live-grid-ready)
                           (desktop-font-live-presented submission)))
           (lambda ()
             (desktop-font-live-log "LIVE-GEOMETRY-AFTER %S receipt=%S"
                                    (desktop-font-live-frame-state)
                                    (desktop-font-live-receipt))
             (funcall (or continuation #'desktop-font-live-pass)))
           (+ (float-time) 8))))))
   (+ (float-time) 8)))

(defun desktop-font-live-overlapping-resize ()
  (menu-bar-mode -1)
  (tool-bar-mode -1)
  (set-frame-size nil 80 24)
  (desktop-font-live-await
   #'desktop-font-live-grid-ready
   (lambda ()
     (let ((submission (or (plist-get (desktop-font-live-receipt) :submission) 0)))
       ;; Native observations may interleave or coalesce these requests. The
       ;; deterministic core control supplies the specifically older size.
       (set-frame-size nil 85 24)
       (set-frame-size nil 91 25)
       (set-face-attribute 'default nil :font "DejaVu Sans Mono 16")
       (desktop-font-live-await
        (lambda () (and (= (frame-width) 91) (= (frame-text-lines) 25)
                        (= (frame-char-width) 13) (= (frame-char-height) 25)
                        (desktop-font-live-presented submission)))
        #'desktop-font-live-pass (+ (float-time) 8))))
   (+ (float-time) 8)))

(defun desktop-font-live-repeated ()
  (desktop-font-live-geometry
   (lambda ()
     (let ((before (desktop-font-live-frame-state))
           (font (face-attribute 'default :font)))
       (desktop-font-live-write "monospace-font-name" "DejaVu Sans Mono 16")
       ;; The later role change is an observable native-event barrier. No-op
       ;; success must not be inferred merely from a timer elapsing.
       (desktop-font-live-write "font-name" "DejaVu Sans 12")
       (desktop-font-live-await
        (lambda () (equal (font-get-system-normal-font) "DejaVu Sans 12"))
        (lambda ()
          (desktop-font-live-unchanged before font)
          (let ((submission (or (plist-get (desktop-font-live-receipt) :submission) 0)))
            (desktop-font-live-write "monospace-font-name" "Ubuntu Mono 13")
            (desktop-font-live-await
             (lambda ()
               (and (equal (font-get-system-font) "Ubuntu Mono 13")
                    (equal (face-attribute 'default :family) "Ubuntu Mono")
                    (= (face-attribute 'default :height) 128)
                    (= (window-font-width) 9) (= (window-font-height) 18)
                    (desktop-font-live-grid-ready)
                    (desktop-font-live-presented submission)))
             #'desktop-font-live-pass
             (+ (float-time) 8))))
        (+ (float-time) 8))))))

(defun desktop-font-live-inhibited ()
  (desktop-font-live-geometry
   (lambda ()
     (setq frame-inhibit-implied-resize t)
     (desktop-font-live-keep-pixels))))

(defun desktop-font-live-keep-pixels ()
  (let ((pixels (list (frame-pixel-width) (frame-pixel-height)))
        (submission (or (plist-get (desktop-font-live-receipt) :submission) 0)))
    (desktop-font-live-write "monospace-font-name" "Ubuntu Mono 13")
    (desktop-font-live-await
     (lambda ()
       (and (equal (font-get-system-font) "Ubuntu Mono 13")
            (equal (face-attribute 'default :family) "Ubuntu Mono")
            (= (window-font-width) 9) (= (window-font-height) 18)
            (desktop-font-live-presented submission)))
     (lambda ()
       (unless (equal pixels (list (frame-pixel-width) (frame-pixel-height)))
         (error "Inhibited font update changed native pixels"))
       (unless (= (frame-width) (/ (frame-text-width) (frame-char-width)))
         (error "Inhibited font update left stale columns: %S text=%S"
                (desktop-font-live-frame-state) (frame-text-width)))
       (desktop-font-live-pass))
     (+ (float-time) 8))))

(defun desktop-font-live-fullscreen ()
  (desktop-font-live-geometry
   (lambda ()
     (let ((width (frame-pixel-width)) (height (frame-pixel-height))
           (submission (or (plist-get (desktop-font-live-receipt) :submission) 0)))
       (set-frame-parameter nil 'fullscreen 'fullboth)
       (desktop-font-live-await
        (lambda () (and (> (frame-pixel-width) width) (> (frame-pixel-height) height)
                        (desktop-font-live-presented submission)))
        (lambda ()
          (setq frame-inhibit-implied-resize nil)
          (desktop-font-live-keep-pixels))
        (+ (float-time) 8))))))

(defun desktop-font-live-grown-minibuffer ()
  (menu-bar-mode -1)
  (tool-bar-mode -1)
  (setq resize-mini-windows nil)
  (set-frame-size nil 80 24)
  (desktop-font-live-await
   #'desktop-font-live-grid-ready
   (lambda ()
     (window-resize (minibuffer-window) 2)
     (unless (= (window-pixel-height (minibuffer-window)) (* 3 (frame-char-height)))
       (error "Minibuffer did not grow to three rows"))
     (let ((height (frame-native-height))
           (submission (or (plist-get (desktop-font-live-receipt) :submission) 0)))
       (set-frame-width nil 81)
       (desktop-font-live-await
        (lambda ()
          (and (= (frame-width) 81) (= (frame-height) 24)
               (= (frame-text-lines) 24) (= (frame-native-height) height)
               (= (window-pixel-height (minibuffer-window)) (* 3 (frame-char-height)))
               (desktop-font-live-presented submission)))
        (lambda ()
          (desktop-font-live-opt-in
           (lambda ()
             (desktop-font-live-await
              (lambda ()
                (and (= (frame-width) 81) (= (frame-height) 24)
                     (= (frame-text-lines) 24)
                     (= (frame-text-height) (* 24 (frame-char-height)))
                     (= (+ (window-pixel-height (frame-root-window))
                           (window-pixel-height (minibuffer-window)))
                        (frame-text-height))
                     (desktop-font-live-presented submission)))
              #'desktop-font-live-pass (+ (float-time) 8)))))
        (+ (float-time) 8))))
   (+ (float-time) 8)))

(defun desktop-font-live-minimum (&optional vertical)
  (menu-bar-mode -1)
  (tool-bar-mode -1)
  (setq window-min-width 40)
  (setq window-min-height 10)
  (when vertical
    (modify-frame-parameters nil '((left-fringe . 0) (right-fringe . 0)
                                   (vertical-scroll-bars . nil))))
  ;; 450px is a whole number of both 18px and 25px rows, so the X11
  ;; window manager's size increments cannot obscure axis inhibition.
  ;; 936px is divisible by both 9px and 13px columns for the vertical control.
  (set-frame-size nil (if vertical 104 100) 25)
  (desktop-font-live-await
   (lambda () (and (= (frame-width) (if vertical 104 100)) (= (frame-text-lines) 25)))
   (lambda ()
     (if vertical (split-window-below) (split-window-right))
     (modify-frame-parameters nil '((min-width . nil) (min-height . nil)))
     (setq frame-inhibit-implied-resize t)
     (let ((width (frame-native-width)) (height (frame-native-height))
           (submission (or (plist-get (desktop-font-live-receipt) :submission) 0)))
       (desktop-font-live-opt-in
        (lambda ()
          (desktop-font-live-await
           (lambda ()
             (and (if vertical
                      (and (> (frame-native-height) height)
                           (>= (frame-native-height) (frame-windows-min-size nil nil nil t))
                           (= (frame-native-width) width))
                    (and (> (frame-native-width) width)
                         (>= (frame-native-width) (frame-windows-min-size nil t nil t))
                         (= (frame-native-height) height)))
                  (desktop-font-live-presented submission)))
           #'desktop-font-live-pass
           (+ (float-time) 8))))))
   (+ (float-time) 8)))

(defun desktop-font-live-child (&optional chrome)
  (let ((child (make-frame `((parent-frame . ,(selected-frame))
                             (font . ,(if (eq chrome 'scrollbar) "DejaVu Sans Mono 12" "Ubuntu Mono 13"))
                             (minibuffer . nil)
                             (width . 40) (height . 10) (undecorated . t)
                             (left-fringe . ,(if chrome 7 0)) (right-fringe . ,(if chrome 11 0))
                             (vertical-scroll-bars . ,(when (eq chrome 'scrollbar) 'right))
                             (scroll-bar-width . 17) (internal-border-width . ,(if chrome 3 0))
                             (menu-bar-lines . 0) (tool-bar-lines . 0)))))
    (desktop-font-live-log "LIVE-CHILD-BEFORE %S text=%S"
                           (frame-parameters child)
                           (list (frame-text-width child) (frame-text-height child)))
    (desktop-font-live-await
     (lambda () (and (= (frame-text-width child) (* 40 (frame-char-width child)))
                     (= (frame-text-height child) (* 10 (frame-char-height child)))
                     (or (not (eq chrome 'scrollbar))
                         (and (/= (frame-char-width child) (frame-char-width))
                              (> (frame-scroll-bar-width child) 0)))
                     (or (not chrome)
                         (= (frame-native-width child)
                            (+ (* 40 (frame-char-width child)) 24
                               (if (eq chrome 'scrollbar) (frame-scroll-bar-width child) 0))))))
     (lambda ()
       (desktop-font-live-opt-in
        (lambda ()
          (desktop-font-live-log "LIVE-CHILD-AFTER %S text=%S"
                                 (frame-parameters child)
                                 (list (frame-text-width child) (frame-text-height child)))
          (desktop-font-live-await
           (lambda ()
             (and (= (frame-char-width child) 13) (= (frame-char-height child) 25)
                  (= (frame-text-width child) (* 40 13))
                  (= (frame-text-height child) (* 10 25))))
           #'desktop-font-live-pass
           (+ (float-time) 8)))))
     (+ (float-time) 8))))

(defun desktop-font-live-probe ()
  "Everything the isolation failure needs on a runner: the setting
values as GSettings itself sees them, the harness env, and the
keyfile contents the keyfile backend would read."
  (let ((probe
         (lambda (key)
           (condition-case nil
               (format "%s=%S" key
                       (with-output-to-string
                         (call-process "gsettings" nil standard-output nil
                                       "get" "org.gnome.desktop.interface" key)))
             (error (format "%s=<gsettings unreadable>" key))))))
    (list
     (funcall probe "monospace-font-name")
     (funcall probe "font-name")
     (cons 'schema-dir (getenv "GSETTINGS_SCHEMA_DIR"))
     (cons 'backend (getenv "GSETTINGS_BACKEND"))
     (cons 'config-home (getenv "XDG_CONFIG_HOME"))
     (cons 'data-dirs (getenv "XDG_DATA_DIRS"))
     (cons 'keyfile
           (condition-case nil
               (with-temp-buffer
                 (insert-file-contents
                  (expand-file-name "glib-2.0/settings/keyfile"
                                    (or (getenv "XDG_CONFIG_HOME") "~")))
                 (buffer-string))
             (error "<no keyfile>"))))))

(run-at-time
 0.1 nil
 (lambda ()
   (condition-case err
       (progn
         (unless (equal (font-get-system-font) "Ubuntu Mono 13")
           (error "Initial settings are not isolated: %S probe: %S"
                  (desktop-font-live-state) (desktop-font-live-probe)))
         (desktop-font-live-log "LIVE-FONT-BEFORE %S" (desktop-font-live-state))
         (pcase (getenv "NEOMACS_GUI_LIVE_FONT_CASE")
           ("opt-in" (desktop-font-live-opt-in))
           ("opt-out" (desktop-font-live-opt-out))
           ("explicit-and-future" (desktop-font-live-explicit-and-future))
           ("geometry" (desktop-font-live-geometry))
           ("repeated" (desktop-font-live-repeated))
           ("inhibited" (desktop-font-live-inhibited))
           ("child" (desktop-font-live-child))
           ("child-chrome" (desktop-font-live-child t))
           ("child-scrollbar" (desktop-font-live-child 'scrollbar))
           ("minimum" (desktop-font-live-minimum))
           ("minimum-vertical" (desktop-font-live-minimum t))
           ("grown-minibuffer" (desktop-font-live-grown-minibuffer))
           ("overlapping-resize" (desktop-font-live-overlapping-resize))
           ("fullscreen" (desktop-font-live-fullscreen))
           (_ (error "Unknown live font case: %S" (getenv "NEOMACS_GUI_LIVE_FONT_CASE")))))
     (error (desktop-font-live-log "LIVE-FONT-FAIL %S" err) (kill-emacs 1)))))
