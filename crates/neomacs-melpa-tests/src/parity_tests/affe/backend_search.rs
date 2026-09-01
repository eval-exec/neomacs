use expect_test::expect;

use super::ParityBatchCase;

fn affe_backend_append_producer_handles_empty_queue_then_splices_and_resets_nonempty_queue()
-> ParityBatchCase {
    ParityBatchCase::value(
        "affe_backend_append_producer_handles_empty_queue_then_splices_and_resets_nonempty_queue",
        r##"(let* ((affe-backend--search-head
                      (list nil "old"))
                     (affe-backend--search-tail
                      (last
                       affe-backend--search-head))
                     (affe-backend--producer-head
                      (list nil))
                     (affe-backend--producer-tail
                      affe-backend--producer-head))
               (let ((empty-result
                      (affe-backend--append-producer))
                     (empty-search
                      (copy-sequence
                       affe-backend--search-head))
                     (empty-producer
                      (copy-sequence
                       affe-backend--producer-head)))
                 (setq
                  affe-backend--producer-head
                  (list nil "new-1" "new-2")
                  affe-backend--producer-tail
                  (last
                   affe-backend--producer-head))
                 (let ((result
                        (affe-backend--append-producer)))
                   (list
                    empty-result
                    empty-search
                    empty-producer
                    result
                    (cdr
                     affe-backend--search-head)
                    (eq
                     affe-backend--search-tail
                     (last
                      affe-backend--search-head))
                    affe-backend--producer-head
                    (eq
                     affe-backend--producer-head
                     affe-backend--producer-tail)))))"##,
        expect![[r#"OK (nil (nil "old") (nil) #1=(nil) ("old" "new-1" "new-2") t #1# t)"#]],
    )
}

fn affe_backend_search_match_found_sends_properties_increments_and_throws_at_limit()
-> ParityBatchCase {
    ParityBatchCase::value(
        "affe_backend_search_match_found_sends_properties_increments_and_throws_at_limit",
        r##"(let ((affe-backend--client 'client)
                    (affe-backend--search-found 0)
                    (affe-backend--search-limit 3)
                    writes)
               (cl-letf
                   (((symbol-function
                      'process-send-string)
                     (lambda (_process string)
                       (push string writes))))
                 (let ((first
                        (copy-sequence "body")))
                   (add-text-properties
                    0 1
                    '(affe--prefix "pre:"
                      affe--suffix ":post"
                      face bold)
                    first)
                   (let ((first-result
                          (affe-backend--search-match-found
                           first))
                         (first-found
                          affe-backend--search-found)
                         (first-writes
                          (nreverse writes)))
                     (setq writes nil
                           affe-backend--search-found
                           2)
                     (let ((limit-result
                            (catch
                                'affe-backend--search-done
                              (list
                               'returned
                               (affe-backend--search-match-found
                                "last")))))
                       (list
                        first-result
                        first-found
                        first-writes
                        limit-result
                        affe-backend--search-found
                        (nreverse writes)))))))"##,
        expect![[
            r#"OK (nil 1 ("(search t)\n" "flush\n" "(match \"pre:\" \"body\" \":post\")\n") nil 3 ("(search t)\n" "(match nil \"last\" nil)\n"))"#
        ]],
    )
}

