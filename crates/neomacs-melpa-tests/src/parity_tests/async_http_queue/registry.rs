use expect_test::expect;

use super::ParityBatchCase;

fn async_http_queue_descriptor_and_installed_sources_pin_exact_melpa_payload() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_http_queue_descriptor_and_installed_sources_pin_exact_melpa_payload",
        r##"(let* ((descriptor
                (cadr
                 (assq
                  'async-http-queue
                  package-alist)))
               (directory
                (package-desc-dir descriptor))
               (sources
                (sort
                 (directory-files
                  directory
                  t
                  "\\.el\\'")
                 #'string<)))
          (list
           (list
            (package-desc-name descriptor)
            (package-version-join
             (package-desc-version descriptor))
            (package-desc-summary descriptor)
            (package-desc-reqs descriptor)
            (package-desc-extras descriptor))
           (mapcar
            (lambda (file)
              (list
               (file-name-nondirectory file)
               (file-attribute-size
                (file-attributes file))
               (with-temp-buffer
                 (insert-file-contents-literally file)
                 (secure-hash
                  'sha256
                  (current-buffer)))))
            sources)))"##,
        expect![[
            r#"OK ((async-http-queue "20260316.755" "Async HTTP queue with parallel fetching." ((emacs (28 1))) ((:maintainers ("Andros Fenollosa" . "hi@andros.dev")) (:authors ("Andros Fenollosa" . "hi@andros.dev")) (:keywords "comm" "processes" "http") (:revdesc . "bd37342372a0") (:commit . "bd37342372a0b24ce0d54e9dad8070af997b0a0b") (:url . "https://git.andros.dev/andros/async-http-queue-el"))) (("async-http-queue-autoloads.el" 1987 "5e5172bc7345da202e7b23724ad57d2e265b612200788849e6a5640f92db5a70") ("async-http-queue-pkg.el" 457 "013411f667af9e43faf9ef293fdeb3616c1032fcbed3928bfc8aab73fac573a0") ("async-http-queue.el" 13719 "78039e8eada8d6d6957f5fc95a14f2b74fbf9b3bbca70d792ee19e35e96d7502")))"#
        ]],
    )
}

fn async_http_queue_complete_prefixed_symbol_inventory_records_generated_surface() -> ParityBatchCase
{
    ParityBatchCase::value(
        "async_http_queue_complete_prefixed_symbol_inventory_records_generated_surface",
        r##"(let (symbols)
          (mapatoms
           (lambda (symbol)
             (let ((name (symbol-name symbol)))
               (when
                   (and
                    (string-prefix-p
                     "async-http-queue"
                     name)
                    (not
                     (string-prefix-p
                      "async-http-queue-test"
                      name)))
                 (push
                  (list
                   symbol
                   (fboundp symbol)
                   (boundp symbol)
                   (macrop symbol)
                   (when (fboundp symbol)
                     (copy-tree
                      (help-function-arglist
                       symbol
                       t))))
                  symbols)))))
          (sort
           symbols
           (lambda (left right)
             (string<
              (symbol-name (car left))
              (symbol-name (car right))))))"##,
        expect![
            "OK ((async-http-queue t nil nil (urls &rest --cl-rest--)) (async-http-queue--check-completion t nil nil (state)) (async-http-queue--fetch-url t nil nil (state url success-callback error-callback)) (async-http-queue--process t nil nil (state)) (async-http-queue--process-next-pending t nil nil (state)) (async-http-queue--state nil nil nil nil) (async-http-queue--state-active-workers t nil nil (x)) (async-http-queue--state-active-workers--inliner t nil nil (inline--form x)) (async-http-queue--state-completion-callback t nil nil (x)) (async-http-queue--state-completion-callback--inliner t nil nil (inline--form x)) (async-http-queue--state-create t nil nil (&rest --cl-rest--)) (async-http-queue--state-create--cmacro t nil nil (cl-whole &rest --cl-rest--)) (async-http-queue--state-error-callback t nil nil (x)) (async-http-queue--state-error-callback--inliner t nil nil (inline--form x)) (async-http-queue--state-max-concurrent t nil nil (x)) (async-http-queue--state-max-concurrent--inliner t nil nil (inline--form x)) (async-http-queue--state-p t nil nil (x)) (async-http-queue--state-p--inliner t nil nil (inline--form x)) (async-http-queue--state-parser t nil nil (x)) (async-http-queue--state-parser--inliner t nil nil (inline--form x)) (async-http-queue--state-queue t nil nil (x)) (async-http-queue--state-queue--inliner t nil nil (inline--form x)) (async-http-queue--state-timeout t nil nil (x)) (async-http-queue--state-timeout--inliner t nil nil (inline--form x)) (async-http-queue--update-data t nil nil (state url data)) (async-http-queue--update-status t nil nil (state url status)) (async-http-queue-autoloads nil nil nil nil))"
        ],
    )
}

