use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, YAML_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const YAML_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const YAML_TEST_PRELUDE: &str = r####"
(require 'cl-lib)

(defvar yaml-parity-original-max-lisp-eval-depth max-lisp-eval-depth)
(setq max-lisp-eval-depth 10000)
(with-eval-after-load 'yaml
  (setq max-lisp-eval-depth yaml-parity-original-max-lisp-eval-depth))

(defun yaml-parity-parse (source)
  (yaml-parse-string
   source
   :object-type 'alist
   :object-key-type 'string
   :sequence-type 'list))

(defun yaml-parity-positioned-string (value)
  (list
   :text (substring-no-properties value)
   :position (and (> (length value) 0)
                  (get-text-property 0 'yaml-position value))))

(defun yaml-parity-error (thunk)
  (condition-case error
      (list :unexpected-success (funcall thunk))
    (error (list :error (car error) (cadr error)))))
"####;

fn yaml_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(YAML_MELPA_PIN, "yaml.el")
        .expect("prepare pinned yaml.el source below ./tmp")
        .with_prelude(YAML_TEST_PRELUDE)
        .with_timeout(YAML_TEST_TIMEOUT)
}

fn kubernetes_deployment_parses_nested_runtime_configuration() -> ParityBatchCase {
    let elisp_form = r####"
(yaml-parity-parse
 "apiVersion: apps/v1
kind: Deployment
metadata:
  name: neomacs
  labels: {app: editor, tier: desktop}
spec:
  replicas: 3
  enabled: true
  strategy: null
  template:
    containers:
      - name: editor
        image: evalexec/neomacs:2.1
        ports: [8080, 8081]
        args:
          - --batch
          - --debug-init
")
"####;
    let expect = expect![[
        r####"OK (("apiVersion" . "apps/v1") ("kind" . "Deployment") ("metadata" ("name" . "neomacs") ("labels" ("tier" . "desktop") ("app" . "editor"))) ("spec" ("replicas" . 3) ("enabled" . t) ("strategy" . :null) ("template" ("containers" (("image" . "evalexec/neomacs:2.1") ("ports" 8080 8081) ("args" "--batch" "--debug-init") ("name" . "editor"))))))"####
    ]];
    ParityBatchCase::value(
        "kubernetes_deployment_parses_nested_runtime_configuration",
        elisp_form,
        expect,
    )
}

fn ci_defaults_aliases_and_block_scalars_preserve_operational_text() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((source
        "defaults: &defaults
  image: rust:1.96
  retries: 2
build: *defaults
script: |-
  cargo build --release
  cargo nextest run --workspace
summary: >
  Build artifacts once and
  reuse them in every shard.
")
       (parsed (yaml-parity-parse source))
       (defaults (cdr (assoc "defaults" parsed)))
       (build (cdr (assoc "build" parsed))))
  (list
   :parsed (copy-tree parsed)
   :defaults (copy-tree defaults)
   :build (copy-tree build)
   :same-object (eq defaults build)
   :script (cdr (assoc "script" parsed))
   :summary (cdr (assoc "summary" parsed))))
"####;
    let expect = expect![[
        r####"OK (:parsed (("defaults" ("image" . "rust:1.96") ("retries" . 2)) ("build" ("image" . "rust:1.96") ("retries" . 2)) ("script" . "cargo build --release\ncargo nextest run --workspace") ("summary" . "Build artifacts once and reuse them in every shard.\n")) :defaults (("image" . "rust:1.96") ("retries" . 2)) :build (("image" . "rust:1.96") ("retries" . 2)) :same-object nil :script "cargo build --release\ncargo nextest run --workspace" :summary "Build artifacts once and reuse them in every shard.\n")"####
    ]];
    ParityBatchCase::value(
        "ci_defaults_aliases_and_block_scalars_preserve_operational_text",
        elisp_form,
        expect,
    )
}

fn scalar_and_container_options_support_typed_and_string_only_consumers() -> ParityBatchCase {
    let elisp_form = r####"
(let ((source
       "name: neomacs
enabled: true
disabled: false
missing: null
replicas: 3
negative: -7
octal: 0o20
hex: 0x2a
ratio: 1.25
values: [false, null, 9, text]
"))
  (list
   :typed
   (yaml-parse-string
    source
    :object-type 'plist
    :sequence-type 'list
    :null-object 'missing-value
    :false-object 'disabled-value)
   :strings
   (yaml-parse-string
    source
    :object-type 'alist
    :object-key-type 'string
    :sequence-type 'list
    :string-values t)
   :keyword-alist
   (yaml-parse-string
    "release: {channel: stable, ready: true}"
    :object-type 'alist
    :object-key-type 'keyword
    :sequence-type 'list)))
"####;
    let expect = expect![[
        r####"OK (:typed (:name "neomacs" :enabled t :disabled disabled-value :missing missing-value :replicas 3 :negative -7 :octal 16 :hex 42 :ratio 1.25 :values (disabled-value missing-value 9 "text")) :strings (("name" . "neomacs") ("enabled" . "true") ("disabled" . "false") ("missing" . "null") ("replicas" . "3") ("negative" . "-7") ("octal" . "0o20") ("hex" . "0x2a") ("ratio" . "1.25") ("values" "false" "null" "9" "text")) :keyword-alist ((:release (:ready . t) (:channel . "stable"))))"####
    ]];
    ParityBatchCase::value(
        "scalar_and_container_options_support_typed_and_string_only_consumers",
        elisp_form,
        expect,
    )
}

fn application_config_encodes_and_round_trips_nested_real_values() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((object
        '((service . "neomacs")
          (enabled . t)
          (ports . [8080 8081])
          (commands . ("cargo build --release" "cargo nextest run"))
          (metadata . ((owner . "editor team")
                       (note . "line one\nline two\t\"quoted\"")))))
       (encoded (yaml-encode object))
       (parsed
        (yaml-parse-string encoded
                           :object-type 'alist
                           :object-key-type 'symbol
                           :sequence-type 'list))
       (encoded-again (yaml-encode parsed)))
  (list
   :encoded encoded
   :parsed parsed
   :encoded-again encoded-again))
"####;
    let expect = expect![[
        r####"OK (:encoded "service: neomacs\nenabled: true\nports: [8080, 8081]\ncommands: [\"cargo build --release\", \"cargo nextest run\"]\nmetadata: \n  owner: \"editor team\"\n  note: \"line one\\nline two\\t\\\"quoted\\\"\"" :parsed ((service . "neomacs") (enabled . t) (ports 8080 8081) (commands "cargo build --release" "cargo nextest run") (metadata (owner . "editor team") (note . "line one\nline two\11\"quoted\""))) :encoded-again "service: neomacs\nenabled: true\nports: [8080, 8081]\ncommands: [\"cargo build --release\", \"cargo nextest run\"]\nmetadata: \n  owner: \"editor team\"\n  note: \"line one\\nline two\\t\\\"quoted\\\"\"")"####
    ]];
    ParityBatchCase::value(
        "application_config_encodes_and_round_trips_nested_real_values",
        elisp_form,
        expect,
    )
}

fn encoding_dialects_emit_exact_payloads_for_external_consumers() -> ParityBatchCase {
    let elisp_form = r####"
(let ((object '((deeper . [((foo . "bar")
                            (count . 3)
                            (labels . ["editor" "gui"]))]))))
  (list
   :auto (let ((yaml-encode-dialect :auto)
               (yaml-encode-indent-width 2))
           (yaml-encode object))
   :compact (let ((yaml-encode-dialect :kyaml-compact)
                  (yaml-encode-indent-width 2))
              (yaml-encode object))
   :pretty (let ((yaml-encode-dialect :kyaml-pretty)
                 (yaml-encode-indent-width 4))
             (yaml-encode object))
   :escaped
   (let ((yaml-encode-dialect :auto))
     (yaml-encode "line one\nline two\t\"quoted\"\\path"))))
"####;
    let expect = expect![[
        r####"OK (:auto "deeper: \n- foo: bar\n  count: 3\n  labels: [editor, gui]" :compact "{deeper: [{foo: \"bar\", count: 3, labels: [\"editor\", \"gui\", ], }, ], }" :pretty "{\n    deeper: [\n        {\n            foo: \"bar\",\n            count: 3,\n            labels: [\n                \"editor\",\n                \"gui\",\n            ],\n        },\n    ],\n}" :escaped "\"line one\\nline two\\t\\\"quoted\\\"\\path\"")"####
    ]];
    ParityBatchCase::value(
        "encoding_dialects_emit_exact_payloads_for_external_consumers",
        elisp_form,
        expect,
    )
}

fn editor_navigation_receives_exact_source_positions_for_nested_values() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((source
        "service:
  name: neomacs
  replicas: 3
  owners:
    - release
    - gui
")
       (parsed (yaml-parse-string-with-pos source))
       (service-cell (assoc "service" parsed))
       (service (cdr service-cell))
       (name-cell (assoc "name" service))
       (replicas-cell (assoc "replicas" service))
       (owners-cell (assoc "owners" service)))
  (list
   :root-key (yaml-parity-positioned-string (car service-cell))
   :name-key (yaml-parity-positioned-string (car name-cell))
   :name-value (yaml-parity-positioned-string (cdr name-cell))
   :replicas-key (yaml-parity-positioned-string (car replicas-cell))
   :replicas-value (yaml-parity-positioned-string (cdr replicas-cell))
   :owners-key (yaml-parity-positioned-string (car owners-cell))
   :owner-values
   (mapcar #'yaml-parity-positioned-string (append (cdr owners-cell) nil))
   :plain (yaml-parity-parse source)))
"####;
    let expect = expect![[
        r####"OK (:root-key (:text "service" :position (1 . 8)) :name-key (:text "name" :position (12 . 16)) :name-value (:text "neomacs" :position (18 . 25)) :replicas-key (:text "replicas" :position (28 . 36)) :replicas-value (:text "3" :position (38 . 39)) :owners-key (:text "owners" :position (42 . 48)) :owner-values ((:text "release" :position (56 . 63)) (:text "gui" :position (70 . 73))) :plain (("service" ("name" . "neomacs") ("replicas" . 3) ("owners" "release" "gui"))))"####
    ]];
    ParityBatchCase::value(
        "editor_navigation_receives_exact_source_positions_for_nested_values",
        elisp_form,
        expect,
    )
}

fn configuration_validation_reports_parser_option_errors_and_continuations() -> ParityBatchCase {
    let elisp_form = r####"
(list
 :unterminated-flow
 (yaml-parity-error (lambda () (yaml-parity-parse "ports: [8080, 8081")))
 :indented-continuation
 (yaml-parity-parse "name: neomacs\n  orphan")
 :object-type
 (yaml-parity-error
  (lambda () (yaml-parse-string "name: neomacs" :object-type 'vector)))
 :sequence-type
 (yaml-parity-error
  (lambda () (yaml-parse-string "[one, two]" :sequence-type 'cons)))
 :key-type
 (yaml-parity-error
  (lambda () (yaml-parse-string "name: neomacs" :object-key-type 'number))))
"####;
    let expect = expect![[
        r####"OK (:unterminated-flow (:error error "Unable to parse YAML.  Parser finished before end of input 0/18") :indented-continuation (("name" . "neomacs orphan")) :object-type (:error error "Invalid object-type.  Must be hash-table, alist, or plist") :sequence-type (:error error "Invalid sequence-type.  sequence-type must be list or array") :key-type (:error error "Invalid object-key-type.  Must be string, keyword, or symbol"))"####
    ]];
    ParityBatchCase::value(
        "configuration_validation_reports_parser_option_errors_and_continuations",
        elisp_form,
        expect,
    )
}

#[test]
fn yaml_package_batch() {
    let cases = vec![
        kubernetes_deployment_parses_nested_runtime_configuration(),
        ci_defaults_aliases_and_block_scalars_preserve_operational_text(),
        scalar_and_container_options_support_typed_and_string_only_consumers(),
        application_config_encodes_and_round_trips_nested_real_values(),
        encoding_dialects_emit_exact_payloads_for_external_consumers(),
        editor_navigation_receives_exact_source_positions_for_nested_values(),
        configuration_validation_reports_parser_option_errors_and_continuations(),
    ];
    let thread = std::thread::current();
    let test_name = thread.name().unwrap_or("unnamed yaml.el parity test");
    assert_oracle_batch_cases(yaml_oracle(), test_name, "yaml_parity", &cases);
}
