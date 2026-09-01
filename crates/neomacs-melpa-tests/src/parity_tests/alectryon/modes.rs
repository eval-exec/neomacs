use expect_test::expect;

use super::ParityBatchCase;

fn alectryon_code_mode_activation_installs_real_buffer_local_editing_and_save_state()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alectryon_code_mode_activation_installs_real_buffer_local_editing_and_save_state",
        r##"(with-temp-buffer
  (coq-mode)
  (list
   major-mode alectryon-mode alectryon--prog-mode alectryon--text-mode
   alectryon--original-mode alectryon-prog-mode alectryon-text-mode
   visual-line-mode
   (local-variable-p 'write-contents-functions)
   write-contents-functions
   flyspell-mode-hook
   font-lock-extra-managed-props
   (lookup-key (current-local-map) (kbd "C-c C-="))
   (lookup-key (current-local-map) (kbd "C-c C-S-a"))
   (local-variable-p 'font-lock-syntactic-face-function)
   (not (null alectryon--prog-font-lock-keywords))))"##,
        expect![
            "OK (coq-mode t t nil coq-mode coq-mode nil t t (alectryon--save) (alectryon--flyspell-hook t) (modification-hooks wrap-prefix display) 1 1 t t)"
        ],
    )
}

fn alectryon_markup_activation_uses_text_keymap_and_records_original_mode_without_code_state()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alectryon_markup_activation_uses_text_keymap_and_records_original_mode_without_code_state",
        r##"(with-temp-buffer
  (let ((alectryon--winding-down t))
    (rst-mode))
  (setq-local alectryon-prog-mode 'coq-mode
              alectryon-text-mode 'rst-mode)
  (alectryon-mode 1)
  (list
   major-mode alectryon-mode alectryon--prog-mode alectryon--text-mode
   alectryon--original-mode
   write-contents-functions
   (lookup-key (current-local-map) (kbd "C-c C-S-a"))
   (lookup-key (current-local-map) (kbd "C-c C-="))
   (lookup-key (current-local-map) [remap newline])
   visual-line-mode
   alectryon--prog-font-lock-keywords))"##,
        expect!["OK (rst-mode t nil t rst-mode (alectryon--save) nil rst-adjust 1 nil nil)"],
    )
}

fn alectryon_disabling_in_original_code_mode_cleans_hooks_maps_and_font_lock_state()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alectryon_disabling_in_original_code_mode_cleans_hooks_maps_and_font_lock_state",
        r##"(with-temp-buffer
  (coq-mode)
  (flyspell-mode 1)
  (let ((before
         (list alectryon-mode alectryon--prog-mode
               (memq 'alectryon-comment flyspell-prog-text-faces)
               (memq #'alectryon--save write-contents-functions)
               (memq #'alectryon--flyspell-hook flyspell-mode-hook))))
    (alectryon-mode -1)
    (list before
          major-mode alectryon-mode alectryon--prog-mode alectryon--text-mode
          (memq 'alectryon-comment flyspell-prog-text-faces)
          (memq #'alectryon--save write-contents-functions)
          (memq #'alectryon--flyspell-hook flyspell-mode-hook)
          (lookup-key (current-local-map) (kbd "C-c C-="))
          (local-variable-p 'font-lock-syntactic-face-function))))"##,
        expect![
            "OK ((t t nil (alectryon--save) (alectryon--flyspell-hook t)) coq-mode nil nil nil nil nil nil 1 nil)"
        ],
    )
}

fn alectryon_auto_enable_hooks_respect_winding_down_and_cover_only_code_modes() -> ParityBatchCase {
    ParityBatchCase::value(
        "alectryon_auto_enable_hooks_respect_winding_down_and_cover_only_code_modes",
        r##"(list
 (with-temp-buffer
   (coq-mode)
   (list major-mode alectryon-mode alectryon-prog-mode))
 (with-temp-buffer
   (lean4-mode)
   (list major-mode alectryon-mode alectryon-prog-mode))
 (with-temp-buffer
   (dafny-mode)
   (list major-mode alectryon-mode alectryon-prog-mode))
 (with-temp-buffer
   (let ((alectryon--winding-down t))
     (coq-mode))
   (list major-mode alectryon-mode alectryon--original-mode))
 (with-temp-buffer
   (rst-mode)
   (list major-mode alectryon-mode alectryon--original-mode)))"##,
        expect![
            "OK ((coq-mode t coq-mode) (lean4-mode t lean4-mode) (dafny-mode t dafny-mode) (coq-mode nil nil) (rst-mode nil nil))"
        ],
    )
}

fn alectryon_failed_disable_still_runs_cleanup_through_unwind_protect() -> ParityBatchCase {
    ParityBatchCase::value(
        "alectryon_failed_disable_still_runs_cleanup_through_unwind_protect",
        r##"(with-temp-buffer
  (let ((alectryon--winding-down t))
    (rst-mode))
  (setq-local alectryon-prog-mode 'coq-mode
              alectryon-text-mode 'rst-mode
              alectryon--original-mode 'coq-mode)
  (alectryon-mode 1)
  (cl-letf (((symbol-function 'alectryon--toggle)
             (lambda () (error "deterministic conversion failure"))))
    (let ((failure
           (condition-case err
               (progn (alectryon-mode -1) nil)
             (error (list (car err) (error-message-string err))))))
      (list failure
            alectryon-mode
            (memq #'alectryon--save write-contents-functions)
            (memq #'alectryon--flyspell-hook flyspell-mode-hook)
            alectryon--text-mode alectryon--prog-mode))))"##,
        expect![[r#"OK ((error "deterministic conversion failure") nil nil nil nil nil)"#]],
    )
}

fn alectryon_flyspell_integration_is_buffer_local_idempotent_and_reversible() -> ParityBatchCase {
    ParityBatchCase::value(
        "alectryon_flyspell_integration_is_buffer_local_idempotent_and_reversible",
        r##"(with-temp-buffer
  (let ((alectryon--winding-down t))
    (coq-mode))
  (setq-local flyspell-mode t
              flyspell-prog-text-faces '(font-lock-string-face))
  (alectryon--flyspell-hook)
  (alectryon--flyspell-hook)
  (let ((enabled (copy-sequence flyspell-prog-text-faces)))
    (alectryon--flyspell-unhook)
    (list enabled flyspell-prog-text-faces
          (local-variable-p 'flyspell-prog-text-faces))))"##,
        expect!["OK ((alectryon-comment font-lock-string-face) (font-lock-string-face) t)"],
    )
}

pub(super) fn modes_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        alectryon_code_mode_activation_installs_real_buffer_local_editing_and_save_state(),
        alectryon_markup_activation_uses_text_keymap_and_records_original_mode_without_code_state(),
        alectryon_disabling_in_original_code_mode_cleans_hooks_maps_and_font_lock_state(),
        alectryon_auto_enable_hooks_respect_winding_down_and_cover_only_code_modes(),
        alectryon_failed_disable_still_runs_cleanup_through_unwind_protect(),
        alectryon_flyspell_integration_is_buffer_local_idempotent_and_reversible(),
    ]
}
