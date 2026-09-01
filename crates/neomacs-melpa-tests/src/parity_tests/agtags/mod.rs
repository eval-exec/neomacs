use std::time::Duration;

use crate::{AGTAGS_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod completion;
mod database;
mod editing;
mod search;
mod xref;

const AGTAGS_TEST_TIMEOUT: Duration = Duration::from_secs(180);

// agtags drives the `gtags' and `global' binaries of GNU GLOBAL.  The suite
// replays a recording of GNU GLOBAL 6.6.14 (nixpkgs `global-6.6.14', "Powered
// by Berkeley DB 1.85 and SQLite3 3.8.7.1") taken against byte-identical copies
// of the fixture files written by `neomacs-agtags-test-make-project': the real
// `gtags -i' was run on that tree and the real `global' was invoked with the
// exact argument vectors agtags builds.  Every OUT line in the replay table
// below is that recording, so the stand-in's answer depends on its arguments
// exactly as the real tool's does; an argument vector with no recording writes
// UNRECORDED into the trace and exits 99 rather than silently answering
// nothing.  Real GNU GLOBAL 6.6.14 exits 0 with empty output when nothing
// matches, which is why a fruitless search reports "no matches found" rather
// than "exited abnormally".
const AGTAGS_TEST_PRELUDE: &str = r####"
(defconst neomacs-agtags-test-global-script
  (mapconcat
   #'identity
   '("#!/bin/sh"
     "# Replay stand-in for GNU GLOBAL 6.6.14 `global'."
     "printf 'global cwd=%s' \"$PWD\" >> \"$AGTAGS_TEST_TRACE\""
     "for argument do printf ' <%s>' \"$argument\" >> \"$AGTAGS_TEST_TRACE\"; done"
     "printf '\\n' >> \"$AGTAGS_TEST_TRACE\""
     "# `-u --single-update=FILE' re-indexes FILE as it stands on disk at that"
     "# moment, so which generation of the recording applies depends on the"
     "# file's current contents -- exactly as it does for the real tool."
     "case \"$1 $2\" in"
     "  '-u --single-update=')"
     "    # Recorded: with an empty path real global 6.6.14 refuses outright."
     "    echo \"gtags: path '$PWD' is out of the project.\" >&2"
     "    exit 1"
     "    ;;"
     "  '-u --single-update='*)"
     "    if grep -q parser_flush \"${2#--single-update=}\" 2>/dev/null; then"
     "      printf 'parser_flush\\n' > \"$AGTAGS_TEST_STATE\""
     "    else"
     "      rm -f \"$AGTAGS_TEST_STATE\""
     "    fi"
     "    exit 0"
     "    ;;"
     "esac"
     "if [ -f \"$AGTAGS_TEST_STATE\" ]; then"
     "  key=\"updated|$*\""
     "else"
     "  key=\"initial|$*\""
     "fi"
     "# The key reaches awk through the environment: `awk -v' expands escape"
     "# sequences, so a pattern such as `\\.c$' would arrive as `.c$' and match"
     "# nothing -- which reads as \"no results\" rather than as a lookup failure."
     "if grep -Fxq \"KEY $key\" \"$AGTAGS_TEST_TABLE\"; then"
     "  AGTAGS_REPLAY_KEY=\"KEY $key\" awk 'BEGIN { key = ENVIRON[\"AGTAGS_REPLAY_KEY\"] } $0 == key { inside = 1; next } /^KEY /{ inside = 0 } inside && /^OUT /{ print substr($0, 5) }' \"$AGTAGS_TEST_TABLE\" | sed \"s|@ROOT@|$PWD|g\""
     "  exit 0"
     "fi"
     "printf 'UNRECORDED %s\\n' \"$key\" >> \"$AGTAGS_TEST_TRACE\""
     "echo \"global: no recording for: $key\" >&2"
     "exit 99")
   "\n"))

