use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, EVIL_MATCHIT_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const EVIL_MATCHIT_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const EVIL_MATCHIT_TEST_PRELUDE: &str = r###"
(require 'cl-lib)
(require 'evil-matchit)

(defun neomacs-evilmi-test-location ()
  "Describe the current source location without buffer identity."
  (list :point (point)
        :line (line-number-at-pos)
        :column (current-column)
        :char (and (char-after) (string (char-after)))
        :symbol (thing-at-point 'symbol t)
        :line-text (buffer-substring-no-properties
                    (line-beginning-position)
                    (line-end-position))))

(defun neomacs-evilmi-test-jump ()
  "Jump once and return the resulting stable source location."
  (evilmi-jump-items-native 1)
  (neomacs-evilmi-test-location))
"###;

fn evil_matchit_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(EVIL_MATCHIT_MELPA_PIN, "evil-matchit.el")
        .expect("prepare revision-pinned Evil Matchit source below ./tmp")
        .with_prelude(EVIL_MATCHIT_TEST_PRELUDE)
        .with_timeout(EVIL_MATCHIT_TEST_TIMEOUT)
}

fn javascript_nested_payload_ignores_delimiters_in_strings_and_comments() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (js-mode)
  (insert "function deploy(release) {\n"
          "  const payload = { message: \"literal } stays\",\n"
          "                    steps: [validate(release), publish(release)] };\n"
          "  // A comment with } must not close deploy.\n"
          "  if (release.ready) {\n"
          "    notify(release);\n"
          "  }\n"
          "}\n")
  (font-lock-ensure (point-min) (point-max))
  (goto-char (point-min))
  (search-forward "{")
  (backward-char)
  (let ((function-open (neomacs-evilmi-test-location))
        (function-close (neomacs-evilmi-test-jump))
        (function-return (neomacs-evilmi-test-jump)))
    (search-forward "payload = {")
    (backward-char)
    (let ((payload-open (neomacs-evilmi-test-location))
          (payload-close (neomacs-evilmi-test-jump))
          (payload-return (neomacs-evilmi-test-jump)))
      (search-forward "\"literal")
      (goto-char (match-beginning 0))
      (let ((quote-open (neomacs-evilmi-test-location))
            (quote-close (neomacs-evilmi-test-jump))
            (quote-return (neomacs-evilmi-test-jump)))
        (list :function (list function-open function-close function-return)
              :payload (list payload-open payload-close payload-return)
              :string (list quote-open quote-close quote-return))))))
"###;
    let expected = expect![[
        r####"OK (:function ((:point 26 :line 1 :column 25 :char "{" :symbol nil :line-text "function deploy(release) {") (:point 237 :line 8 :column 0 :char "}" :symbol nil :line-text "}") (:point 26 :line 1 :column 25 :char "{" :symbol nil :line-text "function deploy(release) {")) :payload ((:point 46 :line 2 :column 18 :char "{" :symbol nil :line-text "  const payload = { message: \"literal } stays\",") (:point 141 :line 3 :column 65 :char "}" :symbol nil :line-text "                    steps: [validate(release), publish(release)] };") (:point 46 :line 2 :column 18 :char "{" :symbol nil :line-text "  const payload = { message: \"literal } stays\",")) :string ((:point 57 :line 2 :column 29 :char "\"" :symbol nil :line-text "  const payload = { message: \"literal } stays\",") (:point 73 :line 2 :column 45 :char "\"" :symbol "stays" :line-text "  const payload = { message: \"literal } stays\",") (:point 57 :line 2 :column 29 :char "\"" :symbol nil :line-text "  const payload = { message: \"literal } stays\",")))"####
    ]];
    ParityBatchCase::value(
        "javascript_nested_payload_ignores_delimiters_in_strings_and_comments",
        elisp_form,
        expected,
    )
}

fn merge_conflict_cycle_visits_ours_separator_theirs_and_origin() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (js-mode)
  (insert "const release = prepare();\n"
          "<<<<<<< HEAD\n"
          "deployCanary(release);\n"
          "=======\n"
          "deployStable(release);\n"
          ">>>>>>> release/stable\n"
          "audit(release);\n")
  (goto-char (point-min))
  (search-forward "<<<<<<<")
  (goto-char (match-beginning 0))
  (let ((ours (neomacs-evilmi-test-location))
        (separator (neomacs-evilmi-test-jump))
        (theirs (neomacs-evilmi-test-jump))
        (origin (neomacs-evilmi-test-jump)))
    (list :cycle (list ours separator theirs origin)
          :source (buffer-string))))
