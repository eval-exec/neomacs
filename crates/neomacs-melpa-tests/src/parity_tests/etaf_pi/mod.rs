//! Behavioural parity for the pinned `etaf-pi` client's event projection.
//!
//! Pi's wire events are projected by a pure state reducer
//! (`etaf-pi-state-accept`), which is the same reducer the live RPC client
//! feeds.  These probes apply recorded and synthetic wire records and assert
//! the published phase, messages, errors, notices, queue and dialogs.  The
//! subprocess transport is not exercised here, so no Python fixture, network,
//! login or SQLite is involved.

use std::time::Duration;

use expect_test::expect;

use crate::CachedMelpaOracle;

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'etaf)
(require 'etaf-pi)

(defun neomacs-pi-text (content)
  "Project message CONTENT to one comparable string."
  (if (stringp content)
      content
    (mapconcat (lambda (block)
                 (or (plist-get block :text)
                     (plist-get block :thinking)
                     ""))
               content "|")))

(defun neomacs-pi-facts (state)
  "Return STATE's published facts compactly."
  (let ((info (etaf-pi-state-info state)))
    (list :phase (plist-get info :phase)
          :error (plist-get info :error)
          :notice (plist-get info :notice)
          :queue (plist-get info :queue)
          :dialogs (mapcar (lambda (dialog) (plist-get dialog :method))
                           (plist-get info :dialogs))
          :messages (mapcar (lambda (message)
                              (list (plist-get message :role)
                                    (neomacs-pi-text (plist-get message :content))
                                    (plist-get message :stopReason)))
                            (etaf-pi-state-messages state)))))

(defun neomacs-pi-accept (state &rest records)
  "Apply RECORDS to STATE in order and return STATE."
  (dolist (record records state)
    (etaf-pi-state-accept state record)))
"####;

fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(("etaf-pi", "0.1.1"), "etaf-pi.el")
        .expect("prepare pinned etaf-pi source below ./tmp")
        .with_timeout(Duration::from_secs(240))
        .with_prelude(PRELUDE)
}

#[test]
fn etaf_pi_behavior_batch() {
    let cases = vec![
        // A new projection starts disconnected and empty.
        ParityBatchCase::value(
            "initial_state",
            r####"
(neomacs-pi-facts (etaf-pi-state-create))
"####,
            expect![[
                r#"OK (:phase "连接中" :error nil :notice nil :queue 0 :dialogs nil :messages nil)"#
            ]],
        ),
        // Agent lifecycle changes the activity phase; the intermediate
        // agent_end is not the settled state.
        ParityBatchCase::value(
            "agent_lifecycle",
            r####"
(let ((state (etaf-pi-state-create)))
  (neomacs-pi-accept state '(:type "agent_start"))
  (let ((started (neomacs-pi-facts state)))
    (neomacs-pi-accept state '(:type "agent_end"))
    (let ((ended (neomacs-pi-facts state)))
      (neomacs-pi-accept state '(:type "agent_settled"))
      (list started ended (neomacs-pi-facts state)))))
"####,
            expect![[
                r#"OK ((:phase "回复中" :error nil :notice nil :queue 0 :dialogs nil :messages nil) (:phase "回复中" :error nil :notice nil :queue 0 :dialogs nil :messages nil) (:phase "就绪" :error nil :notice nil :queue 0 :dialogs nil :messages nil))"#
            ]],
        ),
        // Streaming deltas build a live draft; the final message replaces it.
        ParityBatchCase::value(
            "streaming_text",
            r####"
(let ((state (etaf-pi-state-create)))
  (neomacs-pi-accept
   state
   '(:type "agent_start")
   '(:type "message_start" :message (:role "assistant" :content []))
   '(:type "message_update"
     :assistantMessageEvent (:type "text_delta" :contentIndex 0 :delta "你"))
   '(:type "message_update"
     :assistantMessageEvent (:type "text_delta" :contentIndex 0 :delta "好")))
  (let ((drafting (neomacs-pi-facts state)))
    (neomacs-pi-accept
     state
     '(:type "message_end"
       :message (:role "assistant" :stopReason "stop"
                       :content [(:type "text" :text "你好")]))
     '(:type "agent_settled"))
    (list drafting (neomacs-pi-facts state))))
"####,
            expect![[
                r#"OK ((:phase "回复中" :error nil :notice nil :queue 0 :dialogs nil :messages (("assistant" "你好" "pending"))) (:phase "就绪" :error nil :notice nil :queue 0 :dialogs nil :messages (("assistant" "你好" "stop"))))"#
            ]],
        ),
        // Responses publish session phase facts and authoritative messages.
        ParityBatchCase::value(
            "authoritative_messages",
            r####"
(let ((state (etaf-pi-state-create)))
  (neomacs-pi-accept
   state
   '(:type "response" :command "get_state" :success t
     :data (:isStreaming t))
   '(:type "response" :command "get_messages" :success t
     :data (:messages ((:role "user" :content "你好")
                       (:role "assistant" :stopReason "stop"
                               :content "回答")))))
  (let ((streaming (neomacs-pi-facts state)))
    (neomacs-pi-accept
     state
     '(:type "response" :command "get_state" :success t
       :data (:isCompacting t))
     '(:type "response" :command "get_state" :success t
       :data ()))
    (list streaming (neomacs-pi-facts state))))
