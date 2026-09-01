use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_chunk_list_is_buffer_local_and_accessor_returns_each_buffer_dictionary()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_chunk_list_is_buffer_local_and_accessor_returns_each_buffer_dictionary",
        r##"(let ((first (generate-new-buffer " *chunk-first*"))
                             (second (generate-new-buffer " *chunk-second*")))
                         (unwind-protect
                             (progn
                               (with-current-buffer first
                                 (setq ac-chunk-list
                                       '("os.path.abspath"
                                         "os.path.basename")))
                               (with-current-buffer second
                                 (setq ac-chunk-list
                                       '("json.decoder.JSONDecoder")))
                               (list
                                (default-value 'ac-chunk-list)
                                (with-current-buffer first
                                  (list
                                   (local-variable-p 'ac-chunk-list)
                                   (ac-chunk-list)))
                                (with-current-buffer second
                                  (list
                                   (local-variable-p 'ac-chunk-list)
                                   (ac-chunk-list)))))
                           (kill-buffer first)
                           (kill-buffer second)))"##,
        expect![[
            r#"OK (nil (t ("os.path.abspath" "os.path.basename")) (t ("json.decoder.JSONDecoder")))"#
        ]],
    )
}

fn auto_complete_chunk_default_dictionary_is_inherited_until_a_buffer_overrides_it()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_chunk_default_dictionary_is_inherited_until_a_buffer_overrides_it",
        r##"(let ((old-default
                                (default-value 'ac-chunk-list))
                               (inherited
                                (generate-new-buffer " *chunk-inherited*"))
                               (overridden
                                (generate-new-buffer " *chunk-overridden*")))
                           (unwind-protect
                               (progn
                                 (setq-default
                                  ac-chunk-list
                                  '("pathlib.Path"
                                    "pathlib.PurePath"))
                                 (with-current-buffer overridden
                                   (setq ac-chunk-list
                                         '("pathlib.PosixPath")))
                                 (list
                                  (with-current-buffer inherited
                                    (list
                                     (local-variable-p 'ac-chunk-list)
                                     (ac-chunk-list)))
                                  (with-current-buffer overridden
                                    (list
                                     (local-variable-p 'ac-chunk-list)
                                     (ac-chunk-list)))
                                  (default-value 'ac-chunk-list)))
                             (set-default 'ac-chunk-list old-default)
                             (kill-buffer inherited)
                             (kill-buffer overridden)))"##,
        expect![[
            r#"OK ((nil #1=("pathlib.Path" "pathlib.PurePath")) (t ("pathlib.PosixPath")) #1#)"#
        ]],
    )
}

fn auto_complete_chunk_list_candidates_use_the_active_buffers_dictionary_and_prefix()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_chunk_list_candidates_use_the_active_buffers_dictionary_and_prefix",
        r##"(let ((python (generate-new-buffer " *chunk-python*"))
                             (json (generate-new-buffer " *chunk-json*")))
                         (unwind-protect
                             (progn
                               (with-current-buffer python
                                 (python-mode)
                                 (setq ac-chunk-list
                                       '("os.path.abspath"
                                         "os.path.altsep"
                                         "sys.path"))
                                 (insert "target = os.path.a"))
                               (with-current-buffer json
                                 (fundamental-mode)
                                 (setq ac-chunk-list
                                       '("json.decoder.JSONDecoder"
                                         "json.decoder.JSONDecodeError"
                                         "json.encoder.JSONEncoder"))
                                 (insert "(json.decoder.JSOND"))
                               (list
                                (with-current-buffer python
                                  (list
                                   (buffer-string)
                                   (ac-chunk-beginning)
                                   (ac-chunk-list-candidates)))
                                (with-current-buffer json
                                  (list
                                   (buffer-string)
                                   (ac-chunk-beginning)
                                   (ac-chunk-list-candidates)))))
                           (kill-buffer python)
                           (kill-buffer json)))"##,
        expect![[
            r#"OK (("target = os.path.a" 10 ("os.path.abspath" "os.path.altsep")) ("(json.decoder.JSOND" 2 ("json.decoder.JSONDecoder" "json.decoder.JSONDecodeError")))"#
        ]],
    )
    .fresh_process()
}

fn auto_complete_chunk_dictionary_candidates_delegate_once_and_preserve_dictionary_order()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_chunk_dictionary_candidates_delegate_once_and_preserve_dictionary_order",
        r##"(with-temp-buffer
                           (python-mode)
                           (insert "module.service.f")
                           (let ((calls 0)
                                 (dictionary
                                  '("module.service.fetch"
                                    "other.service.fetch"
                                    "module.service.flush"
                                    "module.service.fetch")))
                             (cl-letf
                                 (((symbol-function
                                    'ac-buffer-dictionary)
                                   (lambda ()
                                     (setq calls (1+ calls))
                                     dictionary)))
                               (let ((result
                                      (ac-dictionary-chunk-candidates)))
                                 (list
                                  calls
                                  result
                                  (eq result dictionary)
                                  dictionary)))))"##,
        expect![[
            r#"OK (1 ("module.service.fetch" "module.service.flush" "module.service.fetch") nil ("module.service.fetch" "other.service.fetch" "module.service.flush" "module.service.fetch"))"#
        ]],
    )
    .fresh_process()
}

