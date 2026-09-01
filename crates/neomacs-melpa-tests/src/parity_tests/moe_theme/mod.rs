//! Practical parity for Moe Theme's public color-scheme workflows.
//!
//! The cases load its light and dark themes, apply the documented modeline
//! customization, switch palette flavours, and drive the timed global
//! switcher through an owned timer boundary.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, MOE_THEME_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(240);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'org)
(require 'moe-theme)
(require 'moe-theme-flavours)
(require 'moe-theme-switcher)

(defconst moe404-test-source-manifest
  '(("moe-dark-theme.el" . "1e14caef1e8e2d59806b1ca1616e749e053d0dab59a772cddc9f1186a43f131e")
    ("moe-light-theme.el" . "5e659a95065f21f80bbb6cd950cdc1da1cb3ff10226aa240bf36d04dc178b3a1")
    ("moe-theme-flavours.el" . "26ac290017c496cd43ca4af334dba3e8b41a16654e32a449496f5e1214c2da0b")
    ("moe-theme-switcher.el" . "49c0010efec51c69a30a102487af60a4c81ea4a9e70dc524a6dc853183fcb836")
    ("moe-theme.el" . "576eafa440a990d62ad5604dda5b7902baa78d7207432e0b001f28ff7b2ae428")))

(defun moe404-test-file-sha256 (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(let* ((source (symbol-file 'moe-light 'defun))
       (directory (and source (file-name-directory source)))
       (payload
        (and directory
             (sort (seq-filter
                    (lambda (name)
                      (and (string-suffix-p ".el" name)
                           (not (string-suffix-p "-autoloads.el" name))
                           (not (string-suffix-p "-pkg.el" name))))
                    (directory-files directory nil nil t))
                   #'string<))))
  (unless (and source
               (equal payload (mapcar #'car moe404-test-source-manifest)))
    (error "Unexpected installed Moe Theme payload: %S" payload))
  (dolist (entry moe404-test-source-manifest)
    (let ((file (expand-file-name (car entry) directory)))
      (unless (and (file-regular-p file)
                   (not (file-symlink-p file))
                   (equal (moe404-test-file-sha256 file) (cdr entry)))
        (error "Unexpected installed Moe Theme source: %S" entry)))))

(defun moe404-test-condition (condition)
  (list :type (car condition)
        :data (copy-tree (cdr condition))
        :message (error-message-string condition)))

(defun moe404-test-face-state (face)
  (list :foreground (face-attribute face :foreground nil 'default)
        :background (face-attribute face :background nil 'default)
        :weight (face-attribute face :weight nil 'default)
        :height (face-attribute face :height nil 'default)))

(defun moe404-test-declared-foreground (theme face)
  (let ((setting
         (seq-find (lambda (entry)
                     (and (eq (car-safe entry) 'theme-face)
                          (eq (cadr entry) face)))
                   (get theme 'theme-settings))))
    (and setting
         (plist-get (cadr (car (nth 3 setting))) :foreground))))

(defun moe404-test-window-state ()
  (mapcar (lambda (window)
            (list (buffer-name (window-buffer window))
                  (window-point window)
                  (window-start window)
                  (window-dedicated-p window)))
          (seq-mapcat (lambda (frame) (window-list frame 'nomini))
                      (frame-list))))

(defun moe404-test-run (body)
  (let* ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (root (and sandbox
                    (file-name-as-directory
                     (expand-file-name "moe-theme/" sandbox))))
         (window-before (current-window-configuration))
         (window-state-before (moe404-test-window-state))
         (selected-window-before (selected-window))
         (buffer-before (current-buffer))
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (copy-sequence timer-list))
         (frames-before (frame-list))
         (themes-before (copy-sequence custom-enabled-themes))
         (background-before (frame-parameter nil 'background-mode))
         (custom-enabled-themes (copy-sequence custom-enabled-themes))
         (custom-safe-themes t)
         (moe-theme-modeline-color 'blue)
         (moe-theme-resize-title-markdown nil)
         (moe-theme-resize-title-adoc nil)
         (moe-theme-resize-title-org nil)
         (moe-theme-resize-title-rst nil)
         (moe-theme--need-reload-theme t)
         (moe-light-pure-white-background-in-terminal nil)
         (moe-theme-switch-by-sunrise-and-sunset nil)
         (moe-theme-switcher-mode nil)
         (moe-theme-switcher--which-enabled nil)
         (moe-theme-switcher--24h/sunrise nil)
         (moe-theme-switcher--24h/sunset nil)
         (moe-theme-switcher--compute-sunrise-sunset-timer nil)
         (moe-theme-switcher--timer nil)
         (moe-dark-bg moe-dark-bg)
         (moe-dark-fg moe-dark-fg)
         (moe-dark-builtin moe-dark-builtin)
         (moe-dark-comment-delimiter moe-dark-comment-delimiter)
         (moe-dark-comment moe-dark-comment)
         (moe-dark-constant moe-dark-constant)
         (moe-dark-doc moe-dark-doc)
         (moe-dark-doc-string moe-dark-doc-string)
         (moe-dark-function-name moe-dark-function-name)
         (moe-dark-keyword moe-dark-keyword)
         (moe-dark-negation-char moe-dark-negation-char)
         (moe-dark-preprocessor moe-dark-preprocessor)
         (moe-dark-regexp-grouping-backslash moe-dark-regexp-grouping-backslash)
         (moe-dark-regexp-grouping-construct moe-dark-regexp-grouping-construct)
         (moe-dark-string moe-dark-string)
         (moe-dark-type moe-dark-type)
         (moe-dark-variable-name moe-dark-variable-name)
         (moe-dark-warning moe-dark-warning)
         (moe-light-bg moe-light-bg)
         (moe-light-fg moe-light-fg)
         (moe-light-builtin moe-light-builtin)
         (moe-light-comment-delimiter moe-light-comment-delimiter)
         (moe-light-comment moe-light-comment)
         (moe-light-constant moe-light-constant)
         (moe-light-doc moe-light-doc)
         (moe-light-doc-string moe-light-doc-string)
         (moe-light-function-name moe-light-function-name)
         (moe-light-keyword moe-light-keyword)
         (moe-light-negation-char moe-light-negation-char)
         (moe-light-preprocessor moe-light-preprocessor)
         (moe-light-regexp-grouping-backslash moe-light-regexp-grouping-backslash)
         (moe-light-regexp-grouping-construct moe-light-regexp-grouping-construct)
         (moe-light-string moe-light-string)
         (moe-light-type moe-light-type)
         (moe-light-variable-name moe-light-variable-name)
         (moe-light-warning moe-light-warning)
         root-owned result body-error cleanup-errors)
    (unwind-protect
        (condition-case condition
            (progn
              (unless (and root (file-name-absolute-p root))
                (error "Missing absolute Moe Theme sandbox root"))
              (when (file-exists-p root)
                (error "Moe Theme sandbox root already exists: %s" root))
              (setq root-owned t)
              (make-directory root t)
              (cl-letf (((symbol-function 'call-process)
                         (lambda (&rest arguments)
                           (error "Unexpected call-process: %S" arguments)))
                        ((symbol-function 'call-process-region)
                         (lambda (&rest arguments)
                           (error "Unexpected call-process-region: %S" arguments)))
                        ((symbol-function 'start-process)
                         (lambda (&rest arguments)
                           (error "Unexpected start-process: %S" arguments)))
                        ((symbol-function 'make-process)
                         (lambda (&rest arguments)
                           (error "Unexpected make-process: %S" arguments)))
                        ((symbol-function 'make-network-process)
                         (lambda (&rest arguments)
                           (error "Unexpected network process: %S" arguments)))
                        ((symbol-function 'url-retrieve)
                         (lambda (&rest arguments)
                           (error "Unexpected URL retrieval: %S" arguments))))
                (save-window-excursion
                  (save-current-buffer
                    (setq result (funcall body))))))
          (t (setq body-error (moe404-test-condition condition))))
      (condition-case condition
          (when moe-theme-switcher-mode
            (moe-theme-switcher-mode -1))
        (t (push (moe404-test-condition condition) cleanup-errors)))
      (dolist (theme '(moe-dark moe-light))
        (condition-case condition
            (when (and (custom-theme-enabled-p theme)
                       (not (memq theme themes-before)))
              (disable-theme theme))
          (t (push (moe404-test-condition condition) cleanup-errors))))
      (dolist (timer (seq-difference timer-list timers-before #'eq))
        (condition-case condition (cancel-timer timer)
          (t (push (moe404-test-condition condition) cleanup-errors))))
      (dolist (process (seq-difference (process-list) processes-before #'eq))
        (condition-case condition (delete-process process)
          (t (push (moe404-test-condition condition) cleanup-errors))))
      (dolist (buffer (seq-difference (buffer-list) buffers-before #'eq))
        (condition-case condition
            (when (buffer-live-p buffer)
              (with-current-buffer buffer
                (let ((kill-buffer-hook nil)
                      (kill-buffer-query-functions nil))
                  (set-buffer-modified-p nil)
                  (kill-buffer buffer))))
          (t (push (moe404-test-condition condition) cleanup-errors))))
      (dolist (frame (seq-difference (frame-list) frames-before #'eq))
        (condition-case condition (delete-frame frame t)
          (t (push (moe404-test-condition condition) cleanup-errors))))
      (condition-case condition
          (progn
            (set-frame-parameter nil 'background-mode background-before)
            (dolist (face '(default mode-line mode-line-buffer-id minibuffer-prompt
                            font-lock-keyword-face font-lock-string-face
                            org-document-title org-level-1 org-level-2 org-level-3
                            org-level-4 org-level-5 org-level-6 org-level-7 org-level-8))
              (when (facep face) (face-spec-recalc face nil))))
        (t (push (moe404-test-condition condition) cleanup-errors)))
      (condition-case condition
          (when (and root-owned root (file-exists-p root))
            (delete-directory root t))
        (t (push (moe404-test-condition condition) cleanup-errors)))
      (condition-case condition (set-window-configuration window-before)
        (t (push (moe404-test-condition condition) cleanup-errors)))
      (when (buffer-live-p buffer-before) (set-buffer buffer-before)))
    (let ((cleanup
           (list :new-buffers
                 (mapcar #'buffer-name
                         (seq-filter #'buffer-live-p
                                     (seq-difference (buffer-list) buffers-before #'eq)))
                 :new-processes (length (seq-difference
                                         (process-list) processes-before #'eq))
                 :new-timers (length (seq-difference timer-list timers-before #'eq))
                 :new-frames (length (seq-difference (frame-list) frames-before #'eq))
                 :themes-restored (equal custom-enabled-themes themes-before)
                 :switcher-disabled (not moe-theme-switcher-mode)
                 :background-restored
                 (equal (frame-parameter nil 'background-mode) background-before)
                 :root-exists (and root (file-exists-p root))
                 :window-restored
                 (and (equal (moe404-test-window-state) window-state-before)
                      (eq (selected-window) selected-window-before))
                 :buffer-restored (eq (current-buffer) buffer-before)
                 :body-error body-error
                 :cleanup-errors (nreverse cleanup-errors))))
      (if (or body-error cleanup-errors)
          (error "Moe Theme workflow failed: %S" (list result cleanup))
        (list :result result :cleanup cleanup)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(MOE_THEME_MELPA_PIN, "moe-theme.el")
        .expect("prepare exact shallow Moe Theme source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn public_light_theme_applies_palette_and_modeline_color() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_light_theme_applies_palette_and_modeline_color",
        r####"
(moe404-test-run
 (lambda ()
   (setq moe-theme-modeline-color 'cyan)
   (moe-light)
   (list :enabled (copy-sequence custom-enabled-themes)
         :theme (and (custom-theme-enabled-p 'moe-light) t)
         :registered (and (custom-theme-p 'moe-light) t)
         :background-mode (frame-parameter nil 'background-mode)
         :palette (list :background moe-light-bg
                        :foreground moe-light-fg
                        :keyword moe-light-keyword
                        :string moe-light-string)
         :declared (list :default
                         (moe404-test-declared-foreground 'moe-light 'default)
                         :keyword
                         (moe404-test-declared-foreground
                          'moe-light 'font-lock-keyword-face))
         :mode-line (moe404-test-face-state 'mode-line))))
"####,
        expect![[
            r##"OK (:result (:enabled (moe-light) :theme t :registered t :background-mode dark :palette (:background "#fdfdf6" :foreground "#5f5f5f" :keyword "#00af00" :string "#ff1f8b") :declared (:default "#5f5f5f" :keyword "#00af00") :mode-line (:foreground "#005f5f" :background "#87d7af" :weight normal :height 1)) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :themes-restored t :switcher-disabled t :background-restored t :root-exists nil :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"##
        ]],
    )
}

fn public_dark_theme_and_interactive_modeline_selection_apply_exact_color() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_dark_theme_and_interactive_modeline_selection_apply_exact_color",
        r####"
(moe404-test-run
 (lambda ()
   (let (completion-calls)
     (moe-dark)
     (cl-letf (((symbol-function 'completing-read)
                (lambda (prompt collection predicate require-match
                                 &optional initial history default inherit)
                  (push (list :prompt prompt
                              :collection (copy-tree collection)
                              :predicate predicate
                              :require-match require-match
                              :initial initial
                              :history history
                              :default default
                              :inherit inherit)
                        completion-calls)
                  "orange")))
       (call-interactively #'moe-theme-modeline-select-color))
     (list :enabled (copy-sequence custom-enabled-themes)
           :theme (and (custom-theme-enabled-p 'moe-dark) t)
           :background-mode (frame-parameter nil 'background-mode)
           :selected moe-theme-modeline-color
           :completion (nreverse completion-calls)
           :default (moe404-test-face-state 'default)
           :mode-line (moe404-test-face-state 'mode-line)
           :buffer-id (moe404-test-face-state 'mode-line-buffer-id)
           :prompt (moe404-test-face-state 'minibuffer-prompt)))))
"####,
        expect![[
            r##"OK (:result (:enabled (moe-dark) :theme t :background-mode dark :selected orange :completion ((:prompt "Select a color: " :collection ((blue) (green) (orange) (magenta) (yellow) (purple) (red) (cyan) (w/b)) :predicate nil :require-match t :initial "" :history nil :default nil :inherit t)) :default (:foreground "unspecified-fg" :background "unspecified-bg" :weight normal :height 1) :mode-line (:foreground "#b75f00" :background "#ffaf5f" :weight normal :height 1) :buffer-id (:foreground "#080808" :background "#ffaf5f" :weight bold :height 1) :prompt (:foreground "#080808" :background "#ffaf5f" :weight normal :height 1)) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :themes-restored t :switcher-disabled t :background-restored t :root-exists nil :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"##
        ]],
    )
}

fn public_flavour_commands_reload_tomorrow_monokai_and_defaults() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_flavour_commands_reload_tomorrow_monokai_and_defaults",
        r####"
(moe404-test-run
 (lambda ()
   (moe-dark)
   (moe-theme-flavour-tomorrow)
   (let ((tomorrow
          (list :background moe-dark-bg
                :foreground moe-dark-fg
                :keyword moe-dark-keyword
                :string moe-dark-string
                :declared-string
                (moe404-test-declared-foreground
                 'moe-dark 'font-lock-string-face))))
     (moe-theme-flavour-monokai)
     (let ((monokai
            (list :background moe-dark-bg
                  :foreground moe-dark-fg
                  :keyword moe-dark-keyword
                  :string moe-dark-string
                  :declared-string
                  (moe404-test-declared-foreground
                   'moe-dark 'font-lock-string-face))))
       (moe-theme-flavour-default)
       (list :tomorrow tomorrow
             :monokai monokai
             :default
             (list :background moe-dark-bg
                   :foreground moe-dark-fg
                   :keyword moe-dark-keyword
                   :string moe-dark-string
                   :declared-string
                   (moe404-test-declared-foreground
                    'moe-dark 'font-lock-string-face))
             :enabled (copy-sequence custom-enabled-themes))))))
"####,
        expect![[
            r##"OK (:result (:tomorrow (:background "#1d1f21" :foreground "#c5c8c6" :keyword "#b5bd68" :string "#8abeb7" :declared-string "#8abeb7") :monokai (:background "#272722" :foreground "#F8F8F2" :keyword "#F92672" :string "#E6DB74" :declared-string "#E6DB74") :default (:background "#303030" :foreground "#c6c6c6" :keyword "#a1db00" :string "#ff4ea3" :declared-string "#ff4ea3") :enabled (moe-dark)) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :themes-restored t :switcher-disabled t :background-restored t :root-exists nil :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"##
        ]],
    )
}

fn public_switcher_mode_owns_timer_day_night_transitions_and_disable() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_switcher_mode_owns_timer_day_night_transitions_and_disable",
        r####"
(moe404-test-run
 (lambda ()
   (let ((original-run-with-timer (symbol-function 'run-with-timer))
         (original-format-time-string (symbol-function 'format-time-string))
         plans timer active-after-enable midday night)
     (cl-letf (((symbol-function 'run-with-timer)
                (lambda (time repeat function &rest arguments)
                  (push (list time repeat function (copy-tree arguments)) plans)
                  (setq timer
                        (apply original-run-with-timer
                               86400 repeat function arguments))))
               ((symbol-function 'format-time-string)
                (lambda (&rest arguments)
                  (if (equal (car arguments) "%H")
                      "12"
                    (apply original-format-time-string arguments)))))
       (moe-theme-switcher-mode 1)
       (setq active-after-enable (and (memq timer timer-list) t))
       (apply (nth 2 (car plans)) (nth 3 (car plans)))
       (setq midday
             (list :which moe-theme-switcher--which-enabled
                   :light (and (custom-theme-enabled-p 'moe-light) t)
                   :background (frame-parameter nil 'background-mode)))
       (cl-letf (((symbol-function 'format-time-string)
                  (lambda (&rest arguments)
                    (if (equal (car arguments) "%H")
                        "23"
                      (apply original-format-time-string arguments)))))
         (apply (nth 2 (car plans)) (nth 3 (car plans))))
       (setq night
             (list :which moe-theme-switcher--which-enabled
                   :dark (and (custom-theme-enabled-p 'moe-dark) t)
                   :background (frame-parameter nil 'background-mode)))
       (moe-theme-switcher-mode -1))
     (list :plans (nreverse plans)
           :active-after-enable active-after-enable
           :midday midday
           :night night
           :mode moe-theme-switcher-mode
           :timer-active-after-disable (and (memq timer timer-list) t)
           :enabled (copy-sequence custom-enabled-themes)))))
"####,
        expect![
            "OK (:result (:plans ((0 60 moe-theme-switcher--auto-switch nil)) :active-after-enable t :midday (:which light :light t :background dark) :night (:which dark :dark t :background dark) :mode nil :timer-active-after-disable nil :enabled (moe-dark moe-light)) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :themes-restored t :switcher-disabled t :background-restored t :root-exists nil :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"
        ],
    )
}

#[test]
fn moe_theme_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        public_light_theme_applies_palette_and_modeline_color(),
        public_dark_theme_and_interactive_modeline_selection_apply_exact_color(),
        public_flavour_commands_reload_tomorrow_monokai_and_defaults(),
        public_switcher_mode_owns_timer_day_night_transitions_and_disable(),
    ];
    assert_oracle_batch_cases(oracle(), "moe-theme-rank404", "moe-theme", &cases);
}
