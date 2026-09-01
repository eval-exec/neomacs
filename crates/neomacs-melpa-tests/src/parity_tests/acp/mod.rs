use std::time::Duration;

use crate::{ACP_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ACP_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// acp.el speaks the Agent Client Protocol: line-delimited JSON-RPC over a
/// subprocess' stdin and stdout.  The package needs no external program of its
/// own -- the agent is whatever command the caller hands to `acp-make-client`
/// -- so these workflows write a real ACP agent into the sandbox and drive the
/// real thing: a real subprocess, real JSON on the wire in both directions,
/// real request-id correlation, real streamed notifications, real
/// agent-to-client requests and real stderr.  Nothing inside acp.el is stubbed.
const ACP_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

;; acp.el speaks the Agent Client Protocol -- line-delimited JSON-RPC over a
;; subprocess' stdin/stdout.  The package depends on no external program: the
;; agent is whatever command the caller passes to `acp-make-client'.  So these
;; workflows write a real ACP agent into the sandbox and drive the real thing:
;; a real subprocess, real JSON on the wire in both directions, real request-id
;; correlation, real streamed notifications, real agent-to-client requests and
;; real stderr.  Nothing inside acp.el is stubbed.

(setq make-backup-files nil
      create-lockfiles nil)

(defvar acp-test-root
  (file-name-as-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")))

(defun acp-test-write (path text)
  (make-directory (file-name-directory path) t)
  (with-temp-buffer
    (insert text)
    (write-region (point-min) (point-max) path nil 'silent))
  path)

;; --- the agent ------------------------------------------------------------
;; A real ACP agent: reads one JSON-RPC message per line, records it, and
;; answers per the spec.  `session/prompt' streams two session/update
;; notifications before its result, the way a real agent does.

(defconst acp-test-agent-script
  "#!/bin/sh
log=\"$ACP_TEST_DIR/agent-in.log\"
say() { printf '%s\\n' \"$1\"; }
while IFS= read -r line; do
  printf '%s\\n' \"$line\" >> \"$log\"
  id=$(printf '%s' \"$line\" | sed -n 's/.*\"id\":\\([0-9][0-9]*\\).*/\\1/p')
  method=$(printf '%s' \"$line\" | sed -n 's/.*\"method\":\"\\([^\"]*\\)\".*/\\1/p')
  case \"$method\" in
    initialize)
      say \"{\\\"jsonrpc\\\":\\\"2.0\\\",\\\"id\\\":$id,\\\"result\\\":{\\\"protocolVersion\\\":1,\\\"agentCapabilities\\\":{\\\"loadSession\\\":true,\\\"promptCapabilities\\\":{\\\"embeddedContext\\\":true}},\\\"authMethods\\\":[{\\\"id\\\":\\\"api-key\\\",\\\"name\\\":\\\"API key\\\"}]}}\" ;;
    session/new)
      say \"{\\\"jsonrpc\\\":\\\"2.0\\\",\\\"id\\\":$id,\\\"result\\\":{\\\"sessionId\\\":\\\"sess-42\\\",\\\"modes\\\":{\\\"currentModeId\\\":\\\"ask\\\",\\\"availableModes\\\":[{\\\"id\\\":\\\"ask\\\",\\\"name\\\":\\\"Ask\\\"},{\\\"id\\\":\\\"code\\\",\\\"name\\\":\\\"Code\\\"}]}}}\" ;;
    session/prompt)
      case \"$line\" in
        *PERMISSION*)
          say \"{\\\"jsonrpc\\\":\\\"2.0\\\",\\\"id\\\":9001,\\\"method\\\":\\\"session/request_permission\\\",\\\"params\\\":{\\\"sessionId\\\":\\\"sess-42\\\",\\\"toolCall\\\":{\\\"toolCallId\\\":\\\"call-1\\\",\\\"title\\\":\\\"Write README.md\\\"},\\\"options\\\":[{\\\"optionId\\\":\\\"allow\\\",\\\"name\\\":\\\"Allow\\\",\\\"kind\\\":\\\"allow_once\\\"},{\\\"optionId\\\":\\\"reject\\\",\\\"name\\\":\\\"Reject\\\",\\\"kind\\\":\\\"reject_once\\\"}]}}\"
          IFS= read -r answer
          printf '%s\\n' \"$answer\" >> \"$log\"
          chosen=$(printf '%s' \"$answer\" | sed -n 's/.*\"optionId\":\"\\([^\"]*\\)\".*/\\1/p')
          say \"{\\\"jsonrpc\\\":\\\"2.0\\\",\\\"id\\\":$id,\\\"result\\\":{\\\"stopReason\\\":\\\"end_turn\\\",\\\"granted\\\":\\\"$chosen\\\"}}\" ;;
        *BOOM*)
          say \"{\\\"jsonrpc\\\":\\\"2.0\\\",\\\"id\\\":$id,\\\"error\\\":{\\\"code\\\":-32601,\\\"message\\\":\\\"Method not found\\\",\\\"data\\\":{\\\"method\\\":\\\"session/prompt\\\"}}}\" ;;
        *RETRY*)
          cat \"$ACP_TEST_DIR/stderr-retry.txt\" >&2
          say \"{\\\"jsonrpc\\\":\\\"2.0\\\",\\\"id\\\":$id,\\\"result\\\":{\\\"stopReason\\\":\\\"refusal\\\"}}\" ;;
        *STDERR*)
          cat \"$ACP_TEST_DIR/stderr-plain.txt\" >&2
          say \"{\\\"jsonrpc\\\":\\\"2.0\\\",\\\"id\\\":$id,\\\"result\\\":{\\\"stopReason\\\":\\\"refusal\\\"}}\" ;;
        *DIE*)
          exit 3 ;;
        *)
          say \"{\\\"jsonrpc\\\":\\\"2.0\\\",\\\"method\\\":\\\"session/update\\\",\\\"params\\\":{\\\"sessionId\\\":\\\"sess-42\\\",\\\"update\\\":{\\\"sessionUpdate\\\":\\\"agent_message_chunk\\\",\\\"content\\\":{\\\"type\\\":\\\"text\\\",\\\"text\\\":\\\"Gr\\\\u00fc\\\\u00dfe! \\\"}}}}\"
          say \"{\\\"jsonrpc\\\":\\\"2.0\\\",\\\"method\\\":\\\"session/update\\\",\\\"params\\\":{\\\"sessionId\\\":\\\"sess-42\\\",\\\"update\\\":{\\\"sessionUpdate\\\":\\\"agent_message_chunk\\\",\\\"content\\\":{\\\"type\\\":\\\"text\\\",\\\"text\\\":\\\"Fertig.\\\"}}}}\"
          say \"{\\\"jsonrpc\\\":\\\"2.0\\\",\\\"id\\\":$id,\\\"result\\\":{\\\"stopReason\\\":\\\"end_turn\\\"}}\" ;;
      esac ;;
    session/die)
      exit 3 ;;
  esac
