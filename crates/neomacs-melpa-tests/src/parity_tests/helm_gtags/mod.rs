//! Practical parity for Helm Gtags' documented GNU Global workflows.
//!
//! The cases drive public package commands through a closed executable replay
//! of byte-exact GNU Global 6.6.14 observations. Helm, process sentinels,
//! buffers, files, navigation, context stacks, mode hooks, and cleanup remain
//! real editor behavior.

use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, HELM_GTAGS_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const TEST_TIMEOUT: Duration = Duration::from_secs(240);

const REPLAY_SCRIPT: &str = r#"#!/usr/bin/env python3
import fcntl, hashlib, json, os, pathlib, sys

ROOT = pathlib.Path(os.environ["HGT397_ROOT"]).resolve()
PLAN = pathlib.Path(os.environ["HGT397_PLAN"])
STATE = pathlib.Path(os.environ["HGT397_STATE"])
TRACE = pathlib.Path(os.environ["HGT397_TRACE"])
INITIAL = {
    "include/math.h": "14aec838294e0fa36ba4db62778a602cd92daa7e00c4b68841aa44b2b7a0da2f",
    "src/main.c": "192cc45c9a6982491777dc03e0cc330a516ac25097d43a1bb682d4c6daa35cb4",
    "src/math.c": "9571ff7cd9a64068e087718a1768aaccf3fa309d67c4344abce2ca615a4481ff",
}
UPDATED_MATH = "059fb6354bad039c193439f6ece42b32b082440409672b7afc0ca7a2e68f6b4b"

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def expand(value):
    return value.replace("[ROOT]", str(ROOT))

def append(record):
    with TRACE.open("a", encoding="utf-8") as stream:
        stream.write(json.dumps(record, ensure_ascii=False, sort_keys=True) + "\n")

program = pathlib.Path(sys.argv[0]).name
argv = sys.argv[1:]
cwd = pathlib.Path.cwd().resolve()
try:
    relative_cwd = cwd.relative_to(ROOT).as_posix() or "."
except ValueError:
    append({"kind": "MISS", "reason": "cwd escaped root", "cwd": str(cwd)})
    raise SystemExit(86)

with STATE.open("r+", encoding="utf-8") as state_stream:
    fcntl.flock(state_stream.fileno(), fcntl.LOCK_EX)
    state = json.load(state_stream)
    plan = json.loads(PLAN.read_text(encoding="utf-8"))
    index = state["index"]
    actual = {"program": program, "cwd": relative_cwd, "args": argv}
    actual_env = {name: os.environ.get(name) for name in
                  ("GTAGSROOT", "GTAGSDBPATH", "GTAGSLABEL", "GTAGSCONF",
                   "GTAGSLIBPATH", "LC_ALL")}
    expected_env = {"GTAGSROOT": str(ROOT) + os.sep, "GTAGSDBPATH": None,
                    "GTAGSLABEL": None, "GTAGSCONF": None,
                    "GTAGSLIBPATH": None, "LC_ALL": "C.UTF-8"}
    if index >= len(plan):
        append({"kind": "MISS", "index": index, "actual": actual,
                "reason": "plan exhausted"})
        raise SystemExit(86)
    expected = plan[index]
    expected_shape = {
        "program": expected["program"],
        "cwd": expected.get("cwd", "."),
        "args": [expand(word) for word in expected["args"]],
    }
    fixture = expected.get("fixture", "initial")
    wanted = dict(INITIAL)
    if fixture == "updated":
        wanted["src/math.c"] = UPDATED_MATH
    files_ok = all((ROOT / name).is_file() and digest(ROOT / name) == sha
                   for name, sha in wanted.items())
    if actual != expected_shape or actual_env != expected_env or not files_ok:
        append({"kind": "MISS", "index": index, "actual": actual,
                "expected": expected_shape, "fixture": fixture,
                "environment": actual_env, "files_ok": files_ok})
        raise SystemExit(86)
    action = expected.get("action")
    if action == "create-database":
        for name in ("GPATH", "GRTAGS", "GTAGS"):
            (ROOT / name).write_bytes(("GNU Global 6.6.14 replay " + name + "\n").encode())
    state["index"] = index + 1
    state_stream.seek(0)
    json.dump(state, state_stream, sort_keys=True)
    state_stream.truncate()
    state_stream.flush()
    os.fsync(state_stream.fileno())
    append({"kind": "CALL", "index": index, **actual,
            "environment": actual_env, "fixture": fixture,
            "status": expected.get("status", 0)})

