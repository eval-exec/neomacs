use expect_test::expect;

use super::ParityBatchCase;

/// The protocol's primary session: initialize, open a session, prompt, and read
/// the agent's streamed answer.  Everything the client puts on the wire is
/// pinned from the agent's side, so the JSON-RPC envelope, the monotonic
/// request ids and the capabilities the constructors derive are all covered.
fn a_real_agent_handshake_streams_updates_and_completes_the_prompt() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_real_agent_handshake_streams_updates_and_completes_the_prompt",
        r##"(acp-test-with-client
 ((client (acp-make-client :command agent)))
 (let (updates)
   (acp-subscribe-to-notifications
    :client client
    :on-notification (lambda (n) (push n updates)))
   (let* ((init (acp-send-request
                 :client client
                 :request (acp-make-initialize-request
                           :protocol-version 1
                           :client-info '((name . "neomacs-parity") (version . "1.0")))
                 :sync t))
          (session (acp-send-request
                    :client client
                    :request (acp-make-session-new-request :cwd "/work/project")
                    :sync t))
          (prompt (acp-send-request
                   :client client
                   :request (acp-make-session-prompt-request
                             :session-id "sess-42"
                             :prompt [((type . "text") (text . "Sag Grüße"))])
                   :sync t)))
     (acp-test-wait-until (lambda () (= (length updates) 2)))
     (list :init init :session session :prompt prompt
           :updates (reverse updates)
           :received (acp-test-agent-received)))))"##,
        expect![[
            r#"OK (:init ((protocolVersion . 1) (agentCapabilities (loadSession . t) (promptCapabilities (embeddedContext . t))) (authMethods . [((id . "api-key") (name . "API key"))])) :session ((sessionId . "sess-42") (modes (currentModeId . "ask") (availableModes . [((id . "ask") (name . "Ask")) ((id . "code") (name . "Code"))]))) :prompt ((stopReason . "end_turn")) :updates (((jsonrpc . "2.0") (method . "session/update") (params (sessionId . "sess-42") (update (sessionUpdate . "agent_message_chunk") (content (type . "text") (text . "Grüße! "))))) ((jsonrpc . "2.0") (method . "session/update") (params (sessionId . "sess-42") (update (sessionUpdate . "agent_message_chunk") (content (type . "text") (text . "Fertig.")))))) :received ("{\"jsonrpc\":\"2.0\",\"method\":\"initialize\",\"id\":1,\"params\":{\"clientInfo\":{\"name\":\"neomacs-parity\",\"version\":\"1.0\"},\"protocolVersion\":1,\"clientCapabilities\":{\"fs\":{\"readTextFile\":false,\"writeTextFile\":false}}}}" "{\"jsonrpc\":\"2.0\",\"method\":\"session/new\",\"id\":2,\"params\":{\"cwd\":\"/work/project\",\"mcpServers\":[]}}" "{\"jsonrpc\":\"2.0\",\"method\":\"session/prompt\",\"id\":3,\"params\":{\"sessionId\":\"sess-42\",\"prompt\":[{\"type\":\"text\",\"text\":\"Sag Grüße\"}]}}"))"#
        ]],
    )
}

fn the_agent_asks_the_client_for_permission_and_gets_an_answer() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_agent_asks_the_client_for_permission_and_gets_an_answer",
        r##"(acp-test-with-client
 ((client (acp-make-client :command agent)))
 (let (seen-requests)
   (acp-subscribe-to-requests
    :client client
    :on-request
    (lambda (request)
      (push request seen-requests)
      (acp-send-response
       :client client
       :response (acp-make-session-request-permission-response
                  :request-id (map-elt request 'id)
                  :option-id "allow"))))
   (let ((prompt (acp-send-request
                  :client client
                  :request (acp-make-session-prompt-request
                            :session-id "sess-42"
                            :prompt [((type . "text") (text . "PERMISSION please write"))])
                  :sync t)))
     (list :prompt prompt
           :requests (reverse seen-requests)
           :received (acp-test-agent-received)))))"##,
        expect![[
            r#"OK (:prompt ((stopReason . "end_turn") (granted . "allow")) :requests (((jsonrpc . "2.0") (id . 9001) (method . "session/request_permission") (params (sessionId . "sess-42") (toolCall (toolCallId . "call-1") (title . "Write README.md")) (options . [((optionId . "allow") (name . "Allow") (kind . "allow_once")) ((optionId . "reject") (name . "Reject") (kind . "reject_once"))])))) :received ("{\"jsonrpc\":\"2.0\",\"method\":\"session/prompt\",\"id\":1,\"params\":{\"sessionId\":\"sess-42\",\"prompt\":[{\"type\":\"text\",\"text\":\"PERMISSION please write\"}]}}" "{\"jsonrpc\":\"2.0\",\"id\":9001,\"result\":{\"outcome\":{\"outcome\":\"selected\",\"optionId\":\"allow\"}}}"))"#
        ]],
    )
    .fresh_process()
}

