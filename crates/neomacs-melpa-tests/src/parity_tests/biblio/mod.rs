//! Practical parity for biblio / biblio-core.  The package queries
//! bibliographic backends (crossref here) and renders the parsed results
//! into a tabulated buffer, from which entries can be inserted as
//! BibTeX into the source buffer.  The HTTP layer is stubbed with a
//! recorded real crossref API response; the package runs its real
//! parsing, rendering, and selection-insert flows.

use std::time::Duration;

use crate::{BIBLIO_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const TEST_PRELUDE: &str = r####"(require 'cl-lib)
(require 'package)

(setq make-backup-files nil create-lockfiles nil)

(defvar biblio--test-root
  (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

;; Provenance: pinned upstream bb9d6b4b962fb2a4e965d27888268b66d868766b.
(defconst biblio--test-upstream-tree
  "2e5baf3f77b588608f57b10a590ae213b645faf0"
  "Git tree of the pinned upstream commit this suite installs.")

(defconst biblio--test-manifest
  '(("biblio-core.el" . "4f4327127c3cef7064a30ec77f360bdef825246dd0532bbb36804b0f80c2b462")
    ("biblio-crossref.el" . "f088028e23f79db1fa1c1dec53d73a4f304f05f7744b65adbaf38eb25f15f7f8"))
  "Per-file sha256 of the package-built sources the suite verifies.")

(defconst biblio--test-crossref-json-b64
  "eyJzdGF0dXMiOiJvayIsIm1lc3NhZ2UtdHlwZSI6IndvcmstbGlzdCIsIm1lc3NhZ2UtdmVyc2lvbiI6IjEuMC4wIiwibWVzc2FnZSI6eyJmYWNldHMiOnt9LCJ0b3RhbC1yZXN1bHRzIjoyNTgzNTk2LCJpdGVtcyI6W3siaW5kZXhlZCI6eyJkYXRlLXBhcnRzIjpbWzIwMjQsOSw0XV0sImRhdGUtdGltZSI6IjIwMjQtMDktMDRUMTM6NTE6MTNaIiwidGltZXN0YW1wIjoxNzI1NDU3ODczNzM1fSwicHVibGlzaGVyLWxvY2F0aW9uIjoiQm9zdG9uIiwicmVmZXJlbmNlLWNvdW50IjowLCJwdWJsaXNoZXIiOiJLbHV3ZXIgQWNhZGVtaWMgUHVibGlzaGVycyIsImlzYm4tdHlwZSI6W3sidHlwZSI6InByaW50IiwidmFsdWUiOiIwNzkyMzc2NDQ3In1dLCJjb250ZW50LWRvbWFpbiI6eyJkb21haW4iOltdLCJjcm9zc21hcmstcmVzdHJpY3Rpb24iOmZhbHNlfSwiRE9JIjoiMTAuMTAwN1wvMC0zMDYtNDc1MDctM184IiwidHlwZSI6ImJvb2stY2hhcHRlciIsImNyZWF0ZWQiOnsiZGF0ZS1wYXJ0cyI6W1syMDA1LDEyLDIwXV0sImRhdGUtdGltZSI6IjIwMDUtMTItMjBUMTE6NTg6MzVaIiwidGltZXN0YW1wIjoxMTM1MDc5OTE1MDAwfSwicGFnZSI6IjE1MS0xNzMiLCJzb3VyY2UiOiJDcm9zc3JlZiIsImlzLXJlZmVyZW5jZWQtYnktY291bnQiOjAsInRpdGxlIjpbIkRlc2lnbiBmb3IgVGVzdCJdLCJwcmVmaXgiOiIxMC4xMDA3IiwibWVtYmVyIjoiMjk3IiwiY29udGFpbmVyLXRpdGxlIjpbIkFkdmFuY2VkIEFTSUMgQ2hpcCBTeW50aGVzaXMgVXNpbmcgU3lub3BzeXNcdTAwYWUgRGVzaWduIENvbXBpbGVyXHUyMTIyIFBoeXNpY2FsIENvbXBpbGVyXHUyMTIyIGFuZCBQcmltZVRpbWVcdTAwYWUiXSwibGFuZ3VhZ2UiOiJlbiIsImxpbmsiOlt7IlVSTCI6Imh0dHA6XC9cL2xpbmsuc3ByaW5nZXIuY29tXC9jb250ZW50XC9wZGZcLzEwLjEwMDdcLzAtMzA2LTQ3NTA3LTNfOC5wZGYiLCJjb250ZW50LXR5cGUiOiJ1bnNwZWNpZmllZCIsImNvbnRlbnQtdmVyc2lvbiI6InZvciIsImludGVuZGVkLWFwcGxpY2F0aW9uIjoic2ltaWxhcml0eS1jaGVja2luZyJ9XSwiZGVwb3NpdGVkIjp7ImRhdGUtcGFydHMiOltbMjAyMSw0LDI3XV0sImRhdGUtdGltZSI6IjIwMjEtMDQtMjdUMDQ6MzI6MDBaIiwidGltZXN0YW1wIjoxNjE5NDk3OTIwMDAwfSwic2NvcmUiOjIyLjc2MTY1OCwicmVzb3VyY2UiOnsicHJpbWFyeSI6eyJVUkwiOiJodHRwOlwvXC9saW5rLnNwcmluZ2VyLmNvbVwvMTAuMTAwN1wvMC0zMDYtNDc1MDctM184In19LCJpc3N1ZWQiOnsiZGF0ZS1wYXJ0cyI6W1tudWxsXV19LCJJU0JOIjpbIjA3OTIzNzY0NDciXSwicmVmZXJlbmNlcy1jb3VudCI6MCwiVVJMIjoiaHR0cHM6XC9cL2RvaS5vcmdcLzEwLjEwMDdcLzAtMzA2LTQ3NTA3LTNfOCJ9LHsiaW5kZXhlZCI6eyJkYXRlLXBhcnRzIjpbWzIwMjQsOSw0XV0sImRhdGUtdGltZSI6IjIwMjQtMDktMDRUMTM6NTE6MTNaIiwidGltZXN0YW1wIjoxNzI1NDU3ODczODE1fSwicHVibGlzaGVyLWxvY2F0aW9uIjoiQm9zdG9uIiwicmVmZXJlbmNlLWNvdW50IjowLCJwdWJsaXNoZXIiOiJLbHV3ZXIgQWNhZGVtaWMgUHVibGlzaGVycyIsImlzYm4tdHlwZSI6W3sidHlwZSI6InByaW50IiwidmFsdWUiOiIwNzkyMzc2NDQ3In1dLCJjb250ZW50LWRvbWFpbiI6eyJkb21haW4iOltdLCJjcm9zc21hcmstcmVzdHJpY3Rpb24iOmZhbHNlfSwiRE9JIjoiMTAuMTAwN1wvMC0zMDYtNDc1MDctM18xIiwidHlwZSI6ImJvb2stY2hhcHRlciIsImNyZWF0ZWQiOnsiZGF0ZS1wYXJ0cyI6W1syMDA1LDEyLDIwXV0sImRhdGUtdGltZSI6IjIwMDUtMTItMjBUMTE6NTg6MzVaIiwidGltZXN0YW1wIjoxMTM1MDc5OTE1MDAwfSwicGFnZSI6IjEtMTciLCJzb3VyY2UiOiJDcm9zc3JlZiIsImlzLXJlZmVyZW5jZWQtYnktY291bnQiOjEsInRpdGxlIjpbIkFzaWMgRGVzaWduIE1ldGhvZG9sb2d5Il0sInByZWZpeCI6IjEwLjEwMDciLCJtZW1iZXIiOiIyOTciLCJjb250YWluZXItdGl0bGUiOlsiQWR2YW5jZWQgQVNJQyBDaGlwIFN5bnRoZXNpcyBVc2luZyBTeW5vcHN5c1x1MDBhZSBEZXNpZ24gQ29tcGlsZXJcdTIxMjIgUGh5c2ljYWwgQ29tcGlsZXJcdTIxMjIgYW5kIFByaW1lVGltZVx1MDBhZSJdLCJsYW5ndWFnZSI6ImVuIiwibGluayI6W3siVVJMIjoiaHR0cDpcL1wvbGluay5zcHJpbmdlci5jb21cL2NvbnRlbnRcL3BkZlwvMTAuMTAwN1wvMC0zMDYtNDc1MDctM18xLnBkZiIsImNvbnRlbnQtdHlwZSI6InVuc3BlY2lmaWVkIiwiY29udGVudC12ZXJzaW9uIjoidm9yIiwiaW50ZW5kZWQtYXBwbGljYXRpb24iOiJzaW1pbGFyaXR5LWNoZWNraW5nIn1dLCJkZXBvc2l0ZWQiOnsiZGF0ZS1wYXJ0cyI6W1syMDIxLDQsMjddXSwiZGF0ZS10aW1lIjoiMjAyMS0wNC0yN1QwNDozMTo1NVoiLCJ0aW1lc3RhbXAiOjE2MTk0OTc5MTUwMDB9LCJzY29yZSI6MjIuNjIzNDAyLCJyZXNvdXJjZSI6eyJwcmltYXJ5Ijp7IlVSTCI6Imh0dHA6XC9cL2xpbmsuc3ByaW5nZXIuY29tXC8xMC4xMDA3XC8wLTMwNi00NzUwNy0zXzEifX0sImlzc3VlZCI6eyJkYXRlLXBhcnRzIjpbW251bGxdXX0sIklTQk4iOlsiMDc5MjM3NjQ0NyJdLCJyZWZlcmVuY2VzLWNvdW50IjowLCJVUkwiOiJodHRwczpcL1wvZG9pLm9yZ1wvMTAuMTAwN1wvMC0zMDYtNDc1MDctM18xIn0seyJpbmRleGVkIjp7ImRhdGUtcGFydHMiOltbMjAyNSw5LDE3XV0sImRhdGUtdGltZSI6IjIwMjUtMDktMTdUMTU6NDM6NTdaIiwidGltZXN0YW1wIjoxNzU4MTIzODM3NzA4fSwicHVibGlzaGVyLWxvY2F0aW9uIjoiQm9zdG9uIiwicmVmZXJlbmNlLWNvdW50IjowLCJwdWJsaXNoZXIiOiJLbHV3ZXIgQWNhZGVtaWMgUHVibGlzaGVycyIsImlzYm4tdHlwZSI6W3sidHlwZSI6InByaW50IiwidmFsdWUiOiIwNzkyMzc2NDQ3In1dLCJsaWNlbnNlIjpbeyJzdGFydCI6eyJkYXRlLXBhcnRzIjpbWzIwMDIsMSwxXV0sImRhdGUtdGltZSI6IjIwMDItMDEtMDFUMDA6MDA6MDBaIiwidGltZXN0YW1wIjoxMDA5ODQzMjAwMDAwfSwiY29udGVudC12ZXJzaW9uIjoidGRtIiwiZGVsYXktaW4tZGF5cyI6MCwiVVJMIjoiaHR0cDpcL1wvd3d3LnNwcmluZ2VyLmNvbVwvdGRtIn1dLCJjb250ZW50LWRvbWFpbiI6eyJkb21haW4iOltdLCJjcm9zc21hcmstcmVzdHJpY3Rpb24iOmZhbHNlfSwicHVibGlzaGVkLXByaW50Ijp7ImRhdGUtcGFydHMiOltbMjAwMl1dfSwiRE9JIjoiMTAuMTAwN1wvYjExNzAyNCIsInR5cGUiOiJib29rIiwiY3JlYXRlZCI6eyJkYXRlLXBhcnRzIjpbWzIwMDUsMTIsMjBdXSwiZGF0ZS10aW1lIjoiMjAwNS0xMi0yMFQwNjo1ODozNVoiLCJ0aW1lc3RhbXAiOjExMzUwNjE5MTUwMDB9LCJzb3VyY2UiOiJDcm9zc3JlZiIsImlzLXJlZmVyZW5jZWQtYnktY291bnQiOjIsInRpdGxlIjpbIkFkdmFuY2VkIEFTSUMgQ2hpcCBTeW50aGVzaXMgVXNpbmcgU3lub3BzeXNcdTAwYWUgRGVzaWduIENvbXBpbGVyXHUyMTIyIFBoeXNpY2FsIENvbXBpbGVyXHUyMTIyIGFuZCBQcmltZVRpbWVcdTAwYWUiXSwicHJlZml4IjoiMTAuMTAwNyIsIm1lbWJlciI6IjI5NyIsImxhbmd1YWdlIjoiZW4iLCJsaW5rIjpbeyJVUkwiOiJodHRwOlwvXC9saW5rLnNwcmluZ2VyLmNvbVwvY29udGVudFwvcGRmXC8xMC4xMDA3XC9iMTE3MDI0LnBkZiIsImNvbnRlbnQtdHlwZSI6ImFwcGxpY2F0aW9uXC9wZGYiLCJjb250ZW50LXZlcnNpb24iOiJ2b3IiLCJpbnRlbmRlZC1hcHBsaWNhdGlvbiI6InRleHQtbWluaW5nIn0seyJVUkwiOiJodHRwOlwvXC9saW5rLnNwcmluZ2VyLmNvbVwvY29udGVudFwvcGRmXC8xMC4xMDA3XC9iMTE3MDI0IiwiY29udGVudC10eXBlIjoidW5zcGVjaWZpZWQiLCJjb250ZW50LXZlcnNpb24iOiJ2b3IiLCJpbnRlbmRlZC1hcHBsaWNhdGlvbiI6InNpbWlsYXJpdHktY2hlY2tpbmcifV0sImRlcG9zaXRlZCI6eyJkYXRlLXBhcnRzIjpbWzIwMTksNCw1XV0sImRhdGUtdGltZSI6IjIwMTktMDQtMDVUMjI6MTQ6MDVaIiwidGltZXN0YW1wIjoxNTU0NTAyNDQ1MDAwfSwic2NvcmUiOjIxLjk4MjI2NSwicmVzb3VyY2UiOnsicHJpbWFyeSI6eyJVUkwiOiJodHRwOlwvXC9saW5rLnNwcmluZ2VyLmNvbVwvMTAuMTAwN1wvYjExNzAyNCJ9fSwiaXNzdWVkIjp7ImRhdGUtcGFydHMiOltbMjAwMl1dfSwiSVNCTiI6WyIwNzkyMzc2NDQ3Il0sInJlZmVyZW5jZXMtY291bnQiOjAsIlVSTCI6Imh0dHBzOlwvXC9kb2kub3JnXC8xMC4xMDA3XC9iMTE3MDI0IiwicHVibGlzaGVkIjp7ImRhdGUtcGFydHMiOltbMjAwMl1dfX1dLCJpdGVtcy1wZXItcGFnZSI6MywicXVlcnkiOnsic3RhcnQtaW5kZXgiOjAsInNlYXJjaC10ZXJtcyI6ImNvbXBpbGVyIGRlc2lnbiJ9fX0="
  "Base64 of the recorded crossref API response (a real
`https://api.crossref.org/works?query=compiler design&rows=3'
payload, frozen so the HTTP layer can be stubbed).")

(defun biblio--test-source-state ()
  "Verify the installed payload is the pinned upstream build."
  (dolist (entry biblio--test-manifest)
    (let* ((located (locate-library (car entry)))
           (file (and located (file-truename located))))
      (unless (and file (file-regular-p file) (not (file-symlink-p file)))
        (error "Unexpected installed biblio location: %S" located))
      (with-temp-buffer
        (set-buffer-multibyte nil)
        (insert-file-contents-literally file)
        (unless (equal (secure-hash 'sha256 (current-buffer)) (cdr entry))
          (error "Unexpected installed biblio source: %S (got %S)"
                   (car entry)
                   (secure-hash 'sha256 (current-buffer)))))))
  (list :upstream-tree biblio--test-upstream-tree
        :feature (featurep 'biblio-core)
        :version (package-version-join
                  (package-desc-version
                   (cadr (assq 'biblio package-alist)))))))

(defun biblio--test-http-mock (url)
  "Return a buffer holding the recorded HTTP response for URL."
  (let ((buf (generate-new-buffer " *biblio-http*")))
    (with-current-buffer buf
      (insert "HTTP/1.1 200 OK\n")
      (insert "Content-Type: application/json; charset=utf-8\n\n")
      (insert (decode-coding-string
               (base64-decode-string biblio--test-crossref-json-b64)
               'utf-8))
      (goto-char (point-min)))
    buf))

(defvar biblio--test-messages nil)
(defvar biblio--test-reads nil)

(defmacro biblio--test-with-ui-capture (&rest body)
  "Run BODY with `message' captured and `completing-read' fed the
first option of the real collection it was offered."
  `(let ((biblio--test-messages nil)
         (biblio--test-reads nil))
     (cl-letf (((symbol-function 'message)
                (lambda (fmt &rest args)
                  (push (apply #'format-message fmt args)
                        biblio--test-messages)))
               ((symbol-function 'completing-read)
                (lambda (prompt collection &rest _)
                  (push (list :prompt prompt :options collection)
                        biblio--test-reads)
                  (car collection))))
       ,@body)))

(defun biblio--test-result (&rest plist)
  (append
   plist
   (list :messages (nreverse biblio--test-messages)
         :reads (nreverse biblio--test-reads))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(BIBLIO_MELPA_PIN, "biblio.el")
        .expect("prepare pinned biblio source below ./tmp")
        .with_prelude(TEST_PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

#[test]
fn biblio_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_oracle_batch_cases(oracle(), "biblio_package_batch", "biblio_parity", &cases);
}
