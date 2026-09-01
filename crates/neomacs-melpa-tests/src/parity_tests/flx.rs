use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, FLX_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const FLX_TEST_TIMEOUT: Duration = Duration::from_secs(60);
const FLX_TEST_PRELUDE: &str = r###"
(require 'cl-lib)
(require 'flx)

(defun neomacs-flx-test-display (candidate)
  "Return CANDIDATE's display string."
  (if (consp candidate) (car candidate) candidate))

(defun neomacs-flx-test-rank (query candidates cache)
  "Rank CANDIDATES for QUERY with CACHE like Flx's Ido and Helm consumers."
  (let (scored)
    (dolist (candidate candidates)
      (let* ((display (neomacs-flx-test-display candidate))
             (score (flx-score display query cache)))
        (when score
          (push (list display
                      :score (car score)
                      :positions (cdr score)
                      :value (and (consp candidate) (cdr candidate)))
                scored))))
    (sort scored
          (lambda (left right)
            (let ((left-score (plist-get (cdr left) :score))
                  (right-score (plist-get (cdr right) :score)))
              (if (= left-score right-score)
                  (string< (car left) (car right))
                (> left-score right-score)))))))

(defun neomacs-flx-test-face-runs (string)
  "Return exact face runs from STRING."
  (let ((position 0)
        runs)
    (while (< position (length string))
      (let* ((face (get-text-property position 'face string))
             (next (next-single-property-change
                    position 'face string (length string))))
        (push (list (substring-no-properties string position next)
                    face position next)
              runs)
        (setq position next)))
    (nreverse runs)))
"###;

fn flx_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(FLX_MELPA_PIN, "flx.el")
        .expect("prepare revision-pinned Flx source below ./tmp")
        .with_prelude(FLX_TEST_PRELUDE)
        .with_timeout(FLX_TEST_TIMEOUT)
}

fn repository_file_finder_balances_basename_word_boundaries_and_contiguous_matches()
-> ParityBatchCase {
    let elisp_form = r###"
(let* ((files '("projects/clojure-mode/clojure-mode.el"
                "projects/prelude/core/prelude-mode.el"
                "projects/prelude/modules/personal-mode.el"
                "docs/prelude-mode.md"
                "src/release_mode.rs"
                "src/modes/pre-release-mode.el"))
       (cache (flx-make-filename-cache)))
  (list
   :premode (neomacs-flx-test-rank "premode" files cache)
   :pre-mode (neomacs-flx-test-rank "pre-mode" files cache)
   :release-mode (neomacs-flx-test-rank "relmode" files cache)
   :cache-entries (hash-table-count cache)))
"###;
    let expected = expect![[
        r#"OK (:premode (("docs/prelude-mode.md" :score 460 :positions (5 6 7 13 14 15 16) :value nil) ("projects/prelude/core/prelude-mode.el" :score 446 :positions (22 23 24 30 31 32 33) :value nil) ("src/modes/pre-release-mode.el" :score 440 :positions (10 14 15 22 23 24 25) :value nil) ("projects/clojure-mode/clojure-mode.el" :score 334 :positions (0 27 28 30 31 32 33) :value nil) ("projects/prelude/modules/personal-mode.el" :score 298 :positions (0 1 26 34 35 36 37) :value nil)) :pre-mode (("docs/prelude-mode.md" :score 577 :positions (5 6 11 12 13 14 15 16) :value nil) ("projects/clojure-mode/clojure-mode.el" :score 574 :positions (0 27 28 29 30 31 32 33) :value nil) ("src/modes/pre-release-mode.el" :score 566 :positions (10 14 20 21 22 23 24 25) :value nil) ("projects/prelude/core/prelude-mode.el" :score 561 :positions (22 23 28 29 30 31 32 33) :value nil) ("projects/prelude/modules/personal-mode.el" :score 386 :positions (0 1 26 33 34 35 36 37) :value nil)) :release-mode (("src/release_mode.rs" :score 460 :positions (4 5 6 12 13 14 15) :value nil) ("src/modes/pre-release-mode.el" :score 425 :positions (14 15 16 22 23 24 25) :value nil) ("docs/prelude-mode.md" :score 372 :positions (6 7 8 13 14 15 16) :value nil) ("projects/prelude/core/prelude-mode.el" :score 358 :positions (23 24 25 30 31 32 33) :value nil) ("projects/clojure-mode/clojure-mode.el" :score 208 :positions (14 15 23 30 31 32 33) :value nil) ("projects/prelude/modules/personal-mode.el" :score 183 :positions (1 26 32 34 35 36 37) :value nil)) :cache-entries 7)"#
    ]];
    ParityBatchCase::value(
        "repository_file_finder_balances_basename_word_boundaries_and_contiguous_matches",
        elisp_form,
        expected,
    )
}

fn command_palette_ranks_real_commands_and_respects_explicit_uppercase_intent() -> ParityBatchCase {
    let elisp_form = r###"
(let* ((commands '("project-find-file"
                   "project-find-regexp"
                   "project-forget-project"
                   "find-file"
                   "find-function"
                   "grep-find"
                   "magit-file-dispatch"
                   "neomacs-publish-release"
                   "JSONParseBuffer"
                   "json-parse-buffer"))
       (cache (flx-make-string-cache)))
  (list
   :project-file (neomacs-flx-test-rank "pff" commands cache)
   :grep-find (neomacs-flx-test-rank "gf" commands cache)
   :lowercase-json (neomacs-flx-test-rank "jpb" commands cache)
   :uppercase-json (neomacs-flx-test-rank "JPB" commands cache)))
"###;
    let expected = expect![[
        r#"OK (:project-file (("project-find-file" :score 237 :positions (0 8 13) :value nil) ("json-parse-buffer" :score 116 :positions (5 13 14) :value nil) ("JSONParseBuffer" :score 42 :positions (4 11 12) :value nil)) :grep-find (("grep-find" :score 163 :positions (0 5) :value nil) ("magit-file-dispatch" :score 74 :positions (2 6) :value nil)) :lowercase-json (("json-parse-buffer" :score 237 :positions (0 5 11) :value nil) ("JSONParseBuffer" :score 160 :positions (0 4 9) :value nil)) :uppercase-json (("JSONParseBuffer" :score 160 :positions (0 4 9) :value nil)))"#
    ]];
    ParityBatchCase::value(
        "command_palette_ranks_real_commands_and_respects_explicit_uppercase_intent",
        elisp_form,
        expected,
    )
}

fn incremental_buffer_narrowing_reuses_heatmaps_while_the_ranked_set_converges() -> ParityBatchCase
{
    let elisp_form = r###"
(let* ((buffers '("release-dashboard"
                  "release-deploy-log"
                  "release-notes.md"
                  "renderer-debug"
                  "remote-development"
                  "*Messages*"
                  "README.md"))
       (cache (flx-make-string-cache))
       reports)
  (dolist (query '("r" "re" "rel" "reld" "reldl"))
    (let ((ranked (neomacs-flx-test-rank query buffers cache)))
      (push (list :query query
                  :matches (mapcar #'car ranked)
                  :top-details (car ranked)
                  :cache-entries (hash-table-count cache))
            reports)))
  (nreverse reports))
"###;
    let expected = expect![[
        r#"OK ((:query "r" :matches ("README.md" "release-dashboard" "remote-development" "renderer-debug" "release-deploy-log" "release-notes.md") :top-details ("README.md" :score 83 :positions (0) :value nil) :cache-entries 8) (:query "re" :matches ("README.md" "release-dashboard" "remote-development" "renderer-debug" "release-deploy-log" "release-notes.md") :top-details ("README.md" :score 140 :positions (0 1) :value nil) :cache-entries 8) (:query "rel" :matches ("release-deploy-log" "release-dashboard" "release-notes.md" "remote-development") :top-details ("release-deploy-log" :score 214 :positions (0 1 15) :value nil) :cache-entries 8) (:query "reld" :matches ("release-dashboard" "release-deploy-log" "release-notes.md") :top-details ("release-dashboard" :score 291 :positions (0 1 2 8) :value nil) :cache-entries 8) (:query "reldl" :matches ("release-deploy-log") :top-details ("release-deploy-log" :score 363 :positions (0 1 2 8 15) :value nil) :cache-entries 8))"#
    ]];
    ParityBatchCase::value(
        "incremental_buffer_narrowing_reuses_heatmaps_while_the_ranked_set_converges",
        elisp_form,
        expected,
    )
}

fn annotated_completion_candidates_keep_their_values_and_highlight_exact_match_runs()
-> ParityBatchCase {
    let elisp_form = r###"
(let* ((candidate
        (cons (propertize "deploy-release-to-region"
                          'help-echo "stale decoration")
              '(:kind command :key "C-c d" :scope project)))
       (score (flx-score (car candidate) "drelreg"
                         (flx-make-string-cache)))
       (decorated (flx-propertize candidate score t))
       (plain (flx-propertize decorated nil)))
  (list
   :score score
   :decorated-text (substring-no-properties (car decorated))
   :decorated-runs (neomacs-flx-test-face-runs (car decorated))
   :value (cdr decorated)
   :stale-help-removed (get-text-property 0 'help-echo (car decorated))
   :cleared-text (car plain)
   :cleared-face (get-text-property 0 'face (car plain))
   :cleared-value (cdr plain)))
"###;
    let expected = expect![[
        r#"OK (:score (455 0 7 8 9 18 19 20) :decorated-text "deploy-release-to-region [455]" :decorated-runs (("d" flx-highlight-face 0 1) ("eploy-" nil 1 7) ("rel" flx-highlight-face 7 10) ("ease-to-" nil 10 18) ("reg" flx-highlight-face 18 21) ("ion [455]" nil 21 30)) :value #1=(:kind command :key "C-c d" :scope project) :stale-help-removed nil :cleared-text "deploy-release-to-region [455]" :cleared-face nil :cleared-value #1#)"#
    ]];
    ParityBatchCase::value(
        "annotated_completion_candidates_keep_their_values_and_highlight_exact_match_runs",
        elisp_form,
        expected,
    )
}

fn multilingual_buffer_switching_uses_unicode_case_boundaries_without_losing_character_positions()
-> ParityBatchCase {
    let elisp_form = r###"
(let* ((buffers '("ΔeltaReleaseNotes"
                  "δelta-release-notes"
                  "ДоставкаРелиза"
                  "доставка-релиза"
                  "déploiement-release"
                  "release-notes"))
       (cache (flx-make-string-cache)))
  (list
   :greek-folded (neomacs-flx-test-rank "δrn" buffers cache)
   :greek-explicit-capitals (neomacs-flx-test-rank "ΔRN" buffers cache)
   :cyrillic-folded (neomacs-flx-test-rank "др" buffers cache)
   :cyrillic-explicit-capitals (neomacs-flx-test-rank "ДР" buffers cache)
   :accented (neomacs-flx-test-rank "dér" buffers cache)))
"###;
    let expected = expect![[
        r#"OK (:greek-folded (("ΔeltaReleaseNotes" :score 243 :positions (0 5 12) :value nil) ("δelta-release-notes" :score 237 :positions (0 6 14) :value nil)) :greek-explicit-capitals (("ΔeltaReleaseNotes" :score 243 :positions (0 5 12) :value nil)) :cyrillic-folded (("ДоставкаРелиза" :score 165 :positions (0 8) :value nil) ("доставка-релиза" :score 163 :positions (0 9) :value nil)) :cyrillic-explicit-capitals (("ДоставкаРелиза" :score 165 :positions (0 8) :value nil)) :accented (("déploiement-release" :score 220 :positions (0 1 12) :value nil)))"#
    ]];
    ParityBatchCase::value(
        "multilingual_buffer_switching_uses_unicode_case_boundaries_without_losing_character_positions",
        elisp_form,
        expected,
    )
}

#[test]
fn flx_package_batch() {
    assert_oracle_batch_cases(
        flx_oracle(),
        "flx-package-batch",
        "Flx",
        &[
            repository_file_finder_balances_basename_word_boundaries_and_contiguous_matches(),
            command_palette_ranks_real_commands_and_respects_explicit_uppercase_intent(),
            incremental_buffer_narrowing_reuses_heatmaps_while_the_ranked_set_converges(),
            annotated_completion_candidates_keep_their_values_and_highlight_exact_match_runs(),
            multilingual_buffer_switching_uses_unicode_case_boundaries_without_losing_character_positions(),
        ],
    );
}
