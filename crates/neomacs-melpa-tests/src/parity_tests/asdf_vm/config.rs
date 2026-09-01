use expect_test::expect;

use super::ParityBatchCase;

fn asdf_vm_config_scalar_decoders_and_encoders_cover_boolean_integer_string_and_key_shapes()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_config_scalar_decoders_and_encoders_cover_boolean_integer_string_and_key_shapes",
        r##"(list
               (mapcar
                #'asdf-vm-config--file-decode-key
                '(" legacy_version_file "
                  "use-release-candidates"
                  "資料_field"))
               (mapcar
                #'asdf-vm-config--file-decode-value
                '("yes"
                  " no "
                  "0"
                  "0042"
                  "-1"
                  "auto"
                  "資料 λ"))
               (mapcar
                #'asdf-vm-config--file-decode-line
                '("concurrency = 8"
                  "legacy_version_file=yes"
                  "custom_value = 資料 λ"))
               (mapcar
                #'asdf-vm-config--file-encode-key
                '(legacy-version-file
                  use-release-candidates
                  資料-field))
               (mapcar
                #'asdf-vm-config--file-encode-value
                '(t nil 0 42 "auto" "資料 λ")))"##,
        expect![[
            r#"OK ((:legacy-version-file :use-release-candidates :資料-field) (t nil 0 42 "-1" "auto" "資料 λ") ((:concurrency 8) (:legacy-version-file t) (:custom-value "資料 λ")) ("legacy_version_file" "use_release_candidates" "資料_field") ("yes" "no" "0" "42" "auto" "資料 λ"))"#
        ]],
    )
}

fn asdf_vm_config_decode_maps_real_asdfrc_text_into_typed_object_slots() -> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_config_decode_maps_real_asdfrc_text_into_typed_object_slots",
        r##"(let* ((text
                     (concat
                      "legacy_version_file = yes\n"
                      "use_release_candidates = no\n"
                      "always_keep_download = yes\n"
                      "plugin_repository_last_check_duration = 125\n"
                      "disable_plugin_short_name_repository = no\n"
                      "concurrency = auto\n"))
                    (object
                     (asdf-vm-ui--decode
                      'asdf-vm-config--file
                      text)))
               (list
                (eieio-object-class-name object)
                (mapcar
                 (lambda (slot)
                   (list
                    slot
                    (slot-value object slot)))
                 '(legacy-version-file
                   use-release-candidates
                   always-keep-download
                   plugin-repository-last-check-duration
                   disable-plugin-short-name-repository
                   concurrency))))"##,
        expect![[
            r#"OK (asdf-vm-config--file ((legacy-version-file t) (use-release-candidates nil) (always-keep-download t) (plugin-repository-last-check-duration 125) (disable-plugin-short-name-repository nil) (concurrency "auto")))"#
        ]],
    )
}

fn asdf_vm_config_decode_exposes_inline_comment_whitespace_unknown_and_malformed_line_behavior()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_config_decode_exposes_inline_comment_whitespace_unknown_and_malformed_line_behavior",
        r##"(mapcar
               (lambda (text)
                 (condition-case error-data
                     (let ((object
                            (asdf-vm-ui--decode
                             'asdf-vm-config--file
                             text)))
                       (list
                        :ok
                        (list
                         (slot-value
                          object
                          'legacy-version-file)
                         (slot-value
                          object
                          'concurrency))))
                   (invalid-slot-name
                   (list
                     :error
                     (car error-data)
                     (caddr error-data)))
                   (error
                    (list
                     :error
                     (car error-data)
                     (cdr error-data)))))
               '("legacy_version_file = yes ; retained comment\n"
                 "; full comment\nlegacy_version_file = yes\n"
                 "legacy_version_file = yes\n\nconcurrency = 4\n"
                 "unknown_field = value\n"
                 "whitespace only follows\n   \n"))"##,
        expect![[
            r##"OK ((:error invalid-slot-type (asdf-vm-config--file legacy-version-file boolean "yes ; retained comment")) (:ok (t "auto")) (:error invalid-slot-type (asdf-vm-config--file concurrency string 4)) (:error invalid-slot-name :unknown-field) (:error wrong-type-argument (number-or-marker-p nil)))"##
        ]],
    )
}

fn asdf_vm_config_encode_emits_every_supported_field_in_class_order() -> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_config_encode_emits_every_supported_field_in_class_order",
        r##"(let ((object
                    (make-instance
                     'asdf-vm-config--file
                     :legacy-version-file t
                     :use-release-candidates nil
                     :always-keep-download t
                     :plugin-repository-last-check-duration 17
                     :disable-plugin-short-name-repository t
                     :concurrency "6")))
               (list
                asdf-vm-config--valid-file-fields
                (asdf-vm-ui--encode object)))"##,
        expect![[
            r#"OK ((always-keep-download concurrency disable-plugin-short-name-repository legacy-version-file plugin-repository-last-check-duration use-release-candidates) "legacy_version_file = yes\nuse_release_candidates = no\nalways_keep_download = yes\nplugin_repository_last_check_duration = 17\ndisable_plugin_short_name_repository = yes\nconcurrency = 6")"#
        ]],
    )
}

