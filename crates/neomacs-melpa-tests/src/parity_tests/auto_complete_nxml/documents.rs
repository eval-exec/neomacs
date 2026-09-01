use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_nxml_note_store_appends_lines_at_current_index() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_nxml_note_store_appends_lines_at_current_index",
        r##"(progn
         (auto-complete-nxml-start-make-doc4ac-in-nxml)
         (auto-complete-nxml-store-note "first line")
         (auto-complete-nxml-store-note "second line")
         (let ((zero (auto-complete-nxml-get-stored-note 0))
               (next-index (auto-complete-nxml-get-note-stored-index)))
           (auto-complete-nxml-store-note "third line")
           (list zero
                 next-index
                 (auto-complete-nxml-get-stored-note next-index)
                 (acnxml-test-hash-alist auto-complete-nxml-note-store-hash))))"##,
        expect![[
            r#"OK ("first line\nsecond line" 1 "third line" ((0 . "first line\nsecond line") (1 . "third line")))"#
        ]],
    )
}

fn auto_complete_nxml_name_class_store_keeps_independent_indexed_values() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_nxml_name_class_store_keeps_independent_indexed_values",
        r##"(progn
         (auto-complete-nxml-start-make-doc4ac-in-nxml)
         (auto-complete-nxml-store-ncls '(name (ns . "root")))
         (let ((first-index (auto-complete-nxml-get-ncls-stored-index)))
           (auto-complete-nxml-store-ncls '(choice (other . "child")))
           (list first-index
                 (auto-complete-nxml-get-stored-ncls 0)
                 (auto-complete-nxml-get-stored-ncls first-index)
                 (acnxml-test-hash-alist auto-complete-nxml-ncls-store-hash))))"##,
        expect![[
            r#"OK (1 #1=(name (ns . "root")) #2=(choice (other . "child")) ((0 . #1#) (1 . #2#)))"#
        ]],
    )
}

fn auto_complete_nxml_document_capture_reset_replaces_all_mutable_stores() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_nxml_document_capture_reset_replaces_all_mutable_stores",
        r##"(progn
         (setq auto-complete-nxml-note-stored-index 17
               auto-complete-nxml-ncls-stored-index 23)
         (puthash 1 "old-note" auto-complete-nxml-note-store-hash)
         (puthash 2 '(old) auto-complete-nxml-ncls-store-hash)
         (puthash "old-element" :old auto-complete-nxml-element-document-hash)
         (puthash "old-attribute" :old auto-complete-nxml-attribute-document-hash)
         (let ((result (auto-complete-nxml-start-make-doc4ac-in-nxml)))
           (list result
                 auto-complete-nxml-note-stored-index
                 auto-complete-nxml-ncls-stored-index
                 (hash-table-count auto-complete-nxml-note-store-hash)
                 (hash-table-count auto-complete-nxml-ncls-store-hash)
                 (hash-table-count auto-complete-nxml-element-document-hash)
                 (hash-table-count auto-complete-nxml-attribute-document-hash))))"##,
        expect!["OK (t 0 0 0 0 0 0)"],
    )
}

fn auto_complete_nxml_make_document_combines_namespace_comment_and_note() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_nxml_make_document_combines_namespace_comment_and_note",
        r##"(progn
         (auto-complete-nxml-start-make-doc4ac-in-nxml)
         (puthash 4 '(name (html . "table"))
                  auto-complete-nxml-ncls-store-hash)
         (puthash 9 "A tabular element."
                  auto-complete-nxml-note-store-hash)
         (cl-letf (((symbol-function 'nxml-namespace-name)
                    (lambda (symbol)
                      (cdr (assq symbol
                                  '((html . "urn:xhtml")
                                    (svg . "urn:svg")))))))
           (auto-complete-nxml-make-document
            4 9 "Schema comment" auto-complete-nxml-element-document-hash))
         (list
          (hash-table-count auto-complete-nxml-element-document-hash)
          (acnxml-test-doc-value
           (gethash "urn:xhtml:table"
                    auto-complete-nxml-element-document-hash))))"##,
        expect![[
            r#"OK (1 (:name "table" :ns "urn:xhtml" :comment "Schema comment" :note "A tabular element."))"#
        ]],
    )
}

