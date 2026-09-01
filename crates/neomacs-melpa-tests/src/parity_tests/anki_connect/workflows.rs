use expect_test::expect;

use super::ParityBatchCase;

fn anki_connect_declared_install_cannot_create_a_hierarchical_deck_without_s() -> ParityBatchCase {
    ParityBatchCase::signal(
        "anki_connect_declared_install_cannot_create_a_hierarchical_deck_without_s",
        r##"(anki-connect-ensure-deck
                           "Languages::Spanish")"##,
        expect!["ERR (void-function s-split)"],
    )
}

fn anki_connect_creates_missing_deck_hierarchy_once_through_the_http_api() -> ParityBatchCase {
    ParityBatchCase::value(
        "anki_connect_creates_missing_deck_hierarchy_once_through_the_http_api",
        r##"(let ((decks
                                '("Default"))
                               requests
                               response-buffers)
                           (unwind-protect
                               (cl-letf
                                   (((symbol-function
                                      'url-retrieve-synchronously)
                                     (lambda (url &rest _arguments)
                                       (let* ((payload
                                               (json-read-from-string
                                                url-request-data))
                                              (action
                                               (alist-get
                                                'action
                                                payload))
                                              (params
                                               (alist-get
                                                'params
                                                payload))
                                              (result
                                               (pcase action
                                                 ("deckNames"
                                                  (vconcat decks))
                                                 ("createDeck"
                                                  (let ((deck
                                                         (alist-get
                                                          'deck
                                                          params)))
                                                    (setq decks
                                                          (append
                                                           decks
                                                           (list deck)))
                                                    (length decks))))))
                                         (push
                                          (list
                                           action
                                           params)
                                          requests)
                                         (let ((buffer
                                                (generate-new-buffer
                                                 " *anki-connect-response*")))
                                           (push buffer
                                                 response-buffers)
                                           (with-current-buffer buffer
                                             (insert
                                              "HTTP/1.1 200 OK\n"
                                              "Content-Type: application/json\n"
                                              "\n"
                                              (json-encode
                                               `((result . ,result)
                                                 (error . nil)))))
                                           (unless
                                               (equal
                                                url
                                                anki-connect-url)
                                             (error
                                              "unexpected AnkiConnect URL: %s"
                                              url))
                                           buffer)))))
                                 (anki-connect-ensure-deck
                                  "Languages::Spanish::Verbs")
                                 (let ((after-first
                                        (copy-sequence decks))
                                       (first-request-count
                                        (length requests)))
                                   (anki-connect-ensure-deck
                                    "Languages::Spanish::Verbs")
                                   (list
                                    after-first
                                    decks
                                    first-request-count
                                    (length requests)
                                    (nreverse requests)
                                    (mapcar
                                     #'buffer-live-p
                                     response-buffers))))
                             (mapc
                              (lambda (buffer)
                                (when
                                    (buffer-live-p buffer)
                                  (kill-buffer buffer)))
                              response-buffers)))"##,
        expect![[
            r#"OK (("Default" "Languages" "Languages::Spanish" "Languages::Spanish::Verbs") ("Default" "Languages" "Languages::Spanish" "Languages::Spanish::Verbs") 6 9 (("deckNames" nil) ("createDeck" ((deck . "Languages"))) ("deckNames" nil) ("createDeck" ((deck . "Languages::Spanish"))) ("deckNames" nil) ("createDeck" ((deck . "Languages::Spanish::Verbs"))) ("deckNames" nil) ("deckNames" nil) ("deckNames" nil)) (t t t t t t t t t))"#
        ]],
    )
}

fn anki_connect_discovers_a_model_then_adds_and_updates_a_realistic_note() -> ParityBatchCase {
    ParityBatchCase::value(
        "anki_connect_discovers_a_model_then_adds_and_updates_a_realistic_note",
        r##"(let ((models
                                ["Basic" "Basic (and reversed card)"])
                               (model-fields
                                '(("Basic"
                                   . ["Front" "Back"])))
                               notes
                               requests
                               response-buffers
                               (next-id 1001))
                           (unwind-protect
                               (cl-letf
                                   (((symbol-function
                                      'url-retrieve-synchronously)
                                     (lambda (_url &rest _arguments)
                                       (let* ((payload
                                               (json-read-from-string
                                                url-request-data))
                                              (action
                                               (alist-get
                                                'action
                                                payload))
                                              (params
                                               (alist-get
                                                'params
                                                payload))
                                              (result
                                               (pcase action
                                                 ("modelNames"
                                                  models)
                                                 ("modelFieldNames"
                                                  (cdr
                                                   (assoc
                                                    (alist-get
                                                     'modelName
                                                     params)
                                                    model-fields)))
                                                 ("addNote"
                                                  (let ((note
                                                         (alist-get
                                                          'note
                                                          params)))
                                                    (push
                                                     (cons
                                                      next-id
                                                      note)
                                                     notes)
                                                    next-id))
                                                 ("updateNote"
                                                  (let* ((note
                                                          (alist-get
                                                           'note
                                                           params))
                                                         (id
                                                          (alist-get
                                                           'id
                                                           note))
                                                         (entry
                                                          (assq
                                                           id
                                                           notes)))
                                                    (setcdr
                                                     entry
                                                     note)
                                                    nil)))))
                                         (push
                                          (list
                                           action
                                           params)
                                          requests)
                                         (let ((buffer
                                                (generate-new-buffer
                                                 " *anki-connect-note-response*")))
                                           (push buffer
                                                 response-buffers)
                                           (with-current-buffer buffer
                                             (insert
                                              "HTTP/1.1 200 OK\n"
                                              "Content-Type: application/json\n"
                                              "\n"
                                              (json-encode
                                               `((result . ,result)
                                                 (error . nil)))))
                                           buffer)))))
                                 (let* ((available-models
                                         (anki-connect-model-names))
                                        (fields
                                         (anki-connect-model-field-names
                                          "Basic"))
                                        (note-id
                                         (anki-connect-add-note
                                          "Languages::Spanish::Verbs"
                                          "Basic"
                                          '(("Front"
                                             . "hablar")
                                            ("Back"
                                             . "to speak"))
                                          '[(("url"
                                              . "https://audio.test/hablar.mp3")
                                             ("filename"
                                              . "hablar.mp3")
                                             ("fields"
                                              . ["Front"]))]))
                                        (update-result
                                         (anki-connect-update-note
                                          note-id
                                          '(("Front"
                                             . "hablar")
                                            ("Back"
                                             . "to speak; to talk"))
                                          ["spanish" "verb"])))
                                   (list
                                    available-models
                                    fields
                                    note-id
                                    update-result
                                    (reverse
                                     (copy-tree notes))
                                    (nreverse requests)
                                    (mapcar
                                     #'buffer-live-p
                                     response-buffers))))
                             (mapc
                              (lambda (buffer)
                                (when
                                    (buffer-live-p buffer)
                                  (kill-buffer buffer)))
                              response-buffers)))"##,
        expect![[
            r#"OK (("Basic" "Basic (and reversed card)") ("Front" "Back") 1001 nil ((1001 (id . 1001) (fields (Front . "hablar") (Back . "to speak; to talk")) (tags . #1=["spanish" "verb"]))) (("modelNames" nil) ("modelFieldNames" ((modelName . "Basic"))) ("addNote" ((note (deckName . "Languages::Spanish::Verbs") (modelName . "Basic") (fields (Front . "hablar") (Back . "to speak")) (tags . []) (audio . [((url . "https://audio.test/hablar.mp3") (filename . "hablar.mp3") (fields . ["Front"]))])))) ("updateNote" ((note (id . 1001) (fields (Front . "hablar") (Back . "to speak; to talk")) (tags . #1#))))) (t t t t))"#
        ]],
    )
}

