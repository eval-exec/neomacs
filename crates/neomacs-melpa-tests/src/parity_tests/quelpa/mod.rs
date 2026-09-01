use std::time::Duration;

use crate::{CachedMelpaOracle, QUELPA_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const TEST_TIMEOUT: Duration = Duration::from_secs(300);

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'subr-x)
(require 'quelpa)

(defun neomacs-quelpa-test-path (root relative)
  "Return RELATIVE below the test ROOT."
  (expand-file-name relative (file-name-as-directory root)))

(defun neomacs-quelpa-test-write (root relative contents)
  "Write CONTENTS to RELATIVE below ROOT and return its path."
  (let ((path (neomacs-quelpa-test-path root relative)))
    (make-directory (file-name-directory path) t)
    (with-temp-file path
      (insert contents))
    path))

(defun neomacs-quelpa-test-git-program (program repository arguments)
  "Run Git PROGRAM with ARGUMENTS in REPOSITORY and return trimmed output."
  (with-temp-buffer
    (let ((default-directory (file-name-as-directory repository)))
      (let ((status (apply #'call-process program nil t nil arguments)))
        (unless (and (integerp status) (zerop status))
          (error "git %S failed (%S): %s"
                 arguments status (buffer-string)))
        (string-trim-right (buffer-string))))))

(defun neomacs-quelpa-test-git (repository &rest arguments)
  "Run the configured Git with ARGUMENTS in REPOSITORY."
  (neomacs-quelpa-test-git-program "git" repository arguments))

(defun neomacs-quelpa-test-commit (repository timestamp message)
  "Commit REPOSITORY at TIMESTAMP with MESSAGE and return its SHA."
  (neomacs-quelpa-test-git repository "add" "--all")
  (let ((process-environment
         (append
          (list (concat "GIT_AUTHOR_DATE=" timestamp)
                (concat "GIT_COMMITTER_DATE=" timestamp))
          process-environment)))
    (neomacs-quelpa-test-git
     repository
     "-c" "user.name=Quelpa Parity"
     "-c" "user.email=quelpa-parity@example.invalid"
     "-c" "commit.gpgsign=false"
     "commit" "--quiet" "-m" message))
  (neomacs-quelpa-test-git repository "rev-parse" "HEAD"))

(defun neomacs-quelpa-test-repository (root relative files timestamp)
  "Create a real Git repository under ROOT from FILES at TIMESTAMP."
  (let ((repository (neomacs-quelpa-test-path root relative)))
    (make-directory repository t)
    (neomacs-quelpa-test-git
     repository "init" "--quiet" "--initial-branch=main" "--object-format=sha1")
    (neomacs-quelpa-test-git repository "config" "core.autocrlf" "false")
    (neomacs-quelpa-test-git repository "config" "core.filemode" "false")
    (neomacs-quelpa-test-git repository "config" "commit.gpgsign" "false")
    (dolist (file files)
      (neomacs-quelpa-test-write repository (car file) (cdr file)))
    (cons repository
          (neomacs-quelpa-test-commit repository timestamp "initial release"))))

(defun neomacs-quelpa-test-advance (repository files timestamp message)
  "Apply FILES to REPOSITORY and commit them at TIMESTAMP with MESSAGE."
  (dolist (file files)
    (neomacs-quelpa-test-write repository (car file) (cdr file)))
  (neomacs-quelpa-test-commit repository timestamp message))

(defun neomacs-quelpa-test-recipe (name repository &rest properties)
  "Return a local Git Quelpa recipe for NAME and REPOSITORY."
  (append (list name
                :fetcher 'git
                :url (concat "file://" repository)
                :branch "main"
                :depth 1)
          properties))

(defun neomacs-quelpa-test-description (name)
  "Return NAME's installed package descriptor."
  (cadr (assq name package-alist)))

(defun neomacs-quelpa-test-version (name)
  "Return NAME's installed version string, or nil."
  (when-let* ((description (neomacs-quelpa-test-description name)))
    (package-version-join (package-desc-version description))))

(defun neomacs-quelpa-test-installed-files (name)
  "Return NAME's exact installed non-dot file names."
  (when-let* ((description (neomacs-quelpa-test-description name)))
    (directory-files (package-desc-dir description) nil "^[^.].*")))

(defun neomacs-quelpa-test-installed-tree (name)
  "Return NAME's complete installed file tree relative to its directory."
  (when-let* ((description (neomacs-quelpa-test-description name))
              (directory (package-desc-dir description)))
    (sort
     (mapcar
      (lambda (file) (file-relative-name file directory))
      (cl-remove-if
       #'file-directory-p
       (directory-files-recursively directory "." nil)))
     #'string<)))

(defun neomacs-quelpa-test-installed-source (name file)
  "Return FILE's exact installed contents for package NAME."
  (when-let* ((description (neomacs-quelpa-test-description name))
              (path (expand-file-name file (package-desc-dir description))))
    (with-temp-buffer
      (insert-file-contents path)
      (buffer-string))))

(defun neomacs-quelpa-test-read-file (path)
  "Return PATH's complete decoded contents."
  (with-temp-buffer
    (insert-file-contents path)
    (buffer-string)))

(defun neomacs-quelpa-test-normalize-sandbox-paths (value)
  "Normalize absolute and workspace-relative oracle sandbox paths in VALUE."
  (let* ((sandbox (directory-file-name (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
         (workspace (file-name-as-directory
                     (getenv "NEOMACS_TEST_WORKSPACE_ROOT")))
         (relative (directory-file-name (file-relative-name sandbox workspace))))
    (dolist (path (list sandbox relative) value)
      (setq value
            (replace-regexp-in-string
             (regexp-quote path) "[ORACLE-SANDBOX]" value t t)))))

(defun neomacs-quelpa-test-normalize-description (description)
  "Return the stable public fields of package DESCRIPTION."
  (when description
    (list :name (package-desc-name description)
          :version (package-version-join (package-desc-version description))
          :summary (package-desc-summary description)
          :requirements (package-desc-reqs description)
          :kind (package-desc-kind description))))

(defun neomacs-quelpa-test-descriptor (name)
  "Return NAME's normalized installed package descriptor."
  (neomacs-quelpa-test-normalize-description
   (neomacs-quelpa-test-description name)))

(defun neomacs-quelpa-test-selected-fixtures ()
  "Return sorted qpt-* entries in `package-selected-packages'."
  (sort
   (cl-remove-if-not
    (lambda (name) (string-prefix-p "qpt-" (symbol-name name)))
    (copy-sequence package-selected-packages))
   (lambda (left right) (string< (symbol-name left) (symbol-name right)))))

(defun neomacs-quelpa-test-cache-record-on-disk ()
  "Read and validate the exact persistent Quelpa cache record."
  (when (file-exists-p quelpa-persistent-cache-file)
    (with-temp-buffer
      (insert-file-contents-literally quelpa-persistent-cache-file)
      (let* ((raw (buffer-substring-no-properties (point-min) (point-max)))
             (read-eval nil)
             (decoded (read-from-string raw))
             (trailing (substring raw (cdr decoded))))
        (unless (string-match-p "\\`[[:space:]]*\\'" trailing)
          (error "Quelpa cache contains trailing protocol bytes"))
        (list :value (car decoded) :raw raw)))))

(defun neomacs-quelpa-test-cache-on-disk ()
  "Return the validated persistent Quelpa cache value."
  (plist-get (neomacs-quelpa-test-cache-record-on-disk) :value))

(defun neomacs-quelpa-test-cache-summary ()
  "Return exact ordered live and persisted Quelpa recipes."
  (let* ((record (neomacs-quelpa-test-cache-record-on-disk))
         (disk (plist-get record :value)))
    (list :live quelpa-cache
          :disk disk
          :raw (plist-get record :raw)
          :same (equal quelpa-cache disk))))

(defun neomacs-quelpa-test-build-head-matches-p (name sha)
  "Return whether NAME's retained build checkout is exactly SHA."
  (let ((checkout (expand-file-name (symbol-name name) quelpa-build-dir)))
    (and (file-directory-p (expand-file-name ".git" checkout))
         (string= (neomacs-quelpa-test-git checkout "rev-parse" "HEAD") sha))))

(defun neomacs-quelpa-test-live-processes ()
  "Return live process names belonging to Quelpa."
  (sort
   (delq nil
         (mapcar
          (lambda (process)
            (when (and (process-live-p process)
                       (string-match-p "quelpa" (process-name process)))
              (process-name process)))
          (process-list)))
   #'string<))

(defun neomacs-quelpa-test-deep-copy (value)
  "Copy VALUE recursively, including string leaves.
The parity observation should describe semantic equality, not expose sharing
introduced by Quelpa's in-memory caches."
  (cond
   ((stringp value) (copy-sequence value))
   ((consp value)
    (cons (neomacs-quelpa-test-deep-copy (car value))
          (neomacs-quelpa-test-deep-copy (cdr value))))
   ((vectorp value)
    (apply #'vector
           (mapcar #'neomacs-quelpa-test-deep-copy value)))
   (t value)))

(defun neomacs-quelpa-test-in-sandbox (name function)
  "Call FUNCTION in an isolated workspace-local Quelpa sandbox NAME."
  (let* ((root (neomacs-quelpa-test-path
                (getenv "NEOMACS_TEST_SANDBOX_ROOT")
                (concat "quelpa-workflows/" name)))
         (elpa (neomacs-quelpa-test-path root "elpa"))
         (temp (neomacs-quelpa-test-path root "tmp")))
    (when (file-exists-p root)
      (delete-directory root t))
    (make-directory elpa t)
    (make-directory temp t)
    (let* ((default-directory (file-name-as-directory root))
           (temporary-file-directory (file-name-as-directory temp))
           (process-environment (copy-sequence process-environment))
           (package-user-dir elpa)
           (package-alist (copy-tree package-alist))
           (package-selected-packages (copy-sequence package-selected-packages))
           (package-activated-list (copy-sequence package-activated-list))
           (package--initialized package--initialized)
           (load-path (copy-sequence load-path))
           (load-history (copy-tree load-history))
           (features (copy-sequence features))
           (print-circle nil)
           (package-check-signature nil)
           (package-native-compile nil)
           (package-quickstart nil)
           (quelpa-dir (neomacs-quelpa-test-path root "quelpa"))
           (quelpa-melpa-dir (neomacs-quelpa-test-path quelpa-dir "melpa"))
           (quelpa-build-dir (neomacs-quelpa-test-path quelpa-dir "build"))
           (quelpa-packages-dir (neomacs-quelpa-test-path quelpa-dir "packages"))
           (quelpa-persistent-cache-file
            (neomacs-quelpa-test-path quelpa-dir "cache"))
           (quelpa-melpa-recipe-stores nil)
           (quelpa-cache nil)
           (quelpa-initialized-p nil)
           (quelpa-checkout-melpa-p nil)
           (quelpa-update-melpa-p nil)
           (quelpa-persistent-cache-p t)
           (quelpa-self-upgrade-p nil)
           (quelpa-upgrade-p nil)
           (quelpa-stable-p nil)
           (quelpa-autoremove-p t)
           (quelpa-git-clone-depth 1)
           (quelpa-git-clone-partial nil)
           (quelpa-build-timeout-executable "timeout")
           (quelpa-build-tar-executable "tar")
           (quelpa-verbose nil)
           (quelpa-build-verbose nil)
           (quelpa-async-p nil)
           (quelpa--override-version-check nil)
           (quelpa--git-version :uninitialized)
           (quelpa--tar-type nil))
      (setenv "TMPDIR" temp)
      (setenv "TZ" "UTC")
      (setenv "LC_ALL" "C")
      (setenv "GIT_CONFIG_NOSYSTEM" "1")
      (setenv "GIT_CONFIG_GLOBAL" (neomacs-quelpa-test-path root "no-gitconfig"))
      (setenv "GIT_TERMINAL_PROMPT" "0")
      (setenv "GIT_ALLOW_PROTOCOL" "file")
      ;; Quelpa runs its Git commands through `make-process' with no
      ;; `:connection-type' (quelpa.el:615-618), so they inherit
      ;; `process-connection-type' -- a PTY.  Git turns its pager ON when
      ;; stdout is a terminal, and it only sets LESS=FRX -- the F that means
      ;; quit-if-one-screen -- when LESS is UNSET.  A developer shell that
      ;; exports LESS for its own reasons therefore leaves `git tag' paging and
      ;; waiting for a keypress that no one can send, until quelpa's own
      ;; `timeout -k 60 600' kills it with status 124.  Measured on this
      ;; machine, `git tag' on a PTY with three tags:
      ;;
      ;;   as the sandbox ran it   exit=124  elapsed=10.0s   (LESS=-R inherited)
      ;;   with LESS unset         exit=0    elapsed=0.0s
      ;;   with GIT_PAGER=cat      exit=0    elapsed=0.0s
      ;;
      ;; The sandbox already refuses the system and global config, the terminal
      ;; prompt and every protocol but file; the pager was the one ambient
      ;; channel left open.  GIT_PAGER outranks both `core.pager' and PAGER.
      (setenv "GIT_PAGER" "cat")
      (unwind-protect
          (neomacs-quelpa-test-deep-copy (funcall function root))
        (dolist (buffer (buffer-list))
          (when-let* ((file (buffer-file-name buffer))
                      ((file-in-directory-p file root)))
            (with-current-buffer buffer
              (set-buffer-modified-p nil))
            (kill-buffer buffer)))
        (dolist (buffer '("*quelpa-build-checkout*" "*quelpa-build-info*"))
          (when (get-buffer buffer)
            (with-current-buffer buffer
              (set-buffer-modified-p nil))
            (kill-buffer buffer)))
        (let (symbols)
          (mapatoms
           (lambda (symbol)
             (when (string-prefix-p "qpt-" (symbol-name symbol))
               (push symbol symbols))))
          (dolist (symbol symbols)
            (setq features (delq symbol features))
            (when (fboundp symbol)
              (fmakunbound symbol))
            (when (boundp symbol)
              (makunbound symbol))
            (setplist symbol nil)))
        (when (file-exists-p root)
          (delete-directory root t))))))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(QUELPA_MELPA_PIN, "quelpa.el")
        .expect("prepare exact shallow Quelpa source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

#[test]
fn quelpa_package_batch() {
    assert_oracle_batch_cases(
        oracle(),
        "quelpa_package_batch",
        "quelpa_parity",
        &workflows::workflow_batch_cases(),
    );
}
