use std::time::Duration;

use crate::{CachedMelpaOracle, RG_MELPA_PIN, TRANSIENT_MELPA_PIN, WGREP_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const RG_TEST_TIMEOUT: Duration = Duration::from_secs(240);

/// Every workflow enters the documented way: `rg-run' a pattern over a real
/// fixture tree, which starts the pinned `rg' executable through
/// `compilation-start' and lands in an `rg-mode' results buffer -- always
/// through `rg-test-run', whose docstring explains why a search that reaches
/// a pin may not be PTY-connected (DIVERGENCES.md entry 133).  A batch
/// editor runs asynchronous processes through `accept-process-output', so
/// the workflows wait for the search process the same way an interactive
/// session's sentinel would.  The suite pins the grouped results buffer
/// (file headers, match rows, faces), the file navigation commands, the
/// hidden command line, the wgrep edit round trip that writes the disk, and
/// the configuration surface.
///
/// transient 20260725.1105 and wgrep 20230203.1214 are prepared through
/// `with_melpa_dependency'; all three versions are pinned.
const RG_TEST_PRELUDE: &str = r####"
(require 'cl-lib)

(defconst rg-test-upstream-tree
  "77f2abe594fb0a6e6ec827dceaf70ef50f897e7c"
  "Git tree of the pinned upstream commit this suite installs.")

(defconst rg-test-manifest
  '(("rg-pkg.el"
     . "e0e20555beb379323a8378234388b836906fe0d551b79c3ebb644742e05b6da3")
    ("rg.el"
     . "1f569309e3c48bd6667e92df0801b74732ba9a396d357f3735f2bfe1082f54bc")
    ("rg-header.el"
     . "b146ab12c398b49ddabde4001f85fd35e9a4ef3be6594acdaab49982176cba0e")
    ("rg-history.el"
     . "2e3c502badcf0d0375609a35bf36276440d3995fbfc62570d2d0b90e71befadc")
    ("rg-ibuffer.el"
     . "8234df9749036ddc8e2549337aae880bef232f255b83fca85604f1c3808bd981")
    ("rg-info-hack.el"
     . "f928c96dccf156b083813cc316539ddab80eb47c864e74d6901f8e34bb813798")
    ("rg-isearch.el"
     . "4e78df5e5526e183df8f962bb13b4ad50edcba450f4379d065d29a4b25c46703")
    ("rg-menu.el"
     . "72364fc6cac27fa4aee6a8f2c7c326d1ab847fe9643d596e040ec885de51d2e4")
    ("rg-result.el"
     . "93a53f556033fb342ba6facd4a26392dc212d60aff3eb10d21a69a240bd79b81")
    ("wgrep-rg.el"
     . "d614a7a80fc0d988f6cf086b5c8d74faf288cdccc2b6a23c683a0bcd657c7ccc"))
  "Per-file sha256 of the package-built sources the suite verifies.")

