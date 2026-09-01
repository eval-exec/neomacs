use std::time::Duration;

use crate::{CachedMelpaOracle, GH_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const GH_TEST_TIMEOUT: Duration = Duration::from_secs(240);

const GH_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'json)
(require 'url-http)
(require 'gh)
(require 'gh-repos)
(require 'gh-search)

(defvar neomacs-gh-test-root
  (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
(defvar neomacs-gh-test-responses nil)
(defvar neomacs-gh-test-requests nil)

(defun neomacs-gh-test-observe-string (value)
  "Return an independent UTF-8-decoded copy of string VALUE."
  (when value
    (copy-sequence
     (if (multibyte-string-p value)
         value
       (decode-coding-string value 'utf-8)))))

(defun neomacs-gh-test-normalize-headers (headers)
  "Return HEADERS in deterministic name order with decoded values."
  (sort
   (mapcar
    (lambda (header)
      (cons (copy-sequence (car header))
            (neomacs-gh-test-observe-string (cdr header))))
    headers)
   (lambda (left right) (string< (car left) (car right)))))

(defun neomacs-gh-test-response-buffer (spec)
  "Build a real url.el-shaped response buffer from SPEC."
  (let* ((status (plist-get spec :status))
         (headers (plist-get spec :headers))
         (body (or (plist-get spec :body) ""))
         (buffer (generate-new-buffer " *gh-parity-response*")))
    (with-current-buffer buffer
      ;; Match url-http's wire boundary.  gh-url-set-response is responsible
      ;; for converting this unibyte UTF-8 response before JSON decoding.
      (set-buffer-multibyte nil)
      (insert (encode-coding-string
               (format "HTTP/1.1 %s Fixture\n" status) 'utf-8))
      (dolist (header headers)
        (insert (encode-coding-string
                 (format "%s: %s\n" (car header) (cdr header)) 'utf-8)))
      (insert "\n")
      (setq-local url-http-end-of-headers (1- (point)))
      (insert (encode-coding-string body 'utf-8))
      (goto-char (point-min)))
    buffer))

(defun neomacs-gh-test-url-retrieve-synchronously (url &rest _ignored)
  "Record one real gh.el request and return its scripted HTTP response."
  (unless neomacs-gh-test-responses
    (error "Unexpected gh.el request: %s %s" url-request-method url))
  (let ((spec (pop neomacs-gh-test-responses)))
    (push
     (list :url (neomacs-gh-test-observe-string url)
           :method (neomacs-gh-test-observe-string url-request-method)
           :headers
           (neomacs-gh-test-normalize-headers url-request-extra-headers)
           :data (neomacs-gh-test-observe-string url-request-data))
     neomacs-gh-test-requests)
    (when-let* ((expected-url (plist-get spec :expect-url)))
      (unless (equal url expected-url)
        (error "gh.el requested %S, fixture requires %S" url expected-url)))
    (when-let* ((expected-method (plist-get spec :expect-method)))
      (unless (equal url-request-method expected-method)
        (error "gh.el used %S, fixture requires %S"
               url-request-method expected-method)))
    (neomacs-gh-test-response-buffer spec)))

(defmacro neomacs-gh-test-with-sandbox (name responses &rest body)
  "Run BODY with isolated gh.el state and scripted RESPONSES."
  (declare (indent 2) (debug (form form body)))
  `(let* ((case-root
          (file-name-as-directory
            (expand-file-name ,name neomacs-gh-test-root)))
          (buffers-before (buffer-list))
          (pcache-directory (expand-file-name "pcache/" case-root))
          (*pcache-repositories* (make-hash-table :test 'equal))
          (gh-auth-alist nil)
          (gh-profile-current-profile "github")
          (gh-profile-default-profile "github")
          (gh-profile-alist
           '(("github" :url "https://api.github.test")
             ("enterprise" :url "https://ghe.example/api/v3")))
          (neomacs-gh-test-responses ,responses)
          (neomacs-gh-test-requests nil))
     (when (file-directory-p case-root)
       (delete-directory case-root t))
     (make-directory pcache-directory t)
     (unwind-protect
         (cl-letf
             (((symbol-function 'url-retrieve-synchronously)
               #'neomacs-gh-test-url-retrieve-synchronously))
           (prog1 (progn ,@body)
             (when neomacs-gh-test-responses
               (error "Unused gh.el fixture responses: %s"
                      (length neomacs-gh-test-responses)))))
       (dolist (buffer (buffer-list))
         (when (and (not (memq buffer buffers-before))
                    (buffer-live-p buffer))
           (with-current-buffer buffer (set-buffer-modified-p nil))
           (kill-buffer buffer)))
       (when (file-directory-p case-root)
         (delete-directory case-root t)))))
"####;

fn gh_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(GH_MELPA_PIN, "gh.el")
        .expect("prepare exact shallow gh.el source graph below ./tmp")
        .with_prelude(GH_TEST_PRELUDE)
        .with_timeout(GH_TEST_TIMEOUT)
}

fn assert_gh_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(gh_oracle(), "gh-package-batch", "gh.el", cases);
}

#[test]
fn gh_package_batch() {
    assert_gh_batch(&workflows::workflow_batch_cases());
}
