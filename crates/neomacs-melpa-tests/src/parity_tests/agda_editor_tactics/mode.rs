use expect_test::expect;

use super::ParityBatchCase;

fn agda_editor_tactics_mode_enable_toggle_and_disable_lifecycle_matches() -> ParityBatchCase {
    ParityBatchCase::value(
        "agda_editor_tactics_mode_enable_toggle_and_disable_lifecycle_matches",
        r##"(progn
         (defvar agda-editor-tactics-test-events nil)
         (setq agda-editor-tactics-test-events nil)
         (with-temp-buffer
           (let ((agda-editor-tactics-mode-hook
                  (list
                   (lambda ()
                     (push
                      (list agda-editor-tactics-mode
                            (buffer-name)
                            (buffer-modified-p))
                      agda-editor-tactics-test-events)))))
             (list
              (agda-editor-tactics-mode 1)
              agda-editor-tactics-mode
              (agda-editor-tactics-mode)
              agda-editor-tactics-mode
              (agda-editor-tactics-mode 0)
              agda-editor-tactics-mode
              (reverse agda-editor-tactics-test-events)))))"##,
        expect![[
            r#"OK (t t t t nil nil ((t " *temp*" nil) (t " *temp*" nil) (nil " *temp*" nil)))"#
        ]],
    )
}

fn agda_editor_tactics_mode_state_is_buffer_local_and_independent() -> ParityBatchCase {
    ParityBatchCase::value(
        "agda_editor_tactics_mode_state_is_buffer_local_and_independent",
        r##"(let ((first (generate-new-buffer " *agda-tactics-first*"))
             (second (generate-new-buffer " *agda-tactics-second*")))
         (unwind-protect
             (progn
               (with-current-buffer first
                 (insert "record First : Set where\n  field\n    value : Set")
                 (agda-editor-tactics-mode 1))
               (with-current-buffer second
                 (insert "record Second : Set where\n  field\n    value : Set")
                 (agda-editor-tactics-mode 0))
               (list
                (with-current-buffer first
                  (list agda-editor-tactics-mode
                        (buffer-string)
                        (local-variable-p 'agda-editor-tactics-mode)))
                (with-current-buffer second
                  (list agda-editor-tactics-mode
                        (buffer-string)
                        (local-variable-p 'agda-editor-tactics-mode)))
                (default-value 'agda-editor-tactics-mode)))
           (kill-buffer first)
           (kill-buffer second)))"##,
        expect![[
            r#"OK ((t "record First : Set where\n  field\n    value : Set" t) (nil "record Second : Set where\n  field\n    value : Set" t) nil)"#
        ]],
    )
}

fn agda_editor_tactics_mode_integrates_with_a_real_major_mode_hook() -> ParityBatchCase {
    ParityBatchCase::value(
        "agda_editor_tactics_mode_integrates_with_a_real_major_mode_hook",
        r##"(progn
         (define-derived-mode agda-editor-tactics-test-mode
           fundamental-mode
           "Agda-Tactics-Test")
         (add-hook
          'agda-editor-tactics-test-mode-hook
          #'agda-editor-tactics-mode)
         (with-temp-buffer
           (agda-editor-tactics-test-mode)
           (insert "record Hooked : Set where\n  field\n    x : Set")
           (list
            major-mode
            mode-name
            agda-editor-tactics-mode
            (local-variable-p 'agda-editor-tactics-mode)
            (buffer-substring-no-properties (point-min) (point-max)))))"##,
        expect![[
            r#"OK (agda-editor-tactics-test-mode "Agda-Tactics-Test" t t "record Hooked : Set where\n  field\n    x : Set")"#
        ]],
    )
}

fn agda_editor_tactics_mode_hook_observes_each_explicit_transition() -> ParityBatchCase {
    ParityBatchCase::value(
        "agda_editor_tactics_mode_hook_observes_each_explicit_transition",
        r##"(progn
         (defvar agda-editor-tactics-test-states nil)
         (setq agda-editor-tactics-test-states nil)
         (with-temp-buffer
           (let ((agda-editor-tactics-mode-hook
                  (list
                   (lambda ()
                     (push
                      (cons agda-editor-tactics-mode
                            (local-variable-p 'agda-editor-tactics-mode))
                      agda-editor-tactics-test-states)))))
             (agda-editor-tactics-mode 1)
             (agda-editor-tactics-mode 1)
             (agda-editor-tactics-mode -1)
             (agda-editor-tactics-mode -1)
             (list
              agda-editor-tactics-mode
              (reverse agda-editor-tactics-test-states)
              (local-variable-p 'agda-editor-tactics-mode)))))"##,
        expect!["OK (nil ((t . t) (t . t) (nil . t) (nil . t)) t)"],
    )
}

pub(super) fn mode_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        agda_editor_tactics_mode_enable_toggle_and_disable_lifecycle_matches(),
        agda_editor_tactics_mode_state_is_buffer_local_and_independent(),
        agda_editor_tactics_mode_integrates_with_a_real_major_mode_hook(),
        agda_editor_tactics_mode_hook_observes_each_explicit_transition(),
    ]
}
