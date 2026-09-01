use expect_test::expect;

use super::ParityBatchCase;

fn opening_the_menu_refreshes_real_decks_models_and_fields_before_rendering() -> ParityBatchCase {
    ParityBatchCase::value(
        "opening_the_menu_refreshes_real_decks_models_and_fields_before_rendering",
        r##"(let ((anki-mode--decks nil)
      (anki-mode--card-types nil)
      (anki-mode--previous-deck nil)
      (anki-mode--previous-card-type nil)
      requests
      menu-buffer)
  (unwind-protect
      (cl-letf
          (((symbol-function 'request)
            (lambda (_url &rest settings)
              (let* ((json-object-type 'alist)
                     (json-array-type 'list)
                     (request
                      (json-read-from-string (plist-get settings :data)))
                     (action (cdr (assq 'action request)))
                     (params (cdr (assq 'params request)))
                     (model (and params (cdr (assq 'modelName params))))
                     (result
                      (cond
                       ((equal action "version") 6)
                       ((equal action "deckNames")
                        ["Computing" "Languages::Japanese"])
                       ((equal action "modelNames")
                        ["Basic" "Cloze"])
                       ((equal model "Basic") ["Front" "Back"])
                       ((equal model "Cloze") ["Text" "Extra"])
                       (t (error "Unexpected AnkiConnect request: %S" request)))))
                (push request requests)
                (funcall
                 (plist-get settings :success)
                 :data `((result . ,result) (error)))
                nil))))
        (anki-mode-menu)
        (setq menu-buffer (current-buffer))
        (list
         (nreverse requests)
         anki-mode--decks
         anki-mode--card-types
         major-mode
         (buffer-substring-no-properties (point-min) (point-max))))
    (when (buffer-live-p menu-buffer)
      (kill-buffer menu-buffer))))"##,
        expect![[
            r#"OK ((((action . "version") (version . 6)) ((action . "deckNames") (version . 6)) ((action . "modelNames") (version . 6)) ((action . "modelFieldNames") (version . 6) (params (modelName . "Basic"))) ((action . "modelFieldNames") (version . 6) (params (modelName . "Cloze")))) ("Computing" "Languages::Japanese") (("Cloze" "Text" "Extra") ("Basic" "Front" "Back")) anki-mode-menu-mode "Anki Mode\n---------------\n[n]: New card\n[a]: New card with current settings (deck: 'NULL', card type: 'NULL')\n[r]: Refresh decks list\n\n\n\nDecks\n---------------\n* Computing\n* Languages::Japanese\n")"#
        ]],
    )
}

fn authors_a_basic_card_with_math_and_sends_the_complete_ankiconnect_note() -> ParityBatchCase {
    ParityBatchCase::value(
        "authors_a_basic_card_with_math_and_sends_the_complete_ankiconnect_note",
        r##"(let ((anki-mode--decks '("Computer Science"))
      (anki-mode--card-types
       '(("Basic" "Front" "Back")
         ("Cloze" "Text" "Extra")))
      (anki-mode--previous-deck nil)
      (anki-mode--previous-card-type nil)
      (choices '("Computer Science" "Basic"))
      requests
      created-messages
      card-buffer
      card-file
      menu-buffer)
  (unwind-protect
      (cl-letf
          (((symbol-function 'completing-read)
            (lambda (&rest _arguments)
              (pop choices)))
           ((symbol-function 'request)
            (lambda (_url &rest settings)
              (let* ((json-object-type 'alist)
                     (json-array-type 'list)
                     (request
                      (json-read-from-string (plist-get settings :data))))
                (push request requests)
                (funcall
                 (plist-get settings :success)
                 :data '((result . [555001]) (error)))
                nil)))
           ((symbol-function 'message)
            (lambda (format-string &rest arguments)
              (let ((rendered (apply #'format format-string arguments)))
                (when (string-prefix-p "Created card" rendered)
                  (push rendered created-messages))
                rendered))))
        (let ((anki-mode-markdown-command "cat"))
          (anki-mode-new-card)
          (setq card-buffer (current-buffer)
                card-file buffer-file-name)
          (insert "What is the complexity of binary search over O(log n) items?")
          (search-backward "O(log n)")
          (goto-char (match-beginning 0))
          (push-mark (match-end 0) t t)
          (anki-mode-insert-latex-math)
          (anki-mode-next-field)
          (insert
           "It halves the remaining sorted search space after each comparison.")
          (let ((authored
                 (buffer-substring-no-properties (point-min) (point-max))))
            (anki-mode-send-new-card)
            (setq menu-buffer (current-buffer))
            (let ((saved
                   (with-temp-buffer
                     (insert-file-contents card-file)
                     (buffer-string))))
              (list
               anki-mode--previous-deck
               anki-mode--previous-card-type
               authored
               saved
               (nreverse requests)
               (buffer-substring-no-properties (point-min) (point-max))
               (nreverse created-messages))))))
    (when (buffer-live-p card-buffer)
      (with-current-buffer card-buffer
        (set-buffer-modified-p nil))
      (kill-buffer card-buffer))
    (when (buffer-live-p menu-buffer)
      (kill-buffer menu-buffer))
    (when (and card-file (file-exists-p card-file))
      (delete-file card-file))))"##,
        expect![[
            r#"OK ("Computer Science" "Basic" "@Front\nWhat is the complexity of binary search over [$][/$]O(log n) items?\n\n@Back\nIt halves the remaining sorted search space after each comparison.\n\n" "@Front\nWhat is the complexity of binary search over [$][/$]O(log n) items?\n\n@Back\nIt halves the remaining sorted search space after each comparison.\n\n" (((action . "addNotes") (version . 6) (params (notes ((deckName . "Computer Science") (modelName . "Basic") (tags) (options (allowDuplicate . :json-false)) (fields (Front . "What is the complexity of binary search over [$][/$]O(log n) items?") (Back . "It halves the remaining sorted search space after each comparison."))))))) "Anki Mode\n---------------\n[n]: New card\n[a]: New card with current settings (deck: 'Computer Science', card type: 'Basic')\n[r]: Refresh decks list\n\n\n\nDecks\n---------------\n* Computer Science\n" ("Created card, got back [555001]"))"#
        ]],
    )
}

fn creates_sequential_clozes_and_sends_the_edited_card_fields() -> ParityBatchCase {
    ParityBatchCase::value(
        "creates_sequential_clozes_and_sends_the_edited_card_fields",
        r##"(let ((anki-mode--decks '("Biology"))
      (anki-mode--card-types '(("Cloze" "Text" "Extra")))
      requests
      card-buffer
      card-file
      menu-buffer)
  (unwind-protect
      (cl-letf
          (((symbol-function 'request)
            (lambda (_url &rest settings)
              (let* ((json-object-type 'alist)
                     (json-array-type 'list)
                     (request
                      (json-read-from-string (plist-get settings :data))))
                (push request requests)
                (funcall
                 (plist-get settings :success)
                 :data '((result . [777002]) (error)))
                nil))))
        (let ((anki-mode-markdown-command "cat"))
          (anki-mode-new-card-noninteractive "Biology" "Cloze")
          (setq card-buffer (current-buffer)
                card-file buffer-file-name)
          (insert "The mitochondria produce ATP during cellular respiration.")
          (search-backward "mitochondria")
          (anki-mode-cloze-region (match-beginning 0) (match-end 0))
          (search-forward "ATP")
          (anki-mode-cloze-region (match-beginning 0) (match-end 0))
          (anki-mode-next-field)
          (insert "Review the electron transport chain and chemiosmosis.")
          (let ((authored
                 (buffer-substring-no-properties (point-min) (point-max))))
            (anki-mode-send-new-card)
            (setq menu-buffer (current-buffer))
            (list authored (nreverse requests)))))
    (when (buffer-live-p card-buffer)
      (with-current-buffer card-buffer
        (set-buffer-modified-p nil))
      (kill-buffer card-buffer))
    (when (buffer-live-p menu-buffer)
      (kill-buffer menu-buffer))
    (when (and card-file (file-exists-p card-file))
      (delete-file card-file))))"##,
        expect![[
            r#"OK ("@Text\nThe {{c1::mitochondria}} produce {{c2::ATP}} during cellular respiration.\n\n@Extra\nReview the electron transport chain and chemiosmosis.\n\n" (((action . "addNotes") (version . 6) (params (notes ((deckName . "Biology") (modelName . "Cloze") (tags) (options (allowDuplicate . :json-false)) (fields (Text . "The {{c1::mitochondria}} produce {{c2::ATP}} during cellular respiration.") (Extra . "Review the electron transport chain and chemiosmosis."))))))))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        opening_the_menu_refreshes_real_decks_models_and_fields_before_rendering(),
        authors_a_basic_card_with_math_and_sends_the_complete_ankiconnect_note(),
        creates_sequential_clozes_and_sends_the_edited_card_fields(),
    ]
}