"###;
    let expected = expect![[
        r####"OK (:cycle ((:point 28 :line 2 :column 0 :char "<" :symbol nil :line-text "<<<<<<< HEAD") (:point 64 :line 4 :column 0 :char "=" :symbol nil :line-text "=======") (:point 117 :line 6 :column 22 :char "\n" :symbol "stable" :line-text ">>>>>>> release/stable") (:point 28 :line 2 :column 0 :char "<" :symbol nil :line-text "<<<<<<< HEAD")) :source "const release = prepare();\n<<<<<<< HEAD\ndeployCanary(release);\n=======\ndeployStable(release);\n>>>>>>> release/stable\naudit(release);\n")"####
    ]];
    ParityBatchCase::value(
        "merge_conflict_cycle_visits_ours_separator_theirs_and_origin",
        elisp_form,
        expected,
    )
}

fn html_nested_and_self_closing_tags_match_real_document_structure() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (html-mode)
  (insert "<section id=\"release\">\n"
          "  <div class=\"card\" data-expression=\"3 > 2\">\n"
          "    <img src=\"canary.svg\" />\n"
          "    <div class=\"status\">Ready</div>\n"
          "  </div>\n"
          "</section>\n")
  (font-lock-ensure (point-min) (point-max))
  (goto-char (point-min))
  (search-forward "<div class=\"card\"")
  (goto-char (match-beginning 0))
  (let ((outer-open (neomacs-evilmi-test-location))
        (outer-close (neomacs-evilmi-test-jump))
        (outer-return (neomacs-evilmi-test-jump)))
    (goto-char (point-min))
    (search-forward "<img")
    (goto-char (match-beginning 0))
    (let ((image-open (neomacs-evilmi-test-location))
          (image-close (neomacs-evilmi-test-jump))
          (image-return (progn
                          (backward-char)
                          (neomacs-evilmi-test-jump))))
      (list :nested (list outer-open outer-close outer-return)
            :self-closing (list image-open image-close image-return)))))
"###;
    let expected = expect![[
        r####"OK (:nested ((:point 26 :line 2 :column 2 :char "<" :symbol nil :line-text "  <div class=\"card\" data-expression=\"3 > 2\">") (:point 142 :line 5 :column 8 :char "\n" :symbol nil :line-text "  </div>") (:point 26 :line 2 :column 2 :char "<" :symbol nil :line-text "  <div class=\"card\" data-expression=\"3 > 2\">")) :self-closing ((:point 73 :line 3 :column 4 :char "<" :symbol nil :line-text "    <img src=\"canary.svg\" />") (:point 97 :line 3 :column 28 :char "\n" :symbol nil :line-text "    <img src=\"canary.svg\" />") (:point 73 :line 3 :column 4 :char "<" :symbol nil :line-text "    <img src=\"canary.svg\" />")))"####
    ]];
    ParityBatchCase::value(
        "html_nested_and_self_closing_tags_match_real_document_structure",
        elisp_form,
        expected,
    )
}

fn python_branch_cycle_skips_nested_blocks_and_returns_from_the_suite_end() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (python-mode)
  (insert "if release.channel == \"canary\":\n"
          "    if health_ok:\n"
          "        deploy_canary()\n"
          "    else:\n"
          "        rollback()\n"
          "elif release.channel == \"stable\":\n"
          "    deploy_stable()\n"
          "else:\n"
          "    hold_release()\n"
          "audit_release()\n")
  (goto-char (point-min))
  (let ((if-branch (neomacs-evilmi-test-location))
        (elif-branch (neomacs-evilmi-test-jump))
        (else-branch (neomacs-evilmi-test-jump))
        (suite-end (neomacs-evilmi-test-jump))
        (cycle-return (neomacs-evilmi-test-jump)))
    (list :cycle (list if-branch elif-branch else-branch suite-end cycle-return)
          :source (buffer-string))))
"###;
    let expected = expect![[
        r####"OK (:cycle ((:point 1 :line 1 :column 0 :char "i" :symbol "if" :line-text "if release.channel == \"canary\":") (:point 104 :line 6 :column 0 :char "e" :symbol "elif" :line-text "elif release.channel == \"stable\":") (:point 158 :line 8 :column 0 :char "e" :symbol "else" :line-text "else:") (:point 182 :line 9 :column 18 :char "\n" :symbol nil :line-text "    hold_release()") (:point 1 :line 1 :column 0 :char "i" :symbol "if" :line-text "if release.channel == \"canary\":")) :source "if release.channel == \"canary\":\n    if health_ok:\n        deploy_canary()\n    else:\n        rollback()\nelif release.channel == \"stable\":\n    deploy_stable()\nelse:\n    hold_release()\naudit_release()\n")"####
    ]];
    ParityBatchCase::value(
        "python_branch_cycle_skips_nested_blocks_and_returns_from_the_suite_end",
        elisp_form,
        expected,
    )
}

