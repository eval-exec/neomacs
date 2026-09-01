use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, PKG_INFO_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PKG_INFO_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const PKG_INFO_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'package)
(require 'pkg-info)

(defun pkg-info-test-write-library (name headers body)
  (let* ((root
          (expand-file-name
           "libraries"
           (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
         (file (expand-file-name (concat name ".el") root)))
    (make-directory root t)
    (with-temp-file file
      (insert ";;; " name ".el --- Test library\n")
      (dolist (header headers)
        (insert ";; " (car header) ": " (cdr header) "\n"))
      (insert "\n;;; Code:\n" body "\n(provide '" name ")\n"))
    (add-to-list 'load-path root)
    file))

(defun pkg-info-test-prepare-libraries ()
  (let ((core
         (pkg-info-test-write-library
          "pkg-info-test-core"
          '(("Version" . "9.1.0")
            ("X-Original-Version" . "2.7.3"))
          "(defun pkg-info-test-core-run (order)\n  (list :processed order :status 'ok))"))
        (plain
         (pkg-info-test-write-library
          "pkg-info-test-plain"
          '(("Package-Version" . "4.2beta3"))
          "(defun pkg-info-test-plain-render (value)\n  (format \"rendered:%s\" value))"))
        (invalid
         (pkg-info-test-write-library
          "pkg-info-test-invalid"
          '(("Version" . "1.0")
            ("X-Original-Version" . "rolling"))
          "(defun pkg-info-test-invalid-run () t)")))
    (load core nil t)
    (load plain nil t)
    (load invalid nil t)
    (list core plain invalid)))

(defconst pkg-info-test-library-files
  (pkg-info-test-prepare-libraries))

(defun pkg-info-test-package-desc (name version)
  (package-desc-create
   :name name
   :version version
   :summary "Sandbox package"
   :reqs nil
   :kind 'single
   :archive "sandbox"
   :dir (file-name-directory (car pkg-info-test-library-files))))

(defun pkg-info-test-http-buffer (status json)
  (let ((buffer (generate-new-buffer " *pkg-info-http*")))
    (with-current-buffer buffer
      (insert (format "HTTP/1.1 %d Test\r\nContent-Type: application/json\r\n\r\n%s"
                      status json))
      (goto-char (point-min))
      (search-forward "\r\n\r\n")
      (setq-local url-http-end-of-headers (point)))
    buffer))
"##;

fn pkg_info_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(PKG_INFO_MELPA_PIN, "pkg-info.el")
        .expect("prepare pinned Pkg Info source below ./tmp")
        .with_prelude(PKG_INFO_TEST_PRELUDE)
        .with_timeout(PKG_INFO_TEST_TIMEOUT)
}

fn library_inventory_resolves_sources_headers_and_defining_functions() -> ParityBatchCase {
    let elisp_form = r##"
(let ((root (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
  (list
   :core
   (list
    :source
    (file-relative-name
     (pkg-info-library-source 'pkg-info-test-core)
     root)
    :version (pkg-info-library-version 'pkg-info-test-core)
    :original (pkg-info-library-original-version "pkg-info-test-core")
    :defining-source
    (file-relative-name
     (pkg-info-defining-library #'pkg-info-test-core-run)
     root)
    :defining-version
    (pkg-info-defining-library-version #'pkg-info-test-core-run)
    :defining-original
    (pkg-info-defining-library-original-version
     #'pkg-info-test-core-run))
   :plain
   (list
    :source
    (file-relative-name
     (pkg-info-library-source "pkg-info-test-plain.el")
     root)
    :version (pkg-info-library-version "pkg-info-test-plain.el")
    :defining-version
    (pkg-info-defining-library-version
     #'pkg-info-test-plain-render))))
"##;
    let expect = expect![[
        r####"OK (:core (:source "libraries/pkg-info-test-core.el" :version (9 1 0) :original (2 7 3) :defining-source "libraries/pkg-info-test-core.el" :defining-version (9 1 0) :defining-original (2 7 3)) :plain (:source "libraries/pkg-info-test-plain.el" :version (4 2 -2 3) :defining-version (4 2 -2 3)))"####
    ]];
    ParityBatchCase::value(
        "library_inventory_resolves_sources_headers_and_defining_functions",
        elisp_form,
        expect,
    )
}

fn version_formatting_roundtrips_release_and_prerelease_metadata() -> ParityBatchCase {
    let elisp_form = r##"
(let ((versions
       '("3.4.2.1"
         "4.2beta3"
         "2.7alpha"
         "1.0pre2"
         "0.9snapshot"
         ".5")))
  (mapcar
   (lambda (version)
     (let ((parsed (version-to-list version)))
       (list version
             :parsed parsed
             :formatted (pkg-info-format-version parsed)
             :trailing-zero-equal
             (version-list-= parsed
                             (append parsed '(0 0))))))
   versions))
"##;
    let expect = expect![[
        r####"OK (("3.4.2.1" :parsed (3 4 2 1) :formatted "3.4.2.1" :trailing-zero-equal t) ("4.2beta3" :parsed (4 2 -2 3) :formatted "4.2beta3" :trailing-zero-equal t) ("2.7alpha" :parsed (2 7 -3) :formatted "2.7alpha" :trailing-zero-equal t) ("1.0pre2" :parsed (1 0 -1 2) :formatted "1.0pre2" :trailing-zero-equal t) ("0.9snapshot" :parsed (0 9 -4) :formatted "0.9snapshot" :trailing-zero-equal t) (".5" :parsed (0 5) :formatted "0.5" :trailing-zero-equal t))"####
    ]];
    ParityBatchCase::value(
        "version_formatting_roundtrips_release_and_prerelease_metadata",
        elisp_form,
        expect,
    )
}

fn installed_package_versions_combine_with_library_header_precedence() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((core-package
        (pkg-info-test-package-desc
         'pkg-info-test-core '(2026 8 2)))
       (plain-package
        (pkg-info-test-package-desc
         'pkg-info-test-plain '(4 2 -2 3)))
       (package-alist
        (list
         (list 'pkg-info-test-core core-package)
         (list 'pkg-info-test-plain plain-package)))
       messages)
  (cl-letf (((symbol-function 'message)
             (lambda (format-string &rest args)
               (let ((text (apply #'format format-string args)))
                 (push text messages)
                 text))))
    (list
     :package-version
     (list
      (pkg-info-package-version 'pkg-info-test-core)
      (pkg-info-package-version "pkg-info-test-plain"))
     :core-info
     (pkg-info-version-info
      'pkg-info-test-core 'pkg-info-test-core t)
     :plain-info
     (pkg-info-version-info
      "pkg-info-test-plain" 'pkg-info-test-plain t)
     :without-installed-package
     (pkg-info-version-info
      'pkg-info-test-core 'pkg-info-test-missing)
     :messages (nreverse messages))))
"##;
    let expect = expect![[
        r####"OK (:package-version ((2026 8 2) (4 2 -2 3)) :core-info "2.7.3 (package: 2026.8.2)" :plain-info "4.2beta3" :without-installed-package "2.7.3" :messages ("2.7.3 (package: 2026.8.2)" "4.2beta3"))"####
    ]];
    ParityBatchCase::value(
        "installed_package_versions_combine_with_library_header_precedence",
        elisp_form,
        expect,
    )
}

fn cached_melpa_recipes_drive_fetcher_and_wiki_classification() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((recipes
        '((magit
           (fetcher . "github")
           (repo . "magit/magit"))
          (dired-plus
           (fetcher . "wiki")
           (files . ("dired+.el")))
          (local-tool
           (fetcher . "git")
           (url . "https://example.test/local-tool.git"))))
       (pkg-info-melpa-recipes recipes))
  (list
   :magit
   (list :recipe (pkg-info-get-melpa-recipe 'magit)
         :fetcher (pkg-info-get-melpa-fetcher 'magit)
         :wiki (pkg-info-wiki-package-p 'magit))
   :wiki
   (list :recipe (pkg-info-get-melpa-recipe 'dired-plus)
         :fetcher (pkg-info-get-melpa-fetcher 'dired-plus)
         :wiki (pkg-info-wiki-package-p 'dired-plus))
   :missing
   (list :recipe (pkg-info-get-melpa-recipe 'missing)
         :fetcher (pkg-info-get-melpa-fetcher 'missing)
         :wiki (pkg-info-wiki-package-p 'missing))
   :cache-reused (eq recipes (pkg-info-get-melpa-recipes))))
"##;
    let expect = expect![[
        r####"OK (:magit (:recipe ((fetcher . "github") (repo . "magit/magit")) :fetcher "github" :wiki nil) :wiki (:recipe ((fetcher . "wiki") (files "dired+.el")) :fetcher "wiki" :wiki t) :missing (:recipe nil :fetcher nil :wiki nil) :cache-reused t)"####
    ]];
    ParityBatchCase::value(
        "cached_melpa_recipes_drive_fetcher_and_wiki_classification",
        elisp_form,
        expect,
    )
}

fn recipe_retrieval_parses_http_json_and_kills_the_response_buffer() -> ParityBatchCase {
    let elisp_form = r##"
(let ((response
       (pkg-info-test-http-buffer
        200
        "{\"magit\":{\"fetcher\":\"github\",\"repo\":\"magit/magit\"},\"dired-plus\":{\"fetcher\":\"wiki\"}}")))
  (cl-letf (((symbol-function 'url-retrieve-synchronously)
             (lambda (_url) response)))
    (let ((recipes (pkg-info-retrieve-melpa-recipes)))
      (list
       :recipes (copy-tree recipes)
       :magit (copy-tree (cdr (assq 'magit recipes)))
       :wiki-fetcher
       (cdr (assq 'fetcher (cdr (assq 'dired-plus recipes))))
       :response-live (buffer-live-p response)))))
"##;
    let expect = expect![[
        r####"OK (:recipes ((magit (fetcher . "github") (repo . "magit/magit")) (dired-plus (fetcher . "wiki"))) :magit ((fetcher . "github") (repo . "magit/magit")) :wiki-fetcher "wiki" :response-live nil)"####
    ]];
    ParityBatchCase::value(
        "recipe_retrieval_parses_http_json_and_kills_the_response_buffer",
        elisp_form,
        expect,
    )
}

fn failed_recipe_retrieval_reports_status_and_still_kills_the_response_buffer() -> ParityBatchCase {
    let elisp_form = r##"
(let ((response
       (pkg-info-test-http-buffer
        503
        "{\"error\":\"maintenance\"}")))
  (cl-letf (((symbol-function 'url-retrieve-synchronously)
             (lambda (_url) response)))
    (let ((failure
           (condition-case error
               (list :returned (pkg-info-retrieve-melpa-recipes))
             (error
              (list :signal (car error)
                    :data (cdr error)
                    :message (error-message-string error))))))
      (list :failure failure
            :response-live (buffer-live-p response)))))
"##;
    let expect = expect![[
        r####"OK (:failure (:signal error :data ("Failed to retrieve MELPA recipes from http://melpa.org/recipes.json (code 503)") :message "Failed to retrieve MELPA recipes from http://melpa.org/recipes.json (code 503)") :response-live nil)"####
    ]];
    ParityBatchCase::value(
        "failed_recipe_retrieval_reports_status_and_still_kills_the_response_buffer",
        elisp_form,
        expect,
    )
}

fn malformed_and_missing_metadata_return_actionable_diagnostics() -> ParityBatchCase {
    let elisp_form = r##"
(let ((text-quoting-style 'straight))
  (cl-labels
      ((capture
        (thunk)
        (condition-case error
            (list :returned (funcall thunk))
          (error
           (list :signal (car error)
                 :data (cdr error)
                 :message (error-message-string error))))))
    (list
     :invalid-original
     (capture
      (lambda ()
        (pkg-info-library-original-version
         'pkg-info-test-invalid)))
     :missing-original
     (capture
      (lambda ()
        (pkg-info-library-original-version
         'pkg-info-test-plain)))
     :missing-library
     (capture
      (lambda ()
        (pkg-info-library-version
         'pkg-info-test-does-not-exist)))
     :non-function
     (capture
      (lambda ()
        (pkg-info-defining-library
         'pkg-info-test-does-not-exist)))
     :anonymous
     (let ((failure
            (capture
             (lambda ()
               (pkg-info-defining-library
                (lambda () "anonymous"))))))
       (list
        :signal (plist-get failure :signal)
        :message
        (if (string-prefix-p
             "Can't find definition of "
             (plist-get failure :message))
            "Can't find definition of <anonymous function>"
          (plist-get failure :message)))))))
"##;
    let expect = expect![[
        r####"OK (:invalid-original (:signal error :data ("Invalid version syntax: 'rolling' (must start with a number)") :message "Invalid version syntax: 'rolling' (must start with a number)") :missing-original (:signal error :data ("Library pkg-info-test-plain has no original version") :message "Library pkg-info-test-plain has no original version") :missing-library (:signal file-error :data ("Can't find library" "pkg-info-test-does-not-exist") :message "Can't find library: pkg-info-test-does-not-exist") :non-function (:signal wrong-type-argument :data (functionp pkg-info-test-does-not-exist) :message "Wrong type argument: functionp, pkg-info-test-does-not-exist") :anonymous (:signal error :message "Can't find definition of <anonymous function>"))"####
    ]];
    ParityBatchCase::value(
        "malformed_and_missing_metadata_return_actionable_diagnostics",
        elisp_form,
        expect,
    )
}

fn newest_installed_descriptor_wins_and_equal_versions_omit_package_suffix() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((old
        (pkg-info-test-package-desc
         'pkg-info-test-plain '(3 9 0)))
       (same
        (pkg-info-test-package-desc
         'pkg-info-test-plain '(4 2 -2 3 0)))
       (package-alist
        (list
         (list 'pkg-info-test-plain old same))))
  (list
   :selected
   (copy-sequence
    (pkg-info-package-version 'pkg-info-test-plain))
   :version-info
   (pkg-info-version-info
    'pkg-info-test-plain 'pkg-info-test-plain)
   :installed-order
   (mapcar
    (lambda (package)
      (copy-sequence (epl-package-version package)))
    (epl-find-installed-packages
     'pkg-info-test-plain))))
"##;
    let expect = expect![[
        r####"OK (:selected (4 2 -2 3 0) :version-info "4.2beta3" :installed-order ((4 2 -2 3 0) (3 9 0)))"####
    ]];
    ParityBatchCase::value(
        "newest_installed_descriptor_wins_and_equal_versions_omit_package_suffix",
        elisp_form,
        expect,
    )
}

#[test]
fn pkg_info_package_batch() {
    let cases = vec![
        library_inventory_resolves_sources_headers_and_defining_functions(),
        version_formatting_roundtrips_release_and_prerelease_metadata(),
        installed_package_versions_combine_with_library_header_precedence(),
        cached_melpa_recipes_drive_fetcher_and_wiki_classification(),
        recipe_retrieval_parses_http_json_and_kills_the_response_buffer(),
        failed_recipe_retrieval_reports_status_and_still_kills_the_response_buffer(),
        malformed_and_missing_metadata_return_actionable_diagnostics(),
        newest_installed_descriptor_wins_and_equal_versions_omit_package_suffix(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed Pkg Info parity test");
    assert_oracle_batch_cases(pkg_info_oracle(), test_name, "pkg_info_parity", &cases);
}
