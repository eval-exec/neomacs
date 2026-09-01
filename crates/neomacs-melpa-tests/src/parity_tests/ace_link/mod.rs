use std::time::Duration;

use crate::{ACE_LINK_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ACE_LINK_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// ace-link labels every link visible in the selected window and follows the
/// one whose key the user presses, so every workflow needs a real buffer in the
/// selected window and real keys: avy reads its label key with `read-key',
/// which during `execute-kbd-macro' consumes the macro's remaining keys.
///
/// Nothing in ace-link or avy is stubbed.  The labels are observed the way a
/// user sees them -- the overlays carrying an avy lead face, read from
/// `avy-translate-char-function', avy's public hook for non-QWERTY layouts,
/// which runs while those overlays are still on screen.  The only test doubles
/// are the two browser functions, a true external boundary; both are declared
/// with a `browse-url-browser-kind' so `browse-url' routes to them instead of
/// launching a real browser.
const ACE_LINK_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defvar ace-link-test-keys nil
  "Each key avy read, with the labels visible at that moment.")

(defvar ace-link-test-browsed nil
  "Each URL handed to a browser, newest first.")

(defun ace-link-test-path (name)
  "Return the absolute sandbox path of NAME."
  (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defun ace-link-test-compilation-complete-p (buffer)
  "Non-nil once `compilation-handle-exit' has written BUFFER's last line.
That is the causal end of the child's output rather than a guess about
it: Emacs drains a dying process's remaining reads before it runs the
sentinel, the sentinel is what calls `compilation-handle-exit', and that
function marks the text it writes with a `compilation-handle-exit' text
property (lisp/progmodes/compile.el:2630).  The property cannot appear
until every byte the child wrote has been through `compilation-filter'."
  (and (buffer-live-p (get-buffer buffer))
       (with-current-buffer buffer
         (and (text-property-not-all (point-min) (point-max)
                                     'compilation-handle-exit nil)
              t))))

(defun ace-link-test-await-compilation (buffer)
  "Wait until BUFFER holds all of its compilation's output, or signal.
The child's process dying is not that condition: it can be gone with
reads still queued, and the link candidates `ace-link-compilation'
collects from a half-filled buffer are a fact about the kernel's
scheduling rather than about either editor -- the defect DIVERGENCES.md
133 removed from the `rg' suite.  Signalling rather than returning means a
future edit that reintroduces the race fails on its first run."
  (let ((rounds 0))
    (while (and (< rounds 1200)
                (not (ace-link-test-compilation-complete-p buffer)))
      (accept-process-output nil 0.05)
      (setq rounds (1+ rounds)))
    (unless (ace-link-test-compilation-complete-p buffer)
      (error "ace-link-test-await-compilation: %s never reached \
`compilation-handle-exit'; its links would describe only as much of the \
child's output as had been read" buffer))
    :finished))

