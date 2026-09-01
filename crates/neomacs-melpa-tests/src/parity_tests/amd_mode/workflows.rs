use expect_test::expect;

use super::ParityBatchCase;

/// Starting a module from nothing, which is the first thing the package's
/// commentary tells a user to do: `amd-auto-insert' writes an empty `define'
/// with point inside the function body, then `amd-import-module' adds
/// dependencies by name.
///
/// The buffer is asserted after every step, so the two halves amd-mode has to
/// keep in step are both visible: the string goes into the dependency array and
/// the same name goes into the function's parameter list, in the same position.
/// Importing a name that is already required is asserted to change nothing at
/// all - `amd--import' returns early rather than duplicating it - and the
/// imports are separated by an idle period because js2-mode reparses from an
/// idle timer and each command reads the AST.
///
/// The last import is the one where the two halves deliberately differ.  Every
/// import prompts "Import as (DEFAULT):", and the first three answer it with
/// RET and take the default, which is the module path's base name; the fourth
/// answers `shortcut', so `lib/keyboard/bindings' enters the array under its
/// full path and the parameter list under the name the user chose.
fn starting_an_empty_module_and_importing_dependencies_by_name() -> ParityBatchCase {
    ParityBatchCase::value(
        "starting_an_empty_module_and_importing_dependencies_by_name",
        r##"(let* ((root (amd-test-project "amd-start"))
       (buffer (amd-test-open root "src/app/main.js" "")))
  (amd-test-in buffer
    (amd-auto-insert)
    (let ((template (list (amd-test-text) (point))))
      (amd-test-idle)
      (amd-test-answering "" nil (amd-import-module "lib/router"))
      (let ((first (amd-test-text)))
        (amd-test-idle)
        (amd-test-answering "" nil (amd-import-module "widgets/button"))
        (let ((second (amd-test-text)))
          (amd-test-idle)
          (amd-test-answering "" nil (amd-import-module "lib/router"))
          (let ((duplicate (amd-test-text)))
            (amd-test-idle)
            (amd-test-answering "shortcut" nil (amd-import-module "lib/keyboard/bindings"))
            (list template first second duplicate
                  (equal second duplicate)
                  (amd-test-text))))))))"##,
        expect![[
            r#"OK (("define([], function() {\n    \n});\n" 29) "define(['lib/router'], function(router) {\n    \n});\n" "define(['lib/router',\n\11'widgets/button'], function(router, button) {\n    \n});\n" "define(['lib/router',\n\11'widgets/button'], function(router, button) {\n    \n});\n" t "define(['lib/router',\n\11'widgets/button',\n\11'lib/keyboard/bindings'], function(router, button, shortcut) {\n    \n});\n")"#
        ]],
    )
}

fn importing_a_file_uses_a_relative_path_only_where_the_settings_say_so() -> ParityBatchCase {
    ParityBatchCase::value(
        "importing_a_file_uses_a_relative_path_only_where_the_settings_say_so",
        r##"(let* ((root (amd-test-project "amd-paths"))
       (_ (amd-test-write root "src/app/util/format.js" "define([], function() {});\n"))
       (_ (amd-test-write root "src/vendor/moment.js" "define([], function() {});\n")))
  (cl-flet ((import-both
              (label)
              (let ((buffer (amd-test-open root "src/app/main.js"
                                           "define([], function() {\n\n});\n")))
                (amd-test-in buffer
                  (amd-test-answering "" "src/app/util/format.js" (amd-import-file))
                  (amd-test-idle)
                  (amd-test-answering "" "src/vendor/moment.js" (amd-import-file))
                  (list label (amd-test-text))))))
    (list (import-both :default)
          (let ((amd-use-relative-file-name t))
            (import-both :relative-when-below))
          (let ((amd-always-use-relative-file-name t))
            (import-both :always-relative)))))"##,
        expect![[
            r#"OK ((:default "define(['src/app/util/format',\n\11'src/vendor/moment'], function(format, moment) {\n\n});\n") (:relative-when-below "define(['./util/format',\n\11'src/vendor/moment'], function(format, moment) {\n\n});\n") (:always-relative "define(['./util/format',\n\11'../vendor/moment'], function(format, moment) {\n\n});\n"))"#
        ]],
    )
}