done
")

(defun acp-test-install-agent ()
  "Write the ACP agent into the sandbox and return its absolute path."
  (let ((path (expand-file-name "bin/acp-test-agent" acp-test-root)))
    (acp-test-write path acp-test-agent-script)
    (set-file-modes path #o755)
    (acp-test-write
     (expand-file-name "stderr-retry.txt" acp-test-root)
     (concat "Attempt 1 failed with status 429. Retrying with backoff. "
             "ApiError: {\"error\":{\"message\":"
             "\"{\\\"error\\\":{\\\"type\\\":\\\"rate_limit_error\\\","
             "\\\"message\\\":\\\"Quota exceeded\\\"}}\"}}\n"))
    (acp-test-write
     (expand-file-name "stderr-plain.txt" acp-test-root)
     "agent: could not reach api.example.test\n")
    (setenv "ACP_TEST_DIR" (directory-file-name acp-test-root))
    (setenv "PATH" (concat (expand-file-name "bin" acp-test-root)
                           path-separator (getenv "PATH")))
    (push (expand-file-name "bin" acp-test-root) exec-path)
    path))

(defun acp-test-agent-received ()
  "Every JSON line the agent read, oldest first."
  (let ((path (expand-file-name "agent-in.log" acp-test-root)))
    (if (file-regular-p path)
        (with-temp-buffer
          (let ((coding-system-for-read 'utf-8))
            (insert-file-contents path))
          (split-string (buffer-string) "\n" t))
      nil)))

(defun acp-test-wait-until (predicate &optional seconds)
  "Pump process output and timers until PREDICATE holds or SECONDS elapse."
  (let ((deadline (+ (float-time) (or seconds 5))))
    (while (and (not (funcall predicate)) (< (float-time) deadline))
      (accept-process-output nil 0.02)
      (sit-for 0.01))
    (funcall predicate)))

(defmacro acp-test-with-client (varlist &rest body)
  "Run BODY with a client bound per VARLIST, shutting it down afterwards."
  `(let* ((agent (acp-test-install-agent))
          ,@varlist)
     (unwind-protect
         (progn ,@body)
       (ignore-errors (acp-shutdown :client client))
       (dolist (process (process-list))
         (set-process-query-on-exit-flag process nil)
         (delete-process process)))))

(defun acp-test-buffer-text (name)
  (let ((buffer (get-buffer name)))
    (if buffer
        (with-current-buffer buffer
          (buffer-substring-no-properties (point-min) (point-max)))
      'no-such-buffer)))

(defun acp-test-traffic-lines (client)
  "Rendered traffic lines with the wall-clock timestamp normalised."
  (let ((buffer (acp-traffic-buffer :client client)))
    (with-current-buffer buffer
      (mapcar (lambda (line)
                (replace-regexp-in-string "\\`[0-9][0-9:.]+ " "TIME " line))
              (split-string (buffer-substring-no-properties (point-min) (point-max))
                            "\n" t)))))
"##;

fn acp_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ACP_MELPA_PIN, "acp.el")
        .expect("prepare pinned acp source below ./tmp")
        .with_prelude(ACP_TEST_PRELUDE)
        .with_timeout(ACP_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread.name().unwrap_or("unnamed acp parity test").into()
}

/// Multi-probe batch for `assert_acp_parity` cases (2a).
pub(crate) fn assert_acp_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(acp_oracle(), &name, "acp_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn acp_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_acp_batch(&cases);
}

// END generated package batch tests
