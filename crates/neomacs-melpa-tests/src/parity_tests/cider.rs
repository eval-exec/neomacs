use std::time::Duration;

use expect_test::expect;

use crate::{CIDER_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const CIDER_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const CIDER_TEST_PRELUDE: &str = r###"
(require 'cl-lib)
(require 'queue)
(require 'nrepl-bencode)
(require 'nrepl-dict)
(require 'cider-completion-context)
(require 'cider-compilation)
(require 'cider-endpoint)
(require 'cider-inspector)
(require 'cider-repl)
(require 'cider-stacktrace)
(require 'cider-test)

(defun neomacs-cider-test-property-runs (property)
  "Return stable (VALUE START END TEXT) runs for PROPERTY in this buffer."
  (let ((position (point-min))
        runs)
    (while (< position (point-max))
      (let* ((value (get-text-property position property))
             (next (next-single-property-change
                    position property nil (point-max))))
        (when value
          (push (list (copy-tree value) position next
                      (buffer-substring-no-properties position next))
                runs))
        (setq position next)))
    (nreverse runs)))

(defun neomacs-cider-test-reset ()
  "Discard mutable package state shared by the practical parity workflows."
  (setq cider-repl-input-history nil
        cider-repl-input-history-position -1
        cider-repl-history-pattern nil
        cider-stacktrace-suppressed-errors nil)
  (dolist (name '(" *nrepl-decoding*" "*cider-error*"
                  "*cider-test-report*" " *neomacs-cider-inspector*"))
    (when-let ((buffer (get-buffer name)))
      (kill-buffer buffer))))
"###;

fn cider_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(CIDER_MELPA_PIN, "cider.el")
        .expect("prepare revision-pinned CIDER source closure below ./tmp")
        .with_prelude(CIDER_TEST_PRELUDE)
        .with_timeout(CIDER_TEST_TIMEOUT)
}

fn fragmented_nrepl_transport_decodes_and_aggregates_a_real_response() -> ParityBatchCase {
    let elisp_form = r###"
(progn
  (neomacs-cider-test-reset)
  (let* ((first (nrepl-dict
                 "id" "release-42"
                 "session" "session-a"
                 "out" "Deploying λ\n"
                 "status" '("eval")))
         (second (nrepl-dict
                  "id" "release-42"
                  "session" "session-a"
                  "out" "Ready\r\n"
                  "value" "{:release 42 :state :healthy}"
                  "status" '("done")))
         (wire (concat (nrepl-bencode first) (nrepl-bencode second)))
         (cuts '(7 23 41 67))
         (start 0)
         chunks
         (strings (queue-create))
         (responses (nrepl-response-queue))
         decoded
         merged)
    (dolist (end cuts)
      (push (substring wire start end) chunks)
      (setq start end))
    (push (substring wire start) chunks)
    (setq chunks (nreverse chunks))
    (dolist (chunk chunks)
      (queue-enqueue strings chunk)
      (nrepl-bdecode strings responses))
    (while (queue-head responses)
      (push (queue-dequeue responses) decoded))
    (setq decoded (nreverse decoded))
    (dolist (response decoded)
      (setq merged (nrepl--merge merged (copy-tree response))))
    (list :chunks (mapcar #'string-bytes chunks)
          :wire wire
          :decoded decoded
          :merged merged
          :reencoded (mapcar #'nrepl-bencode decoded)
          :pending-input (queue-head strings)
          :stub (nrepl-response-queue-stub responses))))
"###;
    let expected = expect![[
        r####"OK (:chunks (7 16 19 26 116) :wire "d2:id10:release-423:out13:Deploying λ\n7:session9:session-a6:statusl4:evaleed2:id10:release-423:out7:Ready\15\n7:session9:session-a6:statusl4:donee5:value29:{:release 42 :state :healthy}e" :decoded ((dict "id" "release-42" "out" "Deploying λ\n" "session" "session-a" "status" ("eval")) (dict "id" "release-42" "out" "Ready\n" "session" "session-a" "status" ("done") "value" "{:release 42 :state :healthy}")) :merged (dict "id" "release-42" "out" "Deploying λ\nReady\n" "session" "session-a" "status" ("eval" "done") "value" "{:release 42 :state :healthy}") :reencoded ("d2:id10:release-423:out13:Deploying λ\n7:session9:session-a6:statusl4:evalee" "d2:id10:release-423:out6:Ready\n7:session9:session-a6:statusl4:donee5:value29:{:release 42 :state :healthy}e") :pending-input nil :stub nil)"####
    ]];
    ParityBatchCase::value(
        "fragmented_nrepl_transport_decodes_and_aggregates_a_real_response",
        elisp_form,
        expected,
    )
}

fn completion_context_tracks_nested_clojure_edits_and_incomplete_input() -> ParityBatchCase {
    let elisp_form = r###"
(progn
  (neomacs-cider-test-reset)
  (with-temp-buffer
    (clojure-mode)
    (insert "(defn deploy-release [release]\n"
            "  (-> release\n"
            "      (assoc :status :ready)\n"
            "      (update :attempts inc)\n"
            "      (get-in [:metadata :owner])))\n\n"
            "(defn render-message [release]\n"
            "  (str \"release=\" (:id release)))\n")
    (let (contexts)
      (dolist (target '("assoc" ":attempts" "get-in" ":owner"))
        (goto-char (point-min))
        (search-forward target)
        (push (list target
                    :point (point)
                    :completion (cider-completion-get-context-at-point)
                    :info (cider-completion-get-info-context-at-point))
              contexts))
      (goto-char (point-min))
      (search-forward "release=")
      (let ((inside-string (cider-completion-get-context)))
        (goto-char (point-max))
        (insert "\n(defn unfinished [x]\n  (assoc x :state")
        (let ((unfinished (cider-completion-get-context)))
          (list :contexts (nreverse contexts)
                :inside-string inside-string
                :unfinished unfinished
                :point (point)
                :line (line-number-at-pos)))))))
"###;
    let expected = expect![[
        r####"OK (:contexts (("assoc" :point 58 :completion "(defn deploy-release [release]\n  (-> release\n      (__prefix__ :status :ready)\n      (update :attempts inc)\n      (get-in [:metadata :owner])))\n" :info "(defn deploy-release [release]\n  (-> release\n      (__prefix__ :status :ready)\n      (update :attempts inc)\n      (get-in [:metadata :owner])))") (":attempts" :point 98 :completion "(defn deploy-release [release]\n  (-> release\n      (assoc :status :ready)\n      (update __prefix__ inc)\n      (get-in [:metadata :owner])))\n" :info "(defn deploy-release [release]\n  (-> release\n      (assoc :status :ready)\n      (update __prefix__ inc)\n      (get-in [:metadata :owner])))") ("get-in" :point 117 :completion "(defn deploy-release [release]\n  (-> release\n      (assoc :status :ready)\n      (update :attempts inc)\n      (__prefix__ [:metadata :owner])))\n" :info "(defn deploy-release [release]\n  (-> release\n      (assoc :status :ready)\n      (update :attempts inc)\n      (__prefix__ [:metadata :owner])))") (":owner" :point 135 :completion "(defn deploy-release [release]\n  (-> release\n      (assoc :status :ready)\n      (update :attempts inc)\n      (get-in [:metadata __prefix__])))\n" :info "(defn deploy-release [release]\n  (-> release\n      (assoc :status :ready)\n      (update :attempts inc)\n      (get-in [:metadata __prefix__])))")) :inside-string "nil" :unfinished "nil" :point 245 :line 11)"####
    ]];
    ParityBatchCase::value(
        "completion_context_tracks_nested_clojure_edits_and_incomplete_input",
        elisp_form,
        expected,
    )
}

fn compiler_diagnostics_preserve_locations_severity_and_actionable_messages() -> ParityBatchCase {
    let elisp_form = r###"
(progn
  (neomacs-cider-test-reset)
  (let ((messages
         '("Reflection warning, src/release/core.clj:14:7 - call to method deploy can't be resolved."
           "Syntax error compiling at (src/release/core.clj:31:3). Unable to resolve symbol: artifact in this context."
           "Unexpected error (ClassCastException) macroexpanding defmulti at (src/release/policy.cljc:21:1). class A cannot be cast to class B (A is in unnamed module of loader 'app'; B is in module java.base of loader 'bootstrap')"
           "Syntax error reading source at (REPL:9). EOF while reading, starting at line 7"
           "Release completed successfully.")))
    (mapcar
     (lambda (message)
       (let ((info (cider-extract-error-info cider-compilation-regexp message)))
         (list :message message
               :matched (and info t)
               :file (nth 0 info)
               :line (nth 1 info)
               :column (nth 2 info)
               :face (nth 3 info)
               :short (cider--shorten-error-message message))))
     messages)))
"###;
    let expected = expect![[
        r####"OK ((:message "Reflection warning, src/release/core.clj:14:7 - call to method deploy can't be resolved." :matched t :file "src/release/core.clj" :line 14 :column 7 :face cider-warning-highlight-face :short "call to method deploy can't be resolved.") (:message "Syntax error compiling at (src/release/core.clj:31:3). Unable to resolve symbol: artifact in this context." :matched t :file "src/release/core.clj" :line 31 :column 3 :face cider-error-highlight-face :short "Unable to resolve symbol: artifact in this context.") (:message "Unexpected error (ClassCastException) macroexpanding defmulti at (src/release/policy.cljc:21:1). class A cannot be cast to class B (A is in unnamed module of loader 'app'; B is in module java.base of loader 'bootstrap')" :matched t :file "src/release/policy.cljc" :line 21 :column 1 :face cider-error-highlight-face :short "class A cannot be cast to class B") (:message "Syntax error reading source at (REPL:9). EOF while reading, starting at line 7" :matched t :file nil :line 9 :column nil :face cider-error-highlight-face :short "EOF while reading, starting at line 7") (:message "Release completed successfully." :matched nil :file nil :line nil :column nil :face nil :short "Release completed successfully."))"####
    ]];
    ParityBatchCase::value(
        "compiler_diagnostics_preserve_locations_severity_and_actionable_messages",
        elisp_form,
        expected,
    )
}

fn inspector_renders_nested_values_and_cycles_through_actionable_regions() -> ParityBatchCase {
    let elisp_form = r###"
(progn
  (neomacs-cider-test-reset)
  (with-current-buffer (get-buffer-create " *neomacs-cider-inspector*")
    (erase-buffer)
    (let ((cider-inspector-skip-uninteresting t)
          (cider-inspector-uninteresting-regexp
           "\\(?:nil\\|true\\|false\\)\\_>"))
      (cider-inspector-render*
       '("Deployment" ": " (:value "{:release \"REL-42\"}" 0)
         (:newline)
         "  owner = " (:value "\"ops@example.test\"" 1)
         (:newline)
         "  healthy = " (:value "true" 2)
         (:newline)
         "  stages = " (:value "[:build :canary :stable]" 3)
         (:newline)
         "  metadata = " (:value "{:attempt 3 :region :east}" 4)))
      (let ((text (buffer-substring-no-properties (point-min) (point-max)))
            (runs (neomacs-cider-test-property-runs 'cider-value-idx))
            visited)
        (goto-char (point-min))
        (dotimes (_ 6)
          (cider-inspector-next-inspectable-object 1)
          (push (list :point (point)
                      :index (get-text-property (point) 'cider-value-idx)
                      :value (buffer-substring-no-properties
                              (point)
                              (or (next-single-property-change
                                   (point) 'cider-value-idx nil (point-max))
                                  (point-max))))
                visited))
        (list :text text
              :runs runs
              :visited (nreverse visited)
              :property-at-end
              (progn
                (goto-char (point-max))
                (cider-inspector-property-at-point)))))))
"###;
    let expected = expect![[
        r####"OK (:text "Deployment: {:release \"REL-42\"}\n  owner = \"ops@example.test\"\n  healthy = true\n  stages = [:build :canary :stable]\n  metadata = {:attempt 3 :region :east}" :runs ((0 13 32 "{:release \"REL-42\"}") (1 43 61 "\"ops@example.test\"") (2 74 78 "true") (3 90 114 "[:build :canary :stable]") (4 128 154 "{:attempt 3 :region :east}")) :visited ((:point 13 :index 0 :value "{:release \"REL-42\"}") (:point 43 :index 1 :value "\"ops@example.test\"") (:point 90 :index 3 :value "[:build :canary :stable]") (:point 128 :index 4 :value "{:attempt 3 :region :east}") (:point 13 :index 0 :value "{:release \"REL-42\"}") (:point 43 :index 1 :value "\"ops@example.test\"")) :property-at-end (cider-value-idx 4))"####
    ]];
    ParityBatchCase::value(
        "inspector_renders_nested_values_and_cycles_through_actionable_regions",
        elisp_form,
        expected,
    )
}

fn test_report_renders_failures_errors_diffs_and_navigation_metadata() -> ParityBatchCase {
    let elisp_form = r###"
(progn
  (neomacs-cider-test-reset)
  (let* ((summary (nrepl-dict "ns" 1 "var" 2 "test" 4
                              "pass" 2 "fail" 1 "error" 1))
         (results
          (nrepl-dict
           "release.core-test"
           (nrepl-dict
            "deploys-release"
            (list
             (nrepl-dict "var" "deploys-release" "type" "pass")
             (nrepl-dict
              "var" "deploys-release" "type" "fail"
              "context" "canary promotion"
              "message" "promotes only after health checks"
              "expected" "{:state :healthy\\n :replicas 3}"
              "actual" "{:state :degraded\\n :replicas 2}"
              "diffs" '(("{:state :degraded :replicas 2}"
                         ("{:state :healthy :replicas 3}"
                          "{:state :degraded :replicas 2}")))))
            "rolls-back-release"
            (list
             (nrepl-dict
              "var" "rolls-back-release" "type" "error"
              "context" "stable rollback"
              "message" "returns the previous artifact"
              "error" "java.lang.IllegalStateException: missing artifact"
              "gen-input" "{:release \"REL-42\" :attempt 3}")))))
         (buffer (get-buffer-create "*cider-test-report*")))
    (unwind-protect
        (progn
          (with-current-buffer buffer
            (let ((cider-test-fail-fast t)
                  (cider-test-items-background-color "gray20"))
              (cider-test-render-report
               buffer summary results
               (nrepl-dict "ms" 128)
               (nrepl-dict "release.core-test" (nrepl-dict "ms" 120))
               (nrepl-dict
                "release.core-test"
                (nrepl-dict
                 "deploys-release"
                 (nrepl-dict "elapsed-time" (nrepl-dict "ms" 70))
                 "rolls-back-release"
                 (nrepl-dict "elapsed-time" (nrepl-dict "ms" 50))))))
            (list :text (buffer-substring-no-properties (point-min) (point-max))
                  :types (neomacs-cider-test-property-runs 'type)
                  :namespaces (neomacs-cider-test-property-runs 'ns)
                  :variables (neomacs-cider-test-property-runs 'var)
                  :expected (neomacs-cider-test-property-runs 'expected)
                  :actual (neomacs-cider-test-property-runs 'actual)
                  :overlays
                  (mapcar (lambda (overlay)
                            (list (overlay-start overlay)
                                  (overlay-end overlay)))
                          (sort (overlays-in (point-min) (point-max))
                                (lambda (left right)
                                  (< (overlay-start left)
                                     (overlay-start right))))))))
      (when (buffer-live-p buffer)
        (kill-buffer buffer)))))
"###;
    let expected = expect![[
        r####"OK (:text "Test Summary\nrelease.core-test 120 ms\n  deploys-release 70 ms\n  rolls-back-release 50 ms\n\nTested 1 namespaces in 128 ms\nRan 4 assertions, in 2 test functions\n1 failures\n1 errors\ncider-test-fail-fast: t\n\n\nResults\n\nrelease.core-test\n1 non-passing tests:\n\nFail in deploys-release\ncanary promotion\npromotes only after health checks\n\n\n\n\nrelease.core-test\n1 non-passing tests:\n\nError in rolls-back-release\nstable rollback\nreturns the previous artifact\n   error: java.lang.IllegalStateException: missing artifact\n   input: {:release \"REL-42\" :attempt 3}\n          + {:state :degraded :replicas 2}    diff: - {:state :healthy :replicas 3}  actual: {:state :degraded :replicas 2}expected: {:state :healthy\n :replicas 3}" :types (("fail" 254 333 "Fail in deploys-release\ncanary promotion\npromotes only after health checks\n\n\n\n\n") ("error" 373 548 "Error in rolls-back-release\nstable rollback\nreturns the previous artifact\n   error: java.lang.IllegalStateException: missing artifact\n   input: {:release \"REL-42\" :attempt 3}\n")) :namespaces nil :variables (("deploys-release" 254 333 "Fail in deploys-release\ncanary promotion\npromotes only after health checks\n\n\n\n\n") ("rolls-back-release" 373 548 "Error in rolls-back-release\nstable rollback\nreturns the previous artifact\n   error: java.lang.IllegalStateException: missing artifact\n   input: {:release \"REL-42\" :attempt 3}\n")) :expected (("{:state :healthy\\n :replicas 3}" 254 333 "Fail in deploys-release\ncanary promotion\npromotes only after health checks\n\n\n\n\n")) :actual (("{:state :degraded\\n :replicas 2}" 254 333 "Fail in deploys-release\ncanary promotion\npromotes only after health checks\n\n\n\n\n")) :overlays ((254 332) (373 547)))"####
    ]];
    ParityBatchCase::value(
        "test_report_renders_failures_errors_diffs_and_navigation_metadata",
        elisp_form,
        expected,
    )
}

fn repl_streaming_preserves_transcript_syntax_results_and_searchable_history() -> ParityBatchCase {
    let elisp_form = r###"
(progn
  (neomacs-cider-test-reset)
  (with-temp-buffer
    (clojure-mode)
    (setq-local parse-sexp-lookup-properties t)
    (let ((cider-repl-use-clojure-font-lock t)
          (cider-repl-result-prefix "=> ")
          (cider-preferred-clojure-mode 'clojure-mode)
          (escape (string 27)))
      (cider-repl-reset-markers)
      (cider-repl--emit-output
       (current-buffer) (concat escape "[32mDeploying" escape "[0m\n")
       'cider-repl-stdout-face)
      (cider-repl--emit-output
       (current-buffer) ")))))" 'cider-repl-stderr-face)
      (cider-repl-emit-result (current-buffer) "{:release \"REL-" t)
      (cider-repl-emit-result (current-buffer) "42\"\n :state :healthy}" t)
      (cider-repl--finalize-result (current-buffer))
      (dolist (input '("(deploy! \"REL-41\")"
                       "(deploy! \"REL-42\")"
                       "(status \"REL-42\")"
                       "(deploy! \"REL-42\")"
                       ""))
        (cider-repl--add-to-input-history input))
      (let ((close-pos (save-excursion
                         (goto-char (point-min))
                         (search-forward ")))))")
                         (1- (point)))))
        (list :text (buffer-substring-no-properties (point-min) (point-max))
              :output-end (marker-position cider-repl-output-end)
              :result-markers (list cider-repl--result-start
                                    cider-repl--result-end)
              :output-close-syntax
              (syntax-class (get-text-property close-pos 'syntax-table))
              :scan-safe (ignore-errors
                           (scan-sexps (1+ close-pos) -1)
                           t)
              :result-face
              (save-excursion
                (goto-char (point-min))
                (search-forward ":state")
                (get-text-property (- (point) 6) 'face))
              :history cider-repl-input-history
              :deploy-index
              (cider-repl--position-in-history -1 'backward
                                               "^(deploy!")
              :status-index
              (cider-repl--position-in-history -1 'backward
                                               "^(status")
              :malformed
              (cider-repl--history-malformed-entries
               '("(deploy! \"REL-42\")"
                 "(status {:release \"REL-42\"})"
                 "(broken [1 2"
                 42)))))))
"###;
    let expected = expect![[
        r####"OK (:text "Deploying\n)))))=> {:release \"REL-42\"\n :state :healthy}\n" :output-end 55 :result-markers (nil nil) :output-close-syntax 1 :scan-safe t :result-face clojure-keyword-face :history ("(deploy! \"REL-42\")" "(status \"REL-42\")" "(deploy! \"REL-42\")" "(deploy! \"REL-41\")") :deploy-index 0 :status-index 1 :malformed ((2 . "(broken [1 2")))"####
    ]];
    ParityBatchCase::value(
        "repl_streaming_preserves_transcript_syntax_results_and_searchable_history",
        elisp_form,
        expected,
    )
}

fn stacktrace_rendering_filters_tooling_and_preserves_source_metadata() -> ParityBatchCase {
    let elisp_form = r###"
(progn
  (neomacs-cider-test-reset)
  (let* ((project-frame
          (nrepl-dict
           "ns" "release.core" "fn" "deploy!" "var" "release.core/deploy!"
           "file" "core.clj" "file-url" "file:/workspace/src/release/core.clj"
           "line" 42 "flags" '("clj" "project")))
         (tooling-frame
          (nrepl-dict
           "ns" "clojure.core" "fn" "apply" "var" "clojure.core/apply"
           "file" "core.clj" "line" 667 "flags" '("clj" "tooling")))
         (java-frame
          (nrepl-dict
           "class" "java.lang.Thread" "method" "run"
           "file" "Thread.java" "line" 840 "flags" '("java")))
         (causes
          (list
           (nrepl-dict
            "class" "java.lang.IllegalStateException"
            "message" "release REL-42 failed health checks"
            "data" "{:release \"REL-42\" :state :degraded}"
            "stacktrace" (list project-frame tooling-frame java-frame))
           (nrepl-dict
            "class" "clojure.lang.ExceptionInfo"
            "message" "deployment aborted"
            "data" "{:attempt 3}"
            "stacktrace" (list project-frame tooling-frame))))
         (buffer (get-buffer-create cider-error-buffer)))
    (unwind-protect
        (with-current-buffer buffer
          (cider-stacktrace-mode)
          (let ((cider-stacktrace-fill-column nil))
            (setq-local cider-stacktrace-filters '(tooling dup))
            (cider-stacktrace-render buffer causes)
            (let ((initial-text
                   (buffer-substring-no-properties (point-min) (point-max)))
                  (initial-hidden cider-stacktrace-hidden-frame-count)
                  (frames (neomacs-cider-test-property-runs 'flags))
                  (sources
                   (let ((position (point-min))
                         rows)
                     (while (< position (point-max))
                       (when-let ((flags (get-text-property position 'flags)))
                         (push
                          (list :flags (copy-tree flags)
                                :file (get-text-property position 'file)
                                :file-url (get-text-property position 'file-url)
                                :line (get-text-property position 'line)
                                :var (get-text-property position 'var)
                                :invisible
                                (get-text-property position 'invisible))
                          rows))
                       (setq position
                             (next-single-property-change
                              position 'flags nil (point-max))))
                     (nreverse rows))))
              (cider-stacktrace-toggle-java)
              (let ((without-java cider-stacktrace-hidden-frame-count))
                (cider-stacktrace-toggle-all)
                (list :text initial-text
                      :initial-hidden initial-hidden
                      :frames frames
                      :sources sources
                      :without-java without-java
                      :all-visible cider-stacktrace-hidden-frame-count
                      :filters cider-stacktrace-filters
                      :cause-visibility
                      (append cider-stacktrace-cause-visibility nil))))))
      (when (buffer-live-p buffer)
        (kill-buffer buffer)))))
"###;
    let expected = expect![[
        r####"OK (:text "\n  Show: Project-Only All \n  Hide: Clojure Java REPL Tooling Duplicates  (1 frames hidden)\n\n2. Unhandled java.lang.IllegalStateException\n   release REL-42 failed health checks\n   {:release \"REL-42\" :state :degraded}\n                  core.clj:   42  release.core/deploy!\n                  core.clj:  667  clojure.core/apply\n               Thread.java:  840  java.lang.Thread/run\n\n1. Caused by clojure.lang.ExceptionInfo\n   deployment aborted\n   {:attempt 3}\n                  core.clj:   42  release.core/deploy!\n                  core.clj:  667  clojure.core/apply\n\n" :initial-hidden 1 :frames (((clj project) 217 271 "                  core.clj:   42  release.core/deploy!") ((clj tooling) 272 324 "                  core.clj:  667  clojure.core/apply") ((java) 325 379 "               Thread.java:  840  java.lang.Thread/run") ((clj project) 459 513 "                  core.clj:   42  release.core/deploy!") ((clj tooling) 514 566 "                  core.clj:  667  clojure.core/apply")) :sources ((:flags (clj project) :file "core.clj" :file-url "file:/workspace/src/release/core.clj" :line 42 :var "release.core/deploy!" :invisible t) (:flags (clj tooling) :file "core.clj" :file-url nil :line 667 :var "clojure.core/apply" :invisible t) (:flags (java) :file "Thread.java" :file-url nil :line 840 :var nil :invisible t) (:flags (clj project) :file "core.clj" :file-url "file:/workspace/src/release/core.clj" :line 42 :var "release.core/deploy!" :invisible nil) (:flags (clj tooling) :file "core.clj" :file-url nil :line 667 :var "clojure.core/apply" :invisible t)) :without-java 1 :all-visible 0 :filters (all java tooling dup) :cause-visibility (1 2 1 1 1 1 1 1 1 1))"####
    ]];
    ParityBatchCase::value(
        "stacktrace_rendering_filters_tooling_and_preserves_source_metadata",
        elisp_form,
        expected,
    )
}

fn endpoint_selection_handles_labels_history_ports_and_discovered_services() -> ParityBatchCase {
    let elisp_form = r###"
(progn
  (neomacs-cider-test-reset)
  (let ((cider-known-endpoints
         '(("production" "prod.example.test" "7888")
           ("staging" "stage.example.test" "7999")))
        (cider-host-history '("old.example.test:7000"))
        prompts)
    (cl-letf (((symbol-function 'cider--ssh-hosts)
               (lambda () '(("bastion.example.test"))))
              ((symbol-function 'cider--infer-ports)
               (lambda (host _ssh-hosts)
                 (if (equal host "localhost")
                     '(("service-a" "4555") ("service-b" "4666"))
                   nil)))
              ((symbol-function 'completing-read)
               (lambda (prompt collection &rest _)
                 (push (list :prompt prompt
                             :choices
                             (mapcar
                              (lambda (item)
                                (if (consp item) (car item) item))
                              collection))
                       prompts)
                 (cond
                  ((string-prefix-p "Host:" prompt)
                   "production:prod.example.test:7888")
                  ((string-prefix-p "Port for" prompt) "service-a:4555")
                  (t (error "Unexpected prompt: %s" prompt))))))
      (let ((known (cider-select-endpoint)))
        (setq cider-known-endpoints nil
              cider-host-history nil)
        (let ((local
               (cl-letf (((symbol-function 'completing-read)
                          (lambda (prompt collection &rest _)
                            (push (list :prompt prompt
                                        :choices
                                        (mapcar
                                         (lambda (item)
                                           (if (consp item) (car item) item))
                                         collection))
                                  prompts)
                            (if (string-prefix-p "Host:" prompt)
                                "localhost"
                              "service-a:4555"))))
                 (cider-select-endpoint))))
          (list :known known
                :local local
                :prompts (nreverse prompts)
                :join
                (cider-join-into-alist
                 '(("production" "prod.example.test" "7888")
                   ("localhost")
                   ("label" "host" "9000")))))))))
"###;
    let expected = expect![[
        r####"OK (:known ("prod.example.test" . "7888") :local ("localhost" . 4555) :prompts ((:prompt "Host: " :choices ("old.example.test:7000" "localhost" "production:prod.example.test:7888" "staging:stage.example.test:7999" "bastion.example.test" "local-unix-domain-socket")) (:prompt "Host: " :choices ("localhost" "bastion.example.test" "local-unix-domain-socket")) (:prompt "Port for localhost: " :choices ("service-a:4555" "service-b:4666"))) :join (("production:prod.example.test:7888" "production" "prod.example.test" "7888") ("localhost" "localhost") ("label:host:9000" "label" "host" "9000")))"####
    ]];
    ParityBatchCase::value(
        "endpoint_selection_handles_labels_history_ports_and_discovered_services",
        elisp_form,
        expected,
    )
}

#[test]
fn cider_package_batch() {
    assert_oracle_batch_cases(
        cider_oracle(),
        "cider-package-batch",
        "cider",
        &[
            fragmented_nrepl_transport_decodes_and_aggregates_a_real_response(),
            completion_context_tracks_nested_clojure_edits_and_incomplete_input(),
            compiler_diagnostics_preserve_locations_severity_and_actionable_messages(),
            inspector_renders_nested_values_and_cycles_through_actionable_regions(),
            test_report_renders_failures_errors_diffs_and_navigation_metadata(),
            repl_streaming_preserves_transcript_syntax_results_and_searchable_history(),
            stacktrace_rendering_filters_tooling_and_preserves_source_metadata(),
            endpoint_selection_handles_labels_history_ports_and_discovered_services(),
        ],
    );
}
