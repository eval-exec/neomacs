use expect_test::expect;

use super::ParityBatchCase;

fn opening_each_file_of_a_real_alan_project_selects_its_mode_and_configures_its_compiler()
-> ParityBatchCase {
    ParityBatchCase::value(
        "opening_each_file_of_a_real_alan_project_selects_its_mode_and_configures_its_compiler",
        r##"(let* ((root (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
       (project (file-name-as-directory (expand-file-name "customer" root)))
       (tools (expand-file-name "dependencies/dev/internals/alan/tools/" project))
       ;; The sandbox is inside the neomacs worktree, whose .dir-locals.el
       ;; imposes tab-width 8 on every mode after the major mode has run.
       ;; A customer's own Alan checkout has no such directory above it.
       (enable-dir-local-variables nil)
       (files '("schema.alan" "grammar.alan" "wiring.alan" "application.alan"
                "interface.alan" "settings.alan" "control.alan" "mapping.alan"
                "migration.alan" "deployment.alan" "phrases.alan"
                "views/main.alan" "widgets/list.alan" "templates/page.alan"
                "translations/nl.alan" "models/plain.alan")))
  (alan-test-write (expand-file-name "build.alan" project) "")
  (dolist (tool '("compiler-project" "pretty-printer"))
    (alan-test-write (expand-file-name tool tools) "#!/bin/sh\nexit 0\n")
    (set-file-modes (expand-file-name tool tools) #o755))
  (list
   ;; The package sets these for the compiler at load time, in the user's
   ;; own environment.
   :compiler-environment
   (list (alan-test-copy (getenv "ALAN_COMPILER_FORMAT"))
         (alan-test-copy (getenv "ALAN_COMPILER_LOG")))
   ;; Tree-sitter is compiled in but no Alan grammar is installed, so every
   ;; mode below takes the regexp font-lock path.  Pinning the gate keeps
   ;; the snapshot honest about which path it measured.
   :tree-sitter-gate
   (list (treesit-available-p)
         (ignore-errors (treesit-ready-p 'alan_generic t)))
   :opened
   (mapcar
    (lambda (relative)
      (let ((file (expand-file-name relative project))
            buffer)
        (alan-test-write file "'root' -> component { }\n")
        (setq buffer (find-file-noselect file))
        (unwind-protect
            (with-current-buffer buffer
              (list relative
                    major-mode
                    (alan-test-copy mode-name)
                    (alan-test-copy alan-language-definition)
                    (alan-test-copy alan-compiler-project-root)
                    alan-pretty-print
                    (alan-test-relative flycheck-alan-executable project)
                    (alan-test-relative alan--flycheck-language-definition project)))
          (kill-buffer buffer))))
    files)))"##,
        expect![[
            r#"OK (:compiler-environment ("emacs" "warning") :tree-sitter-gate (t nil) :opened (("schema.alan" alan-schema-mode "schema" "dependencies/dev/internals/alan/language" "../.." t "dependencies/dev/internals/alan/tools/compiler-project" "dependencies/dev/internals/alan/language") ("grammar.alan" alan-grammar-mode "grammar" "dependencies/dev/internals/alan/language" "../.." t "dependencies/dev/internals/alan/tools/compiler-project" "dependencies/dev/internals/alan/language") ("wiring.alan" alan-wiring-mode "wiring" nil "." nil nil nil) ("application.alan" alan-application-mode "application" ".alan/devenv/platform/if-types/model/language" "." t "dependencies/dev/internals/alan/tools/compiler-project" ".alan/devenv/platform/if-types/model/language") ("interface.alan" alan-interface-mode "interface" nil "." t nil nil) ("settings.alan" alan-settings-mode "settings" ".alan/devenv/system-types/auto-webclient/language" "." t "dependencies/dev/internals/alan/tools/compiler-project" ".alan/devenv/system-types/auto-webclient/language") ("control.alan" alan-control-mode "control" nil "." t nil nil) ("mapping.alan" alan-mapping-mode "mapping" nil "." nil nil nil) ("migration.alan" alan-migration-mode "migration" nil "." t nil nil) ("deployment.alan" alan-deployment-mode "deployment" ".alan/devenv/platform/project-build-environment/language" "." nil "dependencies/dev/internals/alan/tools/compiler-project" ".alan/devenv/platform/project-build-environment/language") ("phrases.alan" alan-phrases-mode "phrases" nil "." nil nil nil) ("views/main.alan" alan-views-mode "views" ".alan/devenv/system-types/webclient/language" "../" t "dependencies/dev/internals/alan/tools/compiler-project" ".alan/devenv/system-types/webclient/language") ("widgets/list.alan" alan-widget-mode "widget" ".alan/devenv/system-types/webclient/language" "../" t "dependencies/dev/internals/alan/tools/compiler-project" ".alan/devenv/system-types/webclient/language") ("templates/page.alan" alan-template-mode "template" "dependencies/dev/internals/alan-to-text-transformation/language" "../../../" t "dependencies/dev/internals/alan/tools/compiler-project" "dependencies/dev/internals/alan-to-text-transformation/language") ("translations/nl.alan" alan-translations-mode "translations" nil "." nil nil nil) ("models/plain.alan" alan-mode "Alan" nil "." nil nil nil)))"#
        ]],
    )
}

fn flycheck_runs_the_project_compiler_and_reports_only_this_files_diagnostics() -> ParityBatchCase {
    ParityBatchCase::value(
        "flycheck_runs_the_project_compiler_and_reports_only_this_files_diagnostics",
        r##"(let* ((root (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
       (project (file-name-as-directory (expand-file-name "customer" root)))
       (source (expand-file-name "models/accounts/schema.alan" project))
       (other (expand-file-name "models/accounts/other.alan" project))
       (compiler (expand-file-name
                  "dependencies/dev/internals/alan/tools/compiler-project" project))
       (standin (file-name-as-directory (expand-file-name "standin" root)))
       (log (expand-file-name "invocations" standin))
       (enable-dir-local-variables nil)
       buffer)
  (alan-test-write (expand-file-name "build.alan" project) "")
  (alan-test-write source "'root' -> component { }\n")
  (alan-test-write other "'other' -> component { }\n")
  (alan-test-write-standin compiler)
  (alan-test-write log "")
  ;; The diagnostic text below is derived from the package's OWN
  ;; `:error-patterns' grammar; the Alan compiler is a commercial tool and
  ;; is not obtainable here, so this is a format contract rather than a
  ;; recording.  What it is built to witness is therefore the package's
  ;; own decisions: the optional "line:column" span the pattern tolerates,
  ;; a continuation message, a warning level, and the exclusion
  ;; `alan-flycheck-error-filter' performs.  Four parseable diagnostics go
  ;; in and two must come out -- a filter that stopped excluding would give
  ;; four, and the unparseable trailer must give none.
  ;;
  ;; Only ONE exclusion is observable, though the filter names two: a
  ;; "/dev/null" diagnostic is already not the current buffer's file, so
  ;; the filter's explicit `/dev/null' clause cannot fire independently and
  ;; deleting it changes nothing.  The /dev/null line is kept in the fixture
  ;; because the compiler really emits one, but no test can distinguish that
  ;; clause and this comment records why rather than implying otherwise.
  (alan-test-write
   (expand-file-name "reply-language" standin)
   (concat source ":3:5: error: 12:34 unresolved reference 'balance'\n"
           " candidates are 'balances'\n"
           " in collection 'accounts'\n"
           source ":7:1: warning: node 'status' is never read\n"
           other ":2:2: error: a sibling file in the same project\n"
           "/dev/null:1:1: error: compiler context\n"
           "alan project compiler: 4 diagnostics\n"))
  (alan-test-write (expand-file-name "reply-build" standin)
                   (concat source ":1:1: error: the build branch ran\n"))
  (setenv "ALAN_STANDIN_LOG" log)
  (setenv "ALAN_STANDIN_DIR" (directory-file-name standin))
  (setq buffer (find-file-noselect source))
  (unwind-protect
      (with-current-buffer buffer
        (let ((finished (alan-test-check-buffer)))
          (list :mode major-mode
                :finished finished
                :status flycheck-last-status-change
                :executable (alan-test-relative flycheck-alan-executable project)
                :language (alan-test-relative alan--flycheck-language-definition project)
                :project-root (alan-test-copy alan-compiler-project-root)
                ;; What the package actually sent, recorded by the stand-in.
                :invocations (alan-test-invocations log project)
                :emitted 4
                :shown (length flycheck-current-errors)
                :diagnostics (alan-test-diagnostics project))))
    (when (buffer-live-p buffer)
      (with-current-buffer buffer (set-buffer-modified-p nil))
      (kill-buffer buffer))))"##,
        expect![[
            r#"OK (:mode alan-schema-mode :finished t :status finished :executable "dependencies/dev/internals/alan/tools/compiler-project" :language "dependencies/dev/internals/alan/language" :project-root "../.." :invocations "cwd=[PROJECT]models/accounts\nargv: [[PROJECT]dependencies/dev/internals/alan/language] [-C] [../..] [/dev/null]\n" :emitted 4 :shown 2 :diagnostics ((3 5 error "models/accounts/schema.alan" " unresolved reference 'balance'\n candidates are 'balances'\n in collection 'accounts'") (7 1 warning "models/accounts/schema.alan" " node 'status' is never read")))"#
        ]],
    )
}

fn without_a_project_compiler_flycheck_falls_back_to_the_projects_build_script() -> ParityBatchCase
{
    ParityBatchCase::value(
        "without_a_project_compiler_flycheck_falls_back_to_the_projects_build_script",
        r##"(let* ((root (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
       (project (file-name-as-directory (expand-file-name "scripted" root)))
       (source (expand-file-name "models/accounts/schema.alan" project))
       (script (expand-file-name "alan" project))
       (standin (file-name-as-directory (expand-file-name "standin" root)))
       (log (expand-file-name "invocations" standin))
       (enable-dir-local-variables nil)
       buffer)
  (alan-test-write (expand-file-name "build.alan" project) "")
  (alan-test-write source "'root' -> component { }\n")
  ;; No dependencies/dev/internals/alan/tools/compiler-project in this
  ;; project, so `alan-setup-build-system' must fall through to the
  ;; project's own `alan' script and the checker must take its other
  ;; `:command' branch.  The stand-in replies differently per branch, so a
  ;; suite that pinned one canned reply could not tell these two apart.
  (alan-test-write-standin script)
  (alan-test-write log "")
  (alan-test-write (expand-file-name "reply-build" standin)
                   (concat source ":2:9: error: the build branch ran\n"))
  (alan-test-write (expand-file-name "reply-language" standin)
                   (concat source ":9:9: error: the language branch ran\n"))
  (setenv "ALAN_STANDIN_LOG" log)
  (setenv "ALAN_STANDIN_DIR" (directory-file-name standin))
  (setq buffer (find-file-noselect source))
  (unwind-protect
      (with-current-buffer buffer
        (let ((finished (alan-test-check-buffer)))
          (list :mode major-mode
                :finished finished
                :status flycheck-last-status-change
                :executable (alan-test-relative flycheck-alan-executable project)
                :language alan--flycheck-language-definition
                :compile-command
                (replace-regexp-in-string (regexp-quote project) "[PROJECT]"
                                          compile-command t t)
                :invocations (alan-test-invocations log project)
                :diagnostics (alan-test-diagnostics project))))
    (when (buffer-live-p buffer)
      (with-current-buffer buffer (set-buffer-modified-p nil))
      (kill-buffer buffer))))"##,
        expect![[
            r#"OK (:mode alan-schema-mode :finished t :status finished :executable "alan" :language nil :compile-command "[PROJECT]alan build" :invocations "cwd=[PROJECT]models/accounts\nargv: [build]\n" :diagnostics ((2 9 error "models/accounts/schema.alan" " the build branch ran")))"#
        ]],
    )
}

fn the_documented_electric_layout_rule_opens_a_line_inside_an_empty_alan_block() -> ParityBatchCase
{
    ParityBatchCase::value(
        "the_documented_electric_layout_rule_opens_a_line_inside_an_empty_alan_block",
        r##"(let* ((root (getenv "NEOMACS_TEST_SANDBOX_ROOT"))
       (project (file-name-as-directory (expand-file-name "layout" root)))
       (source (expand-file-name "models/accounts/schema.alan" project))
       (enable-dir-local-variables nil))
  (alan-test-write (expand-file-name "build.alan" project) "")
  (alan-test-write source "")
  (cl-flet
      ((session (install-rule)
         ;; The README's own illustration: an empty Alan block, point
         ;; between the braces, and one RET.
         (let ((buffer (find-file-noselect source)))
           (unwind-protect
               (progn
                 (set-window-buffer (selected-window) buffer)
                 (set-buffer buffer)
                 (erase-buffer)
                 (setq tab-width 2)
                 (electric-indent-mode 1)
                 (electric-layout-mode 1)
                 (when install-rule
                   (set (make-local-variable 'electric-layout-rules)
                        (list alan-add-line-in-braces-rule)))
                 (insert "'root' -> component { }")
                 (goto-char (point-min))
                 (search-forward "{ ")
                 (execute-kbd-macro (kbd "RET"))
                 (list (alan-test-copy
                        (buffer-substring-no-properties (point-min) (point-max)))
                       (point)
                       (line-number-at-pos)
                       (current-column)
                       (alan-test-copy
                        (buffer-substring-no-properties
                         (line-beginning-position) (line-end-position)))))
             (when (buffer-live-p buffer)
               (with-current-buffer buffer (set-buffer-modified-p nil))
               (kill-buffer buffer))))))
    (let ((opened (find-file-noselect source)))
      (prog1
          (list
           ;; `alan-schema-mode' is one of the modes whose `:pairs' makes the
           ;; braces real parens; in the generic `alan-mode' they are symbol
           ;; constituents and neither the rule nor the indenter sees them.
           :syntax (with-current-buffer opened
                     (list major-mode (char-syntax ?{) (char-syntax ?})))
           :generic-syntax (with-temp-buffer
                             (alan-mode)
                             (list major-mode (char-syntax ?{) (char-syntax ?})))
           :without-rule (session nil)
           :with-rule (session t))
        (kill-buffer opened)))))"##,
        expect![[
            r#"OK (:syntax (alan-schema-mode 40 41) :generic-syntax (alan-mode 95 95) :without-rule ("'root' -> component {\n}" 23 2 0 "}") :with-rule ("'root' -> component {\n\11\n}" 24 2 2 "\11"))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        opening_each_file_of_a_real_alan_project_selects_its_mode_and_configures_its_compiler(),
        flycheck_runs_the_project_compiler_and_reports_only_this_files_diagnostics(),
        without_a_project_compiler_flycheck_falls_back_to_the_projects_build_script(),
        the_documented_electric_layout_rule_opens_a_line_inside_an_empty_alan_block(),
    ]
}
