use std::time::Duration;

use expect_test::expect;

use crate::{CCC_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const CCC_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const CCC_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defun neomacs-ccc-test-frame-state ()
  "Capture the real and CCC baseline color parameters of the selected frame."
  (let ((frame (selected-frame)))
    (mapcar (lambda (parameter)
              (cons parameter (frame-parameter frame parameter)))
            '(cursor-color foreground-color background-color
              ccc-frame-cursor-color
              ccc-frame-foreground-color
              ccc-frame-background-color))))

(defun neomacs-ccc-test-restore-frame-state (state)
  "Restore selected-frame color STATE captured by the parity fixture."
  (modify-frame-parameters (selected-frame) state))

(defun neomacs-ccc-test-visible-colors ()
  "Return the selected frame's three user-visible color parameters."
  (let ((frame (selected-frame)))
    (list :cursor (frame-parameter frame 'cursor-color)
          :foreground (frame-parameter frame 'foreground-color)
          :background (frame-parameter frame 'background-color))))
"##;

fn ccc_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(CCC_MELPA_PIN, "ccc.el")
        .expect("prepare pinned CCC source below ./tmp")
        .with_prelude(CCC_TEST_PRELUDE)
        .with_timeout(CCC_TEST_TIMEOUT)
}

fn cursor_override_follows_the_active_work_buffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "cursor_override_follows_the_active_work_buffer",
        r##"
(let ((saved (neomacs-ccc-test-frame-state))
      (review-buffer (generate-new-buffer " *ccc-review*"))
      (chat-buffer (generate-new-buffer " *ccc-chat*"))
      (plain-buffer (generate-new-buffer " *ccc-plain*")))
  (unwind-protect
      (progn
        (set-cursor-color "black")
        (with-current-buffer review-buffer
          (ccc-set-buffer-local-cursor-color "red"))
        (with-current-buffer chat-buffer
          (ccc-set-buffer-local-cursor-color "blue"))
        (let ((review-local
               (buffer-local-value 'ccc-buffer-local-cursor-color
                                   review-buffer))
              (chat-local
               (buffer-local-value 'ccc-buffer-local-cursor-color
                                   chat-buffer)))
          (ccc-update-buffer-local-frame-params review-buffer)
          (let ((review-visible
                 (frame-parameter nil 'cursor-color)))
            (ccc-update-buffer-local-frame-params chat-buffer)
            (let ((chat-visible
                   (frame-parameter nil 'cursor-color)))
              (ccc-update-buffer-local-frame-params plain-buffer)
              (list :locals (list review-local chat-local)
                    :review-visible review-visible
                    :chat-visible chat-visible
                    :plain-visible (frame-parameter nil 'cursor-color)
                    :remembered-global (ccc-frame-cursor-color))))))
    (mapc (lambda (buffer)
            (when (buffer-live-p buffer) (kill-buffer buffer)))
          (list review-buffer chat-buffer plain-buffer))
    (neomacs-ccc-test-restore-frame-state saved)))
"##,
        expect![[
            r##"OK (:locals ("red" "blue") :review-visible "red" :chat-visible "blue" :plain-visible "black" :remembered-global "black")"##
        ]],
    )
}

fn local_cursor_override_survives_global_changes_and_restores_the_baseline() -> ParityBatchCase {
    ParityBatchCase::value(
        "local_cursor_override_survives_global_changes_and_restores_the_baseline",
        r##"
(let ((saved (neomacs-ccc-test-frame-state)))
  (unwind-protect
      (with-temp-buffer
        (set-cursor-color "black")
        (set-cursor-color "yellow")
        (let ((global-before-local (ccc-frame-cursor-color)))
          (ccc-set-buffer-local-cursor-color "red")
          (set-cursor-color "blue")
          (let ((visible-after-external-change
                 (frame-parameter nil 'cursor-color))
                (remembered-during-local (ccc-frame-cursor-color)))
            (ccc-update-buffer-local-cursor-color)
            (let ((visible-after-refresh
                   (frame-parameter nil 'cursor-color)))
              (ccc-set-cursor-color-buffer-local nil)
              (list :global-before-local global-before-local
                    :visible-after-external-change visible-after-external-change
                    :remembered-during-local remembered-during-local
                    :visible-after-refresh visible-after-refresh
                    :visible-after-disable
                    (frame-parameter nil 'cursor-color)
                    :local-after-disable ccc-buffer-local-cursor-color)))))
    (neomacs-ccc-test-restore-frame-state saved)))
"##,
        expect![[
            r##"OK (:global-before-local "yellow" :visible-after-external-change "blue" :remembered-during-local "yellow" :visible-after-refresh "red" :visible-after-disable "yellow" :local-after-disable nil)"##
        ]],
    )
}

fn terminal_buffers_keep_rendered_colors_and_record_unspecified_fallbacks() -> ParityBatchCase {
    ParityBatchCase::value(
        "terminal_buffers_keep_rendered_colors_and_record_unspecified_fallbacks",
        r##"
(let ((saved (neomacs-ccc-test-frame-state)))
  (unwind-protect
      (with-temp-buffer
        (set-foreground-color "white")
        (set-background-color "black")
        (let ((before (neomacs-ccc-test-visible-colors)))
          (ccc-set-buffer-local-foreground-color "red")
          (ccc-set-buffer-local-background-color "blue")
          (list :window-system window-system
                :before before
                :after (neomacs-ccc-test-visible-colors)
                :locals
                (list ccc-buffer-local-foreground-color
                      ccc-buffer-local-background-color)
                :remembered
                (list (ccc-frame-foreground-color)
                      (ccc-frame-background-color)))))
    (neomacs-ccc-test-restore-frame-state saved)))
"##,
        expect![[
            r##"OK (:window-system nil :before (:cursor "white" :foreground "white" :background "black") :after (:cursor "white" :foreground "white" :background "black") :locals ("unspecified-fg" "unspecified-bg") :remembered ("unspecified-fg" "unspecified-bg"))"##
        ]],
    )
}

fn setup_installs_the_runtime_hooks_and_captures_the_current_palette() -> ParityBatchCase {
    ParityBatchCase::value(
        "setup_installs_the_runtime_hooks_and_captures_the_current_palette",
        r##"
(let ((saved-frame (neomacs-ccc-test-frame-state))
      (saved-defaults
       (list ccc-default-cursor-color
             ccc-default-foreground-color
             ccc-default-background-color))
      (saved-post-command-hook (default-value 'post-command-hook))
      (saved-after-frame-hook (default-value 'after-make-frame-functions)))
  (unwind-protect
      (progn
        (setq ccc-default-cursor-color nil
              ccc-default-foreground-color nil
              ccc-default-background-color nil)
        (set-cursor-color "yellow")
        (set-foreground-color "white")
        (set-background-color "black")
        (ccc-setup)
              (list :hooks
              (list (and (memq 'ccc-update-buffer-local-frame-params
                               (default-value 'post-command-hook))
                         t)
                    (and (memq 'ccc-setup-new-frame
                               (default-value 'after-make-frame-functions))
                         t))
              :defaults
              (list ccc-default-cursor-color
                    ccc-default-foreground-color
                    ccc-default-background-color)
              :remembered
              (list (ccc-frame-cursor-color)
                    (ccc-frame-foreground-color)
                    (ccc-frame-background-color))))
    (set-default 'post-command-hook saved-post-command-hook)
    (set-default 'after-make-frame-functions saved-after-frame-hook)
    (setq ccc-default-cursor-color (nth 0 saved-defaults)
          ccc-default-foreground-color (nth 1 saved-defaults)
          ccc-default-background-color (nth 2 saved-defaults))
    (neomacs-ccc-test-restore-frame-state saved-frame)))
"##,
        expect![[
            r##"OK (:hooks (t t) :defaults ("white" "white" "black") :remembered ("white" "unspecified-fg" "unspecified-bg"))"##
        ]],
    )
}

#[test]
fn ccc_package_batch() {
    let cases = vec![
        cursor_override_follows_the_active_work_buffer(),
        local_cursor_override_survives_global_changes_and_restores_the_baseline(),
        terminal_buffers_keep_rendered_colors_and_record_unspecified_fallbacks(),
        setup_installs_the_runtime_hooks_and_captures_the_current_palette(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed CCC parity test");
    assert_oracle_batch_cases(ccc_oracle(), test_name, "ccc_parity", &cases);
}
