use expect_test::expect;

use super::ParityBatchCase;

fn aangit_menu_scaffolds_a_new_angular_workspace_with_the_default_switches() -> ParityBatchCase {
    ParityBatchCase::value(
        "aangit_menu_scaffolds_a_new_angular_workspace_with_the_default_switches",
        r##"(progn
  (aangit-test-setup-cli)
  (aangit-menu)
  (execute-kbd-macro (kbd "n"))
  (execute-kbd-macro (kbd "n storefront RET"))
  (list
   (aangit-test-commands)
   (buffer-name)
   major-mode
   (file-relative-name default-directory aangit-test-root)
   (aangit-test-relative-files "storefront")
   (aangit-test-active-prefix)
   (length (window-list))))"##,
        expect![[
            r#"OK (("ng new --defaults storefront --style=css --routing --standalone" "ng add --defaults --skip-confirmation @angular-eslint/schematics") "storefront" dired-mode "storefront/" ("angular.json" "src/app/app.component.ts" "src/main.ts") aangit-menu--generate-submenu 2)"#
        ]],
    )
}

fn aangit_new_project_switches_and_style_option_reach_the_ng_command_line() -> ParityBatchCase {
    ParityBatchCase::value(
        "aangit_new_project_switches_and_style_option_reach_the_ng_command_line",
        r##"(progn
  (aangit-test-setup-cli)
  (aangit-menu)
  (execute-kbd-macro (kbd "n"))
  (execute-kbd-macro (kbd "-r"))
  (execute-kbd-macro (kbd "-i"))
  (execute-kbd-macro (kbd "-t"))
  (execute-kbd-macro (kbd "-S"))
  (execute-kbd-macro (kbd "-y C-a C-k scss RET"))
  (execute-kbd-macro (kbd "n dashboard RET"))
  (list
   (aangit-test-commands)
   (aangit-test-relative-files "dashboard")
   (aangit-test-active-prefix)))"##,
        expect![[
            r#"OK (("ng new --defaults dashboard --skip-tests --style=scss --inline-template --inline-style --standalone" "ng add --defaults --skip-confirmation @angular-eslint/schematics") ("angular.json" "src/app/app.component.ts" "src/main.ts") aangit-menu--generate-submenu)"#
        ]],
    )
}

fn aangit_generate_component_prompts_for_a_name_and_writes_it_into_the_workspace() -> ParityBatchCase
{
    ParityBatchCase::value(
        "aangit_generate_component_prompts_for_a_name_and_writes_it_into_the_workspace",
        r##"(progn
  (aangit-test-setup-cli)
  (aangit-menu)
  (execute-kbd-macro (kbd "g"))
  (execute-kbd-macro (kbd "c"))
  (execute-kbd-macro (kbd "-s"))
  (execute-kbd-macro (kbd "-e"))
  (execute-kbd-macro (kbd "-m shared.module RET"))
  (execute-kbd-macro (kbd "n product-card RET"))
  (list
   (aangit-test-commands)
   (aangit-test-relative-files "src")
   (aangit-test-active-prefix)))"##,
        expect![[
            r#"OK (("ng generate component product-card --defaults --export --module=shared.module --standalone") ("app/product-card/product-card.component.ts") nil)"#
        ]],
    )
}

fn aangit_generate_service_and_module_submenus_issue_their_own_ng_commands() -> ParityBatchCase {
    ParityBatchCase::value(
        "aangit_generate_service_and_module_submenus_issue_their_own_ng_commands",
        r##"(progn
  (aangit-test-setup-cli)
  (aangit-menu)
  (execute-kbd-macro (kbd "g s n auth RET"))
  (aangit-menu)
  (execute-kbd-macro (kbd "g m -r -F n admin RET"))
  (aangit-menu)
  (execute-kbd-macro (kbd "g i n order RET"))
  (list
   (aangit-test-commands)
   (aangit-test-active-prefix)))"##,
        expect![[
            r#"OK (("ng generate service auth" "ng generate module admin --defaults --routing --flat" "ng generate interface order") nil)"#
        ]],
    )
}

fn aangit_adds_each_selected_schematic_and_installs_an_npm_package() -> ParityBatchCase {
    ParityBatchCase::value(
        "aangit_adds_each_selected_schematic_and_installs_an_npm_package",
        r##"(progn
  (aangit-test-setup-cli)
  (aangit-menu)
  (execute-kbd-macro (kbd "a"))
  (execute-kbd-macro (kbd "m"))
  (execute-kbd-macro (kbd "s"))
  (execute-kbd-macro (kbd "c"))
  (execute-kbd-macro (kbd "a"))
  (aangit-menu)
  (execute-kbd-macro (kbd "p rxjs@7.8.1 SPC @types/node RET"))
  (list
   (aangit-test-commands)
   (aangit-test-active-prefix)))"##,
        expect![[
            r#"OK (("ng add --defaults --skip-confirmation @ngrx/store" "ng add --defaults --skip-confirmation @angular/cdk/schematics" "ng add --defaults --skip-confirmation @angular/material" "npm install rxjs@7.8.1 @types/node") nil)"#
        ]],
    )
}

fn aangit_reports_missing_names_and_runs_no_command_line_tool_at_all() -> ParityBatchCase {
    ParityBatchCase::value(
        "aangit_reports_missing_names_and_runs_no_command_line_tool_at_all",
        r##"(progn
  (aangit-test-setup-cli)
  (let (observed)
    (aangit-menu)
    (execute-kbd-macro (kbd "g c n RET"))
    (push (list 'component (aangit-test-last-message)) observed)
    (aangit-menu)
    (execute-kbd-macro (kbd "g s n RET"))
    (push (list 'service (aangit-test-last-message)) observed)
    (aangit-menu)
    (execute-kbd-macro (kbd "g m n RET"))
    (push (list 'module (aangit-test-last-message)) observed)
    (aangit-menu)
    (execute-kbd-macro (kbd "g i n RET"))
    (push (list 'interface (aangit-test-last-message)) observed)
    (aangit-menu)
    (execute-kbd-macro (kbd "p RET"))
    (push (list 'npm (aangit-test-last-message)) observed)
    (aangit-menu)
    (execute-kbd-macro (kbd "a a"))
    (push (list 'schematic (aangit-test-last-message)) observed)
    (list (nreverse observed)
          (aangit-test-commands)
          (file-exists-p aangit-test-log))))"##,
        expect![[
            r#"OK (((component "missing component name") (service "missing service name") (module "missing module name") (interface "missing interface name") (npm "missing package name") (schematic "missing schematic name")) no-command-ran nil)"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        aangit_menu_scaffolds_a_new_angular_workspace_with_the_default_switches(),
        aangit_new_project_switches_and_style_option_reach_the_ng_command_line(),
        aangit_generate_component_prompts_for_a_name_and_writes_it_into_the_workspace(),
        aangit_generate_service_and_module_submenus_issue_their_own_ng_commands(),
        aangit_adds_each_selected_schematic_and_installs_an_npm_package(),
        aangit_reports_missing_names_and_runs_no_command_line_tool_at_all(),
    ]
}
