use expect_test::expect;

use super::ParityBatchCase;

fn searches_real_project_and_opens_the_precise_source_entry() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((project (xcscope-test-project "xcscope-search-project"))
       (source (expand-file-name "src/main.c" project))
       (cscope-program (expand-file-name "cscope" xcscope-test-bin))
       (cscope-display-times nil)
       (cscope-display-cscope-buffer nil)
       (cscope-edit-single-match nil)
       (cscope-option-do-not-update-database t)
       (cscope-use-face t)
       (cscope-marker-ring (make-ring cscope-marker-ring-length))
       (cscope-marker nil)
       (cscope-marker-window nil)
       (cscope-previous-user-search nil)
       (source-buffer (find-file-noselect source))
       selected result)
  (xcscope-test-reset)
  (unwind-protect
      (progn
        (with-current-buffer source-buffer
          (goto-char (point-max))
          (cscope-find-this-symbol "release_count"))
        (xcscope-test-await-search)
        (save-window-excursion
          (switch-to-buffer cscope-output-buffer-name)
          (goto-char cscope-first-match-point)
          (cscope-select-entry-inplace)
          (setq selected
                (list (file-relative-name (buffer-file-name) project)
                      (line-number-at-pos)
                      (buffer-substring-no-properties
                       (line-beginning-position) (line-end-position))
                      (ring-length cscope-marker-ring))))
        (setq result
              (list
               :mode (with-current-buffer cscope-output-buffer-name major-mode)
               :search cscope-previous-user-search
               :buffer
               (with-current-buffer cscope-output-buffer-name
                 (xcscope-test-normalize
                  (buffer-substring-no-properties (point-min) (point-max))
                  project))
               :entries (xcscope-test-result-entries project)
               :selected selected
               :complete
               (with-current-buffer cscope-output-buffer-name
                 (string-suffix-p "Search complete.\n" (buffer-string)))
               :calls (xcscope-test-calls project)
               :misses (xcscope-test-misses))))
    (xcscope-test-cleanup project))
  result)
"####;
    let expect = expect![[
        r#"OK (:mode cscope-list-entry-mode :search (cscope-find-this-symbol "release_count") :buffer "===============================================================================\nFinding symbol: release_count\nDatabase directory: [PROJECT]/\n\n*** src/release.c:\npublish_release[7]             release_count += 1;\n\n*** src/main.c:\nvalidate_release[4]            if (release_count > 0) publish_release();\n\nSearch complete.\n" :entries (("src/release.c" 7 "release\\s-*_\\s-*count\\s-*\\+=\\s-*1\\s-*;" cscope-function-face "publish_release[7]             release_count += 1;") ("src/main.c" 4 "if\\s-*(\\s-*release\\s-*_\\s-*count\\s-*>\\s-*0\\s-*)\\s-*publish\\s-*_\\s-*release\\s-*();" cscope-function-face "validate_release[4]            if (release_count > 0) publish_release();")) :selected ("src/release.c" 9 "  release_count += 1;" 1) :complete t :calls ("cwd=[PROJECT]|<-f><cscope.out><-d><-L><-0><release_count>") :misses nil)"#
    ]];
    ParityBatchCase::value(
        "searches_real_project_and_opens_the_precise_source_entry",
        elisp_form,
        expect,
    )
}