fn c_preprocessor_and_switch_cycles_respect_nested_control_structures() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (c-mode)
  (insert "#ifdef RELEASE_FEATURE\n"
          "# ifdef DEBUG_RELEASE\n"
          "trace_release();\n"
          "# endif\n"
          "deploy_canary();\n"
          "#else\n"
          "deploy_stable();\n"
          "#endif\n\n"
          "switch (release_state) {\n"
          "case READY:\n"
          "  deploy();\n"
          "  break;\n"
          "case FAILED:\n"
          "  rollback();\n"
          "  break;\n"
          "default:\n"
          "  hold();\n"
          "}\n")
  (font-lock-ensure (point-min) (point-max))
  (goto-char (point-min))
  (let ((directive-open (neomacs-evilmi-test-location))
        (directive-middle (neomacs-evilmi-test-jump))
        (directive-close (neomacs-evilmi-test-jump))
        (directive-return (neomacs-evilmi-test-jump)))
    (search-forward "switch")
    (goto-char (match-beginning 0))
    (let ((switch-open (neomacs-evilmi-test-location))
          (first-case (neomacs-evilmi-test-jump))
          (second-case (neomacs-evilmi-test-jump))
          (default-case (neomacs-evilmi-test-jump))
          (switch-return (neomacs-evilmi-test-jump)))
      (list :directives
            (list directive-open directive-middle directive-close directive-return)
            :switch
            (list switch-open first-case second-case default-case switch-return)))))
"###;
    let expected = expect![[
        r####"OK (:directives ((:point 1 :line 1 :column 0 :char "#" :symbol nil :line-text "#ifdef RELEASE_FEATURE") (:point 88 :line 6 :column 0 :char "#" :symbol nil :line-text "#else") (:point 117 :line 8 :column 6 :char "\n" :symbol "endif" :line-text "#endif") (:point 1 :line 1 :column 0 :char "#" :symbol nil :line-text "#ifdef RELEASE_FEATURE")) :switch ((:point 119 :line 10 :column 0 :char "s" :symbol "switch" :line-text "switch (release_state) {") (:point 144 :line 11 :column 0 :char "c" :symbol "case" :line-text "case READY:") (:point 177 :line 14 :column 0 :char "c" :symbol "case" :line-text "case FAILED:") (:point 221 :line 17 :column 8 :char "\n" :symbol nil :line-text "default:") (:point 142 :line 10 :column 23 :char "{" :symbol nil :line-text "switch (release_state) {")))"####
    ]];
    ParityBatchCase::value(
        "c_preprocessor_and_switch_cycles_respect_nested_control_structures",
        elisp_form,
        expected,
    )
}

fn org_source_and_quote_blocks_cycle_without_confusing_embedded_code() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (org-mode)
  (insert "* Release runbook\n"
          "#+begin_src javascript\n"
          "if (release.ready) {\n"
          "  deploy(release);\n"
          "}\n"
          "#+end_src\n\n"
          "#+begin_quote\n"
          "Canary operators verify the release.\n"
          "#+end_quote\n")
  (font-lock-ensure (point-min) (point-max))
  (goto-char (point-min))
  (search-forward "#+begin_src")
  (goto-char (line-beginning-position))
  (let ((source-open (neomacs-evilmi-test-location))
        (source-close (neomacs-evilmi-test-jump))
        (source-return (neomacs-evilmi-test-jump)))
    (search-forward "#+begin_quote")
    (goto-char (line-beginning-position))
    (let ((quote-open (neomacs-evilmi-test-location))
          (quote-close (neomacs-evilmi-test-jump))
          (quote-return (neomacs-evilmi-test-jump)))
      (list :source-block (list source-open source-close source-return)
            :quote-block (list quote-open quote-close quote-return)))))
"###;
    let expected = expect![[
        r####"OK (:source-block ((:point 19 :line 2 :column 0 :char "#" :symbol nil :line-text "#+begin_src javascript") (:point 93 :line 6 :column 9 :char "\n" :symbol "+end_src" :line-text "#+end_src") (:point 19 :line 2 :column 0 :char "#" :symbol nil :line-text "#+begin_src javascript")) :quote-block ((:point 95 :line 8 :column 0 :char "#" :symbol nil :line-text "#+begin_quote") (:point 157 :line 10 :column 11 :char "\n" :symbol "+end_quote" :line-text "#+end_quote") (:point 95 :line 8 :column 0 :char "#" :symbol nil :line-text "#+begin_quote")))"####
    ]];
    ParityBatchCase::value(
        "org_source_and_quote_blocks_cycle_without_confusing_embedded_code",
        elisp_form,
        expected,
    )
}

