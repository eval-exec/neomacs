//! Practical parity coverage for rank 414 `clj-refactor`.
//!
//! The corpus drives public mode/keybinding and changelog routes, local
//! structural refactorings, offline namespace cleanup, project dependency
//! sorting, and the unavailable-middleware failure followed by local recovery.

use std::time::Duration;

use expect_test::expect;

use crate::{CLJ_REFACTOR_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)
(require 'clj-refactor)

(get-buffer-create " *code-conversion-work*")

(defconst cljr414-test-upstream-main-sha
  "1dd4f983638ca555b9a432958dc93e3b62d13de8dc2ac5e3c3e0c88a85385f98")
(defconst cljr414-test-source-manifest
  '(("CHANGELOG.md" . "526336467b0a2e372159fde973176d133733f711045ee4413fdb5fb2b581b04b")
    ("clj-refactor-pkg.el" . "dd7c7b1ee267d0d01893eeae0b10c441189ba1f6639eea4a9678d4558e961e43")
    ("clj-refactor.el" . "2ad295ec89d3dc320d9f7dbcebe36a5a826eab47a56cce764807f1aec586b413")))

(defvar cljr414-test-root nil)
(defvar cljr414-test-root-owned nil)
(defvar cljr414-test-save-ledger nil)

(defun cljr414-test-file-sha (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun cljr414-test-source-state ()
  (let* ((source (file-truename (locate-library "clj-refactor.el")))
         (directory (file-name-directory source))
         (manifest
          (mapcar
           (lambda (entry)
             (let ((file (expand-file-name (car entry) directory)))
               (unless (and (file-regular-p file) (not (file-symlink-p file)))
                 (error "Unexpected clj-refactor source entry: %s" file))
               (cons (car entry) (cljr414-test-file-sha file))))
           cljr414-test-source-manifest)))
    (unless (and (string-suffix-p "/clj-refactor.el" source)
                 (equal manifest cljr414-test-source-manifest))
      (error "Unexpected clj-refactor source manifest: %S" manifest))
    (list :upstream-sha256 cljr414-test-upstream-main-sha
          :installed-sha256 (copy-tree manifest)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'clj-refactor package-alist))))
          :feature (featurep 'clj-refactor))))

(defun cljr414-test-write (relative contents)
  (let ((file (expand-file-name relative cljr414-test-root)))
    (unless (and cljr414-test-root-owned
                 (file-in-directory-p file cljr414-test-root))
      (error "Refusing clj-refactor write outside owned root: %s" file))
    (make-directory (file-name-directory file) t)
    (let ((coding-system-for-write 'utf-8-unix))
      (with-temp-file file (insert contents)))
    file))

(defun cljr414-test-manifest (root)
  (sort
   (mapcar
    (lambda (file)
      (unless (and (file-regular-p file) (not (file-symlink-p file)))
        (error "Unexpected clj-refactor fixture entry: %s" file))
      (cons (file-relative-name file root) (cljr414-test-file-sha file)))
    (directory-files-recursively root "."))
   (lambda (left right) (string< (car left) (car right)))))

(defun cljr414-test-buffer-state ()
  (list :text (buffer-string)
        :point (point)
        :modified (buffer-modified-p)
        :mode major-mode
        :refactor-mode (and (boundp 'clj-refactor-mode)
                            clj-refactor-mode)
        :post-hook (and (memq #'cljr--post-command-hook post-command-hook) t)
        :undo (and buffer-undo-list t)))

(defun cljr414-test-condition (thunk)
  (condition-case condition
      (list :value (funcall thunk))
    (error
     (list :error (car condition)
           :data (copy-tree (cdr condition))
           :message (error-message-string condition)))))

(defun cljr414-test-forbid-external (operation &rest arguments)
  (error "Unexpected clj-refactor external boundary: %S %S"
         operation arguments))

(defun cljr414-test-park-buffer (name)
  (when-let* ((buffer (get-buffer name)))
    (let ((parked (generate-new-buffer-name (concat " *parked " name "*"))))
      (with-current-buffer buffer (rename-buffer parked t))
      (cons buffer name))))

(defun cljr414-test-run (case-name body)
  (let* ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (root (and sandbox
                    (file-name-as-directory
                     (expand-file-name (concat "clj-refactor-" case-name "/")
                                       sandbox))))
         (buffer-before (current-buffer))
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (copy-sequence timer-list))
         (frames-before (frame-list))
         (window-before (current-window-configuration))
         (source-before (cljr414-test-source-state))
         (parked nil)
         (cljr414-test-root root)
         (cljr414-test-root-owned nil)
         (cljr414-test-save-ledger nil)
         (clj-refactor-map (copy-keymap clj-refactor-map))
         (minor-mode-map-alist
          (mapcar (lambda (entry)
                    (if (eq (car-safe entry) 'clj-refactor-mode)
                        (cons 'clj-refactor-mode clj-refactor-map)
                      entry))
                  minor-mode-map-alist))
         (cljr--post-command-messages nil)
         (cljr--occurrences nil)
         (cljr--signature-rows nil)
         (cljr--signature-original-params nil)
         (cljr--opened-buffers 'unset)
         (cljr--last-refactoring nil)
         (cljr--refactoring-active nil)
         (cljr--refactoring-files nil)
         (cljr--refactoring-opened nil)
         (cljr--artifacts-cache nil)
         (cljr--versions-cache (make-hash-table :test #'equal))
         (cljr--suggest-libspecs-cache (make-hash-table :test #'equal))
         (cljr--debug-mode nil)
         (cljr-preview-refactorings nil)
         (cljr-auto-clean-ns nil)
         (cljr-populate-artifact-cache-on-startup nil)
         (cljr-eagerly-build-asts-on-startup nil)
         (cljr-warn-on-eval nil)
         (cljr-hotload-dependencies nil)
         result cleanup-errors source-after)
    (unwind-protect
        (progn
          (unless (and root (file-name-absolute-p root))
            (error "Missing absolute clj-refactor sandbox root"))
          (when (file-exists-p root)
            (error "clj-refactor sandbox root exists: %s" root))
          (dolist (name '("*cljr-find-usages*" "*clj-refactor preview*"))
            (when-let* ((entry (cljr414-test-park-buffer name)))
              (push entry parked)))
          (make-directory root)
          (setq cljr414-test-root-owned t)
          (cl-letf (((symbol-function 'call-process)
                     (lambda (&rest arguments)
                       (apply #'cljr414-test-forbid-external
                              'call-process arguments)))
                    ((symbol-function 'call-process-region)
                     (lambda (&rest arguments)
                       (apply #'cljr414-test-forbid-external
                              'call-process-region arguments)))
                    ((symbol-function 'make-process)
                     (lambda (&rest arguments)
                       (apply #'cljr414-test-forbid-external
                              'make-process arguments)))
                    ((symbol-function 'process-file)
                     (lambda (&rest arguments)
                       (apply #'cljr414-test-forbid-external
                              'process-file arguments)))
                    ((symbol-function 'start-file-process)
                     (lambda (&rest arguments)
                       (apply #'cljr414-test-forbid-external
                              'start-file-process arguments)))
                    ((symbol-function 'start-process)
                     (lambda (&rest arguments)
                       (apply #'cljr414-test-forbid-external
                              'start-process arguments)))
                    ((symbol-function 'url-retrieve)
                     (lambda (&rest arguments)
                       (apply #'cljr414-test-forbid-external
                              'url-retrieve arguments)))
                    ((symbol-function 'url-retrieve-synchronously)
                     (lambda (&rest arguments)
                       (apply #'cljr414-test-forbid-external
                              'url-retrieve-synchronously arguments))))
            (setq result (funcall body root)))
          (setq source-after (cljr414-test-source-state))
          (unless (equal source-before source-after)
            (error "clj-refactor source changed")))
      (cl-labels
          ((attempt (label thunk)
             (condition-case condition
                 (funcall thunk)
               (error
                (push (list label (car condition) (copy-tree (cdr condition)))
                      cleanup-errors)))))
        (dolist (process (process-list))
          (unless (memq process processes-before)
            (attempt (list 'process (process-name process))
                     (lambda () (delete-process process)))))
        (dolist (buffer (buffer-list))
          (unless (or (memq buffer buffers-before) (assq buffer parked))
            (attempt (list 'buffer (buffer-name buffer))
                     (lambda () (kill-buffer buffer)))))
        (dolist (timer (copy-sequence timer-list))
          (unless (memq timer timers-before)
            (attempt 'timer (lambda () (cancel-timer timer)))))
        (dolist (frame (frame-list))
          (unless (memq frame frames-before)
            (attempt 'frame (lambda () (delete-frame frame t)))))
        (attempt 'window (lambda () (set-window-configuration window-before)))
        (dolist (entry parked)
          (attempt (list 'parked (cdr entry))
                   (lambda ()
                     (if (buffer-live-p (car entry))
                         (with-current-buffer (car entry)
                           (rename-buffer (cdr entry) t))
                       (error "Parked clj-refactor buffer died: %s" (cdr entry))))))
        (when (buffer-live-p buffer-before)
          (attempt 'current-buffer (lambda () (set-buffer buffer-before))))
        (when cljr414-test-root-owned
          (attempt 'root (lambda () (delete-directory root t))))))
    (let ((cleanup
           (list :source-unchanged (equal source-before source-after)
                 :new-buffers
                 (mapcar #'buffer-name
                         (seq-remove (lambda (buffer) (memq buffer buffers-before))
                                     (buffer-list)))
                 :new-processes
                 (length (seq-remove (lambda (process) (memq process processes-before))
                                     (process-list)))
                 :new-timers
                 (length (seq-remove (lambda (timer) (memq timer timers-before))
                                     timer-list))
                 :new-frames
                 (length (seq-remove (lambda (frame) (memq frame frames-before))
                                     (frame-list)))
                 :root-exists (and root (file-exists-p root))
                 :buffer-restored (eq (current-buffer) buffer-before)
                 :cleanup-errors (nreverse cleanup-errors))))
      (if cleanup-errors
          (error "clj-refactor cleanup failed: %S" (list result cleanup))
        (list :source source-before :result result :cleanup cleanup)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(CLJ_REFACTOR_MELPA_PIN, "clj-refactor.el")
        .expect("prepare exact clj-refactor source and dependency closure below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn public_mode_keybindings_and_changelog_lifecycle() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_mode_keybindings_and_changelog_lifecycle",
        r####"
(cljr414-test-run
 "mode"
 (lambda (_root)
   (with-temp-buffer
     (clojure-mode)
     (cljr-add-keybindings-with-prefix "C-c C-r")
     (clj-refactor-mode 1)
     (let ((enabled (cljr414-test-buffer-state))
           (keys (mapcar (lambda (key)
                           (cons key (key-binding (kbd key))))
                         '("C-c C-r ct" "C-c C-r dk" "C-c C-r cn"
                           "C-c C-r rs" "C-c C-r hh"))))
       (clj-refactor-mode -1)
       (let ((disabled (cljr414-test-buffer-state)))
         (cljr-show-changelog)
         (let ((changelog
                (list :file (file-name-nondirectory (buffer-file-name))
                      :view view-mode
                      :read-only buffer-read-only
                      :heading (buffer-substring-no-properties
                                (point-min) (line-end-position)))))
           (list :enabled enabled
                 :keys keys
                 :disabled disabled
                 :changelog changelog)))))))
"####,
        expect![[
            r##"OK (:source (:upstream-sha256 "1dd4f983638ca555b9a432958dc93e3b62d13de8dc2ac5e3c3e0c88a85385f98" :installed-sha256 (("CHANGELOG.md" . "526336467b0a2e372159fde973176d133733f711045ee4413fdb5fb2b581b04b") ("clj-refactor-pkg.el" . "dd7c7b1ee267d0d01893eeae0b10c441189ba1f6639eea4a9678d4558e961e43") ("clj-refactor.el" . "2ad295ec89d3dc320d9f7dbcebe36a5a826eab47a56cce764807f1aec586b413")) :version "20260716.1545" :feature t) :result (:enabled (:text "" :point 1 :modified nil :mode clojure-mode :refactor-mode t :post-hook t :undo t) :keys (("C-c C-r ct" . cljr-cycle-thread) ("C-c C-r dk" . cljr-destructure-keys) ("C-c C-r cn" . cljr-clean-ns) ("C-c C-r rs" . cljr-rename-symbol) ("C-c C-r hh" . clj-refactor-menu)) :disabled (:text "" :point 1 :modified nil :mode clojure-mode :refactor-mode nil :post-hook nil :undo t) :changelog (:file "CHANGELOG.md" :view t :read-only t :heading "# Changelog")) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :cleanup-errors nil))"##
        ]],
    )
}

fn public_local_structural_refactorings_transform_real_code() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_local_structural_refactorings_transform_real_code",
        r####"
(cljr414-test-run
 "local"
 (lambda (_root)
   (let (thread destructure remove-let declaration)
     (with-temp-buffer
       (clojure-mode)
       (insert "(defn ship [xs]\n  (-> xs\n      (map #(str % \"界\"))))\n")
       (goto-char (point-min))
       (search-forward "(map")
       (call-interactively #'cljr-cycle-thread)
       (setq thread (cljr414-test-buffer-state)))
     (with-temp-buffer
       (clojure-mode)
       (insert "(defn total [order]\n  (+ (:net order) (:tax order) (audit order)))\n")
       (goto-char (point-min))
       (search-forward "order]")
       (backward-char (length "order]"))
       (call-interactively #'cljr-destructure-keys)
       (setq destructure (cljr414-test-buffer-state)))
     (with-temp-buffer
       (clojure-mode)
       (insert "(let [base 40 result (+ base 2)]\n  {:total result :label \"café\"})\n")
       (goto-char (point-max))
       (search-backward "result")
       (call-interactively #'cljr-remove-let)
       (setq remove-let (cljr414-test-buffer-state)))
     (with-temp-buffer
       (clojure-mode)
       (insert "(ns release.core)\n\n(defn- ^:tracked deliver [item]\n  (str item \"界\"))\n")
       (goto-char (point-min))
       (search-forward "deliver")
       (call-interactively #'cljr-add-declaration)
       (setq declaration (cljr414-test-buffer-state)))
     (list :thread thread :destructure destructure
           :remove-let remove-let :declaration declaration))))
"####,
        expect![[
            r#"OK (:source (:upstream-sha256 "1dd4f983638ca555b9a432958dc93e3b62d13de8dc2ac5e3c3e0c88a85385f98" :installed-sha256 (("CHANGELOG.md" . "526336467b0a2e372159fde973176d133733f711045ee4413fdb5fb2b581b04b") ("clj-refactor-pkg.el" . "dd7c7b1ee267d0d01893eeae0b10c441189ba1f6639eea4a9678d4558e961e43") ("clj-refactor.el" . "2ad295ec89d3dc320d9f7dbcebe36a5a826eab47a56cce764807f1aec586b413")) :version "20260716.1545" :feature t) :result (:thread (:text "(defn ship [xs]\n  (->> xs\n       (map #(str % \"界\"))))\n" :point 38 :modified t :mode clojure-mode :refactor-mode nil :post-hook nil :undo t) :destructure (:text "(defn total [{:keys [net tax] :as order}]\n  (+ net tax (audit order)))\n" :point 41 :modified t :mode clojure-mode :refactor-mode nil :post-hook nil :undo t) :remove-let (:text "{:total (+ 40 2) :label \"café\"}\n" :point 9 :modified t :mode clojure-mode :refactor-mode nil :post-hook nil :undo t) :declaration (:text "(ns release.core)\n\n(declare deliver)\n\n(defn- ^:tracked deliver [item]\n  (str item \"界\"))\n" :point 63 :modified t :mode clojure-mode :refactor-mode nil :post-hook nil :undo t)) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :cleanup-errors nil))"#
        ]],
    )
}

fn public_clean_ns_uses_offline_fallback_and_reports_message() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_clean_ns_uses_offline_fallback_and_reports_message",
        r####"
(cljr414-test-run
 "clean-ns"
 (lambda (_root)
   (with-temp-buffer
     (clojure-mode)
     (insert "(ns release.core\n  (:require [zeta.core :as z]\n            [alpha.core :as a])\n  (:import [java.util UUID Date]))\n\n(def label \"界\")\n")
     (goto-char (point-min))
     (clj-refactor-mode 1)
     (let (connection-calls)
       (cl-letf (((symbol-function 'cider-connected-p)
                  (lambda () (push 'clean-ns connection-calls) nil)))
         (call-interactively #'cljr-clean-ns))
       (unless (equal connection-calls '(clean-ns))
         (error "Unexpected clean-ns connection checks: %S" connection-calls))
       (cl-letf (((symbol-function 'message)
                  (lambda (format-string &rest arguments)
                    (push (apply #'format format-string arguments)
                          cljr414-test-save-ledger))))
         (run-hooks 'post-command-hook))
       (list :buffer (cljr414-test-buffer-state)
             :connection-checks (nreverse connection-calls)
             :displayed (nreverse cljr414-test-save-ledger))))))
"####,
        expect![[
            r#"OK (:source (:upstream-sha256 "1dd4f983638ca555b9a432958dc93e3b62d13de8dc2ac5e3c3e0c88a85385f98" :installed-sha256 (("CHANGELOG.md" . "526336467b0a2e372159fde973176d133733f711045ee4413fdb5fb2b581b04b") ("clj-refactor-pkg.el" . "dd7c7b1ee267d0d01893eeae0b10c441189ba1f6639eea4a9678d4558e961e43") ("clj-refactor.el" . "2ad295ec89d3dc320d9f7dbcebe36a5a826eab47a56cce764807f1aec586b413")) :version "20260716.1545" :feature t) :result (:buffer (:text "(ns release.core\n  (:require [alpha.core :as a]\n            [zeta.core :as z])\n  (:import [java.util UUID Date]))\n\n(def label \"界\")\n" :point 1 :modified t :mode clojure-mode :refactor-mode t :post-hook t :undo t) :connection-checks (clean-ns) :displayed ("Sorted the ns form (pruning unused libspecs needs the refactor-nrepl middleware).")) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :cleanup-errors nil))"#
        ]],
    )
}

fn public_project_dependency_sort_preserves_comments_and_saves() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_project_dependency_sort_preserves_comments_and_saves",
        r####"
(cljr414-test-run
 "sort-project"
 (lambda (root)
   (let* ((project
           (cljr414-test-write
            "project.clj"
            "(defproject demo \"0.1.0\"\n  :dependencies [[zeta/lib \"2.0\"]\n                 ;; Unicode dependency 界\n                 ^:replace [alpha/lib \"1.0\"]\n                 [mid/lib \"3.0\"]])\n"))
          (fixture-before (cljr414-test-manifest root))
          (default-directory root)
          (enable-dir-local-variables nil)
          buffer result)
     (setq buffer (find-file-noselect project))
     (with-current-buffer buffer
       (clojure-mode)
       (add-hook 'after-save-hook
                 (lambda ()
                   (push (list (file-relative-name buffer-file-name root)
                               (cljr414-test-file-sha buffer-file-name))
                         cljr414-test-save-ledger))
                 nil t)
       (call-interactively #'cljr-sort-project-dependencies)
       (setq result
             (list :buffer (cljr414-test-buffer-state)
                   :disk (buffer-string)
                   :save-ledger (nreverse cljr414-test-save-ledger)
                   :file (file-relative-name buffer-file-name root))))
     (list :before fixture-before
           :after (cljr414-test-manifest root)
           :result result))))
"####,
        expect![[
            r#"OK (:source (:upstream-sha256 "1dd4f983638ca555b9a432958dc93e3b62d13de8dc2ac5e3c3e0c88a85385f98" :installed-sha256 (("CHANGELOG.md" . "526336467b0a2e372159fde973176d133733f711045ee4413fdb5fb2b581b04b") ("clj-refactor-pkg.el" . "dd7c7b1ee267d0d01893eeae0b10c441189ba1f6639eea4a9678d4558e961e43") ("clj-refactor.el" . "2ad295ec89d3dc320d9f7dbcebe36a5a826eab47a56cce764807f1aec586b413")) :version "20260716.1545" :feature t) :result (:before (("project.clj" . "6351f334259bb7ab94a61f314c25884877dd4e9125a9eee65bcd13b690b5a97d")) :after (("project.clj" . "fbd7c00ce6b0ab830ab0f887945abd6055d0660071d1fde64012860f298e5dec") ("project.clj~" . "6351f334259bb7ab94a61f314c25884877dd4e9125a9eee65bcd13b690b5a97d")) :result (:buffer (:text "(defproject demo \"0.1.0\"\n  :dependencies [;; Unicode dependency 界\n                 ^:replace [alpha/lib \"1.0\"]\n                 [mid/lib \"3.0\"]\n                 [zeta/lib \"2.0\"]])\n" :point 179 :modified nil :mode clojure-mode :refactor-mode nil :post-hook nil :undo t) :disk "(defproject demo \"0.1.0\"\n  :dependencies [;; Unicode dependency 界\n                 ^:replace [alpha/lib \"1.0\"]\n                 [mid/lib \"3.0\"]\n                 [zeta/lib \"2.0\"]])\n" :save-ledger (("project.clj" "fbd7c00ce6b0ab830ab0f887945abd6055d0660071d1fde64012860f298e5dec")) :file "project.clj")) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :cleanup-errors nil))"#
        ]],
    )
}

fn public_middleware_failure_is_exact_then_local_refactor_recovers() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_middleware_failure_is_exact_then_local_refactor_recovers",
        r####"
(cljr414-test-run
 "failure"
 (lambda (_root)
   (with-temp-buffer
     (clojure-mode)
     (insert "(defn calculate [x]\n  (-> x inc))\n")
     (goto-char (point-min))
     (search-forward "calculate")
     (let ((before (cljr414-test-buffer-state))
           failure after-failure recovery connection-calls)
       (cl-letf (((symbol-function 'cider-connected-p)
                  (lambda () (push 'rename-symbol connection-calls) nil)))
         (setq failure
               (cljr414-test-condition
                (lambda () (cljr-rename-symbol "compute")))))
       (unless (equal connection-calls '(rename-symbol))
         (error "Unexpected rename connection checks: %S" connection-calls))
       (setq after-failure (cljr414-test-buffer-state))
       (goto-char (point-max))
       (search-backward "inc")
       (call-interactively #'cljr-cycle-thread)
       (setq recovery (cljr414-test-buffer-state))
       (list :before before
             :failure failure
             :connection-checks (nreverse connection-calls)
             :failure-atomic (equal before after-failure)
             :recovery recovery)))))
"####,
        expect![[
            r#"OK (:source (:upstream-sha256 "1dd4f983638ca555b9a432958dc93e3b62d13de8dc2ac5e3c3e0c88a85385f98" :installed-sha256 (("CHANGELOG.md" . "526336467b0a2e372159fde973176d133733f711045ee4413fdb5fb2b581b04b") ("clj-refactor-pkg.el" . "dd7c7b1ee267d0d01893eeae0b10c441189ba1f6639eea4a9678d4558e961e43") ("clj-refactor.el" . "2ad295ec89d3dc320d9f7dbcebe36a5a826eab47a56cce764807f1aec586b413")) :version "20260716.1545" :feature t) :result (:before (:text "(defn calculate [x]\n  (-> x inc))\n" :point 16 :modified t :mode clojure-mode :refactor-mode nil :post-hook nil :undo t) :failure (:error user-error :data ("CIDER isn’t connected") :message "CIDER isn’t connected") :connection-checks (rename-symbol) :failure-atomic t :recovery (:text "(defn calculate [x]\n  (->> x inc))\n" :point 30 :modified t :mode clojure-mode :refactor-mode nil :post-hook nil :undo t)) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :cleanup-errors nil))"#
        ]],
    )
}

#[test]
fn clj_refactor_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        public_mode_keybindings_and_changelog_lifecycle(),
        public_local_structural_refactorings_transform_real_code(),
        public_clean_ns_uses_offline_fallback_and_reports_message(),
        public_project_dependency_sort_preserves_comments_and_saves(),
        public_middleware_failure_is_exact_then_local_refactor_recovers(),
    ];
    assert_oracle_batch_cases(
        oracle(),
        "clj-refactor-rank414",
        "clj_refactor_parity",
        &cases,
    );
}
