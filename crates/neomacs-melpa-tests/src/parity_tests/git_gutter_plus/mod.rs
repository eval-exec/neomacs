//! Practical parity coverage for retired rank 412 `git-gutter+`.
//!
//! The corpus drives its public mode, navigation, popup, revert, staging,
//! unstage, and commit-failure recovery routes against a deterministic real Git
//! repository.
//! Only Git process execution and confirmation input are controlled boundaries.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, GIT_GUTTER_PLUS_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(180);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)
(require 'git-gutter+)

;; GNU lazily creates this reserved coding cache for the Unicode fixture path.
;; Make it common prelude state rather than per-case residue.
(get-buffer-create " *code-conversion-work*")

(defconst ggp412-test-upstream-source-sha
  "f64612560477186db3d4e2533ba55a0316dcbae1539b0dc0abc721ac1890d948")
(defconst ggp412-test-installed-source-sha
  "288d40efc9d52b6527aded6e8c4e34caf4d9cf7031810b3466f44c0820ff69fa")
(defconst ggp412-test-git-sha
  "f01676568f1dc06110d91eb3923ba069338c0cada4b5798b225170991363c352")

(defvar ggp412-test-root nil)
(defvar ggp412-test-git nil)
(defvar ggp412-test-real-call-process nil)
(defvar ggp412-test-real-call-process-region nil)
(defvar ggp412-test-real-process-file nil)
(defvar ggp412-test-inside-process-file nil)
(defvar ggp412-test-plan nil)
(defvar ggp412-test-ledger nil)

(defun ggp412-test-condition (condition)
  (list :symbol (car condition)
        :data (copy-tree (cdr condition))
        :message (error-message-string condition)))

(defun ggp412-test-file-sha (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun ggp412-test-source-state ()
  (let ((source (file-truename (locate-library "git-gutter+.el"))))
    (unless (and (string-suffix-p "/git-gutter+.el" source)
                 (equal (ggp412-test-file-sha source)
                        ggp412-test-installed-source-sha))
      (error "Git-Gutter+ installed source mismatch: %s" source))
    (list :upstream-sha256 ggp412-test-upstream-source-sha
          :installed-sha256 ggp412-test-installed-source-sha
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'git-gutter+ package-alist))))
          :feature (featurep 'git-gutter+))))

(defun ggp412-test-git (root &rest arguments)
  (let ((default-directory root))
    (with-temp-buffer
      (let ((status (apply ggp412-test-real-call-process
                           ggp412-test-git nil t nil arguments)))
        (unless (zerop status)
          (error "Fixture git %S failed (%s): %s"
                 arguments status (buffer-string)))
        (string-trim-right (buffer-string))))))

(defun ggp412-test-write (file contents)
  (unless (and (file-name-absolute-p file)
               (file-in-directory-p file ggp412-test-root))
    (error "Refusing Git-Gutter+ fixture write outside owned root: %s" file))
  (let ((coding-system-for-write 'utf-8-unix))
    (with-temp-file file (insert contents))))

(defun ggp412-test-fixture (root)
  (let ((file (expand-file-name "release 界.txt" root)))
    (ggp412-test-git root "init" "--quiet" "--initial-branch=main")
    (ggp412-test-git root "config" "core.hooksPath" "/dev/null")
    (ggp412-test-git root "config" "user.name" "Gutter Plus Parity")
    (ggp412-test-git root "config" "user.email" "gutter-plus@example.test")
    (ggp412-test-write
     file
     "# Release 界\nowner: platform\n\nsteps:\n- validate\n- publish\n\nnotes:\n- legacy\nend\n")
    (let ((process-environment (copy-sequence process-environment)))
      (setenv "GIT_AUTHOR_DATE" "2024-01-02T03:04:05+0000")
      (setenv "GIT_COMMITTER_DATE" "2024-01-02T03:04:05+0000")
      (ggp412-test-git root "add" "release 界.txt")
      (ggp412-test-git root "commit" "--quiet" "--no-gpg-sign"
                       "-m" "Baseline release"))
    (ggp412-test-write
     file
     "# Release 界\nowner: delivery\n\nsteps:\n- validate\n- notify\n- publish\n\nnotes:\nend\n")
    file))

(defun ggp412-test-normalize-args (arguments)
  (mapcar
   (lambda (argument)
     (if (and (stringp argument)
              (file-name-absolute-p argument)
              (file-in-directory-p argument ggp412-test-root))
         (file-relative-name argument ggp412-test-root)
       argument))
   arguments))

(defun ggp412-test-pop-plan (kind arguments)
  (let* ((actual (cons kind (ggp412-test-normalize-args arguments)))
         (expected (pop ggp412-test-plan)))
    (unless (equal actual expected)
      (error "Unexpected Git-Gutter+ boundary: expected %S, got %S"
             expected actual))
    actual))

(defun ggp412-test-call-process
    (program &optional infile destination display &rest arguments)
  (if ggp412-test-inside-process-file
    (apply ggp412-test-real-call-process
           program infile destination display arguments)
    (unless (and (equal program ggp412-test-git)
                 (null infile)
                 (not display)
                 (file-in-directory-p default-directory ggp412-test-root))
      (error "Unexpected Git-Gutter+ call-process: %S"
             (list program infile destination display arguments)))
    (let ((entry (ggp412-test-pop-plan 'call arguments)))
      (push (append entry (list :cwd (file-relative-name default-directory
                                                          ggp412-test-root)))
            ggp412-test-ledger)
      (apply ggp412-test-real-call-process
             program infile destination display arguments))))

(defun ggp412-test-call-process-region
    (start end program &optional delete destination display &rest arguments)
  (unless (and (equal program ggp412-test-git)
               delete
               (eq destination t)
               (not display)
               (file-in-directory-p default-directory ggp412-test-root))
    (error "Unexpected Git-Gutter+ call-process-region: %S"
           (list start end program delete destination display arguments)))
  (let* ((input (buffer-substring-no-properties start end))
         (entry (ggp412-test-pop-plan 'region arguments)))
    (push (append entry
                  (list :cwd (file-relative-name default-directory
                                                  ggp412-test-root)
                        :input-sha (secure-hash 'sha256 input)
                        :input-lines (length (split-string input "\n" t))))
          ggp412-test-ledger)
    (apply ggp412-test-real-call-process-region
           start end program delete destination display arguments)))

(defun ggp412-test-process-file
    (program &optional infile destination display &rest arguments)
  (unless (and (string= program "git")
               (equal (file-truename (executable-find program)) ggp412-test-git)
               (null infile)
               (equal destination '(t nil))
               (not display)
               (file-in-directory-p default-directory ggp412-test-root))
    (error "Unexpected Git-Gutter+ process-file: %S"
           (list program infile destination display arguments)))
  (let ((entry (ggp412-test-pop-plan 'file arguments)))
    (push (append entry (list :cwd (file-relative-name default-directory
                                                        ggp412-test-root)))
          ggp412-test-ledger)
    (let ((ggp412-test-inside-process-file t))
      (apply ggp412-test-real-process-file
             program infile destination display arguments))))

(defun ggp412-test-forbid-external (kind &rest arguments)
  (error "Unexpected external boundary: %S" (cons kind arguments)))

(defun ggp412-test-hunks ()
  (mapcar
   (lambda (hunk)
     (list :type (plist-get hunk :type)
           :start (plist-get hunk :start-line)
           :end (plist-get hunk :end-line)
           :content (substring-no-properties (plist-get hunk :content))))
   git-gutter+-diffinfos))

(defun ggp412-test-overlays ()
  (mapcar
   (lambda (overlay)
     (let* ((before (overlay-get overlay 'before-string))
            (display (and before (get-text-property 0 'display before)))
            (value (and (consp display) (cadr display))))
       (list :line (line-number-at-pos (overlay-start overlay))
             :gutter (and (overlay-get overlay 'git-gutter+) t)
             :text (and (stringp value) (substring-no-properties value))
             :face (and (stringp value) (get-text-property 0 'face value)))))
   (sort (cl-remove-if-not
          (lambda (overlay) (overlay-get overlay 'git-gutter+))
          (overlays-in (point-min) (point-max)))
         (lambda (left right) (< (overlay-start left) (overlay-start right))))))

(defun ggp412-test-git-observation (root &rest args)
  (apply #'ggp412-test-git root args))

(defun ggp412-test-park-buffer (name)
  (when-let ((buffer (get-buffer name)))
    (let ((parked (generate-new-buffer-name (concat " *parked " name "*"))))
      (with-current-buffer buffer (rename-buffer parked t))
      (cons buffer name))))

(defun ggp412-test-run (case-name plan body)
  (let* ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (root (and sandbox
                    (file-name-as-directory
                     (expand-file-name (concat "git-gutter-plus-" case-name "/")
                                       sandbox))))
         (buffer-before (current-buffer))
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (copy-sequence timer-list))
         (frames-before (frame-list))
         (window-before (current-window-configuration))
         (source-before (ggp412-test-source-state))
         (parked nil)
         (ggp412-test-real-call-process (symbol-function 'call-process))
         (ggp412-test-real-call-process-region
          (symbol-function 'call-process-region))
         (ggp412-test-real-process-file (symbol-function 'process-file))
         (ggp412-test-git (file-truename
                           (or (executable-find "git")
                               (error "Missing exact Git executable"))))
         (ggp412-test-root root)
         (ggp412-test-plan (copy-tree plan))
         (ggp412-test-ledger nil)
         (process-environment (copy-sequence process-environment))
         (git-gutter+-git-executable ggp412-test-git)
         (git-gutter+-verbosity 0)
         (git-gutter+-window-width 1)
         (git-gutter+-separator-sign nil)
         (git-gutter+-modified-sign "=")
         (git-gutter+-added-sign "+")
         (git-gutter+-deleted-sign "-")
         (git-gutter+-unchanged-sign nil)
         (git-gutter+-hide-gutter t)
         (git-gutter+-buffers-to-reenable nil)
         (git-gutter+-pre-commit-window-config nil)
         (git-gutter+-commit-origin-buffer nil)
         (root-owned nil)
         file buffer result body-error cleanup-errors source-after)
    (setenv "LC_ALL" "C")
    (setenv "LANG" "C")
    (setenv "TZ" "UTC")
    (setenv "GIT_CONFIG_GLOBAL" "/dev/null")
    (setenv "GIT_CONFIG_NOSYSTEM" "1")
    (unwind-protect
        (condition-case condition
            (progn
              (unless (and root (file-name-absolute-p root))
                (error "Missing absolute Git-Gutter+ sandbox root"))
              (when (file-exists-p root)
                (error "Git-Gutter+ sandbox root exists: %s" root))
              (unless (and (file-regular-p ggp412-test-git)
                           (not (file-symlink-p ggp412-test-git))
                           (equal (ggp412-test-file-sha ggp412-test-git)
                                  ggp412-test-git-sha)
                           (string= (ggp412-test-git default-directory "--version")
                                    "git version 2.51.2"))
                (error "Git executable provenance mismatch: %s" ggp412-test-git))
              (dolist (name (list git-gutter+-popup-buffer
                                  git-gutter+-commit-buffer-name
                                  git-gutter+-staged-changes-buffer-name))
                (when-let ((entry (ggp412-test-park-buffer name)))
                  (push entry parked)))
              (setq parked (nreverse parked))
              (make-directory root)
              (setq root-owned t
                    file (ggp412-test-fixture root)
                    buffer (let ((enable-local-variables nil)
                                 (enable-dir-local-variables nil))
                             (find-file-noselect file)))
              (setq result
                    (save-window-excursion
                      (set-window-buffer (selected-window) buffer)
                      (with-current-buffer buffer
                        (let ((default-directory root))
                          (cl-letf (((symbol-function 'call-process)
                                     #'ggp412-test-call-process)
                                    ((symbol-function 'call-process-region)
                                     #'ggp412-test-call-process-region)
                                    ((symbol-function 'process-file)
                                     #'ggp412-test-process-file)
                                    ((symbol-function 'start-process)
                                     (lambda (&rest args)
                                       (apply #'ggp412-test-forbid-external
                                              'start-process args)))
                                    ((symbol-function 'start-file-process)
                                     (lambda (&rest args)
                                       (apply #'ggp412-test-forbid-external
                                              'start-file-process args)))
                                    ((symbol-function 'make-process)
                                     (lambda (&rest args)
                                       (apply #'ggp412-test-forbid-external
                                              'make-process args))))
                            (funcall body root file))))))
              (when ggp412-test-plan
                (error "Unused Git-Gutter+ boundary plan: %S" ggp412-test-plan))
              (setq source-after (ggp412-test-source-state))
              (unless (equal source-before source-after)
                (error "Git-Gutter+ source changed")))
          (error (setq body-error (ggp412-test-condition condition))))
      (cl-labels
          ((attempt (label thunk)
             (condition-case condition
                 (funcall thunk)
               (error
                (push (list label (ggp412-test-condition condition))
                      cleanup-errors)))))
        (when (buffer-live-p buffer)
          (attempt 'disable-mode
                   (lambda ()
                     (with-current-buffer buffer
                       (when git-gutter+-mode (git-gutter+-mode -1)))))
          (attempt 'clear-modified
                   (lambda ()
                     (with-current-buffer buffer (set-buffer-modified-p nil)))))
        (dolist (process (process-list))
          (unless (memq process processes-before)
            (attempt (list 'delete-process (process-name process))
                     (lambda () (delete-process process)))))
        (dolist (buffer-now (buffer-list))
          (unless (or (memq buffer-now buffers-before)
                      (assq buffer-now parked))
            (attempt (list 'kill-buffer (buffer-name buffer-now))
                     (lambda () (kill-buffer buffer-now)))))
        (dolist (timer (copy-sequence timer-list))
          (unless (memq timer timers-before)
            (attempt 'cancel-timer (lambda () (cancel-timer timer)))))
        (dolist (frame (frame-list))
          (unless (memq frame frames-before)
            (attempt 'delete-frame (lambda () (delete-frame frame t)))))
        (attempt 'restore-window
                 (lambda () (set-window-configuration window-before)))
        (dolist (entry parked)
          (attempt (list 'restore-buffer-name (cdr entry))
                   (lambda ()
                     (if (buffer-live-p (car entry))
                         (with-current-buffer (car entry)
                           (rename-buffer (cdr entry) t))
                       (error "Parked Git-Gutter+ buffer died: %s"
                              (cdr entry))))))
        (when (buffer-live-p buffer-before)
          (attempt 'restore-current-buffer
                   (lambda () (set-buffer buffer-before))))
        (when root-owned
          (attempt 'delete-root (lambda () (delete-directory root t))))))
    (let ((cleanup
           (list :source-unchanged (equal source-before source-after)
                 :new-buffers
                 (mapcar #'buffer-name
                         (seq-filter
                          (lambda (candidate)
                            (and (buffer-live-p candidate)
                                 (not (memq candidate buffers-before))))
                          (buffer-list)))
                 :new-processes
                 (length (seq-remove
                          (lambda (process) (memq process processes-before))
                          (process-list)))
                 :new-timers
                 (length (seq-remove
                          (lambda (timer) (memq timer timers-before)) timer-list))
                 :new-frames
                 (length (seq-remove
                          (lambda (frame) (memq frame frames-before))
                          (frame-list)))
                 :root-exists (and root (file-exists-p root))
                 :buffer-restored (eq (current-buffer) buffer-before)
                 :body-error body-error
                 :cleanup-errors (nreverse cleanup-errors))))
      (if (or body-error cleanup-errors)
          (error "Git-Gutter+ workflow failed: %S" (list result cleanup))
        (list :source source-before
              :result result
              :git (nreverse ggp412-test-ledger)
              :cleanup cleanup)))))

(defconst ggp412-test-open-plan
  '((call "rev-parse" "--is-inside-work-tree")
    (call "--no-pager" "diff" "--no-color" "--no-ext-diff" "-U0"
          "release 界.txt")))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(GIT_GUTTER_PLUS_MELPA_PIN, "git-gutter+.el")
        .expect("prepare exact retired Git-Gutter+ source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn public_mode_renders_hunks_and_navigates_popup() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_mode_renders_hunks_and_navigates_popup",
        r####"
(ggp412-test-run
 "mode"
 ggp412-test-open-plan
 (lambda (_root _file)
   (git-gutter+-mode 1)
   (let ((initial (list :mode git-gutter+-mode
                        :hunks (ggp412-test-hunks)
                        :overlays (ggp412-test-overlays)
                        :hooks (list
                                (and (memq #'git-gutter+-refresh after-save-hook) t)
                                (and (memq #'git-gutter+-turn-off before-revert-hook) t))
                        :margin (car (window-margins)))))
     (goto-char (point-min))
     (git-gutter+-next-hunk 1)
     (let ((next-line (line-number-at-pos)))
       (call-interactively #'git-gutter+-show-hunk)
       (let ((popup (get-buffer git-gutter+-popup-buffer)))
         (git-gutter+-previous-hunk 1)
         (let ((previous-line (line-number-at-pos))
               (popup-state
                (and popup
                     (with-current-buffer popup
                       (list :text (buffer-substring-no-properties
                                    (point-min) (point-max))
                             :mode major-mode :view view-mode)))))
           (git-gutter+-mode -1)
           (list :initial initial
                 :navigation (list next-line previous-line)
                 :popup popup-state
                 :disabled (list git-gutter+-mode
                                 (ggp412-test-overlays)
                                 (car (window-margins))))))))))
"####,
        expect![[
            r#"OK (:source (:upstream-sha256 "f64612560477186db3d4e2533ba55a0316dcbae1539b0dc0abc721ac1890d948" :installed-sha256 "288d40efc9d52b6527aded6e8c4e34caf4d9cf7031810b3466f44c0820ff69fa" :version "20151204.923" :feature t) :result (:initial (:mode t :hunks ((:type modified :start 2 :end 2 :content "@@ -2 +2 @@\n-owner: platform\n+owner: delivery") (:type added :start 6 :end 6 :content "@@ -5,0 +6 @@ steps:\n+- notify") (:type deleted :start 10 :end 10 :content "@@ -9 +9,0 @@ notes:\n-- legacy")) :overlays ((:line 2 :gutter t :text "=" :face git-gutter+-modified) (:line 6 :gutter t :text "+" :face git-gutter+-added) (:line 10 :gutter t :text "-" :face git-gutter+-deleted)) :hooks (t t) :margin 1) :navigation (2 10) :popup (:text "@@ -9 +9,0 @@ notes:\n-- legacy\n" :mode diff-mode :view t) :disabled (nil nil nil)) :git ((call "rev-parse" "--is-inside-work-tree" :cwd "./") (call "--no-pager" "diff" "--no-color" "--no-ext-diff" "-U0" "release 界.txt" :cwd "./")) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_revert_confirms_saves_and_refreshes_one_hunk() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_revert_confirms_saves_and_refreshes_one_hunk",
        r####"
(ggp412-test-run
 "revert"
 (append ggp412-test-open-plan
         '((file "--no-pager" "status" "--porcelain" "-z"
                 "--untracked-files" "--ignored" "--" "release 界.txt"))
         (list (cadr ggp412-test-open-plan)))
 (lambda (root file)
   (git-gutter+-mode 1)
   (goto-char (point-min))
   (forward-line 1)
   (let ((before (buffer-substring-no-properties (point-min) (point-max)))
         prompt)
     (cl-letf (((symbol-function 'yes-or-no-p)
                (lambda (text) (setq prompt text) t)))
       ;; The package's own local refresh hook is the behavior under test.
       ;; Exclude ambient editor/package hooks from this owned save.
       (let ((after-save-hook '(git-gutter+-refresh)))
         (git-gutter+-revert-hunks)))
     (list :prompt prompt
           :before before
           :after (buffer-substring-no-properties (point-min) (point-max))
           :disk (with-temp-buffer
                   (insert-file-contents file)
                   (buffer-string))
           :hunks (ggp412-test-hunks)
           :status (ggp412-test-git-observation root "status" "--short")))))
"####,
        expect![[
            r##"OK (:source (:upstream-sha256 "f64612560477186db3d4e2533ba55a0316dcbae1539b0dc0abc721ac1890d948" :installed-sha256 "288d40efc9d52b6527aded6e8c4e34caf4d9cf7031810b3466f44c0820ff69fa" :version "20151204.923" :feature t) :result (:prompt "Revert hunk?" :before "# Release 界\nowner: delivery\n\nsteps:\n- validate\n- notify\n- publish\n\nnotes:\nend\n" :after "# Release 界\nowner: platform\n\nsteps:\n- validate\n- notify\n- publish\n\nnotes:\nend\n" :disk "# Release 界\nowner: platform\n\nsteps:\n- validate\n- notify\n- publish\n\nnotes:\nend\n" :hunks ((:type added :start 6 :end 6 :content "@@ -5,0 +6 @@ steps:\n+- notify") (:type deleted :start 10 :end 10 :content "@@ -9 +9,0 @@ notes:\n-- legacy")) :status " M \"release \\347\\225\\214.txt\"") :git ((call "rev-parse" "--is-inside-work-tree" :cwd "./") (call "--no-pager" "diff" "--no-color" "--no-ext-diff" "-U0" "release 界.txt" :cwd "./") (file "--no-pager" "status" "--porcelain" "-z" "--untracked-files" "--ignored" "--" "release 界.txt" :cwd "./") (call "--no-pager" "diff" "--no-color" "--no-ext-diff" "-U0" "release 界.txt" :cwd "./")) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :body-error nil :cleanup-errors nil))"##
        ]],
    )
}

fn public_region_stage_then_unstage_preserves_worktree() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_region_stage_then_unstage_preserves_worktree",
        r####"
(ggp412-test-run
 "stage"
 (append ggp412-test-open-plan
         '((region "apply" "--unidiff-zero" "--cached" "-")
           (call "--no-pager" "diff" "--no-color" "--no-ext-diff" "-U0"
                 "release 界.txt")
           (call "reset" "--quiet" "HEAD" "--" "release 界.txt")
           (call "--no-pager" "diff" "--no-color" "--no-ext-diff" "-U0"
                 "release 界.txt")))
 (lambda (root _file)
   (git-gutter+-mode 1)
   (goto-char (point-min))
   (forward-line 4)
   (push-mark (line-beginning-position) t t)
   (forward-line 2)
   (let ((transient-mark-mode t))
     (git-gutter+-stage-hunks))
   (let ((staged (ggp412-test-git-observation
                  root "diff" "--cached" "--" "release 界.txt"))
         (remaining (ggp412-test-hunks)))
     (deactivate-mark)
     (git-gutter+-unstage-whole-buffer)
     (list :staged staged
           :remaining remaining
           :after-unstage (ggp412-test-git-observation
                           root "diff" "--cached" "--" "release 界.txt")
           :worktree (ggp412-test-git-observation root "status" "--short")
           :hunks (ggp412-test-hunks)))))
"####,
        expect![[
            r#"OK (:source (:upstream-sha256 "f64612560477186db3d4e2533ba55a0316dcbae1539b0dc0abc721ac1890d948" :installed-sha256 "288d40efc9d52b6527aded6e8c4e34caf4d9cf7031810b3466f44c0820ff69fa" :version "20151204.923" :feature t) :result (:staged "diff --git \"a/release \\347\\225\\214.txt\" \"b/release \\347\\225\\214.txt\"\nindex 67bf846..d1994e6 100644\n--- \"a/release \\347\\225\\214.txt\"\11\n+++ \"b/release \\347\\225\\214.txt\"\11\n@@ -3,6 +3,7 @@ owner: platform\n \n steps:\n - validate\n+- notify\n - publish\n \n notes:" :remaining ((:type modified :start 2 :end 2 :content "@@ -2 +2 @@\n-owner: platform\n+owner: delivery") (:type deleted :start 10 :end 10 :content "@@ -10 +9,0 @@ notes:\n-- legacy")) :after-unstage "" :worktree " M \"release \\347\\225\\214.txt\"" :hunks ((:type modified :start 2 :end 2 :content "@@ -2 +2 @@\n-owner: platform\n+owner: delivery") (:type added :start 6 :end 6 :content "@@ -5,0 +6 @@ steps:\n+- notify") (:type deleted :start 10 :end 10 :content "@@ -9 +9,0 @@ notes:\n-- legacy"))) :git ((call "rev-parse" "--is-inside-work-tree" :cwd "./") (call "--no-pager" "diff" "--no-color" "--no-ext-diff" "-U0" "release 界.txt" :cwd "./") (region "apply" "--unidiff-zero" "--cached" "-" :cwd "./" :input-sha "068dda9da01ca03786a465e1e5df5dabd01c061f8230005cbd50d021d231f64c" :input-lines 6) (call "--no-pager" "diff" "--no-color" "--no-ext-diff" "-U0" "release 界.txt" :cwd "./") (call "reset" "--quiet" "HEAD" "--" "release 界.txt" :cwd "./") (call "--no-pager" "diff" "--no-color" "--no-ext-diff" "-U0" "release 界.txt" :cwd "./")) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_stage_and_commit_surfaces_dependency_failure_then_recovers() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_stage_and_commit_surfaces_dependency_failure_then_recovers",
        r####"
(ggp412-test-run
 "commit"
 (append ggp412-test-open-plan
         '((region "apply" "--unidiff-zero" "--cached" "-")
           (call "--no-pager" "diff" "--no-color" "--no-ext-diff" "-U0"
                 "release 界.txt")
           (call "diff" "--quiet" "--cached")
           (call "reset" "--quiet" "HEAD" "--" "release 界.txt")
           (call "--no-pager" "diff" "--no-color" "--no-ext-diff" "-U0"
                 "release 界.txt")))
 (lambda (root _file)
   (git-gutter+-mode 1)
   (let ((source (current-buffer)) failure partial)
     (condition-case condition
         (git-gutter+-stage-and-commit-whole-buffer)
       (error (setq failure (ggp412-test-condition condition))))
     (let ((commit-buffer (get-buffer git-gutter+-commit-buffer-name)))
       (setq partial
             (and commit-buffer
                  (with-current-buffer commit-buffer
                    (list :name (buffer-name)
                          :mode major-mode
                          :text (buffer-substring-no-properties
                                 (point-min) (point-max)))))))
     (with-current-buffer source
       (git-gutter+-unstage-whole-buffer)
       (list :failure failure
             :partial-commit partial
             :cached-after-recovery
             (ggp412-test-git-observation
              root "diff" "--cached" "--" "release 界.txt")
             :worktree (ggp412-test-git-observation root "status" "--short")
             :mode git-gutter+-mode
             :hunks (ggp412-test-hunks))))))
"####,
        expect![[
            r#"OK (:source (:upstream-sha256 "f64612560477186db3d4e2533ba55a0316dcbae1539b0dc0abc721ac1890d948" :installed-sha256 "288d40efc9d52b6527aded6e8c4e34caf4d9cf7031810b3466f44c0820ff69fa" :version "20151204.923" :feature t) :result (:failure (:symbol void-function :data (git-commit-mode-font-lock-keywords) :message "Symbol’s function definition is void: git-commit-mode-font-lock-keywords") :partial-commit (:name "*Commit Message*" :mode git-gutter+-commit-mode :text "") :cached-after-recovery "" :worktree " M \"release \\347\\225\\214.txt\"" :mode t :hunks ((:type modified :start 2 :end 2 :content "@@ -2 +2 @@\n-owner: platform\n+owner: delivery") (:type added :start 6 :end 6 :content "@@ -5,0 +6 @@ steps:\n+- notify") (:type deleted :start 10 :end 10 :content "@@ -9 +9,0 @@ notes:\n-- legacy"))) :git ((call "rev-parse" "--is-inside-work-tree" :cwd "./") (call "--no-pager" "diff" "--no-color" "--no-ext-diff" "-U0" "release 界.txt" :cwd "./") (region "apply" "--unidiff-zero" "--cached" "-" :cwd "./" :input-sha "d4a9380e9857c58b8f8462886934c1206909c60727bb12d8b46fc080e1efcff6" :input-lines 11) (call "--no-pager" "diff" "--no-color" "--no-ext-diff" "-U0" "release 界.txt" :cwd "./") (call "diff" "--quiet" "--cached" :cwd "./") (call "reset" "--quiet" "HEAD" "--" "release 界.txt" :cwd "./") (call "--no-pager" "diff" "--no-color" "--no-ext-diff" "-U0" "release 界.txt" :cwd "./")) :cleanup (:source-unchanged t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

#[test]
fn git_gutter_plus_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        public_mode_renders_hunks_and_navigates_popup(),
        public_revert_confirms_saves_and_refreshes_one_hunk(),
        public_region_stage_then_unstage_preserves_worktree(),
        public_stage_and_commit_surfaces_dependency_failure_then_recovers(),
    ];
    assert_oracle_batch_cases(
        oracle(),
        "git-gutter-plus-rank412",
        "git_gutter_plus_parity",
        &cases,
    );
}
