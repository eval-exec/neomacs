use expect_test::expect;

use super::ParityBatchCase;

fn interactive_multi_target_build_discovers_the_project_runs_compilation_and_repeats_last_task()
-> ParityBatchCase {
    ParityBatchCase::value(
        "interactive_multi_target_build_discovers_the_project_runs_compilation_and_repeats_last_task",
        r##"
(let* ((fixture (neomacs-ant-fixture))
       (root (plist-get fixture :root))
       (source (plist-get fixture :source))
       (ant-program
        (plist-get fixture :ant-command))
       (command-log
        (plist-get fixture :command-log))
       (default-directory
        (file-name-directory source))
       (ant-command
        (concat ant-program " -emacs"))
       (ant-build-file-name "build.xml")
       (ant-last-task "compile")
       (*ant-tasks-cache* nil)
       completion-call
       first-buffer
       second-buffer)
  (unwind-protect
      (progn
        (cl-letf
            (((symbol-function
               'completing-read-multiple)
              (lambda (prompt candidates &rest _arguments)
                (setq
                 completion-call
                 (list
                  prompt
                  (copy-sequence candidates)
                  default-directory))
                '("compile" "test"))))
          (setq first-buffer (ant)))
        (unless
            (neomacs-ant-wait-for-compilation
             first-buffer)
          (error "initial Ant compilation did not finish"))
        (let ((first-state
               (list
                ant-last-task
                completion-call
                (with-current-buffer first-buffer
                  (list
                   major-mode
                   (file-relative-name
                    default-directory root)))
                (neomacs-ant-normalized-compilation
                 first-buffer))))
          (setq second-buffer (ant-last))
          (unless
              (neomacs-ant-wait-for-compilation
               second-buffer)
            (error "repeated Ant compilation did not finish"))
          (list
           first-state
           ant-last-task
           (neomacs-ant-normalized-compilation
            second-buffer)
           (neomacs-ant-read-file command-log)
           (copy-tree *ant-tasks-cache*))))
    (dolist
        (buffer
         (delete-dups
          (list first-buffer second-buffer)))
      (when (buffer-live-p buffer)
        (kill-buffer buffer)))))
"##,
        expect![[
            r#"OK (("compile test" ("Task (default): " ("compile" "test" "package" "") "[ORACLE-SANDBOX]/storefront/") (compilation-mode "./") "-*- mode: compilation; default-directory: \"[ORACLE-SANDBOX]/storefront/\" -*-\nCompilation started at [TIME]\n\n[ORACLE-SANDBOX]/bin/ant -emacs compile test\nBuildfile: [ORACLE-SANDBOX]/storefront/build.xml\n\ncompile:\n    [javac] Compiling 12 source files\ntest:\n    [junit] Tests run: 48, Failures: 0\n\nBUILD SUCCESSFUL\n\nCompilation finished at [TIME]\n") "compile test" "-*- mode: compilation; default-directory: \"[ORACLE-SANDBOX]/storefront/\" -*-\nCompilation started at [TIME]\n\n[ORACLE-SANDBOX]/bin/ant -emacs compile test\nBuildfile: [ORACLE-SANDBOX]/storefront/build.xml\n\ncompile:\n    [javac] Compiling 12 source files\ntest:\n    [junit] Tests run: 48, Failures: 0\n\nBUILD SUCCESSFUL\n\nCompilation finished at [TIME]\n" "[ORACLE-SANDBOX]/storefront|-emacs compile test\n[ORACLE-SANDBOX]/storefront|-emacs compile test\n" (("[ORACLE-SANDBOX]/storefront/" "clean" "compile" "test" "package" "")))"#
        ]],
    )
}

fn editing_a_custom_build_file_then_killing_the_cache_exposes_and_runs_the_new_target()
-> ParityBatchCase {
    ParityBatchCase::value(
        "editing_a_custom_build_file_then_killing_the_cache_exposes_and_runs_the_new_target",
        r##"
(let* ((fixture (neomacs-ant-fixture))
       (root (plist-get fixture :root))
       (original-build
        (plist-get fixture :build-file))
       (source (plist-get fixture :source))
       (ant-program
        (plist-get fixture :ant-command))
       (command-log
        (plist-get fixture :command-log))
       (custom-build
        (expand-file-name "project.xml" root))
       (default-directory
        (file-name-directory source))
       (ant-build-file-name "project.xml")
       (ant-command
        (concat
         ant-program
         " -emacs -f project.xml"))
       (*ant-tasks-cache* nil)
       compilation-buffer)
  (unwind-protect
      (progn
        (rename-file original-build custom-build)
        (let* ((project-root
                (ant-find-root
                 ant-build-file-name))
               (first-read
                (ant-tasks project-root))
               (cached-read
                (ant-tasks project-root)))
          (with-temp-buffer
            (insert-file-contents custom-build)
            (goto-char (point-max))
            (search-backward "</project>")
            (insert
             "  <target name=\"deploy\" "
             "description=\"Deploy staging\"/>\n")
            (write-region
             (point-min) (point-max)
             custom-build nil 'silent))
          (let ((still-cached
                 (ant-tasks project-root)))
            (ant-kill-cache)
            (let ((after-cache-reset
                   (ant-tasks project-root)))
              (setq compilation-buffer
                    (ant "deploy"))
              (unless
                  (neomacs-ant-wait-for-compilation
                   compilation-buffer)
                (error
                 "custom Ant compilation did not finish"))
              (list
               (file-relative-name
                project-root root)
               first-read
               cached-read
               still-cached
               after-cache-reset
               ant-last-task
               (neomacs-ant-normalized-compilation
                compilation-buffer)
               (neomacs-ant-read-file
                command-log)
               (neomacs-ant-read-file
                custom-build))))))
    (when (buffer-live-p compilation-buffer)
      (kill-buffer compilation-buffer))))
"##,
        expect![[
            r#"OK ("./" #1=("compile" "test" "package" "") #2=("clean" . #1#) #2# ("compile" "test" "package" "deploy" "") "deploy" "-*- mode: compilation; default-directory: \"[ORACLE-SANDBOX]/storefront/\" -*-\nCompilation started at [TIME]\n\n[ORACLE-SANDBOX]/bin/ant -emacs -f project.xml deploy\nBuildfile: [ORACLE-SANDBOX]/storefront/project.xml\n\ndeploy:\n     [copy] storefront.jar -> staging\n\nBUILD SUCCESSFUL\n\nCompilation finished at [TIME]\n" "[ORACLE-SANDBOX]/storefront|-emacs -f project.xml deploy\n" "<project name=\"storefront\" default=\"test\">\n  <target name=\"clean\" description=\"Remove build output\"/>\n  <target name=\"compile\" description=\"Compile Java sources\"/>\n  <target name=\"test\" description=\"Run unit tests\"/>\n  <target name=\"package\" description=\"Create release archive\"/>\n  <target name=\"deploy\" description=\"Deploy staging\"/>\n</project>\n")"#
        ]],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        interactive_multi_target_build_discovers_the_project_runs_compilation_and_repeats_last_task(
        ),
        editing_a_custom_build_file_then_killing_the_cache_exposes_and_runs_the_new_target(),
    ]
}
