use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, GOOGLE_TRANSLATE_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const GOOGLE_TRANSLATE_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const GOOGLE_TRANSLATE_TEST_PRELUDE: &str = r###"
(require 'cl-lib)
(require 'google-translate)
(require 'google-translate-smooth-ui)

(defconst neomacs-google-translate-test-dictionary-response
  "[[[\"Lançamento pronto\",\"Release ready\",\"lansamento prontu\",\"rɪˈliːs rɛdi\"]],[[\"noun\",[\"lançamento\",\"liberação\"],[[\"lançamento\",[\"release\",\"launch\"],null,0.9],[\"liberação\",[\"release\",\"clearance\"],null,0.7]],\"release\",1]],\"en\",null,null,null,null,[],null,null,null,null,[[\"noun\",[[\"an action of releasing\",null,\"the release completed\"],[\"permission to publish\"]]]]]")

(defconst neomacs-google-translate-test-suggestion-response
  "[[[\"successful\",\"sucesful\",\"\",\"\"]],null,\"en\",null,null,null,null,[\"<b><i>successful</i></b>\",\"successful\",[1]]]")

(defun neomacs-google-translate-test-property-runs (begin end properties)
  "Describe selected PROPERTIES between BEGIN and END."
  (let ((position begin)
        runs)
    (while (< position end)
      (let* ((next (next-property-change position nil end))
             (values
              (delq nil
                    (mapcar
                     (lambda (property)
                       (when-let ((value (get-text-property position property)))
                         (list property value)))
                     properties))))
        (when values
          (push (list (- position begin) (- next begin) values) runs))
        (setq position next)))
    (nreverse runs)))

(defun neomacs-google-translate-test-reset ()
  "Restore mutable package state between practical probes."
  (setq google-translate-backend-method 'emacs
        google-translate-backend-debug nil
        google-translate-show-phonetic nil
        google-translate-display-translation-phonetic t
        google-translate-listen-program nil
        google-translate-listen-maxlen 200
        google-translate-output-destination nil
        google-translate-translation-to-kill-ring nil
        google-translate-result-translation nil
        google-translate-default-source-language nil
        google-translate-default-target-language nil
        google-translate-input-method-auto-toggling nil
        google-translate-preferable-input-methods-alist '((nil . nil))
        google-translate-translation-directions-alist nil
        google-translate-current-translation-direction 0
        google-translate-last-translation-direction nil
        google-translate-translation-direction-query ""
        google-translate-try-other-direction nil
        google-translate-minibuffer-keymap nil
        kill-ring nil)
  (dolist (name '("*Google Translate*" "*google-translate-backend-debug*"))
    (when-let ((buffer (get-buffer name)))
      (kill-buffer buffer))))

(defun neomacs-google-translate-test-run (function)
  "Run FUNCTION with clean Google Translate state."
  (neomacs-google-translate-test-reset)
  (unwind-protect
      (funcall function)
    (neomacs-google-translate-test-reset)))
"###;

fn google_translate_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(GOOGLE_TRANSLATE_MELPA_PIN, "google-translate.el")
        .expect("prepare revision-pinned Google Translate source below ./tmp")
        .with_prelude(GOOGLE_TRANSLATE_TEST_PRELUDE)
        .with_timeout(GOOGLE_TRANSLATE_TEST_TIMEOUT)
}

fn request_pipeline_encodes_user_text_and_repairs_sparse_google_json() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-google-translate-test-run
 (lambda ()
   (let ((response
          "[[[\"sucesful\",\"sucesful\",\"sucesful\",\"\"]],,\"en\",,[[\"sucesful\",[1],true,false,1000,0,1,0]],[[\"sucesful\",1,[[\"sucesful\",1000,true,false]],[[0,8]],\"sucesful\"]],,[\"<b><i>successful</i></b>\",\"successful\",[1]],[[]],7]")
         captured-url)
     (cl-letf (((symbol-function 'google-translate-backend-retrieve)
                (lambda (url)
                  (setq captured-url url)
                  (insert response))))
       (let* ((raw "  sucesful\n deploy  ")
              (json (google-translate-request "auto" "pt" raw)))
         (list
          :prepared (google-translate-prepare-text-for-request raw)
          :request-url captured-url
          :translation (google-translate-json-translation json)
          :translation-phonetic
          (google-translate-json-translation-phonetic json)
          :text-phonetic (google-translate-json-text-phonetic json)
          :detailed (google-translate-json-detailed-translation json)
          :suggestion (google-translate-json-suggestion json)
          :detected (aref json 2)
          :repaired (google-translate--insert-nulls "[,[,,],]")))))))
"###;
    let expected = expect![[
        r#"OK (:prepared "sucesful deploy" :request-url "http://translate.google.com/translate_a/single?client=gtx&ie=UTF-8&oe=UTF-8&hl=en&sl=auto&tl=pt&q=%20%20sucesful%0A%20deploy%20%20&dt=bd&dt=ex&dt=ld&dt=md&dt=qc&dt=rw&dt=rm&dt=ss&dt=t&dt=at&pc=1&otf=1&srcrom=1&ssel=0&tsel=0" :translation "sucesful" :translation-phonetic "sucesful" :text-phonetic "" :detailed nil :suggestion "successful" :detected "en" :repaired "[null,[null,null,null],null]")"#
    ]];
    ParityBatchCase::value(
        "request_pipeline_encodes_user_text_and_repairs_sparse_google_json",
        elisp_form,
        expected,
    )
}

fn dictionary_result_renders_phonetics_ranked_synonyms_and_definitions() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-google-translate-test-run
 (lambda ()
   (let* ((google-translate-show-phonetic t)
          (json (json-read-from-string
                 neomacs-google-translate-test-dictionary-response))
          (gtos
           (make-gtos
            :source-language "auto"
            :target-language "pt"
            :auto-detected-language (aref json 2)
            :text "Release ready"
            :text-phonetic (google-translate-json-text-phonetic json)
            :translation (google-translate-json-translation json)
            :translation-phonetic
            (google-translate-json-translation-phonetic json)
            :detailed-translation
            (google-translate-json-detailed-translation json)
            :detailed-definition
            (google-translate-json-detailed-definition json)))
          rendered properties mode)
     (with-temp-buffer
       (google-translate-buffer-insert-translation gtos)
       (setq rendered (buffer-string)
             properties
             (neomacs-google-translate-test-property-runs
              (point-min) (point-max)
              '(font-lock-face face category button))
             mode major-mode))
     (list :rendered rendered
           :properties properties
           :mode mode
           :translation (gtos-translation gtos)
           :definition-count
           (length (gtos-detailed-definition gtos))))))
"###;
    let expected = expect![[
        r#"OK (:rendered #("Translate from English (detected) to Portuguese:\n\nRelease ready\n\nrɪˈliːs rɛdi\n\nLançamento pronto\n\nlansamento prontu\n\nnoun\n 1. lançamento (release, launch)\n 2. liberação (release, clearance)\n\nDEFINITION\n\nnoun\n 1. an action of releasing\n    \"the release completed\"\n 2. permission to publish\n" 50 64 (face google-translate-text-face) 64 78 (face google-translate-phonetic-face) 79 97 (face google-translate-translation-face) 97 116 (face google-translate-phonetic-face) 117 121 (font-lock-face google-translate-translation-face) 191 201 (font-lock-face google-translate-translation-face) 203 207 (font-lock-face google-translate-translation-face)) :properties ((50 64 ((face google-translate-text-face))) (64 78 ((face google-translate-phonetic-face))) (79 97 ((face google-translate-translation-face))) (97 116 ((face google-translate-phonetic-face))) (117 121 ((font-lock-face google-translate-translation-face))) (191 201 ((font-lock-face google-translate-translation-face))) (203 207 ((font-lock-face google-translate-translation-face)))) :mode google-translate-mode :translation "Lançamento pronto" :definition-count 1)"#
    ]];
    ParityBatchCase::value(
        "dictionary_result_renders_phonetics_ranked_synonyms_and_definitions",
        elisp_form,
        expected,
    )
}

fn misspelling_suggestion_is_a_live_button_that_retries_the_translation() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-google-translate-test-run
 (lambda ()
   (let* ((json (json-read-from-string
                 neomacs-google-translate-test-suggestion-response))
          (gtos
           (make-gtos
            :source-language "en"
            :target-language "pt"
            :auto-detected-language (aref json 2)
            :text "sucesful"
            :text-phonetic (google-translate-json-text-phonetic json)
            :translation (google-translate-json-translation json)
            :translation-phonetic
            (google-translate-json-translation-phonetic json)
            :detailed-translation
            (google-translate-json-detailed-translation json)
            :suggestion (google-translate-json-suggestion json)))
          retry)
     (with-temp-buffer
       (google-translate-insert-translation gtos)
       (let* ((rendered (buffer-string))
              (button (next-button (point-min)))
              (button-state
               (list :label (button-label button)
                     :action (button-get button 'action)
                     :follow-link (button-get button 'follow-link)
                     :suggestion (button-get button 'suggestion)
                     :source (button-get button 'source-language)
                     :target (button-get button 'target-language))))
         (cl-letf (((symbol-function 'google-translate-translate)
                    (lambda (&rest args) (setq retry args))))
           (google-translate--suggestion-action button))
         (list :rendered rendered
               :button button-state
               :retry retry
               :button-runs
               (neomacs-google-translate-test-property-runs
                (point-min) (point-max)
                '(button category action follow-link suggestion
                         source-language target-language))))))))
"###;
    let expected = expect![[
        r#"OK (:rendered #("English -> Portuguese: sucesful - successful\nDid you mean: successful\n" 22 31 (face google-translate-text-face) 34 44 (face google-translate-translation-face) 45 59 (face google-translate-suggestion-label-face) 59 69 (face (google-translate-suggestion-face button) target-language "pt" source-language "en" suggestion "successful" follow-link t action google-translate--suggestion-action category default-button button (t))) :button (:label "successful" :action google-translate--suggestion-action :follow-link t :suggestion "successful" :source "en" :target "pt") :retry ("en" "pt" "successful") :button-runs ((59 69 ((button (t)) (category default-button) (action google-translate--suggestion-action) (follow-link t) (suggestion "successful") (source-language "en") (target-language "pt")))))"#
    ]];
    ParityBatchCase::value(
        "misspelling_suggestion_is_a_live_button_that_retries_the_translation",
        elisp_form,
        expected,
    )
}

fn speech_urls_split_a_long_multilingual_release_at_readable_boundaries() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-google-translate-test-run
 (lambda ()
   (let* ((google-translate-listen-maxlen 24)
          (text "Deploy release one. Verify canary, then publish 日本語 safely.")
          (chunks (google-translate--split-text
                   text google-translate-listen-maxlen))
          (urls (google-translate-format-listen-urls text "ja")))
     (list :chunks chunks
           :lengths (mapcar #'length chunks)
           :urls urls
           :single
           (google-translate-format-listen-url
            "release & rollback 日本語" "ja" "3" "2")))))
"###;
    let expected = expect![[
        r#"OK (:chunks ("Deploy release one. " "Verify canary," " " "then publish 日本語 safely.") :lengths (20 14 1 24) :urls ("http://translate.google.com/translate_tts?ie=UTF-8&q=Deploy%20release%20one.%20&tl=ja&total=4&idx=0&textlen=20&client=gtx&prev=input" "http://translate.google.com/translate_tts?ie=UTF-8&q=Verify%20canary%2C&tl=ja&total=4&idx=1&textlen=14&client=gtx&prev=input" "http://translate.google.com/translate_tts?ie=UTF-8&q=%20&tl=ja&total=4&idx=2&textlen=1&client=gtx&prev=input" "http://translate.google.com/translate_tts?ie=UTF-8&q=then%20publish%20%E6%97%A5%E6%9C%AC%E8%AA%9E%20safely.&tl=ja&total=4&idx=3&textlen=24&client=gtx&prev=input") :single "http://translate.google.com/translate_tts?ie=UTF-8&q=release%20%26%20rollback%20%E6%97%A5%E6%9C%AC%E8%AA%9E&tl=ja&total=3&idx=2&textlen=22&client=gtx&prev=input")"#
    ]];
    ParityBatchCase::value(
        "speech_urls_split_a_long_multilingual_release_at_readable_boundaries",
        elisp_form,
        expected,
    )
}

fn defaults_overrides_and_language_completion_drive_real_translation_pairs() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-google-translate-test-run
 (lambda ()
   (let ((google-translate-default-source-language "en")
         (google-translate-default-target-language "ja")
         (google-translate-preferable-input-methods-alist
          '((nil . ("en" "fr"))
            (japanese . ("ja"))
            (cyrillic-translit . ("ru"))))
         answers prompts)
     (setq answers '("French" "" "German"))
     (cl-letf (((symbol-function 'google-translate-completing-read)
                (lambda (prompt choices &optional default)
                  (push (list :prompt prompt
                              :default default
                              :choice-count (length choices)
                              :first (car choices)
                              :last (car (last choices)))
                        prompts)
                  (pop answers))))
       (list
        :normal (google-translate-read-args nil nil)
        :reverse (google-translate-read-args nil t)
        :override (google-translate-read-args t nil)
        :prompts (nreverse prompts)
        :round-trips
        (mapcar
         (lambda (name)
           (let ((code (google-translate-language-abbreviation name)))
             (list name code
                   (google-translate-language-display-name code))))
         '("English" "Japanese" "Portuguese"))
        :detect (google-translate-language-abbreviation "Detect language")
        :input-methods
        (mapcar #'google-translate-find-preferable-input-method
                '("en" "ja" "ru" "pt")))))))
"###;
    let expected = expect![[
        r#"OK (:normal ("en" "ja") :reverse ("ja" "en") :override ("fr" "de") :prompts ((:prompt "Translate from: " :default "Detect language" :choice-count 104 :first "Afrikaans" :last "Zulu") (:prompt "Translate from French to: " :default nil :choice-count 104 :first "Afrikaans" :last "Zulu") (:prompt "Translate from French to: " :default nil :choice-count 104 :first "Afrikaans" :last "Zulu")) :round-trips (("English" "en" "English") ("Japanese" "ja" "Japanese") ("Portuguese" "pt" "Portuguese")) :detect "auto" :input-methods (nil japanese cyrillic-translit nil))"#
    ]];
    ParityBatchCase::value(
        "defaults_overrides_and_language_completion_drive_real_translation_pairs",
        elisp_form,
        expected,
    )
}

fn smooth_ui_direction_ring_wraps_and_builds_its_minibuffer_controls() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-google-translate-test-run
 (lambda ()
   (let ((google-translate-translation-directions-alist
          '(("en" . "pt") ("pt" . "en") ("ja" . "en")))
         reports)
     (cl-letf (((symbol-function 'minibuffer-contents)
                (lambda () "release status")))
       (dolist (step '(previous next next next next))
         (google-translate-change-translation-direction step)
         (push
          (list :step step
                :index google-translate-current-translation-direction
                :source
                (google-translate--current-direction-source-language)
                :target
                (google-translate--current-direction-target-language)
                :query google-translate-translation-direction-query)
          reports)))
     (google-translate--setup-minibuffer-keymap)
     (list :transitions (nreverse reports)
           :bindings
           (mapcar
            (lambda (key)
              (list (key-description key)
                    (lookup-key google-translate-minibuffer-keymap key)))
            (list (kbd "C-p") (kbd "C-n") (kbd "C-l")))
           :inherits-minibuffer
           (eq (keymap-parent google-translate-minibuffer-keymap)
               minibuffer-local-map)))))
"###;
    let expected = expect![[
        r#"OK (:transitions ((:step previous :index 2 :source "ja" :target "en" :query "release status") (:step next :index 0 :source "en" :target "pt" :query "release status") (:step next :index 1 :source "pt" :target "en" :query "release status") (:step next :index 2 :source "ja" :target "en" :query "release status") (:step next :index 0 :source "en" :target "pt" :query "release status")) :bindings (("C-p" google-translate-previous-translation-direction) ("C-n" google-translate-next-translation-direction) ("C-l" google-translate-clear-minibuffer)) :inherits-minibuffer t)"#
    ]];
    ParityBatchCase::value(
        "smooth_ui_direction_ring_wraps_and_builds_its_minibuffer_controls",
        elisp_form,
        expected,
    )
}

fn at_point_and_active_region_preserve_real_editor_text_boundaries() -> ParityBatchCase {
    let elisp_form = r###"
(neomacs-google-translate-test-run
 (lambda ()
   (let ((google-translate-default-source-language "en")
         (google-translate-default-target-language "fr")
         calls)
     (cl-letf (((symbol-function 'google-translate-translate)
                (lambda (&rest args) (push args calls))))
       (with-temp-buffer
         (let ((transient-mark-mode t))
           (text-mode)
           (insert "Release REL-2048 passed canary deployment.\n"
                   "Rollback remains available.")
           (goto-char (point-min))
           (search-forward "canary")
           (google-translate-at-point)
           (goto-char (point-min))
           (search-forward "canary")
           (let ((begin (match-beginning 0)))
             (search-forward "deployment")
             (goto-char (match-end 0))
             (push-mark begin t t)
             (google-translate-at-point-reverse))
           (list :calls (nreverse calls)
                 :buffer (buffer-string)
                 :point (point)
                 :mark (mark t)
                 :region-active (use-region-p))))))))
"###;
    let expected = expect![[
        r#"OK (:calls (("en" "fr" "canary") ("fr" "en" "canary deployment")) :buffer "Release REL-2048 passed canary deployment.\nRollback remains available." :point 42 :mark 25 :region-active t)"#
    ]];
    ParityBatchCase::value(
        "at_point_and_active_region_preserve_real_editor_text_boundaries",
        elisp_form,
        expected,
    )
}

fn external_backend_dispatches_a_configured_translator_and_reports_bad_methods() -> ParityBatchCase
{
    let elisp_form = r###"
(neomacs-google-translate-test-run
 (lambda ()
   (let ((google-translate-backend-method 'translator)
         (google-translate-backend-user-agent "Neomacs parity/1.0")
         (google-translate-backend-commands
          '((translator :name "translator-cli"
                        :args ("--silent" "--follow" "--agent"))))
         invocation)
     (with-temp-buffer
       (cl-letf (((symbol-function 'call-process)
                  (lambda (program infile destination display &rest args)
                    (setq invocation
                          (list program infile destination display args))
                    (insert neomacs-google-translate-test-suggestion-response)
                    0)))
         (google-translate-backend-retrieve
          "https://translation.invalid/v1?q=release%20ready"))
       (let ((body (buffer-string))
             (bad-method
              (condition-case error
                  (let ((google-translate-backend-method 'missing))
                    (google-translate-backend-retrieve "unused")
                    'unexpected-success)
                (error
                 (list (car error) (error-message-string error))))))
         (list :invocation invocation
               :body body
               :parsed-translation
               (google-translate-json-translation
                (json-read-from-string body))
               :bad-method bad-method))))))
"###;
    let expected = expect![[
        r#"OK (:invocation ("translator-cli" nil t nil ("--silent" "--follow" "--agent" "Neomacs parity/1.0" "https://translation.invalid/v1?q=release%20ready")) :body "[[[\"successful\",\"sucesful\",\"\",\"\"]],null,\"en\",null,null,null,null,[\"<b><i>successful</i></b>\",\"successful\",[1]]]" :parsed-translation "successful" :bad-method (error "Unknown backend method: missing"))"#
    ]];
    ParityBatchCase::value(
        "external_backend_dispatches_a_configured_translator_and_reports_bad_methods",
        elisp_form,
        expected,
    )
}

#[test]
fn google_translate_package_batch() {
    let cases = vec![
        request_pipeline_encodes_user_text_and_repairs_sparse_google_json(),
        dictionary_result_renders_phonetics_ranked_synonyms_and_definitions(),
        misspelling_suggestion_is_a_live_button_that_retries_the_translation(),
        speech_urls_split_a_long_multilingual_release_at_readable_boundaries(),
        defaults_overrides_and_language_completion_drive_real_translation_pairs(),
        smooth_ui_direction_ring_wraps_and_builds_its_minibuffer_controls(),
        at_point_and_active_region_preserve_real_editor_text_boundaries(),
        external_backend_dispatches_a_configured_translator_and_reports_bad_methods(),
    ];
    assert_oracle_batch_cases(
        google_translate_oracle(),
        "google-translate-package-batch",
        "Google Translate",
        &cases,
    );
}
