use expect_test::expect;

use super::ParityBatchCase;

fn asilea_exact_pin_descriptor_dependencies_origin_and_feature_contract_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "asilea_exact_pin_descriptor_dependencies_origin_and_feature_contract_match",
        r##"(let ((descriptor
                (cadr
                 (assq 'asilea package-alist))))
         (list
          (package-desc-name descriptor)
          (package-version-join
           (package-desc-version descriptor))
          (package-desc-summary descriptor)
          (package-desc-kind descriptor)
          (package-desc-reqs descriptor)
          (package-desc-extras descriptor)
          (featurep 'asilea)))"##,
        expect![[
            r#"OK (asilea "20150105.1525" "Find best compiler options using simulated annealing." nil ((emacs (24)) (cl-lib (0 5))) ((:maintainers ("Fanael Linithien" . "fanael4@gmail.com")) (:authors ("Fanael Linithien" . "fanael4@gmail.com")) (:revdesc . "2aab1cc63b64") (:commit . "2aab1cc63b64ef08d12e84fd7ba5c94065f6039f") (:url . "https://github.com/Fanael/asilea")) t)"#
        ]],
    )
}

fn asilea_installed_payload_inventory_sizes_and_content_digests_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "asilea_installed_payload_inventory_sizes_and_content_digests_match",
        r##"(let* ((descriptor
                  (cadr
                   (assq 'asilea package-alist)))
                 (directory
                  (package-desc-dir descriptor)))
         (mapcar
          (lambda (file)
            (let ((path
                   (expand-file-name file directory)))
              (list
               file
               (file-attribute-size
                (file-attributes path))
               (with-temp-buffer
                 (insert-file-contents-literally path)
                 (secure-hash
                  'sha256
                  (current-buffer))))))
          (sort
           (seq-filter
            (lambda (file)
              (file-regular-p
               (expand-file-name file directory)))
            (directory-files directory nil "\\`[^.]"))
           #'string<)))"##,
        expect![[
            r#"OK (("asilea-autoloads.el" 671 "789051238f361972d0a74e61b8caef971bee13faf8a67119316ab1f3110e8714") ("asilea-pkg.el" 429 "5086c0efc627fe6981ca743c3671370c03fbe0b4328e9ffeefc264da4ae5373e") ("asilea.el" 16326 "ea0a4b390818cd780c323eb1e44a082c29fc153bff6a7681370aa7fe0bf41a5b") ("asilea.elc" 10931 "edd8d3d3137c47cd7086e96c26c6d9b04b2ff6268d71b3711f74011cd51cebd3"))"#
        ]],
    )
}

fn asilea_complete_callable_command_arglist_doc_and_source_surface_matches() -> ParityBatchCase {
    ParityBatchCase::value(
        "asilea_complete_callable_command_arglist_doc_and_source_surface_matches",
        r##"(mapcar
         (lambda (symbol)
           (list
            symbol
            (fboundp symbol)
            (macrop symbol)
            (commandp symbol)
            (interactive-form symbol)
            (help-function-arglist symbol t)
            (let ((doc (documentation symbol t)))
              (and doc (secure-hash 'sha256 doc)))
            (let ((file
                   (symbol-file symbol 'defun)))
              (and file (file-name-nondirectory file)))))
         '(asilea-run
           asilea-run-synchronously
           asilea-default-acceptance-function
           asilea--sanitize-variables
           asilea--initial-temperature
           asilea--state-to-option-list
           asilea--neighboring-state
           asilea--generate-random-state
           asilea--start-process))"##,
        expect![[
            r#"OK ((asilea-run t nil nil nil (program options) "4b391b975642789ccbb8e9d2e5cf40012e2c911e6b8ae7241c30fdcc08aac0e8" "asilea.el") (asilea-run-synchronously t nil nil nil (program options) "76acfe22f86b640d089a82fb7c26bd2cea4cff002af5fc840fd4578852bddf3b" "asilea.el") (asilea-default-acceptance-function t nil nil nil (new-energy old-energy temperature random-function) "472a801d89b8ac1ce62b2def121c4d577f9485ad39d9c43efff28ef895832710" "asilea.el") (asilea--sanitize-variables t nil nil nil nil "e7a1a5999dc97bc1e8ae89730cae712e10b744297637c331a0b7c78efa05f43d" "asilea.el") (asilea--initial-temperature t nil nil nil nil "5bde1ebbb273a2f86955a5a7384479c4eda6e2ab0f5ce6601b872f867789543d" "asilea.el") (asilea--state-to-option-list t nil nil nil (state options) "260eb446e6a3d21b8e3d77134cd9eb1689a3033308d7ac718cdca1eab5a11b9f" "asilea.el") (asilea--neighboring-state t nil nil nil (state options random-function) "1cf62abfb9ce7782ecaa3765a07b0de9912a3676e6242de54e2337d15b45c0ef" "asilea.el") (asilea--generate-random-state t nil nil nil (options random-function) "8fc6c3806bb6d86dff16ff5d40cc35ae112d0198055bc7ffa7a4790ad11790dc" "asilea.el") (asilea--start-process t nil nil nil (program state options) "c9c5c64416c9c7d7813e36bf8b13727bef993876b8e12278df70af072f919149" "asilea.el"))"#
        ]],
    )
}

fn asilea_every_configuration_variable_default_scope_doc_and_source_match() -> ParityBatchCase {
    ParityBatchCase::value(
        "asilea_every_configuration_variable_default_scope_doc_and_source_match",
        r##"(mapcar
         (lambda (symbol)
           (list
            symbol
            (symbol-value symbol)
            (default-value symbol)
            (default-boundp symbol)
            (special-variable-p symbol)
            (local-variable-if-set-p symbol)
            (custom-variable-p symbol)
            (get symbol 'custom-type)
            (get symbol 'custom-group)
            (let ((doc
                   (documentation-property
                    symbol
                    'variable-documentation
                    t)))
              (and doc (secure-hash 'sha256 doc)))
            (let ((file
                   (symbol-file symbol 'defvar)))
              (and file (file-name-nondirectory file)))))
         '(asilea-random-generator-function
           asilea-concurrent-jobs
           asilea-max-steps
           asilea-cooling-rate
           asilea-initial-temperature
           asilea-final-temperature
           asilea-acceptance-function
           asilea-parse-energy-function
           asilea-report-candidate-function
           asilea-solution-accepted-function
           asilea-finished-function))"##,
        expect![[
            r#"OK ((asilea-random-generator-function cl-random cl-random t t nil nil nil nil "651d50e588ae94bc706347b3dea75e94c37a38819c9dbe561a3164cd1e4f816d" "asilea.el") (asilea-concurrent-jobs 1 1 t t nil nil nil nil "e579c84ae4df90b60e2f3a344d87bd3d66e124b70f901a76f255843dc1b0751e" "asilea.el") (asilea-max-steps nil nil t t nil nil nil nil "512f2d0d97806fe041ecc452f5a0df542e6658d04403a3c9aa18582f021360ce" "asilea.el") (asilea-cooling-rate 0.005 0.005 t t nil nil nil nil "b2db98ea18c72df7ead334667d7c88a4240eb48f72065ab2a8676a9290491779" "asilea.el") (asilea-initial-temperature nil nil t t nil nil nil nil "c9c168ef95eddff0ff3a42532e235d117939b70d25acd60ba40c442c53bf6d5e" "asilea.el") (asilea-final-temperature nil nil t t nil nil nil nil "cee9631abf505f824a5e73a1d53948cd463ec98dd406563859e15ea32f74b0c7" "asilea.el") (asilea-acceptance-function asilea-default-acceptance-function asilea-default-acceptance-function t t nil nil nil nil "60a0893825a06842e87b672fe385d413c32895ff7e3c3238f542ea0be2bd77a5" "asilea.el") (asilea-parse-energy-function string-to-number string-to-number t t nil nil nil nil "3a01c1fa9224f730b673fa3821d14453ce1f6ce4fca85d6d80f96552d8384a21" "asilea.el") (asilea-report-candidate-function ignore ignore t t nil nil nil nil "04830b219cd9de5d436554e4dc7b6e49df9d963a4e6bb1f2312b3c5e3e9c0d5a" "asilea.el") (asilea-solution-accepted-function ignore ignore t t nil nil nil nil "3e16b84dfc7db333508615cbd9857e474aaedede4a99cdf73268bcd8131718da" "asilea.el") (asilea-finished-function ignore ignore t t nil nil nil nil "68b8b09851e95dab4d8848b518c557e9f91199d8b15ed65a21fc1022470776c4" "asilea.el"))"#
        ]],
    )
}

fn asilea_load_has_no_global_keymap_hook_command_or_process_side_effects() -> ParityBatchCase {
    ParityBatchCase::value(
        "asilea_load_has_no_global_keymap_hook_command_or_process_side_effects",
        r##"(list
         (featurep 'asilea)
         (commandp 'asilea-run)
         (commandp 'asilea-run-synchronously)
         (where-is-internal 'asilea-run)
         (where-is-internal 'asilea-run-synchronously)
         (seq-filter
          (lambda (buffer)
            (string-match-p
             "asilea"
             (buffer-name buffer)))
          (buffer-list))
         (seq-filter
          (lambda (process)
            (string-match-p
             "asilea"
             (process-name process)))
          (process-list)))"##,
        expect!["OK (t nil nil nil nil nil nil)"],
    )
}

fn asilea_generated_autoload_file_exposes_run_entrypoints_without_loading_feature()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asilea_generated_autoload_file_exposes_run_entrypoints_without_loading_feature",
        r##"(list
         (featurep 'asilea)
         (mapcar
          (lambda (symbol)
            (list
             symbol
             (fboundp symbol)
             (and
              (fboundp symbol)
              (autoloadp
               (symbol-function symbol)))
             (commandp symbol)
             (let ((file
                    (symbol-file symbol 'defun)))
               (and file (file-name-nondirectory file)))))
          '(asilea-run
            asilea-run-synchronously
            asilea-default-acceptance-function
            asilea--state-to-option-list)))"##,
        expect![
            "OK (nil ((asilea-run nil nil nil nil) (asilea-run-synchronously nil nil nil nil) (asilea-default-acceptance-function nil nil nil nil) (asilea--state-to-option-list nil nil nil nil)))"
        ],
    )
}

pub(super) fn registry_asilea_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        asilea_exact_pin_descriptor_dependencies_origin_and_feature_contract_match(),
        asilea_installed_payload_inventory_sizes_and_content_digests_match(),
        asilea_complete_callable_command_arglist_doc_and_source_surface_matches(),
        asilea_every_configuration_variable_default_scope_doc_and_source_match(),
        asilea_load_has_no_global_keymap_hook_command_or_process_side_effects(),
    ]
}

pub(super) fn registry_asilea_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![asilea_generated_autoload_file_exposes_run_entrypoints_without_loading_feature()]
}
