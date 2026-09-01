//! Practical parity for Autothemer's public theme-authoring surface.
//!
//! The cases define and enable an owned multi-display theme, reuse its
//! palette, drive interactive color insertion, exercise documented conversion
//! and sorting helpers, export JSON, and prove guarded failure recovery.

use std::time::Duration;

use expect_test::expect;

use crate::{AUTOTHEMER_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(240);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'autothemer)

(defconst autothemer393-test-source
  '("autothemer.el" "4eec0757311e8980cc4cd2e8a1e53cc841b7418e80c04fa4bbdfbce923defcb3"))

(defun autothemer393-test-file-sha256 (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(let ((file (symbol-file 'autothemer-deftheme 'defun)))
  (unless (and (file-regular-p file)
               (equal (file-name-nondirectory file)
                      (car autothemer393-test-source))
               (equal (autothemer393-test-file-sha256 file)
                      (cadr autothemer393-test-source)))
    (error "Unexpected installed Autothemer source: %S" file)))

(defvar autothemer393-body-state nil)

(defun autothemer393-test-condition (condition)
  (list :type (car condition)
        :data (cdr condition)
        :message (error-message-string condition)))

(defun autothemer393-test-theme-state ()
  (list :name (autothemer--theme-name autothemer-current-theme)
        :description (autothemer--theme-description autothemer-current-theme)
        :colors
        (mapcar (lambda (color)
                  (cons (autothemer--color-name color)
                        (autothemer--color-value color)))
                (autothemer--theme-colors autothemer-current-theme))
        :faces (autothemer--theme-defined-faces autothemer-current-theme)
        :reduced (autothemer--theme-reduced-specs autothemer-current-theme)))

(defun autothemer393-test-symbol-state (symbol)
  (list symbol
        (boundp symbol) (and (boundp symbol) (symbol-value symbol))
        (copy-tree (symbol-plist symbol))))

(defun autothemer393-test-restore-symbol (state)
  (let ((symbol (nth 0 state)))
    (if (nth 1 state) (set symbol (nth 2 state)) (makunbound symbol))
    (setplist symbol (copy-tree (nth 3 state)))))

(defun autothemer393-test-read-file (file)
  (with-temp-buffer
    (insert-file-contents-literally file)
    (buffer-string)))

(defun autothemer393-test-run (body)
  (let* ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (root (and sandbox
                    (file-name-as-directory
                     (expand-file-name "autothemer/" sandbox))))
         (window-before (current-window-configuration))
         (buffer-before (current-buffer))
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (copy-sequence timer-list))
         (frames-before (frame-list))
         (custom-known-themes (copy-sequence custom-known-themes))
         (custom-enabled-themes (copy-sequence custom-enabled-themes))
         (autothemer-current-theme autothemer-current-theme)
         (autothemer393-body-state nil)
         (symbol-states
          (mapcar #'autothemer393-test-symbol-state
                  '(autothemer393-theme)))
         result body-error cleanup-errors)
    (unless (and root (file-name-absolute-p root))
      (error "Missing absolute Autothemer sandbox root"))
    (when (file-exists-p root)
      (error "Autothemer sandbox root already exists: %s" root))
    (make-directory root t)
    (condition-case condition
        (cl-letf (((symbol-function 'call-process)
                   (lambda (&rest args)
                     (error "Unexpected call-process: %S" args)))
                  ((symbol-function 'call-process-region)
                   (lambda (&rest args)
                     (error "Unexpected call-process-region: %S" args)))
                  ((symbol-function 'start-process)
                   (lambda (&rest args)
                     (error "Unexpected start-process: %S" args)))
                  ((symbol-function 'make-process)
                   (lambda (&rest args)
                     (error "Unexpected make-process: %S" args)))
                  ((symbol-function 'make-network-process)
                   (lambda (&rest args)
                     (error "Unexpected network process: %S" args)))
                  ((symbol-function 'url-retrieve)
                   (lambda (&rest args)
                     (error "Unexpected URL retrieval: %S" args))))
          (save-window-excursion
            (save-current-buffer
              (setq result (funcall body root)))))
      (t (setq body-error (autothemer393-test-condition condition))))
    (dolist (buffer (seq-difference (buffer-list) buffers-before #'eq))
      (condition-case condition
          (when (buffer-live-p buffer)
            (with-current-buffer buffer
              (let ((kill-buffer-hook nil)
                    (kill-buffer-query-functions nil))
                (set-buffer-modified-p nil)
                (kill-buffer buffer))))
        (t (push (autothemer393-test-condition condition) cleanup-errors))))
    (condition-case condition
        (when (custom-theme-enabled-p 'autothemer393-theme)
          (disable-theme 'autothemer393-theme))
      (t (push (autothemer393-test-condition condition) cleanup-errors)))
    (dolist (state symbol-states)
      (condition-case condition (autothemer393-test-restore-symbol state)
        (t (push (autothemer393-test-condition condition) cleanup-errors))))
    (dolist (timer (seq-difference timer-list timers-before #'eq))
      (condition-case condition (cancel-timer timer)
        (t (push (autothemer393-test-condition condition) cleanup-errors))))
    (dolist (process (seq-difference (process-list) processes-before #'eq))
      (condition-case condition (delete-process process)
        (t (push (autothemer393-test-condition condition) cleanup-errors))))
    (dolist (frame (seq-difference (frame-list) frames-before #'eq))
      (condition-case condition (delete-frame frame t)
        (t (push (autothemer393-test-condition condition) cleanup-errors))))
    (condition-case condition
        (when (file-exists-p root) (delete-directory root t))
      (t (push (autothemer393-test-condition condition) cleanup-errors)))
    (condition-case condition (set-window-configuration window-before)
      (t (push (autothemer393-test-condition condition) cleanup-errors)))
    (when (buffer-live-p buffer-before) (set-buffer buffer-before))
    (let ((cleanup
           (list :new-buffers
                 (mapcar #'buffer-name
                         (seq-filter #'buffer-live-p
                                     (seq-difference (buffer-list) buffers-before #'eq)))
                 :new-processes (length (seq-difference
                                         (process-list) processes-before #'eq))
                 :new-timers (length (seq-difference timer-list timers-before #'eq))
                 :new-frames (length (seq-difference (frame-list) frames-before #'eq))
                 :root-exists (file-exists-p root)
                 :theme-disabled (not (custom-theme-enabled-p 'autothemer393-theme))
                 :buffer-restored (eq (current-buffer) buffer-before)
                 :body-error body-error
                 :cleanup-errors (nreverse cleanup-errors))))
      (if (or body-error cleanup-errors)
          (error "Autothemer workflow failed: %S" (list result cleanup))
        (list :result result :cleanup cleanup)))))

(defmacro autothemer393-test-define-theme ()
  '(autothemer-deftheme autothemer393-theme "Café theme 界"
     ((((class color) (min-colors #xFFFFFF))
       ((class color) (min-colors #xFF))
       t)
      (canvas "#101820" nil nil)
      (ink "#f2f2f2" "white" nil)
      (accent "#ff6600" "orange" "red"))
     ((default (:background canvas :foreground ink))
      (font-lock-keyword-face
       (:foreground accent :weight 'bold :underline t)))
     (setq autothemer393-body-state (list canvas ink accent))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AUTOTHEMER_MELPA_PIN, "autothemer.el")
        .expect("prepare exact shallow Autothemer source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn public_theme_definition_builds_palette_faces_and_body() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_theme_definition_builds_palette_faces_and_body",
        r####"
(autothemer393-test-run
 (lambda (_root)
   (autothemer393-test-define-theme)
   (enable-theme 'autothemer393-theme)
   (list :known (and (memq 'autothemer393-theme custom-known-themes) t)
         :enabled (and (custom-theme-enabled-p 'autothemer393-theme) t)
         :enabled-themes (copy-sequence custom-enabled-themes)
         :resolved-face
         (list :foreground
               (face-attribute 'font-lock-keyword-face :foreground nil 'default)
               :weight
               (face-attribute 'font-lock-keyword-face :weight nil 'default)
               :underline
               (face-attribute 'font-lock-keyword-face :underline nil 'default))
         :settings
         (copy-tree (get 'autothemer393-theme 'theme-settings))
         :body autothemer393-body-state
         :theme (autothemer393-test-theme-state)
         :palette-let
         (autothemer-let-palette (list canvas ink accent)))))
"####,
        expect![[
            r##"OK (:result (:known t :enabled t :enabled-themes (autothemer393-theme) :resolved-face (:foreground "red" :weight bold :underline t) :settings ((theme-face font-lock-keyword-face autothemer393-theme ((((class color) (min-colors 16777215)) (:foreground "#ff6600" :weight bold :underline t)) (((class color) (min-colors 255)) (:foreground "orange" :weight bold :underline t)) (t (:foreground "red" :weight bold :underline t)))) (theme-face default autothemer393-theme ((((class color) (min-colors 16777215)) (:background "#101820" :foreground "#f2f2f2")) (((class color) (min-colors 255)) (:background "#101820" :foreground "white")) (t (:background "#101820" :foreground "white"))))) :body ("#101820" "#f2f2f2" "#ff6600") :theme (:name "autothemer393-theme" :description "Café theme 界" :colors ((canvas . "#101820") (ink . "#f2f2f2") (accent . "#ff6600")) :faces (default font-lock-keyword-face) :reduced ((default (:background canvas :foreground ink)) (font-lock-keyword-face (:foreground accent :weight 'bold :underline t)))) :palette-let ("#101820" "#f2f2f2" "#ff6600")) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :theme-disabled t :buffer-restored t :body-error nil :cleanup-errors nil))"##
        ]],
    )
}

fn interactive_color_commands_select_and_insert_palette_values() -> ParityBatchCase {
    ParityBatchCase::value(
        "interactive_color_commands_select_and_insert_palette_values",
        r####"
(autothemer393-test-run
 (lambda (_root)
   (autothemer393-test-define-theme)
   (let (calls)
     (cl-letf (((symbol-function 'completing-read)
                (lambda (prompt collection &rest _)
                  (let* ((plain (mapcar #'substring-no-properties collection))
                         (selected (seq-find
                                    (lambda (candidate)
                                      (string-match-p "accent" candidate))
                                    plain)))
                    (push (list :prompt prompt :candidates plain :selected selected)
                          calls)
                    selected))))
       (with-temp-buffer
         (call-interactively #'autothemer-insert-color-name)
         (insert "=")
         (call-interactively #'autothemer-insert-color)
         (list :text (buffer-string) :calls (nreverse calls)))))))
"####,
        expect![[
            r#"OK (:result (:text "accent=#ff6600" :calls ((:prompt "Insert a color name: " :candidates ("                                         #101820  canvas                                       " "                                         #f2f2f2  ink                                          " "                                         #ff6600  accent                                       ") :selected "                                         #ff6600  accent                                       ") (:prompt "Insert a color: " :candidates ("                                         #101820  canvas                                       " "                                         #f2f2f2  ink                                          " "                                         #ff6600  accent                                       ") :selected "                                         #ff6600  accent                                       "))) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :theme-disabled t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn documented_conversion_and_sorting_helpers_transform_real_colors() -> ParityBatchCase {
    ParityBatchCase::value(
        "documented_conversion_and_sorting_helpers_transform_real_colors",
        r####"
(autothemer393-test-run
 (lambda (_root)
   (autothemer393-test-define-theme)
   (let* ((colors (copy-sequence (autothemer--theme-colors autothemer-current-theme)))
          (darkest (autothemer-sort-palette colors #'autothemer-darkest-order))
          (lightest (autothemer-sort-palette colors #'autothemer-lightest-order)))
     (list :hex-to-rgb (autothemer-hex-to-rgb "#80ff00")
           :hex-to-srgb (autothemer-hex-to-srgb "#80ff00")
           :rgb-to-hex (autothemer-rgb-to-hex '(32896 65535 0))
           :hsv (mapcar (lambda (color)
                          (list (autothemer-color-hue color)
                                (autothemer-color-sat color)
                                (autothemer-color-brightness color)))
                        '("#ff0000" "#808080"))
           :darkest (mapcar #'autothemer--color-name darkest)
           :lightest (mapcar #'autothemer--color-name lightest)))))
"####,
        expect![[
            r##"OK (:result (:hex-to-rgb (32896 65535 0) :hex-to-srgb (0.5019607843137255 1.0 0.0) :rgb-to-hex "#80FF00" :hsv ((0.0 1.0 1.0) (0.0 0.0 0.5019607843137255)) :darkest (canvas ink accent) :lightest (accent ink canvas)) :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :theme-disabled t :buffer-restored t :body-error nil :cleanup-errors nil))"##
        ]],
    )
}

fn palette_json_export_and_guarded_recovery_use_public_api() -> ParityBatchCase {
    ParityBatchCase::value(
        "palette_json_export_and_guarded_recovery_use_public_api",
        r####"
(autothemer393-test-run
 (lambda (root)
   (let ((autothemer-current-theme nil)
         (file (expand-file-name "palette.json" root))
         failure message)
     (setq failure
           (condition-case condition
               (autothemer-generate-palette-json)
             (t (autothemer393-test-condition condition))))
     (autothemer393-test-define-theme)
     (cl-letf (((symbol-function 'message)
                (lambda (format-string &rest arguments)
                  (setq message (apply #'format format-string arguments)))))
       (autothemer-write-palette-json file))
     (list :failure failure
           :message (replace-regexp-in-string (regexp-quote root) "[ROOT]/" message)
           :json (autothemer393-test-read-file file)
           :generated (autothemer-generate-palette-json)))))
"####,
        expect![[
            r##"OK (:result (:failure (:type user-error :data ("No current theme available. Evaluate an autotheme definition") :message "No current theme available. Evaluate an autotheme definition") :message "Palette JSON written to [ROOT]/palette.json" :json "[{\"name\":\"canvas\",\"color\":\"#101820\"},{\"name\":\"ink\",\"color\":\"#f2f2f2\"},{\"name\":\"accent\",\"color\":\"#ff6600\"}]" :generated "[{\"name\":\"canvas\",\"color\":\"#101820\"},{\"name\":\"ink\",\"color\":\"#f2f2f2\"},{\"name\":\"accent\",\"color\":\"#ff6600\"}]") :cleanup (:new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :theme-disabled t :buffer-restored t :body-error nil :cleanup-errors nil))"##
        ]],
    )
}

fn cases() -> Vec<ParityBatchCase> {
    vec![
        public_theme_definition_builds_palette_faces_and_body(),
        interactive_color_commands_select_and_insert_palette_values(),
        documented_conversion_and_sorting_helpers_transform_real_colors(),
        palette_json_export_and_guarded_recovery_use_public_api(),
    ]
}

#[test]
fn public_autothemer_workflows_match() {
    assert_oracle_batch_cases(oracle(), "autothemer-rank393", "Autothemer", &cases());
}
