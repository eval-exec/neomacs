use expect_test::expect;

use super::ParityBatchCase;

fn aurora_config_mode_keyword_families_preserve_complete_order_counts_and_uniqueness()
-> ParityBatchCase {
    ParityBatchCase::value(
        "aurora_config_mode_keyword_families_preserve_complete_order_counts_and_uniqueness",
        r##"(list
          aurora-config-aurora-struct-keywords
          (length
           aurora-config-aurora-struct-keywords)
          (length
           (delete-dups
            (copy-sequence
             aurora-config-aurora-struct-keywords)))
          aurora-config-pystachio-struct-keywords
          (length
           aurora-config-pystachio-struct-keywords)
          (length
           (delete-dups
            (copy-sequence
             aurora-config-pystachio-struct-keywords)))
          (append
           aurora-config-aurora-struct-keywords
           aurora-config-pystachio-struct-keywords))"##,
        expect![[
            r#"OK (("HealthCheckConfig" "Job" "Process" "JVMProcess" "Resources" "SequentialTask" "Service" "Task" "UpdateConfig") 9 9 #1=("Enum" "Integer" "List" "Map" "String" "Struct") 6 6 ("HealthCheckConfig" "Job" "Process" "JVMProcess" "Resources" "SequentialTask" "Service" "Task" "UpdateConfig" . #1#))"#
        ]],
    )
}

fn aurora_config_mode_font_lock_rules_have_exact_regexps_faces_and_matching_matrix()
-> ParityBatchCase {
    ParityBatchCase::value(
        "aurora_config_mode_font_lock_rules_have_exact_regexps_faces_and_matching_matrix",
        r##"(let ((aurora-regexp
                (caar
                 aurora-config-font-lock-keywords))
               (pystachio-regexp
                (caadr
                 aurora-config-font-lock-keywords))
               (samples
                '("Job"
                  "xJob"
                  "Job2"
                  "_Job"
                  "HealthCheckConfig"
                  "JVMProcess"
                  "UpdateConfig"
                  "String"
                  "string"
                  "StringMap"
                  "Map"
                  "Map.Entry"
                  "Struct"
                  "Enum")))
          (list
           aurora-config-font-lock-keywords
           (mapcar
            (lambda (sample)
              (list
               sample
               (when
                   (string-match
                    aurora-regexp
                    sample)
                 (list
                  (match-string 0 sample)
                  (match-beginning 0)
                  (match-end 0)))
               (when
                   (string-match
                    pystachio-regexp
                    sample)
                 (list
                  (match-string 0 sample)
                  (match-beginning 0)
                  (match-end 0)))))
            samples)))"##,
        expect![[
            r#"OK ((("\\_<\\(HealthCheckConfig\\|J\\(?:VMProcess\\|ob\\)\\|Process\\|Resources\\|Se\\(?:quentialTask\\|rvice\\)\\|Task\\|UpdateConfig\\)\\_>" . font-lock-function-name-face) ("\\_<\\(Enum\\|Integer\\|List\\|Map\\|Str\\(?:ing\\|uct\\)\\)\\_>" . font-lock-type-face)) (("Job" ("Job" 0 3) nil) ("xJob" nil nil) ("Job2" nil nil) ("_Job" nil nil) ("HealthCheckConfig" ("HealthCheckConfig" 0 17) nil) ("JVMProcess" ("JVMProcess" 0 10) nil) ("UpdateConfig" ("UpdateConfig" 0 12) nil) ("String" nil ("String" 0 6)) ("string" nil ("string" 0 6)) ("StringMap" nil nil) ("Map" nil ("Map" 0 3)) ("Map.Entry" nil nil) ("Struct" nil ("Struct" 0 6)) ("Enum" nil ("Enum" 0 4))))"#
        ]],
    )
}