fn anki_connect_protocol_error_rejects_a_note_without_mutating_collection_state() -> ParityBatchCase
{
    ParityBatchCase::value(
        "anki_connect_protocol_error_rejects_a_note_without_mutating_collection_state",
        r##"(let (requests response-buffer)
                           (unwind-protect
                               (cl-letf
                                   (((symbol-function
                                      'url-retrieve-synchronously)
                                     (lambda (_url &rest _arguments)
                                       (setq requests
                                             (append
                                              requests
                                              (list
                                               (json-read-from-string
                                                url-request-data))))
                                       (setq response-buffer
                                             (generate-new-buffer
                                              " *anki-connect-error-response*"))
                                       (with-current-buffer response-buffer
                                         (insert
                                          "HTTP/1.1 200 OK\n"
                                          "Content-Type: application/json\n"
                                          "\n"
                                          "{\"result\":null,\"error\":\"collection is unavailable\"}"))
                                       response-buffer)))
                                 (list
                                  (anki-connect-add-note
                                   "Inbox"
                                   "Basic"
                                   '(("Front"
                                      . "question")
                                     ("Back"
                                      . "answer")))
                                  requests
                                  (buffer-live-p
                                   response-buffer)))
                             (when
                                 (buffer-live-p response-buffer)
                               (kill-buffer response-buffer))))"##,
        expect![[
            r#"OK (nil (((action . "addNote") (version . 6) (params (note (deckName . "Inbox") (modelName . "Basic") (fields (Front . "question") (Back . "answer")) (tags . []))))) t)"#
        ]],
    )
}

pub(super) fn workflows_anki_connect_missing_dependency_batch_cases() -> Vec<ParityBatchCase> {
    vec![anki_connect_declared_install_cannot_create_a_hierarchical_deck_without_s()]
}

pub(super) fn workflows_anki_connect_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        anki_connect_creates_missing_deck_hierarchy_once_through_the_http_api(),
        anki_connect_discovers_a_model_then_adds_and_updates_a_realistic_note(),
        anki_connect_protocol_error_rejects_a_note_without_mutating_collection_state(),
    ]
}