fn async_http_queue_all_declared_and_generated_functions_have_exact_contracts() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_http_queue_all_declared_and_generated_functions_have_exact_contracts",
        r##"(mapcar
          (lambda (symbol)
            (list
             symbol
             (fboundp symbol)
             (macrop symbol)
             (commandp symbol)
             (when (fboundp symbol)
               (copy-tree
                (help-function-arglist symbol t)))
             (when (fboundp symbol)
               (when-let
                   ((documentation
                     (documentation symbol t)))
                 (secure-hash
                  'sha256
                  documentation)))))
          '(async-http-queue
            async-http-queue--state-create
            async-http-queue--state-p
            async-http-queue--state-queue
            async-http-queue--state-active-workers
            async-http-queue--state-max-concurrent
            async-http-queue--state-timeout
            async-http-queue--state-parser
            async-http-queue--state-completion-callback
            async-http-queue--state-error-callback
            async-http-queue--update-status
            async-http-queue--update-data
            async-http-queue--fetch-url
            async-http-queue--process-next-pending
            async-http-queue--process
            async-http-queue--check-completion
            copy-async-http-queue--state))"##,
        expect![[
            r#"OK ((async-http-queue t nil nil (urls &rest --cl-rest--) "4177827ba6a66738ab74eaa12483222dd5979c9c920d1f6e9c8372c367f5d930") (async-http-queue--state-create t nil nil (&rest --cl-rest--) "b5a490856fa67c9981fe927ce8f669e8c593850a1974e6455e99961f9ff815b7") (async-http-queue--state-p t nil nil (x) nil) (async-http-queue--state-queue t nil nil (x) "e44ef9fe17d7041be8f256aa376531f7b40d3d5f0faeaac8e58bed17d406420a") (async-http-queue--state-active-workers t nil nil (x) "663b18acbe3d864e1441c13771e49b32fc5830b9f93e6d96d1368c0d2b3e945f") (async-http-queue--state-max-concurrent t nil nil (x) "012903dd38d771255032e73be90e9eb8d03c769caf64ec1e1c8805ffa6ee674d") (async-http-queue--state-timeout t nil nil (x) "7d748ed04931426a16ffba8f7124c6cfff0205ca37970d2d5f86fc455d41e018") (async-http-queue--state-parser t nil nil (x) "ea5bc8dd935d053143318482293ed9bb0034ac332c635daf635cf7dda13a1952") (async-http-queue--state-completion-callback t nil nil (x) "5570bf7ee4c77f2a175cdae5d938d5778ba80739091cf6dcdddc4d52b1cc4db7") (async-http-queue--state-error-callback t nil nil (x) "cb4534a805d09d361ead1f4a717deed8528b1167470c6f8b09a10dd372edbcb2") (async-http-queue--update-status t nil nil (state url status) "8dc8db383e68974b1d05876286c904f717d287963cab92c118dd0d31637695ee") (async-http-queue--update-data t nil nil (state url data) "967c79168c10b0729a5eab14233571b7b53e05e1736f97ccc04e50c4b5da69ed") (async-http-queue--fetch-url t nil nil (state url success-callback error-callback) "d4eb33bda69ac857088ef79219fb65bb473092d7100cf9018508f189bca68c7a") (async-http-queue--process-next-pending t nil nil (state) "5c2ca9a114917c4885a6c3e5d1821029cb972edccc6d388d61cd838aa73be1f7") (async-http-queue--process t nil nil (state) "5f2c450305cee418acc9e51f19eccf8191daf770fe0bd985bb27596ed0ca5ad3") (async-http-queue--check-completion t nil nil (state) "d54f90e711bbca4528487a5c02a1a2accd113301405d08cd4f051df74afa0893") (copy-async-http-queue--state nil nil nil nil nil))"#
        ]],
    )
}

