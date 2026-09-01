use expect_test::expect;

use super::ParityBatchCase;

fn aurora_config_mode_activation_has_exact_python_derived_buffer_contract() -> ParityBatchCase {
    ParityBatchCase::value(
        "aurora_config_mode_activation_has_exact_python_derived_buffer_contract",
        r##"(with-temp-buffer
          (insert
           "job = Job(name='demo')\n")
          (set-buffer-modified-p nil)
          (aurora-config-mode)
          (list
           (aurora-config-test-buffer-state)
           (derived-mode-p
            'prog-mode)
           (derived-mode-p
            'fundamental-mode)
           (eq
            (current-local-map)
            aurora-config-mode-map)
           (eq
            (syntax-table)
            aurora-config-mode-syntax-table)
           local-abbrev-table
           comment-start
           comment-start-skip
           indent-line-function
           parse-sexp-lookup-properties
           (and
            syntax-propertize-function
            (functionp
             syntax-propertize-function))
           (buffer-modified-p)))"##,
        expect![[
            r##"OK ((aurora-config-mode "Aurora" python-mode "job = Job(name='demo')\n" nil nil t 6 aurora-config-inspect aurora-config-diff) prog-mode nil t t #<obarray n=1> "# " "#+\\s-*" python-indent-line-function t t nil)"##
        ]],
    )
}

fn aurora_config_mode_keymap_exposes_exact_prefix_commands_and_inherits_python_bindings()
-> ParityBatchCase {
    ParityBatchCase::value(
        "aurora_config_mode_keymap_exposes_exact_prefix_commands_and_inherits_python_bindings",
        r##"(list
          (keymapp
           aurora-config-mode-map)
          (keymap-parent
           aurora-config-mode-map)
          (mapcar
           (lambda (key)
             (list
              key
              (lookup-key
               aurora-config-mode-map
               (kbd key))))
           '("C-c a"
             "C-c a i"
             "C-c a d"
             "C-c a x"
             "C-c C-r"
             "C-c C-c"
             "M-."
             "TAB"))
          (with-temp-buffer
            (aurora-config-mode)
            (mapcar
             (lambda (key)
               (list
                key
                (key-binding
                 (kbd key))))
             '("C-c a i"
               "C-c a d"
               "C-c a x"
               "TAB"))))"##,
        expect![[
            r#"OK (t nil (("C-c a" (keymap (100 . aurora-config-diff) (105 . aurora-config-inspect))) ("C-c a i" aurora-config-inspect) ("C-c a d" aurora-config-diff) ("C-c a x" nil) ("C-c C-r" nil) ("C-c C-c" nil) ("M-." nil) ("TAB" nil)) (("C-c a i" aurora-config-inspect) ("C-c a d" aurora-config-diff) ("C-c a x" nil) ("TAB" indent-for-tab-command)))"#
        ]],
    )
    .fresh_process()
}

fn aurora_config_mode_font_lock_defaults_append_exact_rules_without_mutating_python_global()
-> ParityBatchCase {
    ParityBatchCase::value(
        "aurora_config_mode_font_lock_defaults_append_exact_rules_without_mutating_python_global",
        r##"(let ((python-before
                (copy-tree
                 python-font-lock-keywords)))
          (with-temp-buffer
            (aurora-config-mode)
            (list
             (length
              python-before)
             (length
              python-font-lock-keywords)
             (equal
              python-before
              python-font-lock-keywords)
             (length
              (car font-lock-defaults))
             (equal
              (seq-take
               (car font-lock-defaults)
               (length python-before))
              python-before)
             (seq-drop
              (car font-lock-defaults)
              (length python-before))
             (equal
              (seq-drop
               (car font-lock-defaults)
               (length python-before))
              aurora-config-font-lock-keywords)
             (cdr font-lock-defaults))))"##,
        expect![[
            r#"OK (4 4 t 6 t (("\\_<\\(HealthCheckConfig\\|J\\(?:VMProcess\\|ob\\)\\|Process\\|Resources\\|Se\\(?:quentialTask\\|rvice\\)\\|Task\\|UpdateConfig\\)\\_>" . font-lock-function-name-face) ("\\_<\\(Enum\\|Integer\\|List\\|Map\\|Str\\(?:ing\\|uct\\)\\)\\_>" . font-lock-type-face)) t (nil nil nil nil (font-lock-syntactic-face-function . python-font-lock-syntactic-face-function)))"#
        ]],
    )
}