(defconst neomacs-agtags-test-gtags-script
  (mapconcat
   #'identity
   '("#!/bin/sh"
     "# Replay stand-in for GNU GLOBAL 6.6.14 `gtags'.  The real `gtags -i'"
     "# writes GPATH, GTAGS and GRTAGS (16384 bytes each) into the working"
     "# directory, prints nothing on stdout or stderr and exits 0.  Their bytes"
     "# are GLOBAL's private database format, so the stand-in creates the same"
     "# three files and the suite asserts their existence, never their content."
     "printf 'gtags cwd=%s' \"$PWD\" >> \"$AGTAGS_TEST_TRACE\""
     "for argument do printf ' <%s>' \"$argument\" >> \"$AGTAGS_TEST_TRACE\"; done"
     "printf '\\n' >> \"$AGTAGS_TEST_TRACE\""
     "case \"$*\" in"
     "  -i)"
     "    for name in GPATH GTAGS GRTAGS; do"
     "      printf 'GNU GLOBAL tag database\\n' > \"$name\""
     "    done"
     "    exit 0"
     "    ;;"
     "esac"
     "printf 'UNRECORDED gtags %s\\n' \"$*\" >> \"$AGTAGS_TEST_TRACE\""
     "echo \"gtags: no recording for: $*\" >&2"
     "exit 99")
   "\n"))

(defconst neomacs-agtags-test-replay-table
  (mapconcat
   #'identity
   '("KEY initial|--result=grep parser_reset"
     "OUT src/parser.c:11:int parser_reset(int state) {"
     "KEY initial|--result=grep -i parser_reset"
     "OUT src/parser.c:11:int parser_reset(int state) {"
     "KEY initial|--result=grep -r parser_reset"
     "OUT include/parser.h:4:int parser_reset(int state);"
     "OUT src/main.c:11:  return parser_reset(input);"
     "OUT src/parser.c:18:  return parser_reset(state - 1);"
     "KEY initial|--result=grep -g parser_reset"
     "OUT include/parser.h:4:int parser_reset(int state);"
     "OUT src/main.c:11:  return parser_reset(input);"
     "OUT src/parser.c:11:int parser_reset(int state) {"
     "OUT src/parser.c:18:  return parser_reset(state - 1);"
     "KEY initial|--result=grep -o -g parser_reset"
     "OUT docs/notes.txt:4:parser_reset returns the next state."
     "OUT include/parser.h:4:int parser_reset(int state);"
     "OUT src/main.c:11:  return parser_reset(input);"
     "OUT src/parser.c:11:int parser_reset(int state) {"
     "OUT src/parser.c:18:  return parser_reset(state - 1);"
     "KEY initial|--result=grep -g 状態"
     "OUT src/parser.c:12:  /* 状態をリセットする */"
     "KEY initial|--result=grep zzz_absent"
     "KEY initial|--result=grep parser_flush"
     "KEY initial|--result=path -P \\.c$"
     "OUT src/main.c"
     "OUT src/parser.c"
     "KEY initial|--result=path -o -P notes"
     "OUT docs/notes.txt"
     "KEY initial|-c parse"
     "OUT parse_request"
     "OUT parser_init"
     "OUT parser_reset"
     "KEY initial|-c parser_"
     "OUT parser_init"
     "OUT parser_reset"
     "KEY initial|-c parser_f"
     "KEY initial|-c -P inc"
     "OUT include/parser.h"
     "KEY initial|-c -P main"
     "OUT main.c"
     "KEY initial|-c -P -o notes"
     "OUT notes.txt"
     "KEY initial|-c -r parse"
     "OUT parse_request"
     "OUT parser_init"
     "OUT parser_reset"
     "KEY initial|-c -r -i PARSE"
     "OUT parse_request"
     "OUT parser_init"
     "OUT parser_reset"
     "KEY initial|-d -x -a parser_reset"
     "OUT parser_reset       11 @ROOT@/src/parser.c int parser_reset(int state) {"
     "KEY initial|-r -x -a parser_reset"
     "OUT parser_reset        4 @ROOT@/include/parser.h int parser_reset(int state);"
     "OUT parser_reset       11 @ROOT@/src/main.c   return parser_reset(input);"
     "OUT parser_reset       18 @ROOT@/src/parser.c   return parser_reset(state - 1);"
     "KEY initial|-d -x -a log_line"
     "OUT log_line            3 @ROOT@/src/main.c static int log_line(int value) {"
     "OUT log_line            3 @ROOT@/src/parser.c static int log_line(int value) {"
     "KEY initial|-r -x -a log_line"
     "OUT log_line            8 @ROOT@/src/parser.c   return log_line(seed);"
     "KEY initial|-d -x -a zzz_absent"
     "KEY updated|-c parse"
     "OUT parse_request"
     "OUT parser_flush"
     "OUT parser_init"
     "OUT parser_reset"
     "KEY updated|-c parser_"
     "OUT parser_flush"
     "OUT parser_init"
     "OUT parser_reset"
     "KEY updated|-c parser_f"
     "OUT parser_flush"
     "KEY updated|--result=grep parser_flush"
     "OUT src/parser.c:21:int parser_flush(int state) {"
     "KEY updated|-d -x -a parser_flush"
     "OUT parser_flush       21 @ROOT@/src/parser.c int parser_flush(int state) {"
     "KEY updated|-r -x -a parser_reset"
     "OUT parser_reset        4 @ROOT@/include/parser.h int parser_reset(int state);"
     "OUT parser_reset       11 @ROOT@/src/main.c   return parser_reset(input);"
     "OUT parser_reset       18 @ROOT@/src/parser.c   return parser_reset(state - 1);"
     "OUT parser_reset       22 @ROOT@/src/parser.c   return parser_reset(state);"
     "KEY updated|--result=grep parser_reset"
     "OUT src/parser.c:11:int parser_reset(int state) {")
   "\n"))

