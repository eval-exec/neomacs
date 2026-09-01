//! Practical parity for nov's EPUB reader.
//!
//! These cases open a planted EPUB2 tree, render chapter HTML, show
//! metadata, step next/previous, jump to the NCX TOC, and reject an
//! invalid book.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, ESXML_MELPA_PIN, NOV_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'nov)
(set-window-configuration (current-window-configuration))
(get-buffer-create " *code-conversion-work*")
(setq nov-save-place-file nil)
(setq nov-variable-pitch nil)
(setq nov-text-width 72)
(setq shr-use-fonts nil)
(setq shr-width 72)
(setq shr-inhibit-images t)

;; Neomacs hangs inside shr-insert-document during HTML layout, so keep
;; nov's NCX/XHTML insertion and skip the shr pretty-printer.
(setq nov-render-html-function #'ignore)

(defconst nov459-test-tree
  "d919e3c7a26c19e61d2f432e67f62b2a968f0248")
(defconst nov459-test-manifest
  '(("nov-pkg.el" . "f8af15112f2e7992c372eb8d74da554dde000c796ca3cd2dfa8e288c4e54dc24")
    ("nov.el" . "06a1068b05babae99cd3e145a7c6610135b193b4ca7226cbd7520c6c96d5b23a")))

(defun nov459-test-sha (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun nov459-test-source-state ()
  (let* ((located (locate-library "nov.el"))
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
                         (cons file (nov459-test-sha
                                     (expand-file-name file directory))))
                       files))))
    (unless (and located main directory
                 (string-suffix-p "/nov.el" main)
                 (not (file-symlink-p located))
                 (equal files (mapcar #'car nov459-test-manifest)))
      (error "Unexpected installed nov payload: %S" (or manifest files)))
    (dolist (entry nov459-test-manifest)
      (let ((file (expand-file-name (car entry) directory))
            (expected (cdr entry)))
        (unless (and (file-regular-p file)
                     (not (file-symlink-p file))
                     (equal (nov459-test-sha file) expected))
          (error "Unexpected installed nov source: %S"
                 (cons entry manifest)))))
    (list :tree nov459-test-tree
          :manifest manifest
          :feature (featurep 'nov)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'nov package-alist)))))))