fn diff_navigation_keeps_each_file_patch_as_one_matching_unit() -> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (diff-mode)
  (insert "diff --git a/deploy.el b/deploy.el\n"
          "index 1111111..2222222 100644\n"
          "--- a/deploy.el\n"
          "+++ b/deploy.el\n"
          "@@ -1 +1 @@\n"
          "-channel: stable\n"
          "+channel: canary\n"
          "diff --git a/runbook.org b/runbook.org\n"
          "index 3333333..4444444 100644\n"
          "--- a/runbook.org\n"
          "+++ b/runbook.org\n"
          "@@ -1 +1 @@\n"
          "-Stable only\n"
          "+Canary then stable\n")
  (goto-char (point-min))
  (let ((patch-open (neomacs-evilmi-test-location))
        (patch-close (neomacs-evilmi-test-jump))
        (patch-return (neomacs-evilmi-test-jump)))
    (forward-line 1)
    (let ((header-line (neomacs-evilmi-test-location))
          (header-close (neomacs-evilmi-test-jump)))
      (list :from-diff (list patch-open patch-close patch-return)
            :from-index (list header-line header-close)))))
"###;
    let expected = expect![[
        r####"OK (:from-diff ((:point 1 :line 1 :column 0 :char "d" :symbol "diff" :line-text "diff --git a/deploy.el b/deploy.el") (:point 143 :line 7 :column 16 :char "\n" :symbol "canary" :line-text "+channel: canary") (:point 1 :line 1 :column 0 :char "d" :symbol "diff" :line-text "diff --git a/deploy.el b/deploy.el")) :from-index ((:point 36 :line 2 :column 0 :char "i" :symbol "index" :line-text "index 1111111..2222222 100644") (:point 143 :line 7 :column 16 :char "\n" :symbol "canary" :line-text "+channel: canary")))"####
    ]];
    ParityBatchCase::value(
        "diff_navigation_keeps_each_file_patch_as_one_matching_unit",
        elisp_form,
        expected,
    )
}

fn shell_first_branch_selection_and_deletion_share_boundaries_and_fire_jump_hooks()
-> ParityBatchCase {
    let elisp_form = r###"
(let ((source "before_release\nif health_check; then\n  deploy_canary\nelse\n  rollback_release\nfi\nafter_release\n")
      (events nil)
      (kill-ring nil)
      (kill-ring-yank-pointer nil)
      (interprogram-cut-function nil))
  (let ((evilmi-jump-hook
         (list (lambda (before-p)
                 (push (list (if before-p :before :after)
                             (line-number-at-pos)
                             (current-column))
                       events)))))
    (let ((selection
           (with-temp-buffer
             (sh-mode)
             (insert source)
             (goto-char (point-min))
             (forward-line 1)
             (evilmi-select-items 1)
             (list :bounds (list (region-beginning) (region-end))
                   :text (buffer-substring-no-properties
                          (region-beginning) (region-end))
                   :point (neomacs-evilmi-test-location))))
          (deletion
           (with-temp-buffer
             (sh-mode)
             (insert source)
             (goto-char (point-min))
             (forward-line 1)
             (evilmi-delete-items 1)
             (list :source (buffer-string)
                   :point (neomacs-evilmi-test-location)
                   :kill (car kill-ring)))))
      (list :selection selection
            :deletion deletion
            :hook-events (nreverse events)))))
"###;
    let expected = expect![[
        r####"OK (:selection (:bounds (16 53) :text "if health_check; then\n  deploy_canary" :point (:point 53 :line 3 :column 15 :char "\n" :symbol "deploy_canary" :line-text "  deploy_canary")) :deletion (:source "before_release\nelse\n  rollback_release\nfi\nafter_release\n" :point (:point 16 :line 2 :column 0 :char "e" :symbol "else" :line-text "else") :kill "if health_check; then\n  deploy_canary\n") :hook-events ((:before 2 0) (:after 4 0) (:before 2 0) (:after 4 0)))"####
    ]];
    ParityBatchCase::value(
        "shell_first_branch_selection_and_deletion_share_boundaries_and_fire_jump_hooks",
        elisp_form,
        expected,
    )
}

#[test]
fn evil_matchit_package_batch() {
    assert_oracle_batch_cases(
        evil_matchit_oracle(),
        "evil-matchit-package-batch",
        "evil-matchit",
        &[
            javascript_nested_payload_ignores_delimiters_in_strings_and_comments(),
            merge_conflict_cycle_visits_ours_separator_theirs_and_origin(),
            html_nested_and_self_closing_tags_match_real_document_structure(),
            python_branch_cycle_skips_nested_blocks_and_returns_from_the_suite_end(),
            c_preprocessor_and_switch_cycles_respect_nested_control_structures(),
            org_source_and_quote_blocks_cycle_without_confusing_embedded_code(),
            diff_navigation_keeps_each_file_patch_as_one_matching_unit(),
            shell_first_branch_selection_and_deletion_share_boundaries_and_fire_jump_hooks(),
        ],
    );
}