;; The fixture the recording was taken against.  Line numbers in the replay
;; table are line numbers in these files, so they must not drift.
(defconst neomacs-agtags-test-header-text
  (mapconcat
   #'identity
   '("#pragma once"
     ""
     "int parser_init(int seed);"
     "int parser_reset(int state);"
     "int parse_request(int state);"
     "")
   "\n"))

(defconst neomacs-agtags-test-parser-text
  (mapconcat
   #'identity
   '("#include \"parser.h\""
     ""
     "static int log_line(int value) {"
     "  return value;"
     "}"
     ""
     "int parser_init(int seed) {"
     "  return log_line(seed);"
     "}"
     ""
     "int parser_reset(int state) {"
     "  /* 状態をリセットする */"
     "  if (state < 0) return 0;"
     "  return state + 1;"
     "}"
     ""
     "int parse_request(int state) {"
     "  return parser_reset(state - 1);"
     "}"
     "")
   "\n"))

(defconst neomacs-agtags-test-main-text
  (mapconcat
   #'identity
   '("#include \"parser.h\""
     ""
     "static int log_line(int value) {"
     "  return value;"
     "}"
     ""
     "int main(void) {"
     "  int input = 41;"
     "  /* Recover before dispatch. */"
     "  input = parser_init(input);"
     "  return parser_reset(input);"
     "}"
     "")
   "\n"))

(defconst neomacs-agtags-test-notes-text
  (mapconcat
   #'identity
   '("Design notes"
     "============"
     ""
     "parser_reset returns the next state."
     "Call PARSER_INIT once before any request."
     "")
   "\n"))

;; The two edits a user makes in the editing workflow.  With the first
;; appended, the real `global -u --single-update=' recording puts
;; `parser_flush' on line 21 and its `parser_reset' call on line 22; the
;; second adds no symbol and moves nothing, and was recorded to confirm that
;; every "updated" answer above is unchanged by it.
(defconst neomacs-agtags-test-flush-text
  (mapconcat
   #'identity
   '(""
     "int parser_flush(int state) {"
     "  return parser_reset(state);"
     "}"
     "")
   "\n"))

(defconst neomacs-agtags-test-audit-text
  (mapconcat
   #'identity
   '(""
     "/* TODO: audit the flush path. */"
     "")
   "\n"))

