use std::time::Duration;

use expect_test::expect;

use crate::{
    CachedMelpaOracle, DASH_MELPA_PIN, POWERLINE_MELPA_PIN, S_MELPA_PIN, SPACELINE_MELPA_PIN,
};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const SPACELINE_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const SPACELINE_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'rect)
(require 'spaceline-config)

(defun neomacs-spaceline-test-literal-format-mode-line (format &rest _)
  "Format literal strings at the display boundary unavailable in batch mode."
  (cond
   ((stringp format) format)
   ((null format) "")
   (t (error "Spaceline fixture expected a literal string, got %S" format))))

(defun neomacs-spaceline-test-property-runs (string property)
  "Return every non-nil PROPERTY run in STRING."
  (let ((position 0)
        runs)
    (while (< position (length string))
      (let* ((value (get-text-property position property string))
             (next (or (next-single-property-change position property string)
                       (length string))))
        (when value
          (push (list position next (copy-tree value)) runs))
        (setq position next)))
    (nreverse runs)))

(defun neomacs-spaceline-test-summary (format)
  "Return Spaceline's exact mode-line FORMAT and layout properties."
  (let ((string format))
    (list :text (substring-no-properties string)
          :width (string-width string)
          :display
          (neomacs-spaceline-test-property-runs string 'display))))

(defun neomacs-spaceline-test-segment-summary (format text)
  "Return TEXT's range, styling, help, and mouse actions in FORMAT."
  (let* ((string format)
         (beginning (string-match (regexp-quote text) string))
         (end (and beginning (+ beginning (length text))))
         (map (and beginning (get-text-property beginning 'local-map string))))
    (and beginning
         (list :range (list beginning end)
               :text (substring-no-properties string beginning end)
               :face (copy-tree (get-text-property beginning 'face string))
               :mouse-face (get-text-property beginning 'mouse-face string)
               :help (get-text-property beginning 'help-echo string)
               :mouse-1 (and map (lookup-key map [mode-line mouse-1]))
               :down-mouse-1
               (and map (lookup-key map [mode-line down-mouse-1]))
               :mouse-2 (and map (lookup-key map [mode-line mouse-2]))
               :mouse-3 (and map (lookup-key map [mode-line mouse-3]))
               :header-mouse-3
               (and map (lookup-key map [header-line down-mouse-3]))))))
"####;

fn spaceline_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(SPACELINE_MELPA_PIN, "spaceline.el")
        .expect("prepare revision-pinned Spaceline source below ./tmp")
        .with_melpa_dependency(DASH_MELPA_PIN)
        .expect("prepare revision-pinned Dash dependency below ./tmp")
        .with_melpa_dependency(POWERLINE_MELPA_PIN)
        .expect("prepare revision-pinned Powerline dependency below ./tmp")
        .with_melpa_dependency(S_MELPA_PIN)
        .expect("prepare revision-pinned s dependency below ./tmp")
        .with_prelude(SPACELINE_TEST_PRELUDE)
        .with_timeout(SPACELINE_TEST_TIMEOUT)
}

fn emacs_theme_installs_a_file_buffer_format_and_tracks_highlight_state() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((buffer (generate-new-buffer "release-controller.el"))
       (window (selected-window))
       (old-window-buffer (window-buffer window))
       (old-default-format (copy-tree (default-value 'mode-line-format)))
       (spaceline--mode-lines nil)
       (spaceline-byte-compile nil)
       (spaceline-responsive nil)
       (spaceline-minor-modes-p nil)
       (global-mode-string nil)
       (powerline-default-separator 'utf-8)
       (powerline-utf-8-separator-left ?>)
       (powerline-utf-8-separator-right ?<))
  (unwind-protect
      (progn
        (set-window-buffer window buffer)
        (with-current-buffer buffer
          (insert "(defun deploy-release (service)\n  (message \"deploy %s ✓\" service))\n")
          (emacs-lisp-mode)
          (setq-local buffer-file-name "/workspace/release/services/controller.el")
          (setq-local buffer-file-coding-system 'utf-8-unix)
          (set-buffer-modified-p nil)
          (spaceline-emacs-theme "DEPLOY")
          (let ((modeline (spaceline-ml-main))
                (saved-face (spaceline-highlight-face-modified)))
            (goto-char (point-max))
            (insert ";; pending production approval\n")
            (let ((modified-face (spaceline-highlight-face-modified)))
              (setq buffer-read-only t)
              (let ((read-only-face (spaceline-highlight-face-modified)))
                (list
                 :installed (copy-tree (default-value 'mode-line-format))
                 :modeline (neomacs-spaceline-test-summary modeline)
                 :buffer-id
                 (neomacs-spaceline-test-segment-summary modeline "%I")
                 :highlight-faces
                 (list saved-face modified-face read-only-face)))))))
    (set-default 'mode-line-format old-default-format)
    (when (window-live-p window) (set-window-buffer window old-window-buffer))
    (when (buffer-live-p buffer)
      (with-current-buffer buffer (setq buffer-read-only nil))
      (kill-buffer buffer))))
"####;
    let expected = expect![[
        r#"OK (:installed ("%e" (:eval (spaceline-ml-main))) :modeline (:text " %* %I > < unix | %l:%2c < DEPLOY < %p " :width 39 :display ((8 9 ((space :align-to (- (+ right right-fringe right-margin) 0)))))) :buffer-id (:range (4 6) :text "%I" :face (spaceline-highlight-face) :mouse-face mode-line-highlight :help nil :mouse-1 #[nil ((setq powerline-buffer-size-suffix (not powerline-buffer-size-suffix)) (force-mode-line-update)) nil nil nil nil] :down-mouse-1 nil :mouse-2 nil :mouse-3 nil :header-mouse-3 1) :highlight-faces (spaceline-unmodified spaceline-modified spaceline-read-only))"#
    ]];
    ParityBatchCase::value(
        "emacs_theme_installs_a_file_buffer_format_and_tracks_highlight_state",
        elisp_form,
        expected,
    )
}

fn custom_segments_handle_fallback_global_override_toggle_and_recompile() -> ParityBatchCase {
    let elisp_form = r####"
(progn
  (defvar neomacs-spaceline-environment nil)
  (defvar neomacs-spaceline-failed-jobs nil)
  (defvar neomacs-spaceline-build-lighter nil)
  (defvar neomacs-spaceline-external-lighter nil)
  (spaceline-define-segment neomacs-deployment
    "Deployment environment and failed-job count."
    (when neomacs-spaceline-environment
      (format "%s:%d"
              neomacs-spaceline-environment
              neomacs-spaceline-failed-jobs)))
  (spaceline-define-segment neomacs-build
    "Build status supplied by the release service."
    neomacs-spaceline-build-lighter
    :global-override neomacs-spaceline-build-lighter)
  (let ((neomacs-spaceline-environment "staging✓")
        (neomacs-spaceline-failed-jobs 2)
        (neomacs-spaceline-build-lighter " build:green ")
        (neomacs-spaceline-external-lighter " sync:queued ")
        (global-mode-string
         '("" neomacs-spaceline-build-lighter
              neomacs-spaceline-external-lighter))
        (spaceline--mode-lines nil)
        (spaceline-byte-compile nil)
        (spaceline-responsive nil)
        (powerline-default-separator 'utf-8)
        (powerline-utf-8-separator-left ?>)
        (powerline-utf-8-separator-right ?<))
    (with-temp-buffer
      (rename-buffer "release-dashboard" t)
      (insert "alpha\nbeta\ngamma\n")
      (set-buffer-modified-p nil)
      (spaceline-compile
       'neomacs-release
       '((neomacs-deployment :face highlight-face)
         ((buffer-modified buffer-id) :separator "/")
         (selection-info :fallback "no-selection"))
       '(neomacs-build global))
      (let ((initial (spaceline-ml-neomacs-release)))
        (spaceline-toggle-neomacs-build-off)
        (let ((disabled (spaceline-ml-neomacs-release)))
          (spaceline-toggle-neomacs-build-on)
          (spaceline-define-segment neomacs-deployment
            "Expanded deployment status."
            (format "release=%s failures=%d"
                    neomacs-spaceline-environment
                    neomacs-spaceline-failed-jobs))
          (let ((before-recompile (spaceline-ml-neomacs-release)))
            (spaceline-compile 'neomacs-release)
            (let ((after-recompile (spaceline-ml-neomacs-release)))
              (list
               :initial (neomacs-spaceline-test-summary initial)
               :initial-deployment
               (neomacs-spaceline-test-segment-summary initial "staging✓:2")
               :disabled (neomacs-spaceline-test-summary disabled)
               :before-recompile
               (neomacs-spaceline-test-summary before-recompile)
               :after-recompile
               (neomacs-spaceline-test-summary after-recompile)
               :build-enabled spaceline-neomacs-build-p
               :compiled-definition
               (copy-tree (assq 'neomacs-release spaceline--mode-lines))))))))))
"####;
    let expected = expect![[
        r#"OK (:initial (:text " staging✓:2 > %* > no-selection > <  build:green  " :width 50 :display ((33 34 ((space :align-to (- (+ right right-fringe right-margin) 0)))))) :initial-deployment (:range (1 11) :text "staging✓:2" :face (spaceline-highlight-face) :mouse-face nil :help nil :mouse-1 nil :down-mouse-1 nil :mouse-2 nil :mouse-3 nil :header-mouse-3 nil) :disabled (:text " staging✓:2 > %* > no-selection > " :width 34 :display ((33 34 ((space :align-to (- (+ right right-fringe right-margin) 0)))))) :before-recompile (:text " staging✓:2 > %* > no-selection > <  build:green  " :width 50 :display ((33 34 ((space :align-to (- (+ right right-fringe right-margin) 0)))))) :after-recompile (:text " release=staging✓ failures=2 > %* > no-selection > <  build:green  " :width 67 :display ((50 51 ((space :align-to (- (+ right right-fringe right-margin) 0)))))) :build-enabled t :compiled-definition (neomacs-release ((neomacs-deployment :face highlight-face) ((buffer-modified buffer-id) :separator "/") (selection-info :fallback "no-selection")) neomacs-build global))"#
    ]];
    ParityBatchCase::value(
        "custom_segments_handle_fallback_global_override_toggle_and_recompile",
        elisp_form,
        expected,
    )
}

fn selection_segment_reports_unicode_line_and_rectangle_workflows() -> ParityBatchCase {
    let elisp_form = r####"
(let ((spaceline--mode-lines nil)
      (spaceline-byte-compile nil)
      (spaceline-responsive nil)
      (powerline-default-separator 'utf-8)
      (powerline-utf-8-separator-left ?>)
      (powerline-utf-8-separator-right ?<))
  (with-temp-buffer
    (rename-buffer "release-notes.md" t)
    (insert "alpha ✓ beta\nrelease 東京 delta\nrollback omega\n")
    (spaceline-compile
     'neomacs-selection
     '(buffer-id)
     '((selection-info :fallback "no selection") line-column))
    (cl-labels
        ((capture (name beginning end rectangle)
           (goto-char end)
           (set-mark beginning)
           (setq mark-active t)
           (let ((rectangle-mark-mode rectangle))
             (list name
                   :region (buffer-substring-no-properties beginning end)
                   :modeline
                   (neomacs-spaceline-test-summary
                    (spaceline-ml-neomacs-selection))))))
      (let* ((without-selection
              (progn
                (deactivate-mark)
                (neomacs-spaceline-test-summary
                 (spaceline-ml-neomacs-selection))))
             (alpha (progn (goto-char (point-min)) (search-forward "alpha")
                           (- (point) 5)))
             (beta-end (progn (goto-char (point-min)) (search-forward "beta")
                              (point)))
             (delta-end (progn (goto-char (point-min)) (search-forward "delta")
                               (point))))
        (list
         :without-selection without-selection
         :single-line (capture 'single-line alpha beta-end nil)
         :multi-line (capture 'multi-line alpha delta-end nil)
         :rectangle (capture 'rectangle alpha delta-end t))))))
"####;
    let expected = expect![[
        r#"OK (:without-selection (:text " < no selection < %l:%2c " :width 25 :display ((0 1 ((space :align-to (- (+ right right-fringe right-margin) 0)))))) :single-line (single-line :region "alpha ✓ beta" :modeline (:text " < 12 chars < %l:%2c " :width 21 :display ((0 1 ((space :align-to (- (+ right right-fringe right-margin) 0))))))) :multi-line (multi-line :region "alpha ✓ beta\nrelease 東京 delta" :modeline (:text " < 2 lines < %l:%2c " :width 20 :display ((0 1 ((space :align-to (- (+ right right-fringe right-margin) 0))))))) :rectangle (rectangle :region "alpha ✓ beta\nrelease 東京 delta" :modeline (:text " < 2×18 block < %l:%2c " :width 23 :display ((0 1 ((space :align-to (- (+ right right-fringe right-margin) 0))))))))"#
    ]];
    ParityBatchCase::value(
        "selection_segment_reports_unicode_line_and_rectangle_workflows",
        elisp_form,
        expected,
    )
}

fn responsive_priorities_hide_and_restore_operational_status_on_resize() -> ParityBatchCase {
    let elisp_form = r####"
(progn
  (spaceline-define-segment neomacs-service
    "Current production service."
    "checkout-api")
  (spaceline-define-segment neomacs-region
    "Current production region."
    "東京-east-1")
  (spaceline-define-segment neomacs-incident
    "Current incident identifier."
    "INC-2048")
  (spaceline-define-segment neomacs-owner
    "Current release owner."
    "release-engineering")
  (let ((spaceline--mode-lines nil)
        (spaceline-byte-compile nil)
        (spaceline-responsive t)
        (powerline-default-separator 'utf-8)
        (powerline-utf-8-separator-left ?>)
        (powerline-utf-8-separator-right ?<)
        width)
    (with-temp-buffer
      (spaceline-compile
       'neomacs-responsive
       '((neomacs-service :priority 100)
         (neomacs-region :priority 60)
         (neomacs-incident :priority 90))
       '((neomacs-owner :priority 20)))
      (cl-letf (((symbol-function 'window-width) (lambda (&optional _) width))
                ((symbol-function 'window-margins) (lambda (&optional _) '(0 . 0)))
                ((symbol-function 'format-mode-line)
                 #'neomacs-spaceline-test-literal-format-mode-line))
        (mapcar
         (lambda (requested-width)
           (setq width requested-width)
           (let ((rendered (spaceline-ml-neomacs-responsive)))
             (list
              :window requested-width
              :rendered (neomacs-spaceline-test-summary rendered)
              :visibility
              (mapcar
               (lambda (entry)
                 (list (aref entry 0) (aref entry 1) (aref entry 2)))
               spaceline--runtime-data-neomacs-responsive))))
         '(96 30 52 96))))))
"####;
    let expected = expect![[
        r#"OK ((:window 96 :rendered (:text " checkout-api > 東京-east-1 > INC-2048 > < release-engineering " :width 63 :display ((38 39 ((space :align-to (- (+ right right-fringe right-margin) 22)))))) :visibility ((20 21 t) (60 14 t) (90 11 t) (100 14 t))) (:window 30 :rendered (:text " checkout-api > INC-2048 > " :width 27 :display ((26 27 ((space :align-to (- (+ right right-fringe right-margin) 0)))))) :visibility ((20 21 nil) (60 14 nil) (90 11 t) (100 14 t))) (:window 52 :rendered (:text " checkout-api > 東京-east-1 > INC-2048 > " :width 41 :display ((38 39 ((space :align-to (- (+ right right-fringe right-margin) 0)))))) :visibility ((20 21 nil) (60 14 t) (90 11 t) (100 14 t))) (:window 96 :rendered (:text " checkout-api > 東京-east-1 > INC-2048 > < release-engineering " :width 63 :display ((38 39 ((space :align-to (- (+ right right-fringe right-margin) 22)))))) :visibility ((20 21 t) (60 14 t) (90 11 t) (100 14 t))))"#
    ]];
    ParityBatchCase::value(
        "responsive_priorities_hide_and_restore_operational_status_on_resize",
        elisp_form,
        expected,
    )
}

fn live_minor_modes_render_actions_and_pre_hook_updates() -> ParityBatchCase {
    let elisp_form = r####"
(progn
  (define-minor-mode neomacs-spaceline-review-mode
    "Review release changes."
    :lighter " Review✓")
  (define-minor-mode neomacs-spaceline-sync-mode
    "Synchronize release metadata."
    :lighter " Sync:2")
  (let ((spaceline--mode-lines nil)
        (spaceline-byte-compile nil)
        (spaceline-responsive nil)
        (spaceline-minor-modes-p t)
        (spaceline-minor-modes-separator " • ")
        (powerline-default-separator 'utf-8)
        (powerline-utf-8-separator-left ?>)
        (powerline-utf-8-separator-right ?<)
        (hook-count 0)
        (spaceline-pre-hook nil))
    (with-temp-buffer
      (neomacs-spaceline-review-mode 1)
      (neomacs-spaceline-sync-mode 1)
      (add-hook 'spaceline-pre-hook (lambda () (setq hook-count (1+ hook-count))))
      (cl-letf (((symbol-function 'format-mode-line)
                 #'neomacs-spaceline-test-literal-format-mode-line))
        (spaceline-compile 'neomacs-modes '(minor-modes) nil)
        (let* ((both (spaceline-ml-neomacs-modes))
               (review
                (neomacs-spaceline-test-segment-summary both "Review✓"))
               (sync (neomacs-spaceline-test-segment-summary both "Sync:2")))
          (neomacs-spaceline-sync-mode -1)
          (let ((review-only (spaceline-ml-neomacs-modes)))
            (neomacs-spaceline-review-mode -1)
            (let ((none (spaceline-ml-neomacs-modes)))
              (list
               :both (neomacs-spaceline-test-summary both)
               :review review
               :sync sync
               :review-only (neomacs-spaceline-test-summary review-only)
               :none (neomacs-spaceline-test-summary none)
               :hook-count hook-count
               :modes (list neomacs-spaceline-review-mode
                            neomacs-spaceline-sync-mode)))))))))
"####;
    let expected = expect![[
        r#"OK (:both (:text " Sync:2 • Review✓ > " :width 20 :display ((19 20 ((space :align-to (- (+ right right-fringe right-margin) 0)))))) :review (:range (10 17) :text "Review✓" :face (powerline-active1) :mouse-face mode-line-highlight :help "neomacs-spaceline-review-mode\nmouse-1: Display minor mode menu\nmouse-2: Show help for minor mode\nmouse-3: Toggle minor mode" :mouse-1 (lambda #1=(event) #2=(interactive "@e") (minor-mode-menu-from-indicator "Review✓")) :down-mouse-1 nil :mouse-2 (lambda #3=(event) #4=(interactive "@e") (describe-minor-mode-from-indicator "Review✓")) :mouse-3 (lambda #1# #2# (minor-mode-menu-from-indicator "Review✓")) :header-mouse-3 (lambda #1# #2# (minor-mode-menu-from-indicator "Review✓"))) :sync (:range (1 7) :text "Sync:2" :face (powerline-active1) :mouse-face mode-line-highlight :help "neomacs-spaceline-sync-mode\nmouse-1: Display minor mode menu\nmouse-2: Show help for minor mode\nmouse-3: Toggle minor mode" :mouse-1 (lambda #1# #2# (minor-mode-menu-from-indicator "Sync:2")) :down-mouse-1 nil :mouse-2 (lambda #3# #4# (describe-minor-mode-from-indicator "Sync:2")) :mouse-3 (lambda #1# #2# (minor-mode-menu-from-indicator "Sync:2")) :header-mouse-3 (lambda #1# #2# (minor-mode-menu-from-indicator "Sync:2"))) :review-only (:text " Review✓ > " :width 11 :display ((10 11 ((space :align-to (- (+ right right-fringe right-margin) 0)))))) :none (:text " " :width 1 :display ((0 1 ((space :align-to (- (+ right right-fringe right-margin) 0)))))) :hook-count 3 :modes (nil nil))"#
    ]];
    ParityBatchCase::value(
        "live_minor_modes_render_actions_and_pre_hook_updates",
        elisp_form,
        expected,
    )
}

#[test]
fn spaceline_package_batch() {
    let cases = vec![
        emacs_theme_installs_a_file_buffer_format_and_tracks_highlight_state(),
        custom_segments_handle_fallback_global_override_toggle_and_recompile(),
        selection_segment_reports_unicode_line_and_rectangle_workflows(),
        responsive_priorities_hide_and_restore_operational_status_on_resize(),
        live_minor_modes_render_actions_and_pre_hook_updates(),
    ];
    assert_oracle_batch_cases(
        spaceline_oracle(),
        "spaceline-package-batch",
        "Spaceline",
        &cases,
    );
}
