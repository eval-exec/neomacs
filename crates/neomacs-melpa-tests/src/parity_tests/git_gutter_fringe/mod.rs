use std::time::Duration;

use crate::{CachedMelpaOracle, GIT_GUTTER_FRINGE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const TEST_TIMEOUT: Duration = Duration::from_secs(240);

const TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)
(require 'git-gutter-fringe)

;; GNU uses this persistent internal buffer while encoding Unicode process and
;; file names.  Establish it as shared oracle infrastructure before any case
;; records its buffer baseline; cases must not own or hide editor caches.
(encode-coding-string "Ω" 'utf-8)

(defconst ggf351-test-baseline
  "alpha\nbeta\ngamma\ndelta\nepsilon\nzeta\neta\ntheta\niota\nkappa\n")

(defconst ggf351-test-mixed
  "alpha\nBETA changed\ngamma\ndelta\nzeta\neta\ntheta\nadded one\nadded two\niota\nkappa\n")

(defvar ggf351-test-owned-processes nil)
(defvar ggf351-test-owned-process-buffers nil)
(defvar ggf351-test-real-git nil)

(defun ggf351-test-root (name)
  "Return a new owned test root for NAME, failing closed on unsafe input."
  (let* ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (workspace (getenv "NEOMACS_TEST_WORKSPACE_ROOT"))
         (approved (and workspace
                        (file-name-as-directory
                         (expand-file-name "tmp" workspace)))))
    (unless (and (stringp sandbox) (> (length sandbox) 1)
                 (file-name-absolute-p sandbox)
                 approved
                 (file-in-directory-p (file-truename sandbox)
                                      (file-truename approved)))
      (error "GGF351: unsafe sandbox root %S below approved %S"
             sandbox approved))
    (let ((root (expand-file-name (concat "git-gutter-fringe-" name) sandbox)))
      (when (file-exists-p root)
        (error "GGF351: owned root already exists: %s" root))
      root)))

(defun ggf351-test-git (project &rest arguments)
  "Run real Git with ARGUMENTS in PROJECT and return its exact stdout."
  (unless (and (stringp ggf351-test-real-git)
               (file-name-absolute-p ggf351-test-real-git)
               (file-executable-p ggf351-test-real-git))
    (error "GGF351: real Git executable was not established: %S"
           ggf351-test-real-git))
  (let ((default-directory (file-name-as-directory project)))
    (with-temp-buffer
      (let ((status (apply #'process-file ggf351-test-real-git
                           nil (list t t) nil arguments)))
        (unless (zerop status)
          (error "GGF351: git %S failed (%s): %s"
                 arguments status (buffer-string)))
        (buffer-substring-no-properties (point-min) (point-max))))))

(defun ggf351-test-write (file contents)
  "Write CONTENTS to FILE as deterministic UTF-8 Unix text."
  (make-directory (file-name-directory file) t)
  (let ((coding-system-for-write 'utf-8-unix))
    (with-temp-file file (insert contents))))

(defun ggf351-test-fixture (root)
  "Create and return the real mixed-hunk Git fixture below ROOT."
  (let* ((project (file-name-as-directory
                   (expand-file-name "project space Ω" root)))
         (file (expand-file-name "src/sample.txt" project)))
    (make-directory project t)
    (ggf351-test-write file ggf351-test-baseline)
    (ggf351-test-git project "init" "--quiet" "--initial-branch=main")
    (ggf351-test-git project "config" "core.hooksPath" "/dev/null")
    (ggf351-test-git project "config" "user.name" "Fringe Parity")
    (ggf351-test-git project "config" "user.email" "fringe@example.invalid")
    (ggf351-test-git project "add" "src/sample.txt")
    (ggf351-test-git project "commit" "--quiet" "--no-gpg-sign" "-m"
                      "Baseline fringe fixture")
    (ggf351-test-write file ggf351-test-mixed)
    (list :root root :project project :file file :real-git ggf351-test-real-git)))

(defun ggf351-test-wait (expected-hunks)
  "Wait for the exact current Git process and EXPECTED-HUNKS generation."
  (let* ((process-buffer
          (git-gutter:diff-process-buffer (git-gutter:base-file)))
         (buffer (get-buffer process-buffer))
         (process (and buffer (get-buffer-process buffer)))
         (deadline (+ (float-time) 20)))
    (unless (and buffer process)
      (error "GGF351: public refresh did not expose its owned Git process: %S"
             process-buffer))
    (push process ggf351-test-owned-processes)
    (push buffer ggf351-test-owned-process-buffers)
    (while (and (< (float-time) deadline)
                (or (get-buffer process-buffer)
                    (not git-gutter:enabled)
                    (/= (length git-gutter:diffinfos) expected-hunks)))
      (accept-process-output nil 0.02))
    (when (or (get-buffer process-buffer)
              (not git-gutter:enabled)
              (/= (length git-gutter:diffinfos) expected-hunks))
      (error "GGF351: update did not settle: process=%S enabled=%S hunks=%S expected=%S"
             (get-buffer process-buffer) git-gutter:enabled
             (list (length git-gutter:diffinfos)
                   :status (ggf351-test-git default-directory "status" "--porcelain")
                   :diff (ggf351-test-git default-directory "diff" "--no-color"
                                              "--" (file-name-nondirectory
                                                     (buffer-file-name))))
             expected-hunks))
    process))

(defun ggf351-test-hunks ()
  "Return exact public hunk state in diff order."
  (mapcar
   (lambda (hunk)
     (list (git-gutter-hunk-type hunk)
           (git-gutter-hunk-start-line hunk)
           (git-gutter-hunk-end-line hunk)
           (substring-no-properties (git-gutter-hunk-content hunk))))
   git-gutter:diffinfos))

(defun ggf351-test-display (overlay)
  "Return OVERLAY's exact fringe display specification."
  (let ((before (overlay-get overlay 'before-string)))
    (and before (get-text-property 0 'display before))))

(defun ggf351-test-owned-overlay-objects ()
  "Return every live package parent/child overlay in stable buffer order."
  (let ((refs git-gutter-fr:bitmap-references))
    (sort
     (seq-filter
      (lambda (overlay)
        (or (overlay-get overlay 'git-gutter)
            (let ((parent (overlay-get overlay 'fringe-helper-parent)))
              (and parent (memq parent refs)))))
      (apply #'append (overlay-lists)))
     (lambda (left right)
       (let ((left-start (overlay-start left))
             (right-start (overlay-start right))
             (left-end (overlay-end left))
             (right-end (overlay-end right))
             (left-child (if (overlay-get left 'fringe-helper-parent) 1 0))
             (right-child (if (overlay-get right 'fringe-helper-parent) 1 0)))
         (or (< left-start right-start)
             (and (= left-start right-start)
                  (or (< left-end right-end)
                      (and (= left-end right-end)
                           (< left-child right-child))))))))))

(defun ggf351-test-owned-overlays ()
  "Return every live package parent/child in stable buffer order."
  (let ((refs git-gutter-fr:bitmap-references))
    (mapcar
     (lambda (overlay)
       (let ((parent (overlay-get overlay 'fringe-helper-parent)))
         (list :start (overlay-start overlay)
               :end (overlay-end overlay)
               :start-line (line-number-at-pos (overlay-start overlay))
               :end-line (line-number-at-pos (overlay-end overlay))
               :git-gutter (and (overlay-get overlay 'git-gutter) t)
               :fringe-helper (and (overlay-get overlay 'fringe-helper) t)
               :parent (and parent (cl-position parent refs :test #'eq))
               :display (ggf351-test-display overlay))))
     (ggf351-test-owned-overlay-objects))))

(defun ggf351-test-refs ()
  "Return parent references in the package's actual reverse-hunk order."
  (mapcar
   (lambda (overlay)
     (list :start (overlay-start overlay)
           :end (overlay-end overlay)
           :live (and (overlay-buffer overlay) t)
           :display (ggf351-test-display overlay)))
   git-gutter-fr:bitmap-references))

(defun ggf351-test-margin-signs ()
  "Return any ordinary margin display forms, which the adapter must suppress."
  (seq-filter
   #'identity
   (mapcar
    (lambda (overlay)
      (let ((display (ggf351-test-display overlay)))
        (and (consp display) (eq (car display) 'margin) display)))
    (apply #'append (overlay-lists)))))

(defun ggf351-test-linum-artifacts ()
  "Return exact independently owned Linum overlays in buffer order."
  (when (boundp 'linum-mode)
    (list
     :mode (and linum-mode t)
     :artifacts
     (mapcar
      (lambda (overlay)
        (list :line (line-number-at-pos (overlay-start overlay))
              :live (and (overlay-buffer overlay) t)
              :text (substring-no-properties
                     (or (overlay-get overlay 'linum-str) ""))
              :display (let ((before (overlay-get overlay 'before-string)))
                         (and before (get-text-property 0 'display before)))))
      (sort (copy-sequence linum-overlays)
            (lambda (left right)
              (< (overlay-start left) (overlay-start right))))))))

(defun ggf351-test-row-bitmaps ()
  "Return honest batch rendered-row observations for every buffer line."
  (redisplay t)
  (cl-loop for line from 1 to (line-number-at-pos (point-max))
           collect
           (save-excursion
             (goto-char (point-min))
             (forward-line (1- line))
             (list line (fringe-bitmaps-at-pos (point) (selected-window))))))

(defun ggf351-test-clean-processes ()
  "Stop and reap only the Git processes captured by `ggf351-test-wait'."
  (dolist (process ggf351-test-owned-processes)
    (when (process-live-p process) (delete-process process)))
  (let ((deadline (+ (float-time) 5)))
    (while (and (< (float-time) deadline)
                (seq-some #'process-live-p ggf351-test-owned-processes))
      (accept-process-output nil 0.02)))
  (dolist (buffer ggf351-test-owned-process-buffers)
    (when (buffer-live-p buffer) (kill-buffer buffer))))

(defun ggf351-test-run-owned (name function)
  "Run FUNCTION in a real owned repository and return result plus cleanup."
  (let* ((root (ggf351-test-root name))
         (process-environment (copy-sequence process-environment))
         (baseline-processes (process-list))
         (baseline-buffers (buffer-list))
         (baseline-timers (copy-sequence timer-list))
         (baseline-window (selected-window))
         (baseline-window-config (current-window-configuration))
         (baseline-margins (window-margins baseline-window))
         (baseline-fringes (window-fringes baseline-window))
         (original-buffer (current-buffer))
         (fixture nil)
         (buffer nil)
         (body-result nil)
         (body-error nil)
         (cleanup-errors nil)
         (root-owned nil)
         (real-git (or (executable-find "git" t)
                       (error "GGF351: real Git is required")))
         (ggf351-test-real-git nil)
         (ggf351-test-owned-processes nil)
         (ggf351-test-owned-process-buffers nil)
         (final-mode nil)
         (final-enabled nil)
         (final-refs nil)
         (final-overlays nil))
    (setenv "LC_ALL" "C")
    (setenv "LANG" "C")
    (setenv "TZ" "UTC")
    (setenv "GIT_CONFIG_GLOBAL" "/dev/null")
    (setenv "GIT_CONFIG_NOSYSTEM" "1")
    (setenv "GIT_AUTHOR_NAME" "Fringe Parity")
    (setenv "GIT_AUTHOR_EMAIL" "fringe@example.invalid")
    (setenv "GIT_COMMITTER_NAME" "Fringe Parity")
    (setenv "GIT_COMMITTER_EMAIL" "fringe@example.invalid")
    (setenv "GIT_AUTHOR_DATE" "2024-02-03T04:05:06+0000")
    (setenv "GIT_COMMITTER_DATE" "2024-02-03T04:05:06+0000")
    (setq ggf351-test-real-git real-git)
    (unwind-protect
        (condition-case error-data
            (progn
              (make-directory root)
              (setq root-owned t
                    fixture (ggf351-test-fixture root))
              (let ((enable-local-variables nil)
                    (enable-local-eval nil))
                (setq buffer (find-file-noselect (plist-get fixture :file))))
              (setq body-result
                    (save-window-excursion
                      (set-window-buffer (selected-window) buffer)
                      (with-current-buffer buffer
                        (let ((git-gutter:update-interval 0)
                              (git-gutter:verbosity 0)
                              (git-gutter:handled-backends '(git))
                              (git-gutter-fr:side 'left-fringe))
                          (funcall function fixture
                                   baseline-margins baseline-fringes))))))
          (error (setq body-error error-data)))
      (dolist
          (phase
           (list
            (cons 'disable-modes
                  (lambda ()
                    (when (buffer-live-p buffer)
                      (with-current-buffer buffer
                        (when (bound-and-true-p linum-mode) (linum-mode -1))
                        (when git-gutter-mode (git-gutter-mode -1))))))
            (cons 'processes
                  #'ggf351-test-clean-processes)
            (cons 'clear
                  (lambda ()
                    (when (buffer-live-p buffer)
                      (with-current-buffer buffer
                        (git-gutter-fr:clear)
                        (setq final-mode git-gutter-mode
                              final-enabled git-gutter:enabled
                              final-refs git-gutter-fr:bitmap-references
                              final-overlays (ggf351-test-owned-overlays))))))
            (cons 'buffers
                  (lambda ()
                    (when (buffer-live-p (get-buffer git-gutter:popup-buffer))
                      (kill-buffer git-gutter:popup-buffer))
                    (when (buffer-live-p buffer)
                      (with-current-buffer buffer (set-buffer-modified-p nil))
                      (kill-buffer buffer))
                    (let ((conversion (get-buffer " *code-conversion-work*")))
                      (when (and (buffer-live-p conversion)
                                 (not (memq conversion baseline-buffers)))
                        (kill-buffer conversion)))))
            (cons 'timers
                  (lambda ()
                    (dolist (timer (seq-remove
                                    (lambda (candidate)
                                      (memq candidate baseline-timers))
                                    timer-list))
                      (cancel-timer timer))))
            (cons 'window
                  (lambda () (set-window-configuration baseline-window-config)))
            ;; Reap a process that a teardown callback might have started.
            ;; This remains restricted to the exact processes captured by the
            ;; public refresh observer; unrelated editor processes are never
            ;; touched.
            (cons 'final-owned-process-sweep
                  #'ggf351-test-clean-processes)
            (cons 'root
                  (lambda ()
                    (when (and root-owned (file-exists-p root))
                      (delete-directory root t))
                    (setq root-owned nil)))
            ;; GNU may lazily create this internal coding cache while removing
            ;; the Unicode fixture path.  It is owned by that final filesystem
            ;; phase when it was absent from the case baseline.
            (cons 'post-root-coding-cache
                  (lambda ()
                    (let ((conversion (get-buffer " *code-conversion-work*")))
                      (when (and (buffer-live-p conversion)
                                 (not (memq conversion baseline-buffers)))
                        (kill-buffer conversion)))))))
        (condition-case cleanup-error
            (funcall (cdr phase))
          (error (push (list (car phase) cleanup-error) cleanup-errors)))))
    (let* ((new-buffers
            (mapcar #'buffer-name
                    (seq-remove (lambda (candidate)
                                  (memq candidate baseline-buffers))
                                (buffer-list))))
           (new-processes
            (mapcar #'process-name
                    (seq-remove (lambda (candidate)
                                  (memq candidate baseline-processes))
                                (process-list))))
           (new-timers
            (length (seq-remove (lambda (candidate)
                                  (memq candidate baseline-timers))
                                timer-list)))
           (cleanup
           (list :new-buffers
                 new-buffers
                 :new-processes new-processes
                 :new-timers new-timers
                 :root-exists (file-exists-p root)
                 :root-owned root-owned
                 :mode final-mode
                 :enabled final-enabled
                 :refs final-refs
                 :owned-overlays final-overlays
                 :processes-live
                 (seq-some #'process-live-p ggf351-test-owned-processes)
                 :process-buffers-live
                 (seq-some #'buffer-live-p ggf351-test-owned-process-buffers)
                 :window-restored (eq (selected-window) baseline-window)
                 :margins-restored (equal (window-margins baseline-window)
                                          baseline-margins)
                 :fringes-restored (equal (window-fringes baseline-window)
                                          baseline-fringes)
                 :buffer-restored (eq (current-buffer) original-buffer)
                 :body-error body-error
                 :cleanup-errors (nreverse cleanup-errors))))
      (when (or body-error cleanup-errors new-buffers new-processes
                (/= new-timers 0) (file-exists-p root) root-owned
                final-mode final-enabled final-refs final-overlays
                (seq-some #'process-live-p ggf351-test-owned-processes)
                (seq-some #'buffer-live-p ggf351-test-owned-process-buffers)
                (not (eq (selected-window) baseline-window))
                (not (equal (window-margins baseline-window) baseline-margins))
                (not (equal (window-fringes baseline-window) baseline-fringes))
                (not (eq (current-buffer) original-buffer)))
        (error "GGF351: body/cleanup failure: %S" cleanup))
      (list :result body-result :cleanup cleanup))))

(defun ggf351-test-run (name function)
  "Run one isolated world and prove shared global state is restored."
  (let ((environment (copy-sequence process-environment))
        (callbacks (list git-gutter:init-function git-gutter:view-diff-function
                         git-gutter:clear-function git-gutter:window-width))
        result)
    (setq result (ggf351-test-run-owned name function))
    (let ((environment-restored (equal process-environment environment))
          (callbacks-restored
           (equal (list git-gutter:init-function git-gutter:view-diff-function
                        git-gutter:clear-function git-gutter:window-width)
                  callbacks)))
      (unless (and environment-restored callbacks-restored)
        (error "GGF351: shared state leaked: env=%S callbacks=%S"
               environment-restored callbacks-restored))
      (setcdr (last (plist-get result :cleanup))
              (list :environment-restored environment-restored
                    :callbacks-restored callbacks-restored)))
    result))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(GIT_GUTTER_FRINGE_MELPA_PIN, "git-gutter-fringe.el")
        .expect("prepare exact shallow Git Gutter Fringe source below ./tmp")
        .with_prelude(TEST_PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed Git Gutter Fringe parity test")
        .into()
}

fn assert_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        oracle(),
        &current_test_name(),
        "git_gutter_fringe_parity",
        cases,
    );
}

#[test]
fn git_gutter_fringe_package_batch() {
    assert_batch(&workflows::workflow_batch_cases());
}
