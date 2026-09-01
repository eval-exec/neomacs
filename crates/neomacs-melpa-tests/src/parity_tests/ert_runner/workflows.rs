use expect_test::expect;

use super::ParityBatchCase;

fn reporter_path_lists_bundled_reporters() -> ParityBatchCase {
    ParityBatchCase::value(
        "reporter_path_lists_bundled_reporters",
        r####"
(let* ((files (f-files ert-runner-reporters-path
                       (lambda (file) (equal (f-ext file) "el"))))
       (names (sort (mapcar (lambda (f)
                              (s-chop-prefix "ert-runner-reporter-"
                                            (f-no-ext (f-filename f))))
                            files)
                    #'string-lessp)))
  (list :dir (and (f-dir? ert-runner-reporters-path) t)
        :names names
        :default-reporter ert-runner-reporter-name
        :selector ert-runner-selector))
"####,
        expect![[
            r#"OK (:dir t :names ("dot" "ert" "ert+duration") :default-reporter "dot" :selector (and t))"#
        ]],
    )
}

fn tag_and_pattern_selectors_compose_on_runner_state() -> ParityBatchCase {
    ParityBatchCase::value(
        "tag_and_pattern_selectors_compose_on_runner_state",
        r####"
(let ((ert-runner-selector '(and t)))
  (ert-runner/pattern "foo-.*")
  (let ((after-pattern (copy-tree ert-runner-selector)))
    (setq ert-runner-selector '(and t))
    (ert-runner/tags "unit,!slow")
    (let ((after-tags (copy-tree ert-runner-selector))
          (tag-unit (ert-runner/make-tag-selector "unit"))
          (tag-not-slow (ert-runner/make-tag-selector "!slow")))
      (list :after-pattern after-pattern
            :after-tags after-tags
            :tag-unit tag-unit
            :tag-not-slow tag-not-slow))))
"####,
        expect![[
            r#"OK (:after-pattern (and t "foo-.*") :after-tags (and t (or (tag unit) (not (tag slow)))) :tag-unit (tag unit) :tag-not-slow (not (tag slow)))"#
        ]],
    )
}

fn expand_test_path_finds_files_and_rejects_missing_paths() -> ParityBatchCase {
    ParityBatchCase::value(
        "expand_test_path_finds_files_and_rejects_missing_paths",
        r####"
(neomacs-ert-runner-test-with-temp-root
 (lambda (root)
   (let* ((test-dir (f-join root "test"))
          (nested (f-join test-dir "nested"))
          (file-a (f-join test-dir "alpha-test.el"))
          (file-b (f-join nested "beta-test.el"))
          (helper (f-join test-dir "test-helper.el"))
          (missing (f-join root "nope.el")))
     (f-mkdir test-dir)
     (f-mkdir nested)
     (f-write-text ";; a\n" 'utf-8 file-a)
     (f-write-text ";; b\n" 'utf-8 file-b)
     (f-write-text ";; helper\n" 'utf-8 helper)
     (let* ((from-dir (ert-runner--expand-test-path test-dir))
            (from-file (ert-runner--expand-test-path file-a))
            (listed
             (let ((ert-runner-test-path test-dir))
               (ert-runner--test-files nil)))
            (missing-err
             (condition-case err
                 (progn (ert-runner--expand-test-path missing) nil)
               (error (error-message-string err)))))
       (list :from-dir (sort (mapcar #'f-filename from-dir) #'string-lessp)
             :from-file (f-filename from-file)
             :listed (sort (mapcar #'f-filename listed) #'string-lessp)
             :missing-has-nope (and (string-match-p "nope\\.el" missing-err) t)
             :missing-has-ansi
             (and (string-match-p "\033\\[[0-9]+m" missing-err) t)
             :helper-excluded
             (not (member "test-helper.el"
                          (mapcar #'f-filename from-dir))))))))
"####,
        expect![[
            r#"OK (:from-dir ("alpha-test.el" "beta-test.el") :from-file "alpha-test.el" :listed ("alpha-test.el" "beta-test.el") :missing-has-nope t :missing-has-ansi t :helper-excluded t)"#
        ]],
    )
}

fn init_scaffolds_test_project_layout() -> ParityBatchCase {
    ParityBatchCase::value(
        "init_scaffolds_test_project_layout",
        r####"
(neomacs-ert-runner-test-with-temp-root
 (lambda (root)
   (let ((default-directory root)
         (ert-runner-test-path (f-expand "test" root)))
     (ert-runner/init "demo")
     (let ((helper (f-join root "test" "test-helper.el"))
           (suite (f-join root "test" "demo-test.el")))
       (list :test-dir (f-dir? (f-join root "test"))
             :helper (and (f-file? helper) t)
             :suite (and (f-file? suite) t)
             :helper-text (f-read-text helper 'utf-8)
             :suite-text (f-read-text suite 'utf-8)
             :second-init
             (condition-case err
                 (progn (ert-runner/init "demo") :ok)
               (error (error-message-string err))))))))
"####,
        expect![[
            r#"OK (:test-dir t :helper t :suite t :helper-text ";;; test-helper.el --- Helpers for demo-test.el\n\n;;; test-helper.el ends here\n" :suite-text ";;; demo-test.el --- Tests for demo\n\n;;; demo-test.el ends here\n" :second-init "\33[31mDirectory `test` already exists.\33[0m")"#
        ]],
    )
}

fn use_reporter_loads_bundled_dot_reporter() -> ParityBatchCase {
    ParityBatchCase::value(
        "use_reporter_loads_bundled_dot_reporter",
        r####"
(let ((ert-runner-reporter-run-started-functions nil)
      (ert-runner-reporter-run-ended-functions nil)
      (ert-runner-reporter-test-ended-functions nil)
      (ert-runner-reporter-name "dot"))
  (ert-runner/use-reporter "dot")
  (list :feature (featurep 'ert-runner-reporter-dot)
        :started-count (length ert-runner-reporter-run-started-functions)
        :ended-count (length ert-runner-reporter-run-ended-functions)
        :test-ended-count (length ert-runner-reporter-test-ended-functions)
        :invalid
        (condition-case err
            (progn (ert-runner/use-reporter "no-such-reporter") :ok)
          (error (error-message-string err)))))
"####,
        expect![[
            r#"OK (:feature t :started-count 1 :ended-count 1 :test-ended-count 1 :invalid "\33[31mInvalid reporter no-such-reporter, list available with --reporters\33[0m")"#
        ]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        reporter_path_lists_bundled_reporters(),
        tag_and_pattern_selectors_compose_on_runner_state(),
        expand_test_path_finds_files_and_rejects_missing_paths(),
        init_scaffolds_test_project_layout(),
        use_reporter_loads_bundled_dot_reporter(),
    ]
}
