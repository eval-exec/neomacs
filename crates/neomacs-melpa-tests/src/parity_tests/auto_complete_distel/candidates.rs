use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_distel_candidate_expression_forwards_prefix_and_current_buffer_exactly()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_distel_candidate_expression_forwards_prefix_and_current_buffer_exactly",
        r##"(with-temp-buffer
          (rename-buffer
           " *distel-candidate-buffer*")
          (insert "lists:ma")
          (let ((ac-prefix
                 "lists:ma")
                calls)
            (cl-letf
                (((symbol-function
                   'distel-completion-complete)
                  (lambda (prefix buffer)
                    (setq calls
                          (list
                           prefix
                           (eq buffer
                               (current-buffer))
                           (buffer-name buffer)
                           (buffer-string)))
                    '("lists:map"
                      "lists:mapfoldl"))))
              (list
               (eval
                (cdr
                 (assq 'candidates
                       auto-complete-distel)))
               calls))))"##,
        expect![[
            r#"OK (("lists:map" "lists:mapfoldl") ("lists:ma" t " *distel-candidate-buffer*" "lists:ma"))"#
        ]],
    )
}

fn distel_completion_bridge_routes_module_and_function_queries_and_prefixes_results()
-> ParityBatchCase {
    ParityBatchCase::value(
        "distel_completion_bridge_routes_module_and_function_queries_and_prefixes_results",
        r##"(let (events)
          (cl-letf
              (((symbol-function
                 'distel-completion-complete-module)
                (lambda (module)
                  (push
                   (list :module module)
                   events)
                  (setq
                   distel-completion-try-erl-complete-cache
                   (cond
                    ((equal module "li")
                     '("lists" "lib" "lists"))
                    (t nil)))))
               ((symbol-function
                 'distel-completion-complete-function)
                (lambda (module function)
                  (push
                   (list
                    :function
                    module
                    function)
                   events)
                  (setq
                   distel-completion-try-erl-complete-cache
                   '("map" "mapfoldl"))))
               ((symbol-function 'sleep-for)
                (lambda (&rest duration)
                  (push
                   (cons :sleep duration)
                   events))))
            (mapcar
             (lambda (search)
               (setq events nil)
               (let ((result
                      (distel-completion-complete
                       search
                       (current-buffer))))
                 (list
                  search
                  result
                  (nreverse events))))
             '("li"
               "lists:ma"
               ":ma"
               "lists:map:extra"))))"##,
        expect![[
            r#"OK (("li" ("lists" "lib" "lists") ((:module "li") (:sleep 0.1))) ("lists:ma" ("lists:map" "lists:mapfoldl") ((:function "lists" "ma") (:sleep 0.1))) (":ma" (":map" ":mapfoldl") ((:function "" "ma") (:sleep 0.1))) ("lists:map:extra" ("lists:map" "lists:mapfoldl") ((:function "lists" "map:extra") (:sleep 0.1))))"#
        ]],
    )
}

fn distel_completion_bridge_preserves_candidate_order_duplicates_and_text_properties()
-> ParityBatchCase {
    ParityBatchCase::value(
        "distel_completion_bridge_preserves_candidate_order_duplicates_and_text_properties",
        r##"(let* ((first
                                 (propertize
                                  "map"
                                  'arity 2))
                                (second
                                 (propertize
                                  "mapfoldl"
                                  'arity 3))
                                (distel-completion-try-erl-complete-cache
                                 nil))
          (cl-letf
              (((symbol-function
                 'distel-completion-complete-function)
                (lambda (_module _function)
                  (setq
                   distel-completion-try-erl-complete-cache
                   (list
                    first
                    second
                    first))))
               ((symbol-function 'sleep-for)
                (lambda (&rest _duration)
                  nil)))
            (let ((result
                   (distel-completion-complete
                    "lists:ma"
                    (current-buffer))))
              (list
               result
               (mapcar
                (lambda (candidate)
                  (list
                   (get-text-property
                    6 'arity candidate)
                   (eq candidate first)
                   (eq candidate second)))
                result)
               (list first second first)))))"##,
        expect![[
            r#"OK ((#("lists:map" 6 9 (arity 2)) #("lists:mapfoldl" 6 14 (arity 3)) #("lists:map" 6 9 (arity 2))) ((2 nil nil) (3 nil nil) (2 nil nil)) (#("map" 0 3 (arity 2)) #("mapfoldl" 0 8 (arity 3)) #("map" 0 3 (arity 2))))"#
        ]],
    )
}

fn distel_completion_bridge_uses_the_latest_async_cache_after_each_wait() -> ParityBatchCase {
    ParityBatchCase::value(
        "distel_completion_bridge_uses_the_latest_async_cache_after_each_wait",
        r##"(let ((distel-completion-try-erl-complete-cache
                               '("stale"))
                              pending)
          (cl-letf
              (((symbol-function
                 'distel-completion-complete-module)
                (lambda (module)
                  (setq pending
                        (list
                         (concat module "sts")
                         (concat module "b")))))
               ((symbol-function 'sleep-for)
                (lambda (&rest duration)
                  (setq
                   distel-completion-try-erl-complete-cache
                   pending)
                  duration)))
            (list
             (distel-completion-complete
              "li"
              (current-buffer))
             distel-completion-try-erl-complete-cache
             pending)))"##,
        expect![[r#"OK (("lists" "lib") #1=("lists" "lib") #1#)"#]],
    )
}

fn auto_complete_distel_document_entry_delegates_the_selected_erlang_candidate() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_distel_document_entry_delegates_the_selected_erlang_candidate",
        r##"(let (calls)
          (cl-letf
              (((symbol-function
                 'distel-completion-get-doc-string)
                (lambda (candidate)
                  (push candidate calls)
                  (format
                   "%s/2 maps a function over a list"
                   candidate))))
            (let ((document
                   (cdr
                    (assq 'document
                          auto-complete-distel))))
              (list
               (funcall
                document
                "lists:map")
               (funcall
                document
                "maps:find")
               (nreverse calls)))))"##,
        expect![[
            r#"OK ("lists:map/2 maps a function over a list" "maps:find/2 maps a function over a list" ("lists:map" "maps:find"))"#
        ]],
    )
}

fn auto_complete_distel_real_source_resolution_produces_candidates_with_symbol_and_document_metadata()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_distel_real_source_resolution_produces_candidates_with_symbol_and_document_metadata",
        r##"(with-temp-buffer
          (insert "lists:ma")
          (let ((ac-sources
                 '(auto-complete-distel))
                (ac-compiled-sources nil)
                (ac-use-comphist nil)
                (ac-ignore-case nil))
            (cl-letf
                (((symbol-function
                   'distel-completion-complete)
                  (lambda (prefix buffer)
                    (list
                     (concat prefix "p")
                     "lists:mapfoldl"))))
              (let* ((resolved
                      (ac-prefix 0 nil))
                     (ac-prefix
                      (buffer-substring-no-properties
                       (nth 1 resolved)
                       (point)))
                     (ac-current-sources
                      (nth 2 resolved))
                     (candidates
                      (ac-candidates)))
                (list
                 resolved
                 ac-prefix
                 (mapcar
                  (lambda (candidate)
                    (list
                     (substring-no-properties
                      candidate)
                     (popup-item-symbol candidate)
                     (popup-item-property
                      candidate
                      'document)))
                  candidates))))))"##,
        expect![[
            r#"OK ((auto-complete-distel-get-start 1 (((prefix . auto-complete-distel-get-start) (candidates distel-completion-complete ac-prefix (current-buffer)) (document . distel-completion-get-doc-string) (requires . 0) (symbol . "m")))) "lists:ma" (("lists:map" "m" distel-completion-get-doc-string) ("lists:mapfoldl" "m" distel-completion-get-doc-string)))"#
        ]],
    )
}

fn auto_complete_distel_source_resolution_rejects_punctuation_and_accepts_configured_suffixes()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_distel_source_resolution_rejects_punctuation_and_accepts_configured_suffixes",
        r##"(mapcar
          (lambda (fixture)
            (with-temp-buffer
              (insert
               (car fixture))
              (let ((distel-completion-valid-syntax
                     (cdr fixture))
                    (ac-sources
                     '(auto-complete-distel))
                    (ac-compiled-sources nil))
                (let ((resolved
                       (ac-prefix 0 nil)))
                  (list
                   fixture
                   resolved
                   (and
                    resolved
                    (buffer-substring-no-properties
                     (nth 1 resolved)
                     (point))))))))
          '(("." . "a-zA-Z:_-")
            ("lists:ma" . "a-zA-Z:_-")
            ("module2" . "a-zA-Z:_-")
            ("module2" . "a-zA-Z0-9:_-")
            ("app.module:run" . "a-zA-Z:_.-")))"##,
        expect![[
            r#"OK ((("." . "a-zA-Z:_-") nil nil) (("lists:ma" . "a-zA-Z:_-") (auto-complete-distel-get-start 1 (#1=((prefix . auto-complete-distel-get-start) (candidates distel-completion-complete ac-prefix (current-buffer)) (document . distel-completion-get-doc-string) (requires . 0) (symbol . "m")))) "lists:ma") (("module2" . "a-zA-Z:_-") nil nil) (("module2" . "a-zA-Z0-9:_-") (auto-complete-distel-get-start 1 (#1#)) "module2") (("app.module:run" . "a-zA-Z:_.-") (auto-complete-distel-get-start 1 (#1#)) "app.module:run"))"#
        ]],
    )
}

pub(super) fn candidates_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_distel_candidate_expression_forwards_prefix_and_current_buffer_exactly(),
        distel_completion_bridge_routes_module_and_function_queries_and_prefixes_results(),
        distel_completion_bridge_preserves_candidate_order_duplicates_and_text_properties(),
        distel_completion_bridge_uses_the_latest_async_cache_after_each_wait(),
        auto_complete_distel_document_entry_delegates_the_selected_erlang_candidate(),
        auto_complete_distel_real_source_resolution_produces_candidates_with_symbol_and_document_metadata(),
        auto_complete_distel_source_resolution_rejects_punctuation_and_accepts_configured_suffixes(),
    ]
}
