use std::time::Duration;

use crate::{ACTIVITY_WATCH_MODE_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ACTIVITY_WATCH_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// activity-watch-mode reports editing activity to a local ActivityWatch
/// server over HTTP, through request.el, which shells out to curl.  The
/// network is the one boundary these workflows fake: a recording `curl' is
/// installed on PATH and `exec-path', so request builds its real command,
/// writes its real config and JSON body to the process, and parses a real HTTP
/// response - the stand-in just records what would have gone over the wire and
/// answers 200 (or a status the workflow chooses).  Everything else is the
/// package: real files in the per-case sandbox, real hooks, real timers.
///
/// Two environment details are handled here rather than in each workflow.  The
/// mode refuses to turn on while `noninteractive' is non-nil - its own guard
/// against batch sessions - so `aw-test-interactive' makes the editor look
/// interactive for the toggle.  And `activity-watch-turn-on' defers its real
/// work by one second, binding its buffer-local hooks in whatever buffer is
/// current when that timer fires, so `aw-test-watching' keeps the buffer
/// current across the wait.
const ACTIVITY_WATCH_MODE_TEST_PRELUDE: &str = r##"(require 'cl-lib)

(defconst aw-test-curl-script "\
#!/bin/sh
if [ \"$1\" = --version ]; then
  echo 'curl 8.19.0 (x86_64-pc-linux-gnu) libcurl/8.19.0 zlib/1.3.2 libz'
  exit 0
fi
request=$(mktemp \"$AW_TEST_DIR/request-XXXXXX\")
cat > \"$request\"
url=$(sed -n 's/^--url //p' \"$request\" | head -1)
method=$(sed -n 's/^--request //p' \"$request\" | head -1)
headers=$(sed -n 's/^--header //p' \"$request\" | tr '\\n' '|')
body=$(awk 'found { print } /^--data-binary @-$/ { found = 1 }' \"$request\")
printf '%s\\t%s\\t%s\\t%s\\n' \"$method\" \"$url\" \"$headers\" \"$body\" >> \"$AW_TEST_LOG\"
status=${AW_TEST_STATUS:-200}
if [ \"$status\" = 200 ]; then
  payload='{\"status\":\"ok\"}'
  printf 'HTTP/1.1 200 OK\\r\\n'
else
  payload='{\"error\":\"bucket missing\"}'
  printf 'HTTP/1.1 %s Server Error\\r\\n' \"$status\"
fi
printf 'Content-Type: application/json\\r\\n'
printf 'Content-Length: %s\\r\\n' \"$(printf '%s' \"$payload\" | wc -c)\"
printf '\\r\\n'
printf '%s' \"$payload\"
printf '\\n(:num-redirects 0 :url-effective \"%s\")' \"$url\"
exit 0
")

(defun aw-test-path (name)
  (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defun aw-test-setup-server (&optional status)
  "Install a recording stand-in for curl, so request's real path runs."
  (let ((bin (aw-test-path "bin/curl")))
    (make-directory (file-name-directory bin) t)
    (with-temp-buffer
      (insert aw-test-curl-script)
      (write-region (point-min) (point-max) bin nil 'silent))
    (set-file-modes bin #o755)
    (setenv "AW_TEST_DIR" (aw-test-path "bin"))
    (setenv "AW_TEST_LOG" (aw-test-path "requests.log"))
    (setenv "AW_TEST_STATUS" (or status "200"))
    (setenv "PATH" (concat (aw-test-path "bin") path-separator (getenv "PATH")))
    (add-to-list 'exec-path (aw-test-path "bin"))
    bin))

(defun aw-test-requests ()
  "Return every recorded request as (METHOD URL HEADERS BODY), normalised.
The requests are sorted, because the bucket and heartbeat curl
processes run concurrently and finish in either order."
  (let ((log (aw-test-path "requests.log")))
    (if (not (file-exists-p log))
        'no-request
      (with-temp-buffer
        (insert-file-contents log)
        (sort
         (mapcar
          (lambda (line)
            (let ((fields (split-string line "\t")))
              (list (nth 0 fields)
                    (aw-test-normalize (nth 1 fields))
                    (split-string (or (nth 2 fields) "") "|" t)
                    (aw-test-normalize (nth 3 fields)))))
          (split-string (buffer-string) "\n" t))
         (lambda (a b) (string< (format "%S" a) (format "%S" b))))))))

(defun aw-test-normalize (text)
  "Replace the volatile parts of TEXT: host name and heartbeat timestamp."
  (when text
    (let ((text (replace-regexp-in-string
                 (regexp-quote (system-name)) "<HOST>" text t t)))
      (replace-regexp-in-string
       "\"timestamp\":\"[^\"]*\"" "\"timestamp\":\"<TIME>\"" text t t))))

(defun aw-test-drain ()
  "Wait until no subprocess is alive, then drain pending output."
  (let ((deadline (+ (float-time) 30)))
    (while (and (cl-some #'process-live-p (process-list))
                (< (float-time) deadline))
      (accept-process-output nil 0.05)))
  (while (accept-process-output nil 0.05)))

(defun aw-test-open (name text)
  "Write and visit a sandbox file, returning its buffer."
  (let ((path (aw-test-path name)))
    (make-directory (file-name-directory path) t)
    (with-temp-buffer
      (insert text)
      (write-region (point-min) (point-max) path nil 'silent))
    (find-file-noselect path)))

(defmacro aw-test-interactive (&rest body)
  "Run BODY with the editor looking interactive.
`activity-watch-mode' refuses to turn on when `noninteractive' is
non-nil, which is its own guard against batch sessions."
  `(let ((noninteractive nil))
     ,@body))

(defun aw-test-settle (seconds)
  "Let Emacs run timers and process output for SECONDS."
  (let ((deadline (+ (float-time) seconds)))
    (while (< (float-time) deadline)
      (accept-process-output nil 0.05))))

(defun aw-test-park-sampler ()
  "Occupy `activity-watch-timer' so the two second sampler never fires.
`activity-watch--start-timer' only creates its timer when that
variable is nil, so the recorded requests are exactly the ones the
workflow's own saves produced."
  (setq activity-watch-timer (run-at-time 3600 nil #'ignore)))

(defmacro aw-test-watching (buffer &rest body)
  "Turn `activity-watch-mode' on in BUFFER, then run BODY there.
`activity-watch-turn-on' defers its real work by one second and binds
its buffer-local hooks in whatever buffer is current when the timer
fires, so BUFFER stays current across the wait."
  `(with-current-buffer ,buffer
     (aw-test-interactive (activity-watch-mode 1))
     (aw-test-settle 1.3)
     ,@body))

(defun aw-test-forget-requests ()
  "Drop the recorded requests so the next step starts from an empty log."
  (let ((log (aw-test-path "requests.log")))
    (when (file-exists-p log)
      (delete-file log))))
"##;

fn activity_watch_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ACTIVITY_WATCH_MODE_MELPA_PIN, "activity-watch-mode.el")
        .expect("prepare pinned activity-watch-mode source below ./tmp")
        .with_prelude(ACTIVITY_WATCH_MODE_TEST_PRELUDE)
        .with_timeout(ACTIVITY_WATCH_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed activity-watch-mode parity test")
        .into()
}

/// Multi-probe batch for `assert_activity_watch_mode_parity` cases (2a).
pub(crate) fn assert_activity_watch_mode_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        activity_watch_mode_oracle(),
        &name,
        "activity_watch_mode_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn activity_watch_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_activity_watch_mode_batch(&cases);
}

// END generated package batch tests