fn asdf_vm_ui_reads_mutates_and_writes_real_config_file_round_trip() -> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_ui_reads_mutates_and_writes_real_config_file_round_trip",
        r##"(let* ((input
                     (asdf-vm-test-path
                      "config/input.asdfrc"))
                    (output
                     (asdf-vm-test-path
                      "config/output.asdfrc")))
               (asdf-vm-test-write-file
                input
                (concat
                 "legacy_version_file = yes\n"
                 "concurrency = auto\n"))
               (let ((object
                      (asdf-vm-ui--read
                       'asdf-vm-config--file
                       input)))
                 (setf
                  (slot-value object
                              'concurrency)
                  "12"
                  (slot-value object
                              'always-keep-download)
                  t)
                 (list
                  (slot-value object
                              'path)
                  (asdf-vm-ui--write
                   object output)
                  (file-exists-p output)
                  (asdf-vm-test-read-file
                   output))))"##,
        expect![[
            r#"OK ("[ORACLE-SANDBOX]/config/input.asdfrc" nil t "legacy_version_file = yes\nuse_release_candidates = no\nalways_keep_download = yes\nplugin_repository_last_check_duration = 60\ndisable_plugin_short_name_repository = no\nconcurrency = 12\n")"#
        ]],
    )
}

fn asdf_vm_config_edit_reads_existing_or_constructs_missing_backing_object() -> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_config_edit_reads_existing_or_constructs_missing_backing_object",
        r##"(let* ((existing
                     (asdf-vm-test-path
                      "config-edit/existing"))
                    (missing
                     (asdf-vm-test-path
                      "config-edit/missing"))
                    calls)
               (asdf-vm-test-write-file
                existing
                "concurrency = auto\n")
               (cl-letf
                   (((symbol-function
                      'eieio-customize-object)
                     (lambda (object)
                       (push
                        (list
                         (eieio-object-class-name
                          object)
                         (slot-value object
                                     'path)
                         (slot-value object
                                     'concurrency))
                        calls)
                       :customized)))
                 (list
                  (asdf-vm-config-edit
                   existing)
                  (asdf-vm-config-edit
                   missing)
                  (nreverse calls))))"##,
        expect![[
            r#"OK (:customized :customized ((asdf-vm-config--file "[ORACLE-SANDBOX]/config-edit/existing" "auto") (asdf-vm-config--file "[ORACLE-SANDBOX]/config-edit/missing" "auto")))"#
        ]],
    )
}

fn asdf_vm_config_state_injection_and_rollback_restore_present_and_absent_environment_values()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_config_state_injection_and_rollback_restore_present_and_absent_environment_values",
        r##"(let ((asdf-vm-config-file
                    "/new/config")
                   (asdf-vm-tool-versions-filename
                    ".versions-new")
                   (asdf-vm-dir
                    "/new/core")
                   (asdf-vm-data-dir
                    "/new/data")
                   (asdf-vm-concurrency
                    "9"))
               (setenv "ASDF_CONFIG_FILE"
                       "/old/config")
               (setenv "ASDF_TOOL_VERSIONS_FILENAME"
                       nil)
               (setenv "ASDF_DIR"
                       "/old/core")
               (setenv "ASDF_DATA_DIR"
                       nil)
               (setenv "ASDF_CONCURRENCY"
                       "auto")
               (let ((state
                      (asdf-vm-config--state-inject)))
                 (let ((injected
                        (mapcar
                         #'getenv
                         '("ASDF_CONFIG_FILE"
                           "ASDF_TOOL_VERSIONS_FILENAME"
                           "ASDF_DIR"
                           "ASDF_DATA_DIR"
                           "ASDF_CONCURRENCY"))))
                   (asdf-vm-config--state-rollback
                    state)
                   (list
                    state
                    injected
                    (mapcar
                     #'getenv
                     '("ASDF_CONFIG_FILE"
                       "ASDF_TOOL_VERSIONS_FILENAME"
                       "ASDF_DIR"
                       "ASDF_DATA_DIR"
                       "ASDF_CONCURRENCY"))))))"##,
        expect![[
            r#"OK (((asdf-vm-config-file "ASDF_CONFIG_FILE" "/old/config") (asdf-vm-tool-versions-filename "ASDF_TOOL_VERSIONS_FILENAME" nil) (asdf-vm-dir "ASDF_DIR" "/old/core") (asdf-vm-data-dir "ASDF_DATA_DIR" nil) (asdf-vm-concurrency "ASDF_CONCURRENCY" "auto")) ("/new/config" ".versions-new" "/new/core" "/new/data" "9") ("/old/config" nil "/old/core" nil "auto"))"#
        ]],
    )
}

pub(super) fn config_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        asdf_vm_config_scalar_decoders_and_encoders_cover_boolean_integer_string_and_key_shapes(),
        asdf_vm_config_decode_maps_real_asdfrc_text_into_typed_object_slots(),
        asdf_vm_config_decode_exposes_inline_comment_whitespace_unknown_and_malformed_line_behavior(
        ),
        asdf_vm_config_encode_emits_every_supported_field_in_class_order(),
        asdf_vm_ui_reads_mutates_and_writes_real_config_file_round_trip(),
        asdf_vm_config_edit_reads_existing_or_constructs_missing_backing_object(),
        asdf_vm_config_state_injection_and_rollback_restore_present_and_absent_environment_values(),
    ]
}
