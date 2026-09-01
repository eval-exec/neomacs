use std::time::Duration;

use crate::{ADVENT_MODE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ADVENT_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// advent-mode helps with Advent of Code: it stores the session cookie, infers
/// the year and day from where you are in your solutions tree, scaffolds a day
/// directory, downloads the puzzle input and submits answers.
///
/// The network is the boundary, and the package reaches it through exactly one
/// call -- `url-retrieve-synchronously' inside `advent--http-request'.  That
/// call is the only thing stood in for, which is the arrangement the standards
/// describe: "stub the HTTP transport, then let the package's public command
/// perform its real parsing, transformation, buffer rendering, and file
/// writing".  Nothing reaches adventofcode.com.
///
/// The stand-in records what the package asked for -- URL, method, extra
/// headers, urlencoded body -- together with the `Cookie' header url.el itself
/// generates from the cookie `advent-login' really stored, so the cookie under
/// test is the one the request would carry rather than one the fixture made up.
/// It answers with raw HTTP bytes (status line, headers, blank line, body), so
/// the package's own status extraction, header/body split, error handling,
/// file writing and buffer rendering all run for real.
///
/// Everything else is real: a real solutions tree in the sandbox, real
/// directory-name inference through the configured formats, real cookie
/// storage in url-cookie, and real files on disk.
const ADVENT_MODE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'url)
(require 'url-cookie)

(defvar adv-test-root
  (file-name-as-directory
   (expand-file-name "aoc" (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
  "The user's Advent of Code solutions root.")

(defvar adv-test-requests nil
  "Every request the package made, newest last.")

(defvar adv-test-replies nil
  "Raw HTTP responses to serve, in order.")

(defconst adv-test-session
  "53616c7465645f5fdeadbeefcafef00d0123456789abcdef"
  "A session cookie of the shape adventofcode.com issues.")

(defun adv-test-write (path text)
  (make-directory (file-name-directory path) t)
  (let ((coding-system-for-write 'utf-8-unix))
    (with-temp-buffer
      (insert text)
      (write-region (point-min) (point-max) path nil 'silent)))
  path)

(defun adv-test-reply (status headers body)
  "Return a raw HTTP response the way adventofcode.com sends one."
  (concat (format "HTTP/1.1 %s\r\n" status)
          (mapconcat (lambda (h) (format "%s\r\n" h)) headers "")
          "\r\n"
          body))

(defun adv-test-serve (&rest replies)
  "Queue REPLIES for the next requests and clear the request log."
  (setq adv-test-replies replies
        adv-test-requests nil))

(defun adv-test-install-transport ()
  "Stand in for the network transport, and only for that.
`url-retrieve-synchronously' is the one call the package makes to reach
adventofcode.com.  The replacement records what the package asked for --
including the `Cookie' header url.el itself generates from the real cookie
store -- and hands back a raw HTTP response, which the package then parses,
writes and renders with its own code."
  (advice-add
   'url-retrieve-synchronously :override
   (lambda (url &rest _)
     (let ((parsed (url-generic-parse-url url)))
       (push (list :url url
                   :method (or url-request-method "GET")
                   :extra-headers url-request-extra-headers
                   :data url-request-data
                   :cookie-header
                   (url-cookie-generate-header-lines
                    (url-host parsed) (or (url-filename parsed) "/")
                    (equal (url-type parsed) "https")))
             adv-test-requests))
     (let ((reply (pop adv-test-replies)))
       (and reply
            (let ((buffer (generate-new-buffer " *adv-http*")))
              (with-current-buffer buffer
                (set-buffer-multibyte nil)
                (insert reply)
                (goto-char (point-min)))
              buffer))))
   '((name . adv-test-transport))))

(defun adv-test-requests ()
  "Every request the package made, oldest first."
  (reverse adv-test-requests))

(defun adv-test-project (&rest relative-dirs)
  "Create RELATIVE-DIRS under the solutions root and return the root."
  (dolist (dir relative-dirs)
    (make-directory (expand-file-name dir adv-test-root) t))
  (setq advent-root-dir adv-test-root))

(defun adv-test-visit (relative)
  "Visit RELATIVE under the solutions root in a live window."
  (let* ((path (expand-file-name relative adv-test-root))
         (buffer (find-file-noselect path)))
    (set-window-buffer (selected-window) buffer)
    (set-buffer buffer)
    buffer))

(defun adv-test-in-dir (relative)
  "Make a non-file buffer whose `default-directory' is RELATIVE under root."
  (let ((buffer (generate-new-buffer "*aoc-scratch*")))
    (set-window-buffer (selected-window) buffer)
    (set-buffer buffer)
    (setq default-directory
          (file-name-as-directory (expand-file-name relative adv-test-root)))
    buffer))

(defun adv-test-tree ()
  "Every file and directory under the solutions root, relative and sorted."
  (let ((root (expand-file-name adv-test-root)))
    (sort (mapcar (lambda (path)
                    (let ((relative (file-relative-name path root)))
                      (if (file-directory-p path)
                          (concat relative "/")
                        relative)))
                  (cl-remove-if
                   (lambda (path) (string-match-p "/\\.\\.?\\'" path))
                   (directory-files-recursively root "" t)))
          #'string<)))

(defun adv-test-file-text (relative)
  (let ((path (expand-file-name relative adv-test-root)))
    (and (file-exists-p path)
         (with-temp-buffer
           (let ((coding-system-for-read 'utf-8))
             (insert-file-contents path))
           (buffer-string)))))

(defun adv-test-messages (regexp)
  (with-current-buffer (get-buffer-create "*Messages*")
    (cl-remove-if-not
     (lambda (line) (string-match-p regexp line))
     (split-string
      (buffer-substring-no-properties (point-min) (point-max)) "\n" t))))

(defun adv-test-lighter ()
  "The lighter redisplay would show for `advent-mode' in this buffer.
`format-mode-line' returns \"\" in batch in both editors -- there is no mode
line to format -- so the `:eval' form the mode registered in the public
`minor-mode-alist' is evaluated the way redisplay evaluates it."
  (let ((lighter (cadr (assq 'advent-mode minor-mode-alist))))
    (and (consp lighter) (eq (car lighter) :eval)
         (eval (cadr lighter) t))))

(defmacro adv-test-answering (answers &rest body)
  "Run BODY with `y-or-n-p' answering from ANSWERS, recording the prompts.
ANSWERS is a list of booleans consumed in order.  Batch execution cannot
answer a prompt, so this is the interactive-input double the standards allow;
the prompts themselves are recorded so they stay under test."
  `(let ((adv-test-prompts nil)
         (adv-test-answers ,answers))
     (cl-letf (((symbol-function 'y-or-n-p)
                (lambda (prompt)
                  (push prompt adv-test-prompts)
                  (pop adv-test-answers))))
       (let ((result (progn ,@body)))
         (list :prompts (reverse adv-test-prompts) :result result)))))
"##;

fn advent_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ADVENT_MODE_MELPA_PIN, "advent-mode.el")
        .expect("prepare pinned advent-mode source below ./tmp")
        .with_prelude(ADVENT_MODE_TEST_PRELUDE)
        .with_timeout(ADVENT_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed advent-mode parity test")
        .into()
}

/// Multi-probe batch for `assert_advent_mode_parity` cases (2a).
pub(crate) fn assert_advent_mode_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(advent_mode_oracle(), &name, "advent_mode_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn advent_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_advent_mode_batch(&cases);
}

// END generated package batch tests