(defun rg-test-source-state ()
  "Verify the installed payload is the pinned upstream build."
  (let* ((located (locate-library "rg.el"))
         (main (and located (file-truename located)))
         (directory (and main (file-name-directory main))))
    (unless (and main directory
                 (string-suffix-p "/rg.el" main)
                 (not (file-symlink-p main)))
      (error "Unexpected installed rg location: %S" located))
    (dolist (entry rg-test-manifest)
      (let ((file (expand-file-name (car entry) directory)))
        (with-temp-buffer
          (set-buffer-multibyte nil)
          (insert-file-contents-literally file)
          (unless (and (file-regular-p file)
                       (not (file-symlink-p file))
                       (equal (secure-hash 'sha256 (current-buffer))
                              (cdr entry)))
            (error "Unexpected installed rg source: %S" (car entry))))))
    (list :upstream-tree rg-test-upstream-tree
          :feature (featurep 'rg)
          :version (package-version-join
                    (package-desc-version (cadr (assq 'rg package-alist))))
          :transient (package-version-join
                      (package-desc-version
                       (cadr (assq 'transient package-alist))))
          :wgrep (package-version-join
                  (package-desc-version
                   (cadr (assq 'wgrep package-alist))))
          :executable (file-name-nondirectory (rg-executable)))))

(defun rg-test-root (name)
  "A fresh fixture root under the sandbox for NAME."
  (let ((root (file-name-as-directory
               (expand-file-name
                (concat "rg-fixture-" name)
                (getenv "NEOMACS_TEST_SANDBOX_ROOT")))))
    (ignore-errors (delete-directory root t))
    (make-directory root t)
    root))

(defun rg-test-write (file contents)
  "Write CONTENTS to FILE as deterministic UTF-8 Unix text."
  (make-directory (file-name-directory file) t)
  (let ((coding-system-for-write 'utf-8-unix))
    (with-temp-file file (insert contents))))

(defun rg-test-run (pattern root)
  "Start a search for PATTERN under ROOT and return the results buffer.

Every search this suite starts goes through here, because the results
buffer's TEXT is only well-defined when the whole `rg' output reaches
`rg-filter' as ONE chunk, and that is a property of the process's I/O
topology rather than of the editor under test.

`rg-filter' inserts a newline on EVERY invocation until the first match
is counted:

    (when (zerop rg-hit-count)
      (newline))

(rg-result.el:454-455 in the pinned 20260517.1310 build) sits OUTSIDE
the `(when (< (point) end) ...)' guard at :458 that skips a chunk with
no complete line, and `rg-hit-count' is bumped only where a match escape
is rewritten, at :482.  One filter call before the first complete match
line therefore yields the pinned blank line between the command line and
the first `File:' heading; TWO yield a second blank line, after it.

`compilation-start' spawns through `start-file-process-shell-command'
(GNU lisp/progmodes/compile.el:2190), so with the default
`process-connection-type' of t (GNU src/process.c:8923-8929, consulted
by `is_pty_from_symbol' at src/process.c:1345-1354) the search runs on a
PTY.  ripgrep line-buffers to a terminal: the fixture's 205 bytes leave
the child as ELEVEN writes, so where the editor's reads fall between
them is decided by kernel scheduling.  Measured under load, GNU Emacs
31.0.90 splits before the first complete match line as readily as
Neomacs does -- both editors then print the extra blank line -- so a
pinned buffer string is not a parity signal while the search is
PTY-connected.

A pipe removes the choice instead of hiding it: ripgrep block-buffers to
a non-terminal, emitting the fixture's output in a single 205-byte
write, which is below PIPE_BUF and therefore reaches the reader whole.
One write, one chunk, one filter call, in either editor.

The `error' below is the point of the indirection: a search that is
PTY-connected again cannot reach a pin, so the racy shape fails loudly
here rather than intermittently in a snapshot."
  (let ((process-connection-type nil))
    (rg-run pattern "everything" root nil nil (list "--sort" "path" ".")))
  (let* ((buffer (get-buffer (rg-buffer-name)))
         (process (and buffer (get-buffer-process buffer)))
         (tty (and process (process-tty-name process))))
    (when tty
      (error "rg-test-run: search is PTY-connected (%s); its output would \
arrive in scheduling-dependent chunks" tty))
    buffer))

(defun rg-test-wait (buffer)
  "Wait until BUFFER's compilation process has exited and run its sentinel."
  (let ((deadline (+ (float-time) 30)))
    (while (and (< (float-time) deadline)
                (get-buffer-process buffer))
      (accept-process-output nil 0.05))
    (when (get-buffer-process buffer)
      (error "rg-test-wait: search process never exited"))))

(defun rg-test-mask (text)
  "Mask the wall-clock stamps the results header pins (both editors
agree on them modulo the second the search ran)."
  (let ((masked (copy-sequence (or text ""))))
    (setq masked
          (replace-regexp-in-string
           "rg started at .*$" "rg started at [TIME]" masked))
    (setq masked
          (replace-regexp-in-string
           "duration [0-9.]+ s" "duration [N] s" masked))
    (setq masked
          (replace-regexp-in-string
           " at [A-Z][a-z][a-z] [A-Z][a-z][a-z] [0-9]+ [0-9:]+" " at [TIME]" masked))
    masked))

(defun rg-test-offset (position)
  "POSITION as an offset from the end of the results header line.

`compilation-start' opens the results buffer with a mode-setter line
that spells out the absolute `default-directory' (GNU
lisp/progmodes/compile.el:2115-2121), and the sandbox that directory
lives in is created below the workspace root, so its length depends on
where this checkout sits on disk.  A raw buffer position from an
`rg-mode' buffer therefore carries the length of the checkout path and
cannot be pinned: the same editor answers a different number from a
worktree than from the main checkout.  Offsets from the end of that one
line are free of it, so every position this suite reports goes through
here."
  (- position (save-excursion
                (goto-char (point-min))
                (line-end-position))))

(defun rg-test-reset ()
  "Kill result buffers and remove fixture roots."
  (dolist (buffer (buffer-list))
    (when (eq (buffer-local-value 'major-mode buffer) 'rg-mode)
      (with-current-buffer buffer
        (set-buffer-modified-p nil))
      (ignore-errors (kill-buffer buffer))))
  (dolist (name '("widgets" "wgrep" "menu"))
    (ignore-errors
      (delete-directory
       (file-name-as-directory
        (expand-file-name
         (concat "rg-fixture-" name)
         (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
       t))))
"####;

fn rg_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(RG_MELPA_PIN, "rg.el")
        .expect("prepare pinned rg source below ./tmp")
        .with_melpa_dependency(TRANSIENT_MELPA_PIN)
        .expect("prepare pinned transient dependency")
        .with_melpa_dependency(WGREP_MELPA_PIN)
        .expect("prepare pinned wgrep dependency")
        .with_prelude(RG_TEST_PRELUDE)
        .with_timeout(RG_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed rg parity test").into()
}

/// Multi-probe batch for `assert_rg_parity` cases (2a).
pub(crate) fn assert_rg_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(rg_oracle(), &name, "rg_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn rg_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_rg_batch(&cases);
}

// END generated package batch tests
