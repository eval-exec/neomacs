//! Practical parity for docker-tramp's public docker TRAMP method.
//!
//! These cases register the docker method ahead of built-in tramp-container,
//! complete running containers through the registered parser, leave modern
//! tramp cache keys in place, and keep tramp-sh's wait-for-output on tramp 2.8.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, DOCKER_TRAMP_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)
(require 'tramp)
(require 'docker-tramp)
(set-window-configuration (current-window-configuration))

(defconst dt431-test-tree
  "7a4ed29d397f25c6355ba9c8cf213c3041aa5957")
(defconst dt431-test-manifest
  '(("docker-tramp-compat.el" . "e715925d5da0f7fa12b7a6b67e4d48398be9b1f78544c6bd5338df77bf7ba1c1")
    ("docker-tramp-pkg.el" . "99775794db5da3ba692d547632214354b3722cbe2dad728a44da925b83f0cbad")
    ("docker-tramp.el" . "42e969b26488183528a4459f2608e31e8fb343e16cb6c311a6c1233fded4ddae")))

(defvar dt431-test-case-index 0)
(defvar dt431-test-root nil)
(defvar dt431-test-root-owned nil)
(defvar dt431-test-process-plan nil)
(defvar dt431-test-process-ledger nil)

(defun dt431-test-sha (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun dt431-test-source-state ()
  (let* ((located (locate-library "docker-tramp.el"))
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
                         (cons file (dt431-test-sha
                                     (expand-file-name file directory))))
                       files))))
    (unless (and located main directory
                 (string-suffix-p "/docker-tramp.el" main)
                 (not (file-symlink-p located))
                 (equal files (mapcar #'car dt431-test-manifest)))
      (error "Unexpected installed docker-tramp payload: %S" (or manifest files)))
    (dolist (entry dt431-test-manifest)
      (let ((file (expand-file-name (car entry) directory))
            (expected (cdr entry)))
        (unless (and (file-regular-p file)
                     (not (file-symlink-p file))
                     (equal (dt431-test-sha file) expected))
          (error "Unexpected installed docker-tramp source: %S"
                 (cons entry manifest)))))
    (list :tree dt431-test-tree
          :manifest manifest
          :feature (featurep 'docker-tramp)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'docker-tramp package-alist)))))))

(defun dt431-test-condition (thunk)
  (condition-case condition
      (list :returned (funcall thunk))
    (error
     (list :error (car condition)
           :data (mapcar (lambda (item)
                           (if (stringp item)
                               (copy-sequence item)
                             (copy-tree item)))
                         (cdr condition))
           :message (copy-sequence (error-message-string condition))))))

(defun dt431-test-forbid-external (operation &rest arguments)
  (error "Unexpected docker-tramp external boundary: %S %S" operation arguments))

