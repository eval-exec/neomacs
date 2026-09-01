use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, DASH_MELPA_PIN, EYEBROWSE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'eyebrowse)

(defun neomacs-eyebrowse-test-window-buffers ()
  "Return visible non-minibuffer buffer names in window order."
  (mapcar (lambda (window) (buffer-name (window-buffer window)))
          (window-list nil 'nomini (frame-first-window))))

(defun neomacs-eyebrowse-test-config-summary ()
  "Return slot and tag for each current Eyebrowse configuration."
  (mapcar (lambda (config) (list (car config) (nth 2 config)))
          (eyebrowse--get 'window-configs)))

(defun neomacs-eyebrowse-test-error (body)
  "Return stable error data from BODY."
  (condition-case error-data
      (progn (funcall body) 'no-error)
    (error (list (car error-data) (error-message-string error-data)))))
"####;

fn real_window_layouts_are_saved_restored_tagged_and_navigated() -> ParityBatchCase {
    let elisp_form = r####"
(let ((edit-buffer (get-buffer-create " *eyebrowse-edit*"))
      (tests-buffer (get-buffer-create " *eyebrowse-tests*"))
      (build-buffer (get-buffer-create " *eyebrowse-build*"))
      (workspace-buffer (get-buffer-create "workspace-two"))
      events result)
  (unwind-protect
      (save-window-excursion
        (delete-other-windows)
        (switch-to-buffer edit-buffer)
        (set-frame-parameter nil 'eyebrowse-window-configs nil)
        (set-frame-parameter nil 'eyebrowse-current-slot nil)
        (set-frame-parameter nil 'eyebrowse-last-slot nil)
        (let ((right (split-window-right)))
          (set-window-buffer right tests-buffer))
        (let ((eyebrowse-default-workspace-slot 1)
              (eyebrowse-new-workspace "workspace-two")
              (eyebrowse-wrap-around t)
              (eyebrowse-mode-line-style 'always)
              (eyebrowse-pre-window-switch-hook
               (list (lambda () (push (list 'pre
                                             (eyebrowse--get 'current-slot))
                                       events))))
              (eyebrowse-post-window-switch-hook
               (list (lambda () (push (list 'post
                                             (eyebrowse--get 'current-slot))
                                       events)))))
          (eyebrowse-mode 1)
          (let ((slot-one-created (neomacs-eyebrowse-test-window-buffers)))
            (eyebrowse-switch-to-window-config 2)
            (let ((lower (split-window-below)))
              (set-window-buffer lower build-buffer))
            (eyebrowse-rename-window-config 2 "build")
            (let ((slot-two-created (neomacs-eyebrowse-test-window-buffers)))
              (eyebrowse-switch-to-window-config 1)
              (let ((slot-one-restored
                     (neomacs-eyebrowse-test-window-buffers)))
                (eyebrowse-next-window-config nil)
                (let* ((slot-two-restored
                        (neomacs-eyebrowse-test-window-buffers))
                       (indicator (eyebrowse-mode-line-indicator))
                       (slot-one-position
                        (text-property-any 0 (length indicator) 'slot 1
                                           indicator))
                       (slot-two-position
                        (text-property-any 0 (length indicator) 'slot 2
                                           indicator)))
                  (eyebrowse-next-window-config nil)
                  (let ((wrapped-forward (eyebrowse--get 'current-slot)))
                    (eyebrowse-prev-window-config nil)
                    (setq result
                          (list
                           :created (list slot-one-created slot-two-created)
                           :restored
                           (list slot-one-restored slot-two-restored)
                           :wrapped-forward wrapped-forward
                           :wrapped-back (eyebrowse--get 'current-slot)
                           :current (eyebrowse--get 'current-slot)
                           :last (eyebrowse--get 'last-slot)
                           :configs (neomacs-eyebrowse-test-config-summary)
                           :events (nreverse events)
                           :indicator (substring-no-properties indicator)
                           :indicator-slots
                           (list
                            (and slot-one-position
                                 (get-text-property slot-one-position 'slot
                                                    indicator))
                            (and slot-two-position
                                 (get-text-property slot-two-position 'slot
                                                    indicator)))
                           :indicator-faces
                           (list
                            (and slot-one-position
                                 (get-text-property slot-one-position 'face
                                                    indicator))
                            (and slot-two-position
                                 (get-text-property slot-two-position 'face
                                                    indicator)))
                           :mode eyebrowse-mode
                           :frame-hook
                           (not (null
                                 (memq 'eyebrowse-init
                                       after-make-frame-functions)))
                           :mode-line-entry-count
                           (cl-count 'eyebrowse-mode mode-line-misc-info
                                     :key #'car-safe))))))))))
    (when eyebrowse-mode (eyebrowse-mode -1))
    (mapc (lambda (buffer)
            (when (buffer-live-p buffer) (kill-buffer buffer)))
          (list edit-buffer tests-buffer build-buffer workspace-buffer)))
  result)
"####;
    let expected = expect![[
        r#"OK (:created ((" *eyebrowse-edit*" " *eyebrowse-tests*") ("workspace-two" " *eyebrowse-build*")) :restored ((" *eyebrowse-edit*" " *eyebrowse-tests*") ("workspace-two" " *eyebrowse-build*")) :wrapped-forward 1 :wrapped-back 2 :current 2 :last 1 :configs ((1 "") (2 "build")) :events ((pre 1) (post 2) (pre 2) (post 1) (pre 1) (post 2) (pre 2) (post 1) (pre 1) (post 2)) :indicator "[1, 2:build]" :indicator-slots (1 2) :indicator-faces (eyebrowse-mode-line-inactive eyebrowse-mode-line-active) :mode t :frame-hook t :mode-line-entry-count 1)"#
    ]];
    ParityBatchCase::value(
        "real_window_layouts_are_saved_restored_tagged_and_navigated",
        elisp_form,
        expected,
    )
    .fresh_process()
}

fn moving_overwriting_and_renumbering_slots_preserves_tags_and_active_references() -> ParityBatchCase
{
    let elisp_form = r####"
(let ((saved-configs (eyebrowse--get 'window-configs))
      (saved-current (eyebrowse--get 'current-slot))
      (saved-last (eyebrowse--get 'last-slot)))
  (unwind-protect
      (progn
        (eyebrowse--set 'window-configs
          '((1 state-one "edit") (2 state-two "tests")
            (4 state-four "deploy")))
        (eyebrowse--set 'current-slot 2)
        (eyebrowse--set 'last-slot 4)
        (let ((collision
               (neomacs-eyebrowse-test-error
                (lambda () (eyebrowse-move-window-config 1 4))))
              (missing
               (neomacs-eyebrowse-test-error
                (lambda () (eyebrowse-move-window-config 9 8)))))
          (eyebrowse-move-window-config 2 3)
          (let ((after-move
                 (list :configs (eyebrowse--get 'window-configs)
                       :current (eyebrowse--get 'current-slot)
                       :last (eyebrowse--get 'last-slot))))
            (eyebrowse-move-window-config 4 3 t)
            (let ((after-overwrite
                   (list :configs (eyebrowse--get 'window-configs)
                         :current (eyebrowse--get 'current-slot)
                         :last (eyebrowse--get 'last-slot))))
              (eyebrowse--set 'window-configs
                '((2 state-two "two") (4 state-four "four")
                  (7 state-seven "seven")))
              (eyebrowse--set 'current-slot 4)
              (eyebrowse--set 'last-slot 7)
              (eyebrowse-renumber-window-configs)
              (list :collision collision
                    :missing missing
                    :after-move after-move
                    :after-overwrite after-overwrite
                    :renumbered
                    (list :configs (eyebrowse--get 'window-configs)
                          :current (eyebrowse--get 'current-slot)
                          :last (eyebrowse--get 'last-slot)))))))
    (eyebrowse--set 'window-configs saved-configs)
    (eyebrowse--set 'current-slot saved-current)
    (eyebrowse--set 'last-slot saved-last)))
"####;
    let expected = expect![[
        r#"OK (:collision (user-error "Window configuration already exists in slot 4") :missing (user-error "No window configuration in slot 9") :after-move (:configs (#1=(1 state-one "edit") (3 state-two "tests") (4 . #2=(state-four "deploy"))) :current 3 :last 4) :after-overwrite (:configs (#1# (3 . #2#)) :current 3 :last 3) :renumbered (:configs ((1 state-two "two") (2 state-four "four") (3 state-seven "seven")) :current 2 :last 3))"#
    ]];
    ParityBatchCase::value(
        "moving_overwriting_and_renumbering_slots_preserves_tags_and_active_references",
        elisp_form,
        expected,
    )
}

fn slot_formatting_and_mode_line_styles_preserve_active_metadata() -> ParityBatchCase {
    let elisp_form = r####"
(let ((saved-configs (eyebrowse--get 'window-configs))
      (saved-current (eyebrowse--get 'current-slot))
      (eyebrowse-slot-format "slot-%s")
      (eyebrowse-tagged-slot-format "%s:%t")
      (eyebrowse-mode-line-left-delimiter "{")
      (eyebrowse-mode-line-right-delimiter "}")
      (eyebrowse-mode-line-separator " | "))
  (unwind-protect
      (progn
        (eyebrowse--set 'window-configs
          '((1 state-one "edit") (3 state-three "")
            (8 state-eight "deploy")))
        (eyebrowse--set 'current-slot 3)
        (list
         :formatted
         (mapcar #'eyebrowse-format-slot (eyebrowse--get 'window-configs))
         :always
         (let ((eyebrowse-mode-line-style 'always))
           (substring-no-properties (eyebrowse-mode-line-indicator)))
         :current
         (let ((eyebrowse-mode-line-style 'current))
           (substring-no-properties (eyebrowse-mode-line-indicator)))
         :smart-many
         (let ((eyebrowse-mode-line-style 'smart))
           (substring-no-properties (eyebrowse-mode-line-indicator)))
         :smart-one
         (let ((eyebrowse-mode-line-style 'smart))
           (eyebrowse--set 'window-configs '((3 state-three "")))
           (substring-no-properties (eyebrowse-mode-line-indicator)))
         :hidden
         (let ((eyebrowse-mode-line-style 'hide))
           (substring-no-properties (eyebrowse-mode-line-indicator)))))
    (eyebrowse--set 'window-configs saved-configs)
    (eyebrowse--set 'current-slot saved-current)))
"####;
    let expected = expect![[
        r#"OK (:formatted ("1:edit" "slot-3" "8:deploy") :always "{1:edit | slot-3 | 8:deploy}" :current "{slot-3}" :smart-many "{1:edit | slot-3 | 8:deploy}" :smart-one "" :hidden "")"#
    ]];
    ParityBatchCase::value(
        "slot_formatting_and_mode_line_styles_preserve_active_metadata",
        elisp_form,
        expected,
    )
}

fn slot_input_accepts_tagged_candidates_and_numbers_and_rejects_invalid_text() -> ParityBatchCase {
    let elisp_form = r####"
(let ((saved-configs (eyebrowse--get 'window-configs))
      (eyebrowse-slot-format "%s")
      (eyebrowse-tagged-slot-format "%s:%t"))
  (unwind-protect
      (progn
        (eyebrowse--set 'window-configs
          '((1 state-one "edit") (3 state-three "deploy")))
        (list
         :free-slots
         (mapcar #'eyebrowse-free-slot '((1 2 4) (2 3) (1 2 3)))
         :numbers
         (mapcar #'eyebrowse--string-to-number
                 '("0" " 0 workspace" "-3" "12x" "workspace" ""))
         :tagged
         (cl-letf (((symbol-function 'completing-read)
                    (lambda (&rest _arguments) "3:deploy")))
           (eyebrowse--read-slot))
         :new-number
         (cl-letf (((symbol-function 'completing-read)
                    (lambda (&rest _arguments) "12")))
           (eyebrowse--read-slot))
         :invalid
         (cl-letf (((symbol-function 'completing-read)
                    (lambda (&rest _arguments) "workspace")))
           (neomacs-eyebrowse-test-error #'eyebrowse--read-slot))))
    (eyebrowse--set 'window-configs saved-configs)))
"####;
    let expected = expect![[
        r#"OK (:free-slots (3 1 4) :numbers (0 0 -3 12 nil nil) :tagged 3 :new-number 12 :invalid (user-error "Invalid slot number"))"#
    ]];
    ParityBatchCase::value(
        "slot_input_accepts_tagged_candidates_and_numbers_and_rejects_invalid_text",
        elisp_form,
        expected,
    )
}

fn eyebrowse_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(EYEBROWSE_MELPA_PIN, "eyebrowse.el")
        .expect("prepare pinned Eyebrowse source below ./tmp")
        .with_melpa_dependency(DASH_MELPA_PIN)
        .expect("prepare pinned Dash dependency below ./tmp")
        .with_timeout(Duration::from_secs(240))
        .with_prelude(PRELUDE)
}

#[test]
fn eyebrowse_practical_workflows_batch() {
    let cases = vec![
        real_window_layouts_are_saved_restored_tagged_and_navigated(),
        moving_overwriting_and_renumbering_slots_preserves_tags_and_active_references(),
        slot_formatting_and_mode_line_styles_preserve_active_metadata(),
        slot_input_accepts_tagged_candidates_and_numbers_and_rejects_invalid_text(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("eyebrowse parity batch");
    assert_oracle_batch_cases(eyebrowse_oracle(), test_name, "eyebrowse parity", &cases);
}
