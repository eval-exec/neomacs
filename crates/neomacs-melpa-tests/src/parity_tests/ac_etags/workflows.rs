use expect_test::expect;

use super::ParityBatchCase;

fn ac_etags_completes_a_tag_prefix_from_a_real_tags_table() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_etags_completes_a_tag_prefix_from_a_real_tags_table",
        r#"
    ;; The README's whole setup: build a TAGS table for the project, select it
    ;; with `visit-tags-table', run `ac-etags-setup' and `ac-etags-ac-setup', then
    ;; type a prefix and complete.  Every tag sharing the prefix must be offered,
    ;; carrying the source's own faces and symbol, and `ac-complete' must insert
    ;; the first one.
    (let ((tags (ac-etags-test-tags-file "bank/TAGS" ac-etags-test-bank-entries)))
      (ac-etags-test-in-buffer
        (ac-etags-test-with-etags nil
          (visit-tags-table tags)
          (insert "int main(void) { bank_")
          (let* ((candidates (ac-etags-test-candidates))
                 (properties (ac-etags-test-candidate-properties))
                 (prefix ac-prefix))
            (ac-complete)
            (list ac-sources
                  auto-complete-mode
                  (ac-etags-test-table-names)
                  prefix
                  candidates
                  properties
                  (buffer-string)
                  (point)
                  (cdr (assq 'requires ac-source-etags))
                  (cdr (assq 'symbol ac-source-etags))
                  (cdr (assq 'candidates ac-source-etags))
                  (ac-etags-test-cache-entries)
                  (face-attribute 'ac-etags-candidate-face :inherit nil t)
                  (face-attribute 'ac-etags-selection-face :inherit nil t))))))
"#,
        expect![[
            r#"OK ((ac-source-etags) t ("bank") "bank_" ("bank_open" "bank_close" "bank_audit" "bank_transfer" "bank_überweisung") (("bank_open" "s" ac-etags-candidate-face ac-etags-selection-face) ("bank_close" "s" ac-etags-candidate-face ac-etags-selection-face) ("bank_audit" "s" ac-etags-candidate-face ac-etags-selection-face) ("bank_transfer" "s" ac-etags-candidate-face ac-etags-selection-face) ("bank_überweisung" "s" ac-etags-candidate-face ac-etags-selection-face)) "int main(void) { bank_open" 27 3 "s" ac-etags--candidates (("bank_" "bank_open" "bank_close" "bank_transfer" "bank_audit" "bank_überweisung")) ac-candidate-face ac-selection-face)"#
        ]],
    )
}

fn ac_etags_requires_option_sets_the_minimum_prefix_length() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_etags_requires_option_sets_the_minimum_prefix_length",
        r#"
    ;; `ac-etags-requires' is the package's one knob and its default is 3, so a
    ;; one or two character prefix must produce nothing at all.  The README shows
    ;; customizing it and calling `ac-etags-setup' again; the new value has to
    ;; reach the source that auto-complete consults.
    (let ((tags (ac-etags-test-tags-file "bank/TAGS" ac-etags-test-bank-entries)))
      (cl-flet ((attempt (requires text)
                  (let ((ac-etags-requires requires))
                    (ac-etags-test-in-buffer
                      (ac-etags-test-with-etags nil
                        (visit-tags-table tags)
                        (insert text)
                        (let ((candidates (ac-etags-test-candidates))
                              (prefix ac-prefix))
                          ;; Dismissing the completion is also what resets
                          ;; auto-complete's compiled copy of the source.
                          (ac-abort)
                          (list text
                                (cdr (assq 'requires ac-source-etags))
                                prefix
                                candidates)))))))
        (list (default-value 'ac-etags-requires)
              (get 'ac-etags-requires 'custom-type)
              (attempt 3 "int x = b")
              (attempt 3 "int x = ba")
              (attempt 3 "int x = ban")
              (attempt 1 "int x = b")
              (attempt 5 "int x = bank")
              (attempt 5 "int x = bank_"))))
"#,
        expect![[
            r#"OK (3 integer ("int x = b" 3 nil nil) ("int x = ba" 3 nil nil) ("int x = ban" 3 "ban" ("bank_open" "bank_close" "bank_audit" "bank_transfer" "bank_überweisung")) ("int x = b" 1 "b" ("bank_open" "bank_close" "bank_audit" "bank_transfer" "bank_überweisung")) ("int x = bank" 5 nil nil) ("int x = bank_" 5 "bank_" ("bank_open" "bank_close" "bank_audit" "bank_transfer" "bank_überweisung")))"#
        ]],
    )
}

fn ac_etags_cache_serves_a_repeated_prefix_until_the_cache_is_cleared() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_etags_cache_serves_a_repeated_prefix_until_the_cache_is_cleared",
        r#"
    ;; The completion cache is keyed only by prefix, so after switching to
    ;; another project's TAGS the same prefix still answers with the previous
    ;; project's tags.  That is exactly why `ac-etags-clear-cache' is an
    ;; interactive command, and clearing it makes the next completion re-read the
    ;; newly selected table.
    (let ((bank (ac-etags-test-tags-file "bank/TAGS" ac-etags-test-bank-entries))
          (ledger (ac-etags-test-tags-file "ledger/TAGS" ac-etags-test-ledger-entries)))
      (ac-etags-test-with-etags nil
        (cl-flet ((complete-in (table)
                    (ac-etags-test-in-buffer
                      (visit-tags-table table)
                      (insert "call bank_")
                      (prog1 (list (ac-etags-test-table-names)
                                   (ac-etags-test-candidates))
                        (ac-abort)))))
          (let* ((first (complete-in bank))
                 (after-first (ac-etags-test-cache-entries))
                 (stale (complete-in ledger))
                 (cleared (progn (ac-etags-clear-cache)
                                 (hash-table-count ac-etags--completion-cache)))
                 (fresh (complete-in ledger)))
            (list first after-first stale cleared fresh
                  (ac-etags-test-cache-entries)
                  (commandp 'ac-etags-clear-cache))))))
"#,
        expect![[
            r#"OK ((("bank") ("bank_open" "bank_close" "bank_audit" "bank_transfer" "bank_überweisung")) (("bank_" "bank_open" "bank_close" "bank_transfer" "bank_audit" "bank_überweisung")) (("ledger") ("bank_open" "bank_close" "bank_audit" "bank_transfer" "bank_überweisung")) 0 (("ledger") ("bank_settle" "bank_reconcile")) (("bank_" "bank_reconcile" "bank_settle")) t)"#
        ]],
    )
    .fresh_process()
}

fn ac_etags_completes_across_several_tags_tables_at_once() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_etags_completes_across_several_tags_tables_at_once",
        r#"
    ;; With `tags-add-tables' set to t, a second `visit-tags-table' keeps the
    ;; first table instead of asking "Keep current list of tags tables also?".
    ;; Completion then has to span both projects, and the cache entry must hold
    ;; the union.
    (let ((bank (ac-etags-test-tags-file "bank/TAGS" ac-etags-test-bank-entries))
          (ledger (ac-etags-test-tags-file "ledger/TAGS" ac-etags-test-ledger-entries)))
      (ac-etags-test-in-buffer
        (ac-etags-test-with-etags t
          (visit-tags-table bank)
          (let ((after-first (ac-etags-test-table-names)))
            (visit-tags-table ledger)
            (insert "call bank_")
            (let ((candidates (ac-etags-test-candidates)))
              (ac-complete)
              (list after-first
                    (ac-etags-test-table-names)
                    candidates
                    (buffer-string)
                    (point)
                    (ac-etags-test-cache-entries)))))))
"#,
        expect![[
            r#"OK (("bank") ("ledger" "bank") ("bank_open" "bank_close" "bank_audit" "bank_settle" "bank_transfer" "bank_reconcile" "bank_überweisung") "call bank_open" 15 (("bank_" "bank_reconcile" "bank_settle" "bank_open" "bank_close" "bank_transfer" "bank_audit" "bank_überweisung")))"#
        ]],
    )
    .fresh_process()
}

fn ac_etags_offers_nothing_and_caches_nothing_without_a_tags_table() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_etags_offers_nothing_and_caches_nothing_without_a_tags_table",
        r#"
    ;; Without a tags table the source must stay completely quiet: no
    ;; candidates, no cache entry, no text inserted by `ac-complete', and no
    ;; attempt to read a table (which is what would make etags prompt).
    (ac-etags-test-in-buffer
      (ac-etags-test-with-etags nil
        (insert "call bank_")
        (let* ((candidates (ac-etags-test-candidates))
               (prefix ac-prefix)
               (direct (ac-etags--candidates)))
          (ac-complete)
          (list tags-table-list
                tags-file-name
                prefix
                candidates
                ac-candidates
                direct
                (buffer-string)
                (point)
                (hash-table-count ac-etags--completion-cache)
                (buffer-name (window-buffer (selected-window)))))))
"#,
        expect![[r#"OK (nil nil "bank_" nil nil nil "call bank_" 11 0 "*ac-etags-workflow*")"#]],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        ac_etags_completes_a_tag_prefix_from_a_real_tags_table(),
        ac_etags_requires_option_sets_the_minimum_prefix_length(),
        ac_etags_cache_serves_a_repeated_prefix_until_the_cache_is_cleared(),
        ac_etags_completes_across_several_tags_tables_at_once(),
        ac_etags_offers_nothing_and_caches_nothing_without_a_tags_table(),
    ]
}