fn reruns_search_history_and_removes_an_obsolete_result() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((project (xcscope-test-project "xcscope-history-project"))
       (source (expand-file-name "src/main.c" project))
       (cscope-program (expand-file-name "cscope" xcscope-test-bin))
       (cscope-display-times nil)
       (cscope-display-cscope-buffer nil)
       (cscope-edit-single-match nil)
       (cscope-option-do-not-update-database t)
       (cscope-use-face t)
       (cscope-marker-ring (make-ring cscope-marker-ring-length))
       (cscope-marker nil)
       (cscope-marker-window nil)
       (cscope-previous-user-search nil)
       (source-buffer (find-file-noselect source))
       before-rerun after-rerun remaining result)
  (xcscope-test-reset)
  (unwind-protect
      (progn
        (with-current-buffer source-buffer
          (cscope-find-this-symbol "release_count"))
        (xcscope-test-await-search)
        (with-current-buffer source-buffer
          (cscope-find-global-definition "publish_release"))
        (xcscope-test-await-search)
        (with-current-buffer cscope-output-buffer-name
          (setq before-rerun
                (list
                 (count-matches "^===" (point-min) (point-max))
                 (xcscope-test-result-entries project)
                 (xcscope-test-normalize
                  (buffer-substring-no-properties (point-min) (point-max))
                  project)))
          (goto-char (point-max))
          (cscope-rerun-search-at-point))
        (xcscope-test-await-search)
        (with-current-buffer cscope-output-buffer-name
          (let ((after-buffer
                 (xcscope-test-normalize
                  (buffer-substring-no-properties (point-min) (point-max))
                  project)))
            (setq after-rerun
                  (list
                   (count-matches "^===" (point-min) (point-max))
                   (xcscope-test-result-entries project)
                   (equal after-buffer (nth 2 before-rerun)))))
          (goto-char (point-min))
          (cscope-history-kill-result)
          (setq remaining
                (list
                 (count-matches "^===" (point-min) (point-max))
                 (xcscope-test-result-entries project)
                 (xcscope-test-normalize
                  (buffer-substring-no-properties (point-min) (point-max))
                  project))))
        (setq result
              (list
               :before-rerun before-rerun
               :after-rerun after-rerun
               :remaining remaining
               :calls (xcscope-test-calls project)
               :misses (xcscope-test-misses))))
    (xcscope-test-cleanup project))
  result)
"####;
    let expect = expect![[
        r#"OK (:before-rerun (2 (("src/release.c" 7 "release\\s-*_\\s-*count\\s-*\\+=\\s-*1\\s-*;" cscope-function-face "publish_release[7]             release_count += 1;") ("src/main.c" 4 "if\\s-*(\\s-*release\\s-*_\\s-*count\\s-*>\\s-*0\\s-*)\\s-*publish\\s-*_\\s-*release\\s-*();" cscope-function-face "validate_release[4]            if (release_count > 0) publish_release();") ("src/release.c" 8 "void\\s-*publish\\s-*_\\s-*release\\s-*(\\s-*void\\s-*)\\s-*{" cscope-function-face "publish_release[8]             void publish_release(void) {")) "===============================================================================\nFinding symbol: release_count\nDatabase directory: [PROJECT]/\n\n*** src/release.c:\npublish_release[7]             release_count += 1;\n\n*** src/main.c:\nvalidate_release[4]            if (release_count > 0) publish_release();\n\nSearch complete.\n===============================================================================\nFinding global definition: publish_release\nDatabase directory: [PROJECT]/\n\n*** src/release.c:\npublish_release[8]             void publish_release(void) {\n\nSearch complete.\n") :after-rerun (2 (("src/release.c" 7 "release\\s-*_\\s-*count\\s-*\\+=\\s-*1\\s-*;" cscope-function-face "publish_release[7]             release_count += 1;") ("src/main.c" 4 "if\\s-*(\\s-*release\\s-*_\\s-*count\\s-*>\\s-*0\\s-*)\\s-*publish\\s-*_\\s-*release\\s-*();" cscope-function-face "validate_release[4]            if (release_count > 0) publish_release();") ("src/release.c" 8 "void\\s-*publish\\s-*_\\s-*release\\s-*(\\s-*void\\s-*)\\s-*{" cscope-function-face "publish_release[8]             void publish_release(void) {")) t) :remaining (1 (("src/release.c" 8 "void\\s-*publish\\s-*_\\s-*release\\s-*(\\s-*void\\s-*)\\s-*{" cscope-function-face "publish_release[8]             void publish_release(void) {")) "===============================================================================\nFinding global definition: publish_release\nDatabase directory: [PROJECT]/\n\n*** src/release.c:\npublish_release[8]             void publish_release(void) {\n\nSearch complete.\n") :calls ("cwd=[PROJECT]|<-f><cscope.out><-d><-L><-0><release_count>" "cwd=[PROJECT]|<-f><cscope.out><-d><-L><-1><publish_release>" "cwd=[PROJECT]|<-f><cscope.out><-d><-L><-1><publish_release>") :misses nil)"#
    ]];
    ParityBatchCase::value(
        "reruns_search_history_and_removes_an_obsolete_result",
        elisp_form,
        expect,
    )
}