fn a_json_rpc_error_reaches_the_failure_callback_and_signals_when_sync() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_json_rpc_error_reaches_the_failure_callback_and_signals_when_sync",
        r##"(acp-test-with-client
 ((client (acp-make-client :command agent)))
 (let (failures)
   (acp-send-request
    :client client
    :request (acp-make-session-prompt-request
              :session-id "sess-42" :prompt [((type . "text") (text . "BOOM"))])
    :on-success (lambda (r) (push (list 'unexpected r) failures))
    :on-failure (lambda (e) (push e failures)))
   (acp-test-wait-until (lambda () failures))
   (let ((sync-error
          (condition-case error
              (acp-send-request
               :client client
               :request (acp-make-session-prompt-request
                         :session-id "sess-42" :prompt [((type . "text") (text . "BOOM again"))])
               :sync t)
            (error (list (car error) (cadr error))))))
     (list :failures (reverse failures)
           :sync-error sync-error
           :pending (map-elt client :pending-requests)))))"##,
        expect![[
            r#"OK (:failures (((code . -32601) (message . "Method not found") (data (method . "session/prompt")))) :sync-error (error "ACP request failed: ((code . -32601) (message . Method not found) (data (method . session/prompt)))") :pending nil)"#
        ]],
    )
}

fn agent_stderr_becomes_a_parsed_api_error_or_a_generic_internal_error() -> ParityBatchCase {
    ParityBatchCase::value(
        "agent_stderr_becomes_a_parsed_api_error_or_a_generic_internal_error",
        r##"(acp-test-with-client
 ((client (acp-make-client :command agent)))
 (let (agent-errors)
   (acp-subscribe-to-errors :client client :on-error (lambda (e) (push e agent-errors)))
   (acp-send-request
    :client client
    :request (acp-make-session-prompt-request
              :session-id "sess-42" :prompt [((type . "text") (text . "RETRY please"))])
    :sync t)
   (acp-test-wait-until (lambda () agent-errors))
   (let ((parsed (reverse agent-errors)))
     (setq agent-errors nil)
     (acp-send-request
      :client client
      :request (acp-make-session-prompt-request
                :session-id "sess-42" :prompt [((type . "text") (text . "STDERR please"))])
      :sync t)
     (acp-test-wait-until (lambda () agent-errors))
     (list :parsed parsed :generic (reverse agent-errors)))))"##,
        expect![[
            r#"OK (:parsed (((type . "rate_limit_error") (message . "Quota exceeded"))) :generic (((code . -32603) (message . "agent: could not reach api.example.test\n"))))"#
        ]],
    )
}

fn an_agent_that_dies_mid_request_fails_every_pending_request() -> ParityBatchCase {
    ParityBatchCase::value(
        "an_agent_that_dies_mid_request_fails_every_pending_request",
        r##"(acp-test-with-client
 ((client (acp-make-client :command agent)))
 (let (failures)
   (acp-send-request
    :client client
    :request (acp-make-session-prompt-request
              :session-id "sess-42" :prompt [((type . "text") (text . "DIE now"))])
    :on-failure (lambda (e) (push e failures)))
   (acp-test-wait-until (lambda () failures))
   (list :failures (reverse failures)
         :pending (map-elt client :pending-requests)
         :live (and (process-live-p (map-elt client :process)) t))))"##,
        expect![[
            r#"OK (:failures (((code . -32603) (message . "Agent process ended before completing request: exited abnormally with code 3"))) :pending nil :live nil)"#
        ]],
    )
}

