use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, YASNIPPET_MELPA_PIN, YASNIPPET_SNIPPETS_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'yasnippet-snippets)

(defun neomacs-yasnippet-snippets-test-with-mode (mode body)
  "Run BODY in a displayed temporary buffer using MODE and Yasnippet."
  (with-temp-buffer
    (save-window-excursion
      (switch-to-buffer (current-buffer))
      (funcall mode)
      (yas-minor-mode 1)
      (funcall body))))

(defun neomacs-yasnippet-snippets-test-state ()
  "Return exact text, point, and active snippet count."
  (list :text (buffer-substring-no-properties (point-min) (point-max))
        :point (point)
        :active (length (yas-active-snippets 'all))))

(defun neomacs-yasnippet-snippets-test-type (text)
  "Type TEXT through `self-insert-command', one character at a time."
  (mapc (lambda (event)
          (let ((last-command-event event))
            (self-insert-command 1)))
        (string-to-list text)))
"####;

fn python_function_authoring_builds_a_typed_signature_and_argument_documentation() -> ParityBatchCase
{
    let elisp_form = r####"
(progn
  (yasnippet-snippets-initialize)
  (yasnippet-snippets-initialize)
  (list
   :installation
   (list :directory (file-name-nondirectory
                     (directory-file-name yasnippet-snippets-dir))
         :registered
         (cl-count 'yasnippet-snippets-dir yas-snippet-dirs :test #'eq)
         :template-present
         (file-exists-p
          (expand-file-name "python-mode/function_docstring"
                            yasnippet-snippets-dir)))
   :workflow
   (neomacs-yasnippet-snippets-test-with-mode
    'python-mode
    (lambda ()
      (insert "fd")
      (let ((expanded (yas-expand)))
        (yas-clear-field)
        (neomacs-yasnippet-snippets-test-type "fetch_user")
        (yas-next-field-or-maybe-expand)
        (yas-clear-field)
        (neomacs-yasnippet-snippets-test-type
         "user_id: int, region = \"us\"")
        (yas-next-field-or-maybe-expand)
        (yas-clear-field)
        (neomacs-yasnippet-snippets-test-type "Fetch a user profile.")
        (yas-next-field-or-maybe-expand)
        (yas-exit-all-snippets)
        (list :expanded expanded
              :final (neomacs-yasnippet-snippets-test-state)))))))
"####;
    let expected = expect![[
        r#"OK (:installation (:directory "snippets" :registered 1 :template-present t) :workflow (:expanded t :final (:text "def fetch_user(user_id: int, region = \"us\"):\n    \"\"\"Fetch a user profile.\n    Keyword Arguments:\n    user_id -- int: \n    region  -- (default \"us\")\n    \"\"\"\n    \n" :point 161 :active 0)))"#
    ]];
    ParityBatchCase::value(
        "python_function_authoring_builds_a_typed_signature_and_argument_documentation",
        elisp_form,
        expected,
    )
}

fn emacs_lisp_command_authoring_preserves_interactive_defaults() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-yasnippet-snippets-test-with-mode
 'emacs-lisp-mode
 (lambda ()
   (insert "def")
   (let ((expanded (yas-expand)))
     (yas-clear-field)
     (neomacs-yasnippet-snippets-test-type "neomacs-publish-artifact")
     (yas-next-field-or-maybe-expand)
     (yas-clear-field)
     (neomacs-yasnippet-snippets-test-type "artifact")
     (yas-next-field-or-maybe-expand)
     (yas-clear-field)
     (neomacs-yasnippet-snippets-test-type
      "Publish ARTIFACT to the release channel.")
     (yas-next-field 4)
     (insert "(message \"Published %s\" artifact)")
     (yas-exit-all-snippets)
     (list :expanded expanded
           :final (neomacs-yasnippet-snippets-test-state)))))
"####;
    let expected = expect![[
        r#"OK (:expanded t :final (:text "(defun neomacs-publish-artifact (artifact)\n  \"Publish ARTIFACT to the release channel.\"\n  (interactive \"P\")\n  (message \"Published %s\" artifact))" :point 111 :active 0))"#
    ]];
    ParityBatchCase::value(
        "emacs_lisp_command_authoring_preserves_interactive_defaults",
        elisp_form,
        expected,
    )
}

fn c_authoring_scaffolds_a_checked_allocation_program_from_standard_headers() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-yasnippet-snippets-test-with-mode
 'c-mode
 (lambda ()
   (yasnippet-snippets--no-indent)
   (insert "io")
   (let ((stdio-expanded (yas-expand)))
     (yas-exit-all-snippets)
     (insert "\n")
     (insert "std")
     (let ((stdlib-expanded (yas-expand)))
       (yas-exit-all-snippets)
       (insert "\n")
       (insert "ass")
       (let ((assert-expanded (yas-expand)))
         (yas-exit-all-snippets)
         (insert
          "\nint main(void) {\n    char *items = malloc(3 * sizeof *items);\n    assert(items != NULL);\n    free(items);\n    return 0;\n}\n")
         (list :expanded
               (list stdio-expanded stdlib-expanded assert-expanded)
               :final (neomacs-yasnippet-snippets-test-state)))))))
"####;
    let expected = expect![[
        r##"OK (:expanded (t t t) :final (:text "#include <stdio.h>\n#include <stdlib.h>\n#include <assert.h>\n\nint main(void) {\n    char *items = malloc(3 * sizeof *items);\n    assert(items != NULL);\n    free(items);\n    return 0;\n}\n" :point 183 :active 0))"##
    ]];
    ParityBatchCase::value(
        "c_authoring_scaffolds_a_checked_allocation_program_from_standard_headers",
        elisp_form,
        expected,
    )
}