fn renders_empty_and_failed_backend_searches_without_corrupting_history() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((project (xcscope-test-project "xcscope-failure-project"))
       (source (expand-file-name "src/main.c" project))
       (cscope-program (expand-file-name "cscope" xcscope-test-bin))
       (cscope-display-times nil)
       (cscope-display-cscope-buffer nil)
       (cscope-edit-single-match nil)
       (cscope-option-do-not-update-database t)
       (cscope-use-face t)
       (cscope-marker-ring (make-ring cscope-marker-ring-length))
       (cscope-marker nil)
       (cscope-marker-window nil)
       (cscope-previous-user-search nil)
       (source-buffer (find-file-noselect source))
       no-match backend-failure remaining result)
  (xcscope-test-reset)
  (unwind-protect
      (progn
        (with-current-buffer source-buffer
          (cscope-find-global-definition "publish_release"))
        (xcscope-test-await-search)
        (with-current-buffer source-buffer
          (cscope-find-this-text-string "absent_release_marker"))
        (xcscope-test-await-search)
        (with-current-buffer cscope-output-buffer-name
          (goto-char (point-max))
          (let ((bounds (cscope-get-history-bounds-this-result 'result)))
            (setq no-match
                  (list
                   (count-matches "^===" (point-min) (point-max))
                   (xcscope-test-normalize
                    (buffer-substring-no-properties
                     (car bounds) (cadr bounds))
                    project))))
          (cscope-history-kill-result))
        (with-current-buffer source-buffer
          (cscope-find-this-text-string "backend_unavailable"))
        (xcscope-test-await-search)
        (with-current-buffer cscope-output-buffer-name
          (goto-char (point-max))
          (let ((bounds (cscope-get-history-bounds-this-result 'result)))
            (setq backend-failure
                  (list
                   (count-matches "^===" (point-min) (point-max))
                   (xcscope-test-normalize
                    (buffer-substring-no-properties
                     (car bounds) (cadr bounds))
                    project))))
          (cscope-history-kill-result)
          (setq remaining
                (list
                 (count-matches "^===" (point-min) (point-max))
                 (xcscope-test-result-entries project)
                 (xcscope-test-normalize
                  (buffer-substring-no-properties (point-min) (point-max))
                  project))))
        (setq result
              (list
               :no-match no-match
               :backend-failure backend-failure
               :remaining remaining
               :calls (xcscope-test-calls project)
               :misses (xcscope-test-misses))))
    (xcscope-test-cleanup project))
  result)
"####;
    let expect = expect![[
        r#"OK (:no-match (2 "===============================================================================\nFinding text string: absent_release_marker\nDatabase directory: [PROJECT]/\n\n --- No matches were found ---\n\nSearch complete.\n") :backend-failure (2 "===============================================================================\nFinding text string: backend_unavailable\nDatabase directory: [PROJECT]/\n\ncscope: cannot read database\n\n\nSearch complete.\n") :remaining (1 (("src/release.c" 8 "void\\s-*publish\\s-*_\\s-*release\\s-*(\\s-*void\\s-*)\\s-*{" cscope-function-face "publish_release[8]             void publish_release(void) {")) "===============================================================================\nFinding global definition: publish_release\nDatabase directory: [PROJECT]/\n\n*** src/release.c:\npublish_release[8]             void publish_release(void) {\n\nSearch complete.\n") :calls ("cwd=[PROJECT]|<-f><cscope.out><-d><-L><-1><publish_release>" "cwd=[PROJECT]|<-f><cscope.out><-d><-L><-4><absent_release_marker>" "cwd=[PROJECT]|<-f><cscope.out><-d><-L><-4><backend_unavailable>") :misses nil)"#
    ]];
    ParityBatchCase::value(
        "renders_empty_and_failed_backend_searches_without_corrupting_history",
        elisp_form,
        expect,
    )
}

fn recursively_indexes_real_sources_and_discovers_the_database() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((project (xcscope-test-project "xcscope-index-project"))
       (cscope-program (expand-file-name "cscope" xcscope-test-bin))
       (cscope-index-recursively t)
       (cscope-use-relative-paths t)
       (cscope-indexer-suffixes '("*.c" "*.h"))
       (cscope-indexer-ignored-directories '(".git" "generated"))
       (cscope-option-include-directories '("include"))
       (cscope-option-disable-compression t)
       (cscope-option-kernel-mode t)
       (cscope-option-use-inverted-index t)
       (cscope-unix-index-process nil)
       (cscope-indexing-status-string nil)
       indexed-files directory-message indexing-output result)
  (xcscope-test-reset)
  (unwind-protect
      (progn
        (xcscope-test-write
         (expand-file-name ".git/ignored.c" project)
         "int ignored_by_vcs = 1;\n")
        (xcscope-test-write
         (expand-file-name "generated/ignored.h" project)
         "extern int ignored_generated;\n")
        (xcscope-test-write
         (expand-file-name "docs/release.md" project)
         "# Not a source file\n")
        (cscope-index-files project)
        (xcscope-test-await-index)
        (setq indexed-files
              (sort (split-string
                     (xcscope-test-read
                      (expand-file-name "cscope.files" project))
                     "\n" t)
                    #'string<))
        (setq indexing-output
              (with-current-buffer "*cscope-indexing-buffer*"
                (xcscope-test-normalize
                 (buffer-substring-no-properties (point-min) (point-max))
                 project)))
        (let ((default-directory (expand-file-name "src/" project)))
          (setq directory-message
                (xcscope-test-normalize
                 (cscope-tell-user-about-directory) project)))
        (setq result
              (list
               :files indexed-files
               :database
               (xcscope-test-read (expand-file-name "cscope.out" project))
               :directory-message directory-message
               :indexing-output indexing-output
               :calls (xcscope-test-calls project)
               :misses (xcscope-test-misses))))
    (xcscope-test-cleanup project))
  result)
"####;
    let expect = expect![[
        r#"OK (:files ("\"./include/release.h\"" "\"./src/main.c\"" "\"./src/release.c\"") :database "fixture database\n" :directory-message "Cscope directory: [PROJECT]/" :indexing-output "Creating cscope index `cscope.out' in:\n\11[PROJECT]/\n\n===============================================================================\nCreating list of files to index ...\nCreating list of files to index ... done\nIndexing files ...\nIndexed 3 source files.\nIndexing files ... done\n===============================================================================\n\nIndexing finished\n" :calls ("cwd=[PROJECT]|<-Iinclude><-c><-k><-q><-b><-i><cscope.files><-f><cscope.out>") :misses nil)"#
    ]];
    ParityBatchCase::value(
        "recursively_indexes_real_sources_and_discovers_the_database",
        elisp_form,
        expect,
    )
}

pub(super) fn practical_workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        searches_real_project_and_opens_the_precise_source_entry(),
        reruns_search_history_and_removes_an_obsolete_result(),
        renders_empty_and_failed_backend_searches_without_corrupting_history(),
        recursively_indexes_real_sources_and_discovers_the_database(),
    ]
}
