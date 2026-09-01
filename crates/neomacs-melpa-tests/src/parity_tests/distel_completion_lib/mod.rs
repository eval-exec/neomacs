use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, DISTEL_COMPLETION_LIB_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const DISTEL_COMPLETION_LIB_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const DISTEL_COMPLETION_LIB_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

;; Distel is an intentionally undeclared runtime prerequisite of this package.
;; Public workflows replace its asynchronous boundary with deterministic replies.
(provide 'distel)
(defvar erl-nodename-cache 'neomacs-test@localhost)
"##;

fn distel_completion_lib_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(DISTEL_COMPLETION_LIB_MELPA_PIN, "distel-completion-lib.el")
        .expect("prepare pinned Distel Completion Lib source below ./tmp")
        .with_prelude(DISTEL_COMPLETION_LIB_TEST_PRELUDE)
        .with_timeout(DISTEL_COMPLETION_LIB_TEST_TIMEOUT)
}

fn erlang_module_index_and_identifier_extraction_drive_local_completion() -> ParityBatchCase {
    ParityBatchCase::value(
        "erlang_module_index_and_identifier_extraction_drive_local_completion",
        r##"
(with-temp-buffer
  (insert "-module(order_worker).\n"
          "-export([start_link/1, start_monitor/2, stop/0]).\n\n"
          "start_link(Config) ->\n"
          "    gen_server:start_link(?MODULE, Config, []).\n"
          "start_link(Config, Options) ->\n"
          "    gen_server:start_link(?MODULE, Config, Options).\n\n"
          "start_monitor(Config, Options) ->\n"
          "    monitor(process, start_link(Config, Options)).\n\n"
          "stop() -> ok.\n\n"
          "dispatch(Items) -> lists:map(fun route/1, Items).\n")
  (let ((start-definitions (distel-completion-get-functions "start"))
        (stop-definitions (distel-completion-get-functions "stop"))
        (missing-definitions (distel-completion-get-functions "restart")))
    (goto-char (point-max))
    (search-backward "lists:map")
    (goto-char (match-end 0))
    (list :start-definitions start-definitions
          :stop-definitions stop-definitions
          :missing-definitions missing-definitions
          :identifier-at-point (distel-completion-grab-word)
          :point (point)
          :source-size (buffer-size))))
"##,
        expect![[
            r##"OK (:start-definitions ("start_monitor" "start_link") :stop-definitions ("stop") :missing-definitions nil :identifier-at-point "lists:map" :point 359 :source-size 380)"##
        ]],
    )
}

fn erlang_context_filter_separates_code_from_comments_strings_and_quoted_atoms() -> ParityBatchCase
{
    ParityBatchCase::value(
        "erlang_context_filter_separates_code_from_comments_strings_and_quoted_atoms",
        r##"
(with-temp-buffer
  (insert "dispatch(Items) -> lists:map(fun route/1, Items).\n"
          "% disabled: lists:filter(fun active/1, Items).\n"
          "log() -> io:format(\"lists:sort is documentation\").\n"
          "lookup() -> ets:lookup('order-cache', current).\n")
  (mapcar
   (lambda (probe)
     (goto-char (point-min))
     (search-forward (car probe))
     (let ((word (distel-completion-grab-word))
           (context (if (distel-completion-is-comment-or-cite-p)
                        'non-code
                      'code)))
       (list (cdr probe) word context (line-number-at-pos))))
   '(("lists:map" . remote-call)
     ("lists:filter" . commented-call)
     ("io:format" . logging-call)
     ("lists:sort" . string-example)
     ("order-cache" . quoted-atom)
     ("current" . code-after-quoted-atom))))
"##,
        expect![[
            r##"OK ((remote-call "lists:map" code 1) (commented-call "lists:filter" non-code 2) (logging-call "io:format" code 3) (string-example "lists:sort" non-code 3) (quoted-atom "order-cache" non-code 4) (code-after-quoted-atom "current" code 4))"##
        ]],
    )
}

fn local_documentation_assembles_each_arity_and_clears_stale_rpc_state() -> ParityBatchCase {
    ParityBatchCase::value(
        "local_documentation_assembles_each_arity_and_clears_stale_rpc_state",
        r##"
(let ((argument-plans
       '((("Fun" "List") ("Fun" "List1" "List2"))
         nil))
      events)
  (cl-letf
      (((symbol-function 'distel-completion-get-metadoc)
        (lambda (module function)
          (setq distel-completion-try-erl-args-cache
                (pop argument-plans))
          (push (list :metadata module function
                      (copy-tree distel-completion-try-erl-args-cache))
                events)
          distel-completion-try-erl-args-cache))
       ((symbol-function 'distel-completion-describe)
        (lambda (module function arguments)
          (push (list :describe module function (copy-tree arguments)) events)
          (setq distel-completion-try-erl-desc-cache
                (concat (format "%s:%s/%d %S\n"
                                module function
                                (length arguments) arguments)
                        distel-completion-try-erl-desc-cache))))
       ((symbol-function 'sleep-for)
        (lambda (&rest duration)
          (push (cons :wait duration) events))))
    (let ((first (distel-completion-local-docs "lists" "map"))
          (first-cache distel-completion-try-erl-desc-cache)
          (second (distel-completion-local-docs "maps" "missing")))
      (list :first first
            :first-cache first-cache
            :second second
            :final-cache distel-completion-try-erl-desc-cache
            :events (nreverse events)))))
"##,
        expect![[
            r##"OK (:first "lists:map/3 (\"Fun\" \"List1\" \"List2\")\nlists:map/2 (\"Fun\" \"List\")\n" :first-cache "lists:map/3 (\"Fun\" \"List1\" \"List2\")\nlists:map/2 (\"Fun\" \"List\")\n" :second "" :final-cache "" :events ((:metadata "lists" "map" (("Fun" "List") ("Fun" "List1" "List2"))) (:describe "lists" "map" ("Fun" "List")) (:describe "lists" "map" ("Fun" "List1" "List2")) (:wait 0.1) (:metadata "maps" "missing" nil) (:wait 0.1)))"##
        ]],
    )
}

fn company_document_buffer_receives_the_rendered_erlang_help_with_properties() -> ParityBatchCase {
    ParityBatchCase::value(
        "company_document_buffer_receives_the_rendered_erlang_help_with_properties",
        r##"
(let ((buffer (generate-new-buffer " *distel-company-documentation*"))
      calls)
  (unwind-protect
      (cl-letf
          (((symbol-function 'company-doc-buffer)
            (lambda ()
              (push :company-buffer calls)
              (with-current-buffer buffer
                (erase-buffer))
              buffer))
           ((symbol-function 'distel-completion-get-doc-string)
            (lambda (candidate)
              (push (list :documentation candidate) calls)
              (propertize
               "lists:map/2\nMaps Fun over every item and preserves order."
               'face 'font-lock-function-name-face))))
        (let ((returned
               (distel-completion-get-doc-buffer "lists:map")))
          (with-current-buffer returned
            (list :same-buffer (eq returned buffer)
                  :name (buffer-name)
                  :contents (buffer-string)
                  :face-at-start (get-text-property (point-min) 'face)
                  :point (point)
                  :events (nreverse calls)))))
    (when (buffer-live-p buffer)
      (kill-buffer buffer))))
"##,
        expect![[
            r##"OK (:same-buffer t :name " *distel-company-documentation*" :contents #("lists:map/2\nMaps Fun over every item and preserves order." 0 57 (face font-lock-function-name-face)) :face-at-start font-lock-function-name-face :point 58 :events (:company-buffer (:documentation "lists:map")))"##
        ]],
    )
}

#[test]
fn distel_completion_lib_package_batch() {
    let cases = vec![
        erlang_module_index_and_identifier_extraction_drive_local_completion(),
        erlang_context_filter_separates_code_from_comments_strings_and_quoted_atoms(),
        local_documentation_assembles_each_arity_and_clears_stale_rpc_state(),
        company_document_buffer_receives_the_rendered_erlang_help_with_properties(),
    ];
    let thread = std::thread::current();
    let test_name = thread
        .name()
        .unwrap_or("unnamed Distel Completion Lib parity test");
    assert_oracle_batch_cases(
        distel_completion_lib_oracle(),
        test_name,
        "distel_completion_lib_parity",
        &cases,
    );
}
