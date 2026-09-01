use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, STRING_INFLECTION_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r###"
(require 'cl-lib)
(require 'string-inflection)

(defun neomacs-si-test-buffer-state ()
  "Return stable text, point, and active-region state."
  (list :text (buffer-substring-no-properties (point-min) (point-max))
        :point (point)
        :mark (mark t)
        :active (and (use-region-p) t)
        :region (and (use-region-p)
                     (list (region-beginning) (region-end)))))

(defun neomacs-si-test-cycle-trace (mode text command steps)
  "Run COMMAND STEPS times on TEXT in MODE and capture each edit."
  (with-temp-buffer
    (funcall mode)
    (insert text)
    (goto-char (point-min))
    (let ((trace (list (neomacs-si-test-buffer-state))))
      (dotimes (_ steps)
        (funcall command)
        (push (neomacs-si-test-buffer-state) trace))
      (nreverse trace))))

(defun neomacs-si-test-partial-region (start end command)
  "Apply COMMAND to START..END of a realistic mixed identifier."
  (let ((string-inflection-region-selection-behavior 'apply-to-each-symbols))
    (with-temp-buffer
      (text-mode)
      (insert "someFunction_to_do_SomeThing FoofooBarbarBarbarFoofoo")
      (set-mark start)
      (goto-char end)
      (activate-mark)
      (funcall command)
      (neomacs-si-test-buffer-state))))
"###;

fn package_contract_preserves_customization_and_compatibility_entry_points() -> ParityBatchCase {
    let elisp_form = r###"
(let ((descriptor (cadr (assq 'string-inflection package-alist))))
  (list
   :package
   (list :name (package-desc-name descriptor)
         :version (package-version-join (package-desc-version descriptor))
         :requirements (package-desc-reqs descriptor)
         :feature (and (featurep 'string-inflection) t))
   :defaults
   (list string-inflection-final-position
         string-inflection-region-selection-behavior
         (functionp string-inflection-bounds-function))
   :commands
   (mapcar #'commandp
           '(string-inflection-ruby-style-cycle
             string-inflection-elixir-style-cycle
             string-inflection-python-style-cycle
             string-inflection-java-style-cycle
             string-inflection-toggle
             string-inflection-camel-case
             string-inflection-lower-camel-case
             string-inflection-snake-case
             string-inflection-capital-snake-case
             string-inflection-upcase
             string-inflection-kebab-case))
   :compatibility
   (list (eq (indirect-function 'string-inflection-cycle)
             (indirect-function 'string-inflection-ruby-style-cycle))
         (eq (indirect-function 'string-inflection-lisp)
             (indirect-function 'string-inflection-kebab-case))
         (eq (indirect-function 'string-inflection-camelcase-function)
             (indirect-function 'string-inflection-camel-case-function))
         (eq (indirect-function 'string-inflection-underscore-function)
             (indirect-function 'string-inflection-snake-case-function)))))
"###;
    let expected = expect![[
        r#"OK (:package (:name string-inflection :version "20251114.1041" :requirements nil :feature t) :defaults (remain replace-all-spaces-with-underscores t) :commands (t t t t t t t t t t t) :compatibility (t t t t))"#
    ]];
    ParityBatchCase::value(
        "package_contract_preserves_customization_and_compatibility_entry_points",
        elisp_form,
        expected,
    )
}

fn production_identifiers_convert_acronyms_digits_separators_and_unicode() -> ParityBatchCase {
    let elisp_form = r###"
(mapcar
 (lambda (identifier)
   (list
    :input identifier
    :snake (string-inflection-snake-case-function identifier)
    :pascal (string-inflection-pascal-case-function identifier)
    :camel (string-inflection-camel-case-function identifier)
    :constant (string-inflection-upcase-function identifier)
    :kebab (string-inflection-kebab-case-function identifier)
    :capital-snake (string-inflection-capital-snake-case-function identifier)
    :shape
    (mapcar (lambda (predicate) (and (funcall predicate identifier) t))
            '(string-inflection-symbol-p
              string-inflection-snake-case-p
              string-inflection-upcase-p
              string-inflection-pascal-case-p
              string-inflection-camel-case-p
              string-inflection-kebab-case-p
              string-inflection-capital-snake-case-p))))
 '("HTTP2ResponseCode"
   "user__profile-ID"
   "db2XMLParser"
   "already_snake_case"
   "kebab-api-v2"
   "EĤOŜanĝoĈIUĴaŭde"))
"###;
    let expected = expect![[
        r#"OK ((:input "HTTP2ResponseCode" :snake "http2_response_code" :pascal "Http2ResponseCode" :camel "http2ResponseCode" :constant "HTTP2_RESPONSE_CODE" :kebab "http2-response-code" :capital-snake "Http2_Response_Code" :shape (nil nil nil t nil nil nil)) (:input "user__profile-ID" :snake "user_profile_id" :pascal "UserProfileId" :camel "userProfileId" :constant "USER_PROFILE_ID" :kebab "user-profile-id" :capital-snake "User_Profile_Id" :shape (nil nil nil nil nil t nil)) (:input "db2XMLParser" :snake "db2_xml_parser" :pascal "Db2XmlParser" :camel "db2XmlParser" :constant "DB2_XML_PARSER" :kebab "db2-xml-parser" :capital-snake "Db2_Xml_Parser" :shape (nil nil nil nil t nil nil)) (:input "already_snake_case" :snake "already_snake_case" :pascal "AlreadySnakeCase" :camel "alreadySnakeCase" :constant "ALREADY_SNAKE_CASE" :kebab "already-snake-case" :capital-snake "Already_Snake_Case" :shape (nil t nil nil nil nil nil)) (:input "kebab-api-v2" :snake "kebab_api_v2" :pascal "KebabApiV2" :camel "kebabApiV2" :constant "KEBAB_API_V2" :kebab "kebab-api-v2" :capital-snake "Kebab_Api_V2" :shape (nil nil nil nil nil t nil)) (:input "EĤOŜanĝoĈIUĴaŭde" :snake "eĥo_ŝanĝo_ĉiu_ĵaŭde" :pascal "EĥoŜanĝoĈiuĴaŭde" :camel "eĥoŜanĝoĈiuĴaŭde" :constant "EĤO_ŜANĜO_ĈIU_ĴAŬDE" :kebab "eĥo-ŝanĝo-ĉiu-ĵaŭde" :capital-snake "Eĥo_Ŝanĝo_Ĉiu_Ĵaŭde" :shape (nil nil nil t nil nil nil)))"#
    ]];
    ParityBatchCase::value(
        "production_identifiers_convert_acronyms_digits_separators_and_unicode",
        elisp_form,
        expected,
    )
}

fn language_specific_cycles_refactor_identifiers_in_their_native_modes() -> ParityBatchCase {
    let elisp_form = r###"
(list
 :ruby
 (neomacs-si-test-cycle-trace
  #'ruby-mode "invoice_total" #'string-inflection-ruby-style-cycle 3)
 :python
 (neomacs-si-test-cycle-trace
  #'python-mode "retry_count" #'string-inflection-python-style-cycle 3)
 :java
 (neomacs-si-test-cycle-trace
  #'java-mode "responseBody" #'string-inflection-java-style-cycle 4)
 :elixir
 (neomacs-si-test-cycle-trace
  #'fundamental-mode "request_path" #'string-inflection-elixir-style-cycle 2))
"###;
    let expected = expect![[
        r#"OK (:ruby ((:text "invoice_total" :point 1 :mark nil :active nil :region nil) (:text "INVOICE_TOTAL" :point 1 :mark nil :active nil :region nil) (:text "InvoiceTotal" :point 1 :mark nil :active nil :region nil) (:text "invoice_total" :point 1 :mark nil :active nil :region nil)) :python ((:text "retry_count" :point 1 :mark nil :active nil :region nil) (:text "RETRY_COUNT" :point 1 :mark nil :active nil :region nil) (:text "RetryCount" :point 1 :mark nil :active nil :region nil) (:text "retry_count" :point 1 :mark nil :active nil :region nil)) :java ((:text "responseBody" :point 1 :mark nil :active nil :region nil) (:text "RESPONSE_BODY" :point 1 :mark nil :active nil :region nil) (:text "ResponseBody" :point 1 :mark nil :active nil :region nil) (:text "responseBody" :point 1 :mark nil :active nil :region nil) (:text "RESPONSE_BODY" :point 1 :mark nil :active nil :region nil)) :elixir ((:text "request_path" :point 1 :mark nil :active nil :region nil) (:text "RequestPath" :point 1 :mark nil :active nil :region nil) (:text "request_path" :point 1 :mark nil :active nil :region nil)))"#
    ]];
    ParityBatchCase::value(
        "language_specific_cycles_refactor_identifiers_in_their_native_modes",
        elisp_form,
        expected,
    )
}

fn cxx_region_refactor_tracks_length_changes_across_member_access_and_lines() -> ParityBatchCase {
    let elisp_form = r###"
(let ((string-inflection-region-selection-behavior 'apply-to-each-symbols))
  (with-temp-buffer
    (c++-mode)
    (insert "HTTPServerConfig configValue;\n"
            "configValue.loadXMLFile();\n"
            "ObjName->MethName();")
    (set-mark (point-min))
    (goto-char (point-max))
    (activate-mark)
    (string-inflection-snake-case)
    (let ((snake (neomacs-si-test-buffer-state)))
      (string-inflection-lower-camel-case)
      (list :snake snake
            :camel (neomacs-si-test-buffer-state)))))
"###;
    let expected = expect![[
        r#"OK (:snake (:text "http_server_config config_value;\nconfig_value.load_xml_file();\nobj_name->meth_name();" :point 86 :mark 1 :active t :region (1 86)) :camel (:text "httpServerConfig configValue;\nconfigValue.loadXmlFile();\nobjName->methName();" :point 78 :mark 1 :active t :region (1 78)))"#
    ]];
    ParityBatchCase::value(
        "cxx_region_refactor_tracks_length_changes_across_member_access_and_lines",
        elisp_form,
        expected,
    )
}

fn partial_regions_transform_only_selected_identifier_segments_and_preserve_selection()
-> ParityBatchCase {
    let elisp_form = r###"
(let ((string-inflection-region-selection-behavior 'apply-to-each-symbols))
  (list
   :leading-camel
   (neomacs-si-test-partial-region
    1 13 #'string-inflection-ruby-style-cycle)
   :middle-snake
   (neomacs-si-test-partial-region
    14 19 #'string-inflection-ruby-style-cycle)
   :mixed-tail
   (neomacs-si-test-partial-region
    20 41 #'string-inflection-ruby-style-cycle)
   :embedded-http
   (with-temp-buffer
     (emacs-lisp-mode)
     (insert "prefixHTTPServerSuffix")
     (set-mark 7)
     (goto-char 17)
     (activate-mark)
     (string-inflection-kebab-case)
     (neomacs-si-test-buffer-state))))
"###;
    let expected = expect![[
        r#"OK (:leading-camel (:text "some_function_to_do_SomeThing FoofooBarbarBarbarFoofoo" :point 14 :mark 1 :active t :region (1 14)) :middle-snake (:text "someFunction_TO_DO_SomeThing FoofooBarbarBarbarFoofoo" :point 19 :mark 14 :active t :region (14 19)) :mixed-tail (:text "someFunction_to_do_some_thing foofoo_barbarBarbarFoofoo" :point 43 :mark 20 :active t :region (20 43)) :embedded-http (:text "prefixhttp-serverSuffix" :point 18 :mark 7 :active t :region (7 18)))"#
    ]];
    ParityBatchCase::value(
        "partial_regions_transform_only_selected_identifier_segments_and_preserve_selection",
        elisp_form,
        expected,
    )
}

fn default_phrase_selection_collapses_whitespace_without_applying_identifier_case()
-> ParityBatchCase {
    let elisp_form = r###"
(with-temp-buffer
  (insert "release  candidate\tbuild\n42")
  (set-mark (point-min))
  (goto-char (point-max))
  (activate-mark)
  (let ((before (neomacs-si-test-buffer-state)))
    (string-inflection-upcase)
    (list :before before
          :after (neomacs-si-test-buffer-state)
          :deactivate-mark deactivate-mark)))
"###;
    let expected = expect![[
        r#"OK (:before (:text "release  candidate\11build\n42" :point 28 :mark 1 :active t :region (1 28)) :after (:text "release_candidate_build_42" :point 27 :mark 1 :active nil :region nil) :deactivate-mark t)"#
    ]];
    ParityBatchCase::value(
        "default_phrase_selection_collapses_whitespace_without_applying_identifier_case",
        elisp_form,
        expected,
    )
}

fn final_position_policy_controls_cursor_and_region_orientation_after_resizing() -> ParityBatchCase
{
    let elisp_form = r###"
(list
 :symbols
 (mapcar
  (lambda (policy)
    (with-temp-buffer
      (emacs-lisp-mode)
      (setq-local string-inflection-final-position policy)
      (insert "parseHTTPResponse suffix")
      (goto-char 8)
      (string-inflection-snake-case)
      (cons policy (neomacs-si-test-buffer-state))))
  '(remain beginning end))
 :regions
 (mapcar
  (lambda (spec)
    (with-temp-buffer
      (emacs-lisp-mode)
      (setq-local string-inflection-final-position (car spec))
      (setq-local string-inflection-region-selection-behavior
                  'apply-to-each-symbols)
      (insert "FooBar HTTPServer")
      (if (cdr spec)
          (progn (set-mark (point-max)) (goto-char (point-min)))
        (set-mark (point-min)) (goto-char (point-max)))
      (activate-mark)
      (string-inflection-snake-case)
      (list :policy (car spec)
            :inverse (and (cdr spec) t)
            :state (neomacs-si-test-buffer-state))))
  '((remain) (remain . t) (beginning) (beginning . t) (end) (end . t))))
"###;
    let expected = expect![[
        r#"OK (:symbols ((remain :text "parse_http_response suffix" :point 8 :mark nil :active nil :region nil) (beginning :text "parse_http_response suffix" :point 1 :mark nil :active nil :region nil) (end :text "parse_http_response suffix" :point 20 :mark nil :active nil :region nil)) :regions ((:policy remain :inverse nil :state (:text "foo_bar http_server" :point 20 :mark 1 :active t :region (1 20))) (:policy remain :inverse t :state (:text "foo_bar http_server" :point 1 :mark 20 :active t :region (1 20))) (:policy beginning :inverse nil :state (:text "foo_bar http_server" :point 1 :mark 20 :active t :region (1 20))) (:policy beginning :inverse t :state (:text "foo_bar http_server" :point 1 :mark 20 :active t :region (1 20))) (:policy end :inverse nil :state (:text "foo_bar http_server" :point 20 :mark 1 :active t :region (1 20))) (:policy end :inverse t :state (:text "foo_bar http_server" :point 20 :mark 1 :active t :region (1 20)))))"#
    ]];
    ParityBatchCase::value(
        "final_position_policy_controls_cursor_and_region_orientation_after_resizing",
        elisp_form,
        expected,
    )
}

fn syntax_aware_and_legacy_bounds_choose_different_hyphenated_targets() -> ParityBatchCase {
    let elisp_form = r###"
(mapcar
 (lambda (spec)
   (with-temp-buffer
     (funcall (nth 0 spec))
     (setq-local string-inflection-bounds-function (nth 1 spec))
     (insert "api-client_id")
     (goto-char (point-min))
     (search-forward "client")
     (backward-char 2)
     (let ((bounds (funcall string-inflection-bounds-function)))
       (string-inflection-upcase)
       (list :mode (nth 0 spec)
             :bounds-function (nth 2 spec)
             :initial-bounds bounds
             :state (neomacs-si-test-buffer-state)))))
 (list
  (list #'emacs-lisp-mode
        (lambda () (bounds-of-thing-at-point 'symbol))
        'mode-symbol)
  (list #'python-mode
        (lambda () (bounds-of-thing-at-point 'symbol))
        'mode-symbol)
  (list #'python-mode
        #'string-inflection-bounds-of-mode-independent-chunk
        'legacy-chunk)))
"###;
    let expected = expect![[
        r#"OK ((:mode emacs-lisp-mode :bounds-function mode-symbol :initial-bounds (1 . 14) :state (:text "API_CLIENT_ID" :point 9 :mark nil :active nil :region nil)) (:mode python-mode :bounds-function mode-symbol :initial-bounds (5 . 14) :state (:text "api-CLIENT_ID" :point 9 :mark nil :active nil :region nil)) (:mode python-mode :bounds-function legacy-chunk :initial-bounds (14 . 1) :state (:text "API_CLIENT_ID" :point 1 :mark nil :active nil :region nil)))"#
    ]];
    ParityBatchCase::value(
        "syntax_aware_and_legacy_bounds_choose_different_hyphenated_targets",
        elisp_form,
        expected,
    )
}

#[test]
fn string_inflection_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(STRING_INFLECTION_MELPA_PIN, "string-inflection.el")
            .expect("prepare revision-pinned String Inflection below ./tmp")
            .with_timeout(Duration::from_secs(240))
            .with_prelude(PRELUDE),
        "string-inflection-package-batch",
        "String Inflection",
        &[
            package_contract_preserves_customization_and_compatibility_entry_points(),
            production_identifiers_convert_acronyms_digits_separators_and_unicode(),
            language_specific_cycles_refactor_identifiers_in_their_native_modes(),
            cxx_region_refactor_tracks_length_changes_across_member_access_and_lines(),
            partial_regions_transform_only_selected_identifier_segments_and_preserve_selection(),
            default_phrase_selection_collapses_whitespace_without_applying_identifier_case(),
            final_position_policy_controls_cursor_and_region_orientation_after_resizing(),
            syntax_aware_and_legacy_bounds_choose_different_hyphenated_targets(),
        ],
    );
}
