;;; neomacs-showcase-effects-test.el --- transient effect session regression -*- lexical-binding: t; -*-

;; Run against the pinned GNU Emacs from parity-reference.toml:
;;
;;   src/emacs -Q --batch -l test/neomacs/neomacs-showcase-effects-test.el
;;
;; The Neomacs renderer subrs are stubbed with the same two rules the Rust
;; side uses: `neomacs--effect-set' merges properties onto the current
;; config, and `neomacs--effects-apply' starts from the Rust defaults and
;; then applies the profile. `cursor-color-cycle' defaults to enabled.

(defvar showcase-effects-test-failures nil)

(defun showcase-effects-test-fail (fmt &rest args)
  (let ((msg (apply #'format fmt args)))
    (push msg showcase-effects-test-failures)
    (princ (format "FAIL %s\n" msg))))

(defun showcase-effects-test-enabled (plist)
  (plist-get plist :enabled))

(defun showcase-effects-test-reset-session ()
  (when (boundp 'showcase--saved-renderer)
    (setq showcase--saved-renderer nil))
  (when (boundp 'showcase--saved-options)
    (setq showcase--saved-options nil)))

(defun showcase-effects-test-custom-clean (symbol)
  (put symbol 'customized-value nil)
  (put symbol 'theme-value nil)
  (put symbol 'saved-value nil)
  (put symbol 'custom-check-value nil))

(defvar neo-test-config (make-hash-table :test #'eq))

(defun neo-test-default (effect)
  (if (eq effect 'cursor-color-cycle)
      (list :enabled t)
    (list :enabled nil)))

(defun neomacs-effect-names (&optional scope)
  (let ((shaders '(aurora breathing-border cursor-color-cycle cursor-firework
                         cursor-glow focus-gradient-border focus-mode
                         inactive-dim indent-guides line-highlight minimap
                         scroll-progress show-whitespace zen-mode))
        (behavior '(buffer-transition cursor-blink cursor-motion
                                      cursor-size-transition scroll-transition
                                      window-animations window-close
                                      window-movement window-open
                                      window-resize child-frame-animations
                                      child-frame-close child-frame-movement
                                      child-frame-open child-frame-resize)))
    (cond ((eq scope 'shader) shaders)
          ((eq scope 'behavior) behavior)
          ((eq scope 'cursor) '(cursor-color-cycle cursor-firework cursor-glow))
          (t (append shaders behavior)))))

(defun neomacs-effect-schema (effect)
  ;; These two publish :off / :slowdown, not :enabled. A stub :enabled
  ;; entry makes `neomacs-effects--check-schema' warn about a symbol the
  ;; real file deliberately does not define.
  (if (memq effect '(window-animations child-frame-animations))
      nil
    '((:enabled boolean))))

(defun neomacs-effect-get (effect)
  (copy-sequence (or (gethash effect neo-test-config)
                     (neo-test-default effect))))

(defun neomacs--effect-set (effect &rest plist)
  (let ((current (neomacs-effect-get effect)))
    (while plist
      (setq current (plist-put current (car plist) (cadr plist)))
      (setq plist (cddr plist)))
    (puthash effect current neo-test-config)
    nil))

(defun neomacs--effects-apply (profile)
  (clrhash neo-test-config)
  (dolist (effect (neomacs-effect-names))
    (puthash effect (neo-test-default effect) neo-test-config))
  (dolist (entry profile)
    (apply #'neomacs--effect-set entry)))

(defun showcase-effects-test-user-set (symbol value)
  (let ((setter (get symbol 'custom-set)))
    (if setter
        (funcall setter symbol value)
      (set-default symbol value))))

(let* ((root (expand-file-name "../.." (file-name-directory load-file-name)))
       (lisp (expand-file-name "lisp" root)))
  ;; Load Neomacs's effects library by path. Do not put its `lisp/'
  ;; directory on `load-path': that tree replaces GNU macros such as
  ;; `defun' and the showcase then fails to expand.
  (setq showcase--inhibit-init t)
  (load (expand-file-name "neomacs-effects.el" lisp))
  (load (expand-file-name "test/neomacs/neomacs-showcase.el" root)))

(unless (boundp 'neomacs-effect-cursor-firework)
  (showcase-effects-test-fail "gallery option neomacs-effect-cursor-firework was not defined"))

;; A transient enable must not become a user customization, and leaving the
;; effect must turn it off even when a saved Custom value says enabled.
(let ((sym 'neomacs-effect-cursor-firework))
  (showcase-effects-test-reset-session)
  (showcase-effects-test-custom-clean sym)
  (showcase-effects-test-user-set sym '(:enabled nil))
  (put sym 'saved-value (list (list 'quote '(:enabled t))))
  (showcase--effect-enable 'cursor-firework)
  (when (get sym 'customized-value)
    (showcase-effects-test-fail
     "enable stored customized-value %S" (get sym 'customized-value)))
  (when (get sym 'theme-value)
    (showcase-effects-test-fail
     "enable stored theme-value %S" (get sym 'theme-value)))
  (unless (eq (showcase-effects-test-enabled (symbol-value sym)) t)
    (showcase-effects-test-fail "enable left option %S" (symbol-value sym)))
  (unless (eq (showcase-effects-test-enabled (neomacs-effect-get 'cursor-firework)) t)
    (showcase-effects-test-fail
     "enable left renderer %S" (neomacs-effect-get 'cursor-firework)))
  (showcase--effect-reset 'cursor-firework)
  (when (eq (showcase-effects-test-enabled (neomacs-effect-get 'cursor-firework)) t)
    (showcase-effects-test-fail
     "reset re-enabled firework from saved-value; renderer %S option %S"
     (neomacs-effect-get 'cursor-firework) (symbol-value sym)))
  (unless (equal (get sym 'saved-value) (list (list 'quote '(:enabled t))))
    (showcase-effects-test-fail "reset rewrote saved-value %S" (get sym 'saved-value)))
  (when (get sym 'customized-value)
    (showcase-effects-test-fail
     "reset stored customized-value %S" (get sym 'customized-value))))

;; A section reset must leave the option and the renderer describing the
;; same effect, and quitting must put the user's value back in both places.
(let ((sym 'neomacs-effect-breathing-border)
      (user '(:enabled t :color "#112233")))
  (showcase-effects-test-reset-session)
  (showcase-effects-test-custom-clean sym)
  (showcase-effects-test-user-set sym user)
  (showcase--reset-all-effects)
  (let ((option (symbol-value sym))
        (renderer (neomacs-effect-get 'breathing-border)))
    (unless (eq (showcase-effects-test-enabled option)
                (showcase-effects-test-enabled renderer))
      (showcase-effects-test-fail
       "reset-all desynced breathing-border option %S renderer %S"
       option renderer)))
  (when (get sym 'customized-value)
    (showcase-effects-test-fail
     "reset-all stored customized-value %S" (get sym 'customized-value)))
  (showcase-quit)
  (let ((option (symbol-value sym))
        (renderer (neomacs-effect-get 'breathing-border)))
    (unless (and (eq (showcase-effects-test-enabled option) t)
                 (equal (plist-get option :color) "#112233"))
      (showcase-effects-test-fail "quit left option %S" option))
    (unless (and (eq (showcase-effects-test-enabled renderer) t)
                 (equal (plist-get renderer :color) "#112233"))
      (showcase-effects-test-fail "quit left renderer %S" renderer)))
  (when (get sym 'customized-value)
    (showcase-effects-test-fail
     "quit stored customized-value %S" (get sym 'customized-value))))

;; The one default-on shader is off for a clean section and back on after quit.
(let ((sym 'neomacs-effect-cursor-color-cycle))
  (showcase-effects-test-reset-session)
  (showcase-effects-test-custom-clean sym)
  (showcase-effects-test-user-set sym '(:enabled t))
  (showcase--reset-all-effects)
  (when (or (showcase-effects-test-enabled (symbol-value sym))
            (showcase-effects-test-enabled (neomacs-effect-get 'cursor-color-cycle)))
    (showcase-effects-test-fail
     "clean slate left color-cycle option %S renderer %S"
     (symbol-value sym) (neomacs-effect-get 'cursor-color-cycle)))
  (showcase-quit)
  (unless (and (eq (showcase-effects-test-enabled (symbol-value sym)) t)
               (eq (showcase-effects-test-enabled
                    (neomacs-effect-get 'cursor-color-cycle))
                   t))
    (showcase-effects-test-fail
     "quit did not restore color-cycle option %S renderer %S"
     (symbol-value sym) (neomacs-effect-get 'cursor-color-cycle))))

;; A profile must not turn the renderer on while leaving the option off,
;; and it must not wipe an effect the profile does not mention.
(if (not (fboundp 'showcase--apply-profile))
    (showcase-effects-test-fail "showcase--apply-profile is not defined")
  (let ((highlight 'neomacs-effect-line-highlight)
        (aurora 'neomacs-effect-aurora))
    (showcase-effects-test-reset-session)
    (showcase-effects-test-custom-clean highlight)
    (showcase-effects-test-custom-clean aurora)
    (showcase-effects-test-user-set highlight '(:enabled t))
    (showcase--reset-all-effects)
    (showcase--apply-profile '((aurora :enabled t :opacity 0.2)))
    (when (or (showcase-effects-test-enabled (symbol-value highlight))
              (showcase-effects-test-enabled (neomacs-effect-get 'line-highlight)))
      (showcase-effects-test-fail
       "profile resurrected line-highlight option %S renderer %S"
       (symbol-value highlight) (neomacs-effect-get 'line-highlight)))
    (unless (and (eq (showcase-effects-test-enabled (symbol-value aurora)) t)
                 (eq (showcase-effects-test-enabled (neomacs-effect-get 'aurora)) t))
      (showcase-effects-test-fail
       "profile desynced aurora option %S renderer %S"
       (symbol-value aurora) (neomacs-effect-get 'aurora)))
    (when (get aurora 'customized-value)
      (showcase-effects-test-fail
       "profile stored customized-value %S" (get aurora 'customized-value)))
    (showcase-quit)
    (unless (and (eq (showcase-effects-test-enabled (symbol-value highlight)) t)
                 (eq (showcase-effects-test-enabled (neomacs-effect-get 'line-highlight)) t))
      (showcase-effects-test-fail
       "quit lost line-highlight option %S renderer %S"
       (symbol-value highlight) (neomacs-effect-get 'line-highlight)))
    (when (showcase-effects-test-enabled (neomacs-effect-get 'aurora))
      (showcase-effects-test-fail
       "quit left aurora renderer %S" (neomacs-effect-get 'aurora)))))

(when showcase-effects-test-failures
  (princ (format "FAILED %d\n" (length showcase-effects-test-failures)))
  (kill-emacs 1))
(princ "PASS\n")
