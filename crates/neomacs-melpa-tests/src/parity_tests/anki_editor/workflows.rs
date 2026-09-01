use expect_test::expect;

use super::ParityBatchCase;

fn pushes_a_readme_style_card_with_real_html_and_media_through_ankiconnect() -> ParityBatchCase {
    ParityBatchCase::value(
        "pushes_a_readme_style_card_with_real_html_and_media_through_ankiconnect",
        r##"(let* ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (project (expand-file-name "anki-editor-create" sandbox))
         (cards-file (expand-file-name "cards.org" project))
         (image-file (expand-file-name "osi.gif" project))
         requests)
    (when (file-directory-p project)
      (delete-directory project t))
    (make-directory project t)
    (with-temp-file image-file
      (set-buffer-multibyte nil)
      (insert "GIF89a"))
    (with-temp-file cards-file
      (insert
       "* Network layers :study:noexport:\n"
       ":PROPERTIES:\n"
       ":ANKI_DECK: Computing::Networks\n"
       ":ANKI_NOTE_TYPE: Basic\n"
       ":ANKI_TAGS: exam osi\n"
       ":END:\n"
       "** Front\n"
       "Which layer provides end-to-end delivery?\n"
       "** Back\n"
       "The *transport layer*.\n\n"
       "[[file:osi.gif][OSI diagram]]\n"))
    (let ((buffer (find-file-noselect cards-file)))
      (unwind-protect
          (with-current-buffer buffer
            (org-mode)
            (goto-char (point-min))
            (let ((anki-editor--collection-data-updated nil)
                  (anki-editor--api-active-queue 1)
                  (anki-editor--api-request-queue-1 nil)
                  (anki-editor--api-request-queue-2 nil)
                  (anki-editor-export-note-fields-on-push t)
                  (anki-editor-org-tags-as-anki-tags t)
                  (anki-editor-ignored-org-tags '("noexport")))
              (cl-letf
                  (((symbol-function 'call-process)
                    (lambda (_program _infile _destination _display &rest arguments)
                      (let* ((request-file
                              (substring (car (last arguments)) 1))
                             (payload
                              (with-temp-buffer
                                (insert-file-contents request-file)
                                (buffer-string)))
                             (json-array-type 'list)
                             (request (json-read-from-string payload))
                             (action (alist-get 'action request))
                             response)
                        (push request requests)
                        (setq response
                              (cond
                               ((equal action "modelNames")
                                '((result "Basic") (error)))
                               ((and (equal action "multi")
                                     (string-match-p
                                      "\"action\":\"modelFieldNames\"" payload))
                                '((result ((result "Front" "Back") (error)))
                                  (error)))
                               ((equal action "retrieveMediaFile")
                                '((result . :json-false) (error)))
                               ((equal action "storeMediaFile")
                                `((result
                                   . ,(alist-get
                                       'filename
                                       (alist-get 'params request)))
                                  (error)))
                               ((and (equal action "multi")
                                     (string-match-p "\"action\":\"addNote\"" payload))
                                '((result
                                   ((result) (error))
                                   ((result . 424242) (error)))
                                  (error)))
                               (t (error "Unexpected AnkiConnect request: %s" payload))))
                        (insert (json-encode response))
                        0))))
                (anki-editor-push-note-at-point))
              (let ((buffer-text
                     (buffer-substring-no-properties (point-min) (point-max)))
                    (disk-text
                     (with-temp-buffer
                       (insert-file-contents cards-file)
                       (buffer-string))))
                (list (nreverse requests) buffer-text disk-text))))
        (when (buffer-live-p buffer)
          (with-current-buffer buffer
            (set-buffer-modified-p nil))
          (kill-buffer buffer)))))"##,
        expect![[
            r#"OK ((((action . "modelNames") (version . 6)) ((action . "multi") (version . 6) (params (actions ((action . "modelFieldNames") (version . 6) (params (modelName . "Basic")))))) ((action . "retrieveMediaFile") (version . 6) (params (filename . "osi-25c9b37ae36a0a08318d4dca7ca57ea98d776821.gif"))) ((action . "storeMediaFile") (version . 6) (params (filename . "osi-25c9b37ae36a0a08318d4dca7ca57ea98d776821.gif") (data . "R0lGODlh"))) ((action . "multi") (version . 6) (params (actions ((action . "createDeck") (version . 6) (params (deck . "Computing::Networks"))) ((action . "addNote") (version . 6) (params (note (id . 0) (deckName . "Computing::Networks") (modelName . "Basic") (fields (Back . "<p>\nThe <b>transport layer</b>.\n</p>\n\n<p>\n<a href=\"osi-25c9b37ae36a0a08318d4dca7ca57ea98d776821.gif\">OSI diagram</a>\n</p>\n") (Front . "<p>\nWhich layer provides end-to-end delivery?\n</p>\n")) (options (allowDuplicate . :json-false)) (tags "exam" "osi" "study")))))))) "* Network layers :study:noexport:\n:PROPERTIES:\n:ANKI_DECK: Computing::Networks\n:ANKI_NOTE_TYPE: Basic\n:ANKI_TAGS: exam osi\n:ANKI_NOTE_ID: 424242\n:ANKI_NOTE_HASH: 228e4bb11039df47d3051bb539dcc795\n:END:\n** Front\nWhich layer provides end-to-end delivery?\n** Back\nThe *transport layer*.\n\n[[file:osi.gif][OSI diagram]]\n" "* Network layers :study:noexport:\n:PROPERTIES:\n:ANKI_DECK: Computing::Networks\n:ANKI_NOTE_TYPE: Basic\n:ANKI_TAGS: exam osi\n:ANKI_NOTE_ID: 424242\n:ANKI_NOTE_HASH: 228e4bb11039df47d3051bb539dcc795\n:END:\n** Front\nWhich layer provides end-to-end delivery?\n** Back\nThe *transport layer*.\n\n[[file:osi.gif][OSI diagram]]\n")"#
        ]],
    )
}

fn updates_an_existing_card_and_preserves_server_managed_tags() -> ParityBatchCase {
    ParityBatchCase::value(
        "updates_an_existing_card_and_preserves_server_managed_tags",
        r##"(let* ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (project (expand-file-name "anki-editor-update" sandbox))
         (cards-file (expand-file-name "cards.org" project))
         requests)
    (when (file-directory-p project)
      (delete-directory project t))
    (make-directory project t)
    (with-temp-file cards-file
      (insert
       "* TCP handshake :review:\n"
       ":PROPERTIES:\n"
       ":ANKI_DECK: Computing::Networks\n"
       ":ANKI_NOTE_TYPE: Basic\n"
       ":ANKI_NOTE_ID: 9001\n"
       ":ANKI_NOTE_HASH: stale-hash\n"
       ":ANKI_TAGS: current\n"
       ":END:\n"
       "** Front\n"
       "What is the TCP handshake?\n"
       "** Back\n"
       "SYN, SYN-ACK, ACK.\n"))
    (let ((buffer (find-file-noselect cards-file)))
      (unwind-protect
          (with-current-buffer buffer
            (org-mode)
            (goto-char (point-min))
            (let ((anki-editor--collection-data-updated nil)
                  (anki-editor--api-active-queue 1)
                  (anki-editor--api-request-queue-1 nil)
                  (anki-editor--api-request-queue-2 nil)
                  (anki-editor-export-note-fields-on-push t)
                  (anki-editor-org-tags-as-anki-tags t)
                  (anki-editor-protected-tags '("protected")))
              (cl-letf
                  (((symbol-function 'call-process)
                    (lambda (_program _infile _destination _display &rest arguments)
                      (let* ((request-file
                              (substring (car (last arguments)) 1))
                             (payload
                              (with-temp-buffer
                                (insert-file-contents request-file)
                                (buffer-string)))
                             (json-array-type 'list)
                             (request (json-read-from-string payload))
                             (action (alist-get 'action request))
                             response)
                        (push request requests)
                        (setq response
                              (cond
                               ((equal action "modelNames")
                                '((result "Basic") (error)))
                               ((and (equal action "multi")
                                     (string-match-p
                                      "\"action\":\"modelFieldNames\"" payload))
                                '((result ((result "Front" "Back") (error)))
                                  (error)))
                               ((string-match-p "\"action\":\"notesInfo\"" payload)
                                '((result
                                   ((result
                                     ((cards 71 72)
                                      (tags "protected" "obsolete")))
                                    (error)))
                                  (error)))
                               ((string-match-p "\"action\":\"updateNote\"" payload)
                                '((result ((result) (error))) (error)))
                               ((string-match-p "\"action\":\"changeDeck\"" payload)
                                '((result ((result) (error))) (error)))
                               (t (error "Unexpected AnkiConnect request: %s" payload))))
                        (insert (json-encode response))
                        0))))
                (anki-editor-push-note-at-point))
              (let ((buffer-text
                     (buffer-substring-no-properties (point-min) (point-max)))
                    (disk-text
                     (with-temp-buffer
                       (insert-file-contents cards-file)
                       (buffer-string))))
                (list (nreverse requests) buffer-text disk-text))))
        (when (buffer-live-p buffer)
          (with-current-buffer buffer
            (set-buffer-modified-p nil))
          (kill-buffer buffer)))))"##,
        expect![[
            r#"OK ((((action . "modelNames") (version . 6)) ((action . "multi") (version . 6) (params (actions ((action . "modelFieldNames") (version . 6) (params (modelName . "Basic")))))) ((action . "multi") (version . 6) (params (actions ((action . "notesInfo") (version . 6) (params (notes 9001)))))) ((action . "multi") (version . 6) (params (actions ((action . "updateNote") (version . 6) (params (note (id . 9001) (deckName . "Computing::Networks") (modelName . "Basic") (fields (Back . "<p>\nSYN, SYN-ACK, ACK.\n</p>\n") (Front . "<p>\nWhat is the TCP handshake?\n</p>\n")) (options (allowDuplicate . :json-false)) (tags "protected" "current" "review"))))))) ((action . "multi") (version . 6) (params (actions ((action . "changeDeck") (version . 6) (params (deck . "Computing::Networks") (cards 71 72))))))) "* TCP handshake :review:\n:PROPERTIES:\n:ANKI_DECK: Computing::Networks\n:ANKI_NOTE_TYPE: Basic\n:ANKI_NOTE_ID: 9001\n:ANKI_NOTE_HASH: 02d71d167a5678f4ffed4582a61777f1\n:ANKI_TAGS: current\n:END:\n** Front\nWhat is the TCP handshake?\n** Back\nSYN, SYN-ACK, ACK.\n" "* TCP handshake :review:\n:PROPERTIES:\n:ANKI_DECK: Computing::Networks\n:ANKI_NOTE_TYPE: Basic\n:ANKI_NOTE_ID: 9001\n:ANKI_NOTE_HASH: 02d71d167a5678f4ffed4582a61777f1\n:ANKI_TAGS: current\n:END:\n** Front\nWhat is the TCP handshake?\n** Back\nSYN, SYN-ACK, ACK.\n")"#
        ]],
    )
}

fn records_a_rejected_note_without_silently_saving_the_failed_edit() -> ParityBatchCase {
    ParityBatchCase::value(
        "records_a_rejected_note_without_silently_saving_the_failed_edit",
        r##"(let* ((sandbox (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
         (project (expand-file-name "anki-editor-rejection" sandbox))
         (cards-file (expand-file-name "cards.org" project))
         requests)
    (when (file-directory-p project)
      (delete-directory project t))
    (make-directory project t)
    (with-temp-file cards-file
      (insert
       "* Duplicate card\n"
       ":PROPERTIES:\n"
       ":ANKI_DECK: Computing\n"
       ":ANKI_NOTE_TYPE: Basic\n"
       ":END:\n"
       "** Front\n"
       "What does DNS do?\n"
       "** Back\n"
       "It resolves names to addresses.\n"))
    (let ((original
           (with-temp-buffer
             (insert-file-contents cards-file)
             (buffer-string)))
          (buffer (find-file-noselect cards-file)))
      (unwind-protect
          (with-current-buffer buffer
            (org-mode)
            (goto-char (point-min))
            (let ((anki-editor--collection-data-updated nil)
                  (anki-editor--api-active-queue 1)
                  (anki-editor--api-request-queue-1 nil)
                  (anki-editor--api-request-queue-2 nil)
                  (anki-editor-export-note-fields-on-push t))
              (cl-letf
                  (((symbol-function 'call-process)
                    (lambda (_program _infile _destination _display &rest arguments)
                      (let* ((request-file
                              (substring (car (last arguments)) 1))
                             (payload
                              (with-temp-buffer
                                (insert-file-contents request-file)
                                (buffer-string)))
                             (json-array-type 'list)
                             (request (json-read-from-string payload))
                             (action (alist-get 'action request))
                             response)
                        (push request requests)
                        (setq response
                              (cond
                               ((equal action "modelNames")
                                '((result "Basic") (error)))
                               ((and (equal action "multi")
                                     (string-match-p
                                      "\"action\":\"modelFieldNames\"" payload))
                                '((result ((result "Front" "Back") (error)))
                                  (error)))
                               ((string-match-p "\"action\":\"addNote\"" payload)
                                '((result
                                   ((result) (error))
                                   ((result) (error . "duplicate note")))
                                  (error)))
                               (t (error "Unexpected AnkiConnect request: %s" payload))))
                        (insert (json-encode response))
                        0))))
                (let ((failure
                       (condition-case error-data
                           (anki-editor-push-note-at-point)
                         (error error-data)))
                      (buffer-text
                       (buffer-substring-no-properties (point-min) (point-max)))
                      (disk-text
                       (with-temp-buffer
                         (insert-file-contents cards-file)
                         (buffer-string))))
                  (list
                   failure
                   (nreverse requests)
                   buffer-text
                   (equal disk-text original)
                   disk-text)))))
        (when (buffer-live-p buffer)
          (with-current-buffer buffer
            (set-buffer-modified-p nil))
          (kill-buffer buffer)))))"##,
        expect![[
            r#"OK ((user-error "Push failed; see ANKI_FAILURE_REASON property") (((action . "modelNames") (version . 6)) ((action . "multi") (version . 6) (params (actions ((action . "modelFieldNames") (version . 6) (params (modelName . "Basic")))))) ((action . "multi") (version . 6) (params (actions ((action . "createDeck") (version . 6) (params (deck . "Computing"))) ((action . "addNote") (version . 6) (params (note (id . 0) (deckName . "Computing") (modelName . "Basic") (fields (Back . "<p>\nIt resolves names to addresses.\n</p>\n") (Front . "<p>\nWhat does DNS do?\n</p>\n")) (options (allowDuplicate . :json-false)) (tags)))))))) "* Duplicate card\n:PROPERTIES:\n:ANKI_DECK: Computing\n:ANKI_NOTE_TYPE: Basic\n:ANKI_FAILURE_REASON: duplicate note\n:END:\n** Front\nWhat does DNS do?\n** Back\nIt resolves names to addresses.\n" t "* Duplicate card\n:PROPERTIES:\n:ANKI_DECK: Computing\n:ANKI_NOTE_TYPE: Basic\n:END:\n** Front\nWhat does DNS do?\n** Back\nIt resolves names to addresses.\n")"#
        ]],
    )
}

fn creates_and_exports_a_cloze_card_from_an_actual_org_edit() -> ParityBatchCase {
    ParityBatchCase::value(
        "creates_and_exports_a_cloze_card_from_an_actual_org_edit",
        r##"(with-temp-buffer
    (org-mode)
    (insert
     "* OSI model :networking:\n"
     ":PROPERTIES:\n"
     ":ANKI_DECK: Computing::Networks\n"
     ":ANKI_NOTE_TYPE: Cloze\n"
     ":ANKI_TAGS: exam\n"
     ":END:\n"
     "** Text\n"
     "The transport layer provides end-to-end delivery.\n"
     "** Extra\n"
     "Remember TCP and UDP.\n")
    (goto-char (point-min))
    (search-forward "transport layer")
    (set-mark (match-beginning 0))
    (goto-char (match-end 0))
    (activate-mark)
    (anki-editor-cloze-region 2 "OSI layer")
    (goto-char (point-min))
    (let ((anki-editor--collection-data-updated t)
          (anki-editor--model-fields
           '(("Cloze" "Text" "Extra")))
          (anki-editor-export-note-fields-on-push t)
          (anki-editor-org-tags-as-anki-tags t))
      (let* ((edited-org
              (buffer-substring-no-properties (point-min) (point-max)))
             (note (anki-editor-note-at-point))
             (anki-payload (anki-editor-api--note note)))
        (list edited-org anki-payload))))"##,
        expect![[
            r#"OK ("* OSI model :networking:\n:PROPERTIES:\n:ANKI_DECK: Computing::Networks\n:ANKI_NOTE_TYPE: Cloze\n:ANKI_TAGS: exam\n:END:\n** Text\nThe {{c2::transport layer::OSI layer}} provides end-to-end delivery.\n** Extra\nRemember TCP and UDP.\n" (:id 0 :deckName "Computing::Networks" :modelName "Cloze" :fields (("Extra" . "<p>\nRemember TCP and UDP.\n</p>\n") ("Text" . "<p>\nThe {{c2::transport layer::OSI layer}} provides end-to-end delivery.\n</p>\n")) :options (:allowDuplicate :json-false) :tags ["exam" "networking"]))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        pushes_a_readme_style_card_with_real_html_and_media_through_ankiconnect(),
        updates_an_existing_card_and_preserves_server_managed_tags(),
        records_a_rejected_note_without_silently_saving_the_failed_edit(),
        creates_and_exports_a_cloze_card_from_an_actual_org_edit(),
    ]
}
