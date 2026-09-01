use std::time::Duration;

use expect_test::expect;

use crate::{
    CachedMelpaOracle, DASH_MELPA_PIN, F_MELPA_PIN, FLYCHECK_DMD_DUB_MELPA_PIN, FLYCHECK_MELPA_PIN,
    S_MELPA_PIN,
};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const FLYCHECK_DMD_DUB_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const FLYCHECK_DMD_DUB_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defvar fldd-test-sandbox
  (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defun fldd-test-root (name)
  "Create and return a clean test directory NAME."
  (let ((root (file-name-as-directory
               (expand-file-name name fldd-test-sandbox))))
    (when (file-exists-p root)
      (delete-directory root t))
    (make-directory root t)
    root))

(defun fldd-test-write (path contents)
  "Write CONTENTS to PATH after creating its parent."
  (make-directory (file-name-directory path) t)
  (write-region contents nil path nil 'silent)
  path)

(defun fldd-test-executable (directory name body)
  "Create executable NAME containing BODY in DIRECTORY."
  (let ((path (expand-file-name name directory)))
    (fldd-test-write path body)
    (set-file-modes path #o755)
    path))

(defun fldd-test-read (path)
  "Read PATH without text properties."
  (with-temp-buffer
    (insert-file-contents path)
    (buffer-string)))

(defun fldd-test-relative-paths (paths root)
  "Return PATHS relative to ROOT."
  (mapcar (lambda (path) (file-relative-name path root)) paths))

(defun fldd-test-relative-args (args root)
  "Make absolute -J paths in compiler ARGS relative to ROOT."
  (mapcar
   (lambda (arg)
     (if (string-prefix-p "-J" arg)
         (concat "-J" (file-relative-name (substring arg 2) root))
       arg))
   args))
"##;

fn flycheck_dmd_dub_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(FLYCHECK_DMD_DUB_MELPA_PIN, "flycheck-dmd-dub.el")
        .expect("prepare pinned flycheck-dmd-dub source below ./tmp")
        .with_melpa_dependency(FLYCHECK_MELPA_PIN)
        .expect("prepare pinned Flycheck dependency")
        .with_melpa_dependency(F_MELPA_PIN)
        .expect("prepare pinned f dependency")
        .with_melpa_dependency(DASH_MELPA_PIN)
        .expect("prepare pinned dash dependency")
        .with_melpa_dependency(S_MELPA_PIN)
        .expect("prepare pinned s dependency")
        .with_prelude(FLYCHECK_DMD_DUB_TEST_PRELUDE)
        .with_timeout(FLYCHECK_DMD_DUB_TEST_TIMEOUT)
}

fn nested_monorepo_discovery_selects_topmost_or_nearest_dub_project() -> ParityBatchCase {
    ParityBatchCase::value(
        "nested_monorepo_discovery_selects_topmost_or_nearest_dub_project",
        r##"
(let* ((root (fldd-test-root "fldd-project-discovery"))
       (monorepo (expand-file-name "commerce/" root))
       (checkout (expand-file-name "apps/checkout/" monorepo))
       (service (expand-file-name "source/services/tax/" checkout))
       (outside (expand-file-name "scratch/" root)))
  (make-directory service t)
  (make-directory outside t)
  (fldd-test-write (expand-file-name "dub.json" monorepo)
                   "{\"name\":\"commerce\"}\n")
  (fldd-test-write (expand-file-name "dub.sdl" checkout)
                   "name \"checkout\"\n")
  (let ((default-directory service))
    (list
     :topmost
     (file-relative-name
      (let ((fldd-no-recurse-dir nil)) (fldd--get-project-dir))
      root)
     :nearest
     (file-relative-name
      (let ((fldd-no-recurse-dir t)) (fldd--get-project-dir))
      root)
     :outside
     (let ((default-directory outside)) (fldd--get-project-dir)))))
"##,
        expect![[r##"OK (:topmost "commerce/" :nearest "commerce/apps/checkout/" :outside nil)"##]],
    )
}

fn dub_metadata_builds_deduplicated_imports_and_ordered_compiler_flags() -> ParityBatchCase {
    ParityBatchCase::value(
        "dub_metadata_builds_deduplicated_imports_and_ordered_compiler_flags",
        r##"
(let* ((root (fldd-test-root "fldd-metadata"))
       (checkout (expand-file-name "checkout/" root))
       (shared (expand-file-name "shared/" root))
       (json
        (json-encode
         `((packages
            . [((path . ,checkout)
                (importPaths . ["source" "tests" "source"])
                (stringImportPaths . ["views" "emails"])
                (versions . ["CheckoutV2" "TaxAudit"])
                (dflags . ["-preview=in" "-dip1000"]))
               ((path . ,shared)
                (importPaths . ["source"])
                (stringImportPaths . ["templates"]))])))))
  (with-temp-buffer
    (let ((flycheck-dmd-dub-use-cache-p nil))
      (fldd--set-variables-from-json-string json)
      (flycheck-dmd-dub-add-version "Release")
      (flycheck-dmd-dub-add-version "Release")
      (list
       :include-paths
       (fldd-test-relative-paths flycheck-dmd-include-path root)
       :arguments
       (fldd-test-relative-args flycheck-dmd-args root)
       :locals
       (list (local-variable-p 'flycheck-dmd-include-path)
             (local-variable-p 'flycheck-dmd-args))))))
"##,
        expect![[
            r##"OK (:include-paths ("checkout/source" "checkout/tests" "shared/source") :arguments ("-version=Release" "-w" "-unittest" "-Jcheckout/views" "-Jcheckout/emails" "-Jshared/templates" "-preview=in" "-dip1000" "-version=CheckoutV2" "-version=TaxAudit") :locals (t t))"##
        ]],
    )
}

fn public_setup_runs_dub_with_configuration_and_offline_dependency_flags() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_setup_runs_dub_with_configuration_and_offline_dependency_flags",
        r##"
(let* ((root (fldd-test-root "fldd-public-setup"))
       (project (expand-file-name "checkout/" root))
       (bin (expand-file-name "bin/" root))
       (home (expand-file-name "home/" root))
       (response (expand-file-name "dub-response.json" root))
       (log (expand-file-name "dub.log" root))
       (dependency (expand-file-name "vibe-d/" root)))
  (make-directory project t)
  (make-directory bin t)
  (make-directory (expand-file-name ".dub/packages/vibe-d-0.9.6/" home) t)
  (fldd-test-write (expand-file-name "dub.json" project)
                   "{\"name\":\"checkout\"}\n")
  (fldd-test-write
   (expand-file-name "dub.selections.json" project)
   "{\"versions\":{\"vibe-d\":\"~0.9.6\"}}\n")
  (fldd-test-write
   response
   (json-encode
    `((packages
       . [((path . ,project)
           (importPaths . ["source"])
           (stringImportPaths . ["views"])
           (versions . ["Configured"])
           (dflags . ["-checkaction=context"]))
          ((path . ,dependency)
           (importPaths . ["source"])
           (stringImportPaths . []))]))))
  (fldd-test-executable
   bin "dub"
   "#!/bin/sh\nprintf '%s\n' \"$*\" >> \"$FLDD_DUB_LOG\"\nprintf 'Registry warning before JSON\\n'\ncat \"$FLDD_DUB_RESPONSE\"\n")
  (with-temp-buffer
    (let ((default-directory project)
          (process-environment (copy-sequence process-environment))
          (exec-path (cons bin exec-path))
          (fldd--cache-dir (file-name-as-directory
                            (expand-file-name "describe-cache/" root)))
          (fldd-dub-configuration "unittest")
          (flycheck-dmd-dub-use-cache-p nil))
      (setenv "PATH" (concat (directory-file-name bin)
                             path-separator (getenv "PATH")))
      (setenv "HOME" (directory-file-name home))
      (setenv "FLDD_DUB_LOG" log)
      (setenv "FLDD_DUB_RESPONSE" response)
      (flycheck-dmd-dub-set-variables)
      (list
       :command (string-trim-right (fldd-test-read log))
       :include-paths
       (fldd-test-relative-paths flycheck-dmd-include-path root)
       :arguments
       (fldd-test-relative-args flycheck-dmd-args root)))))
"##,
        expect![[
            r##"OK (:command "describe -c unittest --nodeps --skip-registry=all" :include-paths ("checkout/source" "vibe-d/source") :arguments ("-w" "-unittest" "-Jcheckout/views" "-checkaction=context" "-version=Configured"))"##
        ]],
    )
}

fn describe_cache_reuses_metadata_then_invalidates_after_manifest_change() -> ParityBatchCase {
    ParityBatchCase::value(
        "describe_cache_reuses_metadata_then_invalidates_after_manifest_change",
        r##"
(let* ((root (fldd-test-root "fldd-describe-cache"))
       (project (expand-file-name "shipping/" root))
       (bin (expand-file-name "bin/" root))
       (response (expand-file-name "response.json" root))
       (log (expand-file-name "dub.log" root))
       (manifest (expand-file-name "dub.json" project)))
  (make-directory project t)
  (make-directory bin t)
  (fldd-test-write manifest "{\"name\":\"shipping\"}\n")
  (set-file-times manifest (seconds-to-time 1000000000))
  (fldd-test-write
   response
   (json-encode
    `((packages
       . [((path . ,project)
           (importPaths . ["source"])
           (stringImportPaths . [])
           (versions . ["ShippingV1"])
           (dflags . []))]))))
  (fldd-test-executable
   bin "dub"
   "#!/bin/sh\nprintf 'run\\n' >> \"$FLDD_DUB_LOG\"\ncat \"$FLDD_DUB_RESPONSE\"\n")
  (with-temp-buffer
    (let ((default-directory project)
          (process-environment (copy-sequence process-environment))
          (exec-path (cons bin exec-path))
          (fldd--cache-dir (file-name-as-directory
                            (expand-file-name "cache/" root)))
          (flycheck-dmd-dub-use-cache-p nil))
      (setenv "PATH" (concat (directory-file-name bin)
                             path-separator (getenv "PATH")))
      (setenv "FLDD_DUB_LOG" log)
      (setenv "FLDD_DUB_RESPONSE" response)
      (let ((first (fldd--describe-json-for project))
            (second (fldd--describe-json-for project)))
        (set-file-times manifest (seconds-to-time 2000000000))
        (let ((third (fldd--describe-json-for project)))
          (let ((cache-file (fldd--dub-describe-cache-file-name)))
            (list :same-results (and (equal first second) (equal second third))
                  :dub-runs
                  (length (split-string (fldd-test-read log) "\n" t))
                  :cache
                  (list (file-regular-p cache-file)
                        (file-name-nondirectory cache-file)
                        (string-suffix-p
                         "shipping/dub_describe.json" cache-file)))))))))
"##,
        expect![[r##"OK (:same-results t :dub-runs 2 :cache (t "dub_describe.json" t))"##]],
    )
}

fn project_cache_roundtrip_avoids_dub_and_keeps_buffer_state_local() -> ParityBatchCase {
    ParityBatchCase::value(
        "project_cache_roundtrip_avoids_dub_and_keeps_buffer_state_local",
        r##"
(let* ((root (fldd-test-root "fldd-project-cache"))
       (project (expand-file-name "inventory/" root))
       (cache (expand-file-name ".fldd.cache" project))
       (selections (expand-file-name "dub.selections.json" project))
       (other (generate-new-buffer " *fldd-other*"))
       (old-default-includes (default-value 'flycheck-dmd-include-path))
       (old-default-args (default-value 'flycheck-dmd-args)))
  (make-directory project t)
  (fldd-test-write (expand-file-name "dub.json" project)
                   "{\"name\":\"inventory\"}\n")
  (fldd-test-write selections "{\"versions\":{}}\n")
  (fldd-test-write
   cache
   (prin1-to-string
    `((import-paths . (,(expand-file-name "source" project)
                       ,(expand-file-name "vendor/mir" project)))
      (string-import-paths . (,(expand-file-name "templates" project))))))
  (set-file-times selections (seconds-to-time 1000000000))
  (set-file-times cache (seconds-to-time 1100000000))
  (unwind-protect
      (progn
        (setq-default flycheck-dmd-include-path 'global-include-sentinel
                      flycheck-dmd-args 'global-args-sentinel)
        (with-temp-buffer
          (let ((default-directory project)
                (fldd--cache-file cache)
                (flycheck-dmd-dub-use-cache-p t))
            (flycheck-dmd-dub-set-variables)
            (list
             :include-paths
             (fldd-test-relative-paths flycheck-dmd-include-path root)
             :arguments
             (fldd-test-relative-args flycheck-dmd-args root)
             :local
             (list (local-variable-p 'flycheck-dmd-include-path)
                   (local-variable-p 'flycheck-dmd-args))
             :other-buffer
             (with-current-buffer other
               (list flycheck-dmd-include-path flycheck-dmd-args))))))
    (setq-default flycheck-dmd-include-path old-default-includes
                  flycheck-dmd-args old-default-args)
    (when (buffer-live-p other) (kill-buffer other))))
"##,
        expect![[
            r##"OK (:include-paths ("inventory/source" "inventory/vendor/mir") :arguments ("-w" "-unittest" "-Jinventory/templates") :local (t t) :other-buffer (global-include-sentinel global-args-sentinel))"##
        ]],
    )
}

#[test]
fn flycheck_dmd_dub_package_batch() {
    let cases = vec![
        nested_monorepo_discovery_selects_topmost_or_nearest_dub_project(),
        dub_metadata_builds_deduplicated_imports_and_ordered_compiler_flags(),
        public_setup_runs_dub_with_configuration_and_offline_dependency_flags(),
        describe_cache_reuses_metadata_then_invalidates_after_manifest_change(),
        project_cache_roundtrip_avoids_dub_and_keeps_buffer_state_local(),
    ];
    let thread = std::thread::current();
    let test_name = thread
        .name()
        .unwrap_or("unnamed flycheck-dmd-dub parity test");
    assert_oracle_batch_cases(
        flycheck_dmd_dub_oracle(),
        test_name,
        "flycheck_dmd_dub_parity",
        &cases,
    );
}