(defun dt431-test-process-lines (program &rest args)
  (push (list :program (copy-sequence program)
              :args (mapcar #'copy-sequence args))
        dt431-test-process-ledger)
  (cond
   ((eq dt431-test-process-plan :error)
    (error "Cannot connect to the Docker daemon"))
   ((null dt431-test-process-plan)
    (error "Unexpected docker-tramp process-lines: %S %S" program args))
   (t (pop dt431-test-process-plan))))

(defun dt431-test-docker-methods ()
  (mapcar
   (lambda (entry)
     (list :program (cadr (assq 'tramp-login-program (cdr entry)))
           :args (copy-tree (cadr (assq 'tramp-login-args (cdr entry))))
           :shell (cadr (assq 'tramp-remote-shell (cdr entry)))
           :shell-args (copy-tree
                        (cadr (assq 'tramp-remote-shell-args (cdr entry))))))
   (cl-remove-if-not (lambda (entry)
                       (equal (car entry) docker-tramp-method))
                     tramp-methods)))

(defun dt431-test-complete ()
  (let ((entry (assq 'docker-tramp--parse-running-containers
                     (tramp-get-completion-function docker-tramp-method))))
    (and entry (funcall (car entry) (cadr entry)))))

(defun dt431-test-cache-hosts ()
  (let (hosts)
    (maphash
     (lambda (key _value)
       (when (ignore-errors (tramp-file-name-method key))
         (push (list :method (copy-sequence (tramp-file-name-method key))
                     :host (copy-sequence (tramp-file-name-host key)))
               hosts)))
     tramp-cache-data)
    (sort hosts (lambda (left right)
                  (string< (format "%S" left) (format "%S" right))))))

(defun dt431-test-run (body)
  (let* ((index (cl-incf dt431-test-case-index))
         (sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (root (and sandbox
                    (file-name-as-directory
                     (expand-file-name (format "docker-tramp-%d" index)
                                       sandbox))))
         (buffer-before (current-buffer))
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (append timer-list timer-idle-list))
         (frames-before (frame-list))
         (selected-window-before (selected-window))
         (window-before (current-window-configuration))
         (source-before (dt431-test-source-state))
         (directory-before default-directory)
         (enable-local-before enable-local-variables)
         (debug-before debug-on-error)
         (print-circle-before print-circle)
         (methods-before tramp-methods)
         (completion-before tramp-completion-function-alist)
         (cache-before tramp-cache-data)
         (cache-changed-before tramp-cache-data-changed)
         (persistency-before tramp-persistency-file-name)
         (verbose-before tramp-verbose)
         (auth-before auth-sources)
         (executable-before docker-tramp-docker-executable)
         (options-before docker-tramp-docker-options)
         (use-names-before docker-tramp-use-names)
         (dt431-test-root root)
         (dt431-test-root-owned nil)
         (dt431-test-process-plan nil)
         (dt431-test-process-ledger nil)
         result body-error source-after cleanup-errors)
    (unwind-protect
        (condition-case condition
            (progn
              (unless (and root (file-name-absolute-p root))
                (error "Missing absolute docker-tramp sandbox root"))
              (when (file-exists-p root)
                (error "docker-tramp sandbox root exists: %S" root))
              (make-directory root)
              (setq dt431-test-root-owned t
                    enable-local-variables nil
                    debug-on-error nil
                    print-circle nil
                    default-directory root
                    tramp-verbose 0
                    auth-sources nil
                    tramp-persistency-file-name
                    (expand-file-name "tramp-persistency.el" root)
                    tramp-cache-data (make-hash-table :test 'equal)
                    tramp-cache-data-changed nil)
              (cl-letf (((symbol-function 'call-process)
                         (lambda (&rest args)
                           (apply #'dt431-test-forbid-external
                                  'call-process args)))
                        ((symbol-function 'call-process-region)
                         (lambda (&rest args)
                           (apply #'dt431-test-forbid-external
                                  'call-process-region args)))
                        ((symbol-function 'make-process)
                         (lambda (&rest args)
                           (apply #'dt431-test-forbid-external
                                  'make-process args)))
                        ((symbol-function 'start-process)
                         (lambda (&rest args)
                           (apply #'dt431-test-forbid-external
                                  'start-process args)))
                        ((symbol-function 'url-retrieve)
                         (lambda (&rest args)
                           (apply #'dt431-test-forbid-external
                                  'url-retrieve args)))
                        ((symbol-function 'url-retrieve-synchronously)
                         (lambda (&rest args)
                           (apply #'dt431-test-forbid-external
                                  'url-retrieve-synchronously args)))
                        ((symbol-function 'process-lines)
                         #'dt431-test-process-lines))
                (setq result (funcall body root)))
              (setq source-after (dt431-test-source-state))
              (unless (equal source-before source-after)
                (error "docker-tramp source changed")))
          (error (setq body-error
                       (list (car condition)
                             (copy-tree (cdr condition))))))
      (cl-labels
          ((attempt (label thunk)
             (condition-case condition
                 (funcall thunk)
               (error (push (list label (car condition)
                                  (copy-tree (cdr condition)))
                            cleanup-errors)))))
        (setq tramp-methods methods-before
              tramp-completion-function-alist completion-before
              tramp-cache-data cache-before
              tramp-cache-data-changed cache-changed-before
              tramp-persistency-file-name persistency-before
              tramp-verbose verbose-before
              auth-sources auth-before
              docker-tramp-docker-executable executable-before
              docker-tramp-docker-options options-before
              docker-tramp-use-names use-names-before
              enable-local-variables enable-local-before
              debug-on-error debug-before
              print-circle print-circle-before
              default-directory directory-before)
        (dolist (process (process-list))
          (unless (memq process processes-before)
            (attempt (list 'process (process-name process))
                     (lambda () (delete-process process)))))
        (dolist (buffer (buffer-list))
          (unless (memq buffer buffers-before)
            (attempt (list 'buffer (buffer-name buffer))
                     (lambda ()
                       (when (buffer-live-p buffer)
                         (with-current-buffer buffer
                           (set-buffer-modified-p nil))
                         (kill-buffer buffer))))))
        (dolist (timer (append timer-list timer-idle-list))
          (unless (memq timer timers-before)
            (attempt 'timer (lambda () (cancel-timer timer)))))
        (dolist (frame (frame-list))
          (unless (memq frame frames-before)
            (attempt 'frame (lambda () (delete-frame frame t)))))
        (attempt 'window
                 (lambda () (set-window-configuration window-before)))
        (when (window-live-p selected-window-before)
          (attempt 'selected
                   (lambda () (select-window selected-window-before))))
        (when (buffer-live-p buffer-before)
          (attempt 'current-buffer
                   (lambda () (set-buffer buffer-before))))
        (when dt431-test-root-owned
          (attempt 'root (lambda () (delete-directory root t))))))
    (when body-error
      (error "docker-tramp body failed: %S" body-error))
    (let ((cleanup
           (list :source-unchanged (equal source-before source-after)
                 :new-buffers (mapcar #'buffer-name
                                      (seq-remove
                                       (lambda (buffer)
                                         (memq buffer buffers-before))
                                       (buffer-list)))
                 :new-processes (length
                                 (seq-remove
                                  (lambda (process)
                                    (memq process processes-before))
                                  (process-list)))
                 :new-timers (length
                              (seq-remove
                               (lambda (timer)
                                 (memq timer timers-before))
                               (append timer-list timer-idle-list)))
                 :new-frames (length
                              (seq-remove
                               (lambda (frame)
                                 (memq frame frames-before))
                               (frame-list)))
                 :root-exists (and root (file-exists-p root))
                 :buffer-restored (eq (current-buffer) buffer-before)
                 :window-restored (eq (selected-window)
                                      selected-window-before)
                 :methods-restored (eq tramp-methods methods-before)
                 :cache-restored (eq tramp-cache-data cache-before)
                 :body-error body-error
                 :cleanup-errors (nreverse cleanup-errors))))
      (if cleanup-errors
          (error "docker-tramp cleanup failed: %S" (list result cleanup))
        (list :source source-before
              :result result
              :cleanup cleanup)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(DOCKER_TRAMP_MELPA_PIN, "docker-tramp.el")
        .expect("prepare pinned docker-tramp source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn registers_docker_method_ahead_of_builtin_container() -> ParityBatchCase {
    ParityBatchCase::value(
        "registers_docker_method_ahead_of_builtin_container",
        r####"
(dt431-test-run
 (lambda (_root)
   (let* ((loaded (dt431-test-docker-methods))
          (completion
           (mapcar #'car (tramp-get-completion-function docker-tramp-method)))
          (readded
           (progn
             (setq docker-tramp-docker-executable "podman"
                   docker-tramp-docker-options
                   '("--url" "unix:///run/dt431/podman.sock"))
             (docker-tramp-add-method)
             (dt431-test-docker-methods))))
     (list :method docker-tramp-method
           :loaded-count (length loaded)
           :loaded-head (car loaded)
           :builtin-present
           (and (cl-find-if
                 (lambda (method)
                   (equal (plist-get method :args)
                          '(("exec") ("-it") ("-u" "%u")
                            ("-e" "TERM=dumb") ("%h") ("%l"))))
                 loaded)
                t)
           :completion-has-parser
           (and (memq 'docker-tramp--parse-running-containers completion) t)
           :readded-count (length readded)
           :readded-head (car readded)))))
"####,
        expect![[
            r#"OK (:source (:tree "7a4ed29d397f25c6355ba9c8cf213c3041aa5957" :manifest (("docker-tramp-compat.el" . "e715925d5da0f7fa12b7a6b67e4d48398be9b1f78544c6bd5338df77bf7ba1c1") ("docker-tramp-pkg.el" . "99775794db5da3ba692d547632214354b3722cbe2dad728a44da925b83f0cbad") ("docker-tramp.el" . "42e969b26488183528a4459f2608e31e8fb343e16cb6c311a6c1233fded4ddae")) :feature t :version "20230809.511") :result (:method "docker" :loaded-count 2 :loaded-head (:program "docker" :args (nil ("exec" "-it") ("-u" "%u") ("%h") ("sh")) :shell "/bin/sh" :shell-args ("-i" "-c")) :builtin-present t :completion-has-parser t :readded-count 3 :readded-head (:program "podman" :args (("--url" "unix:///run/dt431/podman.sock") ("exec" "-it") ("-u" "%u") ("%h") ("sh")) :shell "/bin/sh" :shell-args ("-i" "-c"))) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :methods-restored t :cache-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn completes_running_containers_by_id_or_name() -> ParityBatchCase {
    ParityBatchCase::value(
        "completes_running_containers_by_id_or_name",
        r####"
(dt431-test-run
 (lambda (_root)
   (let* ((ps-lines
           '("CONTAINER ID IMAGE COMMAND CREATED STATUS PORTS NAMES"
             "abc123def456 alpine /bin/sh 2d Up web"
             "f00ba444dead nginx /usr/sbin/nginx 1d Up api-界"))
          (ids
           (progn
             (setq dt431-test-process-plan (list ps-lines)
                   docker-tramp-use-names nil)
             (dt431-test-complete)))
          (names
           (progn
             (setq dt431-test-process-plan (list ps-lines)
                   docker-tramp-use-names t
                   docker-tramp-docker-options
                   '("--host" "unix:///run/dt431/docker.sock"))
             (dt431-test-complete)))
          (failed
           (progn
             (setq dt431-test-process-plan :error
                   docker-tramp-use-names nil
                   docker-tramp-docker-options nil)
             (dt431-test-complete)))
          (recovered
           (progn
             (setq dt431-test-process-plan (list ps-lines))
             (dt431-test-complete))))
     (list :ids ids
           :names names
           :failed failed
           :recovered recovered
           :calls (nreverse dt431-test-process-ledger)))))
"####,
        expect![[
            r#"OK (:source (:tree "7a4ed29d397f25c6355ba9c8cf213c3041aa5957" :manifest (("docker-tramp-compat.el" . "e715925d5da0f7fa12b7a6b67e4d48398be9b1f78544c6bd5338df77bf7ba1c1") ("docker-tramp-pkg.el" . "99775794db5da3ba692d547632214354b3722cbe2dad728a44da925b83f0cbad") ("docker-tramp.el" . "42e969b26488183528a4459f2608e31e8fb343e16cb6c311a6c1233fded4ddae")) :feature t :version "20230809.511") :result (:ids (("" "abc123def456") ("" "f00ba444dead")) :names (("" "web") ("" "api-界")) :failed nil :recovered (("" "abc123def456") ("" "f00ba444dead")) :calls ((:program "docker" :args ("ps")) (:program "docker" :args ("--host" "unix:///run/dt431/docker.sock" "ps")) (:program "docker" :args ("ps")) (:program "docker" :args ("ps")))) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :methods-restored t :cache-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn cleanup_leaves_modern_tramp_cache_keys() -> ParityBatchCase {
    ParityBatchCase::value(
        "cleanup_leaves_modern_tramp_cache_keys",
        r####"
(dt431-test-run
 (lambda (root)
   (let* ((stale (tramp-dissect-file-name "/docker:gone:/tmp/x"))
          (live (tramp-dissect-file-name "/docker:web:/tmp/y"))
          (other (tramp-dissect-file-name "/ssh:host:/tmp/z"))
          persist)
     (puthash stale '(:stale t) tramp-cache-data)
     (puthash live '(:live t) tramp-cache-data)
     (puthash other '(:ssh t) tramp-cache-data)
     (write-region "seed\n" nil tramp-persistency-file-name nil 'silent)
     (setq persist tramp-persistency-file-name
           dt431-test-process-plan
           '(("CONTAINER ID IMAGE COMMAND CREATED STATUS PORTS NAMES"
              "abc123def456 alpine /bin/sh 2d Up web"))
           docker-tramp-use-names nil)
     (docker-tramp-cleanup)
     (list :key-type (type-of stale)
           :vectorp (and (vectorp stale) t)
           :hosts (dt431-test-cache-hosts)
           :stale-still (copy-tree (gethash stale tramp-cache-data))
           :changed tramp-cache-data-changed
           :persist-exists (and (file-exists-p persist) t)
           :persist-relative
           (file-relative-name persist root)
           :calls (nreverse dt431-test-process-ledger)))))
"####,
        expect![[
            r#"OK (:source (:tree "7a4ed29d397f25c6355ba9c8cf213c3041aa5957" :manifest (("docker-tramp-compat.el" . "e715925d5da0f7fa12b7a6b67e4d48398be9b1f78544c6bd5338df77bf7ba1c1") ("docker-tramp-pkg.el" . "99775794db5da3ba692d547632214354b3722cbe2dad728a44da925b83f0cbad") ("docker-tramp.el" . "42e969b26488183528a4459f2608e31e8fb343e16cb6c311a6c1233fded4ddae")) :feature t :version "20230809.511") :result (:key-type cons :vectorp nil :hosts ((:method "docker" :host "gone") (:method "docker" :host "web") (:method "ssh" :host "host")) :stale-still (:stale t) :changed t :persist-exists t :persist-relative "tramp-persistency.el" :calls ((:program "docker" :args ("ps")))) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :methods-restored t :cache-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn compat_is_noop_on_modern_tramp() -> ParityBatchCase {
    ParityBatchCase::value(
        "compat_is_noop_on_modern_tramp",
        r####"
(dt431-test-run
 (lambda (_root)
   (let* ((wait-before (symbol-function 'tramp-wait-for-output))
          (loaded (require 'docker-tramp-compat)))
     (list :feature loaded
           :tramp-version (copy-sequence tramp-version)
           :old-tramp (version< tramp-version "2.3")
           :escape-const (boundp 'tramp-device-escape-sequence-regexp)
           :wait-was-autoload (autoloadp wait-before)
           :wait-from-tramp-sh
           (let ((file (symbol-file 'tramp-wait-for-output)))
             (and file (string-match-p "tramp-sh" file) t))))))
"####,
        expect![[
            r#"OK (:source (:tree "7a4ed29d397f25c6355ba9c8cf213c3041aa5957" :manifest (("docker-tramp-compat.el" . "e715925d5da0f7fa12b7a6b67e4d48398be9b1f78544c6bd5338df77bf7ba1c1") ("docker-tramp-pkg.el" . "99775794db5da3ba692d547632214354b3722cbe2dad728a44da925b83f0cbad") ("docker-tramp.el" . "42e969b26488183528a4459f2608e31e8fb343e16cb6c311a6c1233fded4ddae")) :feature t :version "20230809.511") :result (:feature docker-tramp-compat :tramp-version "2.8.2-pre" :old-tramp nil :escape-const nil :wait-was-autoload nil :wait-from-tramp-sh t) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :methods-restored t :cache-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

#[test]
fn docker_tramp_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        registers_docker_method_ahead_of_builtin_container(),
        completes_running_containers_by_id_or_name(),
        cleanup_leaves_modern_tramp_cache_keys(),
        compat_is_noop_on_modern_tramp(),
    ];
    assert_oracle_batch_cases(
        oracle(),
        "docker-tramp-rank431",
        "docker_tramp_parity",
        &cases,
    );
}