fn affe_backend_search_filters_case_insensitively_by_all_regexps_and_stops_at_limit()
-> ParityBatchCase {
    ParityBatchCase::value(
        "affe_backend_search_filters_case_insensitively_by_all_regexps_and_stops_at_limit",
        r##"(let* ((affe-backend--client 'client)
                     (affe-backend--search-head
                      (list nil))
                     (affe-backend--search-tail
                      affe-backend--search-head)
                     (affe-backend--search-found 0)
                     (affe-backend--search-limit 2)
                     (affe-backend--search-regexps
                      '("alpha" "beta"))
                     (affe-backend--producer-head
                      (list nil
                            "Alpha beta"
                            "ALPHA"
                            "beta alpha"
                            "gamma"))
                     (affe-backend--producer-tail
                      (last
                       affe-backend--producer-head))
                     (affe-backend--producer-done t)
                     writes)
               (cl-letf
                   (((symbol-function
                      'process-send-string)
                     (lambda (_process string)
                       (push string writes))))
                 (affe-backend--search)
                 (list
                  affe-backend--search-found
                  affe-backend--search-limit
                  (cdr
                   affe-backend--search-head)
                  (cdr
                   affe-backend--producer-head)
                  (nreverse writes))))"##,
        expect![[
            r#"OK (2 0 ("Alpha beta" "ALPHA" "beta alpha" "gamma") nil ("(search t)\n" "(search t)\n" "flush\n" "(match nil \"Alpha beta\" nil)\n" "(search t)\n" "(match nil \"beta alpha\" nil)\n" "(search nil)\n"))"#
        ]],
    )
}

fn affe_backend_search_done_without_matches_flushes_and_deactivates() -> ParityBatchCase {
    ParityBatchCase::value(
        "affe_backend_search_done_without_matches_flushes_and_deactivates",
        r##"(let* ((affe-backend--client 'client)
                     (affe-backend--search-head
                      (list nil))
                     (affe-backend--search-tail
                      affe-backend--search-head)
                     (affe-backend--search-found 0)
                     (affe-backend--search-limit 5)
                     (affe-backend--search-regexps
                      '("missing"))
                     (affe-backend--producer-head
                      (list nil "alpha" "beta"))
                     (affe-backend--producer-tail
                      (last
                       affe-backend--producer-head))
                     (affe-backend--producer-done t)
                     writes)
               (cl-letf
                   (((symbol-function
                      'process-send-string)
                     (lambda (_process string)
                       (push string writes))))
                 (affe-backend--search)
                 (list
                  affe-backend--search-found
                  affe-backend--search-limit
                  (cdr
                   affe-backend--search-head)
                  (nreverse writes))))"##,
        expect![[r#"OK (0 0 ("alpha" "beta") ("(search t)\n" "flush\n" "(search nil)\n"))"#]],
    )
}

fn affe_backend_search_waits_for_incomplete_producer_then_finishes_empty_stream() -> ParityBatchCase
{
    ParityBatchCase::value(
        "affe_backend_search_waits_for_incomplete_producer_then_finishes_empty_stream",
        r##"(let* ((affe-backend--client 'client)
                     (affe-backend--search-head
                      (list nil))
                     (affe-backend--search-tail
                      affe-backend--search-head)
                     (affe-backend--search-found 0)
                     (affe-backend--search-limit 4)
                     (affe-backend--search-regexps
                      '("anything"))
                     (affe-backend--producer-head
                      (list nil))
                     (affe-backend--producer-tail
                      affe-backend--producer-head)
                     (affe-backend--producer-done nil)
                     writes)
               (cl-letf
                   (((symbol-function
                      'process-send-string)
                     (lambda (_process string)
                       (push string writes))))
                 (affe-backend--search)
                 (let ((active-state
                        (list
                         affe-backend--search-limit
                         (nreverse writes))))
                   (setq writes nil
                         affe-backend--producer-done
                         t)
                   (affe-backend--search)
                   (list
                    active-state
                    affe-backend--search-limit
                    (nreverse writes)))))"##,
        expect![[
            r#"OK ((4 ("(search t)\n" "(search t)\n")) 0 ("(search t)\n" "flush\n" "(search nil)\n"))"#
        ]],
    )
}

pub(super) fn backend_search_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        affe_backend_append_producer_handles_empty_queue_then_splices_and_resets_nonempty_queue(),
        affe_backend_search_match_found_sends_properties_increments_and_throws_at_limit(),
        affe_backend_search_filters_case_insensitively_by_all_regexps_and_stops_at_limit(),
        affe_backend_search_done_without_matches_flushes_and_deactivates(),
        affe_backend_search_waits_for_incomplete_producer_then_finishes_empty_stream(),
    ]
}