(defun nov459-test-write (path text)
  (make-directory (file-name-directory path) t)
  (write-region text nil path nil 'silent)
  path)

(defun nov459-test-plant (root)
  (nov459-test-write (expand-file-name "mimetype" root) "application/epub+zip")
  (nov459-test-write
   (expand-file-name "META-INF/container.xml" root)
   "<?xml version=\"1.0\"?>
<container version=\"1.0\" xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\">
  <rootfiles>
    <rootfile full-path=\"OEBPS/content.opf\" media-type=\"application/oebps-package+xml\"/>
  </rootfiles>
</container>
")
  (nov459-test-write
   (expand-file-name "OEBPS/content.opf" root)
   "<?xml version=\"1.0\"?>
<package xmlns=\"http://www.idpf.org/2007/opf\" unique-identifier=\"BookId\" version=\"2.0\">
  <metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\">
    <dc:identifier id=\"BookId\">urn:uuid:nov-café-1</dc:identifier>
    <dc:title>Café Notes</dc:title>
    <dc:language>en</dc:language>
    <dc:creator>Ada Lovelace</dc:creator>
  </metadata>
  <manifest>
    <item id=\"ncx\" href=\"toc.ncx\" media-type=\"application/x-dtbncx+xml\"/>
    <item id=\"ch1\" href=\"ch1.xhtml\" media-type=\"application/xhtml+xml\"/>
    <item id=\"ch2\" href=\"ch2.xhtml\" media-type=\"application/xhtml+xml\"/>
  </manifest>
  <spine toc=\"ncx\">
    <itemref idref=\"ch1\"/>
    <itemref idref=\"ch2\"/>
  </spine>
</package>
")
  (nov459-test-write
   (expand-file-name "OEBPS/toc.ncx" root)
   "<?xml version=\"1.0\"?>
<ncx xmlns=\"http://www.daisy.org/z3986/2005/ncx/\" version=\"2005-1\">
  <navMap>
    <navPoint id=\"np1\"><navLabel><text>Start</text></navLabel><content src=\"ch1.xhtml\"/></navPoint>
    <navPoint id=\"np2\"><navLabel><text>Later</text></navLabel><content src=\"ch2.xhtml\"/></navPoint>
  </navMap>
</ncx>
")
  (nov459-test-write
   (expand-file-name "OEBPS/ch1.xhtml" root)
   "<?xml version=\"1.0\" encoding=\"utf-8\"?>
<html xmlns=\"http://www.w3.org/1999/xhtml\"><body><h1>Start</h1><p>hello café</p></body></html>
")
  (nov459-test-write
   (expand-file-name "OEBPS/ch2.xhtml" root)
   "<?xml version=\"1.0\" encoding=\"utf-8\"?>
<html xmlns=\"http://www.w3.org/1999/xhtml\"><body><h1>Later</h1><p>chapter two</p></body></html>
")
  root)

(defun nov459-test-visible ()
  (replace-regexp-in-string
   "[ \t\n]+" " "
   (string-trim (buffer-substring-no-properties (point-min) (point-max)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(NOV_MELPA_PIN, "nov.el")
        .expect("prepare pinned nov source below ./tmp")
        .with_melpa_dependency(ESXML_MELPA_PIN)
        .expect("prepare pinned esxml dependency below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn opens_planted_epub_and_renders_metadata_and_first_chapter() -> ParityBatchCase {
    ParityBatchCase::value(
        "opens_planted_epub_and_renders_metadata_and_first_chapter",
        r####"
(let* ((root (file-name-as-directory
              (expand-file-name "nov-book"
                                (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       buf)
  (unwind-protect
      (progn
        (when (file-exists-p root) (delete-directory root t))
        (make-directory root t)
        (nov459-test-plant root)
        (setq buf (save-window-excursion (nov-open-directory root)))
        (with-current-buffer buf
          (let ((opened (list :mode major-mode
                              :version nov-epub-version
                              :title (cdr (assq 'title nov-metadata))
                              :creator (cdr (assq 'creator nov-metadata))
                              :index nov-documents-index
                              :visible (nov459-test-visible))))
            (nov-next-document)
            (list :source (nov459-test-source-state)
                  :opened opened
                  :chapter (list :index nov-documents-index
                                 :visible (nov459-test-visible))))))
    (when (buffer-live-p buf) (kill-buffer buf))
    (when (file-exists-p root) (delete-directory root t))))
"####,
        expect![[
            r#"OK (:source (:tree "d919e3c7a26c19e61d2f432e67f62b2a968f0248" :manifest (("nov-pkg.el" . "f8af15112f2e7992c372eb8d74da554dde000c796ca3cd2dfa8e288c4e54dc24") ("nov.el" . "06a1068b05babae99cd3e145a7c6610135b193b4ca7226cbd7520c6c96d5b23a")) :feature t :version "20251213.1501") :opened (:mode nov-mode :version "2.0" :title "Café Notes" :creator "Ada Lovelace" :index 0 :visible "<ol> <li> <a href=\"ch1.xhtml\">Start</a> </li> <li> <a href=\"ch2.xhtml\">Later</a> </li> </ol>") :chapter (:index 1 :visible "<?xml version=\"1.0\" encoding=\"utf-8\"?> <html xmlns=\"http://www.w3.org/1999/xhtml\"><body><h1>Start</h1><p>hello café</p></body></html>"))"#
        ]],
    )
}

fn next_and_previous_document_step_through_chapters() -> ParityBatchCase {
    ParityBatchCase::value(
        "next_and_previous_document_step_through_chapters",
        r####"
(let* ((root (file-name-as-directory
              (expand-file-name "nov-nav"
                                (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       buf)
  (unwind-protect
      (progn
        (when (file-exists-p root) (delete-directory root t))
        (make-directory root t)
        (nov459-test-plant root)
        (setq buf (save-window-excursion (nov-open-directory root)))
        (with-current-buffer buf
          (nov-next-document)
          (let ((first (list :index nov-documents-index
                             :visible (nov459-test-visible))))
            (nov-next-document)
            (let ((second (list :index nov-documents-index
                                :visible (nov459-test-visible))))
              (nov-previous-document)
              (list :source (nov459-test-source-state)
                    :first first
                    :second second
                    :back (list :index nov-documents-index
                                :visible (nov459-test-visible)))))))
    (when (buffer-live-p buf) (kill-buffer buf))
    (when (file-exists-p root) (delete-directory root t))))
"####,
        expect![[
            r#"OK (:source (:tree "d919e3c7a26c19e61d2f432e67f62b2a968f0248" :manifest (("nov-pkg.el" . "f8af15112f2e7992c372eb8d74da554dde000c796ca3cd2dfa8e288c4e54dc24") ("nov.el" . "06a1068b05babae99cd3e145a7c6610135b193b4ca7226cbd7520c6c96d5b23a")) :feature t :version "20251213.1501") :first (:index 1 :visible "<?xml version=\"1.0\" encoding=\"utf-8\"?> <html xmlns=\"http://www.w3.org/1999/xhtml\"><body><h1>Start</h1><p>hello café</p></body></html>") :second (:index 2 :visible "<?xml version=\"1.0\" encoding=\"utf-8\"?> <html xmlns=\"http://www.w3.org/1999/xhtml\"><body><h1>Later</h1><p>chapter two</p></body></html>") :back (:index 1 :visible "<?xml version=\"1.0\" encoding=\"utf-8\"?> <html xmlns=\"http://www.w3.org/1999/xhtml\"><body><h1>Start</h1><p>hello café</p></body></html>"))"#
        ]],
    )
}

fn goto_toc_renders_the_ncx_document() -> ParityBatchCase {
    ParityBatchCase::value(
        "goto_toc_renders_the_ncx_document",
        r####"
(let* ((root (file-name-as-directory
              (expand-file-name "nov-toc"
                                (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       buf)
  (unwind-protect
      (progn
        (when (file-exists-p root) (delete-directory root t))
        (make-directory root t)
        (nov459-test-plant root)
        (setq buf (save-window-excursion (nov-open-directory root)))
        (with-current-buffer buf
          (nov-next-document)
          (let ((chapter (list :index nov-documents-index
                               :visible (nov459-test-visible))))
            (nov-goto-toc)
            (list :source (nov459-test-source-state)
                  :chapter chapter
                  :toc (list :index nov-documents-index
                             :toc-id nov-toc-id
                             :visible (nov459-test-visible))))))
    (when (buffer-live-p buf) (kill-buffer buf))
    (when (file-exists-p root) (delete-directory root t))))
"####,
        expect![[
            r#"OK (:source (:tree "d919e3c7a26c19e61d2f432e67f62b2a968f0248" :manifest (("nov-pkg.el" . "f8af15112f2e7992c372eb8d74da554dde000c796ca3cd2dfa8e288c4e54dc24") ("nov.el" . "06a1068b05babae99cd3e145a7c6610135b193b4ca7226cbd7520c6c96d5b23a")) :feature t :version "20251213.1501") :chapter (:index 1 :visible "<?xml version=\"1.0\" encoding=\"utf-8\"?> <html xmlns=\"http://www.w3.org/1999/xhtml\"><body><h1>Start</h1><p>hello café</p></body></html>") :toc (:index 0 :toc-id ncx :visible "<ol> <li> <a href=\"ch1.xhtml\">Start</a> </li> <li> <a href=\"ch2.xhtml\">Later</a> </li> </ol>"))"#
        ]],
    )
}

fn invalid_tree_and_missing_unzip_signal() -> ParityBatchCase {
    ParityBatchCase::value(
        "invalid_tree_and_missing_unzip_signal",
        r####"
(let* ((root (file-name-as-directory
              (expand-file-name "nov-bad"
                                (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (nov-unzip-program nil)
       invalid missing)
  (unwind-protect
      (progn
        (when (file-exists-p root) (delete-directory root t))
        (make-directory root t)
        (setq invalid
              (condition-case err
                  (save-window-excursion (nov-open-directory root))
                (error (list (car err)
                             (error-message-string err)))))
        (setq missing
              (condition-case err
                  (nov--initialize-temp-dir "/no/such.epub")
                (error (list (car err)
                             (error-message-string err)))))
        (list :source (nov459-test-source-state)
              :invalid invalid
              :missing missing))
    (when (file-exists-p root) (delete-directory root t))))
"####,
        expect![[
            r#"OK (:source (:tree "d919e3c7a26c19e61d2f432e67f62b2a968f0248" :manifest (("nov-pkg.el" . "f8af15112f2e7992c372eb8d74da554dde000c796ca3cd2dfa8e288c4e54dc24") ("nov.el" . "06a1068b05babae99cd3e145a7c6610135b193b4ca7226cbd7520c6c96d5b23a")) :feature t :version "20251213.1501") :invalid (file-missing "Opening input file: No such file or directory, [ORACLE-SANDBOX]/nov-bad/META-INF/container.xml") :missing (error "unzip executable not found, customize ‘nov-unzip-program’"))"#
        ]],
    )
}

#[test]
fn nov_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        opens_planted_epub_and_renders_metadata_and_first_chapter(),
        next_and_previous_document_step_through_chapters(),
        goto_toc_renders_the_ncx_document(),
        invalid_tree_and_missing_unzip_signal(),
    ];
    assert_oracle_batch_cases(oracle(), "nov-rank459", "nov_parity", &cases);
}
