//! Practical parity coverage for rank 411 `tree-sitter-langs`.
//!
//! The corpus keeps grammar loading, parser construction, and release download
//! at explicit native/environmental boundaries. Package-owned registration,
//! query composition, one-shot advice, installation, skip, reinstall, and
//! recovery behavior all run through the pinned public integration routes.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, TREE_SITTER_LANGS_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)

;; Loading the pinned `tsc' package normally acquires a native dynamic module.
;; That acquisition is outside this package's logic and outside the parity
;; corpus.  Supply the narrow API presence needed while loading the two Elisp
;; integration layers; individual workflows own exact native-call plans.
(defconst tsc-dyn--version "0.19.4")
(defun tsc--lang-symbol (language) language)
(defmacro tsc--without-restriction (&rest body)
  `(save-restriction (widen) ,@body))
(provide 'tsc)

;; Loading from an archive must not perform the package's eager release
;; download or inspect an unrelated Git checkout.
(setq tree-sitter-langs--testing t
      tree-sitter-langs-git-dir nil
      tree-sitter-major-mode-language-table (make-hash-table :test 'eq)
      tree-sitter-languages nil)

(defconst tsl411-test-source-count 95)
(defconst tsl411-test-source-manifest-sha
  "23d92dace39eb0248073b9478c8db8803393d1ed926b218b95b6ede18b6b9dc8")
(defconst tsl411-test-source-files
  '(("tree-sitter-langs.el" . "54bffe2106828f9c03aedff52093c678e1a2e29d17be2ad8877dff1939fd3bdb")
    ("tree-sitter-langs-build.el" . "29d9214a760a8f52f3fbdf7bbcb7ecc56fe3d2186c3f48a19ae477f348becb2a")
    ("tree-sitter-langs-pkg.el" . "fe6941fb925ad9ec5c8287e0934b6acb81ae716ca4e8b4ee4bb9e308ab68c1bd")))

(defvar tsl411-test-root nil)
(defvar tsl411-test-real-make-process nil)
(defvar tsl411-test-real-delete-file nil)
(defvar tsl411-test-download-plan nil)
(defvar tsl411-test-process-plan nil)
(defvar tsl411-test-boundaries nil)

(defun tsl411-test-file-sha256 (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun tsl411-test-source-state ()
  (let* ((source (file-truename (locate-library "tree-sitter-langs.el")))
         (directory (file-name-directory source))
         (files
          (sort
           (append
            (mapcar (lambda (name) (expand-file-name name directory))
                    (mapcar #'car tsl411-test-source-files))
            (directory-files-recursively
             (expand-file-name "queries" directory) "\\.scm\\'"))
           #'string<))
         (manifest
          (mapconcat
           (lambda (file)
             (format "%s\t%d\t%s\n"
                     (file-relative-name file directory)
                     (file-attribute-size (file-attributes file))
                     (tsl411-test-file-sha256 file)))
           files ""))
         (digest (secure-hash 'sha256 manifest)))
    (unless (and (= (length files) tsl411-test-source-count)
                 (equal digest tsl411-test-source-manifest-sha)
                 (cl-every
                  (lambda (entry)
                    (equal (tsl411-test-file-sha256
                            (expand-file-name (car entry) directory))
                           (cdr entry)))
                  tsl411-test-source-files))
      (error "Tree-sitter Languages source manifest mismatch: %S"
             (list (length files) digest)))
    (list :count (length files)
          :manifest digest
          :main (cdr (assoc "tree-sitter-langs.el" tsl411-test-source-files))
          :build (cdr (assoc "tree-sitter-langs-build.el"
                             tsl411-test-source-files)))))

(defun tsl411-test-condition (condition)
  (list :symbol (car condition)
        :data (copy-tree (cdr condition))
        :message (error-message-string condition)))

(defun tsl411-test-window-state ()
  (mapcan
   (lambda (frame)
     (mapcar (lambda (window)
               (list (window-buffer window)
                     (window-point window)
                     (window-start window)))
             (window-list frame 'nomini)))
   (frame-list)))

(defun tsl411-test-write-file (file contents)
  (unless (and tsl411-test-root
               (file-name-absolute-p file)
               (file-in-directory-p file tsl411-test-root))
    (error "Refusing Tree-sitter Languages write outside owned root: %s" file))
  (make-directory (file-name-directory file) t)
  (let ((file-name-handler-alist nil))
    (with-temp-file file
      (set-buffer-multibyte nil)
      (insert contents)))
  file)

(defun tsl411-test-relative (file)
  (file-relative-name file tsl411-test-root))

(defun tsl411-test-url-copy-file (url destination &optional ok-if-exists _time)
  (let* ((step (pop tsl411-test-download-plan))
         (absolute-destination (expand-file-name destination default-directory)))
    (unless (and step
                 (stringp url)
                 (string= url
                          "https://github.com/emacs-tree-sitter/tree-sitter-langs/releases/download/0.13.75/tree-sitter-grammars.x86_64-unknown-linux-gnu.v0.13.75.tar.gz")
                 (file-in-directory-p absolute-destination tsl411-test-root)
                 (equal (file-name-directory absolute-destination)
                        (file-name-as-directory default-directory))
                 (eq ok-if-exists 'ok-if-already-exists))
      (error "Unexpected grammar download: %S"
             (list step url destination ok-if-exists)))
    (push (list :kind 'download :action step :url url
                :destination (tsl411-test-relative absolute-destination))
          tsl411-test-boundaries)
    (pcase step
      ('fail (signal 'file-error (list "Recorded grammar download failed" url)))
      ('ok (tsl411-test-write-file absolute-destination
                                   "recorded grammar bundle\n"))
      (_ (error "Invalid grammar download plan: %S" step)))))

(defun tsl411-test-make-process (&rest arguments)
  (let* ((step (pop tsl411-test-process-plan))
         (command (plist-get arguments :command))
         (bundle (and (consp command) (nth 2 command)))
         (bin-dir (tree-sitter-langs--bin-dir)))
    (unless (and (eq step 'extract)
                 (equal (plist-get arguments :name) "tar")
                 (equal (car command) "tar")
                 (equal (cadr command) "-xvzf")
                 (= (length command) 3)
                 (functionp (plist-get arguments :filter))
                 (not (plist-member arguments :buffer))
                 (stringp bundle)
                 (file-in-directory-p bundle tsl411-test-root)
                 (equal (file-truename default-directory)
                        (file-truename bin-dir))
                 (equal (tsl411-test-file-sha256 bundle)
                        "9d9a4cca42b629d3cc2f2e8adb8432a124ae459476354e7cfc6faafd59c93dc5")
                 (member (format "TREE_SITTER_LIBDIR=%s" bin-dir)
                         process-environment))
      (error "Unexpected grammar extraction: %S"
             (list step command default-directory process-environment)))
    (tsl411-test-write-file (expand-file-name "python.so" bin-dir)
                            "python grammar 0.13.75\n")
    (tsl411-test-write-file (expand-file-name "javascript.so" bin-dir)
                            "javascript grammar 0.13.75\n")
    (push (list :kind 'process
                :command (list "tar" "-xvzf" (file-name-nondirectory bundle))
                :directory (tsl411-test-relative default-directory)
                :libdir (tsl411-test-relative bin-dir)
                :filter t)
          tsl411-test-boundaries)
    (funcall tsl411-test-real-make-process
             :name (format "tsl411-recorded-tar-%d"
                           (length tsl411-test-boundaries))
             :command (list (or (executable-find "true")
                                (error "Missing exact no-output process helper")))
             :noquery t)))

(defun tsl411-test-delete-file (file &optional _trash)
  (let ((absolute-file (expand-file-name file default-directory)))
    (unless (and (file-in-directory-p absolute-file tsl411-test-root)
                 (equal (file-name-directory absolute-file)
                        (file-name-as-directory default-directory))
                 (file-regular-p absolute-file)
                 (not (file-symlink-p absolute-file)))
      (error "Unexpected Tree-sitter Languages deletion: %s" file))
    (push (list :kind 'delete :file (tsl411-test-relative absolute-file))
          tsl411-test-boundaries)
    (funcall tsl411-test-real-delete-file absolute-file nil)))

(defun tsl411-test-forbid-external (kind &rest arguments)
  (error "Unexpected external boundary: %S" (cons kind arguments)))

(defun tsl411-test-root-manifest ()
  (when (file-directory-p tsl411-test-root)
    (mapcar
     (lambda (file)
       (list (tsl411-test-relative file)
             (file-attribute-size (file-attributes file))
             (tsl411-test-file-sha256 file)))
     (sort (directory-files-recursively tsl411-test-root "." nil)
           #'string<))))

(defun tsl411-test-restore-advice (symbol function present)
  (let ((current (and (advice-member-p function symbol) t)))
    (cond
     ((and present (not current)) (advice-add symbol :before function))
     ((and (not present) current) (advice-remove symbol function)))))

(defun tsl411-test-run (body)
  (let* ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (root (and sandbox
                    (file-name-as-directory
                     (expand-file-name "tree-sitter-langs/" sandbox))))
         (buffer-before (current-buffer))
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (copy-sequence timer-list))
         (frames-before (frame-list))
         (window-before (current-window-configuration))
         (window-state-before (tsl411-test-window-state))
         (source-before (tsl411-test-source-state))
         (table-before (copy-hash-table tree-sitter-major-mode-language-table))
         (languages-before (copy-tree tree-sitter-languages))
         (load-path-before (copy-sequence tree-sitter-load-path))
         (load-advice-before
          (and (advice-member-p #'tree-sitter-langs--init-load-path
                                'tree-sitter-load) t))
         (mode-advice-before
          (and (advice-member-p #'tree-sitter-langs--init-major-mode-table
                                'tree-sitter--setup) t))
         (hl-advice-before
          (and (advice-member-p #'tree-sitter-langs--set-hl-default-patterns
                                'tree-sitter-hl--setup) t))
         (message-log-max nil)
         (print-circle nil)
         (auto-save-default nil)
         (create-lockfiles nil)
         (make-backup-files nil)
         (tree-sitter-major-mode-language-table (copy-hash-table table-before))
         (tree-sitter-languages (copy-tree languages-before))
         (tree-sitter-load-path (copy-sequence load-path-before))
         (tree-sitter-langs-git-dir nil)
         (tree-sitter-langs--out nil)
         (tree-sitter-langs-grammar-dir root)
         (temporary-file-directory (expand-file-name "tmp/" root))
         (tsl411-test-root root)
         (tsl411-test-real-make-process (symbol-function 'make-process))
         (tsl411-test-real-delete-file (symbol-function 'delete-file))
         (tsl411-test-download-plan nil)
         (tsl411-test-process-plan nil)
         (tsl411-test-boundaries nil)
         (root-owned nil)
         result body-error cleanup-errors source-after)
    (unwind-protect
        (condition-case condition
            (progn
              (unless (and root (file-name-absolute-p root))
                (error "Missing absolute Tree-sitter Languages sandbox root"))
              (when (file-exists-p root)
                (error "Tree-sitter Languages sandbox root exists: %s" root))
              (make-directory root)
              (setq root-owned t)
              (make-directory temporary-file-directory)
              (setq result
                    (cl-letf (((symbol-function 'url-copy-file)
                               #'tsl411-test-url-copy-file)
                              ((symbol-function 'make-process)
                               #'tsl411-test-make-process)
                              ((symbol-function 'delete-file)
                               #'tsl411-test-delete-file)
                              ((symbol-function 'call-process)
                               (lambda (&rest args)
                                 (apply #'tsl411-test-forbid-external
                                        'call-process args)))
                              ((symbol-function 'process-file)
                               (lambda (&rest args)
                                 (apply #'tsl411-test-forbid-external
                                        'process-file args)))
                              ((symbol-function 'start-process)
                               (lambda (&rest args)
                                 (apply #'tsl411-test-forbid-external
                                        'start-process args)))
                              ((symbol-function 'start-file-process)
                               (lambda (&rest args)
                                 (apply #'tsl411-test-forbid-external
                                        'start-file-process args)))
                              ((symbol-function 'url-retrieve-synchronously)
                               (lambda (&rest args)
                                 (apply #'tsl411-test-forbid-external
                                        'url-retrieve-synchronously args))))
                      (funcall body root)))
              (unless (and (null tsl411-test-download-plan)
                           (null tsl411-test-process-plan))
                (error "Unused Tree-sitter Languages boundary plan: %S"
                       (list tsl411-test-download-plan
                             tsl411-test-process-plan)))
              (setq source-after (tsl411-test-source-state))
              (unless (equal source-before source-after)
                (error "Tree-sitter Languages package source changed")))
          (error (setq body-error (tsl411-test-condition condition))))
      (dolist (process (process-list))
        (unless (memq process processes-before)
          (condition-case condition (delete-process process)
            (error (push (tsl411-test-condition condition) cleanup-errors)))))
      (dolist (buffer (buffer-list))
        (unless (memq buffer buffers-before)
          (condition-case condition (kill-buffer buffer)
            (error (push (tsl411-test-condition condition) cleanup-errors)))))
      (dolist (timer (copy-sequence timer-list))
        (unless (memq timer timers-before)
          (condition-case condition (cancel-timer timer)
            (error (push (tsl411-test-condition condition) cleanup-errors)))))
      (dolist (frame (frame-list))
        (unless (memq frame frames-before)
          (condition-case condition (delete-frame frame t)
            (error (push (tsl411-test-condition condition) cleanup-errors)))))
      (condition-case condition
          (tsl411-test-restore-advice
           'tree-sitter-load #'tree-sitter-langs--init-load-path
           load-advice-before)
        (error (push (tsl411-test-condition condition) cleanup-errors)))
      (condition-case condition
          (tsl411-test-restore-advice
           'tree-sitter--setup #'tree-sitter-langs--init-major-mode-table
           mode-advice-before)
        (error (push (tsl411-test-condition condition) cleanup-errors)))
      (condition-case condition
          (tsl411-test-restore-advice
           'tree-sitter-hl--setup #'tree-sitter-langs--set-hl-default-patterns
           hl-advice-before)
        (error (push (tsl411-test-condition condition) cleanup-errors)))
      (condition-case condition (set-window-configuration window-before)
        (error (push (tsl411-test-condition condition) cleanup-errors)))
      (when (buffer-live-p buffer-before) (set-buffer buffer-before))
      (when root-owned
        (condition-case condition (delete-directory root t)
          (error (push (tsl411-test-condition condition) cleanup-errors)))))
    (let ((cleanup
           (list :source-unchanged (equal source-before source-after)
                 :new-buffers
                 (mapcar #'buffer-name
                         (seq-filter
                          (lambda (buffer)
                            (and (buffer-live-p buffer)
                                 (not (memq buffer buffers-before))))
                          (buffer-list)))
                 :new-processes
                 (length (seq-remove
                          (lambda (process) (memq process processes-before))
                          (process-list)))
                 :new-timers
                 (length (seq-remove
                          (lambda (timer) (memq timer timers-before))
                          timer-list))
                 :new-frames
                 (length (seq-remove
                          (lambda (frame) (memq frame frames-before))
                          (frame-list)))
                 :root-exists (and root (file-exists-p root))
                 :window-restored
                 (equal window-state-before (tsl411-test-window-state))
                 :buffer-restored (eq (current-buffer) buffer-before)
                 :body-error body-error
                 :cleanup-errors (nreverse cleanup-errors))))
      (if (or body-error cleanup-errors)
          (error "Tree-sitter Languages workflow failed: %S"
                 (list result cleanup))
        (list :source source-before
              :result result
              :boundaries (nreverse tsl411-test-boundaries)
              :cleanup cleanup)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(TREE_SITTER_LANGS_MELPA_PIN, "tree-sitter-langs.el")
        .expect("prepare exact Tree-sitter Languages source and dependency closure below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn public_tree_sitter_mode_initializes_bundle_mappings_once() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_tree_sitter_mode_initializes_bundle_mappings_once",
        r####"
(tsl411-test-run
 (lambda (_root)
   (with-temp-buffer
     (setq tree-sitter-major-mode-language-table (make-hash-table :test 'eq))
     (puthash 'python-mode 'owned-python tree-sitter-major-mode-language-table)
     (setq major-mode 'js-mode)
     (advice-add 'tree-sitter--setup :before
                 #'tree-sitter-langs--init-major-mode-table)
     (let (native-calls)
       (cl-letf (((symbol-function 'tree-sitter-require)
                  (lambda (language &rest args)
                    (push (list language (copy-tree args)) native-calls)
                    (list :language language)))
                 ((symbol-function 'tsc-make-parser)
                  (lambda () :parser))
                 ((symbol-function 'tsc-set-language)
                  (lambda (parser language)
                    (unless (and (eq parser :parser)
                                 (equal language '(:language javascript)))
                      (error "Unexpected parser language: %S"
                             (list parser language)))))
                 ((symbol-function 'tsc-parse-chunks)
                  (lambda (parser input old-tree)
                    (unless (and (eq parser :parser)
                                 (eq input #'tsc--buffer-input)
                                 (null old-tree))
                      (error "Unexpected parse call: %S"
                             (list parser input old-tree)))
                    :tree)))
         (insert "const café = 1;\n")
         (tree-sitter-mode 1)
         (let ((enabled
                (list :mode tree-sitter-mode
                      :language tree-sitter-language
                      :parser tree-sitter-parser
                      :tree tree-sitter-tree
                      :before-hook
                      (and (memq #'tree-sitter--before-change
                                 before-change-functions) t)
                      :after-hook
                      (and (memq #'tree-sitter--after-change
                                 after-change-functions) t))))
           (tree-sitter-mode -1)
           (list
            :enabled enabled
            :disabled
            (list tree-sitter-mode tree-sitter-language
                  tree-sitter-parser tree-sitter-tree
                  (and (memq #'tree-sitter--before-change
                             before-change-functions) t)
                  (and (memq #'tree-sitter--after-change
                             after-change-functions) t))
            :mappings
            (list :count (hash-table-count
                          tree-sitter-major-mode-language-table)
                  :existing
                  (gethash 'python-mode
                           tree-sitter-major-mode-language-table)
                  :javascript
                  (gethash 'js-mode
                           tree-sitter-major-mode-language-table)
                  :tsx
                  (gethash 'typescript-tsx-mode
                           tree-sitter-major-mode-language-table)
                  :unicode-mode
                  (gethash 'racket-mode
                           tree-sitter-major-mode-language-table))
            :native-calls (nreverse native-calls)
            :advice-removed
            (not (advice-member-p
                  #'tree-sitter-langs--init-major-mode-table
                  'tree-sitter--setup)))))))))
"####,
        expect![[
            r#"OK (:source (:count 95 :manifest "23d92dace39eb0248073b9478c8db8803393d1ed926b218b95b6ede18b6b9dc8" :main "54bffe2106828f9c03aedff52093c678e1a2e29d17be2ad8877dff1939fd3bdb" :build "29d9214a760a8f52f3fbdf7bbcb7ecc56fe3d2186c3f48a19ae477f348becb2a") :result (:enabled (:mode t :language (:language javascript) :parser :parser :tree :tree :before-hook t :after-hook t) :disabled (nil nil nil nil nil nil) :mappings (:count 140 :existing owned-python :javascript javascript :tsx tsx :unicode-mode racket) :native-calls ((javascript nil)) :advice-removed t) :boundaries nil :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_require_discovers_owned_grammar_and_caches_language() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_require_discovers_owned_grammar_and_caches_language",
        r####"
(tsl411-test-run
 (lambda (root)
   (let* ((bin (tree-sitter-langs--bin-dir))
          (grammar (expand-file-name "python.so" bin))
          (tree-sitter-languages nil)
          (tree-sitter-load-path
           (list (expand-file-name "missing/" root)))
          native-calls)
     (tsl411-test-write-file grammar "owned python grammar\n")
     (advice-add 'tree-sitter-load :before
                 #'tree-sitter-langs--init-load-path)
     (cl-letf (((symbol-function 'tsc--load-language)
                (lambda (file native-name language)
                  (unless (and (equal (file-truename file)
                                      (file-truename grammar))
                               (equal native-name "tree_sitter_python")
                               (eq language 'python))
                    (error "Unexpected native grammar load: %S"
                           (list file native-name language)))
                  (push (list (tsl411-test-relative file)
                              native-name language)
                        native-calls)
                  (list :loaded language))))
       (let ((first (tree-sitter-require 'python))
             (second (tree-sitter-require 'python)))
         (list :first first
               :second second
               :same-object (eq first second)
               :registered (copy-tree tree-sitter-languages)
               :load-path
               (mapcar (lambda (path)
                         (if (file-in-directory-p path root)
                             (tsl411-test-relative path)
                           path))
                       tree-sitter-load-path)
               :native-calls (nreverse native-calls)
               :advice-removed
               (not (advice-member-p
                     #'tree-sitter-langs--init-load-path
                     'tree-sitter-load))))))))
"####,
        expect![[
            r#"OK (:source (:count 95 :manifest "23d92dace39eb0248073b9478c8db8803393d1ed926b218b95b6ede18b6b9dc8" :main "54bffe2106828f9c03aedff52093c678e1a2e29d17be2ad8877dff1939fd3bdb" :build "29d9214a760a8f52f3fbdf7bbcb7ecc56fe3d2186c3f48a19ae477f348becb2a") :result (:first #1=(:loaded python) :second #1# :same-object t :registered ((python :loaded python)) :load-path ("bin/" "missing/") :native-calls (("bin/python.so" "tree_sitter_python" python)) :advice-removed t) :boundaries nil :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_highlight_mode_composes_mode_specific_queries_and_preserves_override() -> ParityBatchCase
{
    ParityBatchCase::value(
        "public_highlight_mode_composes_mode_specific_queries_and_preserves_override",
        r####"
(tsl411-test-run
 (lambda (_root)
   (with-temp-buffer
     (setq major-mode 'terraform-mode)
     (font-lock-mode 1)
     (setq-local tree-sitter-mode t
                 tree-sitter-language 'hcl
                 tree-sitter-hl-default-patterns nil
                 tree-sitter-hl--query nil
                 tree-sitter-hl--query-cursor nil)
     (let (query-calls first-patterns)
       (cl-letf (((symbol-function 'tsc--lang-symbol)
                  (lambda (language) language))
                 ((symbol-function 'tsc--stringify-patterns)
                  (lambda (patterns) patterns))
                 ((symbol-function 'tsc-make-query)
                  (lambda (language patterns &optional mapper)
                    (push (list language
                                (length patterns)
                                (secure-hash 'sha256 patterns)
                                (and (functionp mapper) t))
                          query-calls)
                    (list :query (length query-calls))))
                 ((symbol-function 'tsc-make-query-cursor)
                  (lambda () :cursor)))
         (tree-sitter-hl-mode 1)
         (setq first-patterns
               (substring-no-properties tree-sitter-hl-default-patterns))
         (let ((first
                (list :mode tree-sitter-hl-mode
                      :length (length first-patterns)
                      :sha (secure-hash 'sha256 first-patterns)
                      :mode-before-base
                      (< (string-match-p "data\\.aws" first-patterns)
                         (string-match-p "(\"for\" @keyword"
                                         first-patterns))
                      :cursor tree-sitter-hl--query-cursor)))
           (tree-sitter-hl-mode -1)
           (setq-local tree-sitter-hl-default-patterns
                       "((identifier) @variable.special)"
                       tree-sitter-hl--query nil)
           (tree-sitter-hl-mode 1)
           (let ((override
                  (list :patterns tree-sitter-hl-default-patterns
                        :mode tree-sitter-hl-mode
                        :cursor tree-sitter-hl--query-cursor)))
             (tree-sitter-hl-mode -1)
             (list :first first
                   :override override
                   :query-calls (nreverse query-calls)
                   :disabled
                   (list tree-sitter-hl-mode
                         tree-sitter-hl--query
                         tree-sitter-hl--query-cursor)))))))))
"####,
        expect![[
            r#"OK (:source (:count 95 :manifest "23d92dace39eb0248073b9478c8db8803393d1ed926b218b95b6ede18b6b9dc8" :main "54bffe2106828f9c03aedff52093c678e1a2e29d17be2ad8877dff1939fd3bdb" :build "29d9214a760a8f52f3fbdf7bbcb7ecc56fe3d2186c3f48a19ae477f348becb2a") :result (:first (:mode t :length 3409 :sha "04c6a0a971ffb3bf314f7b33c628df705bda4395f344c48fd001e44fd1eab40f" :mode-before-base t :cursor :cursor) :override (:patterns "((identifier) @variable.special)" :mode t :cursor :cursor) :query-calls ((hcl 3409 "04c6a0a971ffb3bf314f7b33c628df705bda4395f344c48fd001e44fd1eab40f" t) (hcl 32 "b023e0346dc53d639b870a25cf52c0cbb81d7e305c5e079e492b2fbda07008c7" t)) :disabled (nil nil nil)) :boundaries nil :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_install_recovers_then_skips_and_reinstalls_exact_bundle() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_install_recovers_then_skips_and_reinstalls_exact_bundle",
        r####"
(tsl411-test-run
 (lambda (root)
   (setq tsl411-test-download-plan '(fail ok ok)
         tsl411-test-process-plan '(extract extract))
   (let* ((bin (tree-sitter-langs--bin-dir))
          (version-file (expand-file-name tree-sitter-langs--bundle-version-file
                                          bin))
          failure failed-state first-state skip-state)
     (condition-case condition
         (tree-sitter-langs-install-grammars
          nil "0.13.75" "linux" nil)
       (error (setq failure (tsl411-test-condition condition))))
     (setq failed-state
           (list :version (file-exists-p version-file)
                 :manifest (tsl411-test-root-manifest)))
     (tree-sitter-langs-install-grammars nil "0.13.75" "linux" nil)
     (setq first-state
           (list :version
                 (with-temp-buffer
                   (insert-file-contents version-file)
                   (buffer-string))
                 :manifest (tsl411-test-root-manifest)
                 :boundary-count (length tsl411-test-boundaries)))
     (tree-sitter-langs-install-grammars t "0.13.75" "linux" nil)
     (setq skip-state
           (list :manifest (tsl411-test-root-manifest)
                 :boundary-count (length tsl411-test-boundaries)))
     (tree-sitter-langs-install-grammars nil "0.13.75" "linux" nil)
     (list :failure failure
           :failed failed-state
           :first first-state
           :skip skip-state
           :reinstall
           (list :version
                 (with-temp-buffer
                   (insert-file-contents version-file)
                   (buffer-string))
                 :manifest (tsl411-test-root-manifest)
                 :bundle-left
                 (directory-files bin nil "\\.tar\\.gz\\'"))))))
"####,
        expect![[
            r#"OK (:source (:count 95 :manifest "23d92dace39eb0248073b9478c8db8803393d1ed926b218b95b6ede18b6b9dc8" :main "54bffe2106828f9c03aedff52093c678e1a2e29d17be2ad8877dff1939fd3bdb" :build "29d9214a760a8f52f3fbdf7bbcb7ecc56fe3d2186c3f48a19ae477f348becb2a") :result (:failure (:symbol file-error :data ("Recorded grammar download failed" "https://github.com/emacs-tree-sitter/tree-sitter-langs/releases/download/0.13.75/tree-sitter-grammars.x86_64-unknown-linux-gnu.v0.13.75.tar.gz") :message "Recorded grammar download failed: https://github.com/emacs-tree-sitter/tree-sitter-langs/releases/download/0.13.75/tree-sitter-grammars.x86_64-unknown-linux-gnu.v0.13.75.tar.gz") :failed (:version nil :manifest nil) :first (:version "0.13.75" :manifest (("bin/BUNDLE-VERSION" 7 "967663d369af99e58df82b1d138d06f1efa854e81dc3df2555182a6587e3e028") ("bin/javascript.so" 27 "095f17ab80d2966b215826f3bed0a261f266dfd42fe7db29abf9a4f22b4a3ff1") ("bin/python.so" 23 "50b6a4a6ab7acfd47380edc4051c79dcd1b06f15decffafd27da89ec740b9adb")) :boundary-count 4) :skip (:manifest (("bin/BUNDLE-VERSION" 7 "967663d369af99e58df82b1d138d06f1efa854e81dc3df2555182a6587e3e028") ("bin/javascript.so" 27 "095f17ab80d2966b215826f3bed0a261f266dfd42fe7db29abf9a4f22b4a3ff1") ("bin/python.so" 23 "50b6a4a6ab7acfd47380edc4051c79dcd1b06f15decffafd27da89ec740b9adb")) :boundary-count 4) :reinstall (:version "0.13.75" :manifest (("bin/BUNDLE-VERSION" 7 "967663d369af99e58df82b1d138d06f1efa854e81dc3df2555182a6587e3e028") ("bin/javascript.so" 27 "095f17ab80d2966b215826f3bed0a261f266dfd42fe7db29abf9a4f22b4a3ff1") ("bin/python.so" 23 "50b6a4a6ab7acfd47380edc4051c79dcd1b06f15decffafd27da89ec740b9adb")) :bundle-left nil)) :boundaries ((:kind download :action fail :url "https://github.com/emacs-tree-sitter/tree-sitter-langs/releases/download/0.13.75/tree-sitter-grammars.x86_64-unknown-linux-gnu.v0.13.75.tar.gz" :destination "bin/tree-sitter-grammars.x86_64-unknown-linux-gnu.v0.13.75.tar.gz") (:kind download :action ok :url "https://github.com/emacs-tree-sitter/tree-sitter-langs/releases/download/0.13.75/tree-sitter-grammars.x86_64-unknown-linux-gnu.v0.13.75.tar.gz" :destination "bin/tree-sitter-grammars.x86_64-unknown-linux-gnu.v0.13.75.tar.gz") (:kind process :command ("tar" "-xvzf" "tree-sitter-grammars.x86_64-unknown-linux-gnu.v0.13.75.tar.gz") :directory "bin/" :libdir "bin/" :filter t) (:kind delete :file "bin/tree-sitter-grammars.x86_64-unknown-linux-gnu.v0.13.75.tar.gz") (:kind download :action ok :url "https://github.com/emacs-tree-sitter/tree-sitter-langs/releases/download/0.13.75/tree-sitter-grammars.x86_64-unknown-linux-gnu.v0.13.75.tar.gz" :destination "bin/tree-sitter-grammars.x86_64-unknown-linux-gnu.v0.13.75.tar.gz") (:kind process :command ("tar" "-xvzf" "tree-sitter-grammars.x86_64-unknown-linux-gnu.v0.13.75.tar.gz") :directory "bin/" :libdir "bin/" :filter t) (:kind delete :file "bin/tree-sitter-grammars.x86_64-unknown-linux-gnu.v0.13.75.tar.gz")) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

#[test]
fn tree_sitter_langs_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        public_tree_sitter_mode_initializes_bundle_mappings_once(),
        public_require_discovers_owned_grammar_and_caches_language(),
        public_highlight_mode_composes_mode_specific_queries_and_preserves_override(),
        public_install_recovers_then_skips_and_reinstalls_exact_bundle(),
    ];
    assert_oracle_batch_cases(
        oracle(),
        "tree-sitter-langs-rank411",
        "Tree-sitter Languages",
        &cases,
    );
}