fn async_http_queue_public_defaults_and_custom_keyword_arguments_build_exact_state()
-> ParityBatchCase {
    ParityBatchCase::value(
        "async_http_queue_public_defaults_and_custom_keyword_arguments_build_exact_state",
        r##"(let (states messages)
          (cl-letf
              (((symbol-function
                 'async-http-queue--process)
                (lambda (state)
                  (push
                   (async-http-queue-test-state-snapshot
                    state)
                   states)))
               ((symbol-function 'message)
                (lambda (format-string &rest arguments)
                  (push
                   (apply
                    #'format
                    format-string
                    arguments)
                   messages))))
            (async-http-queue
             '("https://api.test/default"))
            (async-http-queue
             '("https://api.test/a"
               "https://api.test/b")
             :callback #'ignore
             :error-callback #'ignore
             :max-concurrent 2
             :timeout 37
             :parser
             (lambda () :custom)))
          (list
           (nreverse states)
           (nreverse messages)))"##,
        expect![[
            r#"OK (((:queue (("https://api.test/default" pending nil)) :active 0 :limit 5 :timeout 10 :parser json-parse-buffer :completion nil :error nil) (:queue (("https://api.test/a" pending nil) ("https://api.test/b" pending nil)) :active 0 :limit 2 :timeout 37 :parser :custom :completion t :error t)) ("Fetching 1 URL..." "Fetching 2 URLs..."))"#
        ]],
    )
}

fn async_http_queue_empty_input_and_invalid_keyword_contracts_are_synchronous() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_http_queue_empty_input_and_invalid_keyword_contracts_are_synchronous",
        r##"(let (callbacks messages processed)
          (cl-letf
              (((symbol-function
                 'async-http-queue--process)
                (lambda (state)
                  (push state processed)))
               ((symbol-function 'message)
                (lambda (format-string &rest arguments)
                  (push
                   (apply
                    #'format
                    format-string
                    arguments)
                   messages))))
            (list
             (async-http-queue
              nil
              :callback
              (lambda (results)
                (push
                 (list
                  (vectorp results)
                  (length results)
                  (append results nil))
                 callbacks)))
             (async-http-queue nil)
             (async-http-queue-test-error-data
              (lambda ()
                (async-http-queue
                 '("https://api.test/a")
                 :unknown-option 9)))
             (nreverse callbacks)
             (nreverse messages)
             processed)))"##,
        expect![[
            r#"OK (#1=((t 0 nil)) nil (:error error ("Keyword argument :unknown-option not one of (:callback :error-callback :max-concurrent :timeout :parser)")) #1# ("No URLs provided" "No URLs provided") nil)"#
        ]],
    )
}

fn async_http_queue_generated_autoload_exposes_only_the_public_entry_point() -> ParityBatchCase {
    ParityBatchCase::value(
        "async_http_queue_generated_autoload_exposes_only_the_public_entry_point",
        r##"(let ((definition
                (symbol-function
                 'async-http-queue)))
          (list
           (featurep 'async-http-queue)
           (autoloadp definition)
           (and
            (autoloadp definition)
            (nth 1 definition))
           (and
            (autoloadp definition)
            (nth 4 definition))
           (commandp 'async-http-queue)
           (help-function-arglist
            'async-http-queue
            t)
           (fboundp
            'async-http-queue--state-create)
           (fboundp
            'async-http-queue--process)
           (get
            'async-http-queue
            'function-documentation)))"##,
        expect![[
            r#"OK (nil t "async-http-queue" nil nil "[Arg list not available until function definition is loaded.]" nil nil nil)"#
        ]],
    )
}

pub(super) fn registry_async_http_queue_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        async_http_queue_descriptor_and_installed_sources_pin_exact_melpa_payload(),
        async_http_queue_complete_prefixed_symbol_inventory_records_generated_surface(),
        async_http_queue_all_declared_and_generated_functions_have_exact_contracts(),
        async_http_queue_public_defaults_and_custom_keyword_arguments_build_exact_state(),
        async_http_queue_empty_input_and_invalid_keyword_contracts_are_synchronous(),
    ]
}

pub(super) fn registry_async_http_queue_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![async_http_queue_generated_autoload_exposes_only_the_public_entry_point()]
}
