use std::time::Duration;

use crate::{CachedMelpaOracle, GGTAGS_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const GGTAGS_TEST_TIMEOUT: Duration = Duration::from_secs(240);

// The external-boundary fixture replays byte-exact successful observations
// from official GNU Global 6.7, built from global-6.7.tar.gz with SHA-256
// fdab590c9bda2d68d55e99c51c7e60c2c8595ae4dcebab9bbbb0795f2a5c8bf7.
// This intentionally supersedes the earlier 6.6.15 draft named in one study
// brief. Deliberate nonzero-status/corruption records exercise package failure
// boundaries and are approved fixtures, not claimed as upstream output. Real
// `global' and `gtags' were recorded against the exact source tree written
// below; the stand-in owns only that true executable boundary. Emacs' process,
// compilation, Xref, timer, overlay, buffer, and filesystem machinery remains
// real.
const GGTAGS_REPLAY_SCRIPT: &str = r#"#!/usr/bin/env python3
import fcntl
import hashlib
import json
import os
import pathlib
import sys

VERSION = "GNU Global 6.7"
TARBALL_SHA256 = "fdab590c9bda2d68d55e99c51c7e60c2c8595ae4dcebab9bbbb0795f2a5c8bf7"
HELP_STDOUT_CONTRACT = (
    "status-only:process-file-destination-nil:global-6.7:"
    "8590:3f269245f1c7abedd402629112a843e238947aed79f6556b4228f369e1b7af39"
)
INITIAL_FIXTURE_SHA256 = {
    "app/main.c": "754c8e71550511b22ff0ed5ee28762fa3197cba94b8575b41a035e64d4fc8ea9",
    "src/widget.c": "b9970fa8a81682513eafead5d11a146763f41de72454a36b044f0eafdcdb827d",
    "src/widget_alt.c": "15159c4a3a13ca13f83a8f17f7a6cd0860925dc833e92134389682c80ffe6598",
    "src/widget.h": "70e7a6f09aa17f06477c0aa2cded7df4b695105ae8d360b8793b4997429b1ddf",
    "docs/design Ω notes.txt": "3338e8ca1084a6c4dc64d8670f1747844d9f733e6307ccd9ff06fb9c1645c979",
}
UPDATED_WIDGET_SHA256 = "bc2ea5b463ab073e3f24968a0a31cdcdc04f313265bcdb003273897529b47e23"
# Frozen allowlist of approved replay streams. It intentionally contains both
# normalized official 6.7 success recordings and explicitly simulated package
# failure boundaries; discarded help output has its separate narrowed contract.
# The adapter checks the record-local digest and membership in this set.
APPROVED_REPLAY_STREAM_DIGESTS = {
    "0ef548d0c6ad2408baa68a6a53b0c67fff20535841266fb48069c8014958db73",
    "2fd4fa25336c5229d0e193d9dd26b155018a42b2c9011fbf32d48825df02c910",
    "396316a0b541a487816382800512d33018e810489683027b476fbc604c3a26dd",
    "5da0104944628f0721b91bd18ecd1fa677a7c3093cfd45d55c605516f9a1d41e",
    "6880774a3c5fcf902fd3d3aa1e2e5930a065ade303758227f8597273486a0f7e",
    "6cd18a38b5f3b92cf32563b3a01f6519c64f71f0424497562ee0c04758945a30",
    "715cff98026f812f6397a6fa9ae8c368903ba8d7bf460dff19039ff6fda12cbf",
    "719759c4cda7d1d2829f6035765c47f5cffe6f4aca761a641222fb0da06213bb",
    "73d4fd3a9781d99bb96c68a1d896656e22349196bb1609ec63dbfc6e5c2cc2dc",
    "7d3e41bfff584699cfe96a1636c15c2224898a52c01542673c862fec1d7e761d",
    "8fd149ac622eda6d0dac6eca9906d7b5ed14cd375a8f5c4532fe758e2f69ac2e",
    "b0c4324e7fc48346b1b32a11a76cff33b4593176b7f6a5ac57f6a01bf6e84853",
    "b28bb37d7f39373f0df865d930effdd7540a242ca2d8ebf7cbf29c80260a9426",
    "bb8eab3a176a2494c3af6a0630d1eafae2077f55e208e939a8da194a1f4fb5b1",
    "d7e11de43cb5d5f1f59a618fce06d5f32b772b5c76a648aff3dd6904eb5da5c4",
    "dd5dabcd6f8246006cc545ad66268ca10b90699ab1a85a6dfc7e4d7e97ccd270",
    "dd7f3293b503b995b06e45f0f7ba4073ccddae69f119a99bb09331841b61d6c3",
    "e764edf4de71389be469cbd77d9d3b555990c7b298ceaf916395edd6d75c3bfd",
    "eadb6c511e574df8ca19d1b205a972ea31770dc39c5056fb00c939557bdb72e1",
    "f2e368a0537de0e2b4361e3d639fa079565869efc759190d03caf119ca250128",
    "fcd673b554223c39ab98c616ec053063740282f528b4d81b1c4ab3e34cc6e9fa",
}
ENV_KEYS = [
    "GTAGSROOT", "GTAGSDBPATH", "GTAGSLABEL", "GTAGSCONF", "GTAGSLIBPATH",
    "LC_ALL",
]


def required(name):
    value = os.environ.get(name)
    if not value:
        raise RuntimeError(f"missing required environment {name}")
    return value


def append_trace(path, fields):
    payload = b"\0".join(str(field).encode("utf-8") for field in fields) + b"\0"
    with open(path, "ab", buffering=0) as stream:
        stream.write(payload)


def fail(trace, index, fields, message):
    append_trace(trace, ["MISS", index, *fields, message, "END"])
    sys.stderr.write(f"NEOMACS_GGTAGS_REPLAY: {message}\n")
    raise SystemExit(97)


def stream_digest(stdout, stderr, status, project):
    normalized_stdout = stdout.replace(str(project), "[ROOT]")
    normalized_stderr = stderr.replace(str(project), "[ROOT]")
    payload = f"{normalized_stdout}\0{normalized_stderr}\0{status}".encode("utf-8")
    return hashlib.sha256(payload).hexdigest()


def validate_fixture(trace, index, fields, project, fixture_state):
    expected = dict(INITIAL_FIXTURE_SHA256)
    if fixture_state == "saved":
        expected["src/widget.c"] = UPDATED_WIDGET_SHA256
    for relative, wanted in expected.items():
        target = (project / relative).resolve()
        try:
            target.relative_to(project)
        except ValueError:
            fail(trace, index, fields, f"fixture path escaped root: {relative}")
        if not target.is_file():
            fail(trace, index, fields, f"canonical fixture missing: {relative}")
        actual = hashlib.sha256(target.read_bytes()).hexdigest()
        if actual != wanted:
            fail(
                trace,
                index,
                fields,
                f"canonical fixture digest mismatch for {relative}: {actual} != {wanted}",
            )


case = required("NEOMACS_GGTAGS_CASE")
root = pathlib.Path(required("NEOMACS_GGTAGS_PROJECT_ROOT")).resolve()
plan_path = pathlib.Path(required("NEOMACS_GGTAGS_PLAN"))
state_path = pathlib.Path(required("NEOMACS_GGTAGS_STATE"))
trace_path = pathlib.Path(required("NEOMACS_GGTAGS_TRACE"))
if required("NEOMACS_GGTAGS_RECORDING_VERSION") != VERSION:
    fail(trace_path, -1, ["PROVENANCE"], "wrong GNU Global recording version")
if required("NEOMACS_GGTAGS_TARBALL_SHA256") != TARBALL_SHA256:
    fail(trace_path, -1, ["PROVENANCE"], "wrong GNU Global tarball digest")

program = pathlib.Path(sys.argv[0]).name
argv = sys.argv[1:]
cwd = pathlib.Path.cwd().resolve()
try:
    relative_cwd = cwd.relative_to(root)
except ValueError:
    fail(trace_path, -1, [program, str(cwd)], "cwd escaped owned project")
cwd_key = "." if str(relative_cwd) == "." else relative_cwd.as_posix()
actual_env = {key: os.environ.get(key) for key in ENV_KEYS}

with open(state_path, "r+", encoding="utf-8") as state_stream:
    fcntl.flock(state_stream.fileno(), fcntl.LOCK_EX)
    state = json.load(state_stream)
    plan = json.loads(plan_path.read_text(encoding="utf-8"))
    index = state["index"]
    generation = state["generation"]
    actual_fields = [
        "CALL", index, program, case, generation, cwd_key, len(argv), *argv,
        *(f"{key}={actual_env[key]}" for key in ENV_KEYS), "END"
    ]
    if index >= len(plan):
        fail(trace_path, index, actual_fields, "plan exhausted")
    expected = plan[index]
    expected_shape = {
        "program": expected["program"],
        "case": expected["case"],
        "generation": expected["generation"],
        "cwd": expected["cwd"],
        "args": expected["args"],
        "env": expected["env"],
    }
    actual_shape = {
        "program": program,
        "case": case,
        "generation": generation,
        "cwd": cwd_key,
        "args": argv,
        "env": actual_env,
    }
    if actual_shape != expected_shape:
        fail(
            trace_path,
            index,
            actual_fields,
            "request mismatch expected=" + json.dumps(expected_shape, ensure_ascii=False, sort_keys=True)
            + " actual=" + json.dumps(actual_shape, ensure_ascii=False, sort_keys=True),
        )

    action = expected.get("action")
    fixture_state = expected.get("fixture_state")
    if fixture_state is None:
        fixture_state = (
            "saved"
            if generation == "saved"
            or (action and action.get("kind") == "validate-file")
            else "initial"
        )
    if fixture_state not in {"initial", "saved"}:
        fail(trace_path, index, actual_fields, f"unknown fixture state {fixture_state}")
    validate_fixture(trace_path, index, actual_fields, root, fixture_state)

    wanted_stream_digest = expected.get("recording_stream_sha256")
    actual_stream_digest = stream_digest(
        expected.get("stdout", ""),
        expected.get("stderr", ""),
        expected.get("status", 0),
        root,
    )
    if wanted_stream_digest != actual_stream_digest:
        fail(trace_path, index, actual_fields, "recorded stdout/stderr/status digest mismatch")
    if (
        actual_stream_digest not in APPROVED_REPLAY_STREAM_DIGESTS
    ):
        fail(trace_path, index, actual_fields, "stream digest absent from approved replay allowlist")

    stdout_contract = expected.get("stdout_contract")
    if argv[-1:] == ["--help"]:
        if stdout_contract != HELP_STDOUT_CONTRACT:
            fail(trace_path, index, actual_fields, "missing exact discarded-help contract")
        if expected.get("stdout", ""):
            fail(trace_path, index, actual_fields, "discarded-help route invented visible stdout")
    elif stdout_contract is not None:
        fail(trace_path, index, actual_fields, "help stdout contract attached to non-help route")

    if action:
        kind = action["kind"]
        if kind == "create-database":
            for name in ["GPATH", "GRTAGS", "GTAGS"]:
                target = root / name
                payload = (f"GNU Global 6.7 replay {name}\n".encode("ascii"))
                target.write_bytes(payload + bytes(16384 - len(payload)))
            if action.get("id"):
                target = root / "ID"
                payload = b"GNU Global 6.7 replay ID\n"
                target.write_bytes(payload + bytes(16384 - len(payload)))
        elif kind == "validate-file":
            target = (root / action["path"]).resolve()
            try:
                target.relative_to(root)
            except ValueError:
                fail(trace_path, index, actual_fields, "side-effect path escaped root")
            digest = hashlib.sha256(target.read_bytes()).hexdigest()
            if digest != action["sha256"]:
                fail(trace_path, index, actual_fields, f"source digest mismatch for {action['path']}")
        else:
            fail(trace_path, index, actual_fields, f"unknown side effect {kind}")

    state["index"] = index + 1
    if expected.get("next_generation") is not None:
        state["generation"] = expected["next_generation"]
    state_stream.seek(0)
    json.dump(state, state_stream, ensure_ascii=False, sort_keys=True)
    state_stream.truncate()
    state_stream.flush()
    os.fsync(state_stream.fileno())
    append_trace(trace_path, actual_fields)

sys.stdout.buffer.write(expected.get("stdout", "").encode("utf-8"))
sys.stderr.buffer.write(expected.get("stderr", "").encode("utf-8"))
raise SystemExit(expected.get("status", 0))
"#;

const GGTAGS_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'seq)
(require 'json)
(require 'xref)
(require 'timer)
(require 'cc-mode)

(defvar ggt-test-replay-script nil)
(defvar ggt-test-plan nil)
(defvar ggt-test-prompts nil)
(defvar ggt-test-prompt-ledger nil)
(defvar ggt-test-owned-overlays nil)
(defvar ggt-test-xref-history nil)
(defvar ggt-test-temp-buffer-text nil)
(defvar ggt-test-message-ledger nil)

(defconst ggt-test-recording-version "GNU Global 6.7")
(defconst ggt-test-tarball-sha256
  "fdab590c9bda2d68d55e99c51c7e60c2c8595ae4dcebab9bbbb0795f2a5c8bf7")
(defconst ggt-test-help-stdout-contract
  (concat "status-only:process-file-destination-nil:global-6.7:"
          "8590:3f269245f1c7abedd402629112a843e238947aed79f6556b4228f369e1b7af39"))

(defconst ggt-test-initial-fixture-manifest
  '(("app/main.c" .
     "754c8e71550511b22ff0ed5ee28762fa3197cba94b8575b41a035e64d4fc8ea9")
    ("src/widget.c" .
     "b9970fa8a81682513eafead5d11a146763f41de72454a36b044f0eafdcdb827d")
    ("src/widget_alt.c" .
     "15159c4a3a13ca13f83a8f17f7a6cd0860925dc833e92134389682c80ffe6598")
    ("src/widget.h" .
     "70e7a6f09aa17f06477c0aa2cded7df4b695105ae8d360b8793b4997429b1ddf")
    ("docs/design Ω notes.txt" .
     "3338e8ca1084a6c4dc64d8670f1747844d9f733e6307ccd9ff06fb9c1645c979")))
(defconst ggt-test-updated-widget-sha256
  "bc2ea5b463ab073e3f24968a0a31cdcdc04f313265bcdb003273897529b47e23")
(defvar ggt-test-recording-project nil)

(defconst ggt-test-main-source
  "#include \"widget.h\"\n\nint main(void) {\n      int value = widget_total(2);\n      return value;\n}\n")
(defconst ggt-test-widget-source
  "#include \"widget.h\"\n\nint widget_total(int count) {\n    return count + 1;\n}\n\nint widget_use(void) {\n    return widget_total(41);\n}\n")
(defconst ggt-test-widget-alt-source
  "#include \"widget.h\"\n\nint widget_total(int count) {\n    return count + 100;\n}\n")
(defconst ggt-test-header-source
  "#ifndef WIDGET_H\n#define WIDGET_H\n\nint widget_total(int count);\nint widget_use(void);\n\n#endif\n")
(defconst ggt-test-notes-source
  "Widget totals in a path with spaces and Unicode Ω.\n")

(defun ggt-test-after-save-sentinel () nil)
(defun ggt-test-xref-sentinel () nil)
(defun ggt-test-capf-sentinel () nil)
(defun ggt-test-eldoc-sentinel () "sentinel")

(defun ggt-test-write-file (root relative contents)
  "Write exact CONTENTS to RELATIVE below validated ROOT."
  (let ((file (expand-file-name relative root)))
    (unless (string-prefix-p root file)
      (error "GGTAGS fixture escaped root: %s" file))
    (make-directory (file-name-directory file) t)
    (with-temp-file file (insert contents))
    file))

(defun ggt-test-write-executable (file contents)
  (ggt-test-write-file (file-name-directory file)
                       (file-name-nondirectory file) contents)
  (set-file-modes file #o755)
  file)

(defun ggt-test-create-project (project)
  (ggt-test-write-file project "app/main.c" ggt-test-main-source)
  (ggt-test-write-file project "src/widget.c" ggt-test-widget-source)
  (ggt-test-write-file project "src/widget_alt.c" ggt-test-widget-alt-source)
  (ggt-test-write-file project "src/widget.h" ggt-test-header-source)
  (ggt-test-write-file project "docs/design Ω notes.txt" ggt-test-notes-source)
  project)

(defun ggt-test-file-sha256 (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(defun ggt-test-validate-project-manifest (project manifest)
  "Fail closed unless PROJECT matches canonical source MANIFEST."
  (dolist (entry manifest)
    (let ((file (expand-file-name (car entry) project)))
      (unless (and (string-prefix-p project file) (file-regular-p file))
        (error "GGTAGS canonical fixture missing: %s" file))
      (let ((actual (ggt-test-file-sha256 file)))
        (unless (equal actual (cdr entry))
          (error "GGTAGS fixture digest mismatch for %s: %s != %s"
                 (car entry) actual (cdr entry))))))
  t)

(defun ggt-test-seed-database (project)
  "Install the recorded 16KiB database layout as initial fixture state."
  (dolist (name '("GPATH" "GRTAGS" "GTAGS"))
    (let ((file (expand-file-name name project)))
      (with-temp-buffer
        (set-buffer-multibyte nil)
        (insert (format "GNU Global 6.7 replay %s\n" name))
        (insert (make-string (- 16384 (buffer-size)) 0))
        (write-region (point-min) (point-max) file nil 'silent))))
  project)

(defun ggt-test-env (project &optional gtagsroot label conf dbpath libpath)
  "Return the complete relevant executable environment expectation."
  `(("GTAGSROOT" . ,(and gtagsroot (directory-file-name project)))
    ("GTAGSDBPATH" . ,dbpath)
    ("GTAGSLABEL" . ,label)
    ("GTAGSCONF" . ,conf)
    ("GTAGSLIBPATH" . ,libpath)
    ("LC_ALL" . "C.UTF-8")))

(defun ggt-test-record
    (program case generation cwd args env
             &optional stdout stderr status action next-generation)
  "Create one exact fail-closed GNU Global replay record."
  (let* ((stdout (or stdout ""))
         (stderr (or stderr ""))
         (status (or status 0))
         (project (directory-file-name ggt-test-recording-project))
         (normalized-stdout
          (replace-regexp-in-string (regexp-quote project)
                                    "[ROOT]" stdout t t))
         (normalized-stderr
          (replace-regexp-in-string (regexp-quote project)
                                    "[ROOT]" stderr t t))
         (stream-digest
          (secure-hash
           'sha256
           (encode-coding-string
            (format "%s%c%s%c%d"
                    normalized-stdout 0 normalized-stderr 0 status)
            'utf-8))))
    `((program . ,program) (case . ,case) (generation . ,generation)
    (cwd . ,cwd) (args . ,(apply #'vector args)) (env . ,env)
    (stdout . ,stdout) (stderr . ,stderr)
    (status . ,status) (action . ,action)
    (recording_stream_sha256 . ,stream-digest)
    ;; Exact Global 6.7 help stdout is 8,590 bytes with the recorded digest.
    ;; Pinned `ggtags-process-succeed-p' invokes `process-file' with a nil
    ;; destination, so the package can observe only status.  Record that
    ;; narrowed boundary explicitly instead of inventing empty help bytes.
    (stdout_contract . ,(and (equal (car (last args)) "--help")
                             ggt-test-help-stdout-contract))
    (next_generation . ,next-generation))))

(defun ggt-test-install-plan (case-root project case plan generation)
  "Install PLAN and the executable pair for CASE."
  (let* ((bin (file-name-as-directory (expand-file-name "bin" case-root)))
         (plan-file (expand-file-name "plan.json" case-root))
         (state-file (expand-file-name "state.json" case-root))
         (trace-file (expand-file-name "trace.nul" case-root)))
    (make-directory bin)
    (ggt-test-write-executable (expand-file-name "global" bin)
                               ggt-test-replay-script)
    (ggt-test-write-executable (expand-file-name "gtags" bin)
                               ggt-test-replay-script)
    (let ((json-encoding-pretty-print nil))
      (ggt-test-write-file case-root "plan.json" (json-encode plan)))
    (ggt-test-write-file
     case-root "state.json"
     (json-encode `((index . 0) (generation . ,generation))))
    (ggt-test-write-file case-root "trace.nul" "")
    (setq ggt-test-plan plan)
    (setenv "NEOMACS_GGTAGS_CASE" case)
    (setenv "NEOMACS_GGTAGS_PROJECT_ROOT" (directory-file-name project))
    (setenv "NEOMACS_GGTAGS_PLAN" plan-file)
    (setenv "NEOMACS_GGTAGS_STATE" state-file)
    (setenv "NEOMACS_GGTAGS_TRACE" trace-file)
    (setenv "NEOMACS_GGTAGS_RECORDING_VERSION" ggt-test-recording-version)
    (setenv "NEOMACS_GGTAGS_TARBALL_SHA256" ggt-test-tarball-sha256)
    (setenv "LC_ALL" "C.UTF-8")
    (setenv "GTAGSROOT" nil)
    (setenv "GTAGSDBPATH" nil)
    (setenv "GTAGSLABEL" nil)
    (setenv "GTAGSCONF" nil)
    (setenv "GTAGSLIBPATH" nil)
    (list :bin bin :plan plan-file :state state-file :trace trace-file)))

(defun ggt-test-trace (fixture project)
  "Return the exact NUL field trace with owned paths normalized."
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally (plist-get fixture :trace))
    (mapcar
     (lambda (field)
       (let ((text (decode-coding-string field 'utf-8)))
         (replace-regexp-in-string
          (regexp-quote (directory-file-name project)) "[ROOT]" text t t)))
     (split-string (buffer-string) "\0" t))))

(defun ggt-test-fixture-state (fixture project)
  "Prove exact plan exhaustion and that no request missed."
  (let* ((json-object-type 'alist)
         (json-key-type 'symbol)
         (state (json-read-file (plist-get fixture :state)))
         (trace (ggt-test-trace fixture project))
         (misses (seq-filter (lambda (field) (equal field "MISS")) trace))
         (help-contracts
          (delq nil (mapcar (lambda (record)
                              (alist-get 'stdout_contract record))
                            ggt-test-plan)))
         (stream-contracts
          (delete-dups
           (mapcar (lambda (record)
                     (alist-get 'recording_stream_sha256 record))
                   ggt-test-plan))))
    (unless (and (= (alist-get 'index state) (length ggt-test-plan))
                 (null misses))
      (error "GGTAGS replay plan incomplete or missed: %S"
             (list state :planned (length ggt-test-plan) :trace trace)))
    (list :index (alist-get 'index state)
          :planned (length ggt-test-plan)
          :generation (alist-get 'generation state)
          :misses misses
          :help-stdout-contracts
          (list :count (length help-contracts)
                :values (delete-dups (copy-sequence help-contracts)))
          :recording-stream-contracts
          (list :count (length ggt-test-plan)
                :values stream-contracts)
          :trace trace)))

(defun ggt-test-answer-yes-or-no (prompt)
  "Consume one exact public yes/no prompt plan."
  (unless ggt-test-prompts
    (error "GGTAGS unexpected yes-or-no prompt: %S" prompt))
  (let ((expected (pop ggt-test-prompts)))
    (unless (equal prompt (car expected))
      (error "GGTAGS yes-or-no prompt mismatch: %S != %S"
             prompt (car expected)))
    (push (copy-tree expected) ggt-test-prompt-ledger)
    (cdr expected)))

(defun ggt-test-prompt-calls ()
  (reverse (copy-tree ggt-test-prompt-ledger)))

(defun ggt-test-capture-temp-buffer ()
  "Capture the real public temporary buffer after it is displayed."
  (setq ggt-test-temp-buffer-text
        (buffer-substring-no-properties (point-min) (point-max))))

(defun ggt-test-messages-point ()
  (with-current-buffer (get-buffer-create "*Messages*") (point-max)))

(defun ggt-test-observe-message (original format-string &rest arguments)
  "Record one genuine message, then delegate to ORIGINAL unchanged."
  (let ((text (apply #'format-message format-string arguments)))
    (push (substring-no-properties text) ggt-test-message-ledger)
    (apply original format-string arguments)))

(defun ggt-test-messages-since (point)
  "Return stable Ggtags-owned messages logged after POINT."
  (with-current-buffer (get-buffer-create "*Messages*")
    (let ((lines (split-string
                  (buffer-substring-no-properties point (point-max)) "\n" t)))
      (mapcar
       (lambda (line)
         (replace-regexp-in-string "done ([0-9]+\\.[0-9]+s)" "done (<TIME>)"
                                   line t t))
       (seq-filter
        (lambda (line)
          (string-match-p
           "\\(?:`gtags' in progress\\|`global -u' in progress\\|GTAGS generated\\)"
           line))
        lines)))))

(defun ggt-test-read-file (file)
  (with-temp-buffer
    (insert-file-contents-literally file)
    (buffer-string)))

(defun ggt-test-read-state (fixture)
  (let ((json-object-type 'alist)
        (json-key-type 'symbol))
    (json-read-file (plist-get fixture :state))))

(defun ggt-test-piped-global-start (original &rest arguments)
  "Start Global over a pipe rather than a PTY, or signal.
`ggtags-global-filter' runs from `compilation-filter-hook' and mutates outside
any whole-lines guard: it deletes a \"Using config file ...\" line with a
`re-search-backward' bounded BELOW by `compilation-filter-start', and it feeds
`ggtags-global-output-lines' with `(count-lines compilation-filter-start
\(point))', which counts a partial line as a whole one (ggtags.el:1580-1608).
Both the deletion and the auto-jump that consumes that count are therefore
decided by where a read landed, and read boundaries are not a parity signal.
`compilation-start' gives the child a PTY by default (GNU
src/process.c:8923-8929) and a PTY's line discipline is the only topology here
that can deliver half a line.  This advice is the suite's single chokepoint:
every Ggtags entry point reaches Global through `ggtags-global-start', so no
case can start a search that skips the guard.  See DIVERGENCES.md 133 and 144."
  (let* ((buffer (let ((process-connection-type nil))
                   (apply original arguments)))
         (process (and (buffer-live-p buffer) (get-buffer-process buffer))))
    (unless process
      (error "ggt-test-piped-global-start: no Global process is attached to \
%S, so the pipe guard could not be checked" buffer))
    (when (process-tty-name process)
      (error "ggt-test-piped-global-start: Global is PTY-connected (%s); its \
output would arrive in scheduling-dependent chunks"
             (process-tty-name process)))
    buffer))

(advice-add 'ggtags-global-start :around #'ggt-test-piped-global-start)

(defun ggt-test-wait-index (fixture expected-index)
  "Pump the real process loop until replay state reaches EXPECTED-INDEX."
  (let ((deadline (+ (float-time) 15.0))
        (stable 0)
        previous)
    (while (and (< (float-time) deadline) (< stable 3))
      (accept-process-output nil 0.01)
      (let* ((state (ggt-test-read-state fixture))
             (index (alist-get 'index state))
             (owned-live
              (seq-some
               (lambda (process)
                 (let ((command (process-command process)))
                   (and (process-live-p process)
                        command
                        (seq-some (lambda (word)
                                    (and (stringp word)
                                         (string-match-p "/\\(?:global\\|gtags\\)\\'"
                                                         word)))
                                  command))))
               (process-list)))
             (current (list index (and owned-live t))))
        (if (and (>= index expected-index) (not owned-live)
                 (equal current previous))
            (setq stable (1+ stable))
          (setq stable 0))
        (setq previous current)))
    (let ((state (ggt-test-read-state fixture)))
      (unless (and (>= (alist-get 'index state) expected-index)
                   (= stable 3))
        (error "GGTAGS process/replay wait timed out: %S processes=%S"
               (list state
                     :trace
                     (ggt-test-trace
                      fixture
                      (file-name-as-directory
                       (getenv "NEOMACS_GGTAGS_PROJECT_ROOT")))
                     :global-buffer
                     (and (buffer-live-p ggtags-global-last-buffer)
                          (with-current-buffer ggtags-global-last-buffer
                            (list :mode major-mode
                                  :exit ggtags-global-exit-info
                                  :text (buffer-substring-no-properties
                                         (point-min) (point-max))))))
               (mapcar (lambda (process)
                         (list (process-name process) (process-status process)
                               (process-command process)))
                       (process-list))))
      state)))

(defun ggt-test-wait-global (fixture expected-index)
  "Wait for replay EXPECTED-INDEX and a stable real Global sentinel state."
  (ggt-test-wait-index fixture expected-index)
  (let ((deadline (+ (float-time) 15.0))
        (stable 0)
        previous)
    (while (and (< (float-time) deadline) (< stable 3))
      (accept-process-output nil 0.01)
      (let* ((buffer ggtags-global-last-buffer)
             (current
              (and (buffer-live-p buffer)
                   (with-current-buffer buffer
                     (list (and (derived-mode-p 'ggtags-global-mode) t)
                           ggtags-global-exit-info
                           (not (get-buffer-process buffer))
                           (buffer-substring-no-properties
                            (point-min) (point-max)))))))
        (if (and current (nth 1 current) (nth 2 current)
                 (equal current previous))
            (setq stable (1+ stable))
          (setq stable 0))
        (setq previous current)))
    (unless (= stable 3)
      (error "GGTAGS Global sentinel wait timed out: buffer=%S info=%S process=%S"
             ggtags-global-last-buffer
             (and (buffer-live-p ggtags-global-last-buffer)
                  (buffer-local-value 'ggtags-global-exit-info
                                      ggtags-global-last-buffer))
             (and (buffer-live-p ggtags-global-last-buffer)
                  (get-buffer-process ggtags-global-last-buffer))))
    ggtags-global-last-buffer))

(defun ggt-test-global-text (buffer)
  "Return complete BUFFER text with only volatile compilation time normalized."
  (with-current-buffer buffer
    (replace-regexp-in-string
     "^Global \\(?:started\\|found\\|finished\\|exited\\).*$"
     "Global <STATUS>"
     (buffer-substring-no-properties (point-min) (point-max)))))

(defun ggt-test-location ()
  "Return the current real file location and complete source line."
  (list (and buffer-file-name (file-name-nondirectory buffer-file-name))
        (line-number-at-pos) (current-column) (ggt-test-line)
        (and (eq (window-buffer) (current-buffer)) t)))

(defun ggt-test-property-runs (property)
  "Return every non-nil PROPERTY run in the current real buffer."
  (let ((position (point-min))
        runs)
    (while (< position (point-max))
      (let* ((value (get-text-property position property))
             (next (next-single-property-change
                    position property nil (point-max))))
        (when value
          (push (list (buffer-substring-no-properties position next) value)
                runs))
        (setq position next)))
    (nreverse runs)))

(defun ggt-test-wait-until (description predicate)
  "Pump the real event loop until PREDICATE is stably true."
  (let ((deadline (+ (float-time) 15.0))
        (stable 0))
    (while (and (< (float-time) deadline) (< stable 3))
      (accept-process-output nil 0.01)
      (if (funcall predicate)
          (setq stable (1+ stable))
        (setq stable 0)))
    (unless (= stable 3)
      (error "GGTAGS condition timed out: %s buffers=%S processes=%S"
             description (mapcar #'buffer-name (buffer-list))
             (mapcar (lambda (process)
                       (list (process-name process) (process-status process)
                             (process-command process)))
                     (process-list))))
    t))

(defun ggt-test-line ()
  (buffer-substring-no-properties
   (line-beginning-position) (line-end-position)))

(defun ggt-test-normalize (object case-root project)
  "Normalize only validated owned CASE-ROOT and PROJECT paths."
  (cond
   ((stringp object)
   (let ((text (replace-regexp-in-string
                 (regexp-quote (directory-file-name project))
                 "[ROOT]" object t t)))
      (replace-regexp-in-string
       (regexp-quote (directory-file-name case-root))
       "[CASE]" text t t)))
   ((proper-list-p object)
    ;; Some practical workflows make dozens of exact external calls.  Avoid
    ;; recursive descent through every cdr of their flat NUL trace.
    (mapcar (lambda (value)
              (ggt-test-normalize value case-root project))
            object))
   ((consp object)
    (cons (ggt-test-normalize (car object) case-root project)
          (ggt-test-normalize (cdr object) case-root project)))
   ((vectorp object)
    (apply #'vector
           (mapcar (lambda (value)
                     (ggt-test-normalize value case-root project))
                   object)))
   (t object)))

(defun ggt-test-kill-process (process)
  (let ((deadline (+ (float-time) 5.0))
        (polls-left 500))
    (set-process-query-on-exit-flag process nil)
    (when (process-live-p process) (delete-process process))
    (while (and (process-live-p process)
                (> polls-left 0)
                (< (float-time) deadline))
      (setq polls-left (1- polls-left))
      (accept-process-output process 0.01)))
  (when (process-live-p process)
    (error "GGTAGS process survived teardown: %S" process)))

(defun ggt-test-run (name function)
  "Run FUNCTION in one fully owned Ggtags world NAME."
  (let ((sandbox-root (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
    (unless (and (stringp sandbox-root) (> (length sandbox-root) 0)
                 (file-name-absolute-p sandbox-root))
      (error "NEOMACS_TEST_SANDBOX_ROOT must be a nonempty absolute path"))
    (unless (string-match-p "\\`[a-z0-9-]+\\'" name)
      (error "GGTAGS invalid case name: %S" name))
    (let* ((case-root (file-name-as-directory
                       (expand-file-name name sandbox-root)))
           (project (file-name-as-directory
                     (expand-file-name "project Ω space" case-root)))
           (root-owned nil)
           (buffer-baseline (buffer-list))
           (process-baseline (process-list))
           (timer-baseline (copy-sequence timer-list))
           (idle-timer-baseline (copy-sequence timer-idle-list))
           (window-baseline (window-buffer))
           (process-environment (copy-sequence process-environment))
           (exec-path (copy-sequence exec-path))
           (next-error-hook (copy-sequence next-error-hook))
           (next-error-last-buffer nil)
           (compilation-last-buffer nil)
           (compilation-in-progress (copy-sequence compilation-in-progress))
           (compilation-arguments compilation-arguments)
           (minibuffer-setup-hook (copy-sequence minibuffer-setup-hook))
           (emulation-mode-map-alists (copy-sequence emulation-mode-map-alists))
           (ggt-test-xref-history (xref--make-xref-history))
           (xref-history-storage (lambda () ggt-test-xref-history))
           (xref--read-identifier-history nil)
           (xref--read-pattern-history nil)
           (ggtags-projects (make-hash-table :size 7 :test #'equal))
           (ggtags-global-search-history nil)
           (ggtags-view-search-history-last nil)
           (ggtags-global-last-buffer nil)
           (ggtags-global-continuation nil)
           (ggtags-current-tag-name nil)
           (ggtags-global-start-marker nil)
           (ggtags-global-start-file nil)
           (ggtags-tag-ring-index nil)
           (ggtags-global-line-overlay nil)
           (ggtags-highlight-tag-overlay nil)
           (ggtags-highlight-tag-timer nil)
           (ggtags-navigation-mode nil)
           (ggtags-highlight-tag nil)
           (ggtags-mode-sticky nil)
           (ggtags-use-project-gtagsconf nil)
           (ggtags-use-sqlite3 nil)
           (ggtags-extra-args nil)
           (ggtags-global-abbreviate-filename nil)
           (ggtags-global-search-libpath-for-reference nil)
           (enable-local-variables nil)
           (enable-local-eval nil)
           (ggt-test-plan nil)
           (ggt-test-prompts nil)
           (ggt-test-prompt-ledger nil)
           (ggt-test-owned-overlays nil)
           (ggt-test-temp-buffer-text nil)
           (ggt-test-message-ledger nil)
           (ggt-test-recording-project project)
           result cleanup body-error cleanup-errors)
      (when (file-exists-p case-root)
        (error "GGTAGS owned case root already exists: %s" case-root))
      (cl-labels
          ((attempt (phase thunk)
             (condition-case condition
                 (funcall thunk)
               (t (push (list phase condition) cleanup-errors) nil)))
           (sweep-owned (phase)
             (dolist (process (seq-difference (process-list)
                                              process-baseline #'eq))
               (attempt (list phase 'process)
                        (lambda () (ggt-test-kill-process process))))
             (dolist (timer (seq-difference timer-list timer-baseline #'eq))
               (attempt (list phase 'timer)
                        (lambda () (cancel-timer timer))))
             (dolist (timer (seq-difference timer-idle-list
                                             idle-timer-baseline #'eq))
               (attempt (list phase 'idle-timer)
                        (lambda () (cancel-timer timer))))
             (dolist (buffer (seq-difference (buffer-list)
                                             buffer-baseline #'eq))
               (attempt
                (list phase 'buffer)
                (lambda ()
                  (when (buffer-live-p buffer)
                    (with-current-buffer buffer (set-buffer-modified-p nil))
                    (kill-buffer buffer)))))))
        (unwind-protect
            (condition-case condition
                (progn
                  (unwind-protect (make-directory case-root)
                    (when (file-directory-p case-root) (setq root-owned t)))
                  (unless root-owned
                    (error "GGTAGS failed to own case root: %s" case-root))
                  (make-directory project)
                  (ggt-test-create-project project)
                  (ggt-test-validate-project-manifest
                   project ggt-test-initial-fixture-manifest)
                  ;; Filename/coding setup can create GNU's reusable internal
                  ;; conversion buffer. It belongs to this package adapter,
                  ;; not to a workflow, so establish the per-case baseline
                  ;; only after the canonical fixture is fully materialized.
                  (get-buffer-create " *code-conversion-work*")
                  (setq buffer-baseline (buffer-list))
                  (save-window-excursion
                    (save-current-buffer
                      (setq result (funcall function case-root project)))))
              (t (setq body-error condition)))
          (attempt 'modes
                   (lambda ()
                     (when ggtags-navigation-mode (ggtags-navigation-mode -1))
                     (dolist (buffer (buffer-list))
                       (when (and (buffer-live-p buffer)
                                  (buffer-local-value 'ggtags-mode buffer))
                         (with-current-buffer buffer (ggtags-mode -1))))
                     (ggtags-cancel-highlight-tag-at-point)))
          (attempt
           'navigation-state
           (lambda ()
             (xref-clear-marker-stack)
             (when (markerp ggtags-global-start-marker)
               (set-marker ggtags-global-start-marker nil nil))
             (setq ggtags-global-start-marker nil
                   ggtags-global-start-file nil
                   ggtags-tag-ring-index nil)
             (when (overlayp ggtags-global-line-overlay)
               (delete-overlay ggtags-global-line-overlay))
             (setq ggtags-global-line-overlay nil)
             (when (overlayp ggtags-highlight-tag-overlay)
               (delete-overlay ggtags-highlight-tag-overlay))
             (setq ggtags-highlight-tag-overlay nil)))
          (attempt 'projects (lambda () (clrhash ggtags-projects)))
          (sweep-owned 'first-sweep)
          (dolist (overlay ggt-test-owned-overlays)
            (attempt 'overlay
                     (lambda ()
                       (when (overlayp overlay) (delete-overlay overlay)))))
          ;; Filters, sentinels, and kill hooks can allocate late owned state.
          ;; A second bounded sweep closes those ownership edges before the
          ;; validated root is removed.
          (sweep-owned 'second-sweep)
          (attempt 'root
                   (lambda ()
                     (when root-owned
                       (when (file-exists-p case-root)
                         (delete-directory case-root t))
                       (unless (file-exists-p case-root)
                         (setq root-owned nil)))))
          (attempt
           'state
           (lambda ()
             (setq cleanup
                   (list :new-buffers
                         (delq nil (mapcar (lambda (buffer)
                                            (and (buffer-live-p buffer)
                                                 (buffer-name buffer)))
                                          (seq-difference (buffer-list)
                                                          buffer-baseline #'eq)))
                         :new-processes
                         (mapcar #'process-name
                                 (seq-difference (process-list)
                                                 process-baseline #'eq))
                         :compilation-last-buffer
                         (and (buffer-live-p compilation-last-buffer)
                              (buffer-name compilation-last-buffer))
                         :compilation-processes
                         (delq nil
                               (mapcar (lambda (process)
                                         (and (process-live-p process)
                                              (process-name process)))
                                       compilation-in-progress))
                         :new-timers
                         (+ (length (seq-difference timer-list
                                                    timer-baseline #'eq))
                            (length (seq-difference timer-idle-list
                                                    idle-timer-baseline #'eq)))
                         :root-exists (file-exists-p case-root)
                         :root-owned root-owned
                         :window-restored (eq (window-buffer) window-baseline)
                         :navigation ggtags-navigation-mode
                         :xref-history
                         (list (length (car ggt-test-xref-history))
                               (length (cdr ggt-test-xref-history)))
                         :start-marker ggtags-global-start-marker
                         :start-file ggtags-global-start-file
                         :line-overlay ggtags-global-line-overlay
                         :highlight-overlay ggtags-highlight-tag-overlay
                         :project-count (hash-table-count ggtags-projects)
                         :prompts-remaining (copy-tree ggt-test-prompts)
                         :prompt-calls (ggt-test-prompt-calls)
                         :body-error body-error
                         :cleanup-errors (nreverse cleanup-errors)))
             (when (or (plist-get cleanup :new-buffers)
                       (plist-get cleanup :new-processes)
                       (plist-get cleanup :compilation-last-buffer)
                       (plist-get cleanup :compilation-processes)
                       (not (zerop (plist-get cleanup :new-timers)))
                       (plist-get cleanup :root-exists)
                       (plist-get cleanup :root-owned)
                       (not (plist-get cleanup :window-restored))
                       (plist-get cleanup :navigation)
                       (not (equal (plist-get cleanup :xref-history) '(0 0)))
                       (plist-get cleanup :start-marker)
                       (plist-get cleanup :start-file)
                       (plist-get cleanup :line-overlay)
                       (plist-get cleanup :highlight-overlay)
                       (not (zerop (plist-get cleanup :project-count)))
                       (plist-get cleanup :prompts-remaining))
               (error "GGTAGS final cleanup ledger is not clean: %S"
                      cleanup))))))
      (when body-error
        (if cleanup-errors
            (error "GGTAGS body failed: %S; cleanup failed: %S"
                   body-error cleanup-errors)
          (signal (car body-error) (cdr body-error))))
      (when cleanup-errors
        (error "GGTAGS cleanup failed: %S" cleanup-errors))
      (list :result (ggt-test-normalize result case-root project)
            :cleanup (ggt-test-normalize cleanup case-root project)))))
"####;

fn ggtags_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(GGTAGS_MELPA_PIN, "ggtags.el")
        .expect("prepare pinned ggtags source below ./tmp")
        .with_prelude(format!(
            "{GGTAGS_TEST_PRELUDE}\n(setq ggt-test-replay-script {GGTAGS_REPLAY_SCRIPT:?})"
        ))
        .with_timeout(GGTAGS_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed ggtags parity test")
        .into()
}

fn assert_ggtags_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        ggtags_oracle(),
        &current_test_name(),
        "ggtags_parity",
        cases,
    );
}

#[test]
fn ggtags_package_batch() {
    assert_ggtags_batch(&workflows::public_workflow_cases());
}