fn logging_renders_the_whole_session_in_the_traffic_buffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "logging_renders_the_whole_session_in_the_traffic_buffer",
        r##"(let ((acp-logging-enabled t))
  (acp-test-with-client
   ((client (acp-make-client :command agent)))
   (acp-send-request :client client
                     :request (acp-make-initialize-request :protocol-version 1)
                     :sync t)
   (acp-send-request :client client
                     :request (acp-make-session-prompt-request
                               :session-id "sess-42"
                               :prompt [((type . "text") (text . "Hallo"))])
                     :sync t)
   (acp-test-wait-until (lambda () (>= (length (acp-test-traffic-lines client)) 6)))
   (list :traffic (acp-test-traffic-lines client)
         :buffers (list (buffer-name (acp-logs-buffer :client client))
                        (buffer-name (acp-traffic-buffer :client client)))
         :read-only (list (with-current-buffer (acp-logs-buffer :client client)
                            buffer-read-only)
                          (with-current-buffer (acp-traffic-buffer :client client)
                            buffer-read-only))
         :logs-has-outgoing (and (string-match-p
                                  "OUTGOING TEXT"
                                  (acp-test-buffer-text
                                   (buffer-name (acp-logs-buffer :client client))))
                                 t))))"##,
        expect![[
            r#"OK (:traffic ("TIME → request      initialize" "TIME ← response     result" "TIME → request      session/prompt" "TIME ← notification session/update" "TIME ← notification session/update" "TIME ← response     result") :buffers ("*acp-([ORACLE-SANDBOX]/bin/acp-test-agent)-1 log*" "*acp-([ORACLE-SANDBOX]/bin/acp-test-agent)-1 traffic*") :read-only (nil t) :logs-has-outgoing t)"#
        ]],
    )
    .fresh_process()
}

fn callbacks_run_in_the_context_buffer_and_shutdown_releases_everything() -> ParityBatchCase {
    ParityBatchCase::value(
        "callbacks_run_in_the_context_buffer_and_shutdown_releases_everything",
        r##"(let ((context (generate-new-buffer "*acp-context*")))
  (unwind-protect
      (acp-test-with-client
       ((client (acp-make-client :command agent :context-buffer context)))
       (let (callback-buffers)
         (acp-subscribe-to-notifications
          :client client
          :on-notification (lambda (_n) (push (buffer-name) callback-buffers)))
         (acp-send-request
          :client client
          :request (acp-make-session-prompt-request
                    :session-id "sess-42" :prompt [((type . "text") (text . "Hallo"))])
          :sync t)
         (acp-test-wait-until (lambda () (= (length callback-buffers) 2)))
         (acp-send-notification
          :client client
          :notification (acp-make-session-cancel-notification
                         :session-id "sess-42" :reason "user_cancelled"))
         (acp-test-wait-until (lambda () (= (length (acp-test-agent-received)) 2)))
         (let ((before (list (and (process-live-p (map-elt client :process)) t)
                             (buffer-live-p (acp-logs-buffer :client client))
                             (buffer-live-p (acp-traffic-buffer :client client))))
               (log-name (buffer-name (acp-logs-buffer :client client)))
               (traffic-name (buffer-name (acp-traffic-buffer :client client))))
           (acp-shutdown :client client)
           (let ((after (list (and (map-elt client :process)
                                   (process-live-p (map-elt client :process)) t)
                              (and (get-buffer log-name) t)
                              (and (get-buffer traffic-name) t))))
             (acp-shutdown :client client)
             (list :callback-buffers (reverse callback-buffers)
                   :received (acp-test-agent-received)
                   :before before :after after
                   :message (with-current-buffer "*Messages*"
                              (car (last (split-string (buffer-string) "\n" t)))))))))
    (kill-buffer context)))"##,
        expect![[
            r#"OK (:callback-buffers ("*acp-context*" "*acp-context*") :received ("{\"jsonrpc\":\"2.0\",\"method\":\"session/prompt\",\"id\":1,\"params\":{\"sessionId\":\"sess-42\",\"prompt\":[{\"type\":\"text\",\"text\":\"Hallo\"}]}}" "{\"jsonrpc\":\"2.0\",\"method\":\"session/cancel\",\"params\":{\"sessionId\":\"sess-42\",\"reason\":\"user_cancelled\"}}") :before (t t t) :after (nil nil nil) :message "Client already shut down")"#
        ]],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        a_real_agent_handshake_streams_updates_and_completes_the_prompt(),
        the_agent_asks_the_client_for_permission_and_gets_an_answer(),
        a_json_rpc_error_reaches_the_failure_callback_and_signals_when_sync(),
        agent_stderr_becomes_a_parsed_api_error_or_a_generic_internal_error(),
        an_agent_that_dies_mid_request_fails_every_pending_request(),
        logging_renders_the_whole_session_in_the_traffic_buffer(),
        callbacks_run_in_the_context_buffer_and_shutdown_releases_everything(),
    ]
}