fn auto_complete_chunk_source_alists_invoke_real_prefix_candidate_and_symbol_contracts()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_chunk_source_alists_invoke_real_prefix_candidate_and_symbol_contracts",
        r##"(with-temp-buffer
                           (python-mode)
                           (setq ac-chunk-list
                                 '("client.api.create"
                                   "client.api.close"
                                   "client.cache.clear"))
                           (insert "result = client.api.c")
                           (mapcar
                            (lambda (source)
                              (let ((definition
                                     (symbol-value source)))
                                (list
                                 source
                                 (funcall
                                  (cdr
                                   (assq 'prefix definition)))
                                 (funcall
                                  (cdr
                                   (assq 'candidates definition)))
                                 (cdr
                                  (assq 'symbol definition)))))
                            '(ac-source-chunk-list)))"##,
        expect![[r#"OK ((ac-source-chunk-list 10 ("client.api.create" "client.api.close") "c"))"#]],
    )
    .fresh_process()
}

fn auto_complete_chunk_dictionary_source_alist_uses_real_dictionary_provider_result()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_chunk_dictionary_source_alist_uses_real_dictionary_provider_result",
        r##"(with-temp-buffer
                           (fundamental-mode)
                           (insert "project.cache.r")
                           (let ((calls 0))
                             (cl-letf
                                 (((symbol-function
                                    'ac-buffer-dictionary)
                                   (lambda ()
                                     (setq calls (1+ calls))
                                     '("project.cache.read"
                                       "project.cache.reset"
                                       "project.config.read"))))
                               (let ((definition
                                      ac-source-dictionary-chunk))
                                 (list
                                  (funcall
                                   (cdr
                                    (assq 'prefix definition)))
                                  (funcall
                                   (cdr
                                    (assq 'candidates definition)))
                                  (cdr
                                   (assq 'symbol definition))
                                  calls)))))"##,
        expect![[r#"OK (1 ("project.cache.read" "project.cache.reset") "c" 1)"#]],
    )
    .fresh_process()
}

fn auto_complete_chunk_dictionary_swap_removes_all_identical_builtin_entries_and_prepends_once()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_chunk_dictionary_swap_removes_all_identical_builtin_entries_and_prepends_once",
        r##"(let ((ac-sources
                               '(ac-source-words-in-same-mode-buffers
                                 ac-source-dictionary
                                 ac-source-filename
                                 ac-source-dictionary
                                 ac-source-dictionary-chunk)))
                          (let ((first
                                 (progn
                                   (ac-use-dictionary-chunk)
                                   (copy-sequence ac-sources))))
                            (ac-use-dictionary-chunk)
                            (list
                             first
                             ac-sources
                             (length
                              (delq nil
                                    (mapcar
                                     (lambda (source)
                                       (eq source
                                           'ac-source-dictionary-chunk))
                                     ac-sources)))
                             (memq
                              'ac-source-dictionary
                              ac-sources))))"##,
        expect![
            "OK ((ac-source-words-in-same-mode-buffers ac-source-filename ac-source-dictionary-chunk) (ac-source-words-in-same-mode-buffers ac-source-filename ac-source-dictionary-chunk) 1 nil)"
        ],
    )
}

fn auto_complete_chunk_dictionary_swap_changes_only_the_current_buffers_sources() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_chunk_dictionary_swap_changes_only_the_current_buffers_sources",
        r##"(let ((first (generate-new-buffer " *chunk-sources-first*"))
                             (second (generate-new-buffer " *chunk-sources-second*")))
                         (unwind-protect
                             (progn
                               (with-current-buffer first
                                 (setq ac-sources
                                       '(ac-source-dictionary
                                         ac-source-filename))
                                 (ac-use-dictionary-chunk))
                               (with-current-buffer second
                                 (setq ac-sources
                                       '(ac-source-dictionary
                                         ac-source-words-in-buffer)))
                               (list
                                (with-current-buffer first
                                  (list
                                   (local-variable-p 'ac-sources)
                                   ac-sources))
                                (with-current-buffer second
                                  (list
                                   (local-variable-p 'ac-sources)
                                   ac-sources))))
                           (kill-buffer first)
                           (kill-buffer second)))"##,
        expect![
            "OK ((t (ac-source-dictionary-chunk ac-source-filename)) (t (ac-source-dictionary ac-source-words-in-buffer)))"
        ],
    )
}

pub(super) fn sources_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_chunk_list_is_buffer_local_and_accessor_returns_each_buffer_dictionary(),
        auto_complete_chunk_default_dictionary_is_inherited_until_a_buffer_overrides_it(),
        auto_complete_chunk_list_candidates_use_the_active_buffers_dictionary_and_prefix(),
        auto_complete_chunk_dictionary_candidates_delegate_once_and_preserve_dictionary_order(),
        auto_complete_chunk_source_alists_invoke_real_prefix_candidate_and_symbol_contracts(),
        auto_complete_chunk_dictionary_source_alist_uses_real_dictionary_provider_result(),
        auto_complete_chunk_dictionary_swap_removes_all_identical_builtin_entries_and_prepends_once(
        ),
        auto_complete_chunk_dictionary_swap_changes_only_the_current_buffers_sources(),
    ]
}
