use expect_test::expect;

use super::ParityBatchCase;

fn alda_mode_registry_constants_customs_highlights_and_file_association_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "alda_mode_registry_constants_customs_highlights_and_file_association_match",
        r##"(list
         (featurep 'alda-mode)
         (mapcar
          (lambda (symbol)
            (list symbol (symbol-value symbol)
                  (get symbol 'custom-type)
                  (get symbol 'custom-group)))
          '(+alda-output-buffer+
            +alda-output-name+
            +alda-comment-str+
            +alda-marker-name+
            *alda-history*
            alda-binary-location
            alda-inf-buffer-name
            alda-ess-keymap
            alda-play-region-in-repl))
         (length alda-highlights)
         (mapcar
          (lambda (entry)
            (list (car entry) (cdr entry)))
          alda-highlights)
         (assoc "\\.alda\\'" auto-mode-alist))"##,
        expect![[
            r##"OK (t ((+alda-output-buffer+ "*alda-output*" nil nil) (+alda-output-name+ "alda-playback" nil nil) (+alda-comment-str+ "#" nil nil) (+alda-marker-name+ "alda-mode-internal-marker" nil nil) (*alda-history* "" nil nil) (alda-binary-location nil string nil) (alda-inf-buffer-name "*inferior-alda*" nil nil) (alda-ess-keymap t boolean nil) (alda-play-region-in-repl nil boolean nil)) 12 (("\\(|\\)" (1 font-lock-comment-face)) ("\\([Vv][0-9]+\\):" (1 font-lock-function-name-face)) ("\\([a-zA-Z]\\{2\\}[A-Za-z0-9_-]*\\)\\( *\\(\"[A-Za-z0-9_-]*\"\\)\\)?:" (1 font-lock-type-face)) ("\\(([a-zA-Z-]+!? +\\(\\([0-9]+\\)\\|\\(\\[\\(:[a-zA-Z]+ ?\\)+\\]\\)\\))\\)" (1 font-lock-variable-name-face)) ("\\(o[0-9]+\\)" (1 font-lock-constant-face)) ("\\(>\\|<\\)" (1 font-lock-constant-face)) ("\\([@%][a-zA-Z]\\{2\\}[a-zA-Z0-9()+-]*\\)" (1 font-lock-builtin-face)) ("[a-gA-GrR][ +-]*\\([~.0-9 /]*\\(m?s\\)?\\)" (1 font-lock-builtin-face)) ("\\(\\*[0-9]+\\)" (1 font-lock-builtin-face)) ("\\({\\|}\\)" (1 font-lock-builtin-face)) ("\\(\\[\\|\\]\\)" (1 font-lock-builtin-face)) ("\\([a-gA-GrR] *[-+]+\\)" (1 font-lock-preprocessor-face))) ("\\.alda\\'" . alda-mode))"##
        ]],
    )
}

fn alda_mode_complete_callable_surface_arglists_and_commands_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "alda_mode_complete_callable_surface_arglists_and_commands_match",
        r##"(mapcar
         (lambda (symbol)
           (list symbol
                 (help-function-arglist symbol t)
                 (commandp symbol)
                 (macrop symbol)
                 (autoloadp (symbol-function symbol))))
         '(alda-mode-inf
           alda-input-sender
           alda-interpreter-running-p-1
           alda-check-or-start-interpreter
           alda-location
           alda-repl
           alda-run-alda
           alda-switch-to-interpreter
           alda-run-cmd
           alda-play-text
           alda-stop
           alda-play-file
           alda-history-append-text
           alda-history-clear
           alda-history-append-region
           alda-history-append-buffer
           alda-history-append-block
           alda-history-append-line
           alda-inf-eval-region
           alda-play-region
           alda-down
           alda-indent-line
           alda-indent-prev-level
           alda-calculate-indentation
           alda-colon
           alda-play-block
           alda-play-line
           alda-play-buffer
           alda-mode))"##,
        expect![
            "OK ((alda-mode-inf nil t nil nil) (alda-input-sender (proc string) nil nil nil) (alda-interpreter-running-p-1 nil nil nil nil) (alda-check-or-start-interpreter nil nil nil nil) (alda-location nil nil nil nil) (alda-repl nil nil nil nil) (alda-run-alda nil t nil nil) (alda-switch-to-interpreter nil t nil nil) (alda-run-cmd (&rest args) t nil nil) (alda-play-text (text) nil nil nil) (alda-stop nil nil nil nil) (alda-play-file nil t nil nil) (alda-history-append-text (text) nil nil nil) (alda-history-clear nil t nil nil) (alda-history-append-region (start end) t nil nil) (alda-history-append-buffer nil t nil nil) (alda-history-append-block nil t nil nil) (alda-history-append-line nil t nil nil) (alda-inf-eval-region (start end) t nil nil) (alda-play-region (start end) t nil nil) (alda-down nil t nil nil) (alda-indent-line nil t nil nil) (alda-indent-prev-level nil nil nil nil) (alda-calculate-indentation nil nil nil nil) (alda-colon nil t nil nil) (alda-play-block nil t nil nil) (alda-play-line nil t nil nil) (alda-play-buffer nil t nil nil) (alda-mode nil t nil nil))"
        ],
    )
}