fn auto_complete_nxml_make_document_distinguishes_missing_empty_and_malformed_name_classes()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_nxml_make_document_distinguishes_missing_empty_and_malformed_name_classes",
        r##"(progn
         (auto-complete-nxml-start-make-doc4ac-in-nxml)
         (puthash 1 nil auto-complete-nxml-ncls-store-hash)
         (puthash 2 'wildcard auto-complete-nxml-ncls-store-hash)
         (puthash 3 '(name nil) auto-complete-nxml-ncls-store-hash)
         (mapcar
          (lambda (index)
            (list
             index
             (acnxml-test-error
              (lambda ()
                (auto-complete-nxml-make-document
                 index index "comment"
                 auto-complete-nxml-element-document-hash)))
             (hash-table-count
              auto-complete-nxml-element-document-hash)))
          '(1 2 3)))"##,
        expect![
            "OK ((1 (:value nil) 0) (2 (:signal wrong-type-argument (listp wildcard)) 0) (3 (:value nil) 0))"
        ],
    )
}

fn auto_complete_nxml_document_selected_formats_comment_note_and_fallback() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_nxml_document_selected_formats_comment_note_and_fallback",
        r##"(let ((hash (make-hash-table :test 'equal)))
         (puthash
          "table"
          (make-auto-complete-nxml-doc
           :name "table"
           :ns ""
           :comment "May contain rows."
           :note "Use tr children.")
          hash)
         (puthash
          "empty"
          (make-auto-complete-nxml-doc
           :name "empty"
           :ns ""
           :comment ""
           :note "")
          hash)
         (cl-letf (((symbol-function 'nxml-ns-get-default) (lambda () nil))
                   ((symbol-function 'nxml-ns-get-prefix) (lambda (_prefix) nil)))
           (mapcar
            (lambda (name)
              (cons name
                    (auto-complete-nxml-get-document-selected
                     name hash "ELEMENT")))
            '("table" "empty" "missing" "/closing"))))"##,
        expect![[
            r#"OK (("table" . "'table' is ELEMENT in ''.\n\nComment: \nMay contain rows.\n\nNote: \nUse tr children.\n") ("empty" . "'empty' is ELEMENT in ''.\n\nNot documented.\n") ("missing" . "'missing' is ELEMENT in ''.\n\nNot documented.\n") ("/closing" . ""))"#
        ]],
    )
}

fn auto_complete_nxml_document_selected_resolves_prefixed_namespace_keys() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_nxml_document_selected_resolves_prefixed_namespace_keys",
        r##"(let ((hash (make-hash-table :test 'equal)))
         (puthash
          "urn:math:sum"
          (make-auto-complete-nxml-doc
           :name "sum"
           :ns "urn:math"
           :comment "Adds operands."
           :note "")
          hash)
         (cl-letf (((symbol-function 'nxml-ns-get-prefix)
                    (lambda (prefix)
                      (and (equal prefix "m") 'math-ns)))
                   ((symbol-function 'nxml-ns-get-default)
                    (lambda () 'default-ns))
                   ((symbol-function 'nxml-namespace-name)
                    (lambda (symbol)
                      (if (eq symbol 'math-ns)
                          "urn:math"
                        "urn:default"))))
           (list
            (auto-complete-nxml-get-document-selected
             "m:sum" hash "ELEMENT")
            (auto-complete-nxml-get-document-selected
             "sum" hash "ELEMENT"))))"##,
        expect![[
            r#"OK ("'sum' is ELEMENT in 'urn:math'.\n\nComment: \nAdds operands.\n" "'sum' is ELEMENT in 'urn:default'.\n\nNot documented.\n")"#
        ]],
    )
}

fn auto_complete_nxml_document_selected_strips_candidate_text_properties() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_nxml_document_selected_strips_candidate_text_properties",
        r##"(let* ((hash (make-hash-table :test 'equal))
              (selected (propertize "entry" 'face 'bold 'meta '(1 2))))
         (puthash
          "entry"
          (make-auto-complete-nxml-doc
           :name "entry" :ns "" :comment "Documented." :note "")
          hash)
         (cl-letf (((symbol-function 'nxml-ns-get-default) (lambda () nil)))
           (let ((document
                  (auto-complete-nxml-get-document-selected
                   selected hash "ATTRIBUTE")))
             (list document
                   selected
                   (text-properties-at 0 selected)))))"##,
        expect![[r#"OK ("'entry' is ATTRIBUTE in ''.\n\nComment: \nDocumented.\n" "entry" nil)"#]],
    )
}

fn auto_complete_nxml_document_wrappers_select_their_distinct_hashes() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_nxml_document_wrappers_select_their_distinct_hashes",
        r##"(progn
         (setq auto-complete-nxml-element-document-hash
               (make-hash-table :test 'equal)
               auto-complete-nxml-attribute-document-hash
               (make-hash-table :test 'equal))
         (puthash "shared"
                  (make-auto-complete-nxml-doc
                   :name "shared" :ns "" :comment "element-doc" :note "")
                  auto-complete-nxml-element-document-hash)
         (puthash "shared"
                  (make-auto-complete-nxml-doc
                   :name "shared" :ns "" :comment "attribute-doc" :note "")
                  auto-complete-nxml-attribute-document-hash)
         (cl-letf (((symbol-function 'nxml-ns-get-default) (lambda () nil)))
           (list
            (auto-complete-nxml-get-document-tag "shared")
            (auto-complete-nxml-get-document-attr "shared"))))"##,
        expect![[
            r#"OK ("'shared' is ELEMENT in ''.\n\nComment: \nelement-doc\n" "'shared' is ATTRIBUTE in ''.\n\nComment: \nattribute-doc\n")"#
        ]],
    )
}

pub(super) fn documents_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_nxml_note_store_appends_lines_at_current_index(),
        auto_complete_nxml_name_class_store_keeps_independent_indexed_values(),
        auto_complete_nxml_document_capture_reset_replaces_all_mutable_stores(),
        auto_complete_nxml_make_document_combines_namespace_comment_and_note(),
        auto_complete_nxml_make_document_distinguishes_missing_empty_and_malformed_name_classes(),
        auto_complete_nxml_document_selected_formats_comment_note_and_fallback(),
        auto_complete_nxml_document_selected_resolves_prefixed_namespace_keys(),
        auto_complete_nxml_document_selected_strips_candidate_text_properties(),
        auto_complete_nxml_document_wrappers_select_their_distinct_hashes(),
    ]
}
