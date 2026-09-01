//! Practical parity for rake.el's public Ruby task-runner commands.
//!
//! These cases discover a real Rakefile, list tasks through an owned rake
//! stand-in, apply bundler/zeus/spring prefixes, cache and find a task
//! definition, and recover after a missing project or unused rerun.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, DASH_MELPA_PIN, F_MELPA_PIN, RAKE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)
(require 'rake)
(set-window-configuration (current-window-configuration))

(defconst rk429-test-tree
  "89677ee220740eaa482d1a428041ebecf2b4d500")
(defconst rk429-test-manifest
  '(("rake-pkg.el" . "c171e1f727d4e151c197c04f9bdc0b16ff9165249e71b9f7e92819fa2ea20d26")
    ("rake.el" . "0797245f65e936fb160c865d6ffe022cd4ce31cf701ec1cfeadd61729eb9207c")))

(defvar rk429-test-case-index 0)
(defvar rk429-test-root nil)
(defvar rk429-test-root-owned nil)
(defvar rk429-test-shell-plan nil)
(defvar rk429-test-shell-ledger nil)
(defvar rk429-test-compile-plan nil)
(defvar rk429-test-compile-ledger nil)
(defvar rk429-test-completions nil)
(defvar rk429-test-reads nil)
(defvar rk429-test-minibuffer-ledger nil)

(defun rk429-test-sha (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun rk429-test-source-state ()
  (let* ((located (locate-library "rake.el"))
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
                         (cons file (rk429-test-sha
                                     (expand-file-name file directory))))
                       files))))
    (unless (and located main directory
                 (string-suffix-p "/rake.el" main)
                 (not (file-symlink-p located))
                 (equal files (mapcar #'car rk429-test-manifest)))
      (error "Unexpected installed rake payload: %S" (or manifest files)))
    (dolist (entry rk429-test-manifest)
      (let ((file (expand-file-name (car entry) directory))
            (expected (cdr entry)))
        (unless (and (file-regular-p file)
                     (not (file-symlink-p file))
                     (equal (rk429-test-sha file) expected))
          (error "Unexpected installed rake source: %S"
                 (cons entry manifest)))))
    (list :tree rk429-test-tree
          :manifest manifest
          :feature (featurep 'rake)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'rake package-alist)))))))