fn killing_and_reordering_a_dependency_keeps_the_parameter_list_in_step() -> ParityBatchCase {
    ParityBatchCase::value(
        "killing_and_reordering_a_dependency_keeps_the_parameter_list_in_step",
        r##"(let* ((root (amd-test-project "amd-edit"))
       (buffer (amd-test-open root "src/app/main.js" amd-test-two-module-source)))
  (amd-test-in buffer
    (let ((bindings (list (key-description
                           (where-is-internal 'amd-kill-line amd-mode-map t))
                          (key-description
                           (where-is-internal 'amd-move-line-up amd-mode-map t))
                          (key-description
                           (where-is-internal 'amd-move-line-down amd-mode-map t))))
          (original (amd-test-text)))
      (goto-char (point-min))
      (search-forward "'lib/router'")
      (beginning-of-line)
      (execute-kbd-macro (kbd "C-k"))
      (let ((killed (amd-test-text)))
        (amd-test-idle)
        (amd-test-answering "" nil (amd-import-module "lib/router"))
        (amd-test-idle)
        (let ((reimported (amd-test-text)))
          (goto-char (point-min))
          (search-forward "'widgets/button'")
          (execute-kbd-macro (kbd "<C-S-down>"))
          (amd-test-idle)
          (let ((moved-down (amd-test-text)))
            (goto-char (point-min))
            (search-forward "'widgets/button'")
            (execute-kbd-macro (kbd "<C-S-up>"))
            (list bindings original killed reimported moved-down
                  (amd-test-text))))))))"##,
        expect![[
            r#"OK (("C-k" "C-S-<up>" "C-S-<down>") "define([\n    'lib/router',\n    'widgets/button'\n], function(router, button) {\n    return router;\n});\n" "define([\n    'widgets/button'\n], function(button) {\n    return router;\n});\n" "define([\n    'widgets/button',\n    'lib/router'\n], function(button, router) {\n    return router;\n});\n" "define([\n    'lib/router',\n    'widgets/button'\n], function(router, button) {\n    return router;\n});\n" "define([\n    'widgets/button',\n    'lib/router'\n], function(button, router) {\n    return router;\n});\n")"#
        ]],
    )
}

fn copying_the_buffers_module_path_applies_the_projects_rewrite_rules() -> ParityBatchCase {
    ParityBatchCase::value(
        "copying_the_buffers_module_path_applies_the_projects_rewrite_rules",
        r##"(let* ((root (amd-test-project "amd-copy"))
       (buffer (amd-test-open root "src/widgets/forms/button.js"
                              "define([], function() {\n\n});\n")))
  (amd-test-in buffer
    (cl-flet ((copied
                ()
                (let ((kill-ring nil))
                  (amd-kill-buffer-module)
                  (copy-sequence (car kill-ring)))))
      (list (copied)
            (let ((amd-rewrite-rules-alist '(("^src/" . "")))) (copied))
            (let ((amd-rewrite-rules-alist '(("^src/" . "") ("widgets/" . "ui/"))))
              (copied))
            (let ((amd-use-relative-file-name t)) (copied))
            (let ((amd-always-use-relative-file-name t)) (copied))
            (amd-test-text)
            (with-temp-buffer
              (setq default-directory "/")
              (js2-mode)
              (amd-mode 1)
              (let ((kill-ring nil))
                (list (projectile-project-p)
                      (mapcar (lambda (command)
                                (condition-case error (funcall command)
                                  (error (list (car error) (cadr error)))))
                              '(amd-kill-buffer-module amd-auto-insert
                                amd-search-references))
                      (amd-test-text)
                      kill-ring)))))))"##,
        expect![[
            r#"OK ("'src/widgets/forms/button'" "'widgets/forms/button'" "'ui/forms/button'" "'src/widgets/forms/button'" "'src/widgets/forms/button'" "define([], function() {\n\n});\n" (nil ((error "Not within a project") (error "Not within a project") (error "Not within a project")) "" nil))"#
        ]],
    )
}

fn searching_for_references_runs_ag_with_the_configured_ignores() -> ParityBatchCase {
    ParityBatchCase::value(
        "searching_for_references_runs_ag_with_the_configured_ignores",
        r##"(let* ((root (amd-test-project "amd-refs"))
       (log (amd-test-configure-ag
             root
             (concat "src/app/main.js:3:    'widgets/button',\n"
                     "src/app/other.js:2:define(['widgets/button'], function(button) {\n"
                     "src/vendor/bundle.js:1:var buttonlike = 1;\n")))
       (buffer (amd-test-open root "src/widgets/button.js"
                              "define([], function() {\n\n});\n")))
  (amd-test-in buffer
    (amd-search-references)
    (let ((found (list (amd-test-ag-arguments log)
                       (amd-test-xref-text "amd-refs"))))
      (kill-buffer "*xref*")
      (setenv "AMD_TEST_AG_OUTPUT" "")
      (let* ((message-start (with-current-buffer "*Messages*" (point-max)))
             (result (let ((amd-ag-ignored-dirs '("dist"))
                           (amd-ag-ignored-files '("*.bundle.js" "*.min.js")))
                       (amd-search-references))))
        (list found
              result
              (amd-test-ag-arguments log)
              (with-current-buffer "*Messages*"
                (buffer-substring-no-properties message-start (point-max)))
              (and (get-buffer "*xref*") t))))))"##,
        expect![[
            r#"OK ((("--js" "--noheading" "--ignore-dir" "bower_components" "--ignore-dir" "node_modules" "--ignore-dir" "build" "--ignore-dir" "lib" "--ignore" "*.min.js" "define\\([^])]+['|\"](.*/)?button['|\"]") "src/app/other.js\n2:define(['widgets/button'], function(button) {\nsrc/app/main.js\n3:'widgets/button',\n") "No reference found" ("--js" "--noheading" "--ignore-dir" "dist" "--ignore" "*.bundle.js" "--ignore" "*.min.js" "define\\([^])]+['|\"](.*/)?button['|\"]") "No reference found\n" nil)"#
        ]],
    )
}

fn a_reference_longer_than_a_hundred_characters_aborts_the_search() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_reference_longer_than_a_hundred_characters_aborts_the_search",
        r##"(let* ((root (amd-test-project "amd-long"))
       (long-line (concat "var b=" (make-string 120 ?x) "'button';"))
       (log (amd-test-configure-ag
             root
             (concat "src/app/main.js:3:define(['widgets/button'], function(button) {\n"
                     "src/vendor/all.min.js:1:" long-line "\n")))
       (buffer (amd-test-open root "src/widgets/button.js"
                              "define([], function() {\n\n});\n")))
  (amd-test-in buffer
    (let ((failure (condition-case error (amd-search-references)
                     (error (list :signal (car error)
                                  :on-a-line-of-length (length long-line))))))
      (setenv "AMD_TEST_AG_OUTPUT"
              "src/app/main.js:3:define(['widgets/button'], function(button) {\n")
      (let ((short (amd-search-references)))
        (list failure
              (amd-test-ag-arguments log)
              (and (bufferp short) t)
              (amd-test-xref-text "amd-long"))))))"##,
        expect![[
            r#"OK ((:signal wrong-type-argument :on-a-line-of-length 135) ("--js" "--noheading" "--ignore-dir" "bower_components" "--ignore-dir" "node_modules" "--ignore-dir" "build" "--ignore-dir" "lib" "--ignore" "*.min.js" "define\\([^])]+['|\"](.*/)?button['|\"]") t "src/app/main.js\n3:define(['widgets/button'], function(button) {\n")"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        starting_an_empty_module_and_importing_dependencies_by_name(),
        importing_a_file_uses_a_relative_path_only_where_the_settings_say_so(),
        killing_and_reordering_a_dependency_keeps_the_parameter_list_in_step(),
        copying_the_buffers_module_path_applies_the_projects_rewrite_rules(),
        searching_for_references_runs_ag_with_the_configured_ignores(),
        a_reference_longer_than_a_hundred_characters_aborts_the_search(),
    ]
}
