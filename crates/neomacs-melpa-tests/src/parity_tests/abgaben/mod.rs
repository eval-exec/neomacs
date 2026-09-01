use std::time::Duration;

use crate::{ABGABEN_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ABGABEN_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// abgaben grades mailed student submissions: mu4e hands it an attachment, it
/// files the attachment below `abgaben-root-folder`, unpacks archives, links
/// the result from an org outline, exports pdf-tools annotations back into
/// that outline and finally prepares the reply mail.
///
/// Exactly three boundaries are replaced below, because a batch process cannot
/// have them: the `epdfinfo` server pdf-tools talks to, a mu4e mail store, and
/// the minibuffer.  Everything else the workflows touch — org parsing, the
/// archive helpers, the real file system and the real `mkdir`/`tar`/`unzip`
/// subprocesses — is abgaben's own code running for real.
const ABGABEN_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'org)

;; pdf-tools queries epdfinfo for its compile-time features while `pdf-annot'
;; is loading.  Declaring the feature set of an ordinary epdfinfo build lets
;; the real `pdf-annot' library load, so the workflows use the real annotation
;; accessors and the real `pdf-annot-compare-annotations' ordering.
(require 'pdf-info)
(setq pdf-info-features '(writable-annotations markup-annotations))
(require 'pdf-annot)

(defvar abgaben-test-root
  (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

;; Keep the sandbox listings free of editor bookkeeping files.
(setq make-backup-files nil
      create-lockfiles nil)

(defvar abgaben-test-events nil
  "Reverse-ordered log of everything the stand-ins observed.")

(defun abgaben-test-record (event)
  (push event abgaben-test-events)
  event)

(defun abgaben-test-events ()
  (reverse abgaben-test-events))

(defun abgaben-test-relative (path)
  "Return PATH relative to the per-case sandbox root."
  (file-relative-name path abgaben-test-root))

(defun abgaben-test-write-file (path contents)
  (make-directory (file-name-directory path) t)
  (with-temp-buffer
    (insert contents)
    (write-region (point-min) (point-max) path nil 'silent))
  path)

(defun abgaben-test-tree (directory)
  "Sorted relative paths of every file below DIRECTORY."
  (if (file-directory-p directory)
      (sort (mapcar (lambda (path) (file-relative-name path directory))
                    (directory-files-recursively directory ".*"))
            #'string<)
    'no-such-directory))

(defun abgaben-test-buffer-text (&optional buffer)
  (with-current-buffer (or buffer (current-buffer))
    (buffer-substring-no-properties (point-min) (point-max))))

(defun abgaben-test-contents (path)
  (if (file-regular-p path)
      (with-temp-buffer
        (let ((coding-system-for-read 'utf-8))
          (insert-file-contents path))
        (buffer-string))
    'no-such-file))

;; --- mu4e -----------------------------------------------------------------
;; mu4e keeps the extracted attachment in `mu4e-attachment-dir'; abgaben binds
;; that variable around the save.  The stand-ins mirror mu4e's own accessor
;; contract: `mu4e~view-get-attach' is a 1-based index into the message's
;; :attachments, `mu4e-view-save-attachment-single' writes attachment ATTNUM
;; into `mu4e-attachment-dir'.
(defvar mu4e-attachment-dir "~/")

(defun abgaben-test-attachment (msg attnum)
  (nth (1- attnum) (plist-get msg :attachments)))

(defun mu4e~view-get-attach (msg attnum)
  (let ((attachment (abgaben-test-attachment msg attnum)))
    (abgaben-test-record
     (list 'get-attach attnum (plist-get attachment :name)))
    attachment))

(defun mu4e-view-save-attachment-single (msg attnum)
  (let* ((attachment (abgaben-test-attachment msg attnum))
         (target (expand-file-name (plist-get attachment :name)
                                   mu4e-attachment-dir)))
    (abgaben-test-record
     (list 'save-attachment (plist-get attachment :name)
           (abgaben-test-relative target)))
    (copy-file (plist-get attachment :source) target t)
    target))

(provide 'mu4e)

;; mu4e-org registers the `mu4e:' org link type and opens the message.
(org-link-set-parameters
 "mu4e"
 :follow (lambda (path &optional _arg)
           (abgaben-test-record (list 'open-mail path))))

;; --- minibuffer -----------------------------------------------------------
(defvar abgaben-test-answers nil
  "Queued minibuffer answers for `abgaben--get-group' / `abgaben--get-week'.")

;; `completing-read' hands its whole argument list to this function, so the
;; recorded argument count is part of the observed contract.
(setq completing-read-function
      (lambda (&rest arguments)
        (let ((answer (pop abgaben-test-answers)))
          (abgaben-test-record
           (list 'completing-read (length arguments)
                 (nth 0 arguments) (nth 1 arguments) (nth 3 arguments)
                 (nth 4 arguments) (nth 6 arguments) answer))
          answer)))

;; --- archive tooling ------------------------------------------------------
(defun abgaben-test-install-unzip ()
  "Install a recording `unzip' stand-in ahead of PATH.
No zip archiver exists in the sandbox, so the stand-in both records its exact
argument vector and extracts the archive's plain-text entry list, leaving
abgaben's real `call-process' path intact."
  (let ((program (expand-file-name "bin/unzip" abgaben-test-root)))
    (abgaben-test-write-file
     program
     (concat "#!/bin/sh\n"
             "printf '%s\\n' \"unzip $*\" >> \"$ABGABEN_TEST_LOG\"\n"
             "archive=$1\n"
             "target=.\n"
             "if [ \"$2\" = -d ]; then target=$3; fi\n"
             "mkdir -p \"$target\"\n"
             "sed -n 's/^entry //p' \"$archive\" | while read -r name; do\n"
             "  printf 'unpacked %s\\n' \"$name\" > \"$target/$name\"\n"
             "done\n"
             "exit 0\n"))
    (set-file-modes program #o755)
    (setenv "ABGABEN_TEST_LOG"
            (expand-file-name "commands.log" abgaben-test-root))
    (setenv "PATH" (concat (expand-file-name "bin" abgaben-test-root)
                           path-separator (getenv "PATH")))
    ;; `call-process' resolves bare program names through `exec-path'.
    (push (directory-file-name (file-name-directory program)) exec-path)
    program))

(defun abgaben-test-commands ()
  (let ((log (expand-file-name "commands.log" abgaben-test-root)))
    (if (file-regular-p log)
        (split-string (abgaben-test-contents log) "\n" t)
      'no-command-ran)))

(defun abgaben-test-make-tarball (path files)
  "Create a real gzip tar archive at PATH holding FILES ((NAME . TEXT)...)."
  (let ((staging (expand-file-name "mailstore/staging/" abgaben-test-root)))
    (delete-directory staging t)
    (make-directory staging t)
    (dolist (file files)
      (abgaben-test-write-file (expand-file-name (car file) staging)
                               (cdr file)))
    (make-directory (file-name-directory path) t)
    (let ((default-directory staging))
      (unless (eq 0 (apply #'call-process "tar" nil nil nil
                           "-czf" path (mapcar #'car files)))
        (error "could not build the tar fixture")))
    path))

(defun abgaben-test-org-file (name contents)
  (abgaben-test-write-file
   (expand-file-name name abgaben-test-root) contents))
"##;

fn abgaben_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ABGABEN_MELPA_PIN, "abgaben.el")
        .expect("prepare pinned abgaben source below ./tmp")
        .with_prelude(ABGABEN_TEST_PRELUDE)
        .with_timeout(ABGABEN_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed abgaben parity test")
        .into()
}

/// Multi-probe batch for `assert_abgaben_parity` cases (2a).
pub(crate) fn assert_abgaben_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(abgaben_oracle(), &name, "abgaben_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn abgaben_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_abgaben_batch(&cases);
}

// END generated package batch tests
