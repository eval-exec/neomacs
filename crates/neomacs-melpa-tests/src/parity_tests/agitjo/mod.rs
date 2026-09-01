use std::time::Duration;

use crate::{AGITJO_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod composition;
mod publish;
mod workflows;

const AGITJO_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// Sandbox helpers shared by the workflows.
///
/// agitjo drives Forgejo's AGit-Flow: it turns a local branch plus a drafted
/// Markdown description into a single `git push' whose refspec encodes the
/// pull request target and session, and whose push options carry the title and
/// the base64 encoded description.  The workflows therefore build real git
/// repositories below the per-case sandbox -- real branches, real commits with
/// bodies, real templates committed on a real `origin/main' -- and assert the
/// argument vector that would reach `git push', because that vector *is* the
/// pull request the user gets.
///
/// The one boundary that is stood in for is the push itself: `magit-run-git-async'
/// would contact a Forgejo instance.  The stand-in records the package's own
/// argument vector verbatim and returns a real subprocess with a chosen exit
/// status, so the failure and success halves of the draft lifecycle are driven
/// by a real sentinel.  Nothing about the recorded vector is synthesised -- it
/// is the package's output, not the stand-in's answer -- which is the direction
/// that keeps a stand-in from becoming a double for the logic under test.
///
/// These helpers are definitions only.  `composition.rs' and `publish.rs'
/// predate them, build their fixtures inline, and are unaffected.
const AGITJO_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'subr-x)

(defvar agitjo-test-root
  (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defvar agitjo-test-push-requests nil
  "Every argument vector the push stand-in was handed, oldest first.")

(defvar agitjo-test-sentinel-events nil
  "Every (NAME STATUS EXIT-CODE EVENT) the stand-in sentinel observed.")

(defvar agitjo-test-last-process nil
  "The subprocess the push stand-in returned most recently.")

(defconst agitjo-test-description-prefix "--push-option=description=")

(defun agitjo-test-relative (path)
  "Spell PATH relative to the sandbox so snapshots stay stable."
  (if (stringp path) (file-relative-name path agitjo-test-root) path))

(defun agitjo-test-write (path content)
  "Write CONTENT to PATH, creating its directory, and return PATH."
  (make-directory (file-name-directory path) t)
  (let ((coding-system-for-write 'utf-8-unix))
    (with-temp-buffer
      (insert content)
      (write-region (point-min) (point-max) path nil 'silent)))
  path)

(defun agitjo-test-git (&rest arguments)
  "Run git in `default-directory', returning its trimmed output.

Signals rather than returning quietly on a non-zero status, so a fixture that
failed to establish itself cannot be mistaken for a package result."
  (with-temp-buffer
    (let ((status (apply #'process-file "git" nil t nil arguments)))
      (unless (zerop status)
        (error "git %S failed: %s" arguments (buffer-string)))
      (string-trim (buffer-string)))))

(defun agitjo-test-repo (name files)
  "Create a git repository NAME below the sandbox and return its root.

FILES is a list of (RELATIVE-PATH . CONTENT) committed on `main'.  An `origin'
remote is established whose `main' tracks that commit, which is the shape a
clone from a forge has, and is what agitjo reads templates from -- no network
is involved and the remote URL is deliberately unreachable."
  (let* ((root (file-name-as-directory (expand-file-name name agitjo-test-root)))
         (default-directory root))
    (when (file-exists-p root) (delete-directory root t))
    (make-directory root t)
    (agitjo-test-git "init" "-b" "main")
    (agitjo-test-git "config" "user.name" "Neomacs Oracle")
    (agitjo-test-git "config" "user.email" "oracle@example.invalid")
    (dolist (file files)
      (agitjo-test-write (expand-file-name (car file) root) (cdr file)))
    (agitjo-test-git "add" ".")
    (agitjo-test-git "commit" "-m" "Establish baseline")
    (agitjo-test-git "remote" "add" "origin"
                     (expand-file-name "unreachable-origin.git" root))
    (agitjo-test-git "update-ref" "refs/remotes/origin/main"
                     (agitjo-test-git "rev-parse" "HEAD"))
    root))

(defun agitjo-test-branch (root name files subject &optional body)
  "Commit FILES on a new branch NAME in ROOT with SUBJECT and optional BODY."
  (let ((default-directory root))
    (agitjo-test-git "switch" "-c" name)
    (dolist (file files)
      (agitjo-test-write (expand-file-name (car file) root) (cdr file)))
    (agitjo-test-git "add" ".")
    (if body
        (agitjo-test-git "commit" "-m" subject "-m" body)
      (agitjo-test-git "commit" "-m" subject))
    name))

(defun agitjo-test-transient-keys (prefix)
  "Return every (KEY . COMMAND) reachable in PREFIX's transient layout."
  (let (found)
    (letrec ((walk
              (lambda (node)
                (cond
                 ((vectorp node) (mapc walk (append node nil)))
                 ((and (consp node) (plist-member (cdr-safe node) :key))
                  (push (cons (copy-sequence (plist-get (cdr node) :key))
                              (plist-get (cdr node) :command))
                        found))
                 ((consp node) (mapc walk node))))))
      (funcall walk (get prefix 'transient--layout)))
    (nreverse found)))

(defun agitjo-test-normalize-push (arguments)
  "Spell ARGUMENTS as a flat argv, decoding the base64 description option.

agitjo base64 encodes the description because git push refuses options
containing newlines, so the readable form is what a reviewer needs to see."
  (mapcar
   (lambda (argument)
     (if (and (stringp argument)
              (string-prefix-p agitjo-test-description-prefix argument))
         (list :description
               (decode-coding-string
                (base64-decode-string
                 (substring argument
                            (+ (length agitjo-test-description-prefix)
                               (length "{base64}"))))
                'utf-8-unix))
       (copy-sequence argument)))
   (flatten-list arguments)))

(defun agitjo-test-push-stand-in (exit-code &optional forge-output)
  "Return a `magit-run-git-async' stand-in recording argv, exiting EXIT-CODE.

FORGE-OUTPUT, when given, is written into the Magit process buffer the way
magit's own filter would, so `agitjo-visit-last-pushed-pullreq' reads it back
through its real code path."
  (lambda (&rest arguments)
    (push (agitjo-test-normalize-push arguments) agitjo-test-push-requests)
    (when forge-output
      (with-current-buffer (magit-process-buffer t)
        (goto-char (point-max))
        (let ((inhibit-read-only t))
          (insert forge-output))))
    (setq agitjo-test-last-process
          (make-process :name (format "agitjo-test-push-%s" exit-code)
                        :command (list "sh" "-c"
                                       (format "sleep 0.05; exit %s" exit-code))
                        :connection-type 'pipe
                        :noquery t))))

(defun agitjo-test-record-sentinel (process event)
  "Stand-in for `magit-process-sentinel' that records what it was told."
  (push (list (process-name process)
              (process-status process)
              (process-exit-status process)
              (copy-sequence event))
        agitjo-test-sentinel-events))

(defun agitjo-test-await (process)
  "Block until PROCESS has exited and its sentinel has run."
  (while (process-live-p process)
    (accept-process-output process 0.05))
  (accept-process-output process 0.05))

(defun agitjo-test-requests ()
  "Return the recorded push argument vectors, oldest first."
  (reverse agitjo-test-push-requests))

(defun agitjo-test-events ()
  "Return the recorded sentinel events, oldest first."
  (reverse agitjo-test-sentinel-events))

(defun agitjo-test-draft-contents (file)
  "Return FILE's contents, or that it is absent."
  (if (not (file-exists-p file))
      'no-draft-file
    (with-temp-buffer
      (insert-file-contents file)
      (buffer-string))))
"##;

fn agitjo_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AGITJO_MELPA_PIN, "agitjo.el")
        .expect("prepare pinned agitjo source below ./tmp")
        .with_prelude(AGITJO_TEST_PRELUDE)
        .with_timeout(AGITJO_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed agitjo parity test").into()
}

/// Multi-probe batch for `assert_agitjo_parity` cases (2a).
pub(crate) fn assert_agitjo_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(agitjo_oracle(), &name, "agitjo_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn agitjo_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        composition::composition_public_surface_batch_cases(),
        publish::publish_public_surface_batch_cases(),
        workflows::workflows_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_agitjo_batch(&cases);
}

// END generated package batch tests