sys.stdout.write(expand(expected.get("stdout", "")))
sys.stderr.write(expand(expected.get("stderr", "")))
raise SystemExit(expected.get("status", 0))
"#;

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'json)
(require 'seq)
(require 'subr-x)
(require 'cc-mode)
(require 'helm-gtags)

(defconst hgt397-test-source-sha256
  "2655760b62ca548b8ba2a0da668c1da5a0a2e4bb4a37734ae4660eac2c6795f0")
(defconst hgt397-test-global-version "global (GNU Global) 6.6.14")
(defconst hgt397-test-global-sha256
  "c3f48142bfc80a1c3e7c8f15f4b73bc01685ed70e9d6dfec8c0c451b9bb4f521")
(defconst hgt397-test-gtags-sha256
  "63579e2a4ba7afefce03a90c3e7f1dd2534b64c16687317971d87031ebbd8449")
(defconst hgt397-test-main
  "#include \"math.h\"\nint main(void) {\n  return compute_total();\n}\n")
(defconst hgt397-test-math
  (concat "#include \"math.h\"\n"
          "int add_values(int left, int right) {\n"
          "  return left + right; /* café 界 */\n}\n"
          "/* Parse-file exercises a two-digit definition line.\n"
          " * Unicode fixture: café 界.\n *\n"
          " * Keep this spacing stable for Global’s cscope output.\n */\n\n\n"
          "int compute_total(void) {\n  return add_values(2, 3);\n}\n"))
(defconst hgt397-test-math-updated
  (concat hgt397-test-math
          "\nint multiply_values(int left, int right) {\n"
          "  return left * right;\n}\n"))
(defconst hgt397-test-header
  "int add_values(int left, int right);\nint compute_total(void);\n")

(defconst hgt397-test-real-process-file (symbol-function 'process-file))
(defconst hgt397-test-real-start-file-process
  (symbol-function 'start-file-process))
(defconst hgt397-test-real-start-process (symbol-function 'start-process))
(defconst hgt397-test-real-make-process (symbol-function 'make-process))
(defconst hgt397-test-real-message (symbol-function 'message))

(defvar hgt397-test-root nil)
(defvar hgt397-test-fixture nil)
(defvar hgt397-test-owned-processes nil)
(defvar hgt397-test-start-depth 0)
(defvar hgt397-test-ui-plans nil)
(defvar hgt397-test-ui-calls nil)
(defvar hgt397-test-message-ledger nil)

(defun hgt397-test-message (format-string &rest arguments)
  (let ((text (and format-string
                   (apply #'format-message format-string arguments))))
    (when (and text
               (string-match-p "\\`\\(?:Success\\|Failed\\): .* TAGS" text))
      (push (copy-sequence text) hgt397-test-message-ledger))
    (apply hgt397-test-real-message format-string arguments)))

(defun hgt397-test-file-sha256 (file)
  (with-temp-buffer
    (set-buffer-multibyte nil)
    (insert-file-contents-literally file)
    (secure-hash 'sha256 (current-buffer))))

(let ((file (symbol-file 'helm-gtags-mode 'defun)))
  (unless (and (file-regular-p file)
               (equal (file-name-nondirectory file) "helm-gtags.el")
               (equal (hgt397-test-file-sha256 file)
                      hgt397-test-source-sha256))
    (error "Unexpected installed Helm Gtags source: %S" file)))

(defun hgt397-test-write (root relative contents)
  (let ((file (expand-file-name relative root)))
    (unless (string-prefix-p root file)
      (error "Helm Gtags fixture escaped root: %S" relative))
    (make-directory (file-name-directory file) t)
    (with-temp-file file (insert contents))
    file))

(defun hgt397-test-create-project (root)
  (hgt397-test-write root "src/main.c" hgt397-test-main)
  (hgt397-test-write root "src/math.c" hgt397-test-math)
  (hgt397-test-write root "include/math.h" hgt397-test-header))

(defun hgt397-test-seed-database (root)
  (dolist (name '("GPATH" "GRTAGS" "GTAGS"))
    (hgt397-test-write root name (format "GNU Global 6.6.14 replay %s\n" name))))

(defun hgt397-test-manifest (root)
  (mapcar (lambda (relative)
            (list relative
                  (hgt397-test-file-sha256 (expand-file-name relative root))))
          '("include/math.h" "src/main.c" "src/math.c")))

(defun hgt397-test-expected-manifest (manifest plan)
  (let ((expected (copy-tree manifest)))
    (when (seq-some (lambda (record)
                      (equal (alist-get 'fixture record) "updated"))
                    plan)
      (setf (cadr (assoc "src/math.c" expected))
            "059fb6354bad039c193439f6ece42b32b082440409672b7afc0ca7a2e68f6b4b"))
    expected))

(defun hgt397-test-window-state ()
  (list :selected (selected-window)
        :windows
        (mapcar
         (lambda (window)
           (list :window window :buffer (window-buffer window)
                 :point (window-point window) :start (window-start window)
                 :hscroll (window-hscroll window)
                 :vscroll (window-vscroll window t)
                 :prev (copy-tree (window-prev-buffers window))
                 :next (copy-tree (window-next-buffers window))))
         (window-list nil 'no-minibuf))))

(defun hgt397-test-restore-windows (configuration state)
  (set-window-configuration configuration)
  (dolist (entry (plist-get state :windows))
    (let ((window (plist-get entry :window)))
      (unless (window-live-p window)
        (error "Baseline Helm Gtags window died: %S" window))
      (set-window-buffer window (plist-get entry :buffer))
      (set-window-prev-buffers window (copy-tree (plist-get entry :prev)))
      (set-window-next-buffers window (copy-tree (plist-get entry :next)))
      (set-window-point window (plist-get entry :point))
      (set-window-start window (plist-get entry :start) 'noforce)
      (set-window-hscroll window (plist-get entry :hscroll))
      (set-window-vscroll window (plist-get entry :vscroll) t)))
  (select-window (plist-get state :selected)))

(defun hgt397-test-record
    (program args stdout &optional status action fixture cwd)
  `((program . ,program) (cwd . ,(or cwd "."))
    (args . ,(apply #'vector args))
    (stdout . ,(or stdout "")) (stderr . "") (status . ,(or status 0))
    (action . ,action) (fixture . ,(or fixture "initial"))))

(defun hgt397-test-install-replay (root script plan)
  (let* ((bin (file-name-as-directory (expand-file-name "bin" root)))
         (plan-file (expand-file-name "plan.json" root))
         (state-file (expand-file-name "state.json" root))
         (trace-file (expand-file-name "trace.jsonl" root)))
    (make-directory bin)
    (dolist (program '("global" "gtags"))
      (let ((file (hgt397-test-write root (concat "bin/" program) script)))
        (set-file-modes file #o755)))
    (let ((json-encoding-pretty-print nil))
      (hgt397-test-write root "plan.json" (json-encode plan)))
    (hgt397-test-write root "state.json" "{\"index\":0}")
    (hgt397-test-write root "trace.jsonl" "")
    (setenv "HGT397_ROOT" (directory-file-name root))
    (setenv "HGT397_PLAN" plan-file)
    (setenv "HGT397_STATE" state-file)
    (setenv "HGT397_TRACE" trace-file)
    (setq exec-path (cons bin exec-path))
    (setenv "PATH" (concat bin path-separator (getenv "PATH")))
    (list :bin bin :plan plan :state state-file :trace trace-file)))

(defun hgt397-test-trace (fixture root)
  (with-temp-buffer
    (insert-file-contents (plist-get fixture :trace))
    (let ((json-object-type 'plist) (json-array-type 'list) records)
      (dolist (line (split-string (buffer-string) "\n" t))
        (push (json-read-from-string line) records))
      (hgt397-test-normalize (nreverse records) root))))

(defun hgt397-test-boundary-state (fixture root)
  (let* ((json-object-type 'alist)
         (json-key-type 'symbol)
         (state (json-read-file (plist-get fixture :state)))
         (trace (hgt397-test-trace fixture root))
         (misses (seq-filter (lambda (entry)
                               (equal (plist-get entry :kind) "MISS"))
                             trace)))
    (unless (and (= (alist-get 'index state)
                    (length (plist-get fixture :plan)))
                 (null misses))
      (error "Helm Gtags replay incomplete: %S" (list state trace)))
    (list :index (alist-get 'index state)
          :planned (length (plist-get fixture :plan))
          :misses misses :trace trace)))

(defun hgt397-test-process-file (program infile destination display &rest args)
  (unless (and (equal program "global")
               (null infile)
               (or (eq destination t) (equal destination '(t nil)))
               (null display))
    (error "Unexpected Helm Gtags process-file: %S"
           (list program infile destination display args)))
  (apply hgt397-test-real-process-file
         (expand-file-name "global" (plist-get hgt397-test-fixture :bin))
         infile destination display args))

(defun hgt397-test-start-file-process (name buffer program &rest args)
  (unless (and (member name '("helm-gtags-create" "helm-gtags-update-tag"))
               (member program '("global" "gtags")))
    (error "Unexpected Helm Gtags start-file-process: %S"
           (list name buffer program args)))
  (let* ((executable
          (expand-file-name program (plist-get hgt397-test-fixture :bin)))
         (hgt397-test-start-depth (1+ hgt397-test-start-depth))
         (process (apply hgt397-test-real-start-file-process
                         name buffer executable args)))
    (unless (and (processp process)
                 (equal (process-command process) (cons executable args)))
      (when (processp process) (delete-process process))
      (error "Unexpected created Helm Gtags process: %S" process))
    (unless (memq process hgt397-test-owned-processes)
      (push process hgt397-test-owned-processes))
    process))

(defun hgt397-test-start-process (name buffer program &rest args)
  (unless (and (> hgt397-test-start-depth 0)
               (string-prefix-p (plist-get hgt397-test-fixture :bin) program))
    (error "Unexpected direct Helm Gtags start-process: %S"
           (list name buffer program args)))
  (let ((hgt397-test-start-depth (1+ hgt397-test-start-depth))
        (process (apply hgt397-test-real-start-process name buffer program args)))
    (push process hgt397-test-owned-processes)
    process))

(defun hgt397-test-make-process (&rest args)
  (unless (> hgt397-test-start-depth 0)
    (error "Unexpected direct Helm Gtags make-process: %S" args))
  (apply hgt397-test-real-make-process args))

(defun hgt397-test-wait-processes (fixture expected-index)
  (let ((deadline (+ (float-time) 10.0)) stable previous)
    (while (and (< (float-time) deadline) (< (or stable 0) 3))
      (dolist (process hgt397-test-owned-processes)
        (accept-process-output process 0.02))
      (let* ((json-object-type 'alist)
             (json-key-type 'symbol)
             (index (alist-get 'index
                               (json-read-file (plist-get fixture :state))))
             (live (seq-some #'process-live-p hgt397-test-owned-processes))
             (current (list index (and live t))))
        (if (and (>= index expected-index) (not live) (equal current previous))
            (setq stable (1+ (or stable 0)))
          (setq stable 0))
        (setq previous current)))
    (unless (= (or stable 0) 3)
      (error "Helm Gtags process wait timed out: %S"
             (hgt397-test-trace fixture hgt397-test-root)))
    (mapcar (lambda (process)
              (list :name (process-name process)
                    :status (process-status process)
                    :exit (process-exit-status process)))
            (reverse hgt397-test-owned-processes))))

(defun hgt397-test-face-runs (string)
  (let ((position 0) runs)
    (while (< position (length string))
      (let* ((face (get-text-property position 'face string))
             (next (or (next-single-property-change
                        position 'face string (length string))
                       (length string))))
        (when face
          (push (list :range (list position next)
                      :text (substring-no-properties string position next)
                      :face face)
                runs))
        (setq position next)))
    (nreverse runs)))

(defun hgt397-test-source (source)
  (if (symbolp source) (symbol-value source) source))

(defun hgt397-test-actions (source)
  (let ((action (helm-attr 'action source)))
    (if (and (symbolp action) (boundp action))
        (symbol-value action)
      action)))

(defun hgt397-test-location (root)
  (let ((buffer (window-buffer (selected-window))))
    (with-current-buffer buffer
      (list :file (and buffer-file-name
                       (file-relative-name buffer-file-name root))
            :line (line-number-at-pos (window-point (selected-window)))
            :column (save-excursion
                      (goto-char (window-point (selected-window)))
                      (current-column))
            :text (save-excursion
                    (goto-char (window-point (selected-window)))
                    (buffer-substring-no-properties
                     (line-beginning-position) (line-end-position)))))))

(defun hgt397-test-dispatch-helm (arguments)
  (unless hgt397-test-ui-plans
    (error "Unexpected Helm Gtags UI invocation: %S" arguments))
  (let* ((plan (pop hgt397-test-ui-plans))
         (sources-value (plist-get arguments :sources))
         (source-symbols (if (listp sources-value) sources-value
                           (list sources-value)))
         (candidate-index (or (plist-get plan :candidate) 0))
         (action-name (plist-get plan :action))
         source-states selected)
    (unless (equal (plist-get arguments :buffer) "*helm gtags*")
      (error "Unexpected Helm Gtags buffer: %S" arguments))
    (dolist (source-name source-symbols)
      (let* ((source (hgt397-test-source source-name))
             (helm-current-source source)
             (candidate-buffer (generate-new-buffer " *hgt397-candidates*"))
             (init (helm-attr 'init source))
             raw pairs)
        (cl-letf (((symbol-function 'helm-candidate-buffer)
                   (lambda (&rest _) candidate-buffer)))
          (when init
            (helm-apply-functions-from-source source init)))
        (setq raw (with-current-buffer candidate-buffer (buffer-string)))
        (setq pairs
              (mapcar
               (lambda (candidate)
                 (let* ((transformer (helm-attr 'real-to-display source))
                        (display (if transformer
                                     (helm-apply-functions-from-source
                                      source transformer candidate)
                                   candidate)))
                   (cons display candidate)))
               (split-string raw "\n" t)))
        (push (list :name (helm-attr 'name source)
                    :raw (hgt397-test-normalize raw hgt397-test-root)
                    :candidates
                    (mapcar (lambda (pair)
                              (list :display (substring-no-properties (car pair))
                                    :faces (hgt397-test-face-runs (car pair))
                                    :real (hgt397-test-normalize
                                           (copy-sequence (cdr pair))
                                           hgt397-test-root)))
                            pairs))
              source-states)
        (when (and action-name (null selected))
          (let* ((pair (nth candidate-index pairs))
                 (actions (hgt397-test-actions source))
                 (action (if (and (listp actions) (consp (car actions)))
                             (cdr (assoc action-name actions))
                           actions)))
            (unless (and pair (functionp action))
              (error "Missing Helm Gtags candidate/action: %S/%S"
                     pair action-name))
            (setq selected
                  (list :display (substring-no-properties (car pair))
                        :real (hgt397-test-normalize
                               (copy-sequence (cdr pair)) hgt397-test-root)))
            (funcall action (cdr pair))
            (run-hooks 'helm-after-action-hook)))
        (kill-buffer candidate-buffer)))
    (let ((state (list :sources (nreverse source-states)
                       :action action-name :selected selected
                       :location (and action-name
                                      (hgt397-test-location hgt397-test-root)))))
      (push state hgt397-test-ui-calls)
      state)))

(defun hgt397-test-run-ui (plans thunk)
  (let ((hgt397-test-ui-plans (copy-tree plans))
        (hgt397-test-ui-calls nil))
    (cl-letf (((symbol-function 'helm)
               (lambda (&rest arguments)
                 (hgt397-test-dispatch-helm arguments))))
      (funcall thunk))
    (when hgt397-test-ui-plans
      (error "Missing Helm Gtags UI calls: %S" hgt397-test-ui-plans))
    (nreverse hgt397-test-ui-calls)))

(defun hgt397-test-condition (condition root)
  (list :type (car condition)
        :data (hgt397-test-normalize (copy-tree (cdr condition)) root)
        :message (hgt397-test-normalize (error-message-string condition) root)))

(defun hgt397-test-normalize (value root)
  (cond ((stringp value)
         (replace-regexp-in-string (regexp-quote root) "[ROOT]/" value t t))
        ((consp value)
         (cons (hgt397-test-normalize (car value) root)
               (hgt397-test-normalize (cdr value) root)))
        ((vectorp value)
         (apply #'vector (mapcar (lambda (entry)
                                  (hgt397-test-normalize entry root))
                                value)))
        (t value)))

(defun hgt397-test-park-buffer (name)
  (when-let* ((buffer (get-buffer name)))
    (let ((old-name (buffer-name buffer)))
      (with-current-buffer buffer
        (rename-buffer (format " *hgt397-parked-%s*" (sxhash-eq buffer)) t))
      (cons buffer old-name))))

(defun hgt397-test-run-isolated (script plan seed body)
  (let* ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (root (and sandbox (file-name-as-directory
                             (expand-file-name "helm-gtags/" sandbox))))
         (window-before (current-window-configuration))
         (window-state-before (hgt397-test-window-state))
         (buffer-before (current-buffer))
         (buffers-before (buffer-list))
         (processes-before (process-list))
         (timers-before (copy-sequence timer-list))
         (frames-before (frame-list))
         (process-environment (copy-sequence process-environment))
         (exec-path (copy-sequence exec-path))
         (helm-source-gtags-tags (copy-tree helm-source-gtags-tags))
         (helm-source-gtags-pattern (copy-tree helm-source-gtags-pattern))
         (helm-source-gtags-rtags (copy-tree helm-source-gtags-rtags))
         (helm-source-gtags-gsyms (copy-tree helm-source-gtags-gsyms))
         (helm-source-gtags-files (copy-tree helm-source-gtags-files))
         (helm-source-gtags-parse-file (copy-tree helm-source-gtags-parse-file))
         (helm-after-action-hook (copy-sequence helm-after-action-hook))
         (helm-gtags--context-stack (make-hash-table :test 'equal))
         (helm-gtags--result-cache (make-hash-table :test 'equal))
         (helm-gtags--tag-location nil)
         (helm-gtags--real-tag-location nil)
         (helm-gtags--last-default-directory nil)
         (helm-gtags--local-directory nil)
         (helm-gtags--saved-context nil)
         (helm-gtags--current-position nil)
         (helm-gtags--use-otherwin nil)
         (helm-gtags--query nil)
         (helm-gtags--last-input nil)
         (helm-gtags--parsed-file nil)
         (helm-gtags--last-update-time 0)
         (helm-gtags-path-style 'root)
         (helm-gtags-highlight-candidate t)
         (helm-gtags-display-style nil)
         (helm-gtags-preselect nil)
         (helm-gtags-pulse-at-cursor nil)
         (helm-gtags-read-only nil)
         (helm-gtags-auto-update t)
         (message-log-max nil)
         (print-circle nil)
         (hgt397-test-root root)
         (hgt397-test-owned-processes nil)
         (hgt397-test-start-depth 0)
         (hgt397-test-message-ledger nil)
         (hgt397-test-fixture nil)
         (parked nil) (root-owned nil)
         fixture-before fixture-after result boundary body-error cleanup-errors)
    (unwind-protect
        (condition-case condition
            (progn
              (unless (and root (file-name-absolute-p root))
                (error "Missing absolute Helm Gtags sandbox root"))
              (when (file-exists-p root)
                (error "Helm Gtags sandbox root already exists: %s" root))
              (dolist (name '("*helm gtags*" " *helm-gtags-create*"))
                (when-let* ((entry (hgt397-test-park-buffer name)))
                  (push entry parked)))
              (make-directory root t)
              (setq root-owned t)
              (hgt397-test-create-project root)
              (when seed (hgt397-test-seed-database root))
              (setq fixture-before (hgt397-test-manifest root))
              (setq hgt397-test-fixture
                    (hgt397-test-install-replay root script plan))
              (setenv "GTAGSROOT" root)
              (setenv "GTAGSDBPATH" nil)
              (setenv "GTAGSLABEL" nil)
              (setenv "GTAGSCONF" nil)
              (setenv "GTAGSLIBPATH" nil)
              (setenv "LC_ALL" "C.UTF-8")
              (setq result
                    (cl-letf (((symbol-function 'process-file)
                               #'hgt397-test-process-file)
                              ((symbol-function 'start-file-process)
                               #'hgt397-test-start-file-process)
                              ((symbol-function 'start-process)
                               #'hgt397-test-start-process)
                              ((symbol-function 'make-process)
                               #'hgt397-test-make-process)
                              ((symbol-function 'message)
                               #'hgt397-test-message))
                      (funcall body root hgt397-test-fixture)))
              (setq fixture-after (hgt397-test-manifest root)))
          (error (setq body-error (hgt397-test-condition condition root))))
      (setq boundary
            (and hgt397-test-fixture
                 (condition-case condition
                     (hgt397-test-boundary-state hgt397-test-fixture root)
                   (error
                    (push (hgt397-test-condition condition root) cleanup-errors)
                    nil))))
      (dolist (process hgt397-test-owned-processes)
        (condition-case condition
            (when (process-live-p process) (delete-process process))
          (error (push (hgt397-test-condition condition root) cleanup-errors))))
      (dolist (process (process-list))
        (unless (memq process processes-before)
          (condition-case condition (delete-process process)
            (error (push (hgt397-test-condition condition root) cleanup-errors)))))
      (dolist (buffer (buffer-list))
        (unless (memq buffer buffers-before)
          (condition-case condition (kill-buffer buffer)
            (error (push (hgt397-test-condition condition root) cleanup-errors)))))
      (dolist (timer (copy-sequence timer-list))
        (unless (memq timer timers-before)
          (condition-case condition (cancel-timer timer)
            (error (push (hgt397-test-condition condition root) cleanup-errors)))))
      (dolist (frame (frame-list))
        (unless (memq frame frames-before)
          (condition-case condition (delete-frame frame t)
            (error (push (hgt397-test-condition condition root) cleanup-errors)))))
      (condition-case condition
          (hgt397-test-restore-windows window-before window-state-before)
        (error (push (hgt397-test-condition condition root) cleanup-errors)))
      (dolist (entry parked)
        (condition-case condition
            (if (buffer-live-p (car entry))
                (with-current-buffer (car entry)
                  (rename-buffer (cdr entry) t))
              (error "Parked Helm Gtags buffer died: %S" entry))
          (error (push (hgt397-test-condition condition root) cleanup-errors))))
      (when (buffer-live-p buffer-before) (set-buffer buffer-before))
      (when root-owned
        (condition-case condition (delete-directory root t)
          (error (push (hgt397-test-condition condition root) cleanup-errors)))))
    (let ((cleanup
           (list :fixture-accounted
                 (equal fixture-after
                        (hgt397-test-expected-manifest fixture-before plan))
                 :new-buffers
                 (mapcar #'buffer-name
                         (seq-filter (lambda (buffer)
                                       (and (buffer-live-p buffer)
                                            (not (memq buffer buffers-before))))
                                     (buffer-list)))
                 :new-processes
                 (length (seq-remove (lambda (process)
                                       (memq process processes-before))
                                     (process-list)))
                 :new-timers
                 (length (seq-remove (lambda (timer) (memq timer timers-before))
                                     timer-list))
                 :new-frames
                 (length (seq-remove (lambda (frame) (memq frame frames-before))
                                     (frame-list)))
                 :root-exists (and root (file-exists-p root))
                 :window-restored
                 (equal window-state-before (hgt397-test-window-state))
                 :buffer-restored (eq (current-buffer) buffer-before)
                 :body-error body-error
                 :cleanup-errors (nreverse cleanup-errors))))
      (if (or body-error cleanup-errors)
          (error "Helm Gtags workflow failed: %S" (list result boundary cleanup))
        (hgt397-test-normalize
         (list :provenance
               (list :source hgt397-test-source-sha256
                     :global-version hgt397-test-global-version
                     :global-sha hgt397-test-global-sha256
                     :gtags-sha hgt397-test-gtags-sha256
                     :fixture fixture-before)
               :result result :boundaries boundary :cleanup cleanup)
         root)))))

(defun hgt397-test-run (script plan seed body)
  (let ((package-state-before
         (copy-tree (list helm-gtags--query
                          helm-gtags--last-input
                          helm-gtags--parsed-file
                          helm-gtags--last-update-time)))
        result)
    (unwind-protect
        (setq result (hgt397-test-run-isolated script plan seed body))
      (unless (equal package-state-before
                     (list helm-gtags--query
                           helm-gtags--last-input
                           helm-gtags--parsed-file
                           helm-gtags--last-update-time))
        (error "Helm Gtags package state was not restored")))
    result))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(HELM_GTAGS_MELPA_PIN, "helm-gtags.el")
        .expect("prepare exact shallow Helm Gtags source below ./tmp")
        .with_prelude(PRELUDE)
        .with_timeout(TEST_TIMEOUT)
}

fn create_tags_mode_and_definition_navigation() -> ParityBatchCase {
    ParityBatchCase::value(
        "create_tags_mode_and_definition_navigation",
        format!(
            r####"
(hgt397-test-run
 {script:?}
 (list
  (hgt397-test-record "gtags" '("-q" "--gtagslabel=default") "" 0
                       "create-database")
  (hgt397-test-record "global" '("--result=grep" "add_values")
                       "src/math.c:2:int add_values(int left, int right) {{\n"))
 nil
 (lambda (root fixture)
   (helm-gtags-create-tags root "default")
   (let ((processes (hgt397-test-wait-processes fixture 1))
         mode-state calls)
     (let ((buffer (find-file-noselect (expand-file-name "src/main.c" root))))
       (set-window-buffer (selected-window) buffer)
       (set-buffer buffer)
       (helm-gtags-mode 1)
       (setq mode-state
             (list :enabled helm-gtags-mode
                   :after-save
                   (and (memq #'helm-gtags-update-tags after-save-hook) t)))
       (helm-gtags-mode -1)
       (setq mode-state
             (append mode-state
                     (list :disabled helm-gtags-mode
                           :hook-removed
                           (not (memq #'helm-gtags-update-tags after-save-hook)))))
       (setq calls
             (hgt397-test-run-ui
              '((:action "Open file" :candidate 0))
              (lambda () (helm-gtags-find-tag "add_values")))))
     (list :database
           (mapcar (lambda (name)
                     (list name
                           (file-regular-p (expand-file-name name root))))
                   '("GPATH" "GRTAGS" "GTAGS"))
           :processes processes :mode mode-state :helm calls
           :messages (nreverse (copy-sequence hgt397-test-message-ledger))))))
"####,
            script = REPLAY_SCRIPT,
        ),
        expect![[
            r#"OK (:provenance (:source "2655760b62ca548b8ba2a0da668c1da5a0a2e4bb4a37734ae4660eac2c6795f0" :global-version "global (GNU Global) 6.6.14" :global-sha "c3f48142bfc80a1c3e7c8f15f4b73bc01685ed70e9d6dfec8c0c451b9bb4f521" :gtags-sha "63579e2a4ba7afefce03a90c3e7f1dd2534b64c16687317971d87031ebbd8449" :fixture (("include/math.h" "14aec838294e0fa36ba4db62778a602cd92daa7e00c4b68841aa44b2b7a0da2f") ("src/main.c" "192cc45c9a6982491777dc03e0cc330a516ac25097d43a1bb682d4c6daa35cb4") ("src/math.c" "9571ff7cd9a64068e087718a1768aaccf3fa309d67c4344abce2ca615a4481ff"))) :result (:database (("GPATH" t) ("GRTAGS" t) ("GTAGS" t)) :processes ((:name "helm-gtags-create" :status exit :exit 0)) :mode (:enabled t :after-save t :disabled nil :hook-removed t) :helm ((:sources ((:name "add_values in [ROOT]/" :raw "src/math.c:2:int add_values(int left, int right) {\n" :candidates ((:display "src/math.c:2:int add_values(int left, int right) {" :faces ((:range (0 10) :text "src/math.c" :face helm-gtags-file) (:range (11 12) :text "2" :face helm-gtags-lineno) (:range (17 27) :text "add_values" :face helm-gtags-match)) :real "src/math.c:2:int add_values(int left, int right) {")))) :action "Open file" :selected (:display "src/math.c:2:int add_values(int left, int right) {" :real "src/math.c:2:int add_values(int left, int right) {") :location (:file "src/math.c" :line 2 :column 0 :text "int add_values(int left, int right) {"))) :messages ("Success: create TAGS")) :boundaries (:index 2 :planned 2 :misses nil :trace ((args ("-q" "--gtagslabel=default") cwd "." environment (GTAGSCONF nil GTAGSDBPATH nil GTAGSLABEL nil GTAGSLIBPATH nil GTAGSROOT "[ROOT]/" LC_ALL "C.UTF-8") fixture "initial" index 0 kind "CALL" program "gtags" status 0) (args ("--result=grep" "add_values") cwd "." environment (GTAGSCONF nil GTAGSDBPATH nil GTAGSLABEL nil GTAGSLIBPATH nil GTAGSROOT "[ROOT]/" LC_ALL "C.UTF-8") fixture "initial" index 1 kind "CALL" program "global" status 0))) :cleanup (:fixture-accounted t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_definition_reference_pattern_and_file_navigation() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_definition_reference_pattern_and_file_navigation",
        format!(
            r####"
(hgt397-test-run
 {script:?}
 (list
  (hgt397-test-record "global" '("--result=grep" "add_values")
                       "src/math.c:2:int add_values(int left, int right) {{\n")
  (hgt397-test-record "global" '("--result=grep" "-r" "add_values")
                       (concat "include/math.h:1:int add_values(int left, int right);\n"
                               "src/math.c:13:  return add_values(2, 3);\n"))
  (hgt397-test-record "global" '("--result=grep" "-g" "compute_total")
                       (concat "include/math.h:2:int compute_total(void);\n"
                               "src/main.c:3:  return compute_total();\n"
                               "src/math.c:12:int compute_total(void) {{\n"))
  (hgt397-test-record "global" '("-Poa" "math.c") "[ROOT]/src/math.c\n"))
 t
 (lambda (root _fixture)
   (let ((origin (find-file-noselect (expand-file-name "src/main.c" root))))
     (set-window-buffer (selected-window) origin)
     (set-buffer origin)
     (list
      :definition
      (hgt397-test-run-ui '((:action "Open file" :candidate 0))
                           (lambda () (helm-gtags-find-tag "add_values")))
      :reference
      (hgt397-test-run-ui '((:action "Open file" :candidate 1))
                           (lambda () (helm-gtags-find-rtag "add_values")))
      :pattern
      (hgt397-test-run-ui '((:action "Open file" :candidate 1))
                           (lambda () (helm-gtags-find-pattern "compute_total")))
      :file
      (hgt397-test-run-ui '((:action "Open file" :candidate 0))
                           (lambda () (helm-gtags-find-files "math.c")))
      :context-count
      (length (plist-get (gethash helm-gtags--tag-location
                                  helm-gtags--context-stack)
                         :stack))))))
"####,
            script = REPLAY_SCRIPT,
        ),
        expect![[
            r#"OK (:provenance (:source "2655760b62ca548b8ba2a0da668c1da5a0a2e4bb4a37734ae4660eac2c6795f0" :global-version "global (GNU Global) 6.6.14" :global-sha "c3f48142bfc80a1c3e7c8f15f4b73bc01685ed70e9d6dfec8c0c451b9bb4f521" :gtags-sha "63579e2a4ba7afefce03a90c3e7f1dd2534b64c16687317971d87031ebbd8449" :fixture (("include/math.h" "14aec838294e0fa36ba4db62778a602cd92daa7e00c4b68841aa44b2b7a0da2f") ("src/main.c" "192cc45c9a6982491777dc03e0cc330a516ac25097d43a1bb682d4c6daa35cb4") ("src/math.c" "9571ff7cd9a64068e087718a1768aaccf3fa309d67c4344abce2ca615a4481ff"))) :result (:definition ((:sources ((:name "add_values in [ROOT]/" :raw "src/math.c:2:int add_values(int left, int right) {\n" :candidates ((:display "src/math.c:2:int add_values(int left, int right) {" :faces ((:range (0 10) :text "src/math.c" :face helm-gtags-file) (:range (11 12) :text "2" :face helm-gtags-lineno) (:range (17 27) :text "add_values" :face helm-gtags-match)) :real "src/math.c:2:int add_values(int left, int right) {")))) :action "Open file" :selected (:display "src/math.c:2:int add_values(int left, int right) {" :real "src/math.c:2:int add_values(int left, int right) {") :location (:file "src/math.c" :line 2 :column 0 :text "int add_values(int left, int right) {"))) :reference ((:sources ((:name "add_values in [ROOT]/" :raw "include/math.h:1:int add_values(int left, int right);\nsrc/math.c:13:  return add_values(2, 3);\n" :candidates ((:display "include/math.h:1:int add_values(int left, int right);" :faces ((:range (0 14) :text "include/math.h" :face helm-gtags-file) (:range (15 16) :text "1" :face helm-gtags-lineno) (:range (21 31) :text "add_values" :face helm-gtags-match)) :real "include/math.h:1:int add_values(int left, int right);") (:display "src/math.c:13:  return add_values(2, 3);" :faces ((:range (0 10) :text "src/math.c" :face helm-gtags-file) (:range (11 13) :text "13" :face helm-gtags-lineno) (:range (23 33) :text "add_values" :face helm-gtags-match)) :real "src/math.c:13:  return add_values(2, 3);")))) :action "Open file" :selected (:display "src/math.c:13:  return add_values(2, 3);" :real "src/math.c:13:  return add_values(2, 3);") :location (:file "src/math.c" :line 13 :column 2 :text "  return add_values(2, 3);"))) :pattern ((:sources ((:name "compute_total in [ROOT]/" :raw "include/math.h:2:int compute_total(void);\nsrc/main.c:3:  return compute_total();\nsrc/math.c:12:int compute_total(void) {\n" :candidates ((:display "include/math.h:2:int compute_total(void);" :faces ((:range (0 14) :text "include/math.h" :face helm-gtags-file) (:range (15 16) :text "2" :face helm-gtags-lineno) (:range (21 34) :text "compute_total" :face helm-gtags-match)) :real "include/math.h:2:int compute_total(void);") (:display "src/main.c:3:  return compute_total();" :faces ((:range (0 10) :text "src/main.c" :face helm-gtags-file) (:range (11 12) :text "3" :face helm-gtags-lineno) (:range (22 35) :text "compute_total" :face helm-gtags-match)) :real "src/main.c:3:  return compute_total();") (:display "src/math.c:12:int compute_total(void) {" :faces ((:range (0 10) :text "src/math.c" :face helm-gtags-file) (:range (11 13) :text "12" :face helm-gtags-lineno) (:range (18 31) :text "compute_total" :face helm-gtags-match)) :real "src/math.c:12:int compute_total(void) {")))) :action "Open file" :selected (:display "src/main.c:3:  return compute_total();" :real "src/main.c:3:  return compute_total();") :location (:file "src/main.c" :line 3 :column 2 :text "  return compute_total();"))) :file ((:sources ((:name "math.c in [ROOT]/" :raw "[ROOT]/src/math.c\n" :candidates ((:display "src/math.c" :faces nil :real "[ROOT]/src/math.c")))) :action "Open file" :selected (:display "src/math.c" :real "[ROOT]/src/math.c") :location (:file "src/math.c" :line 13 :column 2 :text "  return add_values(2, 3);"))) :context-count 4) :boundaries (:index 4 :planned 4 :misses nil :trace ((args ("--result=grep" "add_values") cwd "." environment (GTAGSCONF nil GTAGSDBPATH nil GTAGSLABEL nil GTAGSLIBPATH nil GTAGSROOT "[ROOT]/" LC_ALL "C.UTF-8") fixture "initial" index 0 kind "CALL" program "global" status 0) (args ("--result=grep" "-r" "add_values") cwd "." environment (GTAGSCONF nil GTAGSDBPATH nil GTAGSLABEL nil GTAGSLIBPATH nil GTAGSROOT "[ROOT]/" LC_ALL "C.UTF-8") fixture "initial" index 1 kind "CALL" program "global" status 0) (args ("--result=grep" "-g" "compute_total") cwd "." environment (GTAGSCONF nil GTAGSDBPATH nil GTAGSLABEL nil GTAGSLIBPATH nil GTAGSROOT "[ROOT]/" LC_ALL "C.UTF-8") fixture "initial" index 2 kind "CALL" program "global" status 0) (args ("-Poa" "math.c") cwd "." environment (GTAGSCONF nil GTAGSDBPATH nil GTAGSLABEL nil GTAGSLIBPATH nil GTAGSROOT "[ROOT]/" LC_ALL "C.UTF-8") fixture "initial" index 3 kind "CALL" program "global" status 0))) :cleanup (:fixture-accounted t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_parse_file_transforms_and_navigates_cscope_candidates() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_parse_file_transforms_and_navigates_cscope_candidates",
        format!(
            r####"
(hgt397-test-run
 {script:?}
 (list
  (hgt397-test-record "global"
                       '("--result=cscope" "-f" "[ROOT]/src/math.c")
                       (concat "src/math.c add_values 2 int add_values(int left, int right) {{\n"
                               "src/math.c compute_total 12 int compute_total(void) {{\n")
                       0 nil "initial" "src"))
 t
 (lambda (root _fixture)
   (let ((buffer (find-file-noselect (expand-file-name "src/math.c" root))))
     (set-window-buffer (selected-window) buffer)
     (set-buffer buffer)
     (goto-char (point-min))
     (hgt397-test-run-ui '((:action "Parse file" :candidate 1))
                          #'helm-gtags-parse-file))))
"####,
            script = REPLAY_SCRIPT,
        ),
        expect![[
            r#"OK (:provenance (:source "2655760b62ca548b8ba2a0da668c1da5a0a2e4bb4a37734ae4660eac2c6795f0" :global-version "global (GNU Global) 6.6.14" :global-sha "c3f48142bfc80a1c3e7c8f15f4b73bc01685ed70e9d6dfec8c0c451b9bb4f521" :gtags-sha "63579e2a4ba7afefce03a90c3e7f1dd2534b64c16687317971d87031ebbd8449" :fixture (("include/math.h" "14aec838294e0fa36ba4db62778a602cd92daa7e00c4b68841aa44b2b7a0da2f") ("src/main.c" "192cc45c9a6982491777dc03e0cc330a516ac25097d43a1bb682d4c6daa35cb4") ("src/math.c" "9571ff7cd9a64068e087718a1768aaccf3fa309d67c4344abce2ca615a4481ff"))) :result ((:sources ((:name "Parsed File: src/math.c" :raw "src/math.c add_values 2 int add_values(int left, int right) {\nsrc/math.c compute_total 12 int compute_total(void) {\n" :candidates ((:display "add_values                2     int add_values(int left, int right) {" :faces nil :real "src/math.c add_values 2 int add_values(int left, int right) {") (:display "compute_total             12    int compute_total(void) {" :faces nil :real "src/math.c compute_total 12 int compute_total(void) {")))) :action "Parse file" :selected (:display "compute_total             12    int compute_total(void) {" :real "src/math.c compute_total 12 int compute_total(void) {") :location (:file "src/math.c" :line 12 :column 0 :text "int compute_total(void) {"))) :boundaries (:index 1 :planned 1 :misses nil :trace ((args ("--result=cscope" "-f" "[ROOT]/src/math.c") cwd "src" environment (GTAGSCONF nil GTAGSDBPATH nil GTAGSLABEL nil GTAGSLIBPATH nil GTAGSROOT "[ROOT]/" LC_ALL "C.UTF-8") fixture "initial" index 0 kind "CALL" program "global" status 0))) :cleanup (:fixture-accounted t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn public_single_file_update_indexes_new_definition() -> ParityBatchCase {
    ParityBatchCase::value(
        "public_single_file_update_indexes_new_definition",
        format!(
            r####"
(hgt397-test-run
 {script:?}
 (list
  (hgt397-test-record "global" '("--single-update" "[ROOT]/src/math.c")
                       "" 0 nil "updated" "src")
  (hgt397-test-record "global" '("--result=grep" "multiply_values")
                       "src/math.c:16:int multiply_values(int left, int right) {{\n"
                       0 nil "updated"))
 t
 (lambda (root fixture)
   (let ((buffer (find-file-noselect (expand-file-name "src/math.c" root))))
     (set-window-buffer (selected-window) buffer)
     (set-buffer buffer)
     (goto-char (point-max))
     (insert "\nint multiply_values(int left, int right) {{\n"
             "  return left * right;\n}}\n")
     (save-buffer)
     (call-interactively #'helm-gtags-update-tags)
     (let ((processes (hgt397-test-wait-processes fixture 1)))
       (list :updated-digest (hgt397-test-file-sha256 buffer-file-name)
             :processes processes
             :helm
             (hgt397-test-run-ui
              '((:action "Open file" :candidate 0))
              (lambda () (helm-gtags-find-tag "multiply_values"))))))))
"####,
            script = REPLAY_SCRIPT,
        ),
        expect![[
            r#"OK (:provenance (:source "2655760b62ca548b8ba2a0da668c1da5a0a2e4bb4a37734ae4660eac2c6795f0" :global-version "global (GNU Global) 6.6.14" :global-sha "c3f48142bfc80a1c3e7c8f15f4b73bc01685ed70e9d6dfec8c0c451b9bb4f521" :gtags-sha "63579e2a4ba7afefce03a90c3e7f1dd2534b64c16687317971d87031ebbd8449" :fixture (("include/math.h" "14aec838294e0fa36ba4db62778a602cd92daa7e00c4b68841aa44b2b7a0da2f") ("src/main.c" "192cc45c9a6982491777dc03e0cc330a516ac25097d43a1bb682d4c6daa35cb4") ("src/math.c" "9571ff7cd9a64068e087718a1768aaccf3fa309d67c4344abce2ca615a4481ff"))) :result (:updated-digest "059fb6354bad039c193439f6ece42b32b082440409672b7afc0ca7a2e68f6b4b" :processes ((:name "helm-gtags-update-tag" :status exit :exit 0)) :helm ((:sources ((:name "multiply_values in [ROOT]/" :raw "src/math.c:16:int multiply_values(int left, int right) {\n" :candidates ((:display "src/math.c:16:int multiply_values(int left, int right) {" :faces ((:range (0 10) :text "src/math.c" :face helm-gtags-file) (:range (11 13) :text "16" :face helm-gtags-lineno) (:range (18 33) :text "multiply_values" :face helm-gtags-match)) :real "src/math.c:16:int multiply_values(int left, int right) {")))) :action "Open file" :selected (:display "src/math.c:16:int multiply_values(int left, int right) {" :real "src/math.c:16:int multiply_values(int left, int right) {") :location (:file "src/math.c" :line 16 :column 0 :text "int multiply_values(int left, int right) {")))) :boundaries (:index 2 :planned 2 :misses nil :trace ((args ("--single-update" "[ROOT]/src/math.c") cwd "src" environment (GTAGSCONF nil GTAGSDBPATH nil GTAGSLABEL nil GTAGSLIBPATH nil GTAGSROOT "[ROOT]/" LC_ALL "C.UTF-8") fixture "updated" index 0 kind "CALL" program "global" status 0) (args ("--result=grep" "multiply_values") cwd "." environment (GTAGSCONF nil GTAGSDBPATH nil GTAGSLABEL nil GTAGSLIBPATH nil GTAGSROOT "[ROOT]/" LC_ALL "C.UTF-8") fixture "updated" index 1 kind "CALL" program "global" status 0))) :cleanup (:fixture-accounted t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

fn missing_tag_failure_recovers_through_public_definition_search() -> ParityBatchCase {
    ParityBatchCase::value(
        "missing_tag_failure_recovers_through_public_definition_search",
        format!(
            r####"
(hgt397-test-run
 {script:?}
 (list
  (hgt397-test-record "global" '("--result=grep" "missing_界") "" 1)
  (hgt397-test-record "global" '("--result=grep" "add_values")
                       "src/math.c:2:int add_values(int left, int right) {{\n"))
 t
 (lambda (root _fixture)
   (let ((buffer (find-file-noselect (expand-file-name "src/main.c" root)))
         failure recovery)
     (set-window-buffer (selected-window) buffer)
     (set-buffer buffer)
     (setq failure
           (condition-case condition
               (progn
                 (hgt397-test-run-ui '((:action nil))
                                      (lambda ()
                                        (helm-gtags-find-tag "missing_界")))
                 :unexpected-success)
             (error (hgt397-test-condition condition root))))
     (setq recovery
           (hgt397-test-run-ui '((:action "Open file" :candidate 0))
                                (lambda ()
                                  (helm-gtags-find-tag "add_values"))))
     (list :failure failure :recovery recovery))))
"####,
            script = REPLAY_SCRIPT,
        ),
        expect![[
            r#"OK (:provenance (:source "2655760b62ca548b8ba2a0da668c1da5a0a2e4bb4a37734ae4660eac2c6795f0" :global-version "global (GNU Global) 6.6.14" :global-sha "c3f48142bfc80a1c3e7c8f15f4b73bc01685ed70e9d6dfec8c0c451b9bb4f521" :gtags-sha "63579e2a4ba7afefce03a90c3e7f1dd2534b64c16687317971d87031ebbd8449" :fixture (("include/math.h" "14aec838294e0fa36ba4db62778a602cd92daa7e00c4b68841aa44b2b7a0da2f") ("src/main.c" "192cc45c9a6982491777dc03e0cc330a516ac25097d43a1bb682d4c6daa35cb4") ("src/math.c" "9571ff7cd9a64068e087718a1768aaccf3fa309d67c4344abce2ca615a4481ff"))) :result (:failure (:type error :data ("missing_界: not found") :message "missing_界: not found") :recovery ((:sources ((:name "add_values in [ROOT]/" :raw "src/math.c:2:int add_values(int left, int right) {\n" :candidates ((:display "src/math.c:2:int add_values(int left, int right) {" :faces ((:range (0 10) :text "src/math.c" :face helm-gtags-file) (:range (11 12) :text "2" :face helm-gtags-lineno) (:range (17 27) :text "add_values" :face helm-gtags-match)) :real "src/math.c:2:int add_values(int left, int right) {")))) :action "Open file" :selected (:display "src/math.c:2:int add_values(int left, int right) {" :real "src/math.c:2:int add_values(int left, int right) {") :location (:file "src/math.c" :line 2 :column 0 :text "int add_values(int left, int right) {")))) :boundaries (:index 2 :planned 2 :misses nil :trace ((args ("--result=grep" "missing_界") cwd "." environment (GTAGSCONF nil GTAGSDBPATH nil GTAGSLABEL nil GTAGSLIBPATH nil GTAGSROOT "[ROOT]/" LC_ALL "C.UTF-8") fixture "initial" index 0 kind "CALL" program "global" status 1) (args ("--result=grep" "add_values") cwd "." environment (GTAGSCONF nil GTAGSDBPATH nil GTAGSLABEL nil GTAGSLIBPATH nil GTAGSROOT "[ROOT]/" LC_ALL "C.UTF-8") fixture "initial" index 1 kind "CALL" program "global" status 0))) :cleanup (:fixture-accounted t :new-buffers nil :new-processes 0 :new-timers 0 :new-frames 0 :root-exists nil :window-restored t :buffer-restored t :body-error nil :cleanup-errors nil))"#
        ]],
    )
}

#[test]
fn helm_gtags_package_batch() {
    let cases: Vec<ParityBatchCase> = vec![
        create_tags_mode_and_definition_navigation(),
        public_definition_reference_pattern_and_file_navigation(),
        public_parse_file_transforms_and_navigates_cscope_candidates(),
        public_single_file_update_indexes_new_definition(),
        missing_tag_failure_recovers_through_public_definition_search(),
    ];
    assert_oracle_batch_cases(oracle(), "helm-gtags-rank397", "Helm Gtags", &cases);
}
