use expect_test::expect;

use super::ParityBatchCase;

fn asdf_vm_tool_versions_row_decodes_and_encodes_real_version_selectors() -> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_tool_versions_row_decodes_and_encodes_real_version_selectors",
        r##"(mapcar
               (lambda (line)
                 (let ((row
                        (asdf-vm-ui--decode
                         'asdf-vm-tool-versions--file-row
                         line)))
                   (list
                    (slot-value row
                                'tool)
                    (slot-value row
                                'versions)
                    (asdf-vm-ui--encode
                     row))))
               '("ruby 3.3.1 system"
                 "nodejs ref:feature/lts path:/work/node"
                 "資料 λ-version latest"
                 "single"
                 " spaced\t20.1   system "))"##,
        expect![[
            r#"OK (("ruby" ("3.3.1" "system") "ruby 3.3.1 system") ("nodejs" ("ref:feature/lts" "path:/work/node") "nodejs ref:feature/lts path:/work/node") ("資料" ("λ-version" "latest") "資料 λ-version latest") ("single" nil "single") ("spaced" ("20.1" "system") "spaced 20.1 system"))"#
        ]],
    )
}

fn asdf_vm_tool_versions_file_decodes_and_reencodes_ordered_rows() -> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_tool_versions_file_decodes_and_reencodes_ordered_rows",
        r##"(let* ((text
                     (concat
                      "ruby 3.3.1 system\n"
                      "nodejs 20.11.0 lts\n"
                      "python path:/work/python ref:main\n"
                      "資料 λ-version\n"))
                    (object
                     (asdf-vm-ui--decode
                      'asdf-vm-tool-versions--file
                      text)))
               (list
                (mapcar
                 (lambda (row)
                   (list
                    (slot-value row
                                'tool)
                    (slot-value row
                                'versions)))
                 (slot-value object
                             'rows))
                (asdf-vm-ui--encode
                 object)))"##,
        expect![[
            r#"OK ((("ruby" ("3.3.1" "system")) ("nodejs" ("20.11.0" "lts")) ("python" ("path:/work/python" "ref:main")) ("資料" ("λ-version"))) "ruby 3.3.1 system\nnodejs 20.11.0 lts\npython path:/work/python ref:main\n資料 λ-version")"#
        ]],
    )
}

fn asdf_vm_tool_versions_file_exposes_comment_blank_and_empty_input_behavior() -> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_tool_versions_file_exposes_comment_blank_and_empty_input_behavior",
        r##"(mapcar
               (lambda (text)
                 (asdf-vm-test-error-data
                  (lambda ()
                    (let ((object
                           (asdf-vm-ui--decode
                            'asdf-vm-tool-versions--file
                            text)))
                      (mapcar
                       #'asdf-vm-ui--encode
                       (slot-value object
                                   'rows))))))
               '("ruby 3.3.1 # project runtime\n"
                 "# full comment\nruby 3.3.1\n"
                 "ruby 3.3.1\n\nnodejs 20.0\n"
                 ""
                 "   \n"))"##,
        expect![[
            r#"OK ((:ok ("ruby 3.3.1")) (:error wrong-type-argument (stringp nil)) (:error wrong-type-argument (stringp nil)) (:error wrong-type-argument (stringp nil)) (:error invalid-slot-type (asdf-vm-tool-versions--file-row tool string nil)))"#
        ]],
    )
}

fn asdf_vm_tool_versions_real_file_round_trip_preserves_row_order_and_writes_mutations()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_tool_versions_real_file_round_trip_preserves_row_order_and_writes_mutations",
        r##"(let* ((input
                     (asdf-vm-test-path
                      "tool-versions/input"))
                    (output
                     (asdf-vm-test-path
                      "tool-versions/output")))
               (asdf-vm-test-write-file
                input
                (concat
                 "ruby 3.2.0\n"
                 "nodejs 20.0 system\n"))
               (let* ((object
                       (asdf-vm-ui--read
                        'asdf-vm-tool-versions--file
                        input))
                      (rows
                       (slot-value object
                                   'rows))
                      (ruby
                       (car rows)))
                 (setf
                  (slot-value ruby
                              'versions)
                  '("3.3.1"
                    "system"))
                 (asdf-vm-ui--write
                  object output)
                 (list
                  (slot-value object
                              'path)
                  (mapcar
                   #'asdf-vm-ui--encode
                   rows)
                  (asdf-vm-test-read-file
                   output))))"##,
        expect![[
            r#"OK ("[ORACLE-SANDBOX]/tool-versions/input" ("ruby 3.3.1 system" "nodejs 20.0 system") "ruby 3.3.1 system\nnodejs 20.0 system\n")"#
        ]],
    )
}

fn asdf_vm_tool_versions_location_prefers_deepest_readable_dominating_file() -> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_tool_versions_location_prefers_deepest_readable_dominating_file",
        r##"(let* ((home
                     (file-name-as-directory
                      (asdf-vm-test-path
                       "location/home")))
                    (project
                     (file-name-as-directory
                      (asdf-vm-test-path
                       "location/project")))
                    (nested
                     (expand-file-name
                      "src/deep/"
                      project))
                    (buffer-file
                     (expand-file-name
                      "main.rb"
                      nested))
                    (default-file
                     (expand-file-name
                      ".tool-versions"
                      home))
                    (project-file
                     (expand-file-name
                      ".tool-versions"
                      project)))
               (make-directory nested t)
               (asdf-vm-test-write-file
                buffer-file "puts :ok")
               (asdf-vm-test-write-file
                default-file "ruby 3.1")
               (asdf-vm-test-write-file
                project-file "ruby 3.3")
               (let ((buffer-file-name
                      buffer-file)
                     (default-directory
                      nested)
                     (process-environment
                      (copy-sequence
                       process-environment)))
                 (setenv "HOME" home)
                 (list
                  (asdf-vm-tool-versions--locate-dominating-file)
                  project-file
                  default-file)))"##,
        expect![[
            r#"OK ("[ORACLE-SANDBOX]/location/project/.tool-versions" "[ORACLE-SANDBOX]/location/project/.tool-versions" "[ORACLE-SANDBOX]/location/home/.tool-versions")"#
        ]],
    )
}