fn aurora_config_mode_fontifying_every_struct_surfaces_legacy_dotted_rule_failure()
-> ParityBatchCase {
    ParityBatchCase::value(
        "aurora_config_mode_fontifying_every_struct_surfaces_legacy_dotted_rule_failure",
        r##"(with-temp-buffer
          (insert
           "job = Job(\n"
           "  task = Task(\n"
           "    processes = [Process(), JVMProcess()],\n"
           "    resources = Resources(),\n"
           "    constraints = HealthCheckConfig(),\n"
           "    update = UpdateConfig(),\n"
           "    service = Service(),\n"
           "    sequence = SequentialTask()))\n"
           "schema = Struct(\n"
           "  enum = Enum,\n"
           "  count = Integer,\n"
           "  names = List(String),\n"
           "  mapping = Map(String, Integer))\n")
          (aurora-config-mode)
          (list
           (aurora-config-test-buffer-state)
           (aurora-config-test-error-data
            (lambda ()
              (font-lock-ensure)))
           (aurora-config-test-face-runs)))"##,
        expect![[
            r#"OK ((aurora-config-mode "Aurora" python-mode "job = Job(\n  task = Task(\n    processes = [Process(), JVMProcess()],\n    resources = Resources(),\n    constraints = HealthCheckConfig(),\n    update = UpdateConfig(),\n    service = Service(),\n    sequence = SequentialTask()))\nschema = Struct(\n  enum = Enum,\n  count = Integer,\n  names = List(String),\n  mapping = Map(String, Integer))\n" t nil t 6 aurora-config-inspect aurora-config-diff) (:error wrong-type-argument (listp font-lock-type-face)) nil)"#
        ]],
    )
}

fn aurora_config_mode_context_matrix_fails_before_font_lock_applies_any_face() -> ParityBatchCase {
    ParityBatchCase::value(
        "aurora_config_mode_context_matrix_fails_before_font_lock_applies_any_face",
        r##"(with-temp-buffer
          (insert
           "Job = 1\n"
           "job = 2\n"
           "JobFactory = Job\n"
           "xJob = 3\n"
           "_Job = 4\n"
           "String = \"Job String\"\n"
           "# Job Struct Map in a comment\n"
           "value = 'HealthCheckConfig'\n")
          (aurora-config-mode)
          (let ((font-lock-result
                 (aurora-config-test-error-data
                  (lambda ()
                    (font-lock-ensure)))))
            (list
             font-lock-result
             (aurora-config-test-face-runs)
           (mapcar
            (lambda (needle)
              (goto-char
               (point-min))
              (search-forward needle)
              (list
               needle
               (get-text-property
                (match-beginning 0)
                'face)
               (nth 8
                    (syntax-ppss
                     (match-beginning 0)))))
            '("Job ="
              "job ="
              "JobFactory"
              "xJob"
              "_Job"
              "String ="
              "\"Job"
              "# Job"
              "'HealthCheckConfig'")))))"##,
        expect![[
            r##"OK ((:error wrong-type-argument (listp font-lock-type-face)) nil (("Job =" nil nil) ("job =" nil nil) ("JobFactory" nil nil) ("xJob" nil nil) ("_Job" nil nil) ("String =" nil nil) ("\"Job" nil nil) ("# Job" nil nil) ("'HealthCheckConfig'" nil nil)))"##
        ]],
    )
}

