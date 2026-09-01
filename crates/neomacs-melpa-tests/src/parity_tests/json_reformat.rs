use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, JSON_REFORMAT_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const PRELUDE: &str = r####"
(require 'cl-lib)
(require 'json)
(require 'json-reformat)

(defun neomacs-json-reformat-test-format (text &optional width pretty)
  "Format TEXT with WIDTH and PRETTY policy."
  (let ((json-reformat:indent-width
         (if (null width) json-reformat:indent-width width))
        (json-reformat:pretty-string? pretty))
    (json-reformat-from-string text)))

(defun neomacs-json-reformat-test-data (text)
  "Parse TEXT into a stable, inspectable Lisp representation."
  (let ((json-object-type 'alist)
        (json-array-type 'list)
        (json-key-type 'string)
        (json-false :false)
        (json-null :null))
    (json-read-from-string text)))

(defun neomacs-json-reformat-test-capture (function)
  "Return FUNCTION's value or exact signaled error data."
  (condition-case error-data
      (list :ok (funcall function))
    (error
     (list :error (car error-data)
           :data (cdr error-data)
           :message (error-message-string error-data)))))

(defun neomacs-json-reformat-test-region-error (text beginning end)
  "Format TEXT from BEGINNING to END and capture message and editor state."
  (with-temp-buffer
    (insert text)
    (goto-char (+ beginning 3))
    (let ((before (buffer-string))
          (point-before (point))
          messages)
      (cl-letf (((symbol-function 'message)
                 (lambda (format-string &rest arguments)
                   (push (apply #'format-message format-string arguments)
                         messages))))
        (json-reformat-region beginning end))
      (list :before before
            :after (buffer-string)
            :unchanged (equal before (buffer-string))
            :point (list point-before (point))
            :messages (nreverse messages)))))
"####;

fn package_registration_exposes_the_region_command_policies_and_custom_error() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((descriptor (cadr (assq 'json-reformat package-alist)))
       (history-entry
        (cl-find-if
         (lambda (entry)
           (member '(provide . json-reformat) (cdr entry)))
         load-history)))
  (list
   :package
   (list :name (package-desc-name descriptor)
         :version (package-version-join (package-desc-version descriptor))
         :requirements (package-desc-reqs descriptor)
         :feature (and (featurep 'json-reformat) t))
   :surface
   (mapcar #'fboundp
           '(json-reformat-region json-reformat-from-string
             json-reformat:print-node json-reformat:tree-to-string
             json-reformat:vector-to-string json-reformat:string-to-string))
   :custom
   (list :indent json-reformat:indent-width
         :indent-safe (funcall (get 'json-reformat:indent-width
                                    'safe-local-variable)
                               2)
         :pretty json-reformat:pretty-string?
         :pretty-safe (funcall (get 'json-reformat:pretty-string?
                                    'safe-local-variable)
                               t))
   :error
   (list (get 'json-reformat-error 'error-message)
         (get 'json-reformat-error 'error-conditions))
   :history
   (list :source (file-name-nondirectory (car history-entry))
         :requires-json (and (member '(require . json) (cdr history-entry)) t)
         :provides (and (member '(provide . json-reformat)
                                (cdr history-entry))
                        t))))
"####;
    let expected = expect![[
        r#"OK (:package (:name json-reformat :version "20220905.2342" :requirements ((emacs (24 3))) :feature t) :surface (t t t t t t) :custom (:indent 4 :indent-safe t :pretty nil :pretty-safe t) :error ("JSON Reformat error" (json-reformat-error error)) :history (:source "json-reformat.el" :requires-json t :provides t))"#
    ]];
    ParityBatchCase::value(
        "package_registration_exposes_the_region_command_policies_and_custom_error",
        elisp_form,
        expected,
    )
}

fn formatting_a_deployment_manifest_preserves_all_data_and_is_idempotent() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((input
        "{\"service\":\"api\",\"enabled\":true,\"retries\":3,\"ratio\":1.25,\"owner\":null,\"targets\":[{\"name\":\"linux\",\"architectures\":[\"x86_64\",\"aarch64\"]},{\"name\":\"windows\",\"architectures\":[]}],\"policy\":{\"rollback\":false,\"note\":\"deploy\\ncarefully\",\"symbol\":\"\\u2661\"}}")
       (formatted (neomacs-json-reformat-test-format input))
       (again (neomacs-json-reformat-test-format formatted)))
  (list :formatted formatted
        :idempotent (equal formatted again)
        :input-data (neomacs-json-reformat-test-data input)
        :output-data (neomacs-json-reformat-test-data formatted)
        :same-data (equal (neomacs-json-reformat-test-data input)
                          (neomacs-json-reformat-test-data formatted))))
"####;
    let expected = expect![[
        r#"OK (:formatted "{\n    \"service\": \"api\",\n    \"enabled\": true,\n    \"retries\": 3,\n    \"ratio\": 1.25,\n    \"owner\": null,\n    \"targets\": [\n        {\n            \"name\": \"linux\",\n            \"architectures\": [\n                \"x86_64\",\n                \"aarch64\"\n            ]\n        },\n        {\n            \"name\": \"windows\",\n            \"architectures\": []\n        }\n    ],\n    \"policy\": {\n        \"rollback\": false,\n        \"note\": \"deploy\\ncarefully\",\n        \"symbol\": \"♡\"\n    }\n}" :idempotent t :input-data (("service" . "api") ("enabled" . t) ("retries" . 3) ("ratio" . 1.25) ("owner" . :null) ("targets" (("name" . "linux") ("architectures" "x86_64" "aarch64")) (("name" . "windows") ("architectures"))) ("policy" ("rollback" . :false) ("note" . "deploy\ncarefully") ("symbol" . "♡"))) :output-data (("service" . "api") ("enabled" . t) ("retries" . 3) ("ratio" . 1.25) ("owner" . :null) ("targets" (("name" . "linux") ("architectures" "x86_64" "aarch64")) (("name" . "windows") ("architectures"))) ("policy" ("rollback" . :false) ("note" . "deploy\ncarefully") ("symbol" . "♡"))) :same-data t)"#
    ]];
    ParityBatchCase::value(
        "formatting_a_deployment_manifest_preserves_all_data_and_is_idempotent",
        elisp_form,
        expected,
    )
}

fn formatting_an_embedded_payload_rewrites_only_the_selected_region() -> ParityBatchCase {
    let elisp_form = r####"
(with-temp-buffer
  (insert "POST /deploy HTTP/1.1\nContent-Type: application/json\n\nPAYLOAD=>{\"service\":\"worker\",\"targets\":[\"stage\",\"prod\"],\"metadata\":{\"owner\":\"ops\"}}<=END\nstatus: pending\n")
  (goto-char (point-min))
  (search-forward "PAYLOAD=>")
  (let* ((beginning (point))
         (end (progn (search-forward "<=END") (match-beginning 0)))
         (beginning-marker (copy-marker beginning))
         (end-marker (copy-marker end t)))
    (put-text-property (point-min) beginning 'face 'header-line)
    (put-text-property end (point-max) 'face 'shadow)
    (goto-char (+ beginning 18))
    (let ((point-before (point)))
      (json-reformat-region beginning end)
      (list
       :text (buffer-string)
       :point (list point-before (point))
       :markers (list (marker-position beginning-marker)
                      (marker-position end-marker))
       :outside-properties
       (list (get-text-property (point-min) 'face)
             (get-text-property (1- (point-max)) 'face))
       :inside-properties
       (let ((position (marker-position beginning-marker)))
         (list (get-text-property position 'face)
               (get-text-property (1- (marker-position end-marker)) 'face)))
       :restriction (list (point-min) (point-max))))))
"####;
    let expected = expect![[
        r#"OK (:text #("POST /deploy HTTP/1.1\nContent-Type: application/json\n\nPAYLOAD=>{\n    \"service\": \"worker\",\n    \"targets\": [\n        \"stage\",\n        \"prod\"\n    ],\n    \"metadata\": {\n        \"owner\": \"ops\"\n    }\n}<=END\nstatus: pending\n" 0 63 (face header-line) 194 216 (face shadow)) :point (82 64) :markers (64 195) :outside-properties (header-line shadow) :inside-properties (nil nil) :restriction (1 217))"#
    ]];
    ParityBatchCase::value(
        "formatting_an_embedded_payload_rewrites_only_the_selected_region",
        elisp_form,
        expected,
    )
}

fn indentation_policy_reformats_nested_configuration_at_multiple_team_widths() -> ParityBatchCase {
    let elisp_form = r####"
(let ((input
       "{\"release\":{\"name\":\"v2.4.0\",\"jobs\":[{\"name\":\"build\",\"steps\":[\"compile\",\"archive\"]},{\"name\":\"publish\",\"steps\":[]}],\"labels\":{}},\"dryRun\":false}"))
  (mapcar
   (lambda (width)
     (let ((formatted (neomacs-json-reformat-test-format input width nil)))
       (list :width width
             :indent-samples
             (list (let ((json-reformat:indent-width width))
                     (json-reformat:indent 0))
                   (let ((json-reformat:indent-width width))
                     (json-reformat:indent 1))
                   (let ((json-reformat:indent-width width))
                     (json-reformat:indent 3)))
             :text formatted
             :data (neomacs-json-reformat-test-data formatted))))
   '(0 2 6)))
"####;
    let expected = expect![[
        r#"OK ((:width 0 :indent-samples ("" "" "") :text "{\n\"release\": {\n\"name\": \"v2.4.0\",\n\"jobs\": [\n{\n\"name\": \"build\",\n\"steps\": [\n\"compile\",\n\"archive\"\n]\n},\n{\n\"name\": \"publish\",\n\"steps\": []\n}\n],\n\"labels\": {\n}\n},\n\"dryRun\": false\n}" :data (("release" ("name" . "v2.4.0") ("jobs" (("name" . "build") ("steps" "compile" "archive")) (("name" . "publish") ("steps"))) ("labels")) ("dryRun" . :false))) (:width 2 :indent-samples ("" "  " "      ") :text "{\n  \"release\": {\n    \"name\": \"v2.4.0\",\n    \"jobs\": [\n      {\n        \"name\": \"build\",\n        \"steps\": [\n          \"compile\",\n          \"archive\"\n        ]\n      },\n      {\n        \"name\": \"publish\",\n        \"steps\": []\n      }\n    ],\n    \"labels\": {\n    }\n  },\n  \"dryRun\": false\n}" :data (("release" ("name" . "v2.4.0") ("jobs" (("name" . "build") ("steps" "compile" "archive")) (("name" . "publish") ("steps"))) ("labels")) ("dryRun" . :false))) (:width 6 :indent-samples ("" "      " "                  ") :text "{\n      \"release\": {\n            \"name\": \"v2.4.0\",\n            \"jobs\": [\n                  {\n                        \"name\": \"build\",\n                        \"steps\": [\n                              \"compile\",\n                              \"archive\"\n                        ]\n                  },\n                  {\n                        \"name\": \"publish\",\n                        \"steps\": []\n                  }\n            ],\n            \"labels\": {\n            }\n      },\n      \"dryRun\": false\n}" :data (("release" ("name" . "v2.4.0") ("jobs" (("name" . "build") ("steps" "compile" "archive")) (("name" . "publish") ("steps"))) ("labels")) ("dryRun" . :false))))"#
    ]];
    ParityBatchCase::value(
        "indentation_policy_reformats_nested_configuration_at_multiple_team_widths",
        elisp_form,
        expected,
    )
}

fn string_policy_handles_unicode_control_characters_quotes_and_paths() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((input
        "{\"description\":\"<pre>\\nrelease\\n</pre>\",\"nick\":\"ops \\u00e4\",\"quote\":\"say \\\"go\\\"\",\"path\":\"C:\\\\deploy\\\\api\",\"symbol\":\"\\u2661\"}")
       (encoded (neomacs-json-reformat-test-format input 4 nil))
       (pretty (neomacs-json-reformat-test-format input 4 t)))
  (list :encoded encoded
        :encoded-data (neomacs-json-reformat-test-data encoded)
        :pretty pretty
        :pretty-parse
        (neomacs-json-reformat-test-capture
         (lambda () (neomacs-json-reformat-test-data pretty)))
        :source-data (neomacs-json-reformat-test-data input)))
"####;
    let expected = expect![[
        r#"OK (:encoded "{\n    \"description\": \"<pre>\\nrelease\\n</pre>\",\n    \"nick\": \"ops ä\",\n    \"quote\": \"say \\\"go\\\"\",\n    \"path\": \"C:\\\\deploy\\\\api\",\n    \"symbol\": \"♡\"\n}" :encoded-data (("description" . "<pre>\nrelease\n</pre>") ("nick" . "ops ä") ("quote" . "say \"go\"") ("path" . "C:\\deploy\\api") ("symbol" . "♡")) :pretty "{\n    \"description\": \"<pre>\nrelease\n</pre>\",\n    \"nick\": \"ops ä\",\n    \"quote\": \"say \\\"go\\\"\",\n    \"path\": \"C:\\\\deploy\\\\api\",\n    \"symbol\": \"♡\"\n}" :pretty-parse (:error json-string-format :data (10) :message "Bad string format: 10") :source-data (("description" . "<pre>\nrelease\n</pre>") ("nick" . "ops ä") ("quote" . "say \"go\"") ("path" . "C:\\deploy\\api") ("symbol" . "♡")))"#
    ]];
    ParityBatchCase::value(
        "string_policy_handles_unicode_control_characters_quotes_and_paths",
        elisp_form,
        expected,
    )
}

fn scalar_array_and_empty_roots_are_valid_configuration_fragments() -> ParityBatchCase {
    let elisp_form = r####"
(mapcar
 (lambda (source)
   (let ((formatted (neomacs-json-reformat-test-format source)))
     (list :source source
           :formatted formatted
           :data (neomacs-json-reformat-test-data formatted))))
 '("[]" "{}" "true" "false" "null" "-0" "1.25e3"
   "9007199254740993" "\"api\""
   "[1,[2,[3,4],5],6,[],{},null,false]"))
"####;
    let expected = expect![[
        r#"OK ((:source "[]" :formatted "[]" :data nil) (:source "{}" :formatted "{\n}" :data nil) (:source "true" :formatted "true" :data t) (:source "false" :formatted "false" :data :false) (:source "null" :formatted "null" :data :null) (:source "-0" :formatted "0" :data 0) (:source "1.25e3" :formatted "1250.0" :data 1250.0) (:source "9007199254740993" :formatted "9007199254740993" :data 9007199254740993) (:source "\"api\"" :formatted "\"api\"" :data "api") (:source "[1,[2,[3,4],5],6,[],{},null,false]" :formatted "[\n    1,\n    [\n        2,\n        [\n            3,\n            4\n        ],\n        5\n    ],\n    6,\n    [],\n    {\n    },\n    null,\n    false\n]" :data (1 (2 (3 4) 5) 6 nil nil :null :false)))"#
    ]];
    ParityBatchCase::value(
        "scalar_array_and_empty_roots_are_valid_configuration_fragments",
        elisp_form,
        expected,
    )
}

fn object_reformatting_has_stable_order_and_last_duplicate_key_wins() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((source
        "{\"zeta\":0,\"alpha\":1,\"middle\":2,\"alpha\":9,\"nested\":{\"b\":2,\"a\":1},\"tail\":true}")
       (first (neomacs-json-reformat-test-format source))
       (second (neomacs-json-reformat-test-format first)))
  (list :source source
        :formatted first
        :stable (equal first second)
        :formatted-data (neomacs-json-reformat-test-data first)
        :source-alist (neomacs-json-reformat-test-data source)
        :alpha-count
        (cl-count "alpha" (mapcar #'car
                                   (neomacs-json-reformat-test-data first))
                  :test #'equal)))
"####;
    let expected = expect![[
        r#"OK (:source "{\"zeta\":0,\"alpha\":1,\"middle\":2,\"alpha\":9,\"nested\":{\"b\":2,\"a\":1},\"tail\":true}" :formatted "{\n    \"zeta\": 0,\n    \"alpha\": 9,\n    \"middle\": 2,\n    \"nested\": {\n        \"b\": 2,\n        \"a\": 1\n    },\n    \"tail\": true\n}" :stable t :formatted-data (("zeta" . 0) ("alpha" . 9) ("middle" . 2) ("nested" ("b" . 2) ("a" . 1)) ("tail" . t)) :source-alist (("zeta" . 0) ("alpha" . 1) ("middle" . 2) ("alpha" . 9) ("nested" ("b" . 2) ("a" . 1)) ("tail" . t)) :alpha-count 1)"#
    ]];
    ParityBatchCase::value(
        "object_reformatting_has_stable_order_and_last_duplicate_key_wins",
        elisp_form,
        expected,
    )
}

fn malformed_regions_report_buffer_coordinates_while_trailing_records_follow_json_reader_semantics()
-> ParityBatchCase {
    let elisp_form = r####"
(let* ((full "{\"service\":\"api\",\"targets\":[\"linux\" \"windows\"]}")
       (prefix "request one\nrequest two\nPAYLOAD=>")
       (suffix "<=END\nuntouched\n")
       (selected (concat prefix full suffix))
       (selected-beginning (1+ (length prefix)))
       (selected-end (+ selected-beginning (length full)))
       (trailing "{\"accepted\":true}{\"silentlyDropped\":true}"))
  (list
   :direct
   (neomacs-json-reformat-test-capture
    (lambda () (json-reformat-from-string full)))
   :whole-region
   (neomacs-json-reformat-test-region-error full 1 (1+ (length full)))
   :selected-region
   (neomacs-json-reformat-test-region-error
    selected selected-beginning selected-end)
   :trailing
   (list :source trailing
         :formatted (json-reformat-from-string trailing)
         :reader-value (neomacs-json-reformat-test-data trailing))))
"####;
    let expected = expect![[
        r#"OK (:direct (:error json-reformat-error :data ("Bad JSON array: \",\", 34" 1 37) :message "JSON Reformat error: \"Bad JSON array: \\\",\\\", 34\", 1, 37") :whole-region (:before "{\"service\":\"api\",\"targets\":[\"linux\" \"windows\"]}" :after "{\"service\":\"api\",\"targets\":[\"linux\" \"windows\"]}" :unchanged t :point (4 4) :messages ("JSON parse error [Reason] Bad JSON array: \",\", 34 [Position] In buffer, line 1 (char 37)")) :selected-region (:before "request one\nrequest two\nPAYLOAD=>{\"service\":\"api\",\"targets\":[\"linux\" \"windows\"]}<=END\nuntouched\n" :after "request one\nrequest two\nPAYLOAD=>{\"service\":\"api\",\"targets\":[\"linux\" \"windows\"]}<=END\nuntouched\n" :unchanged t :point (37 37) :messages ("JSON parse error [Reason] Bad JSON array: \",\", 34 [Position] In buffer, line 3 (char 70)")) :trailing (:source "{\"accepted\":true}{\"silentlyDropped\":true}" :formatted "{\n    \"accepted\": true\n}" :reader-value (("accepted" . t))))"#
    ]];
    ParityBatchCase::value(
        "malformed_regions_report_buffer_coordinates_while_trailing_records_follow_json_reader_semantics",
        elisp_form,
        expected,
    )
}

#[test]
fn json_reformat_package_batch() {
    assert_oracle_batch_cases(
        CachedMelpaOracle::new(JSON_REFORMAT_MELPA_PIN, "json-reformat.el")
            .expect("prepare revision-pinned JSON Reformat source below ./tmp")
            .with_timeout(Duration::from_secs(180))
            .with_prelude(PRELUDE),
        "json-reformat-package-batch",
        "JSON Reformat",
        &[
            package_registration_exposes_the_region_command_policies_and_custom_error(),
            formatting_a_deployment_manifest_preserves_all_data_and_is_idempotent(),
            formatting_an_embedded_payload_rewrites_only_the_selected_region(),
            indentation_policy_reformats_nested_configuration_at_multiple_team_widths(),
            string_policy_handles_unicode_control_characters_quotes_and_paths(),
            scalar_array_and_empty_roots_are_valid_configuration_fragments(),
            object_reformatting_has_stable_order_and_last_duplicate_key_wins(),
            malformed_regions_report_buffer_coordinates_while_trailing_records_follow_json_reader_semantics(),
        ],
    );
}