"####,
            expect![[
                r#"OK ((:phase "回复中" :error nil :notice nil :queue 0 :dialogs nil :messages (("user" "你好" nil) ("assistant" "回答" "stop"))) (:phase "就绪" :error nil :notice nil :queue 0 :dialogs nil :messages (("user" "你好" nil) ("assistant" "回答" "stop"))))"#
            ]],
        ),
        // Failures stay visible; a new agent start clears the error, and a
        // final aborted or failed message reports itself.
        ParityBatchCase::value(
            "failure_and_notice",
            r####"
(let ((state (etaf-pi-state-create)))
  (neomacs-pi-accept state '(:type "response" :success nil
                              :error "连接失败"))
  (let ((failed (neomacs-pi-facts state)))
    (neomacs-pi-accept state '(:type "agent_start"))
    (let ((recovered (neomacs-pi-facts state)))
      (neomacs-pi-accept
       state
       '(:type "message_end"
         :message (:role "assistant" :stopReason "error" :errorMessage "回复失败"
                         :content [])))
      (let ((errored (neomacs-pi-facts state)))
        (neomacs-pi-accept
         state
         '(:type "message_end"
           :message (:role "assistant" :stopReason "aborted" :content [])))
        (list failed recovered errored (neomacs-pi-facts state))))))
"####,
            expect![[
                r#"OK ((:phase "连接中" :error "连接失败" :notice nil :queue 0 :dialogs nil :messages nil) (:phase "回复中" :error nil :notice nil :queue 0 :dialogs nil :messages nil) (:phase "回复中" :error "回复失败" :notice nil :queue 0 :dialogs nil :messages (("assistant" "" "error"))) (:phase "回复中" :error "回复失败" :notice "已停止" :queue 0 :dialogs nil :messages (("assistant" "" "error") ("assistant" "" "aborted"))))"#
            ]],
        ),
        // Queue updates count steering and follow-up entries; UI requests
        // become dialogs, notices, and notices for status/title/widget.
        ParityBatchCase::value(
            "queue_and_dialog",
            r####"
(let ((state (etaf-pi-state-create)))
  (neomacs-pi-accept
   state
   '(:type "queue_update" :steering (a b) :followUp (c))
   '(:type "extension_ui_request" :method "select")
   '(:type "extension_ui_request" :method "notify" :message "你好")
   '(:type "extension_ui_request" :method "setStatus" :statusText "忙碌")
   '(:type "extension_ui_request" :method "setTitle" :title "标题"))
  (let ((with-dialog (neomacs-pi-facts state)))
    (neomacs-pi-accept
     state
     '(:type "extension_ui_request" :method "setWidget"
       :widgetLines ("一" "二"))
     '(:type "client_exit" :error "已断开"))
    (list with-dialog (neomacs-pi-facts state))))
"####,
            expect![[
                r#"OK ((:phase "连接中" :error nil :notice "标题" :queue 3 :dialogs ("select") :messages nil) (:phase "已断开" :error "已断开" :notice "一 · 二" :queue 3 :dialogs ("select") :messages nil))"#
            ]],
        ),
    ];
    assert_oracle_batch_cases(oracle(), "etaf-pi-behavior", "etaf-pi", &cases);
}
