use expect_test::expect;

use super::ParityBatchCase;

/// The scan itself: `M-x ac-html-csswatcher-refresh' in an HTML buffer runs
/// csswatcher over that file and keeps the directory it names.
///
/// The argv is asserted twice, because it is the only part of the exchange the
/// package writes.  By default it is one argument, the file being edited; with
/// `ac-html-csswatcher-command-args' customized the extra arguments come first
/// and the file name stays last, which is what lets a user add `--outputdir'
/// without displacing it.  `ac-html-csswatcher-command' is customizable too, so
/// the workflow points it at a second stand-in under a different name to show
/// that the command is really taken from the variable.
///
/// Everything after the process is the parsing: the `ACSOURCE:' directory ends
/// up in `ac-html-csswatcher-source-dir', buffer-locally, while the `PROJECT:'
/// directory is what the package announces in the echo area.  The
/// `*csswatcher-output*' buffer is asserted to be gone, since the sentinel kills
/// it once it has read it.
fn refreshing_runs_csswatcher_and_keeps_the_directory_it_names() -> ParityBatchCase {
    ParityBatchCase::value(
        "refreshing_runs_csswatcher_and_keeps_the_directory_it_names",
        r##"(let* ((root (ac-html-csswatcher-test-site))
       (log (ac-html-csswatcher-test-install root ac-html-csswatcher-test-answering))
       (buffer (find-file-noselect (expand-file-name "index.html" root))))
  (ac-html-csswatcher-test-answers root (concat root ".cache/completion"))
  (with-current-buffer buffer
    (let ((mark (with-current-buffer "*Messages*" (point-max))))
      (ac-html-csswatcher-refresh)
      (ac-html-csswatcher-test-settle)
      (let ((default (list :arguments
                           (mapcar (lambda (argument)
                                     (ac-html-csswatcher-test-relative argument root))
                                   (ac-html-csswatcher-test-arguments log))
                           :source-dir
                           (ac-html-csswatcher-test-relative
                            ac-html-csswatcher-source-dir root)
                           :buffer-local
                           (local-variable-p 'ac-html-csswatcher-source-dir)
                           :message
                           (ac-html-csswatcher-test-relative
                            (with-current-buffer "*Messages*"
                              (buffer-substring-no-properties mark (point-max)))
                            root)
                           :output-buffers
                           (ac-html-csswatcher-test-output-buffers))))
        (write-region "" nil log nil 'silent)
        (let ((ac-html-csswatcher-command-args
               (list "--debug" "--outputdir" (concat root "cache"))))
          (ac-html-csswatcher-refresh)
          (ac-html-csswatcher-test-settle))
        (let ((customized (mapcar (lambda (argument)
                                    (ac-html-csswatcher-test-relative argument root))
                                  (ac-html-csswatcher-test-arguments log))))
          (write-region "" nil log nil 'silent)
          (rename-file (expand-file-name "bin/csswatcher" root)
                       (expand-file-name "bin/other-watcher" root))
          (let ((ac-html-csswatcher-command "other-watcher"))
            (ac-html-csswatcher-refresh)
            (ac-html-csswatcher-test-settle))
          (list default
                :with-command-args customized
                :with-another-command
                (mapcar (lambda (argument)
                          (ac-html-csswatcher-test-relative argument root))
                        (ac-html-csswatcher-test-arguments log))
                :source-dir-after
                (ac-html-csswatcher-test-relative ac-html-csswatcher-source-dir
                                                  root)))))))"##,
        expect![[
            r#"OK ((:arguments ("SITE/index.html") :source-dir "SITE/.cache/completion" :buffer-local t :message "[csswatcher] parsed SITE/\n" :output-buffers nil) :with-command-args ("--debug" "--outputdir" "SITE/cache" "SITE/index.html") :with-another-command ("SITE/index.html") :source-dir-after "SITE/.cache/completion")"#
        ]],
    )
}

fn enabling_it_adds_one_project_source_to_this_buffer_only() -> ParityBatchCase {
    ParityBatchCase::value(
        "enabling_it_adds_one_project_source_to_this_buffer_only",
        r##"(let* ((root (ac-html-csswatcher-test-site))
       (log (ac-html-csswatcher-test-install root ac-html-csswatcher-test-answering))
       (global-before (copy-sequence (default-value 'web-completion-data-sources)))
       (buffer (find-file-noselect (expand-file-name "index.html" root))))
  (ac-html-csswatcher-test-answers root (concat root ".cache/completion"))
  (with-current-buffer buffer
    (let ((local-before (local-variable-p 'web-completion-data-sources)))
      (ac-html-csswatcher+)
      (ac-html-csswatcher-test-settle)
      (let ((once (list :sources (copy-tree web-completion-data-sources)
                        :local (local-variable-p 'web-completion-data-sources)
                        :source-dir (ac-html-csswatcher-test-relative
                                     ac-html-csswatcher-source-dir root))))
        (company-web-csswatcher+)
        (ac-html-csswatcher-test-settle)
        (list :local-before local-before
              :after-enabling once
              :after-the-company-alias (copy-tree web-completion-data-sources)
              :aliases (list (eq (symbol-function 'company-web-csswatcher+)
                                 'ac-html-csswatcher+)
                             (eq (symbol-function 'company-web-csswatcher-refresh)
                                 'ac-html-csswatcher-refresh)
                             (eq (symbol-function 'company-web-csswatcher-setup)
                                 'ac-html-csswatcher-setup))
              :global-before global-before
              :global-after (copy-tree (default-value 'web-completion-data-sources))
              :global-untouched (equal global-before
                                       (default-value 'web-completion-data-sources)))))))"##,
        expect![[
            r#"OK (:local-before nil :after-enabling (:sources (("Project" . ac-html-csswatcher-source-dir) ("html" . web-completion-data-html-source-dir)) :local t :source-dir "SITE/.cache/completion") :after-the-company-alias (("Project" . ac-html-csswatcher-source-dir) ("html" . web-completion-data-html-source-dir)) :aliases (t t t) :global-before (("html" . web-completion-data-html-source-dir)) :global-after (("html" . web-completion-data-html-source-dir)) :global-untouched t)"#
        ]],
    )
    .fresh_process()
}

fn a_failed_or_unparsable_scan_can_silently_empty_the_completions() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_failed_or_unparsable_scan_can_silently_empty_the_completions",
        r##"(let* ((root (ac-html-csswatcher-test-site))
       (log (ac-html-csswatcher-test-install root ac-html-csswatcher-test-answering))
       (buffer (find-file-noselect (expand-file-name "index.html" root))))
  (ac-html-csswatcher-test-answers root (concat root ".cache/completion"))
  (with-current-buffer buffer
    (ac-html-csswatcher-refresh)
    (ac-html-csswatcher-test-settle)
    (let ((good (ac-html-csswatcher-test-relative ac-html-csswatcher-source-dir root)))
      (ac-html-csswatcher-test-answers root (concat root ".cache/completion") 3)
      (ac-html-csswatcher-refresh)
      (ac-html-csswatcher-test-settle)
      (let ((after-failure (ac-html-csswatcher-test-relative
                            ac-html-csswatcher-source-dir root)))
        (ac-html-csswatcher-test-install
         root "#!/bin/sh\nprintf 'csswatcher: nothing to report\\n'\nexit 0\n")
        (ac-html-csswatcher-refresh)
        (ac-html-csswatcher-test-settle)
        (let ((after-unparsable (ac-html-csswatcher-test-relative
                                 ac-html-csswatcher-source-dir root)))
          (ac-html-csswatcher-test-install
           root "#!/bin/sh\nprintf 'PROJECT: /somewhere\\n'\nexit 0\n")
          (ac-html-csswatcher-refresh)
          (ac-html-csswatcher-test-settle)
          (list :after-a-good-scan good
                :after-a-failed-scan after-failure
                :kept-the-good-one (equal good after-failure)
                :after-unparsable-output after-unparsable
                :after-project-without-acsource ac-html-csswatcher-source-dir
                :not-visiting-a-file
                (with-temp-buffer
                  (list :returned (ac-html-csswatcher-setup-html-stuff-async)
                        :processes (cl-count-if
                                    (lambda (process)
                                      (string-prefix-p "csswatcher-"
                                                       (process-name process)))
                                    (process-list))))))))))"##,
        expect![[
            r#"OK (:after-a-good-scan "SITE/.cache/completion" :after-a-failed-scan "SITE/.cache/completion" :kept-the-good-one t :after-unparsable-output nil :after-project-without-acsource nil :not-visiting-a-file (:returned nil :processes 0))"#
        ]],
    )
    .fresh_process()
}

fn setup_wires_markup_modes_to_scan_and_css_modes_to_rescan_on_save() -> ParityBatchCase {
    ParityBatchCase::value(
        "setup_wires_markup_modes_to_scan_and_css_modes_to_rescan_on_save",
        r##"(let* ((root (ac-html-csswatcher-test-site))
       (log (ac-html-csswatcher-test-install root ac-html-csswatcher-test-answering))
       (hooks '(html-mode-hook web-mode-hook slim-mode-hook jade-mode-hook
                haml-mode-hook css-mode-hook less-mode-hook))
       (before (mapcar (lambda (hook)
                         (list hook (and (boundp hook) (copy-sequence (symbol-value hook)))))
                       hooks)))
  (ac-html-csswatcher-test-answers root (concat root ".cache/completion"))
  (ac-html-csswatcher-setup)
  (let ((after (mapcar (lambda (hook)
                         (list hook
                               (and (memq 'ac-html-csswatcher+ (symbol-value hook)) t)
                               (length (symbol-value hook))))
                       hooks))
        (stylesheet (find-file-noselect (expand-file-name "css/app.css" root))))
    (with-current-buffer stylesheet
      (css-mode)
      (let ((local-hook (and (local-variable-p 'after-save-hook)
                             (memq 'ac-html-csswatcher-setup-html-stuff-async
                                   after-save-hook)))
            (global-hook (memq 'ac-html-csswatcher-setup-html-stuff-async
                               (default-value 'after-save-hook))))
        (goto-char (point-max))
        (insert ".card { color: blue; }\n")
        (save-buffer)
        (ac-html-csswatcher-test-settle)
        (list :before before
              :after after
              :css-buffer-has-a-local-after-save-hook (and local-hook t)
              :global-after-save-hook-untouched (null global-hook)
              :scanned-on-save
              (mapcar (lambda (argument)
                        (ac-html-csswatcher-test-relative argument root))
                      (ac-html-csswatcher-test-arguments log)))))))"##,
        expect![[
            r#"OK (:before ((html-mode-hook nil) (web-mode-hook nil) (slim-mode-hook nil) (jade-mode-hook nil) (haml-mode-hook nil) (css-mode-hook nil) (less-mode-hook nil)) :after ((html-mode-hook t 1) (web-mode-hook t 1) (slim-mode-hook t 1) (jade-mode-hook t 1) (haml-mode-hook t 1) (css-mode-hook nil 1) (less-mode-hook nil 1)) :css-buffer-has-a-local-after-save-hook t :global-after-save-hook-untouched t :scanned-on-save ("SITE/css/app.css"))"#
        ]],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        refreshing_runs_csswatcher_and_keeps_the_directory_it_names(),
        enabling_it_adds_one_project_source_to_this_buffer_only(),
        a_failed_or_unparsable_scan_can_silently_empty_the_completions(),
        setup_wires_markup_modes_to_scan_and_css_modes_to_rescan_on_save(),
    ]
}