fn aurora_config_mode_repeated_activation_rebuilds_locals_without_duplicate_font_lock_rules()
-> ParityBatchCase {
    ParityBatchCase::value(
        "aurora_config_mode_repeated_activation_rebuilds_locals_without_duplicate_font_lock_rules",
        r##"(with-temp-buffer
          (insert
           "job = Job(task=Task())\n")
          (aurora-config-mode)
          (let ((first
                 (list
                  (aurora-config-test-error-data
                   (lambda ()
                     (font-lock-ensure)))
                  (length
                   (car font-lock-defaults))
                  (copy-tree
                   (car font-lock-defaults))
                  (aurora-config-test-face-runs))))
            (setq
             aurora-config-last-job-path
             "buffer/sentinel")
            (aurora-config-mode)
            (let ((second
                   (list
                    (aurora-config-test-error-data
                     (lambda ()
                       (font-lock-ensure)))
                    (length
                     (car font-lock-defaults))
                    (copy-tree
                     (car font-lock-defaults))
                    (aurora-config-test-face-runs))))
              (list
               first
               second
               (equal first second)
               aurora-config-last-job-path
               (local-variable-p
                'aurora-config-last-job-path)))))"##,
        expect![[
            r#"OK (((:error wrong-type-argument (listp font-lock-type-face)) 6 (python-font-lock-keywords-level-1 python-font-lock-keywords-level-1 python-font-lock-keywords-level-2 python-font-lock-keywords-maximum-decoration ("\\_<\\(HealthCheckConfig\\|J\\(?:VMProcess\\|ob\\)\\|Process\\|Resources\\|Se\\(?:quentialTask\\|rvice\\)\\|Task\\|UpdateConfig\\)\\_>" . font-lock-function-name-face) ("\\_<\\(Enum\\|Integer\\|List\\|Map\\|Str\\(?:ing\\|uct\\)\\)\\_>" . font-lock-type-face)) nil) ((:error wrong-type-argument (listp font-lock-type-face)) 6 (python-font-lock-keywords-level-1 python-font-lock-keywords-level-1 python-font-lock-keywords-level-2 python-font-lock-keywords-maximum-decoration ("\\_<\\(HealthCheckConfig\\|J\\(?:VMProcess\\|ob\\)\\|Process\\|Resources\\|Se\\(?:quentialTask\\|rvice\\)\\|Task\\|UpdateConfig\\)\\_>" . font-lock-function-name-face) ("\\_<\\(Enum\\|Integer\\|List\\|Map\\|Str\\(?:ing\\|uct\\)\\)\\_>" . font-lock-type-face)) nil) t "smf1/" nil)"#
        ]],
    )
    .fresh_process()
}

fn aurora_config_mode_hook_runs_after_python_setup_and_can_observe_and_mutate_buffer_state()
-> ParityBatchCase {
    ParityBatchCase::value(
        "aurora_config_mode_hook_runs_after_python_setup_and_can_observe_and_mutate_buffer_state",
        r##"(with-temp-buffer
          (insert
           "task = Task()\n")
          (let (observations)
            (let ((aurora-config-mode-hook
                   (list
                    (lambda ()
                      (push
                       (list
                        :first
                        major-mode
                        mode-name
                        (derived-mode-p
                         'python-mode)
                        (length
                         (car font-lock-defaults))
                        (lookup-key
                         (current-local-map)
                         (kbd "C-c a i")))
                       observations)
                      (setq-local
                       aurora-config-last-job-path
                       "hook/first"))
                    (lambda ()
                      (push
                       (list
                        :second
                        aurora-config-last-job-path
                        (local-variable-p
                         'aurora-config-last-job-path))
                       observations)
                      (insert
                       "# hook suffix\n")))))
              (aurora-config-mode))
            (list
             (nreverse observations)
             (aurora-config-test-buffer-state))))"##,
        expect![[
            r#"OK (((:first aurora-config-mode "Aurora" python-mode 6 aurora-config-inspect) (:second "hook/first" t)) (aurora-config-mode "Aurora" python-mode "task = Task()\n# hook suffix\n" t "hook/first" t 6 aurora-config-inspect aurora-config-diff))"#
        ]],
    )
}

fn aurora_config_mode_auto_mode_rules_select_expected_files_and_reject_near_misses()
-> ParityBatchCase {
    ParityBatchCase::value(
        "aurora_config_mode_auto_mode_rules_select_expected_files_and_reject_near_misses",
        r##"(mapcar
          (lambda (name)
            (with-temp-buffer
              (setq
               buffer-file-name
               (expand-file-name
                name
                default-directory))
              (set-auto-mode)
              (list
               name
               major-mode
               mode-name
               (derived-mode-p
                'aurora-config-mode))))
          '("service.aurora"
            "service.mesos"
            "SERVICE.AURORA"
            "SERVICE.MESOS"
            "service.aurora.bak"
            ".aurora"
            "aurora"
            "service.py"
            "mesos"))"##,
        expect![[
            r#"OK (("service.aurora" aurora-config-mode "Aurora" aurora-config-mode) ("service.mesos" aurora-config-mode "Aurora" aurora-config-mode) ("SERVICE.AURORA" aurora-config-mode "Aurora" aurora-config-mode) ("SERVICE.MESOS" aurora-config-mode "Aurora" aurora-config-mode) ("service.aurora.bak" aurora-config-mode "Aurora" aurora-config-mode) (".aurora" aurora-config-mode "Aurora" aurora-config-mode) ("aurora" fundamental-mode "Fundamental" nil) ("service.py" python-mode "Python" nil) ("mesos" fundamental-mode "Fundamental" nil))"#
        ]],
    )
}

fn aurora_config_mode_real_python_indentation_produces_exact_nested_job_configuration()
-> ParityBatchCase {
    ParityBatchCase::value(
        "aurora_config_mode_real_python_indentation_produces_exact_nested_job_configuration",
        r##"(with-temp-buffer
          (insert
           "def build(name):\n"
           "return Job(\n"
           "name=name,\n"
           "task=Task(\n"
           "processes=[\n"
           "Process(name='web'),\n"
           "Service(name='sidecar')]))\n")
          (aurora-config-mode)
          (indent-region
           (point-min)
           (point-max))
          (list
           (aurora-config-test-error-data
            (lambda ()
              (font-lock-ensure)))
           (buffer-string)
           (aurora-config-test-face-runs)
           (mapcar
            (lambda (line)
              (goto-char
               (point-min))
              (forward-line line)
              (current-indentation))
            '(0 1 2 3 4 5 6 7))))"##,
        expect![[
            r#"OK ((:error wrong-type-argument (listp font-lock-type-face)) "def build(name):\nreturn Job(\n    name=name,\n    task=Task(\n        processes=[\n            Process(name='web'),\n            Service(name='sidecar')]))\n" nil (0 0 4 4 8 12 12 0))"#
        ]],
    )
}

fn aurora_config_mode_python_comment_and_uncomment_workflow_preserves_code_exactly()
-> ParityBatchCase {
    ParityBatchCase::value(
        "aurora_config_mode_python_comment_and_uncomment_workflow_preserves_code_exactly",
        r##"(with-temp-buffer
          (insert
           "job = Job(\n"
           "    name='api',\n"
           "    task=Task())\n")
          (aurora-config-mode)
          (let ((original
                 (buffer-string))
                commented
                uncommented)
            (comment-region
             (point-min)
             (point-max))
            (setq commented
                  (buffer-string))
            (uncomment-region
             (point-min)
             (point-max))
            (setq uncommented
                  (buffer-string))
            (list
             comment-start
             comment-end
             original
             commented
             uncommented
             (equal
              original
              uncommented))))"##,
        expect![[
            r##"OK ("# " "" "job = Job(\n    name='api',\n    task=Task())\n" "# job = Job(\n#     name='api',\n#     task=Task())\n" "job = Job(\n    name='api',\n    task=Task())\n" t)"##
        ]],
    )
}

fn aurora_config_mode_activation_kills_unrelated_locals_but_preserves_text_point_and_narrowing()
-> ParityBatchCase {
    ParityBatchCase::value(
        "aurora_config_mode_activation_kills_unrelated_locals_but_preserves_text_point_and_narrowing",
        r##"(with-temp-buffer
          (insert
           "prefix\njob = Job()\nsuffix\n")
          (goto-char
           (point-min))
          (forward-line 1)
          (let ((start
                 (point)))
            (forward-line 1)
            (narrow-to-region
             start
             (point)))
          (goto-char
           (point-max))
          (setq-local
           fixture-local
           :sentinel)
          (set-buffer-modified-p nil)
          (let ((before
                 (list
                  (point-min)
                  (point-max)
                  (point)
                  (buffer-string)
                  (local-variable-p
                   'fixture-local))))
            (aurora-config-mode)
            (list
             before
             (list
              (point-min)
              (point-max)
              (point)
              (buffer-string)
              (boundp 'fixture-local)
              (local-variable-p
               'fixture-local)
              (buffer-modified-p)
              major-mode))))"##,
        expect![[
            r#"OK ((8 20 20 "job = Job()\n" t) (8 20 20 "job = Job()\n" nil nil nil aurora-config-mode))"#
        ]],
    )
}

pub(super) fn mode_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        aurora_config_mode_activation_has_exact_python_derived_buffer_contract(),
        aurora_config_mode_keymap_exposes_exact_prefix_commands_and_inherits_python_bindings(),
        aurora_config_mode_font_lock_defaults_append_exact_rules_without_mutating_python_global(),
        aurora_config_mode_repeated_activation_rebuilds_locals_without_duplicate_font_lock_rules(),
        aurora_config_mode_hook_runs_after_python_setup_and_can_observe_and_mutate_buffer_state(),
        aurora_config_mode_auto_mode_rules_select_expected_files_and_reject_near_misses(),
        aurora_config_mode_real_python_indentation_produces_exact_nested_job_configuration(),
        aurora_config_mode_python_comment_and_uncomment_workflow_preserves_code_exactly(),
        aurora_config_mode_activation_kills_unrelated_locals_but_preserves_text_point_and_narrowing(
        ),
    ]
}
