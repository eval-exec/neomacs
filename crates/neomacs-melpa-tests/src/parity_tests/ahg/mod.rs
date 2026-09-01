use std::time::Duration;

use crate::{AHG_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AHG_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// Sandbox helpers shared by the workflows.
///
/// aHg is a Mercurial front end: it runs `hg' asynchronously and renders what
/// comes back into status, log, diff, annotate and patch-queue buffers.  Almost
/// everything it does is therefore *parsing Mercurial's output*, which makes an
/// invented `hg' the one fixture that cannot be allowed here -- writing the
/// input a parser expects biases the suite into confirming that the parser
/// works.
///
/// So every answer in `ahg-test-recordings' was recorded from real Mercurial
/// **7.1** running against a real repository, driven by real aHg through a
/// pass-through wrapper that logged the exact argument vector alongside the
/// exact bytes that came back.  Neither the command lines nor the output were
/// written by hand.  Two repositories were recorded, because the same argument
/// vector legitimately answers differently in each:
///
/// * `repoA' -- three commits with fixed dates and authors, plus an edited
///   `docs/guide.md' and an untracked `notes.todo' in the working tree;
/// * `repoB' -- the same history with an MQ patch queue on top, carrying a real
///   `+linux -windows' guard set with `hg qguard'.
///
/// Both repositories are byte-for-byte reproducible: built twice from scratch
/// they produce identical changeset hashes, which is what makes the recorded
/// `60eb783c89a0' safe to pin.
///
/// The stand-in only ever looks an answer up, keyed on the repository *and* the
/// argument vector, and it **fails loudly** -- exit 99, `UNRECORDED' on stderr,
/// and a line in a miss log -- rather than returning nothing for a request it
/// has no recording for.  Every workflow asserts that miss log is empty, so a
/// rendered buffer is known to have come from Mercurial's own bytes rather than
/// from the stand-in quietly answering "no output", which for a VCS is a
/// legitimate-looking result and therefore reads as data.
const AHG_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'seq)
(require 'subr-x)

(defvar ahg-test-root
  (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defvar ahg-test-records
  (file-name-as-directory (expand-file-name "hg-records" ahg-test-root)))

(defvar ahg-test-calls (expand-file-name "hg-calls.log" ahg-test-root))
(defvar ahg-test-misses (expand-file-name "hg-misses.log" ahg-test-root))

(defconst ahg-test-recordings
  '(
    ("repoA" ("--config" "ui.report_untrusted=0" "annotate" "-undql" "src/main.el") 0 "  ada 2 2023-11-16:1: (defun deploy-release ()\n  ada 2 2023-11-16:2:   (message \"release ready\"))\ngrace 1 2023-11-15:3: \ngrace 1 2023-11-15:4: (defun rollback-release ()\ngrace 1 2023-11-15:5:   (message \"rollback ready\"))\n")
    ("repoA" ("--config" "ui.report_untrusted=0" "diff" "--git") 0 "diff --git a/docs/guide.md b/docs/guide.md\n--- a/docs/guide.md\n+++ b/docs/guide.md\n@@ -1,3 +1,4 @@\n # Release guide\n \n Deploy after review.\n+Rollback if monitoring fails.\n")
    ("repoA" ("--config" "ui.report_untrusted=0" "help" "status") 0 "hg status [OPTION]... [FILE]...\n\naliases: st\n\nshow changed files in the working directory\n\nShow status of files in the repository. If names are given, only files that\nmatch are shown. Files that are clean or ignored or the source of a copy/move\noperation, are not listed unless -c/--clean, -i/--ignored, -C/--copies or\n-A/--all are given. Unless options described with \"show only ...\" are given,\nthe options -mardu are used.\n\nOption -q/--quiet hides untracked (unknown and ignored) files unless\nexplicitly requested with -u/--unknown or -i/--ignored.\n\nNote:\n   'hg status' may appear to disagree with diff if permissions have changed or\n   a merge has occurred. The standard diff format does not report permission\n   changes and diff only reports changes relative to one merge parent.\n\nIf one revision is given, it is used as the base revision. If two revisions\nare given, the differences between them are shown. The --change option can\nalso be used as a shortcut to list the changed files of a revision from its\nfirst parent.\n\nThe codes used to show the status of files are:\n\n  M = modified\n  A = added\n  R = removed\n  C = clean\n  ! = missing (deleted by non-hg command, but still tracked)\n  ? = not tracked\n  I = ignored\n    = origin of the previous file (with --copies)\n\nReturns 0 on success.\n\noptions ([+] can be repeated):\n\n -A --all                 show status of all files\n -m --modified            show only modified files\n -a --added               show only added files\n -r --removed             show only removed files\n -d --deleted             show only missing files\n -c --clean               show only files without changes\n -u --unknown             show only unknown (not tracked) files\n -i --ignored             show only ignored files\n -n --no-status           hide status prefix\n -C --copies              show source of copied files\n -0 --print0              end filenames with NUL, for use with xargs\n    --rev REV [+]         show difference from revision\n    --change REV          list the changed files of a revision\n -I --include PATTERN [+] include names matching the given patterns\n -X --exclude PATTERN [+] exclude names matching the given patterns\n -S --subrepos            recurse into subrepositories\n -T --template TEMPLATE   display with template\n    --mq                  operate on patch repository\n\n(some details hidden, use --verbose to show complete help)\n")
    ("repoA" ("--config" "ui.report_untrusted=0" "log" "--template" "{rev} {desc|firstline}\\n" "src/main.el") 0 "")
    ("repoA" ("--config" "ui.report_untrusted=0" "log" "-r" "." "--template" "{node|short} ") 0 "60eb783c89a0 ")
    ("repoA" ("--config" "ui.report_untrusted=0" "log" "-r" "." "--template" "{rev} ") 0 "2 ")
    ("repoA" ("--config" "ui.report_untrusted=0" "log" "-r" "0:2" "--style" ".hg/ahg-log-style-map") 0 "0:84d4a1540886\n\n\ndraft\n\n\n\n\nGrace Hopper <grace@example.test>\nTue Nov 14 22:13:20 2023 +0000\ndocs/guide.md\nsrc/main.el\n\n\tBootstrap repository\n1:9eb7836204d1\n\n\ndraft\n\n\n\n\nGrace Hopper <grace@example.test>\nWed Nov 15 22:13:20 2023 +0000\nsrc/main.el\n\n\tAdd rollback procedure\n2:60eb783c89a0\n\n\ndraft\n\ntip\n\n\nAda Lovelace <ada@example.test>\nThu Nov 16 22:13:20 2023 +0000\nsrc/main.el\n\n\tShip release safely\n")
    ("repoA" ("--config" "ui.report_untrusted=0" "log" "-r" "0:2" "--template" "{rev} {date|shortdate} {author|user} {desc|firstline}\\n") 0 "0 2023-11-14 grace Bootstrap repository\n1 2023-11-15 grace Add rollback procedure\n2 2023-11-16 ada Ship release safely\n")
    ("repoA" ("--config" "ui.report_untrusted=0" "status") 0 "M docs/guide.md\n? notes.todo\n")
    ("repoA" ("--config" "ui.report_untrusted=0" "summary") 0 "parent: 2:60eb783c89a0 tip\n Ship release safely\nbranch: default\ncommit: 1 modified, 1 unknown\nupdate: (current)\nphases: 3 draft\n")
    ("repoB" ("--config" "ui.report_untrusted=0" "qapplied") 0 "")
    ("repoB" ("--config" "ui.report_untrusted=0" "qguard" "-l") 0 "release-candidate: +linux -windows\ncleanup: unguarded\n")
    ("repoB" ("--config" "ui.report_untrusted=0" "qseries") 0 "release-candidate\ncleanup\n")
    ("repoB" ("--config" "ui.report_untrusted=0" "status") 0 "? notes.todo\n")
    ("repoB" ("--config" "ui.report_untrusted=0" "summary") 0 "parent: 2:60eb783c89a0 tip\n Ship release safely\nbranch: default\ncommit: 1 unknown (clean)\nupdate: (current)\nphases: 3 draft\nmq:     1 unapplied\n")))

(defun ahg-test-key (arguments)
  "Return the record key for ARGUMENTS.

Must agree exactly with the shell stand-in's key function; every character
outside [A-Za-z0-9._-] becomes an underscore and the arguments are joined with
`~', so the key is a filename and no argument can be confused with another."
  (mapconcat (lambda (argument)
               (concat "~" (replace-regexp-in-string
                            "[^A-Za-z0-9._-]" "_" argument)))
             arguments ""))

(defun ahg-test-write (path content &optional executable)
  (make-directory (file-name-directory path) t)
  (let ((coding-system-for-write 'utf-8-unix))
    (with-temp-buffer
      (insert content)
      (write-region (point-min) (point-max) path nil 'silent)))
  (when executable (set-file-modes path #o755))
  path)

(defconst ahg-test-stand-in
  (string-join
   (list
    "#!/bin/sh"
    "# Replay stand-in for Mercurial 7.1.  Every answer below was recorded from"
    "# the real hg against a real repository; this script only looks one up."
    "repo=$PWD"
    "while [ ! -d \"$repo/.hg\" ] && [ \"$repo\" != / ]; do"
    "  repo=$(dirname \"$repo\")"
    "done"
    "repo=$(basename \"$repo\")"
    "key=\"\""
    "for a in \"$@\"; do"
    "  key=\"$key~$(printf '%s' \"$a\" | tr -c 'A-Za-z0-9._-' '_')\""
    "done"
    "printf '%s: %s\\n' \"$repo\" \"$*\" >> \"$AHG_TEST_CALLS\""
    "d=\"$AHG_TEST_RECORDS/$repo/$key\""
    "if [ ! -f \"$d/out\" ]; then"
    "  printf '%s: %s\\n' \"$repo\" \"$*\" >> \"$AHG_TEST_MISSES\""
    "  printf 'UNRECORDED hg invocation: %s\\n' \"$*\" >&2"
    "  exit 99"
    "fi"
    "cat \"$d/out\""
    "exit \"$(cat \"$d/rc\")\""
    "")
   "\n"))

(defun ahg-test-install-hg ()
  "Write every recording to disk and point `ahg-hg-command' at the stand-in.

Returns the number of distinct records installed.  Keys are checked for
collisions, because two different argument vectors mapping to one filename
would silently make one command answer for the other."
  (let ((installed nil))
    (dolist (recording ahg-test-recordings)
      (let* ((repo (nth 0 recording))
             (key (ahg-test-key (nth 1 recording)))
             (path (expand-file-name (concat repo "/" key) ahg-test-records)))
        (when (member path installed)
          (error "Record key collision for %S" (nth 1 recording)))
        (push path installed)
        (ahg-test-write (expand-file-name "out" path) (nth 3 recording))
        (ahg-test-write (expand-file-name "rc" path)
                        (format "%d\n" (nth 2 recording)))))
    (setenv "AHG_TEST_RECORDS" (directory-file-name ahg-test-records))
    (setenv "AHG_TEST_CALLS" ahg-test-calls)
    (setenv "AHG_TEST_MISSES" ahg-test-misses)
    (setq ahg-hg-command
          (ahg-test-write (expand-file-name "bin/hg" ahg-test-root)
                          ahg-test-stand-in t))
    (length installed)))

(defconst ahg-test-main-el
  "(defun deploy-release ()\n  (message \"release ready\"))\n\n(defun rollback-release ()\n  (message \"rollback ready\"))\n")

(defconst ahg-test-guide-md
  "# Release guide\n\nDeploy after review.\nRollback if monitoring fails.\n")

(defun ahg-test-repo (name &optional patches)
  "Create the working tree of recorded repository NAME and return its root.

Only the files matter here: every `hg' answer is replayed from the recording,
so the `.hg' directory needs to exist for aHg's root detection and nothing
else.  PATCHES, when non-nil, creates the queue directory the mq workflow
visits."
  (let ((root (file-name-as-directory (expand-file-name name ahg-test-root))))
    (make-directory (expand-file-name ".hg" root) t)
    (ahg-test-write (expand-file-name "src/main.el" root) ahg-test-main-el)
    (ahg-test-write (expand-file-name "docs/guide.md" root) ahg-test-guide-md)
    (ahg-test-write (expand-file-name "notes.todo" root)
                    "verify release checks\n")
    (when patches
      (make-directory (expand-file-name ".hg/patches" root) t)
      (ahg-test-write (expand-file-name ".hg/patches/release-candidate" root)
                      (concat "# HG changeset patch\n"
                              "# User Ada Lovelace <ada@example.test>\n"
                              "# Date 1700259200 0\n"
                              "Prepare the release candidate\n\n"
                              "diff --git a/docs/guide.md b/docs/guide.md\n"
                              "--- a/docs/guide.md\n"
                              "+++ b/docs/guide.md\n"
                              "@@ -1,3 +1,4 @@\n"
                              " # Release guide\n \n Deploy after review.\n"
                              "+Rollback if monitoring fails.\n")))
    root))

(defun ahg-test-settle (&optional seconds)
  "Let aHg's asynchronous hg processes and their sentinels finish."
  (let ((deadline (+ (float-time) (or seconds 10.0)))
        (previous nil)
        (stable 0))
    (while (and (< (float-time) deadline) (< stable 3))
      (accept-process-output nil 0.05)
      (let ((now (mapcar (lambda (buffer)
                           (cons (buffer-name buffer) (buffer-size buffer)))
                         (buffer-list))))
        (if (equal now previous) (setq stable (1+ stable)) (setq stable 0))
        (setq previous now)))
    stable))

(defun ahg-test-calls ()
  "Every hg invocation the package made, as `REPO: ARGUMENTS'."
  (if (not (file-exists-p ahg-test-calls))
      'no-hg-was-run
    (with-temp-buffer
      (insert-file-contents ahg-test-calls)
      (split-string (buffer-string) "\n" t))))

(defun ahg-test-unrecorded ()
  "Invocations the stand-in had no recording for.

Asserted by every workflow: an empty list is what says the workflow was
answered entirely from recorded Mercurial output rather than from a stand-in
guessing, which is the whole basis for trusting the rendered result."
  (if (not (file-exists-p ahg-test-misses))
      nil
    (with-temp-buffer
      (insert-file-contents ahg-test-misses)
      (split-string (buffer-string) "\n" t))))

(defun ahg-test-buffer-text (name)
  (let ((buffer (get-buffer name)))
    (if (not buffer)
        'no-such-buffer
      (with-current-buffer buffer
        (buffer-substring-no-properties (point-min) (point-max))))))

(defun ahg-test-buffer (prefix)
  "Return the text of the one aHg buffer whose name starts with PREFIX.

aHg names its buffers after the repository's absolute path, which the oracle
normalises; reporting `several' rather than picking the first keeps a workflow
from silently reading a buffer left over from an earlier command."
  (let ((matches (seq-filter (lambda (buffer)
                               (string-prefix-p prefix (buffer-name buffer)))
                             (buffer-list))))
    (cond
     ((null matches) (list 'no-buffer-matching prefix))
     ((cdr matches) (cons 'several (mapcar #'buffer-name matches)))
     (t (with-current-buffer (car matches)
          (buffer-substring-no-properties (point-min) (point-max)))))))

(defun ahg-test-buffer-mode (prefix)
  "Return the major mode of the one aHg buffer whose name starts with PREFIX."
  (let ((matches (seq-filter (lambda (buffer)
                               (string-prefix-p prefix (buffer-name buffer)))
                             (buffer-list))))
    (if (or (null matches) (cdr matches))
        'not-exactly-one-buffer
      (with-current-buffer (car matches) major-mode))))
"##;

fn ahg_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AHG_MELPA_PIN, "ahg.el")
        .expect("prepare pinned ahg source below ./tmp")
        .with_prelude(AHG_TEST_PRELUDE)
        .with_timeout(AHG_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed ahg parity test").into()
}

/// Multi-probe batch for `assert_ahg_parity` cases (2a).
pub(crate) fn assert_ahg_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ahg_oracle(), &name, "ahg_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn ahg_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ahg_batch(&cases);
}

// END generated package batch tests
