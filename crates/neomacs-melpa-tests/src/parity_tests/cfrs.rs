use std::time::Duration;

use expect_test::expect;

use crate::{CFRS_MELPA_PIN, CachedMelpaOracle, DASH_MELPA_PIN, POSFRAME_MELPA_PIN, S_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'cfrs)

(defun neomacs-cfrs-test-buffer-state ()
  "Capture the user-visible input buffer and its prompt lifecycle."
  (let* ((overlays (overlays-in (point-min) (point-max)))
         (prompt (car overlays))
         (before-string (and prompt (overlay-get prompt 'before-string))))
    (list :text (buffer-substring-no-properties (point-min) (point-max))
          :point (point)
          :mode major-mode
          :prompt (and before-string (substring-no-properties before-string))
          :prompt-face (and before-string (get-text-property 0 'face before-string))
          :overlay-count (length overlays)
          :read-only (and prompt (overlay-get prompt 'read-only))
          :rear-nonsticky (and prompt (overlay-get prompt 'rear-nonsticky))
          :height-hook (not (null (memq #'cfrs--adjust-height post-command-hook)))
          :focus-hook
          (not (null (memq #'cfrs--detect-lost-focus
                           window-selection-change-functions)))
          :line-numbers (and (boundp 'display-line-numbers)
                             display-line-numbers))))

(defun neomacs-cfrs-test-child-read (prompt initial-input keys)
  "Run one graphical CFRS workflow with only the frame boundary controlled."
  (let ((buffer-name cfrs--buffer-name)
        (old-window-buffer (window-buffer (selected-window)))
        (show nil)
        (hide-count 0)
        (focus-targets nil)
        (heights nil)
        (exit-kind nil)
        (unread-command-events nil))
    (when-let ((old-buffer (get-buffer buffer-name)))
      (kill-buffer old-buffer))
    (unwind-protect
        (with-temp-buffer
          (cl-letf (((symbol-function 'display-graphic-p)
                     (lambda (&optional _display) t))
                    ((symbol-function 'posframe-show)
                     (lambda (buffer &rest args)
                       (with-current-buffer buffer
                         (erase-buffer)
                         (insert (or (plist-get args :string) "")))
                       (set-window-buffer (selected-window) buffer)
                       (setq show
                             (list
                              :buffer (buffer-name buffer)
                              :min-height (plist-get args :min-height)
                              :min-width (plist-get args :min-width)
                              :border-width
                              (plist-get args :internal-border-width)
                              :accept-focus (plist-get args :accept-focus)
                              :string (plist-get args :string)
                              :cursor
                              (cdr (assq
                                    'cursor-type
                                    (plist-get args :override-parameters)))
                              :custom-frame-parameter
                              (cdr (assq
                                    'neomacs-cfrs-test
                                    (plist-get args :override-parameters)))))
                       (selected-frame)))
                    ((symbol-function 'posframe-hide)
                     (lambda (buffer &rest _)
                       (when (equal buffer buffer-name)
                         (setq hide-count (1+ hide-count)))))
                    ((symbol-function 'x-focus-frame)
                     (lambda (frame &rest _)
                       (push (if frame :frame :parent) focus-targets)))
                    ((symbol-function 'set-frame-height)
                     (lambda (_frame height &optional _pretend)
                       (unless (equal height (car heights))
                         (push height heights))))
                    ((symbol-function 'recursive-edit)
                     (lambda ()
                       (let ((result
                              (catch 'neomacs-cfrs-test-exit
                                (execute-kbd-macro keys)
                                :fell-through)))
                         (when (eq result :cancel)
                           (signal 'quit nil)))))
                    ((symbol-function 'exit-recursive-edit)
                     (lambda ()
                       (setq exit-kind :finish)
                       (throw 'neomacs-cfrs-test-exit :finish)))
                    ((symbol-function 'abort-recursive-edit)
                     (lambda ()
                       (setq exit-kind :cancel)
                       (throw 'neomacs-cfrs-test-exit :cancel))))
            (let ((outcome
                   (condition-case nil
                       (list :value (cfrs-read prompt initial-input))
                     (quit (list :signal 'quit)))))
              (list
               :outcome outcome
               :exit exit-kind
               :show show
               :hide-count hide-count
               :focus-targets (nreverse focus-targets)
               :heights (nreverse heights)
               :buffer-state
               (when-let ((buffer (get-buffer buffer-name)))
                 (with-current-buffer buffer
                   (neomacs-cfrs-test-buffer-state)))))))
      (when (buffer-live-p old-window-buffer)
        (set-window-buffer (selected-window) old-window-buffer))
      (when-let ((buffer (get-buffer buffer-name)))
        (kill-buffer buffer)))))
"####;

fn terminal_session_delegates_prompt_and_initial_input_to_read_string() -> ParityBatchCase {
    let elisp_form = r####"
(let (read-call)
  (cl-letf (((symbol-function 'display-graphic-p)
             (lambda (&optional _display) nil))
            ((symbol-function 'read-string)
             (lambda (&rest args)
               (setq read-call args)
               "production/service-api")))
    (list :result (cfrs-read "Deployment target: " "staging/service-api")
          :read-call read-call
          :posframe-created (not (null (get-buffer cfrs--buffer-name))))))
"####;
    let expected = expect![[
        r#"OK (:result "production/service-api" :read-call ("Deployment target: " "staging/service-api") :posframe-created nil)"#
    ]];
    ParityBatchCase::value(
        "terminal_session_delegates_prompt_and_initial_input_to_read_string",
        elisp_form,
        expected,
    )
}

fn editing_replaces_the_draft_accepts_with_return_and_trims_edges() -> ParityBatchCase {
    let elisp_form = r####"
(let ((cfrs-frame-parameters '((neomacs-cfrs-test . release-dialog))))
  (neomacs-cfrs-test-child-read
   "Deployment target: "
   "staging/service-api"
   (vconcat (kbd "C-a C-k") "  production/service-api  " (kbd "<return>"))))
"####;
    let expected = expect![[
        r#"OK (:outcome (:value "production/service-api") :exit :finish :show (:buffer " *Pos-Frame-Read*" :min-height 1 :min-width 42 :border-width 2 :accept-focus t :string "" :cursor hbar :custom-frame-parameter release-dialog) :hide-count 1 :focus-targets (:frame :parent) :heights (1 0 1) :buffer-state (:text "  production/service-api  " :point 27 :mode cfrs-input-mode :prompt " Deployment target: " :prompt-face minibuffer-prompt :overlay-count 1 :read-only t :rear-nonsticky t :height-hook t :focus-hook nil :line-numbers nil))"#
    ]];
    ParityBatchCase::value(
        "editing_replaces_the_draft_accepts_with_return_and_trims_edges",
        elisp_form,
        expected,
    )
}

fn control_c_acceptance_preserves_multiline_input_and_resizes_the_frame() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-cfrs-test-child-read
 "Release notes: "
 nil
 (vconcat "Fixed parser" (kbd "C-q C-j") "Added cache" (kbd "C-c C-c")))
"####;
    let expected = expect![[
        r#"OK (:outcome (:value "Fixed parser\nAdded cache") :exit :finish :show (:buffer " *Pos-Frame-Read*" :min-height 1 :min-width 42 :border-width 2 :accept-focus t :string "" :cursor hbar :custom-frame-parameter nil) :hide-count 1 :focus-targets (:frame :parent) :heights (0 1 2) :buffer-state (:text "Fixed parser\nAdded cache" :point 25 :mode cfrs-input-mode :prompt " Release notes: " :prompt-face minibuffer-prompt :overlay-count 1 :read-only t :rear-nonsticky t :height-hook t :focus-hook nil :line-numbers nil))"#
    ]];
    ParityBatchCase::value(
        "control_c_acceptance_preserves_multiline_input_and_resizes_the_frame",
        elisp_form,
        expected,
    )
}

fn keyboard_quit_cancels_the_read_and_hides_exactly_once() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-cfrs-test-child-read
 "Secret: "
 "do-not-submit"
 (kbd "C-g"))
"####;
    let expected = expect![[
        r#"OK (:outcome (:signal quit) :exit :cancel :show (:buffer " *Pos-Frame-Read*" :min-height 1 :min-width 42 :border-width 2 :accept-focus t :string "" :cursor hbar :custom-frame-parameter nil) :hide-count 1 :focus-targets (:frame :parent) :heights (1) :buffer-state (:text "do-not-submit" :point 14 :mode cfrs-input-mode :prompt " Secret: " :prompt-face minibuffer-prompt :overlay-count 1 :read-only t :rear-nonsticky t :height-hook t :focus-hook nil :line-numbers nil))"#
    ]];
    ParityBatchCase::value(
        "keyboard_quit_cancels_the_read_and_hides_exactly_once",
        elisp_form,
        expected,
    )
}

fn prompt_width_clamps_to_the_configured_minimum_and_maximum() -> ParityBatchCase {
    let elisp_form = r####"
(let ((cfrs-min-width 12)
      (cfrs-max-width 24))
  (list
   :short
   (neomacs-cfrs-test-child-read
    "Tag: " "v1" (kbd "C-c C-c"))
   :long
   (neomacs-cfrs-test-child-read
    "Remote deployment environment: "
    "production-eu-west-1"
    (kbd "C-c C-c"))))
"####;
    let expected = expect![[
        r#"OK (:short (:outcome (:value "v1") :exit :finish :show (:buffer " *Pos-Frame-Read*" :min-height 1 :min-width 14 :border-width 2 :accept-focus t :string "" :cursor hbar :custom-frame-parameter nil) :hide-count 1 :focus-targets (:frame :parent) :heights (1) :buffer-state (:text "v1" :point 3 :mode cfrs-input-mode :prompt " Tag: " :prompt-face minibuffer-prompt :overlay-count 1 :read-only t :rear-nonsticky t :height-hook t :focus-hook nil :line-numbers nil)) :long (:outcome (:value "production-eu-west-1") :exit :finish :show (:buffer " *Pos-Frame-Read*" :min-height 1 :min-width 26 :border-width 2 :accept-focus t :string "" :cursor hbar :custom-frame-parameter nil) :hide-count 1 :focus-targets (:frame :parent) :heights (1) :buffer-state (:text "production-eu-west-1" :point 21 :mode cfrs-input-mode :prompt " Remote deployment environment: " :prompt-face minibuffer-prompt :overlay-count 1 :read-only t :rear-nonsticky t :height-hook t :focus-hook nil :line-numbers nil)))"#
    ]];
    ParityBatchCase::value(
        "prompt_width_clamps_to_the_configured_minimum_and_maximum",
        elisp_form,
        expected,
    )
}

fn cursor_selection_and_focus_loss_follow_the_frame_lifecycle() -> ParityBatchCase {
    let elisp_form = r####"
(let (hidden aborted focused)
  (cl-letf (((symbol-function 'frame-parameter)
             (lambda (_frame parameter)
               (and (eq parameter 'cursor-type) 'bar)))
            ((symbol-function 'posframe-hide)
             (lambda (buffer &rest _)
               (setq hidden buffer)))
            ((symbol-function 'abort-recursive-edit)
             (lambda ()
               (setq aborted t)
               (throw 'neomacs-cfrs-focus-lost :aborted)))
            ((symbol-function 'frame-parent)
             (lambda (_frame) :parent-frame))
            ((symbol-function 'x-focus-frame)
             (lambda (frame &rest _)
               (setq focused frame))))
    (let ((cursor-type t)
          active inactive)
      (with-temp-buffer
        (cfrs-input-mode)
        (setq active (cfrs--detect-lost-focus nil)))
      (with-temp-buffer
        (fundamental-mode)
        (setq inactive
              (catch 'neomacs-cfrs-focus-lost
                (cfrs--detect-lost-focus nil))))
      (cfrs--on-frame-kill (selected-frame))
      (list :cursor (cfrs--determine-cursor-type)
            :zero-cursor
            (let ((cursor-type '(bar . 0)))
              (cfrs--determine-cursor-type))
            :active active
            :inactive inactive
            :hidden hidden
            :aborted aborted
            :focused focused))))
"####;
    let expected = expect![[
        r#"OK (:cursor bar :zero-cursor (bar . 0) :active nil :inactive :aborted :hidden " *Pos-Frame-Read*" :aborted t :focused :parent-frame)"#
    ]];
    ParityBatchCase::value(
        "cursor_selection_and_focus_loss_follow_the_frame_lifecycle",
        elisp_form,
        expected,
    )
}

fn cfrs_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(CFRS_MELPA_PIN, "cfrs.el")
        .expect("prepare pinned cfrs source below ./tmp")
        .with_melpa_dependency(DASH_MELPA_PIN)
        .expect("prepare pinned Dash dependency")
        .with_melpa_dependency(POSFRAME_MELPA_PIN)
        .expect("prepare pinned Posframe dependency")
        .with_melpa_dependency(S_MELPA_PIN)
        .expect("prepare pinned s dependency")
        .with_timeout(Duration::from_secs(240))
        .with_prelude(PRELUDE)
}

#[test]
fn cfrs_practical_workflows_batch() {
    let cases = vec![
        terminal_session_delegates_prompt_and_initial_input_to_read_string(),
        editing_replaces_the_draft_accepts_with_return_and_trims_edges(),
        control_c_acceptance_preserves_multiline_input_and_resizes_the_frame(),
        keyboard_quit_cancels_the_read_and_hides_exactly_once(),
        prompt_width_clamps_to_the_configured_minimum_and_maximum(),
        cursor_selection_and_focus_loss_follow_the_frame_lifecycle(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("cfrs parity batch");
    assert_oracle_batch_cases(cfrs_oracle(), test_name, "cfrs parity", &cases);
}
