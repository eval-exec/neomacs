use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, FLX_IDO_MELPA_PIN, FLX_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const FLX_IDO_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const FLX_IDO_TEST_PRELUDE: &str = r###"
(require 'cl-lib)
(require 'flx-ido)

;; Ido binds this session variable inside `ido-read-internal'.  Bind it here
;; so direct advised-matcher workflows have the same dynamic session slot.
(defvar ido-cur-item nil)
(defvar ido-require-match nil)

(defun neomacs-flx-ido-test-face-positions (string)
  "Return the character positions highlighted by Flx in STRING."
  (let (positions)
    (dotimes (position (length string))
      (when (eq (get-text-property position 'face string)
                'flx-highlight-face)
        (push position positions)))
    (nreverse positions)))

(defun neomacs-flx-ido-test-candidate (candidate)
  "Return CANDIDATE's display, value, and Flx highlighting."
  (let ((name (ido-name candidate)))
    (list :text (substring-no-properties name)
          :value (and (consp candidate) (copy-tree (cdr candidate)))
          :faces (neomacs-flx-ido-test-face-positions name))))

(defun neomacs-flx-ido-test-candidates (candidates)
  "Return stable snapshots for CANDIDATES."
  (mapcar #'neomacs-flx-ido-test-candidate candidates))

(defun neomacs-flx-ido-test-cache-keys ()
  "Return the narrowed-match cache keys in deterministic order."
  (sort (cl-loop for key being the hash-keys
                 of flx-ido-narrowed-matches-hash collect key)
        #'string<))

(defun neomacs-flx-ido-test-with-reset (function)
  "Run FUNCTION without leaking Flx-Ido mode or narrowed-match state."
  (flx-ido-mode -1)
  (flx-ido-reset)
  (unwind-protect
      (funcall function)
    (flx-ido-mode -1)
    (flx-ido-reset)))
"###;

fn flx_ido_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(FLX_IDO_MELPA_PIN, "flx-ido.el")
        .expect("prepare revision-pinned Flx-Ido source below ./tmp")
        .with_melpa_dependency(FLX_MELPA_PIN)
        .expect("prepare revision-pinned Flx dependency below ./tmp")
        .with_prelude(FLX_IDO_TEST_PRELUDE)
        .with_timeout(FLX_IDO_TEST_TIMEOUT)
}

fn global_mode_replaces_ido_flex_order_with_scored_command_ranking() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-flx-ido-test-with-reset
 (lambda ()
   (let* ((items '("project-find-regexp"
                   "project-forget-project"
                   "json-parse-buffer"
                   "project-find-file"
                   "project-find-dir"
                   "find-file"))
          (ido-text "pff")
          (ido-current-directory "/workspace/")
          (ido-cur-item 'list)
          (ido-enable-flex-matching t)
          (ido-enable-regexp nil)
          (ido-enable-prefix nil)
          (ido-case-fold t)
          (ido-max-prospects 12)
          (flx-ido-threshold 6000)
          (flx-ido-use-faces t)
          disabled enabled)
     (setq disabled (ido-set-matches-1 items))
     (flx-ido-mode 1)
     (setq enabled (ido-set-matches-1 items))
     (list :disabled (neomacs-flx-ido-test-candidates disabled)
           :enabled (neomacs-flx-ido-test-candidates enabled)
           :cache-keys (neomacs-flx-ido-test-cache-keys)))))
"###;
    let expected = expect![[
        r###"OK (:disabled ((:text "project-find-file" :value nil :faces nil) (:text "json-parse-buffer" :value nil :faces nil)) :enabled ((:text "project-find-file" :value nil :faces (0 8 13)) (:text "json-parse-buffer" :value nil :faces (5 13 14))) :cache-keys ("/workspace/pff"))"###
    ]];
    ParityBatchCase::value(
        "global_mode_replaces_ido_flex_order_with_scored_command_ranking",
        elisp_form,
        expected,
    )
}

fn merged_project_choices_keep_directory_metadata_while_being_ranked() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-flx-ido-test-with-reset
 (lambda ()
   (let* ((items '(("release-dashboard" "/repo/app" "/repo/ops")
                   ("renderer-debug" "/repo/engine")
                   ("remote-development" "/repo/tools")
                   ("release-deploy-log" "/repo/ops")
                   ("README.md" "/repo/app")))
          (ido-text "reld")
          (ido-current-directory "/workspace/")
          (ido-cur-item 'list)
          (ido-max-prospects 12)
          (flx-ido-use-faces t))
     (flx-ido-mode 1)
     (let ((matches (ido-set-matches-1 items)))
       (list :matches (neomacs-flx-ido-test-candidates matches)
             :selected-name
             (substring-no-properties (ido-name (car matches)))
             :selected-directories (copy-tree (cdr (car matches))))))))
"###;
    let expected = expect![[
        r###"OK (:matches ((:text "release-dashboard" :value ("/repo/app" "/repo/ops") :faces (0 1 2 8)) (:text "release-deploy-log" :value ("/repo/ops") :faces (0 1 2 8))) :selected-name "release-dashboard" :selected-directories ("/repo/app" "/repo/ops"))"###
    ]];
    ParityBatchCase::value(
        "merged_project_choices_keep_directory_metadata_while_being_ranked",
        elisp_form,
        expected,
    )
}

fn incremental_queries_reuse_the_longest_cached_prefix_and_exact_result() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-flx-ido-test-with-reset
 (lambda ()
   (let* ((items '("release-dashboard"
                   "release-deploy-log"
                   "release-notes.md"
                   "renderer-debug"
                   "remote-development"
                   "README.md"))
          (ido-current-directory "/workspace/")
          (ido-cur-item 'list)
          (ido-max-prospects 12)
          (flx-ido-use-faces nil)
          (original-score (symbol-function 'flx-score))
          (score-calls 0)
          reports)
     (flx-ido-mode 1)
     (cl-letf (((symbol-function 'flx-score)
                (lambda (&rest arguments)
                  (setq score-calls (1+ score-calls))
                  (apply original-score arguments))))
       (dolist (query '("r" "re" "rel" "re"))
         (let ((before score-calls)
               (ido-text query))
           (let ((matches (ido-set-matches-1 items)))
             (push (list :query query
                         :matches
                         (mapcar (lambda (item)
                                   (substring-no-properties (ido-name item)))
                                 matches)
                         :new-score-calls (- score-calls before)
                         :cache-keys
                         (neomacs-flx-ido-test-cache-keys))
                   reports)))))
     (nreverse reports))))
"###;
    let expected = expect![[
        r###"OK ((:query "r" :matches ("release-dashboard" "renderer-debug" "remote-development" "README.md" "release-deploy-log" "release-notes.md") :new-score-calls 6 :cache-keys ("/workspace/r")) (:query "re" :matches ("release-dashboard" "renderer-debug" "remote-development" "README.md" "release-deploy-log" "release-notes.md") :new-score-calls 6 :cache-keys ("/workspace/r" "/workspace/re")) (:query "rel" :matches ("release-deploy-log" "release-dashboard" "release-notes.md" "remote-development") :new-score-calls 4 :cache-keys ("/workspace/r" "/workspace/re" "/workspace/rel")) (:query "re" :matches ("release-dashboard" "renderer-debug" "remote-development" "README.md" "release-deploy-log" "release-notes.md") :new-score-calls 0 :cache-keys ("/workspace/r" "/workspace/re" "/workspace/rel")))"###
    ]];
    ParityBatchCase::value(
        "incremental_queries_reuse_the_longest_cached_prefix_and_exact_result",
        elisp_form,
        expected,
    )
}

fn large_collections_fall_back_to_flex_until_narrowed_below_threshold() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-flx-ido-test-with-reset
 (lambda ()
   (let* ((items '("remote-development"
                   "release-notes.md"
                   "release-dashboard"
                   "release-deploy-log"))
          (ido-current-directory "/workspace/")
          (ido-cur-item 'list)
          (ido-max-prospects 12)
          (flx-ido-use-faces nil)
          fallback scored)
     (flx-ido-mode 1)
     (let ((ido-text "rel")
           (flx-ido-threshold 4))
       (setq fallback (ido-set-matches-1 items)))
     (flx-ido-reset)
     (let ((ido-text "rel")
           (flx-ido-threshold 5))
       (setq scored (ido-set-matches-1 items)))
     (list :fallback (mapcar #'ido-name fallback)
           :scored (mapcar #'ido-name scored)))))
"###;
    let expected = expect![[
        r###"OK (:fallback ("remote-development" "release-notes.md" "release-dashboard" "release-deploy-log") :scored ("release-deploy-log" "release-dashboard" "release-notes.md" "remote-development"))"###
    ]];
    ParityBatchCase::value(
        "large_collections_fall_back_to_flex_until_narrowed_below_threshold",
        elisp_form,
        expected,
    )
}

fn prospect_limit_highlights_only_visible_rows_and_undecoration_clears_them() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-flx-ido-test-with-reset
 (lambda ()
   (let* ((items '("release-dashboard"
                   "release-deploy-log"
                   "release-notes.md"
                   "renderer-debug"
                   "remote-development"))
          (ido-text "reld")
          (ido-current-directory "/workspace/")
          (ido-cur-item 'list)
          (ido-max-prospects 2)
          (flx-ido-use-faces t))
     (flx-ido-mode 1)
     (let* ((matches (ido-set-matches-1 items))
            (decorated (neomacs-flx-ido-test-candidates matches))
            (plain (flx-ido-undecorate matches)))
       (list :decorated decorated
             :plain (neomacs-flx-ido-test-candidates plain)
             :same-order
             (equal (mapcar #'ido-name matches)
                    (mapcar #'ido-name plain)))))))
"###;
    let expected = expect![[
        r###"OK (:decorated ((:text "release-dashboard" :value nil :faces (0 1 2 8)) (:text "release-deploy-log" :value nil :faces (0 1 2 8)) (:text "release-notes.md" :value nil :faces nil)) :plain ((:text "release-dashboard" :value nil :faces nil) (:text "release-deploy-log" :value nil :faces nil) (:text "release-notes.md" :value nil :faces nil)) :same-order t)"###
    ]];
    ParityBatchCase::value(
        "prospect_limit_highlights_only_visible_rows_and_undecoration_clears_them",
        elisp_form,
        expected,
    )
}

fn cache_identity_separates_directories_and_skips_file_completion_sessions() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-flx-ido-test-with-reset
 (lambda ()
   (let ((items '("release-dashboard" "renderer-debug" "README.md"))
         (ido-text "re")
         (ido-max-prospects 12)
         (flx-ido-use-faces nil))
     (flx-ido-mode 1)
     (let ((ido-cur-item 'file)
           (ido-current-directory "/repo/app/"))
       (ido-set-matches-1 items))
     (let ((after-file (neomacs-flx-ido-test-cache-keys)))
       (let ((ido-cur-item 'list)
             (ido-current-directory "/repo/app/"))
         (ido-set-matches-1 items))
       (let ((after-app (neomacs-flx-ido-test-cache-keys)))
         (let ((ido-cur-item 'list)
               (ido-current-directory "/repo/ops/"))
           (ido-set-matches-1 items))
         (list :after-file after-file
               :after-app after-app
               :after-ops (neomacs-flx-ido-test-cache-keys)
               :app-key
               (let ((ido-current-directory "/repo/app/"))
                 (flx-ido-key-for-query "re"))))))))
"###;
    let expected = expect![[
        r###"OK (:after-file nil :after-app ("/repo/app/re") :after-ops ("/repo/app/re" "/repo/ops/re") :app-key "/repo/app/re")"###
    ]];
    ParityBatchCase::value(
        "cache_identity_separates_directories_and_skips_file_completion_sessions",
        elisp_form,
        expected,
    )
}

fn ido_session_boundaries_clear_cached_matches_and_selected_highlights() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-flx-ido-test-with-reset
 (lambda ()
   (let* ((items '("release-dashboard" "renderer-debug" "README.md"))
          (ido-text "rd")
          (ido-current-directory "/workspace/")
          (ido-cur-item 'list)
          (ido-max-prospects 12)
          (flx-ido-use-faces t)
          hook-count restricted exit-state)
     (flx-ido-mode 1)
     (ido-set-matches-1 items)
     (run-hooks 'ido-minibuffer-setup-hook)
     (setq hook-count (hash-table-count flx-ido-narrowed-matches-hash))
     (setq ido-matches (ido-set-matches-1 items)
           ido-cur-list items
           ido-text-init "rd"
           ido-rescan t)
     (cl-letf (((symbol-function 'exit-minibuffer) (lambda () :kept)))
       (ido-restrict-to-matches))
     (setq restricted
           (list :cache (hash-table-count flx-ido-narrowed-matches-hash)
                 :text-init ido-text-init
                 :rescan ido-rescan
                 :exit ido-exit
                 :cur-list
                 (neomacs-flx-ido-test-candidates ido-cur-list)))
     (setq ido-matches (ido-set-matches-1 items))
     (let ((selected (ido-name (car ido-matches)))
           (ido-require-match nil)
           (ido-incomplete-regexp nil)
           exited)
       (cl-letf (((symbol-function 'exit-minibuffer)
                  (lambda () (setq exited t))))
         (ido-exit-minibuffer))
       (setq exit-state
             (list :exited exited
                   :cache (hash-table-count
                           flx-ido-narrowed-matches-hash)
                   :selected (substring-no-properties selected)
                   :faces
                   (neomacs-flx-ido-test-face-positions selected))))
     (list :hook-count hook-count
           :restricted restricted
           :exit exit-state))))
"###;
    let expected = expect![[
        r###"OK (:hook-count 0 :restricted (:cache 0 :text-init "" :rescan nil :exit keep :cur-list ((:text "release-dashboard" :value nil :faces (0 8)) (:text "renderer-debug" :value nil :faces (0 9)) (:text "README.md" :value nil :faces (0 3)))) :exit (:exited t :cache 0 :selected "release-dashboard" :faces nil))"###
    ]];
    ParityBatchCase::value(
        "ido_session_boundaries_clear_cached_matches_and_selected_highlights",
        elisp_form,
        expected,
    )
}

#[test]
fn flx_ido_package_batch() {
    let cases = vec![
        global_mode_replaces_ido_flex_order_with_scored_command_ranking(),
        merged_project_choices_keep_directory_metadata_while_being_ranked(),
        incremental_queries_reuse_the_longest_cached_prefix_and_exact_result(),
        large_collections_fall_back_to_flex_until_narrowed_below_threshold(),
        prospect_limit_highlights_only_visible_rows_and_undecoration_clears_them(),
        cache_identity_separates_directories_and_skips_file_completion_sessions(),
        ido_session_boundaries_clear_cached_matches_and_selected_highlights(),
    ];
    assert_oracle_batch_cases(flx_ido_oracle(), "flx-ido-package-batch", "Flx-Ido", &cases);
}
