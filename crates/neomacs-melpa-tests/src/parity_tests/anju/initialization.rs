use expect_test::expect;

use super::ParityBatchCase;

fn anju_init_executes_the_enabled_mouse_ui_pipeline_in_user_visible_order() -> ParityBatchCase {
    ParityBatchCase::value(
        "anju_init_executes_the_enabled_mouse_ui_pipeline_in_user_visible_order",
        r##"(let ((context-menu-mode nil)
               (anju-unset-legacy-mouse-bindings-enable t)
               (anju-mode-line-bindings-enable t)
               (anju-reconfigure-main-menu-enable t)
               (anju-reconfigure-context-menu-functions-enable t)
               (anju-reconfigure-main-menu-hook
                '(anju-test-main-one anju-test-main-two))
               events)
         (cl-letf (((symbol-function 'context-menu-mode)
                    (lambda (argument)
                      (setq context-menu-mode (> argument 0))
                      (push (list 'context-menu-mode argument) events)))
                   ((symbol-function 'anju-utils--unset-legacy-mouse-bindings)
                    (lambda ()
                      (push 'unset-legacy events)))
                   ((symbol-function 'anju-mode-line--set-bindings)
                    (lambda ()
                      (push 'mode-line-bindings events)))
                   ((symbol-function 'anju-test-main-one)
                    (lambda ()
                      (push 'main-one events)))
                   ((symbol-function 'anju-test-main-two)
                    (lambda ()
                      (push 'main-two events)))
                   ((symbol-function 'anju-reconfigure-context-menu-functions)
                    (lambda ()
                      (push 'context-menu-functions events))))
           (list
            (anju-init)
            context-menu-mode
            (nreverse events))))"##,
        expect![
            "OK (#1=(context-menu-functions) t ((context-menu-mode 1) unset-legacy mode-line-bindings main-one main-two . #1#))"
        ],
    )
}

fn anju_init_respects_disabled_subsystems_and_existing_context_mode() -> ParityBatchCase {
    ParityBatchCase::value(
        "anju_init_respects_disabled_subsystems_and_existing_context_mode",
        r##"(let ((context-menu-mode t)
               (anju-unset-legacy-mouse-bindings-enable nil)
               (anju-mode-line-bindings-enable nil)
               (anju-reconfigure-main-menu-enable nil)
               (anju-reconfigure-context-menu-functions-enable nil)
               events)
         (cl-letf (((symbol-function 'context-menu-mode)
                    (lambda (&rest arguments)
                      (push (cons 'context-menu-mode arguments) events)))
                   ((symbol-function 'anju-utils--unset-legacy-mouse-bindings)
                    (lambda () (push 'legacy events)))
                   ((symbol-function 'anju-mode-line--set-bindings)
                    (lambda () (push 'mode-line events)))
                   ((symbol-function 'run-hooks)
                    (lambda (&rest arguments)
                      (push (cons 'hooks arguments) events)))
                   ((symbol-function 'anju-reconfigure-context-menu-functions)
                    (lambda () (push 'context-menu events))))
           (list (anju-init) context-menu-mode events)))"##,
        expect!["OK (nil t nil)"],
    )
}

fn anju_main_file_options_and_register_menu_reconfiguration_changes_exact_keys() -> ParityBatchCase
{
    ParityBatchCase::value(
        "anju_main_file_options_and_register_menu_reconfiguration_changes_exact_keys",
        r##"(let ((anju-file-menu-replace-make-frame-on t))
         (anju-main-menu--reconfigure-file)
         (anju-main-menu--reconfigure-options)
         (anju-main-menu--reconfigure-registers)
         (list
          (lookup-key global-map [menu-bar file Swap\ Window])
          (lookup-key global-map [menu-bar file make-frame-on-display])
          (lookup-key global-map [menu-bar file make-frame-on-monitor])
          (lookup-key global-map [menu-bar options cua-mode])
          (lookup-key global-map [menu-bar Registers])
          (anju-test-menu-entries anju-registers-menu)))"##,
        expect![[
            r#"OK ((keymap "Swap Window" (↑ menu-item "↑" windmove-swap-states-up :visible (window-in-direction 'above) :help "Swap window up") (↓ menu-item "↓" windmove-swap-states-down :visible (window-in-direction 'below) :help "Swap window down") (← menu-item "←" windmove-swap-states-left :visible (window-in-direction 'left) :help "Swap window left") (→ menu-item "→" windmove-swap-states-right :visible (window-in-direction 'right) :help "Swap window right")) make-frame-on-display make-frame-on-monitor nil (keymap "Registers" (Store menu-item "Store" (keymap "Store" (Text\ Region… menu-item "Text Region…" copy-to-register :enable (use-region-p) :help "Copy region of text between START and END into REGISTER") (Prepend\ to\ Register… menu-item "Prepend to Register…" prepend-to-register :enable (use-region-p) :help "Prepend region of text between START and END to REGISTER") (Append\ to\ Register… menu-item "Append to Register…" append-to-register :enable (use-region-p) :help "Append region of text between START and END to REGISTER") (nil . #1=("--")) (Rectangle… menu-item "Rectangle…" copy-rectangle-to-register :enable (use-region-p) :help "Copy rectangular region of text between START and END into REGISTER") (nil-5 . #1#) (Point… menu-item "Point…" point-to-register :help "Store current location of point in REGISTER") (nil-7 . #1#) (Number… menu-item "Number…" number-to-register :help "Store NUMBER (either at point or via prefix) in REGISTER") (Increment\ Number… menu-item "Increment Number…" increment-register :help "Augment contents of REGISTER using PREFIX") (nil-10 . #1#) (Window\ Configuration… menu-item "Window Configuration…" window-configuration-to-register :help "Store the window configuration of the selected frame in REGISTER") (nil-12 . #1#) (Keyboard\ Macro… menu-item "Keyboard Macro…" kmacro-to-register :help "Store the last keyboard macro in register R"))) (Insert… menu-item "Insert…" insert-register :help "Insert contents of REGISTER at point") (Jump… menu-item "Jump…" jump-to-register :help "Go to location stored in REGISTER, or restore configuration stored there")) ((Store "Store" <submenu> :enable nil :visible nil :style nil :selected nil :help nil) (Insert… "Insert…" insert-register :enable nil :visible nil :style nil :selected nil :help "Insert contents of REGISTER at point") (Jump… "Jump…" jump-to-register :enable nil :visible nil :style nil :selected nil :help "Go to location stored in REGISTER, or restore configuration stored there")))"#
        ]],
    )
}

fn anju_text_menu_reconfiguration_replaces_centering_with_practical_submenus() -> ParityBatchCase {
    ParityBatchCase::value(
        "anju_text_menu_reconfiguration_replaces_centering_with_practical_submenus",
        r##"(progn
         (anju-main-menu--reconfigure-text-mode)
         (list
          (anju-test-menu-labels text-mode-menu)
          (mapcar
           (lambda (label)
             (lookup-key text-mode-menu (vector (intern label))))
           '("Transform Text" "Style" "Center" "Fill"))))"##,
        expect![[
            r#"OK (("Transform Text" "Style" "Center" "Fill" "Auto Fill") ((keymap "Transform Text" (Make\ Upper\ Case menu-item "Make Upper Case" upcase-region :help "Convert selected region to upper case") (Make\ Lower\ Case menu-item "Make Lower Case" downcase-region :help "Convert selected region to lower case") (Capitalize menu-item "Capitalize" capitalize-region :help "Convert the selected region to capitalized form")) (keymap "Style" (Bold menu-item "Bold" anju-style-bold :help "Bold selected region") (Italic menu-item "Italic" anju-style-italic :help "Italic selected region") (Code menu-item "Code" anju-style-code :help "Code selected region") (Underline menu-item "Underline" anju-style-underline :visible (derived-mode-p 'org-mode) :help "Underline selected region") (Verbatim menu-item "Verbatim" anju-style-verbatim :visible (derived-mode-p 'org-mode) :help "Verbatim selected region") (Strike\ Through menu-item "Strike Through" anju-style-strike-through :help "Strike-through selected region") (Remove menu-item "Remove" anju-style-remove :visible (and (derived-mode-p 'org-mode) visible-mode) :help "Remove markup from selected region")) (keymap "Center" (Line menu-item "Line" center-line :help "Center the line point is on, within the width specified by ‘fill-column’") (Region menu-item "Region" center-region :enable (use-region-p) :help "Center each nonblank line starting in the region") (Paragraph menu-item "Paragraph" center-paragraph :help "Center each nonblank line in the paragraph at or after point")) (keymap "Fill" (Paragraph menu-item "Paragraph" fill-paragraph :help "Fill paragraph at or after point") (Region menu-item "Region" fill-region :enable (use-region-p) :help "Fill each of the paragraphs in the region") (Region\ as\ paragraph menu-item "Region as paragraph" fill-region-as-paragraph :enable (use-region-p) :help "Fill the region as if it were a single paragraph") (Individual\ paragraphs menu-item "Individual paragraphs" fill-individual-paragraphs :enable (use-region-p) :help "Fill paragraphs of uniform indentation within the region") (Non-uniform\ paragraphs menu-item "Non-uniform paragraphs" fill-nonuniform-paragraphs :enable (use-region-p) :help "Fill paragraphs within the region, allowing varying indentation within each"))))"#
        ]],
    )
}

fn anju_imenu_reconfiguration_installs_mode_specific_hooks_and_deep_org_indexing() -> ParityBatchCase
{
    ParityBatchCase::value(
        "anju_imenu_reconfiguration_installs_mode_specific_hooks_and_deep_org_indexing",
        r##"(let ((markdown-mode-hook nil)
               (makefile-mode-hook nil)
               (prog-mode-hook nil)
               (org-mode-hook nil)
               (org-imenu-depth 2))
         (anju-main-menu--reconfigure-imenu)
         (list
          markdown-mode-hook
          makefile-mode-hook
          prog-mode-hook
          org-mode-hook
          org-imenu-depth
          (with-temp-buffer
            (setq imenu-auto-rescan nil)
            (anju-imenu-auto-rescan)
            imenu-auto-rescan)))"##,
        expect![
            "OK ((anju-imenu-auto-rescan imenu-add-menubar-index) (anju-imenu-auto-rescan imenu-add-menubar-index) (anju-imenu-auto-rescan anju-imenu-add-menubar-index) (anju-imenu-auto-rescan imenu-add-menubar-index) 7 t)"
        ],
    )
}

fn anju_help_frame_commands_dispatch_their_distinct_interactive_targets() -> ParityBatchCase {
    ParityBatchCase::value(
        "anju_help_frame_commands_dispatch_their_distinct_interactive_targets",
        r##"(let (commands)
         (cl-letf
             (((symbol-function 'anju-utils--command-in-new-frame)
               (lambda (command)
                 (push command commands)
                 command)))
           (list
            (anju-info-in-new-frame)
            (anju-new-info-in-new-frame)
            (anju-man-in-new-frame)
            (nreverse commands))))"##,
        expect!["OK (info info-display-manual man (info info-display-manual man))"],
    )
}

fn anju_main_menu_static_submenus_preserve_every_command_and_predicate() -> ParityBatchCase {
    ParityBatchCase::value(
        "anju_main_menu_static_submenus_preserve_every_command_and_predicate",
        r##"(list
         (anju-test-menu-entries anju-window-swap-menu)
         (anju-test-menu-entries anju-transpose-menu)
         (anju-test-menu-entries anju-move-text-menu)
         (anju-test-menu-entries anju-delete-space-menu)
         (anju-test-menu-entries anju-kmacro-menu)
         (anju-test-menu-entries anju-makefile-modes-menu)
         (anju-test-menu-entries anju-context-window-management-menu))"##,
        expect![[
            r#"OK (((↑ "↑" windmove-swap-states-up :enable nil :visible (window-in-direction 'above) :style nil :selected nil :help "Swap window up") (↓ "↓" windmove-swap-states-down :enable nil :visible (window-in-direction 'below) :style nil :selected nil :help "Swap window down") (← "←" windmove-swap-states-left :enable nil :visible (window-in-direction 'left) :style nil :selected nil :help "Swap window left") (→ "→" windmove-swap-states-right :enable nil :visible (window-in-direction 'right) :style nil :selected nil :help "Swap window right")) ((Characters "Characters" transpose-chars :enable nil :visible nil :style nil :selected nil :help "Interchange characters around point, moving forward one character") (Words "Words" transpose-words :enable nil :visible nil :style nil :selected nil :help "Interchange words around point, leaving point at end of them") (Lines "Lines" transpose-lines :enable nil :visible nil :style nil :selected nil :help "Exchange current line and previous line, leaving point after both") (Sentences "Sentences" transpose-sentences :enable nil :visible nil :style nil :selected nil :help "Interchange the current sentence with the next one") (Paragraphs "Paragraphs" transpose-paragraphs :enable nil :visible nil :style nil :selected nil :help "Interchange the current paragraph with the next one") (Regions "Regions" transpose-regions :enable nil :visible nil :style nil :selected nil :help "region STARTR1 to ENDR1 with STARTR2 to ENDR2") (Balanced\ Expressions\ \(sexps\) "Balanced Expressions (sexps)" transpose-sexps :enable nil :visible nil :style nil :selected nil :help "Like C-t (‘transpose-chars’), but applies to balanced expressions (sexps)")) ((Word\ → "Word →" casual-editkit-move-word-forward :enable nil :visible nil :style nil :selected nil :help "Move word to the right of point forward one word") (Word\ ← "Word ←" casual-editkit-move-word-backward :enable nil :visible nil :style nil :selected nil :help "Move word to the right of point backward one word") (Sentence\ → "Sentence →" casual-editkit-move-sentence-forward :enable nil :visible nil :style nil :selected nil :help "Move sentence to the right of point forward one sentence") (Sentence\ ← "Sentence ←" casual-editkit-move-sentence-backward :enable nil :visible nil :style nil :selected nil :help "Move sentence to the right of point backward one sentence") (Balanced\ Expression\ \(sexp\)\ → "Balanced Expression (sexp) →" casual-editkit-move-sexp-forward :enable nil :visible nil :style nil :selected nil :help "Move balanced expression (sexp) to the right of point forward one sexp") (Balanced\ Expression\ \(sexp\)\ ← "Balanced Expression (sexp) ←" casual-editkit-move-sexp-backward :enable nil :visible nil :style nil :selected nil :help "Move balanced expression (sexp) to the right of point backward one sexp")) ((Join\ Line "Join Line" join-line :enable nil :visible nil :style nil :selected nil :help "Join this line to previous and fix up whitespace at join") (Just\ One\ Space "Just One Space" just-one-space :enable nil :visible nil :style nil :selected nil :help "Delete all spaces and tabs around point, leaving one space") (Horizontal\ Space "Horizontal Space" delete-horizontal-space :enable nil :visible nil :style nil :selected nil :help "Delete all spaces and tabs around point") (Pair "Pair" delete-pair :enable nil :visible nil :style nil :selected nil :help "Delete a pair of characters enclosing ARG sexps that follow point") (Duplicate\ Lines "Duplicate Lines" delete-duplicate-lines :enable (use-region-p) :visible nil :style nil :selected nil :help "Delete all but one copy of any identical lines in the region") (Blank\ Lines "Blank Lines" delete-blank-lines :enable nil :visible nil :style nil :selected nil :help "On blank line, delete all surrounding blank lines, leaving just one") (Whitespace\ Cleanup "Whitespace Cleanup" whitespace-cleanup :enable nil :visible nil :style nil :selected nil :help "Cleanup some blank problems in all buffer or at region") (Trailing\ Whitespace "Trailing Whitespace" delete-trailing-whitespace :enable nil :visible nil :style nil :selected nil :help "Delete trailing whitespace between START and END") (Zap\ up\ to… "Zap up to…" zap-up-to-char :enable nil :visible nil :style nil :selected nil :help "Kill up to, but not including occurrence of CHAR") (Zap\ to… "Zap to…" zap-to-char :enable nil :visible nil :style nil :selected nil :help "Kill up to and including occurrence of CHAR")) ((kmacro-start-macro "Record" kmacro-start-macro :enable nil :visible (not defining-kbd-macro) :style nil :selected nil :help "Record subsequent keyboard input, defining a keyboard macro") (kmacro-end-macro "Stop" kmacro-end-macro :enable nil :visible defining-kbd-macro :style nil :selected nil :help "Finish defining a keyboard macro") (kmacro-insert-counter "Insert counter" kmacro-insert-counter :enable nil :visible defining-kbd-macro :style nil :selected nil :help "Insert current value of ‘kmacro-counter’, then increment it by ARG") (kmacro-set-format "Set counter format…" kmacro-set-format :enable nil :visible defining-kbd-macro :style nil :selected nil :help "Set the format of ‘kmacro-counter’ to FORMAT") (kbd-macro-query "Query user" kbd-macro-query :enable nil :visible defining-kbd-macro :style nil :selected nil :help "Query user during kbd macro execution") (kmacro-end-and-call-macro "Run last" kmacro-end-and-call-macro :enable (and (not defining-kbd-macro) last-kbd-macro) :visible nil :style nil :selected nil :help "Call last keyboard macro, ending it first if currently being defined") (kmacro-name-last-macro "Name last…" kmacro-name-last-macro :enable (and (not defining-kbd-macro) last-kbd-macro) :visible nil :style nil :selected nil :help "Assign a name to the last keyboard macro defined") (kmacro-bind-to-key "Bind last…" kmacro-bind-to-key :enable (and (not defining-kbd-macro) last-kbd-macro) :visible nil :style nil :selected nil :help "When not defining or executing a macro, offer to bind last macro to a key") (kmacro-edit-macro "Edit last" kmacro-edit-macro :enable (and (not defining-kbd-macro) last-kbd-macro) :visible nil :style nil :selected nil :help "As edit last keyboard macro, but without kmacro-repeat property") (kmacro-step-edit-macro "Step edit macro…" kmacro-step-edit-macro :enable (and (not defining-kbd-macro) last-kbd-macro) :visible nil :style nil :selected nil :help "Step edit and execute last keyboard macro") (edit-kbd-macro "Edit with binding…" edit-kbd-macro :enable (not defining-kbd-macro) :visible nil :style nil :selected nil :help "Edit a keyboard macro") (insert-kbd-macro "Insert macro named…" insert-kbd-macro :enable (not defining-kbd-macro) :visible nil :style nil :selected nil :help "Insert in buffer the definition of kbd macro MACRONAME, as Lisp code") (kmacro-edit-lossage "New macro from history…" kmacro-edit-lossage :enable (and (fboundp #'kmacro-edit-lossage) (not defining-kbd-macro)) :visible nil :style nil :selected nil :help "Edit most recent 300 keystrokes as a keyboard macro") (kmacro-menu "List macros" kmacro-menu :enable (and (not defining-kbd-macro) last-kbd-macro) :visible nil :style nil :selected nil :help "List run-time defined keyboard macros")) ((makefile-automake-mode "automake" makefile-automake-mode :enable nil :visible nil :style nil :selected nil :help "An adapted ‘makefile-mode’ that knows about automake") (makefile-bsdmake-mode "BSD" makefile-bsdmake-mode :enable nil :visible nil :style nil :selected nil :help "An adapted ‘makefile-mode’ that knows about BSD make") (makefile-gmake-mode "GNU" makefile-gmake-mode :enable nil :visible nil :style nil :selected nil :help "An adapted ‘makefile-mode’ that knows about gmake") (makefile-imake-mode "imake" makefile-imake-mode :enable nil :visible nil :style nil :selected nil :help "An adapted ‘makefile-mode’ that knows about imake") (makefile-mode "make" makefile-mode :enable nil :visible nil :style nil :selected nil :help "Major mode for editing standard Makefiles") (makefile-makepp-mode "makepp" makefile-makepp-mode :enable nil :visible nil :style nil :selected nil :help "An adapted ‘makefile-mode’ that knows about makepp")) ((× "×" delete-window :enable nil :visible (not (one-window-p t)) :style nil :selected nil :help "Delete window") (Split\ → "Split →" mouse-split-window-horizontally :enable nil :visible nil :style nil :selected nil :help "Split right at mouse point") (Split\ ↓ "Split ↓" mouse-split-window-vertically :enable nil :visible nil :style nil :selected nil :help "Split below at mouse point") (Swap "Swap" <submenu> :enable nil :visible (and (eq (selected-window) (anju-window-under-mouse)) (not (one-window-p t))) :style nil :selected nil :help nil)))"#
        ]],
    )
}

pub(super) fn initialization_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        anju_init_executes_the_enabled_mouse_ui_pipeline_in_user_visible_order(),
        anju_init_respects_disabled_subsystems_and_existing_context_mode(),
        anju_main_file_options_and_register_menu_reconfiguration_changes_exact_keys(),
        anju_text_menu_reconfiguration_replaces_centering_with_practical_submenus(),
        anju_imenu_reconfiguration_installs_mode_specific_hooks_and_deep_org_indexing(),
        anju_help_frame_commands_dispatch_their_distinct_interactive_targets(),
        anju_main_menu_static_submenus_preserve_every_command_and_predicate(),
    ]
}