(defun rk429-test-condition (thunk)
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

(defun rk429-test-forbid-external (operation &rest arguments)
  (error "Unexpected rake external boundary: %S %S" operation arguments))

(defun rk429-test-write (relative contents)
  (let ((file (expand-file-name relative rk429-test-root)))
    (unless (and rk429-test-root-owned
                 (file-in-directory-p file rk429-test-root))
      (error "Refusing rake write outside owned root: %s" file))
    (make-directory (file-name-directory file) t)
    (let ((coding-system-for-write 'utf-8-unix))
      (with-temp-file file (insert contents)))
    file))

(defun rk429-test-mask (value)
  (cond
   ((stringp value)
    (let* ((root (and rk429-test-root (file-name-as-directory rk429-test-root)))
           (plain (and rk429-test-root (directory-file-name rk429-test-root)))
           (text (copy-sequence value)))
      (when root
        (setq text (replace-regexp-in-string (regexp-quote root)
                                             "[ORACLE-SANDBOX]/" text t t)))
      (when plain
        (setq text (replace-regexp-in-string (regexp-quote plain)
                                             "[ORACLE-SANDBOX]" text t t)))
      text))
   ((consp value)
    (cons (rk429-test-mask (car value)) (rk429-test-mask (cdr value))))
   (t value)))

(defun rk429-test-cache-alist ()
  (let (entries)
    (maphash
     (lambda (key value)
       (push (cons (rk429-test-mask key)
                   (mapcar #'copy-sequence value))
             entries))
     rake--cache)
    (sort entries (lambda (left right) (string< (car left) (car right))))))

(defun rk429-test-shell-command-to-string (command)
  (if (equal command "ruby -e 'print RUBY_VERSION'")
      (progn
        (push (list :command (copy-sequence command) :output "3.2.2")
              rk429-test-shell-ledger)
        "3.2.2")
    (unless rk429-test-shell-plan
      (error "Unexpected rake shell command: %s" command))
    (let ((plan (pop rk429-test-shell-plan))
          (masked (rk429-test-mask command)))
      (unless (equal command (plist-get plan :command))
        (error "Unexpected rake shell command: %S vs %S"
               masked (plist-get plan :command)))
      (push (list :command masked
                  :output (rk429-test-mask (plist-get plan :output)))
            rk429-test-shell-ledger)
      (plist-get plan :output))))

(defun rk429-test-compile (command &optional mode)
  (unless rk429-test-compile-plan
    (error "Unexpected rake compile: %s" command))
  (let ((plan (pop rk429-test-compile-plan)))
    (unless (equal command (plist-get plan :command))
      (error "Unexpected rake compile: %S vs %S"
             command (plist-get plan :command)))
    (push (list :command (copy-sequence command)
                :mode mode)
          rk429-test-compile-ledger)
    (let ((buffer (get-buffer-create "*rake-compilation*")))
      (with-current-buffer buffer
        (let ((inhibit-read-only t))
          (erase-buffer)
          (insert (or (plist-get plan :output) (concat command "\n"))))
        (when (fboundp (or mode 'rake-compilation-mode))
          (funcall (or mode 'rake-compilation-mode))))
      buffer)))

(defun rk429-test-completing-read (prompt choices)
  (push (list :prompt (copy-sequence prompt)
              :choices (mapcar #'copy-sequence choices))
        rk429-test-minibuffer-ledger)
  (or (pop rk429-test-completions)
      (error "Unexpected completing-read: %s" prompt)))

(defun rk429-test-read-string (prompt &optional initial-input)
  (push (list :prompt (copy-sequence prompt)
              :initial (and initial-input (copy-sequence initial-input)))
        rk429-test-minibuffer-ledger)
  (or (pop rk429-test-reads)
      (error "Unexpected read-string: %s" prompt)))

(defun rk429-test-project (name)
  (let ((dir (file-name-as-directory (expand-file-name name rk429-test-root))))
    (make-directory dir t)
    (rk429-test-write
     (concat name "/Rakefile")
     "# café widgets\ntask :foo do\n  puts 'foo 界'\nend\n")
    dir))

(defun rk429-test-run (body)
  (let* ((index (cl-incf rk429-test-case-index))
         (sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (root (and sandbox
                    (file-name-as-directory
                     (expand-file-name (format "rake-%d" index) sandbox))))
         (buffer-before (current-buffer))
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (append timer-list timer-idle-list))
         (frames-before (frame-list))
         (selected-window-before (selected-window))
         (window-before (current-window-configuration))
         (source-before (rk429-test-source-state))
         (directory-before default-directory)
         (enable-local-before enable-local-variables)
         (debug-before debug-on-error)
         (print-circle-before print-circle)
         (cache-file-before rake-cache-file)
         (cache-before rake--cache)
         (caching-before rake-enable-caching)
         (completion-before rake-completion-system)
         (last-root-before rake--last-root)
         (last-task-before rake--last-task)
         (last-mode-before rake--last-mode)
         (rk429-test-root root)
         (rk429-test-root-owned nil)
         (rk429-test-shell-plan nil)
         (rk429-test-shell-ledger nil)
         (rk429-test-compile-plan nil)
         (rk429-test-compile-ledger nil)
         (rk429-test-completions nil)
         (rk429-test-reads nil)
         (rk429-test-minibuffer-ledger nil)
         result body-error source-after cleanup-errors)
    (unwind-protect
        (condition-case condition
            (progn
              (unless (and root (file-name-absolute-p root))
                (error "Missing absolute rake sandbox root"))
              (when (file-exists-p root)
                (error "rake sandbox root exists: %S" root))
              (make-directory root)
              (setq rk429-test-root-owned t
                    enable-local-variables nil
                    debug-on-error nil
                    print-circle nil
                    default-directory root
                    rake-cache-file (expand-file-name "rake.cache" root)
                    rake--cache (make-hash-table :test 'equal)
                    rake-enable-caching t
                    rake-completion-system 'default
                    rake--last-root nil
                    rake--last-task nil
                    rake--last-mode nil)
              (cl-letf (((symbol-function 'call-process)
                         (lambda (&rest args)
                           (apply #'rk429-test-forbid-external
                                  'call-process args)))
                        ((symbol-function 'call-process-region)
                         (lambda (&rest args)
                           (apply #'rk429-test-forbid-external
                                  'call-process-region args)))
                        ((symbol-function 'make-process)
                         (lambda (&rest args)
                           (apply #'rk429-test-forbid-external
                                  'make-process args)))
                        ((symbol-function 'process-file)
                         (lambda (&rest args)
                           (apply #'rk429-test-forbid-external
                                  'process-file args)))
                        ((symbol-function 'start-file-process)
                         (lambda (&rest args)
                           (apply #'rk429-test-forbid-external
                                  'start-file-process args)))
                        ((symbol-function 'start-process)
                         (lambda (&rest args)
                           (apply #'rk429-test-forbid-external
                                  'start-process args)))
                        ((symbol-function 'url-retrieve)
                         (lambda (&rest args)
                           (apply #'rk429-test-forbid-external
                                  'url-retrieve args)))
                        ((symbol-function 'url-retrieve-synchronously)
                         (lambda (&rest args)
                           (apply #'rk429-test-forbid-external
                                  'url-retrieve-synchronously args)))
                        ((symbol-function 'shell-command-to-string)
                         #'rk429-test-shell-command-to-string)
                        ((symbol-function 'compile)
                         #'rk429-test-compile)
                        ((symbol-function 'completing-read)
                         (lambda (prompt collection &rest _)
                           (rk429-test-completing-read
                            prompt
                            (if (listp collection)
                                collection
                              (all-completions "" collection)))))
                        ((symbol-function 'ido-completing-read)
                         (lambda (prompt choices &rest _)
                           (rk429-test-completing-read prompt choices)))
                        ((symbol-function 'read-string)
                         (lambda (prompt &optional initial-input &rest _)
                           (rk429-test-read-string prompt initial-input))))
                (setq result (funcall body root)))
              (when rk429-test-shell-plan
                (error "Unused rake shell plan: %S" rk429-test-shell-plan))
              (when rk429-test-compile-plan
                (error "Unused rake compile plan: %S" rk429-test-compile-plan))
              (setq source-after (rk429-test-source-state))
              (unless (equal source-before source-after)
                (error "rake source changed")))
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
        (setq rake-cache-file cache-file-before
              rake--cache cache-before
              rake-enable-caching caching-before
              rake-completion-system completion-before
              rake--last-root last-root-before
              rake--last-task last-task-before
              rake--last-mode last-mode-before
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
        (when rk429-test-root-owned
          (attempt 'root (lambda () (delete-directory root t))))))
    (when body-error
      (error "rake body failed: %S" body-error))
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
                 :cache-restored (eq rake--cache cache-before)
                 :body-error body-error
                 :cleanup-errors (nreverse cleanup-errors))))
      (if cleanup-errors
          (error "rake cleanup failed: %S" (list result cleanup))
        (list :source source-before
              :result result
              :cleanup cleanup)))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(RAKE_MELPA_PIN, "rake.el")
        .expect("prepare pinned rake source below ./tmp")
        .with_melpa_dependency(DASH_MELPA_PIN)
        .expect("prepare pinned dash dependency below ./tmp")
        .with_melpa_dependency(F_MELPA_PIN)
        .expect("prepare pinned f dependency below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn lists_and_runs_a_vanilla_task_from_a_real_rakefile() -> ParityBatchCase {
    ParityBatchCase::value(
        "lists_and_runs_a_vanilla_task_from_a_real_rakefile",
        r####"
(rk429-test-run
 (lambda (_root)
   (let* ((project (rk429-test-project "widgets"))
          (listed
           (concat
            "rake db:migrate:status  # café widgets\n"
            "rake spec:all           # specs\n"))
          (truename (file-truename project)))
     (setq default-directory project
           rk429-test-shell-plan
           (list (list :command "rake -T -A" :output listed))
           rk429-test-compile-plan
           (list (list :command "rake db:migrate:status"
                       :output "rake db:migrate:status\n"))
           rk429-test-completions (list "db:migrate:status"))
     (rake nil)
     (list :root (rk429-test-mask (rake--root))
           :compile (nreverse (copy-tree rk429-test-compile-ledger))
           :shell (nreverse (copy-tree rk429-test-shell-ledger))
           :choices (nreverse (copy-tree rk429-test-minibuffer-ledger))
           :last-task (copy-sequence rake--last-task)
           :last-mode rake--last-mode
           :cache (rk429-test-cache-alist)
           :cache-file
           (and (file-regular-p rake-cache-file)
                (rk429-test-mask
                 (with-temp-buffer
                   (insert-file-contents rake-cache-file)
                   (buffer-string))))
           :prefix (rake--choose-command-prefix
                    truename
                    (list :spring "S" :zeus "Z" :bundler "B" :vanilla "V"))))))
"####,
        expect![[
            r##"OK (:source (:tree "89677ee220740eaa482d1a428041ebecf2b4d500" :manifest (("rake-pkg.el" . "c171e1f727d4e151c197c04f9bdc0b16ff9165249e71b9f7e92819fa2ea20d26") ("rake.el" . "0797245f65e936fb160c865d6ffe022cd4ce31cf701ec1cfeadd61729eb9207c")) :feature t :version "20220211.827") :result (:root "[ORACLE-SANDBOX]/widgets/" :compile ((:command "rake db:migrate:status" :mode rake-compilation-mode)) :shell ((:command "ruby -e 'print RUBY_VERSION'" :output "3.2.2") (:command "ruby -e 'print RUBY_VERSION'" :output "3.2.2") (:command "rake -T -A" :output "rake db:migrate:status  # café widgets\nrake spec:all           # specs\n")) :choices ((:prompt "Rake task: " :choices ("db:migrate:status" "spec:all"))) :last-task "rake db:migrate:status" :last-mode rake-compilation-mode :cache (("[ORACLE-SANDBOX]/widgets/" "db:migrate:status  # café widgets" "spec:all           # specs")) :cache-file "#s(hash-table test equal data (\"[ORACLE-SANDBOX]/widgets/\" (\"db:migrate:status  # café widgets\" \"spec:all           # specs\")))" :prefix "V") :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :cache-restored t :body-error nil :cleanup-errors nil))"##
        ]],
    )
}

fn bundler_zeus_and_spring_prefixes_and_edited_command() -> ParityBatchCase {
    ParityBatchCase::value(
        "bundler_zeus_and_spring_prefixes_and_edited_command",
        r####"
(rk429-test-run
 (lambda (_root)
   (let* ((bundler (rk429-test-project "bundler"))
          (zeus (rk429-test-project "zeus"))
          (spring (rk429-test-project "spring"))
          (listed "rake foo  # café\n")
          bundler-run zeus-run spring-run edited)
     (rk429-test-write "bundler/Gemfile" "source 'https://rubygems.org'\n")
     (rk429-test-write "zeus/.zeus.sock" "")
     (make-directory (expand-file-name "tmp/spring" spring) t)
     (rk429-test-write "spring/tmp/spring/spring.pid" "1\n")
     (setq rk429-test-shell-plan
           (list (list :command "bundle exec rake -T -A" :output listed)
                 (list :command "zeus rake -T -A" :output listed)
                 (list :command "bundle exec spring rake -T -A" :output listed)
                 (list :command "rake -T -A" :output listed))
           rk429-test-compile-plan
           (list (list :command "bundle exec rake foo")
                 (list :command "zeus rake foo")
                 (list :command "bundle exec spring rake foo")
                 (list :command "rake foo APP=café"))
           rk429-test-completions (list "foo" "foo" "foo" "foo")
           rk429-test-reads (list "rake foo APP=café"))
     (let ((default-directory bundler))
       (rake nil)
       (setq bundler-run (copy-sequence rake--last-task)))
     (let ((default-directory zeus))
       (rake nil)
       (setq zeus-run (copy-sequence rake--last-task)))
     (let ((default-directory spring))
       (rake nil)
       (setq spring-run (copy-sequence rake--last-task)))
     (let ((default-directory (rk429-test-project "vanilla")))
       (rake '(4))
       (setq edited (copy-sequence rake--last-task)))
     (list :bundler bundler-run
           :zeus zeus-run
           :spring spring-run
           :edited edited
           :compile (nreverse (copy-tree rk429-test-compile-ledger))
           :shell (nreverse (copy-tree rk429-test-shell-ledger))
           :reads (nreverse (copy-tree rk429-test-minibuffer-ledger))))))
"####,
        expect![[
            r#"OK (:source (:tree "89677ee220740eaa482d1a428041ebecf2b4d500" :manifest (("rake-pkg.el" . "c171e1f727d4e151c197c04f9bdc0b16ff9165249e71b9f7e92819fa2ea20d26") ("rake.el" . "0797245f65e936fb160c865d6ffe022cd4ce31cf701ec1cfeadd61729eb9207c")) :feature t :version "20220211.827") :result (:bundler "bundle exec rake foo" :zeus "zeus rake foo" :spring "bundle exec spring rake foo" :edited "rake foo APP=café" :compile ((:command "bundle exec rake foo" :mode rake-compilation-mode) (:command "zeus rake foo" :mode rake-compilation-mode) (:command "bundle exec spring rake foo" :mode rake-compilation-mode) (:command "rake foo APP=café" :mode rake-compilation-mode)) :shell ((:command "ruby -e 'print RUBY_VERSION'" :output "3.2.2") (:command "ruby -e 'print RUBY_VERSION'" :output "3.2.2") (:command "bundle exec rake -T -A" :output "rake foo  # café\n") (:command "ruby -e 'print RUBY_VERSION'" :output "3.2.2") (:command "ruby -e 'print RUBY_VERSION'" :output "3.2.2") (:command "zeus rake -T -A" :output "rake foo  # café\n") (:command "bundle exec spring rake -T -A" :output "rake foo  # café\n") (:command "ruby -e 'print RUBY_VERSION'" :output "3.2.2") (:command "ruby -e 'print RUBY_VERSION'" :output "3.2.2") (:command "rake -T -A" :output "rake foo  # café\n")) :reads ((:prompt "Rake task: " :choices ("foo")) (:prompt "Rake task: " :choices ("foo")) (:prompt "Rake task: " :choices ("foo")) (:prompt "Rake task: " :choices ("foo")) (:prompt "Rake command: " :initial "rake foo "))) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :cache-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn find_task_visits_definition_and_rerun_replays_the_last_command() -> ParityBatchCase {
    ParityBatchCase::value(
        "find_task_visits_definition_and_rerun_replays_the_last_command",
        r####"
(rk429-test-run
 (lambda (_root)
   (let* ((project (rk429-test-project "locate"))
          (rakefile (expand-file-name "Rakefile" project))
          (where (format "rake foo  %s:2:\n" rakefile))
          visited rerun)
     (setq default-directory project
           rk429-test-shell-plan
           (list (list :command "rake -T -A" :output "rake foo  # café\n")
                 (list :command "rake --where foo" :output where))
           rk429-test-compile-plan
           (list (list :command "rake foo")
                 (list :command "rake foo"))
           rk429-test-completions (list "foo" "foo"))
     (rake-find-task nil)
     (setq visited
           (list :file (rk429-test-mask buffer-file-name)
                 :line (line-number-at-pos)
                 :text (buffer-substring-no-properties
                        (line-beginning-position) (line-end-position))))
     (rake nil)
     (setq rerun
           (progn
             (rake-rerun)
             (copy-sequence rake--last-task)))
     (list :visited visited
           :rerun rerun
           :compile (nreverse (copy-tree rk429-test-compile-ledger))
           :shell (nreverse (copy-tree rk429-test-shell-ledger))))))
"####,
        expect![[
            r#"OK (:source (:tree "89677ee220740eaa482d1a428041ebecf2b4d500" :manifest (("rake-pkg.el" . "c171e1f727d4e151c197c04f9bdc0b16ff9165249e71b9f7e92819fa2ea20d26") ("rake.el" . "0797245f65e936fb160c865d6ffe022cd4ce31cf701ec1cfeadd61729eb9207c")) :feature t :version "20220211.827") :result (:visited (:file "[ORACLE-SANDBOX]/locate/Rakefile" :line 2 :text "task :foo do") :rerun "rake foo" :compile ((:command "rake foo" :mode rake-compilation-mode) (:command "rake foo" :mode rake-compilation-mode)) :shell ((:command "ruby -e 'print RUBY_VERSION'" :output "3.2.2") (:command "ruby -e 'print RUBY_VERSION'" :output "3.2.2") (:command "rake -T -A" :output "rake foo  # café\n") (:command "rake --where foo" :output "rake foo  [ORACLE-SANDBOX]/locate/Rakefile:2:\n") (:command "ruby -e 'print RUBY_VERSION'" :output "3.2.2"))) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :cache-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn missing_rakefile_backends_and_rerun_signal_then_recover() -> ParityBatchCase {
    ParityBatchCase::value(
        "missing_rakefile_backends_and_rerun_signal_then_recover",
        r####"
(rk429-test-run
 (lambda (_root)
   (let* ((empty (expand-file-name "empty" rk429-test-root))
          (project (rk429-test-project "recover"))
          missing helm grizzl stale recovered)
     (make-directory empty t)
     (let ((default-directory empty))
       (setq missing (rk429-test-condition (lambda () (rake nil)))))
     (puthash (file-truename project) (list "foo  # café") rake--cache)
     (let ((rake-completion-system 'helm)
           (default-directory project))
       (setq helm (rk429-test-condition (lambda () (rake nil)))))
     (let ((rake-completion-system 'grizzl)
           (default-directory project))
       (setq grizzl (rk429-test-condition (lambda () (rake nil)))))
     (setq stale (rk429-test-condition #'rake-rerun))
     (setq default-directory project
           rk429-test-compile-plan
           (list (list :command "rake foo"))
           rk429-test-completions (list "foo"))
     (rake nil)
     (setq recovered (copy-sequence rake--last-task))
     (list :missing missing
           :helm helm
           :grizzl grizzl
           :stale stale
           :recovered recovered
           :compile (nreverse (copy-tree rk429-test-compile-ledger))))))
"####,
        expect![[
            r#"OK (:source (:tree "89677ee220740eaa482d1a428041ebecf2b4d500" :manifest (("rake-pkg.el" . "c171e1f727d4e151c197c04f9bdc0b16ff9165249e71b9f7e92819fa2ea20d26") ("rake.el" . "0797245f65e936fb160c865d6ffe022cd4ce31cf701ec1cfeadd61729eb9207c")) :feature t :version "20220211.827") :result (:missing (:error wrong-type-argument :data (arrayp nil) :message "Wrong type argument: arrayp, nil") :helm (:error user-error :data ("Please install helm first") :message "Please install helm first") :grizzl (:error user-error :data ("Please install grizzl first") :message "Please install grizzl first") :stale (:error error :data ("No task was run") :message "No task was run") :recovered "rake foo" :compile ((:command "rake foo" :mode rake-compilation-mode))) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :window-restored t :cache-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

#[test]
fn rake_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        lists_and_runs_a_vanilla_task_from_a_real_rakefile(),
        bundler_zeus_and_spring_prefixes_and_edited_command(),
        find_task_visits_definition_and_rerun_replays_the_last_command(),
        missing_rakefile_backends_and_rerun_signal_then_recover(),
    ];
    assert_oracle_batch_cases(oracle(), "rake-rank429", "rake_parity", &cases);
}