fn aurora_config_mode_combined_python_and_aurora_fontification_fails_before_faces()
-> ParityBatchCase {
    ParityBatchCase::value(
        "aurora_config_mode_combined_python_and_aurora_fontification_fails_before_faces",
        r##"(with-temp-buffer
          (insert
           "class ServiceFactory(object):\n"
           "    def build(self, name):\n"
           "        if name is None:\n"
           "            return Job(name='fallback')\n"
           "        return Service(processes=[Process(name=name)])\n")
          (aurora-config-mode)
          (let ((font-lock-result
                 (aurora-config-test-error-data
                  (lambda ()
                    (font-lock-ensure)))))
            (list
             font-lock-result
             (aurora-config-test-face-runs)
           (mapcar
            (lambda (needle)
              (goto-char
               (point-min))
              (search-forward needle)
              (list
               needle
               (get-text-property
                (match-beginning 0)
                'face)))
            '("class"
              "ServiceFactory"
              "def"
              "build"
              "if"
              "None"
              "return"
              "Job"
              "'fallback'"
              "Service"
              "Process")))))"##,
        expect![[
            r#"OK ((:error wrong-type-argument (listp font-lock-type-face)) nil (("class" nil) ("ServiceFactory" nil) ("def" nil) ("build" nil) ("if" nil) ("None" nil) ("return" nil) ("Job" nil) ("'fallback'" nil) ("Service" nil) ("Process" nil)))"#
        ]],
    )
}

fn aurora_config_mode_mutated_keyword_lists_still_reach_compiled_legacy_rule_failure()
-> ParityBatchCase {
    ParityBatchCase::value(
        "aurora_config_mode_mutated_keyword_lists_still_reach_compiled_legacy_rule_failure",
        r##"(let ((original-aurora
                aurora-config-aurora-struct-keywords)
               (original-pystachio
                aurora-config-pystachio-struct-keywords))
          (unwind-protect
              (progn
                (setq
                 aurora-config-aurora-struct-keywords
                 '("FixtureAurora")
                 aurora-config-pystachio-struct-keywords
                 '("FixturePystachio"))
                (with-temp-buffer
                  (insert
                   "Job FixtureAurora String FixturePystachio")
                  (aurora-config-mode)
                  (list
                   aurora-config-font-lock-keywords
                   (aurora-config-test-error-data
                    (lambda ()
                      (font-lock-ensure)))
                   (aurora-config-test-face-runs))))
            (setq
             aurora-config-aurora-struct-keywords
             original-aurora
             aurora-config-pystachio-struct-keywords
             original-pystachio)))"##,
        expect![[
            r#"OK ((("\\_<\\(HealthCheckConfig\\|J\\(?:VMProcess\\|ob\\)\\|Process\\|Resources\\|Se\\(?:quentialTask\\|rvice\\)\\|Task\\|UpdateConfig\\)\\_>" . font-lock-function-name-face) ("\\_<\\(Enum\\|Integer\\|List\\|Map\\|Str\\(?:ing\\|uct\\)\\)\\_>" . font-lock-type-face)) (:error wrong-type-argument (listp font-lock-type-face)) nil)"#
        ]],
    )
}

fn aurora_config_mode_unfontify_refontify_repeats_failure_without_mutating_content()
-> ParityBatchCase {
    ParityBatchCase::value(
        "aurora_config_mode_unfontify_refontify_repeats_failure_without_mutating_content",
        r##"(with-temp-buffer
          (insert
           "task = Task(processes=[Process(), Service()])\n")
          (set-buffer-modified-p nil)
          (aurora-config-mode)
          (let ((first-result
                 (aurora-config-test-error-data
                  (lambda ()
                    (font-lock-ensure))))
                (first
                 (aurora-config-test-face-runs))
                after-unfontify
                second-result
                second)
            (font-lock-unfontify-buffer)
            (setq after-unfontify
                  (aurora-config-test-face-runs))
            (setq second-result
                  (aurora-config-test-error-data
                   (lambda ()
                     (font-lock-ensure))))
            (setq second
                  (aurora-config-test-face-runs))
            (list
             first-result
             first
             after-unfontify
             second-result
             second
             (equal first second)
             (buffer-string)
             (buffer-modified-p))))"##,
        expect![[
            r#"OK ((:error wrong-type-argument (listp font-lock-type-face)) nil nil (:error wrong-type-argument (listp font-lock-type-face)) nil t "task = Task(processes=[Process(), Service()])\n" nil)"#
        ]],
    )
}

pub(super) fn keywords_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        aurora_config_mode_keyword_families_preserve_complete_order_counts_and_uniqueness(),
        aurora_config_mode_font_lock_rules_have_exact_regexps_faces_and_matching_matrix(),
        aurora_config_mode_fontifying_every_struct_surfaces_legacy_dotted_rule_failure(),
        aurora_config_mode_context_matrix_fails_before_font_lock_applies_any_face(),
        aurora_config_mode_combined_python_and_aurora_fontification_fails_before_faces(),
        aurora_config_mode_mutated_keyword_lists_still_reach_compiled_legacy_rule_failure(),
        aurora_config_mode_unfontify_refontify_repeats_failure_without_mutating_content(),
    ]
}
