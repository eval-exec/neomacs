//! Practical parity for biblio-core formatting and lookup.
//!
//! These cases clean DOIs and join fields, format a BibTeX entry, render
//! a search result, and start a lookup through a planted backend with
//! URL retrieval stubbed.

use std::time::Duration;

use expect_test::expect;

use crate::{BIBLIO_CORE_MELPA_PIN, CachedMelpaOracle, DASH_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'biblio-core)
(set-window-configuration (current-window-configuration))
(get-buffer-create " *code-conversion-work*")

(defconst bc470-test-tree
  "c93fde07ba095f230344afa15c59e22887008aff")
(defconst bc470-test-manifest
  '(("biblio-core-pkg.el" . "7004fbfc66756bc900349cbeca1d7b1ce994ce5abc3a5de1435d058812333ea2")
    ("biblio-core.el" . "4f4327127c3cef7064a30ec77f360bdef825246dd0532bbb36804b0f80c2b462")))

(defun bc470-test-sha (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun bc470-test-source-state ()
  (let* ((located (locate-library "biblio-core.el"))
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
                         (cons file (bc470-test-sha
                                     (expand-file-name file directory))))
                       files))))
    (unless (and located main directory
                 (string-suffix-p "/biblio-core.el" main)
                 (not (file-symlink-p located))
                 (equal files (mapcar #'car bc470-test-manifest)))
      (error "Unexpected installed biblio-core payload: %S"
             (or manifest files)))
    (dolist (entry bc470-test-manifest)
      (let ((file (expand-file-name (car entry) directory))
            (expected (cdr entry)))
        (unless (and (file-regular-p file)
                     (not (file-symlink-p file))
                     (equal (bc470-test-sha file) expected))
          (error "Unexpected installed biblio-core source: %S"
                 (cons entry manifest)))))
    (list :tree bc470-test-tree
          :manifest manifest
          :feature (featurep 'biblio-core)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'biblio-core package-alist)))))))

(defun bc470-test-visible ()
  (replace-regexp-in-string
   "[ \t\n]+" " "
   (string-trim (buffer-substring-no-properties (point-min) (point-max)))))