fn alda_mode_real_buffer_contract_comments_syntax_keys_and_menu_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "alda_mode_real_buffer_contract_comments_syntax_keys_and_menu_match",
        r##"(with-temp-buffer
         (alda-mode)
         (list
          major-mode
          mode-name
          (derived-mode-p 'prog-mode)
          comment-start
          comment-padding
          comment-start-skip
          comment-multi-line
          comment-indent-function
          indent-line-function
          font-lock-defaults
          (char-syntax ?#)
          (char-syntax ?\n)
          (mapcar
           (lambda (key)
             (list key (key-binding (kbd key))))
           '(":" "C-c C-i" "C-c C-r" "C-c C-c"
             "C-c C-n" "C-c C-b" "C-c C-z"))
          (lookup-key
           (current-local-map)
           [menu-bar alda-mode alda-colon])))"##,
        expect![[
            r##"OK (alda-mode "Alda" prog-mode "#" " " "#\\s-*" "# " alda-indent-prev-level alda-indent-line (alda-highlights) 60 62 ((":" alda-colon) ("C-c C-i" alda-run-alda) ("C-c C-r" alda-play-region) ("C-c C-c" alda-play-block) ("C-c C-n" alda-play-line) ("C-c C-b" alda-play-buffer) ("C-c C-z" alda-switch-to-interpreter)) alda-colon)"##
        ]],
    )
}

fn alda_inferior_mode_configures_comint_sender_and_meta_return() -> ParityBatchCase {
    ParityBatchCase::value(
        "alda_inferior_mode_configures_comint_sender_and_meta_return",
        r##"(with-temp-buffer
         (alda-mode-inf)
         (list
          major-mode
          mode-name
          (derived-mode-p 'comint-mode)
          (local-variable-p 'comint-input-sender)
          comint-input-sender
          (key-binding (kbd "M-RET"))))"##,
        expect![[r#"OK (alda-mode-inf "Inferior Alda" comint-mode t alda-input-sender nil)"#]],
    )
}

fn alda_mode_autoload_contract_registers_mode_without_loading_implementation() -> ParityBatchCase {
    ParityBatchCase::value(
        "alda_mode_autoload_contract_registers_mode_without_loading_implementation",
        r##"(list
         (featurep 'alda-mode)
         (assoc "\\.alda\\'" auto-mode-alist)
         (mapcar
          (lambda (symbol)
            (let ((definition (symbol-function symbol)))
              (list symbol
                    (autoloadp definition)
                    (nth 1 definition)
                    (nth 4 definition)
                    (commandp symbol))))
          '(alda-mode alda-run-alda alda-play-region alda-play-buffer)))"##,
        expect![[
            r#"OK (nil ("\\.alda\\'" . alda-mode) ((alda-mode t "alda-mode" nil t) (alda-run-alda nil nil nil nil) (alda-play-region nil nil nil nil) (alda-play-buffer nil nil nil nil)))"#
        ]],
    )
}

pub(super) fn registry_alda_mode_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        alda_mode_registry_constants_customs_highlights_and_file_association_match(),
        alda_mode_complete_callable_surface_arglists_and_commands_match(),
        alda_mode_real_buffer_contract_comments_syntax_keys_and_menu_match(),
        alda_inferior_mode_configures_comint_sender_and_meta_return(),
    ]
}

pub(super) fn registry_alda_mode_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![alda_mode_autoload_contract_registers_mode_without_loading_implementation()]
}