fn shell_authoring_nests_a_real_argument_loop_inside_a_function() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-yasnippet-snippets-test-with-mode
 'sh-mode
 (lambda ()
   (yasnippet-snippets--no-indent)
   (insert "f")
   (let ((function-expanded (yas-expand)))
     (execute-kbd-macro "publish_all")
     (yas-next-field-or-maybe-expand)
     (insert "for")
     (let ((loop-expanded (yas-expand)))
       (yas-next-field-or-maybe-expand)
       (yas-next-field-or-maybe-expand)
       (insert "publish \"$var\"")
       (yas-exit-all-snippets)
       (list :expanded (list function-expanded loop-expanded)
             :final (neomacs-yasnippet-snippets-test-state))))))
"####;
    let expected = expect![[
        r#"OK (:expanded (t t) :final (:text "function publish_all {\n         for var in stuff; do\n    publish \"$var\"\ndone\n}" :point 77 :active 0))"#
    ]];
    ParityBatchCase::value(
        "shell_authoring_nests_a_real_argument_loop_inside_a_function",
        elisp_form,
        expected,
    )
}

fn org_authoring_combines_an_executable_block_with_release_documentation() -> ParityBatchCase {
    let elisp_form = r####"
(neomacs-yasnippet-snippets-test-with-mode
 'org-mode
 (lambda ()
   (insert "<src")
   (let ((source-expanded (yas-expand)))
     (yas-clear-field)
     (neomacs-yasnippet-snippets-test-type "emacs-lisp")
     (yas-next-field-or-maybe-expand)
     (insert "(message \"release deployed\")")
     (yas-exit-all-snippets)
     (goto-char (point-max))
     (insert "\n")
     (insert "<li")
     (let ((link-expanded (yas-expand)))
       (yas-clear-field)
       (neomacs-yasnippet-snippets-test-type
        "https://example.test/releases/42")
       (yas-next-field-or-maybe-expand)
       (yas-clear-field)
       (neomacs-yasnippet-snippets-test-type "release notes")
       (yas-exit-all-snippets)
       (list :expanded (list source-expanded link-expanded)
             :final (neomacs-yasnippet-snippets-test-state))))))
"####;
    let expected = expect![[
        r##"OK (:expanded (t t) :final (:text "#+begin_src emacs-lisp\n  (message \"release deployed\")\n#+end_src\n\n[[https://example.test/releases/42][release notes]]\n" :point 118 :active 0))"##
    ]];
    ParityBatchCase::value(
        "org_authoring_combines_an_executable_block_with_release_documentation",
        elisp_form,
        expected,
    )
}

fn inherited_programming_snippet_uses_each_major_modes_real_comment_syntax() -> ParityBatchCase {
    let elisp_form = r####"
(mapcar
 (lambda (mode)
   (cons
    mode
    (neomacs-yasnippet-snippets-test-with-mode
     mode
     (lambda ()
       (insert "t")
       (let ((expanded (yas-expand)))
         (insert "verify rollback metrics")
         (yas-exit-all-snippets)
         (list :expanded expanded
               :final (neomacs-yasnippet-snippets-test-state)))))))
 '(python-mode c-mode emacs-lisp-mode))
"####;
    let expected = expect![[
        r##"OK ((python-mode :expanded t :final (:text "# TODO: verify rollback metrics" :point 9 :active 0)) (c-mode :expanded t :final (:text "/* TODO: verify rollback metrics */" :point 10 :active 0)) (emacs-lisp-mode :expanded t :final (:text ";TODO: verify rollback metrics" :point 8 :active 0)))"##
    ]];
    ParityBatchCase::value(
        "inherited_programming_snippet_uses_each_major_modes_real_comment_syntax",
        elisp_form,
        expected,
    )
}

fn yasnippet_snippets_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(YASNIPPET_SNIPPETS_MELPA_PIN, "yasnippet-snippets.el")
        .expect("prepare pinned Yasnippet-Snippets source below ./tmp")
        .with_melpa_dependency(YASNIPPET_MELPA_PIN)
        .expect("prepare pinned Yasnippet dependency below ./tmp")
        .with_timeout(Duration::from_secs(240))
        .with_prelude(PRELUDE)
}

#[test]
fn yasnippet_snippets_practical_workflows_batch() {
    let cases = vec![
        python_function_authoring_builds_a_typed_signature_and_argument_documentation(),
        emacs_lisp_command_authoring_preserves_interactive_defaults(),
        c_authoring_scaffolds_a_checked_allocation_program_from_standard_headers(),
        shell_authoring_nests_a_real_argument_loop_inside_a_function(),
        org_authoring_combines_an_executable_block_with_release_documentation(),
        inherited_programming_snippet_uses_each_major_modes_real_comment_syntax(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("yasnippet-snippets parity batch");
    assert_oracle_batch_cases(
        yasnippet_snippets_oracle(),
        test_name,
        "yasnippet-snippets parity",
        &cases,
    );
}
