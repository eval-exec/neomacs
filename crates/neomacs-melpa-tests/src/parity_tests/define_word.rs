use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, DEFINE_WORD_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'define-word)

(defun neomacs-define-word-test-capture (function)
  (condition-case error-data
      (list :ok (funcall function))
    (error
     (list :error (car error-data)
           :data (cdr error-data)
           :message (error-message-string error-data)))))

(defun neomacs-define-word-test-runs (string)
  (let ((position 0) runs)
    (while (< position (length string))
      (let ((next (or (next-single-property-change position 'face string)
                      (length string))))
        (push (list (substring-no-properties string position next)
                    (get-text-property position 'face string))
              runs)
        (setq position next)))
    (nreverse runs)))

(defun neomacs-define-word-test-parse (html parser)
  (with-temp-buffer
    (insert html)
    (goto-char (point-min))
    (funcall parser)))
"####;

fn inflected_and_missing_terms_follow_the_configured_service_and_display_pipeline()
-> ParityBatchCase {
    let elisp_form = r####"
(let (requests displays prompts)
  (let* ((retriever
          (lambda (word)
            (push word requests)
            (cdr (assoc word
                        '(("widgets" . "Plural form of widget.")
                          ("widget" . "n. A configurable interface component.")
                          ("shipped" . "Past participle of ship.")
                          ("ship" . "v. To deliver a release.")
                          ("unknown" . nil))))))
         (display
          (lambda (definition)
            (push definition displays)
            definition))
         (define-word-services (list (list 'fixture retriever nil)))
         (define-word-default-service 'fixture)
         (define-word-displayfn-alist (list (cons 'fixture display))))
    (cl-letf (((symbol-function 'completing-read)
               (lambda (prompt collection &rest _arguments)
                 (push (list prompt (mapcar #'car collection)) prompts)
                 "fixture")))
      (list :plural (define-word "widgets" nil)
            :past (define-word "shipped" 'fixture)
            :missing (define-word "unknown" nil t)
            :requests (nreverse requests)
            :displays (nreverse displays)
            :prompts (nreverse prompts)))))
"####;
    let expected = expect![[
        r#"OK (:plural "Plural form of widget.\nwidget:\n  n. A configurable interface component." :past "Past participle of ship.\nship:\n  v. To deliver a release." :missing "0 definitions found" :requests ("widgets" "widget" "shipped" "ship" "unknown") :displays ("Plural form of widget.\nwidget:\n  n. A configurable interface component." "Past participle of ship.\nship:\n  v. To deliver a release." "0 definitions found") :prompts (("Service: " (fixture))))"#
    ]];
    ParityBatchCase::value(
        "inflected_and_missing_terms_follow_the_configured_service_and_display_pipeline",
        elisp_form,
        expected,
    )
}

fn word_region_and_pdf_commands_send_exact_user_selection_without_text_properties()
-> ParityBatchCase {
    let elisp_form = r####"
(let (calls)
  (cl-letf (((symbol-function 'define-word)
             (lambda (word service &optional choose-service)
               (push (list word service choose-service) calls))))
    (with-temp-buffer
      (insert (propertize "Deploy resilient systems today."
                          'face 'font-lock-keyword-face))
      (goto-char (point-min))
      (search-forward "resilient")
      (define-word-at-point nil 'fixture)
      (goto-char (point-min))
      (search-forward "Deploy")
      (forward-char 1)
      (set-mark (point))
      (search-forward "systems")
      (let ((transient-mark-mode t)
            (mark-active t))
        (define-word-at-point '(4) 'fixture)))
    (with-temp-buffer
      (let ((major-mode 'pdf-view-mode))
        (cl-letf (((symbol-function 'pdf-view-active-region-text)
                   (lambda () '("selected PDF phrase"))))
          (define-word-at-point nil 'fixture))))
    (nreverse calls)))
"####;
    let expected = expect![[
        r#"OK (("resilient" fixture nil) ("resilient systems" fixture (4)) ("selected PDF phrase" fixture nil))"#
    ]];
    ParityBatchCase::value(
        "word_region_and_pdf_commands_send_exact_user_selection_without_text_properties",
        elisp_form,
        expected,
    )
}

fn synchronous_url_services_downcase_queries_select_user_agents_and_parse_response_buffers()
-> ParityBatchCase {
    let elisp_form = r####"
(let ((network-buffer (generate-new-buffer " *define-word-network*"))
      requests inserts)
  (unwind-protect
      (let ((define-word-services
             '((wordnik "https://dictionary.invalid/words/%s"
                        (lambda () (concat "wordnik:" (buffer-string))))
               (glossary "https://glossary.invalid/entry/%s"
                         (lambda () (concat "glossary:" (buffer-string))))))
            (url-user-agent "Neomacs-Parity-Agent/1.0"))
        (cl-letf (((symbol-function 'url-retrieve-synchronously)
                   (lambda (url silent inhibit-cookies)
                     (push (list url silent inhibit-cookies url-user-agent)
                           requests)
                     network-buffer))
                  ((symbol-function 'url-insert-buffer-contents)
                   (lambda (buffer url)
                     (push (list (eq buffer network-buffer) url) inserts)
                     (insert "definition payload"))))
          (list :wordnik (define-word--to-string "Release Train" 'wordnik)
                :glossary (define-word--to-string "Release Train" 'glossary)
                :requests (nreverse requests)
                :inserts (nreverse inserts))))
    (when (buffer-live-p network-buffer) (kill-buffer network-buffer))))
"####;
    let expected = expect![[
        r#"OK (:wordnik "wordnik:definition payload" :glossary "glossary:definition payload" :requests (("https://dictionary.invalid/words/release train" t t "Mozilla/5.0 (Macintosh; Intel Mac OS X 11_5_2) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/93.0.4577.63 Safari/537.36") ("https://glossary.invalid/entry/release train" t t "Neomacs-Parity-Agent/1.0")) :inserts ((t "https://dictionary.invalid/words/release train") (t "https://glossary.invalid/entry/release train")))"#
    ]];
    ParityBatchCase::value(
        "synchronous_url_services_downcase_queries_select_user_agents_and_parse_response_buffers",
        elisp_form,
        expected,
    )
}

fn wordnik_html_parser_preserves_parts_of_speech_semantic_styles_and_result_limit()
-> ParityBatchCase {
    let elisp_form = r####"
(let* ((define-word-limit 2)
       (result
        (neomacs-define-word-test-parse
         (concat
          "<ul>"
          "<li><abbr title='noun'>n.</abbr> A <em>small</em> "
          "<strong>release</strong> artifact.</li>"
          "<li><abbr title='verb'>v.</abbr> To <xref>publish</xref> safely.</li>"
          "<li><abbr title='noun'>n.</abbr> A result beyond the limit.</li>"
          "</ul>")
         #'define-word--parse-wordnik)))
  (list :plain (substring-no-properties result)
        :runs (neomacs-define-word-test-runs result)))
"####;
    let expected = expect![[
        r#"OK (:plain "n. A small release artifact.\nv. To publish safely." :runs (("n. " define-word-face-1) ("A " define-word-face-2) ("small" italic) (" " define-word-face-2) ("release" bold) (" artifact." define-word-face-2) ("\n" nil) ("v. " define-word-face-1) ("To " define-word-face-2) ("publish" link) (" safely." define-word-face-2)))"#
    ]];
    ParityBatchCase::value(
        "wordnik_html_parser_preserves_parts_of_speech_semantic_styles_and_result_limit",
        elisp_form,
        expected,
    )
}

fn webster_parser_maps_grammar_labels_and_limits_a_real_definition_page() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((define-word-limit 4)
       (result
        (neomacs-define-word-test-parse
         (concat
          "<article>"
          "<p><strong>RELEASE</strong>, <em>noun</em></p>\n"
          "<p><strong>1.</strong>A durable published artifact.</p>\n"
          "<p><strong>RELEASE</strong>, <em>verb transitive</em></p>\n"
          "<p><strong>2.</strong>To publish a prepared artifact.</p>\n"
          "<p><strong>RELEASE</strong>, <em>adjective</em></p>\n"
          "<p><strong>3.</strong>Prepared for publication.</p>\n"
          "</article>")
         #'define-word--parse-webster)))
  (list :plain (substring-no-properties result)
        :runs (neomacs-define-word-test-runs result)))
"####;
    let expected = expect![[
        r#"OK (:plain "n., noun\nn.A durable published artifact.\nvt., verb transitive\nvt.To publish a prepared artifact." :runs (("n." bold) (", " nil) ("noun" italic) ("\n" nil) ("n." bold) ("A durable published artifact.\n" nil) ("vt." bold) (", " nil) ("verb transitive" italic) ("\n" nil) ("vt." bold) ("To publish a prepared artifact." nil)))"#
    ]];
    ParityBatchCase::value(
        "webster_parser_maps_grammar_labels_and_limits_a_real_definition_page",
        elisp_form,
        expected,
    )
}

fn openthesaurus_parser_removes_superscripts_trims_synonyms_and_honors_the_limit() -> ParityBatchCase
{
    let elisp_form = r####"
(let ((define-word-limit 2))
  (neomacs-define-word-test-parse
   (concat
    "<?xml version='1.0'?><root>"
    "<sup>usage metadata</sup>"
    "<span class='wiktionaryItem'> 1.</span> rapid, swift <br/>"
    "<span class='wiktionaryItem'> 2.</span> resilient, robust <br/>"
    "<span class='wiktionaryItem'> 3.</span> durable <br/>"
    "</root>")
   #'define-word--parse-openthesaurus))
"####;
    let expected = expect![[r#"OK "rapid, swift\nresilient, robust""#]];
    ParityBatchCase::value(
        "openthesaurus_parser_removes_superscripts_trims_synonyms_and_honors_the_limit",
        elisp_form,
        expected,
    )
}

fn offline_dictionary_search_returns_matching_entries_and_actionable_configuration_errors()
-> ParityBatchCase {
    let elisp_form = r####"
(let* ((root (make-temp-file
              (expand-file-name "define-word-offline-"
                                (file-name-as-directory (getenv "TMPDIR")))
              t))
       (dictionary (expand-file-name "en-en-withforms-enwiktionary.txt" root)))
  (unwind-protect
      (progn
        (with-temp-file dictionary
          (insert "harbor {n} a sheltered place for ships\n"
                  "release {v} to publish an artifact\n"
                  "harboring {v} giving shelter\n"))
        (list :matches
              (let ((define-word-offline-dict-directory root))
                (define-word--get-offline-wikitionary "harbor"))
              :missing-config
              (let ((define-word-offline-dict-directory nil))
                (neomacs-define-word-test-capture
                 (lambda ()
                   (define-word--get-offline-wikitionary "harbor"))))))
    (when (file-directory-p root) (delete-directory root t))))
"####;
    let expected = expect![[
        r#"OK (:matches "harbor {n} a sheltered place for ships\n" :missing-config (:error user-error :data ("Please download the ding (text-format) zip from https://en.wiktionary.org/wiki/User:Matthias_Buchmeier/download and configure ‘define-word-offline-dict-directory’.") :message "Please download the ding (text-format) zip from https://en.wiktionary.org/wiki/User:Matthias_Buchmeier/download and configure ‘define-word-offline-dict-directory’."))"#
    ]];
    ParityBatchCase::value(
        "offline_dictionary_search_returns_matching_entries_and_actionable_configuration_errors",
        elisp_form,
        expected,
    )
}

#[test]
fn define_word_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(DEFINE_WORD_MELPA_PIN, "define-word.el")
            .expect("prepare revision-pinned Define Word source below ./tmp")
            .with_timeout(Duration::from_secs(180))
            .with_prelude(PRELUDE),
        "define-word-package-batch",
        "Define Word",
        &[
            inflected_and_missing_terms_follow_the_configured_service_and_display_pipeline(),
            word_region_and_pdf_commands_send_exact_user_selection_without_text_properties(),
            synchronous_url_services_downcase_queries_select_user_agents_and_parse_response_buffers(
            ),
            wordnik_html_parser_preserves_parts_of_speech_semantic_styles_and_result_limit(),
            webster_parser_maps_grammar_labels_and_limits_a_real_definition_page(),
            openthesaurus_parser_removes_superscripts_trims_synonyms_and_honors_the_limit(),
            offline_dictionary_search_returns_matching_entries_and_actionable_configuration_errors(
            ),
        ],
    );
}