(defun ace-link-test-write (name text)
  "Write TEXT to sandbox file NAME and return its path."
  (let ((path (ace-link-test-path name)))
    (make-directory (file-name-directory path) t)
    (with-temp-buffer
      (insert text)
      (write-region (point-min) (point-max) path nil 'silent))
    path))

(defconst ace-link-test-info-manual
  (concat "This is sandbox.info, produced by hand.\n"
          "\n\037\nFile: sandbox.info,  Node: Top,  Next: Basics,  Up: (dir)\n\n"
          "Sandbox Manual\n**************\n\n* Menu:\n\n"
          "* Basics::      How to begin.\n"
          "* Advanced::    Deeper water.\n"
          "\n\037\nFile: sandbox.info,  Node: Basics,  Next: Advanced,  Prev: Top,  Up: Top\n\n"
          "1 Basics\n========\n\nSee *note Advanced:: for the rest, or go *note Top::.\n"
          "\n\037\nFile: sandbox.info,  Node: Advanced,  Prev: Basics,  Up: Top\n\n"
          "2 Advanced\n==========\n\nBack to *note Basics::.\n")
  "A small hand-written manual with a menu and cross references.")

(defun ace-link-test-open-manual ()
  "Visit the sandbox Info manual in the selected window."
  (ace-link-test-write "manual/sandbox.info" ace-link-test-info-manual)
  (info (ace-link-test-path "manual/sandbox.info"))
  (set-window-buffer (selected-window) (current-buffer))
  (current-buffer))

(defun ace-link-test-labels ()
  "Return the avy labels a user can see right now.
Each entry is (LINE COLUMN LABEL TEXT-AT-LABEL).

The position is recorded as line and column rather than as a buffer
offset on purpose.  Some of these buffers quote the sandbox root -- the
*Help* buffer names the defining file, and a compilation buffer records
`default-directory' in its header -- so every offset past that point
carries the length of the sandbox path.  The oracle masks the path
inside captured strings but cannot mask it inside an integer, which made
the recorded expectation depend on how long this checkout's path
happens to be.  Line and column do not move when the path length
changes, and together with TEXT-AT-LABEL they pin the candidate more
precisely than the offset did."
  (let (labels)
    (dolist (overlay (overlays-in (point-min) (point-max)))
      (let ((text (or (overlay-get overlay 'display)
                      (overlay-get overlay 'before-string)
                      (overlay-get overlay 'after-string))))
        (when (and (stringp text)
                   (> (length text) 0)
                   (memq (get-text-property 0 'face text)
                         '(avy-lead-face avy-lead-face-0)))
          (push (save-excursion
                  (goto-char (overlay-start overlay))
                  (list (line-number-at-pos)
                        (current-column)
                        (substring-no-properties text)
                        (buffer-substring-no-properties (point)
                                                        (line-end-position))))
                labels))))
    (sort labels (lambda (a b)
                   (or (< (car a) (car b))
                       (and (= (car a) (car b))
                            (< (cadr a) (cadr b))))))))

(defun ace-link-test-record-key (char)
  "Record CHAR and the labels on screen, then return CHAR unchanged."
  (push (list (key-description (vector char)) (ace-link-test-labels))
        ace-link-test-keys)
  char)

(defun ace-link-test-pressed ()
  "Return the recorded keys in the order the user pressed them."
  (reverse ace-link-test-keys))

(defun ace-link-test-browser (url &rest _)
  "Record URL as opened in the primary browser."
  (push (list 'browse url) ace-link-test-browsed)
  'browsed)
(function-put 'ace-link-test-browser 'browse-url-browser-kind 'internal)

(defun ace-link-test-external-browser (url &rest _)
  "Record URL as opened in the secondary, external browser."
  (push (list 'browse-external url) ace-link-test-browsed)
  'browsed-externally)
(function-put 'ace-link-test-external-browser 'browse-url-browser-kind 'external)

(defun ace-link-test-capture-browsers ()
  "Route every browser call into `ace-link-test-browsed'."
  (require 'browse-url)
  (setq browse-url-browser-function #'ace-link-test-browser
        browse-url-secondary-browser-function #'ace-link-test-external-browser
        browse-url-handlers nil))

(defun ace-link-test-browsed ()
  "Return the recorded browser calls in call order."
  (reverse ace-link-test-browsed))

(defun ace-link-test-where ()
  "Report where the user ended up."
  (list :buffer (buffer-name)
        :window-buffer (buffer-name (window-buffer (selected-window)))
        :mode major-mode
        :point (- (point) (point-min))
        :line (line-number-at-pos)
        :column (current-column)
        :line-text (buffer-substring-no-properties
                    (line-beginning-position) (line-end-position))))

(defmacro ace-link-test-session (&rest body)
  "Run BODY with key and browser recording, then kill the buffers it made."
  `(let ((existing (buffer-list)))
     (setq ace-link-test-keys nil
           ace-link-test-browsed nil
           avy-translate-char-function #'ace-link-test-record-key)
     (unwind-protect
         (progn ,@body)
       (dolist (buffer (buffer-list))
         (unless (memq buffer existing)
           (with-current-buffer buffer
             (set-buffer-modified-p nil))
           (kill-buffer buffer))))))
"##;

fn ace_link_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ACE_LINK_MELPA_PIN, "ace-link.el")
        .expect("prepare pinned ace-link source below ./tmp")
        .with_prelude(ACE_LINK_TEST_PRELUDE)
        .with_timeout(ACE_LINK_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed ace-link parity test")
        .into()
}

/// Multi-probe batch for `assert_ace_link_parity` cases (2a).
pub(crate) fn assert_ace_link_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ace_link_oracle(), &name, "ace_link_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn ace_link_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ace_link_batch(&cases);
}

// END generated package batch tests
