use expect_test::expect;

use super::ParityBatchCase;

fn python_function_scaffolding_inserts_signature_colon_strings_and_indentation() -> ParityBatchCase
{
    let elisp_form = r##"
(with-temp-buffer
  (save-window-excursion
    (switch-to-buffer (current-buffer))
    (python-mode)
    (smartparens-mode 1)
    (execute-kbd-macro "def deploy")
    (execute-kbd-macro "(")
    (let ((empty-signature
           (neomacs-smartparens-test-state :empty-signature)))
      (execute-kbd-macro "environment)")
      (execute-kbd-macro ":")
      (let ((signature (neomacs-smartparens-test-state :signature)))
        (execute-kbd-macro (kbd "RET"))
        (execute-kbd-macro "message = 'Deploy preview'")
        (execute-kbd-macro (kbd "RET"))
        (execute-kbd-macro "summary = \"Owner's preview\"")
        (list
         :empty-signature empty-signature
         :signature signature
         :body (neomacs-smartparens-test-state :body)
         :indentations
         (save-excursion
           (goto-char (point-min))
           (let (columns)
             (while (not (eobp))
               (push (current-indentation) columns)
               (forward-line 1))
             (nreverse columns))))))))
"##;
    let expected = expect![[
        r####"OK (:empty-signature (:label :empty-signature :buffer "def deploy()" :point 12 :mark nil :depth 1 :string nil :comment nil :balanced t) :signature (:label :signature :buffer "def deploy(environment):" :point 25 :mark nil :depth 0 :string nil :comment nil :balanced t) :body (:label :body :buffer "def deploy(environment):\n    message = 'Deploy preview'\n    summary = \"Owner's preview\"" :point 88 :mark nil :depth 0 :string nil :comment nil :balanced t) :indentations (0 4 4))"####
    ]];
    ParityBatchCase::value(
        "python_function_scaffolding_inserts_signature_colon_strings_and_indentation",
        elisp_form,
        expected,
    )
}

fn markdown_release_note_distinguishes_bullets_emphasis_and_fenced_code() -> ParityBatchCase {
    let elisp_form = r##"
(with-temp-buffer
  (save-window-excursion
    (switch-to-buffer (current-buffer))
    (markdown-mode)
    (setq-local markdown-gfm-use-electric-backquote nil)
    (smartparens-mode 1)
    (execute-kbd-macro "* Deploy preview")
    (execute-kbd-macro (kbd "RET RET"))
    (execute-kbd-macro "Status: **ready")
    (let ((prose (neomacs-smartparens-test-state :prose)))
      (sp-up-sexp)
      (execute-kbd-macro (kbd "RET RET"))
      (execute-kbd-macro "```elisp")
      (let ((fence-header (neomacs-smartparens-test-state :fence-header)))
        (execute-kbd-macro (kbd "RET"))
        (execute-kbd-macro "(message \"ship\")")
        (execute-kbd-macro (kbd "RET"))
        (sp-up-sexp)
        (list
         :prose prose
         :fence-header fence-header
         :completed (neomacs-smartparens-test-state :completed)
         :fence-count
         (save-excursion
           (goto-char (point-min))
           (let ((count 0))
             (while (search-forward "```" nil t)
               (setq count (1+ count)))
             count)))))))
"##;
    let expected = expect![[
        r####"OK (:prose (:label :prose :buffer "* Deploy preview\n\nStatus: **ready**" :point 34 :mark nil :depth 0 :string nil :comment nil :balanced t) :fence-header (:label :fence-header :buffer "* Deploy preview\n\nStatus: **ready**\n\n```elisp```" :point 46 :mark nil :depth 0 :string nil :comment nil :balanced t) :completed (:label :completed :buffer "* Deploy preview\n\nStatus: **ready**\n\n```elisp\n(message \"ship\")\n```" :point 64 :mark nil :depth 0 :string nil :comment nil :balanced t) :fence-count 2)"####
    ]];
    ParityBatchCase::value(
        "markdown_release_note_distinguishes_bullets_emphasis_and_fenced_code",
        elisp_form,
        expected,
    )
}

pub(super) fn language_workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        python_function_scaffolding_inserts_signature_colon_strings_and_indentation(),
        markdown_release_note_distinguishes_bullets_emphasis_and_fenced_code(),
    ]
}
