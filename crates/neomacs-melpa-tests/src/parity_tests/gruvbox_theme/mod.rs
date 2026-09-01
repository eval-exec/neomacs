use std::time::Duration;

use crate::{CachedMelpaOracle, GRUVBOX_THEME_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const GRUVBOX_THEME_TIMEOUT: Duration = Duration::from_secs(180);

const GRUVBOX_THEME_PRELUDE: &str = r####"
(require 'ansi-color)
(require 'cl-lib)
(require 'diff-mode)
(let ((load-suffixes '(".elc" ".el")))
  (require 'org))
(defconst gruvbox-test-org-compiled
  (let ((source (symbol-file 'org-mode 'defun)))
    (and source (string-suffix-p ".elc" source))))
(unless (and (featurep 'org)
             gruvbox-test-org-compiled
             (not (featurep 'gnus-sum))
             (not (facep 'gnus-group-news-low))
             (equal load-suffixes '(".el")))
  (error "Gruvbox real Org load boundary failed: org=%S/%S gnus=%S face=%S suffixes=%S"
         (featurep 'org) (symbol-file 'org-mode 'defun)
         (featurep 'gnus-sum) (facep 'gnus-group-news-low) load-suffixes))
(require 'gruvbox)

;; Gruvbox supports pdf-tools without requiring it.  Give that documented
;; consumer variable an honest pre-theme value so the public theme lifecycle
;; can prove both mutation and restoration without loading or replacing the
;; optional package.
(defvar pdf-view-midnight-colors '("gruvbox-test-light" . "gruvbox-test-dark"))

(defconst gruvbox-test-themes
  '(gruvbox gruvbox-dark-hard gruvbox-dark-medium gruvbox-dark-soft
    gruvbox-light-hard gruvbox-light-medium gruvbox-light-soft))

(defconst gruvbox-test-representative-faces
  '((default :foreground :background)
    (cursor :background)
    (region :foreground :background)
    (mode-line :foreground :background :box)
    (font-lock-keyword-face :foreground :weight)
    (font-lock-string-face :foreground)
    (org-document-title :foreground :background :weight)
    (org-link :foreground :underline)
    (diff-added :foreground :background)
    (diff-removed :foreground :background)))

;; Theme registration and Autothemer's last-declared palette are irreversible
;; process state.  Make all seven exact source theme files part of the
;; shared editor baseline without enabling any of them.
(dolist (theme gruvbox-test-themes)
  (unless (load-theme theme t t)
    (error "failed to register Gruvbox theme %S" theme)))

(defun gruvbox-test-copy (value)
  "Copy VALUE recursively, including strings and vectors."
  (cond ((consp value)
         (cons (gruvbox-test-copy (car value))
               (gruvbox-test-copy (cdr value))))
        ((vectorp value)
         (apply #'vector (mapcar #'gruvbox-test-copy (append value nil))))
        ((stringp value) (copy-sequence value))
        (t value)))

(defun gruvbox-test-variable-state (symbol)
  "Return SYMBOL's binding and copied value without conflating nil and void."
  (if (boundp symbol)
      (list :bound t :value (gruvbox-test-copy (symbol-value symbol)))
    '(:bound nil)))

(defun gruvbox-test-restore-variable (symbol state)
  "Restore SYMBOL to STATE returned by `gruvbox-test-variable-state'."
  (if (plist-get state :bound)
      (set symbol (gruvbox-test-copy (plist-get state :value)))
    (makunbound symbol)))

(defun gruvbox-test-disable-all ()
  "Disable every Gruvbox theme, newest first."
  (dolist (theme gruvbox-test-themes)
    (when (custom-theme-enabled-p theme)
      (disable-theme theme))))

(defun gruvbox-test-face (face attributes)
  "Return direct and resolved ATTRIBUTES for FACE on the real frame."
  (list
   face
   :direct
   (mapcar (lambda (attribute)
             (cons attribute
                   (gruvbox-test-copy
                    (face-attribute face attribute nil nil))))
           attributes)
   :resolved
   (mapcar (lambda (attribute)
             (cons attribute
                   (gruvbox-test-copy
                    (face-attribute face attribute nil 'default))))
           attributes)))

(defun gruvbox-test-representative-face-state ()
  "Return exact direct/resolved state for practical theme faces."
  (mapcar (lambda (spec)
            (gruvbox-test-face (car spec) (cdr spec)))
          gruvbox-test-representative-faces))

(defvar gruvbox-test-owned-buffers nil)

(defun gruvbox-test-own-buffer (name)
  "Create and register the uniquely named test buffer NAME."
  (when (get-buffer name)
    (error "Gruvbox test buffer already exists: %s" name))
  (let ((buffer (generate-new-buffer name)))
    (push buffer gruvbox-test-owned-buffers)
    buffer))

(defun gruvbox-test-record-cleanup (phase thunk errors)
  "Run THUNK and prepend any condition tagged PHASE to ERRORS."
  (condition-case condition
      (progn (funcall thunk) errors)
    (error (cons (list phase condition) errors))))

(defun gruvbox-test-sweep-resources
    (sweep buffers-before processes-before timers-before errors)
  "Remove resources created after the baseline, continuing after each error."
  (dolist (process (cl-set-difference (process-list) processes-before))
    (setq errors
          (gruvbox-test-record-cleanup
           (list 'process-sweep sweep)
           (lambda () (delete-process process))
           errors)))
  (dolist (timer (cl-set-difference timer-list timers-before))
    (setq errors
          (gruvbox-test-record-cleanup
           (list 'timer-sweep sweep)
           (lambda () (cancel-timer timer))
           errors)))
  (dolist (buffer (cl-set-difference (buffer-list) buffers-before))
    (setq errors
          (gruvbox-test-record-cleanup
           (list 'buffer-sweep sweep)
           (lambda ()
             (when (buffer-live-p buffer)
               (kill-buffer buffer)))
           errors)))
  errors)

(defun gruvbox-test-run (thunk)
  "Run THUNK and prove every theme/editor resource is restored."
  (when (cl-some #'custom-theme-enabled-p gruvbox-test-themes)
    (error "Gruvbox theme leaked into the next workflow: %S"
           custom-enabled-themes))
  (let* ((enabled-before (copy-sequence custom-enabled-themes))
         (known-before (copy-sequence custom-known-themes))
         (ansi-before (gruvbox-test-variable-state 'ansi-color-names-vector))
         (pdf-before (gruvbox-test-variable-state 'pdf-view-midnight-colors))
         (bold-before (gruvbox-test-variable-state 'gruvbox-bold-constructs))
         ;; Autothemer's struct owns the 665 reduced face forms.  The themes
         ;; replace this immutable object rather than mutating it, so identity
         ;; is the truthful and non-recursive restoration witness.
         (autothemer-before autothemer-current-theme)
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (copy-sequence timer-list))
         (current-buffer-before (current-buffer))
         (window-before (selected-window))
         (window-configuration-before (current-window-configuration))
         (faces-before (gruvbox-test-representative-face-state))
         (background-mode-before (frame-parameter nil 'background-mode))
         (gruvbox-test-owned-buffers nil)
         result body-error cleanup-errors state)
    (unwind-protect
        (condition-case condition
            (setq result (funcall thunk))
          (error (setq body-error condition)))
      (setq cleanup-errors
            (gruvbox-test-record-cleanup
             'disable-themes #'gruvbox-test-disable-all cleanup-errors))
      (setq cleanup-errors
            (gruvbox-test-record-cleanup
             'restore-options
             (lambda ()
               (gruvbox-test-restore-variable
                'gruvbox-bold-constructs bold-before)
               ;; Re-evaluate every exact source theme file with the
               ;; baseline option so registered specs cannot leak across cases.
               (dolist (theme gruvbox-test-themes)
                 (load-theme theme t t))
               (setq autothemer-current-theme autothemer-before)
               (gruvbox-test-restore-variable
                'ansi-color-names-vector ansi-before)
               (gruvbox-test-restore-variable
                'pdf-view-midnight-colors pdf-before))
             cleanup-errors))
      (dolist (buffer gruvbox-test-owned-buffers)
        (setq cleanup-errors
              (gruvbox-test-record-cleanup
               'kill-buffer
               (lambda ()
                 (when (buffer-live-p buffer)
                   (kill-buffer buffer)))
               cleanup-errors)))
      (setq cleanup-errors
            (gruvbox-test-record-cleanup
             'restore-window
             (lambda ()
               (unless (compare-window-configurations
                        (current-window-configuration)
                        window-configuration-before)
                 (set-window-configuration window-configuration-before))
               (when (buffer-live-p current-buffer-before)
                 (set-buffer current-buffer-before))
               (when (window-live-p window-before)
                 (select-window window-before)))
             cleanup-errors))
      (dotimes (sweep 2)
        (setq cleanup-errors
              (gruvbox-test-sweep-resources
               sweep buffers-before processes-before timers-before
               cleanup-errors)))
      ;; Buffer teardown can alter window histories; reapply the exact owned
      ;; configuration after both bounded cleanup sweeps.
      (setq cleanup-errors
            (gruvbox-test-record-cleanup
             'restore-window-final
             (lambda ()
               (unless (compare-window-configurations
                        (current-window-configuration)
                        window-configuration-before)
                 (set-window-configuration window-configuration-before))
               (when (buffer-live-p current-buffer-before)
                 (set-buffer current-buffer-before))
               (when (window-live-p window-before)
                 (select-window window-before)))
             cleanup-errors))
      (setq state
            (list
             :enabled (equal custom-enabled-themes enabled-before)
             :known (equal custom-known-themes known-before)
             :ansi (equal (gruvbox-test-variable-state
                           'ansi-color-names-vector)
                          ansi-before)
             :pdf (equal (gruvbox-test-variable-state
                          'pdf-view-midnight-colors)
                         pdf-before)
             :bold (equal (gruvbox-test-variable-state
                           'gruvbox-bold-constructs)
                          bold-before)
             :autothemer (eq autothemer-current-theme autothemer-before)
             :faces
             (equal (gruvbox-test-representative-face-state) faces-before)
             :background-mode
             (eq (frame-parameter nil 'background-mode)
                 background-mode-before)
             :owned-buffers
             (mapcar #'buffer-live-p gruvbox-test-owned-buffers)
             :new-buffers
             (cl-set-difference (buffer-list) buffers-before)
             :new-processes
             (cl-set-difference (process-list) processes-before)
             :new-timers
             (cl-set-difference timer-list timers-before)
             :current-buffer (eq (current-buffer) current-buffer-before)
             :selected-window (eq (selected-window) window-before)
             :window-configuration
             (compare-window-configurations
              (current-window-configuration)
              window-configuration-before))))
    (when (or body-error cleanup-errors
              (not (equal state
                          '(:enabled t :known t :ansi t :pdf t :bold t
                            :autothemer t :faces t
                            :background-mode t :owned-buffers nil
                            :new-buffers nil :new-processes nil :new-timers nil
                            :current-buffer t :selected-window t
                            :window-configuration t))))
      (error "Gruvbox workflow/cleanup failure: body=%S cleanup=%S state=%S"
             body-error (nreverse cleanup-errors) state))
    result))
"####;

fn gruvbox_theme_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(GRUVBOX_THEME_MELPA_PIN, "gruvbox.el")
        .expect("prepare exact Gruvbox Theme source below ./tmp")
        .with_installed_autoloads()
        .with_prelude(GRUVBOX_THEME_PRELUDE)
        .with_timeout(GRUVBOX_THEME_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed Gruvbox Theme parity test")
        .into()
}

fn assert_gruvbox_theme_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        gruvbox_theme_oracle(),
        &current_test_name(),
        "gruvbox_theme_parity",
        cases,
    );
}

#[test]
fn gruvbox_theme_package_batch() {
    assert_gruvbox_theme_batch(&workflows::public_workflow_cases());
}
