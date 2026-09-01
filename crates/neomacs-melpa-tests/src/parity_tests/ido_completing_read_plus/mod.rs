//! Practical parity for ido-completing-read+ fallbacks and filters.
//!
//! These cases toggle `ido-ubiquitous-mode`, fall back from empty and
//! oversized collections, compute prefix completions, and apply include
//! and exclude restrictions.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, IDO_COMPLETING_READ_PLUS_MELPA_PIN, MEMOIZE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'ido)
(require 'ido-completing-read+)
(set-window-configuration (current-window-configuration))
(get-buffer-create " *code-conversion-work*")

(defconst icr471-test-tree
  "6e254f1248169e50ffc0351621d19e5c1be08214")
(defconst icr471-test-manifest
  '(("ido-completing-read+-pkg.el" . "b4d3224bc34b3d01255582aeb2f0ae2772e1ab94f58ee0a61fbb0ba95cb41112")
    ("ido-completing-read+.el" . "b11038866b9c68b52fb24f178a78c8c3f7d8f2085a900e219c60ca36095300a1")))

(defun icr471-test-sha (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun icr471-test-source-state ()
  (let* ((located (locate-library "ido-completing-read+.el"))
         (main (and located (file-truename located)))
         (directory (and main (file-name-directory main)))
         (files
          (and directory
               (sort
                (mapcar (lambda (file) (file-relative-name file directory))
                        (seq-filter
                         (lambda (file)
                           (and (string-suffix-p ".el" file)
                                (not (string-suffix-p "-autoloads.el" file))))
                         (directory-files-recursively directory "\\.el\\'")))
                #'string<)))
         (manifest
          (and files
               (mapcar (lambda (file)
                         (cons file (icr471-test-sha
                                     (expand-file-name file directory))))
                       files))))
    (unless (and located main directory
                 (string-suffix-p "/ido-completing-read+.el" main)
                 (not (file-symlink-p located))
                 (equal files (mapcar #'car icr471-test-manifest)))
      (error "Unexpected installed ido-completing-read+ payload: %S"
             (or manifest files)))
    (dolist (entry icr471-test-manifest)
      (let ((file (expand-file-name (car entry) directory))
            (expected (cdr entry)))
        (unless (and (file-regular-p file)
                     (not (file-symlink-p file))
                     (equal (icr471-test-sha file) expected))
          (error "Unexpected installed ido-completing-read+ source: %S"
                 (cons entry manifest)))))
    (list :tree icr471-test-tree
          :manifest manifest
          :feature (featurep 'ido-completing-read+)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'ido-completing-read+ package-alist)))))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(
        IDO_COMPLETING_READ_PLUS_MELPA_PIN,
        "ido-completing-read+.el",
    )
    .expect("prepare pinned ido-completing-read+ source below ./tmp")
    .with_melpa_dependency(MEMOIZE_MELPA_PIN)
    .expect("prepare pinned memoize dependency below ./tmp")
    .with_prelude(PRELUDE)
    .with_timeout(TEST_TIMEOUT)
}

fn ubiquitous_mode_swaps_completing_read_function() -> ParityBatchCase {
    ParityBatchCase::value(
        "ubiquitous_mode_swaps_completing_read_function",
        r####"
(let ((before completing-read-function)
      (mode-before ido-ubiquitous-mode))
  (unwind-protect
      (progn
        (ido-ubiquitous-mode 1)
        (let ((enabled completing-read-function))
          (ido-ubiquitous-mode -1)
          (list :source (icr471-test-source-state)
                :enabled enabled
                :disabled completing-read-function
                :fallback ido-cr+-fallback-function)))
    (ido-ubiquitous-mode (if mode-before 1 -1))
    (setq completing-read-function before)))
"####,
        expect![[
            r#"OK (:source (:tree "6e254f1248169e50ffc0351621d19e5c1be08214" :manifest (("ido-completing-read+-pkg.el" . "b4d3224bc34b3d01255582aeb2f0ae2772e1ab94f58ee0a61fbb0ba95cb41112") ("ido-completing-read+.el" . "b11038866b9c68b52fb24f178a78c8c3f7d8f2085a900e219c60ca36095300a1")) :feature t :version "20240130.30") :enabled ido-completing-read+ :disabled completing-read-default :fallback completing-read-default)"#
        ]],
    )
}

fn empty_and_oversized_collections_fall_back() -> ParityBatchCase {
    ParityBatchCase::value(
        "empty_and_oversized_collections_fall_back",
        r####"
(let ((ido-cr+-fallback-function
       (lambda (prompt collection &rest _)
         (list :fallback prompt :n (length collection))))
      (ido-cr+-max-items 2))
  (list :source (icr471-test-source-state)
        :empty (ido-completing-read+ "Empty: " nil)
        :large (ido-completing-read+ "Large: " '("a" "b" "café"))))
"####,
        expect![[
            r#"OK (:source (:tree "6e254f1248169e50ffc0351621d19e5c1be08214" :manifest (("ido-completing-read+-pkg.el" . "b4d3224bc34b3d01255582aeb2f0ae2772e1ab94f58ee0a61fbb0ba95cb41112") ("ido-completing-read+.el" . "b11038866b9c68b52fb24f178a78c8c3f7d8f2085a900e219c60ca36095300a1")) :feature t :version "20240130.30") :empty (:fallback "Empty: " :n 0) :large (:fallback "Large: " :n 3))"#
        ]],
    )
}

fn prefix_completions_union_dynamic_collection() -> ParityBatchCase {
    ParityBatchCase::value(
        "prefix_completions_union_dynamic_collection",
        r####"
(let* ((dynamic
        (lambda (string _pred _action)
          (cond
           ((string= string "") '("aa" "bb"))
           ((string-prefix-p "a" string) '("aa" "abc" "café"))
           (t nil))))
       (ido-cr+-all-completions-memoized #'all-completions))
  (list :source (icr471-test-source-state)
        :static (ido-cr+-all-prefix-completions "x" '("aa" "ab" "bb"))
        :dynamic (ido-cr+-all-prefix-completions "a" dynamic)))
"####,
        expect![[
            r#"OK (:source (:tree "6e254f1248169e50ffc0351621d19e5c1be08214" :manifest (("ido-completing-read+-pkg.el" . "b4d3224bc34b3d01255582aeb2f0ae2772e1ab94f58ee0a61fbb0ba95cb41112") ("ido-completing-read+.el" . "b11038866b9c68b52fb24f178a78c8c3f7d8f2085a900e219c60ca36095300a1")) :feature t :version "20240130.30") :static ("aa" "ab" "bb") :dynamic ("aa" "bb" "abc" "café"))"#
        ]],
    )
}

fn apply_restrictions_includes_and_excludes_matches() -> ParityBatchCase {
    ParityBatchCase::value(
        "apply_restrictions_includes_and_excludes_matches",
        r####"
(ido-mode 1)
(list :source (icr471-test-source-state)
      :keep (ido-cr+-apply-restrictions
             '("apple" "apricot" "banana" "café")
             '((nil . "ap")))
      :drop (ido-cr+-apply-restrictions
             '("apple" "apricot" "banana" "café")
             '((nil . "a") (t . "p"))))
"####,
        expect![[
            r#"OK (:source (:tree "6e254f1248169e50ffc0351621d19e5c1be08214" :manifest (("ido-completing-read+-pkg.el" . "b4d3224bc34b3d01255582aeb2f0ae2772e1ab94f58ee0a61fbb0ba95cb41112") ("ido-completing-read+.el" . "b11038866b9c68b52fb24f178a78c8c3f7d8f2085a900e219c60ca36095300a1")) :feature t :version "20240130.30") :keep ("apple" "apricot") :drop ("banana" "café"))"#
        ]],
    )
}

#[test]
fn ido_completing_read_plus_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        ubiquitous_mode_swaps_completing_read_function(),
        empty_and_oversized_collections_fall_back(),
        prefix_completions_union_dynamic_collection(),
        apply_restrictions_includes_and_excludes_matches(),
    ];
    assert_oracle_batch_cases(
        oracle(),
        "ido-completing-read-plus-rank471",
        "ido_completing_read_plus_parity",
        &cases,
    );
}
