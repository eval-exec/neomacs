use expect_test::expect;

use super::ParityBatchCase;

fn configures_a_real_anki_model_from_the_server_catalog() -> ParityBatchCase {
    ParityBatchCase::value(
        "configures_a_real_anki_model_from_the_server_catalog",
        r##"(let ((answers
       '("Language::English"
         "Vocabulary"
         "${expression:单词}"
         "${glossary:释义}"
         "${phonetic:音标}"
         "${sentence:原文例句}"
         "${sentence_bold:标粗的原文例句}"
         "${translation:翻译例句}"
         "${sound:发声}"
         "SKIP"))
      requests
      response-buffers)
  (unwind-protect
      (cl-letf
          (((symbol-function 'url-retrieve-synchronously)
            (lambda (_url)
              (let* ((json-object-type 'alist)
                     (json-array-type 'list)
                     (request
                      (json-read-from-string
                       (decode-coding-string url-request-data 'utf-8)))
                     (action (cdr (assq 'action request)))
                     (result
                      (pcase action
                        ("deckNames" ["Inbox" "Language::English"])
                        ("modelNames" ["Basic" "Vocabulary"])
                        ("modelFieldNames"
                         ["Word" "Meaning" "IPA" "Context" "Highlighted"
                          "Translation" "Audio" "Notes"])
                        (_ (error "Unexpected AnkiConnect request: %S" request))))
                     (buffer (generate-new-buffer " *anki-vocabulary-http*")))
                (push request requests)
                (push buffer response-buffers)
                (with-current-buffer buffer
                  (insert
                   "HTTP/1.1 200 OK\nContent-Type: application/json\n\n"
                   (json-encode `((result . ,result) (error)))))
                buffer)))
           ((symbol-function 'completing-read)
            (lambda (&rest _arguments)
              (pop answers))))
        (anki-vocabulary-set-ankiconnect)
        (list
         (nreverse requests)
         anki-vocabulary-deck-name
         anki-vocabulary-model-name
         anki-vocabulary-field-alist
         anki-vocabulary-audio-fileds
         answers))
    (mapc
     (lambda (buffer)
       (when (buffer-live-p buffer)
         (kill-buffer buffer)))
     response-buffers)))"##,
        expect![[
            r#"OK ((((action . "deckNames") (version . 6)) ((action . "modelNames") (version . 6)) ((action . "modelFieldNames") (version . 6) (params (modelName . "Vocabulary")))) "Language::English" "Vocabulary" (("Translation" . "${translation:翻译例句}") ("Highlighted" . "${sentence_bold:标粗的原文例句}") ("Context" . "${sentence:原文例句}") ("IPA" . "${phonetic:音标}") ("Meaning" . "${glossary:释义}") ("Word" . "${expression:单词}")) ("Audio") nil)"#
        ]],
    )
}

fn selected_prose_becomes_a_complete_contextual_vocabulary_note_with_audio() -> ParityBatchCase {
    ParityBatchCase::value(
        "selected_prose_becomes_a_complete_contextual_vocabulary_note_with_audio",
        r##"(let ((anki-vocabulary-deck-name "Language::English")
      (anki-vocabulary-model-name "Vocabulary")
      (anki-vocabulary-field-alist
       '(("Word" . "${expression:单词}")
         ("Meaning" . "${glossary:释义}")
         ("IPA" . "/${phonetic:音标}/")
         ("Context" . "${sentence:原文例句}")
         ("Highlighted" . "${sentence_bold:标粗的原文例句}")
         ("Translation" . "${translation:翻译例句}")))
      (anki-vocabulary-audio-fileds '("Audio"))
      events
      requests
      response-buffers)
  (setq
   anki-vocabulary-sentence-translator
   (lambda (sentence)
     (push (list 'translate sentence) events)
     "练习带来进步；练习建立信心。")
   anki-vocabulary-word-searcher
   (lambda (word)
     (push (list 'lookup word) events)
     '((expression . "practice")
       (glossary . ("n. practice" "v. rehearse"))
       (phonetic . "ˈpræktɪs")))
   anki-vocabulary-before-addnote-functions
   (list
    (lambda (&rest arguments)
      (push (cons 'before-add arguments) events)))
   anki-vocabulary-after-addnote-functions
   (list
    (lambda (&rest arguments)
      (push (cons 'after-add arguments) events))))
  (unwind-protect
      (cl-letf
          (((symbol-function 'url-retrieve-synchronously)
            (lambda (_url)
              (let* ((json-object-type 'alist)
                     (json-array-type 'list)
                     (request
                      (json-read-from-string
                       (decode-coding-string url-request-data 'utf-8)))
                     (action (cdr (assq 'action request)))
                     (buffer (generate-new-buffer " *anki-vocabulary-http*")))
                (unless (equal action "addNote")
                  (error "Unexpected AnkiConnect request: %S" request))
                (push request requests)
                (push buffer response-buffers)
                (with-current-buffer buffer
                  (insert
                   "HTTP/1.1 200 OK\nContent-Type: application/json\n\n"
                   (json-encode '((result . 424242) (error)))))
                buffer)))
           ((symbol-function 'completing-read)
            (lambda (prompt collection &rest _arguments)
              (let ((choice
                     (if (string-prefix-p "Pick The Word:" prompt)
                         "practice"
                       (cadr collection))))
                (push (list 'choose prompt collection choice) events)
                choice))))
        (with-temp-buffer
          (text-mode)
          (transient-mark-mode 1)
          (insert
           "Practice makes progress;\n"
           "practice builds confidence.\n"
           "A second sentence should not enter the card.")
          (goto-char (point-min))
          (push-mark
           (save-excursion
             (search-forward "confidence.")
             (point))
           t t)
          (let ((selected
                 (buffer-substring-no-properties
                  (region-beginning) (region-end))))
            (anki-vocabulary)
            (list
             selected
             (buffer-substring-no-properties (point-min) (point-max))
             (nreverse requests)
             (nreverse events)))))
    (mapc
     (lambda (buffer)
       (when (buffer-live-p buffer)
         (kill-buffer buffer)))
     response-buffers)))"##,
        expect![[
            r#"OK ("Practice makes progress;\npractice builds confidence." "Practice makes progress;\npractice builds confidence.\nA second sentence should not enter the card." (((action . "addNote") (version . 6) (params (note (deckName . "Language::English") (modelName . "Vocabulary") (fields (Word . "practice") (Meaning . "v. rehearse") (IPA . "/ˈpræktɪs/") (Context . "Practice makes progress; practice builds confidence.") (Highlighted . "<B>Practice</B> makes progress; <b>practice</b> builds confidence.") (Translation . "练习带来进步；练习建立信心。")) (tags) (audio (url . "http://dict.youdao.com/dictvoice?type=2&audio=practice") (filename . "youdao-e8302675b1c057aa7eecf27f7b0e2c9f.mp3") (fields "Audio")))))) ((choose "Pick The Word: " ("Practice" "makes" "progress" "practice" "builds" "confidence" "") "practice") (translate "Practice makes progress; practice builds confidence.") (lookup "practice") (choose "练习带来进步；练习建立信心。(practice):" ("n. practice" "v. rehearse") "v. rehearse") (before-add "practice" "Practice makes progress; practice builds confidence." "<B>Practice</B> makes progress; <b>practice</b> builds confidence." "练习带来进步；练习建立信心。" "v. rehearse" "ˈpræktɪs") (after-add "practice" "Practice makes progress; practice builds confidence." "<B>Practice</B> makes progress; <b>practice</b> builds confidence." "练习带来进步；练习建立信心。" "v. rehearse" "ˈpræktɪs")))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        configures_a_real_anki_model_from_the_server_catalog(),
        selected_prose_becomes_a_complete_contextual_vocabulary_note_with_audio(),
    ]
}