(defun bc470-test-backend (command &optional arg)
  (pcase command
    ('name "TestLib")
    ('prompt "Query: ")
    ('url (concat "https://example.test/q=" arg))
    (_ nil)))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(BIBLIO_CORE_MELPA_PIN, "biblio-core.el")
        .expect("prepare pinned biblio-core source below ./tmp")
        .with_melpa_dependency(DASH_MELPA_PIN)
        .expect("prepare pinned dash dependency below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn strip_cleanup_doi_and_join_fields() -> ParityBatchCase {
    ParityBatchCase::value(
        "strip_cleanup_doi_and_join_fields",
        r####"
(list :source (bc470-test-source-state)
      :strip (biblio-strip "  café  \n")
      :doi (biblio-cleanup-doi "https://dx.doi.org/10.1000/café")
      :join (biblio-join ", " "Ada" "" "Lovelace")
      :empty (biblio-parenthesize "")
      :paren (biblio-parenthesize "notes")
      :alist (biblio-alist-get 'title '((title . "Café") (year . "1843"))))
"####,
        expect![[
            r#"OK (:source (:tree "c93fde07ba095f230344afa15c59e22887008aff" :manifest (("biblio-core-pkg.el" . "7004fbfc66756bc900349cbeca1d7b1ce994ce5abc3a5de1435d058812333ea2") ("biblio-core.el" . "4f4327127c3cef7064a30ec77f360bdef825246dd0532bbb36804b0f80c2b462")) :feature t :version "20230202.1721") :strip "café" :doi "10.1000/café" :join "Ada, Lovelace" :empty "" :paren "(notes)" :alist "Café")"#
        ]],
    )
}

fn format_bibtex_aligns_a_planted_entry() -> ParityBatchCase {
    ParityBatchCase::value(
        "format_bibtex_aligns_a_planted_entry",
        r####"
(list :source (bc470-test-source-state)
      :formatted
      (biblio-format-bibtex
       "@article{x,
  title={Café Notes},
  author={Ada Lovelace},
  year={1843}
}"))
"####,
        expect![[
            r#"OK (:source (:tree "c93fde07ba095f230344afa15c59e22887008aff" :manifest (("biblio-core-pkg.el" . "7004fbfc66756bc900349cbeca1d7b1ce994ce5abc3a5de1435d058812333ea2") ("biblio-core.el" . "4f4327127c3cef7064a30ec77f360bdef825246dd0532bbb36804b0f80c2b462")) :feature t :version "20230202.1721") :formatted "@Article{x,\n  author       = {Ada Lovelace},\n  title\11       = {Café Notes},\n  year\11       = 1843\n}")"#
        ]],
    )
}

fn insert_result_renders_title_authors_and_year() -> ParityBatchCase {
    ParityBatchCase::value(
        "insert_result_renders_title_authors_and_year",
        r####"
(with-temp-buffer
  (biblio-insert-result
   '((title . "Café Notes")
     (authors . ("Ada Lovelace" "Charles Babbage"))
     (year . "1843")
     (container . "Notes")
     (url . "https://example.test/café")))
  (list :source (bc470-test-source-state)
        :visible (bc470-test-visible)
        :meta (biblio-alist-get 'title (get-text-property (point-min) 'biblio-metadata))))
"####,
        expect![[
            r#"OK (:source (:tree "c93fde07ba095f230344afa15c59e22887008aff" :manifest (("biblio-core-pkg.el" . "7004fbfc66756bc900349cbeca1d7b1ce994ce5abc3a5de1435d058812333ea2") ("biblio-core.el" . "4f4327127c3cef7064a30ec77f360bdef825246dd0532bbb36804b0f80c2b462")) :feature t :version "20230202.1721") :visible "> Café Notes [1843] Ada Lovelace, Charles Babbage In: Notes URL: https://example.test/café" :meta "Café Notes")"#
        ]],
    )
}

fn lookup_uses_backend_url_with_retrieve_stubbed() -> ParityBatchCase {
    ParityBatchCase::value(
        "lookup_uses_backend_url_with_retrieve_stubbed",
        r####"
(let (captured buf)
  (unwind-protect
      (cl-letf (((symbol-function 'biblio-url-retrieve)
                 (lambda (url _callback)
                   (setq captured url))))
        (setq buf (biblio-lookup #'bc470-test-backend "café notes"))
        (with-current-buffer buf
          (list :source (bc470-test-source-state)
                :buffer (buffer-name)
                :mode major-mode
                :terms biblio--search-terms
                :url captured
                :header (bc470-test-visible))))
    (when (buffer-live-p buf) (kill-buffer buf))))
"####,
        expect![[
            r#"OK (:source (:tree "c93fde07ba095f230344afa15c59e22887008aff" :manifest (("biblio-core-pkg.el" . "7004fbfc66756bc900349cbeca1d7b1ce994ce5abc3a5de1435d058812333ea2") ("biblio-core.el" . "4f4327127c3cef7064a30ec77f360bdef825246dd0532bbb36804b0f80c2b462")) :feature t :version "20230202.1721") :buffer "*TestLib search*" :mode biblio-selection-mode :terms "café notes" :url "https://example.test/q=café notes" :header "TestLib search results for ‘café notes’ (loading…)")"#
        ]],
    )
}

#[test]
fn biblio_core_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        strip_cleanup_doi_and_join_fields(),
        format_bibtex_aligns_a_planted_entry(),
        insert_result_renders_title_authors_and_year(),
        lookup_uses_backend_url_with_retrieve_stubbed(),
    ];
    assert_oracle_batch_cases(
        oracle(),
        "biblio-core-rank470",
        "biblio_core_parity",
        &cases,
    );
}
