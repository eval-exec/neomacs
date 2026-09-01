//! Practical parity for request-deferred wrapping request in deferred.
//!
//! These cases call `request-deferred` through a planted `request`
//! stand-in, deliver a response on the deferred chain, and keep a
//! user COMPLETE callback from replacing the package's.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, DEFERRED_MELPA_PIN, REQUEST_DEFERRED_MELPA_PIN, REQUEST_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'request)
(require 'deferred)
(require 'request-deferred)
(set-window-configuration (current-window-configuration))
(get-buffer-create " *code-conversion-work*")

(defconst rd482-test-tree
  "4e8ac232df3e852ce6ea3e10d4713fc45fac0386")
(defconst rd482-test-manifest
  '(("request-deferred-pkg.el" . "1f1048efb0944551d25c6757b2d669ef454d78b87be00d33aa4a97af74c2b44f")
    ("request-deferred.el" . "4df2a46a52386e4920a883669e5504fd1bce19951b849357336b6dd3b3fcf3eb")))

(defun rd482-test-sha (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun rd482-test-source-state ()
  (let* ((located (locate-library "request-deferred.el"))
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
                         (cons file (rd482-test-sha
                                     (expand-file-name file directory))))
                       files))))
    (unless (and located main directory
                 (string-suffix-p "/request-deferred.el" main)
                 (not (file-symlink-p located))
                 (equal files (mapcar #'car rd482-test-manifest)))
      (error "Unexpected installed request-deferred payload: %S"
             (or manifest files)))
    (dolist (entry rd482-test-manifest)
      (let ((file (expand-file-name (car entry) directory))
            (expected (cdr entry)))
        (unless (and (file-regular-p file)
                     (not (file-symlink-p file))
                     (equal (rd482-test-sha file) expected))
          (error "Unexpected installed request-deferred source: %S"
                 (cons entry manifest)))))
    (list :tree rd482-test-tree
          :manifest manifest
          :feature (featurep 'request-deferred)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'request-deferred package-alist)))))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(REQUEST_DEFERRED_MELPA_PIN, "request-deferred.el")
        .expect("prepare pinned request-deferred source below ./tmp")
        .with_melpa_dependency(DEFERRED_MELPA_PIN)
        .expect("prepare pinned deferred dependency below ./tmp")
        .with_melpa_dependency(REQUEST_MELPA_PIN)
        .expect("prepare pinned request dependency below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn request_deferred_posts_planted_response_on_the_chain() -> ParityBatchCase {
    ParityBatchCase::value(
        "request_deferred_posts_planted_response_on_the_chain",
        r####"
(let* ((planted '(:status-code 200 :data "café"))
       captured
       (deferred-debug nil))
  (cl-letf (((symbol-function 'request)
             (lambda (url &rest args)
               (setq captured (list :url url
                                    :type (plist-get args :type)
                                    :parser (plist-get args :parser)
                                    :has-complete (functionp (plist-get args :complete))))
               (funcall (plist-get args :complete) :response planted)
               nil)))
    (let ((got (deferred:sync!
                 (deferred:$
                   (request-deferred "https://example.test/café"
                                     :type "GET"
                                     :parser 'json-read)
                   (deferred:nextc it #'identity)))))
      (list :source (rd482-test-source-state)
            :captured captured
            :got got))))
"####,
        expect![[
            r#"OK (:source (:tree "4e8ac232df3e852ce6ea3e10d4713fc45fac0386" :manifest (("request-deferred-pkg.el" . "1f1048efb0944551d25c6757b2d669ef454d78b87be00d33aa4a97af74c2b44f") ("request-deferred.el" . "4df2a46a52386e4920a883669e5504fd1bce19951b849357336b6dd3b3fcf3eb")) :feature t :version "20220614.1604") :captured (:url "https://example.test/café" :type "GET" :parser json-read :has-complete t) :got (:status-code 200 :data "café"))"#
        ]],
    )
}

fn user_complete_callback_is_replaced() -> ParityBatchCase {
    ParityBatchCase::value(
        "user_complete_callback_is_replaced",
        r####"
(let (user-ran captured (planted '(:ok t)))
  (cl-letf (((symbol-function 'request)
             (lambda (_url &rest args)
               (setq captured (plist-get args :complete))
               (funcall captured :response planted)
               nil)))
    (let ((got (deferred:sync!
                 (request-deferred "https://example.test/"
                                   :complete (lambda (&rest _)
                                               (setq user-ran t))))))
      (list :source (rd482-test-source-state)
            :user-ran user-ran
            :got got
            :complete-is-user (eq captured (lambda (&rest _) (setq user-ran t)))))))
"####,
        expect![[
            r#"OK (:source (:tree "4e8ac232df3e852ce6ea3e10d4713fc45fac0386" :manifest (("request-deferred-pkg.el" . "1f1048efb0944551d25c6757b2d669ef454d78b87be00d33aa4a97af74c2b44f") ("request-deferred.el" . "4df2a46a52386e4920a883669e5504fd1bce19951b849357336b6dd3b3fcf3eb")) :feature t :version "20220614.1604") :user-ran nil :got (:ok t) :complete-is-user nil)"#
        ]],
    )
}

fn missing_url_still_builds_a_deferred() -> ParityBatchCase {
    ParityBatchCase::value(
        "error_response_still_reaches_the_callback",
        r####"
(let ((planted '(:status-code 500 :error-thrown (error "boom"))))
  (cl-letf (((symbol-function 'request)
             (lambda (_url &rest args)
               (funcall (plist-get args :complete) :response planted)
               nil)))
    (list :source (rd482-test-source-state)
          :got (deferred:sync! (request-deferred "https://example.test/fail")))))
"####,
        expect![[
            r#"OK (:source (:tree "4e8ac232df3e852ce6ea3e10d4713fc45fac0386" :manifest (("request-deferred-pkg.el" . "1f1048efb0944551d25c6757b2d669ef454d78b87be00d33aa4a97af74c2b44f") ("request-deferred.el" . "4df2a46a52386e4920a883669e5504fd1bce19951b849357336b6dd3b3fcf3eb")) :feature t :version "20220614.1604") :got (:status-code 500 :error-thrown (error "boom")))"#
        ]],
    )
}

#[test]
fn request_deferred_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        request_deferred_posts_planted_response_on_the_chain(),
        user_complete_callback_is_replaced(),
        missing_url_still_builds_a_deferred(),
    ];
    assert_oracle_batch_cases(
        oracle(),
        "request-deferred-rank482",
        "request_deferred_parity",
        &cases,
    );
}
