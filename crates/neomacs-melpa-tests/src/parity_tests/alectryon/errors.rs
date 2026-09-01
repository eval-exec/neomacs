use expect_test::expect;

use super::ParityBatchCase;

fn alectryon_unknown_mode_and_configuration_failures_have_precise_actionable_messages()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alectryon_unknown_mode_and_configuration_failures_have_precise_actionable_messages",
        r##"(mapcar
 (lambda (thunk)
   (condition-case err
       (funcall thunk)
     (error (list (car err) (error-message-string err)))))
 (list
  (lambda () (with-temp-buffer
          (setq-local alectryon-prog-mode 'haskell-mode)
          (alectryon--prog-plist)))
  (lambda () (with-temp-buffer
          (setq-local alectryon-text-mode 'org-mode)
          (alectryon--text-plist)))
  (lambda () (alectryon--prog-mode-p 'fundamental-mode))
  (lambda () (with-temp-buffer
          (fundamental-mode)
          (alectryon--config :tag)))
  (lambda () (with-temp-buffer
          (setq-local alectryon-prog-mode 'haskell-mode)
          (alectryon--config-code+markup)))))"##,
        expect![[
            r#"OK ((error "Unrecognized Alectryon programming mode: haskell-mode") (error "Unrecognized Alectryon markup mode: org-mode") (error "Unrecognized mode: fundamental-mode (expecting one of (rst-mode markdown-mode typst-ts-mode coq-mode lean4-mode dafny-mode))") (error "Unrecognized mode: fundamental-mode (expecting one of (rst-mode markdown-mode typst-ts-mode coq-mode lean4-mode dafny-mode))") (error "Unrecognized Alectryon programming mode: haskell-mode"))"#
        ]],
    )
}

fn alectryon_read_mode_rejects_uninstalled_choices_and_empty_supported_sets() -> ParityBatchCase {
    ParityBatchCase::value(
        "alectryon_read_mode_rejects_uninstalled_choices_and_empty_supported_sets",
        r##"(let ((original-prog alectryon-prog-modes)
      (original-text alectryon-text-modes))
  (unwind-protect
      (list
       (cl-letf (((symbol-function 'completing-read)
                  (lambda (&rest _) "missing-mode")))
         (condition-case err
             (alectryon--read-mode t)
           (error (list (car err) (error-message-string err)))))
       (progn
         (setq alectryon-text-modes
               '((missing-one :tag "one") (missing-two :tag "two")))
         (condition-case err
             (alectryon--read-text-mode)
           (error (list (car err) (error-message-string err))))))
    (setq alectryon-prog-modes original-prog
          alectryon-text-modes original-text)))"##,
        expect![[
            r#"OK ((user-error "Not installed: missing-mode") (error "No supported text mode found"))"#
        ]],
    )
}

fn alectryon_atomic_rolls_back_failed_complex_edits_and_groups_successful_edits_for_undo()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alectryon_atomic_rolls_back_failed_complex_edits_and_groups_successful_edits_for_undo",
        r##"(list
 (with-temp-buffer
   (buffer-enable-undo)
   (insert "stable")
   (setq buffer-undo-list nil)
   (let ((failure
          (condition-case err
              (alectryon--atomic
                (goto-char (point-max))
                (insert "-partial")
                (delete-region 1 3)
                (error "conversion exploded"))
            (error (list (car err) (error-message-string err))))))
     (list failure (buffer-string) (point) buffer-undo-list)))
 (with-temp-buffer
   (buffer-enable-undo)
   (insert "base")
   (setq buffer-undo-list nil)
   (alectryon--atomic
     (goto-char (point-max))
     (insert "-one")
     (insert "-two")
     (upcase-region 1 5))
   (let ((after (buffer-string))
         (undo-records (copy-tree buffer-undo-list)))
     (undo)
     (list after (buffer-string) undo-records))))"##,
        expect![[
            r#"OK (((error "conversion exploded") "stable" 7 nil) ("BASE-one-two" "base" (nil (1 . 5) ("base" . 1) (5 . 13))))"#
        ]],
    )
}

fn alectryon_mode_recording_does_not_poison_buffers_in_unsupported_major_modes() -> ParityBatchCase
{
    ParityBatchCase::value(
        "alectryon_mode_recording_does_not_poison_buffers_in_unsupported_major_modes",
        r##"(with-temp-buffer
  (fundamental-mode)
  (setq-local alectryon--original-mode nil
              alectryon-prog-mode 'coq-mode
              alectryon-text-mode nil)
  (let ((failure
         (condition-case err
             (alectryon-mode 1)
           (error (list (car err) (error-message-string err))))))
    (list failure
          major-mode alectryon-mode
          alectryon--original-mode
          alectryon-prog-mode alectryon-text-mode
          (memq #'alectryon--save write-contents-functions))))"##,
        expect![[
            r#"OK ((error "Unrecognized mode: fundamental-mode (expecting one of (rst-mode markdown-mode typst-ts-mode coq-mode lean4-mode dafny-mode))") fundamental-mode t nil coq-mode nil nil)"#
        ]],
    )
}

pub(super) fn errors_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        alectryon_unknown_mode_and_configuration_failures_have_precise_actionable_messages(),
        alectryon_read_mode_rejects_uninstalled_choices_and_empty_supported_sets(),
        alectryon_atomic_rolls_back_failed_complex_edits_and_groups_successful_edits_for_undo(),
        alectryon_mode_recording_does_not_poison_buffers_in_unsupported_major_modes(),
    ]
}