fn asdf_vm_tool_versions_location_uses_project_then_home_fallbacks_and_skips_unreadable_candidates()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_tool_versions_location_uses_project_then_home_fallbacks_and_skips_unreadable_candidates",
        r##"(let ((buffer-file-name
                    "/work/src/main.ex")
                   (calls nil))
               (cl-letf
                   (((symbol-function
                      'featurep)
                     (lambda (feature)
                       (eq feature
                           'project)))
                    ((symbol-function
                      'current-project)
                     (lambda ()
                       '(vc . "/work/project/")))
                    ((symbol-function
                      'locate-dominating-file)
                     (lambda (&rest arguments)
                       (push
                        (cons
                         :locate arguments)
                        calls)
                       "/work/src/"))
                    ((symbol-function
                      'file-readable-p)
                     (lambda (path)
                       (push
                        (list
                         :readable path)
                        calls)
                       (member
                        path
                        '("/work/project/.tool-versions"
                          "/home/test/.tool-versions"))))
                    ((symbol-function
                      'expand-file-name)
                     (let ((original
                            (symbol-function
                             'expand-file-name)))
                       (lambda (name &optional directory)
                         (if
                             (equal directory
                                    "~")
                             "/home/test/.tool-versions"
                           (funcall
                            original name directory))))))
                 (list
                  (asdf-vm-tool-versions--locate-dominating-file)
                  (nreverse calls))))"##,
        expect![[
            r#"OK ("/work/project/.tool-versions" ((:locate "/work/src/main.ex" ".tool-versions") (:readable "/work/project/.tool-versions")))"#
        ]],
    )
}

fn asdf_vm_tool_versions_widget_completers_transform_tool_ref_path_and_version_choices()
-> ParityBatchCase {
    ParityBatchCase::value(
        "asdf_vm_tool_versions_widget_completers_transform_tool_ref_path_and_version_choices",
        r##"(let ((version-results
                    '("ref:"
                      "path"
                      "3.3.1"))
                   calls)
               (cl-letf
                   (((symbol-function
                      'widget-field-value-get)
                     (lambda (widget)
                       (push
                        (list
                         :get widget)
                        calls)
                       (pcase widget
                         ('tool-widget
                          "rub")
                         ('tool-name-widget
                          "ruby")
                         (_ ""))))
                    ((symbol-function
                      'widget-field-value-set)
                     (lambda (widget value)
                       (push
                        (list
                         :set widget value)
                        calls)
                       value))
                    ((symbol-function
                      'widget-get)
                     (lambda (widget property)
                       (pcase
                           (list widget property)
                         (`(version-widget :parent)
                          'repeat-widget)
                         (`(repeat-widget :parent)
                          'object-slot-widget)
                         (`(object-slot-widget :parent)
                          'object-edit-widget)
                         (`(object-edit-widget :children)
                          '(slot-widget))
                         (`(slot-widget :children)
                          '(tool-name-widget))
                         (_ nil))))
                    ((symbol-function
                      'asdf-vm-plugin--installed-plugin-completing-read)
                     (lambda (&rest arguments)
                       (push
                        (cons
                         :tool-completion arguments)
                        calls)
                       "ruby"))
                    ((symbol-function
                      'asdf-vm--installed-package-version-completing-read)
                     (lambda (&rest arguments)
                       (push
                        (cons
                         :version-completion arguments)
                        calls)
                       (pop version-results)))
                    ((symbol-function
                      'read-string)
                     (lambda (&rest arguments)
                       (push
                        (cons
                         :read-string arguments)
                        calls)
                       "feature/λ"))
                    ((symbol-function
                      'read-file-name)
                     (lambda (&rest arguments)
                       (push
                        (cons
                         :read-file arguments)
                        calls)
                       "/work/local tool/")))
                 (list
                  (asdf-vm-tool-versions--file-row-tool-complete
                   'tool-widget)
                  (asdf-vm-tool-versions--file-row-versions-complete
                   'version-widget)
                  (asdf-vm-tool-versions--file-row-versions-complete
                   'version-widget)
                  (asdf-vm-tool-versions--file-row-versions-complete
                   'version-widget)
                  (nreverse calls))))"##,
        expect![[
            r#"OK ("ruby" "ref:feature/λ" "path:/work/local tool/" "3.3.1" ((:get tool-widget) (:tool-completion nil t "rub") (:set tool-widget "ruby") (:get tool-name-widget) (:version-completion "ruby" nil t) (:read-string "Tool git ref: ") (:set version-widget "ref:feature/λ") (:get tool-name-widget) (:version-completion "ruby" nil t) (:read-file "Tool path: " nil nil t) (:set version-widget "path:/work/local tool/") (:get tool-name-widget) (:version-completion "ruby" nil t) (:set version-widget "3.3.1")))"#
        ]],
    )
}

fn asdf_vm_tool_versions_edit_reads_existing_or_constructs_missing_file_object() -> ParityBatchCase
{
    ParityBatchCase::value(
        "asdf_vm_tool_versions_edit_reads_existing_or_constructs_missing_file_object",
        r##"(let* ((existing
                     (asdf-vm-test-path
                      "tool-edit/existing"))
                    (missing
                     (asdf-vm-test-path
                      "tool-edit/missing"))
                    calls)
               (asdf-vm-test-write-file
                existing
                "ruby 3.3.1\n")
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
                         (mapcar
                          #'asdf-vm-ui--encode
                          (slot-value object
                                      'rows)))
                        calls)
                       :customized)))
                 (list
                  (asdf-vm-tool-versions-edit
                   existing)
                  (asdf-vm-tool-versions-edit
                   missing)
                  (nreverse calls))))"##,
        expect![[
            r#"OK (:customized :customized ((asdf-vm-tool-versions--file "[ORACLE-SANDBOX]/tool-edit/existing" ("ruby 3.3.1")) (asdf-vm-tool-versions--file "[ORACLE-SANDBOX]/tool-edit/missing" nil)))"#
        ]],
    )
}

pub(super) fn tool_versions_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        asdf_vm_tool_versions_row_decodes_and_encodes_real_version_selectors(),
        asdf_vm_tool_versions_file_decodes_and_reencodes_ordered_rows(),
        asdf_vm_tool_versions_file_exposes_comment_blank_and_empty_input_behavior(),
        asdf_vm_tool_versions_real_file_round_trip_preserves_row_order_and_writes_mutations(),
        asdf_vm_tool_versions_location_prefers_deepest_readable_dominating_file(),
        asdf_vm_tool_versions_location_uses_project_then_home_fallbacks_and_skips_unreadable_candidates(),
        asdf_vm_tool_versions_widget_completers_transform_tool_ref_path_and_version_choices(),
        asdf_vm_tool_versions_edit_reads_existing_or_constructs_missing_file_object(),
    ]
}
