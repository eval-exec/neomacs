use std::time::Duration;

use crate::{CachedMelpaOracle, GH_MD_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const GH_MD_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const GH_MD_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'subr-x)
(require 'json)
(require 'gh-md)

(defvar neomacs-gh-md-test--http-buffers nil)

(defun neomacs-gh-md-test-cleanup ()
  "Reset preview buffers and fake HTTP buffers between cases."
  (when (get-buffer gh-md-buffer-name)
    (kill-buffer gh-md-buffer-name))
  (dolist (buffer neomacs-gh-md-test--http-buffers)
    (when (buffer-live-p buffer)
      (kill-buffer buffer)))
  (setq neomacs-gh-md-test--http-buffers nil))

(defun neomacs-gh-md-test-http-buffer (body)
  "Build a url.el-shaped response buffer with BODY after the headers."
  (let ((buffer (generate-new-buffer " *gh-md-parity-http*")))
    (push buffer neomacs-gh-md-test--http-buffers)
    (with-current-buffer buffer
      (setq-local url-http-response-status 200)
      (insert "HTTP/1.1 200 OK\nContent-Type: text/html; charset=utf-8\n\n")
      (setq url-http-end-of-headers (point-marker))
      (insert body)
      (goto-char (point-min)))
    buffer))

(defun neomacs-gh-md-test-decode-request (data)
  "Decode `url-request-data' as UTF-8 text when possible."
  (cond
   ((null data) nil)
   ((stringp data) (decode-coding-string data 'utf-8))
   (t data)))

(defun neomacs-gh-md-test-with-transport (response-body thunk &optional error-status)
  "Call THUNK while `url-retrieve' returns RESPONSE-BODY or ERROR-STATUS.
Return a plist of :result and :requests."
  (let (requests)
    (cl-letf (((symbol-function 'url-retrieve)
               (lambda (url callback &optional cbargs silent &rest _)
                 (push (list :url url
                             :method url-request-method
                             :data (neomacs-gh-md-test-decode-request
                                    url-request-data)
                             :silent silent)
                       requests)
                 (let ((buffer
                        (if error-status
                            (generate-new-buffer " *gh-md-parity-http-error*")
                          (neomacs-gh-md-test-http-buffer response-body))))
                   (when error-status
                     (push buffer neomacs-gh-md-test--http-buffers))
                   (with-current-buffer buffer
                     (apply callback
                            (if error-status error-status nil)
                            cbargs))
                   buffer)))
              ((symbol-function 'display-buffer)
               (lambda (buffer &rest _)
                 (get-buffer-window buffer t))))
      (list :result (funcall thunk)
            :requests (nreverse requests)))))

(defun neomacs-gh-md-test-view (&optional buffer)
  "Describe stable public state of BUFFER (default `*gh-md*')."
  (with-current-buffer (or buffer gh-md-buffer-name)
    (list :name (buffer-name)
          :mode major-mode
          :text (string-trim
                 (buffer-substring-no-properties (point-min) (point-max)))
          :point (point)
          :read-only buffer-read-only
          :file (and (buffer-file-name)
                     (file-relative-name
                      (buffer-file-name)
                      (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
          :modified (buffer-modified-p))))
"####;

fn gh_md_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(GH_MD_MELPA_PIN, "gh-md.el")
        .expect("prepare exact shallow gh-md source below ./tmp")
        .with_prelude(GH_MD_TEST_PRELUDE)
        .with_timeout(GH_MD_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed gh-md parity test")
        .into()
}

fn assert_gh_md_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(gh_md_oracle(), &current_test_name(), "gh_md_parity", cases);
}

#[test]
fn gh_md_package_batch() {
    assert_gh_md_batch(&workflows::workflow_batch_cases());
}