(defun neomacs-agtags-test-write-file (file content)
  (make-directory (file-name-directory file) t)
  (let ((coding-system-for-write 'utf-8-unix))
    (with-temp-file file
      (insert content))))

(defun neomacs-agtags-test-write-executable (file content)
  (neomacs-agtags-test-write-file file content)
  (set-file-modes file #o755))

(defun neomacs-agtags-test-make-project (root)
  "Create the recorded fixture tree below ROOT and make it a project.
The sandbox lives inside the neomacs worktree, so without a marker of its
own `project-current' would walk up and return the whole checkout."
  (neomacs-agtags-test-write-file
   (expand-file-name "include/parser.h" root)
   neomacs-agtags-test-header-text)
  (neomacs-agtags-test-write-file
   (expand-file-name "src/parser.c" root)
   neomacs-agtags-test-parser-text)
  (neomacs-agtags-test-write-file
   (expand-file-name "src/main.c" root)
   neomacs-agtags-test-main-text)
  (neomacs-agtags-test-write-file
   (expand-file-name "docs/notes.txt" root)
   neomacs-agtags-test-notes-text)
  (process-file "git" nil nil nil "init" "-q" (expand-file-name root))
  root)

(defun neomacs-agtags-test-install-tools (root)
  "Install the replay stand-ins below ROOT and return their plist."
  (let* ((bin (expand-file-name "tools/bin" root))
         (trace (expand-file-name "tools/invocations.log" root))
         (state (expand-file-name "tools/database-generation" root))
         (table (expand-file-name "tools/replay-table" root)))
    (neomacs-agtags-test-write-executable
     (expand-file-name "global" bin)
     neomacs-agtags-test-global-script)
    (neomacs-agtags-test-write-executable
     (expand-file-name "gtags" bin)
     neomacs-agtags-test-gtags-script)
    (neomacs-agtags-test-write-file
     table
     (concat neomacs-agtags-test-replay-table "\n"))
    (list :bin bin :trace trace :state state :table table)))

(defun neomacs-agtags-test-use-tools (tools)
  (setq exec-path (cons (plist-get tools :bin) exec-path)
        process-environment (copy-sequence process-environment))
  (setenv "PATH"
          (concat (plist-get tools :bin) path-separator (or (getenv "PATH") "")))
  (setenv "AGTAGS_TEST_TRACE" (plist-get tools :trace))
  (setenv "AGTAGS_TEST_STATE" (plist-get tools :state))
  (setenv "AGTAGS_TEST_TABLE" (plist-get tools :table))
  tools)

(defun neomacs-agtags-test-file-string (file)
  (if (file-exists-p file)
      (let ((coding-system-for-read 'utf-8-unix))
        (with-temp-buffer
          (insert-file-contents file)
          (buffer-string)))
    ""))

(defun neomacs-agtags-test-trace (tools)
  (copy-sequence (neomacs-agtags-test-file-string (plist-get tools :trace))))

(defun neomacs-agtags-test-complete-p (buffer)
  "Non-nil once `compilation-handle-exit' has written BUFFER's last line.
`compilation-start' writes that line from the sentinel, so the process
going away is not the end of the output -- and the text going quiet for a
few rounds is only evidence that it has been quiet for a few rounds.  The
line is the causal end instead: Emacs drains a dying process's remaining
reads before running the sentinel, the sentinel calls
`compilation-handle-exit', and that function marks what it writes with a
`compilation-handle-exit' text property (lisp/progmodes/compile.el:2630),
which therefore cannot appear until every byte GLOBAL wrote has been
through `compilation-filter'."
  (and (buffer-live-p buffer)
       (with-current-buffer buffer
         (and (text-property-not-all (point-min) (point-max)
                                     'compilation-handle-exit nil)
              t))))

(defun neomacs-agtags-test-wait-for-buffer (buffer)
  "Wait until BUFFER holds all of its compilation's output, or signal.
Pinning a results buffer that is still being filled records however much
of GLOBAL's output the kernel happened to have delivered, which is a fact
about scheduling rather than about either editor -- the defect
DIVERGENCES.md 133 removed from the `rg' suite."
  (let ((rounds 0))
    (while (and (< rounds 1800)
                (buffer-live-p buffer)
                (not (neomacs-agtags-test-complete-p buffer)))
      (accept-process-output nil 0.02)
      (setq rounds (1+ rounds)))
    (unless (neomacs-agtags-test-complete-p buffer)
      (error "neomacs-agtags-test-wait-for-buffer: %s never reached \
`compilation-handle-exit'; its text records only as much of GLOBAL's \
output as had been read"
             (if (buffer-live-p buffer) (buffer-name buffer) buffer))))
  buffer)

(defun neomacs-agtags-test-result-text (buffer)
  "Return BUFFER's text with the compilation timestamps folded away."
  (copy-sequence
   (replace-regexp-in-string
    "\\(Global [^\n]+\\) at [^\n]+"
    "\\1 at TIME"
    (with-current-buffer buffer
      (substring-no-properties (buffer-string)))
    t)))

(defun neomacs-agtags-test-messages-since (start)
  (copy-sequence
   (with-current-buffer "*Messages*"
     (buffer-substring-no-properties (min start (point-max)) (point-max)))))

(defun neomacs-agtags-test-messages-point ()
  (with-current-buffer (get-buffer-create "*Messages*") (point-max)))

(defun neomacs-agtags-test-visit (file)
  "Visit FILE the way a user would, without this repository's directory locals.
The sandbox sits inside the neomacs worktree, whose `.dir-locals.el' would
otherwise put every visited C file into `bug-reference-prog' mode with the
GNU C style."
  (let ((enable-dir-local-variables nil))
    (find-file-noselect file)))

(defun neomacs-agtags-test-here (root)
  "Describe point in the current buffer, naming the line's text as well as its
number, so a miscounted line shows up as different text."
  (list (file-relative-name (buffer-file-name) root)
        (line-number-at-pos)
        (current-column)
        (copy-sequence
         (buffer-substring-no-properties
          (line-beginning-position)
          (line-end-position)))))

(defun neomacs-agtags-test-where (marker root)
  (with-current-buffer (marker-buffer marker)
    (save-excursion
      (goto-char marker)
      (neomacs-agtags-test-here root))))

(defun neomacs-agtags-test-build-database (root)
  "Run the recorded `gtags -i' in ROOT, the way `agtags-update-tags' does."
  (let ((default-directory root))
    (with-temp-buffer
      (cd root)
      (call-process (executable-find "gtags") nil t nil "-i"))))

(defun neomacs-agtags-test-cleanup (root)
  (dolist (buffer (buffer-list))
    (when-let ((file (buffer-file-name buffer)))
      (when (string-prefix-p root file)
        (with-current-buffer buffer
          (set-buffer-modified-p nil))
        (kill-buffer buffer))))
  (dolist (name '("*agtags-grep*" "*agtags-path*"))
    (when-let ((buffer (get-buffer name)))
      (when-let ((process (get-buffer-process buffer)))
        (when (process-live-p process)
          (delete-process process)))
      (kill-buffer buffer)))
  (setq agtags--history-list nil
        agtags--global-to-list-cache nil)
  (when (file-exists-p root)
    (delete-directory root t)))

(defun neomacs-agtags-test-start (label)
  "Prepare a fixture project named LABEL and return (ROOT . TOOLS)."
  (let ((root (file-name-as-directory
               (expand-file-name label (getenv "NEOMACS_TEST_SANDBOX_ROOT")))))
    (neomacs-agtags-test-cleanup root)
    (neomacs-agtags-test-make-project root)
    (cons root (neomacs-agtags-test-use-tools
                (neomacs-agtags-test-install-tools root)))))
"####;

fn agtags_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AGTAGS_MELPA_PIN, "agtags.el")
        .expect("prepare pinned agtags source below ./tmp")
        .with_prelude(AGTAGS_TEST_PRELUDE)
        .with_timeout(AGTAGS_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed agtags parity test").into()
}

/// Multi-probe batch for `assert_agtags_parity` cases (2a).
pub(crate) fn assert_agtags_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(agtags_oracle(), &name, "agtags_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn agtags_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        completion::completion_public_surface_batch_cases(),
        database::database_public_surface_batch_cases(),
        editing::editing_public_surface_batch_cases(),
        search::search_public_surface_batch_cases(),
        xref::xref_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_agtags_batch(&cases);
}

// END generated package batch tests
