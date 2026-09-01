use expect_test::expect;

use super::ParityBatchCase;

fn opening_an_ahk_script_selects_the_mode_and_sets_up_its_syntax() -> ParityBatchCase {
    ParityBatchCase::value(
        "opening_an_ahk_script_selects_the_mode_and_sets_up_its_syntax",
        r##"
        ;; Opening a `.ahk' file is all a user does: the autoload puts the
        ;; extension on `auto-mode-alist', so the mode arrives without being
        ;; asked for.  What it then installs is what makes editing AutoHotkey
        ;; work at all - a syntax table where `#', `_', `@' and even `\' are
        ;; word constituents so `%A_ScriptDir%' and `#SingleInstance' are single
        ;; words, a backtick that escapes, `;' to end of line for comments and
        ;; `/* */' for blocks - plus the keymap, the comment variables and the
        ;; completion hook.
        (let ((buffer nil))
          (unwind-protect
              (progn
                (setq buffer (ahk-test-visit))
                (list
                 :mode major-mode
                 :derived-from (get 'ahk-mode 'derived-mode-parent)
                 :mode-name (ahk-test-copy mode-name)
                 :selected-by-extension (cdr (assoc "\\.ahk\\'" auto-mode-alist))
                 :word-constituents (mapcar #'ahk-test-syntax-of '(?# ?_ ?@ ?\\ ?a))
                 :comment-syntax (mapcar #'ahk-test-syntax-of '(?\; ?/ ?* ?\n))
                 :escape-character (ahk-test-syntax-of ?`)
                 :comment-variables (list comment-start comment-end comment-start-skip
                                          block-comment-start block-comment-end)
                 :indent-functions (list indent-line-function indent-region-function)
                 :completion-hook completion-at-point-functions
                 :bindings (mapcar (lambda (keys)
                                     (list (ahk-test-copy keys)
                                           (key-binding (kbd keys))))
                                   '("C-c C-c" "C-c C-b" "C-c M-i" "C-c C-k"
                                     "C-c C-?" "C-c C-r"))
                 :indentation-default ahk-indentation))
            (when (buffer-live-p buffer) (kill-buffer buffer))))
    "##,
        expect![[
            r#"OK (:mode ahk-mode :derived-from prog-mode :mode-name "AHK" :selected-by-extension ahk-mode :word-constituents ((35 "w") (95 "w") (64 "w") (92 "w") (97 "w")) :comment-syntax ((59 "<") (47 ".") (42 ".") (10 ">")) :escape-character (96 "\\") :comment-variables (";" "" ";+ *" "/*" "*/") :indent-functions (ahk-indent-line ahk-indent-region) :completion-hook (ahk-completion-at-point t) :bindings (("C-c C-c" ahk-comment-dwim) ("C-c C-b" ahk-comment-block-dwim) ("C-c M-i" ahk-indent-message) ("C-c C-k" ahk-run-script) ("C-c C-?" ahk-lookup-web) ("C-c C-r" ahk-lookup-chm)) :indentation-default 8)"#
        ]],
    )
}

fn a_realistic_script_is_highlighted_by_kind() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_realistic_script_is_highlighted_by_kind",
        r##"
        ;; The whole point of the mode is telling the parts of a script apart on
        ;; sight, and ahk-mode paints seven different kinds.  The fixture is one
        ;; ordinary script containing one of each: directives, a built-in
        ;; variable, a command, a user function definition and its call, a
        ;; hotkey, a hotstring, a label, `return', strings, a line comment and a
        ;; block comment.  Pinning the runs line by line rather than a token
        ;; inventory is deliberate - the keyword list is ordered and later rules
        ;; cannot override earlier ones, so where one rule stops and the next
        ;; begins is the behaviour.
        (let ((buffer nil))
          (unwind-protect
              (progn
                (setq buffer (ahk-test-visit))
                (list
                 :comment (ahk-test-faces-where "; Inventory")
                 :directive (ahk-test-faces-where "#SingleInstance")
                 :builtin-variable (ahk-test-faces-where "SetWorkingDir")
                 :function-definition (ahk-test-faces-where "CountWidgets(name")
                 :command-with-interpolation (ahk-test-faces-where "Counting %name%")
                 :hotkey (ahk-test-faces-where "^!w::")
                 :return-is-a-warning (ahk-test-faces-where "return WidgetCount")
                 :hotstring (ahk-test-faces-where "::btw::")
                 :label (ahk-test-faces-where "ReportLabel:")
                 :string-with-concatenation (ahk-test-faces-where "Total: ")
                 :block-comment (ahk-test-faces-where "A block comment")
                 :directives-found (ahk-test-tokens-with-face 'font-lock-preprocessor-face)
                 :warnings-found (ahk-test-tokens-with-face 'font-lock-warning-face)))
            (when (buffer-live-p buffer) (kill-buffer buffer))))
    "##,
        expect![[
            r##"OK (:comment ((font-lock-comment-delimiter-face "; ") (font-lock-comment-face "Inventory helper hotkeys")) :directive ((font-lock-preprocessor-face "#SingleInstance") (nil " force")) :builtin-variable ((font-lock-keyword-face "SetWorkingDir") (nil " ") (font-lock-variable-name-face "%A_ScriptDir%")) :function-definition ((font-lock-function-name-face "CountWidgets") (nil "(name") (font-lock-builtin-face ",") (nil " price) {")) :command-with-interpolation ((nil "    ") (font-lock-keyword-face "MsgBox") (font-lock-builtin-face ",") (nil " Counting ") (font-lock-variable-name-face "%name%")) :hotkey ((font-lock-constant-face "^!w") (nil "::")) :return-is-a-warning ((nil "    ") (font-lock-warning-face "return") (nil " WidgetCount")) :hotstring ((nil "::btw::by the way")) :label ((font-lock-doc-face "ReportLabel") (nil ":")) :string-with-concatenation ((nil "    ") (font-lock-keyword-face "MsgBox") (font-lock-builtin-face ",") (nil " % ") (font-lock-string-face "\"Total: \"") (nil " ") (font-lock-builtin-face ".") (nil " WidgetCount")) :block-comment ((font-lock-comment-face "   A block comment describing")) :directives-found ("#NoEnv" "#SingleInstance") :warnings-found ("return"))"##
        ]],
    )
}

fn indenting_a_script_follows_its_blocks_and_honours_the_width() -> ParityBatchCase {
    ParityBatchCase::value(
        "indenting_a_script_follows_its_blocks_and_honours_the_width",
        r##"
        ;; A user reindents a script they have pasted in flat.  ahk-mode has to
        ;; put the body of a function, of an `if'/`else' pair and of a hotkey
        ;; label at the right depth, close them again on the brace, and leave
        ;; the top level alone.  `ahk-indentation' is a documented setting, so
        ;; the same text at a different width has to come out differently -
        ;; which is also what proves the indenter is computing depth rather
        ;; than copying the previous line.
        (let ((buffer nil))
          (unwind-protect
              (let ((flat "\
CountWidgets(name, price) {
MsgBox, Counting %name%
if (price > 10) {
WidgetCount += 1
} else {
WidgetCount := 0
}
return WidgetCount
}
"))
                (setq buffer (ahk-test-visit flat "scripts/flat.ahk"))
                (list
                 :default-width ahk-indentation
                 :indented-at-default
                 (progn (indent-region (point-min) (point-max))
                        (buffer-substring-no-properties (point-min) (point-max)))
                 :indented-at-four
                 (let ((ahk-indentation 4))
                   (erase-buffer)
                   (insert flat)
                   (indent-region (point-min) (point-max))
                   (buffer-substring-no-properties (point-min) (point-max)))
                 :reindenting_is_stable
                 (progn (indent-region (point-min) (point-max))
                        (buffer-substring-no-properties (point-min) (point-max)))
                 :reported-indentation
                 (progn (goto-char (point-min))
                        (forward-line 3)
                        (ahk-indent-message)
                        (ahk-test-messages "^[0-9]+$"))))
            (when (buffer-live-p buffer)
              (with-current-buffer buffer (set-buffer-modified-p nil))
              (kill-buffer buffer))))
    "##,
        expect![[
            r#"OK (:default-width 8 :indented-at-default "\11CountWidgets(name, price) {\n\11\11MsgBox, Counting %name%\n\11\11if (price > 10) {\n\11\11\11WidgetCount += 1\n\11\11} else {\n\11\11\11WidgetCount := 0\n\11\11}\n\11\11return WidgetCount\n}\n" :indented-at-four "    CountWidgets(name, price) {\n\11MsgBox, Counting %name%\n\11if (price > 10) {\n\11    WidgetCount += 1\n\11} else {\n\11    WidgetCount := 0\n\11}\n\11return WidgetCount\n}\n" :reindenting_is_stable "\11    CountWidgets(name, price) {\n\11\11    MsgBox, Counting %name%\n\11\11    if (price > 10) {\n\11\11\11    WidgetCount += 1\n\11\11    } else {\n\11\11\11    WidgetCount := 0\n\11\11    }\n\11return WidgetCount\n}\n" :reported-indentation ("28"))"#
        ]],
    )
}

fn commenting_offers_both_line_and_block_notation() -> ParityBatchCase {
    ParityBatchCase::value(
        "commenting_offers_both_line_and_block_notation",
        r##"
        ;; AutoHotkey has two comment forms and ahk-mode binds one command to
        ;; each: `C-c C-c' for `;' line comments and `C-c C-b' for `/* */'
        ;; blocks.  Both are `comment-dwim' underneath, so they have to comment
        ;; a single line, comment an active region, and uncomment what they
        ;; commented - the round trip is the assertion, because a comment
        ;; command that cannot undo itself is worse than none.
        (let ((buffer nil))
          (unwind-protect
              (let ((source "\
MsgBox, first
MsgBox, second
MsgBox, third
"))
                (setq buffer (ahk-test-visit source "scripts/comments.ahk"))
                (list
                 :line-comment
                 (progn (goto-char (point-min))
                        (execute-kbd-macro (kbd "C-c C-c"))
                        (buffer-substring-no-properties (point-min) (point-max)))
                 :line-uncomment
                 (progn (goto-char (point-min))
                        (execute-kbd-macro (kbd "C-c C-c"))
                        (buffer-substring-no-properties (point-min) (point-max)))
                 :block-comment-region
                 (progn (goto-char (point-min))
                        (set-mark (point))
                        (forward-line 2)
                        (activate-mark)
                        (execute-kbd-macro (kbd "C-c C-b"))
                        (deactivate-mark)
                        (buffer-substring-no-properties (point-min) (point-max)))
                 :region-comment-with-line-notation
                 (progn (erase-buffer)
                        (insert source)
                        (goto-char (point-min))
                        (set-mark (point))
                        (forward-line 2)
                        (activate-mark)
                        (execute-kbd-macro (kbd "C-c C-c"))
                        (deactivate-mark)
                        (buffer-substring-no-properties (point-min) (point-max)))))
            (when (buffer-live-p buffer)
              (with-current-buffer buffer (set-buffer-modified-p nil))
              (kill-buffer buffer))))
    "##,
        expect![[
            r#"OK (:line-comment "MsgBox, first\11\11\11;\nMsgBox, second\nMsgBox, third\n" :line-uncomment "MsgBox, first\11\11\11;\nMsgBox, second\nMsgBox, third\n" :block-comment-region "/*\n * MsgBox, first\11\11\11;\n * MsgBox, second\n */\nMsgBox, third\n" :region-comment-with-line-notation "; MsgBox, first\n; MsgBox, second\nMsgBox, third\n")"#
        ]],
    )
}

fn completion_offers_keywords_and_annotates_them_by_kind() -> ParityBatchCase {
    ParityBatchCase::value(
        "completion_offers_keywords_and_annotates_them_by_kind",
        r##"
        ;; The mode puts its own function on `completion-at-point-functions',
        ;; completing from AutoHotkey's command, function and variable tables
        ;; and annotating each candidate with a letter for its kind - `c'
        ;; command, `f' function, `v' variable, `k' key, `d' directive.  It also
        ;; declares itself non-exclusive, so other capfs still get a turn.  The
        ;; prefixes below are chosen to return a small mixed set rather than a
        ;; single kind, so a broken annotation cannot hide behind agreement.
        (let ((buffer nil))
          (unwind-protect
              (progn
                (setq buffer (ahk-test-visit "MsgBox, hello\nWin" "scripts/complete.ahk"))
                (list
                 :after-win (progn (goto-char (point-max)) (ahk-test-candidates))
                 :after-a-variable-prefix
                 (progn (goto-char (point-max))
                        (insert "\nA_Scr")
                        (ahk-test-candidates))
                 :case-insensitive
                 (progn (goto-char (point-max))
                        (insert "\nmsgb")
                        (ahk-test-candidates))
                 :no-match
                 (progn (goto-char (point-max))
                        (insert "\nzzzznotakeyword")
                        (ahk-test-candidates))))
            (when (buffer-live-p buffer)
              (with-current-buffer buffer (set-buffer-modified-p nil))
              (kill-buffer buffer))))
    "##,
        expect![[
            r#"OK (:after-win (:prefix "Win" :exclusive no :candidates ("WinActivate" "WinActivateBottom" "WinActive" "WinClose" "WinExist" "WinGet" "WinGetActiveStats" "WinGetActiveTitle" "WinGetClass" "WinGetPos" "WinGetText" "WinGetTitle" "WinHide" "WinKill" "WinMaximize" "WinMenuSelectItem" "WinMinimize" "WinMinimizeAll" "WinMinimizeAllUndo" "WinMove" "WinRestore" "WinSet" "WinSetTitle" "WinShow" "WinWait" "WinWaitActive" "WinWaitClose" "WinWaitNotActive") :annotations (("WinActivate" "c") ("WinActivateBottom" "c") ("WinActive" "f") ("WinClose" "c") ("WinExist" "f") ("WinGet" "c") ("WinGetActiveStats" "c") ("WinGetActiveTitle" "c") ("WinGetClass" "c") ("WinGetPos" "c") ("WinGetText" "c") ("WinGetTitle" "c") ("WinHide" "c") ("WinKill" "c") ("WinMaximize" "c") ("WinMenuSelectItem" "c") ("WinMinimize" "c") ("WinMinimizeAll" "c") ("WinMinimizeAllUndo" "c") ("WinMove" "c") ("WinRestore" "c") ("WinSet" "c") ("WinSetTitle" "c") ("WinShow" "c") ("WinWait" "c") ("WinWaitActive" "c") ("WinWaitClose" "c") ("WinWaitNotActive" "c"))) :after-a-variable-prefix (:prefix "A_Scr" :exclusive no :candidates ("A_ScreenDPI" "A_ScreenHeight" "A_ScreenWidth" "A_ScriptDir" "A_ScriptFullPath" "A_ScriptHwnd" "A_ScriptName") :annotations (("A_ScreenDPI" "v") ("A_ScreenHeight" "v") ("A_ScreenWidth" "v") ("A_ScriptDir" "v") ("A_ScriptFullPath" "v") ("A_ScriptHwnd" "v") ("A_ScriptName" "v"))) :case-insensitive (:prefix "msgb" :exclusive no :candidates nil :annotations nil) :no-match (:prefix "zzzznotakeyword" :exclusive no :candidates nil :annotations nil))"#
        ]],
    )
}

fn imenu_indexes_functions_labels_hotkeys_and_hotstrings() -> ParityBatchCase {
    ParityBatchCase::value(
        "imenu_indexes_functions_labels_hotkeys_and_hotstrings",
        r##"
        ;; `M-x imenu' is how a user navigates an AutoHotkey script, and the
        ;; mode indexes five separate kinds of entry.  The fixture carries one
        ;; of each - two function definitions, a `^!w' hotkey, a `::btw::'
        ;; hotstring, a `ReportLabel:' label and a `;imenu' marker comment - so
        ;; the index is asserted whole rather than one kind at a time, and the
        ;; positions are real buffer positions the user would be taken to.
        ;;
        ;; The two functions differ only in brace style, and that is the point:
        ;; the Functions pattern requires the opening brace on its own line, so
        ;; `FormatWidget' is indexed and `CountWidgets', whose brace trails the
        ;; signature, is not.  With one style only, an empty Functions group and
        ;; a working one would look the same.
        (let ((buffer nil))
          (unwind-protect
              (progn
                (setq buffer (ahk-test-visit (concat ahk-test-script ";imenu End of script\n")))
                (list
                 :generic-expression imenu-generic-expression
                 :sorted-by-position (eq imenu-sort-function 'imenu--sort-by-position)
                 ;; The index mixes group nodes with the leaf `*Rescan*'
                 ;; entry imenu adds itself, so both shapes are rendered.
                 :index (mapcar
                         (lambda (node)
                           (if (listp (cdr node))
                               (cons (ahk-test-copy (car node))
                                     (mapcar (lambda (entry)
                                               (list (ahk-test-copy (car entry))
                                                     (cdr entry)))
                                             (cdr node)))
                             (list (ahk-test-copy (car node)) (cdr node))))
                         (let ((imenu-use-markers nil)) (imenu--make-index-alist t)))))
            (when (buffer-live-p buffer) (kill-buffer buffer))))
    "##,
        expect![[
            r#"OK (:generic-expression (("Functions" "^[ \11]*\\([^ ]+\\)(.*)[\n]{" 1) ("Labels" "^[ \11]*\\([^:;]+\\):\n" 1) ("Keybindings" "^[ \11]*\\([^;: \11\15\n\13\f].*?\\)::" 1) ("Hotstrings" "^[ \11]*\\(:.*?:.*?::\\)" 1) ("Comments" "^;imenu \\(.+\\)" 1)) :sorted-by-position t :index (("*Rescan*" -99) ("Comments" ("End of script" 631)) ("Hotstrings" ("::btw::" 490)) ("Keybindings" ("^!w" 328)) ("Labels" ("ReportLabel" 509)) ("Functions" ("FormatWidget" 284))))"#
        ]],
    )
}

fn running_a_script_and_opening_local_help_are_windows_only() -> ParityBatchCase {
    ParityBatchCase::value(
        "running_a_script_and_opening_local_help_are_windows_only",
        r##"
        ;; Three commands leave the editor.  `ahk-lookup-web' is portable: it
        ;; builds a documentation URL from the symbol at point and hands it to
        ;; `browse-url', which the workflow captures rather than launching a
        ;; browser.  The other two are not: `ahk-run-script' and
        ;; `ahk-lookup-chm' call `w32-shell-execute', which exists only on
        ;; Windows, and the help lookup first searches two hard-coded
        ;; `c:/Program Files' paths.  Nothing is faked here - the workflow runs
        ;; them as they are and records what a user on this platform gets, which
        ;; for the help lookup is a message telling them to set `ahk-path' and
        ;; for running a script is a void function.
        (let ((buffer nil) (visited nil))
          (unwind-protect
              (progn
                (setq buffer (ahk-test-visit))
                (list
                 :web-lookup
                 (let ((browse-url-browser-function
                        (lambda (url &rest _) (push url visited))))
                   (goto-char (point-min))
                   (search-forward "MsgBox")
                   (goto-char (match-beginning 0))
                   (execute-kbd-macro (kbd "C-c C-?"))
                   (mapcar #'ahk-test-copy visited))
                 :chm-lookup
                 (progn (goto-char (point-min))
                        (search-forward "MsgBox")
                        (goto-char (match-beginning 0))
                        (condition-case error
                            (progn (execute-kbd-macro (kbd "C-c C-r")) :no-signal)
                          (error (list :signal (car error) :data (cdr error)))))
                 :chm-message (ahk-test-messages "^Help file could not be found.*$")
                 :run-script
                 (condition-case error
                     (progn (execute-kbd-macro (kbd "C-c C-k")) :no-signal)
                   (error (list :signal (car error) :data (cdr error))))
                 :w32-available (fboundp 'w32-shell-execute)
                 :version (progn (ahk-version)
                                 (ahk-test-messages "^ahk-mode version .*$"))))
            (when (buffer-live-p buffer) (kill-buffer buffer))))
    "##,
        expect![[
            r#"OK (:web-lookup (#("http://ahkscript.org/docs/commands/MsgBox.htm" 35 41 (face font-lock-keyword-face))) :chm-lookup :no-signal :chm-message ("Help file could not be found, set ahk-path variable.") :run-script (:signal void-function :data (w32-shell-execute)) :w32-available nil :version ("ahk-mode version 1.5.6"))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        opening_an_ahk_script_selects_the_mode_and_sets_up_its_syntax(),
        a_realistic_script_is_highlighted_by_kind(),
        indenting_a_script_follows_its_blocks_and_honours_the_width(),
        commenting_offers_both_line_and_block_notation(),
        completion_offers_keywords_and_annotates_them_by_kind(),
        imenu_indexes_functions_labels_hotkeys_and_hotstrings(),
        running_a_script_and_opening_local_help_are_windows_only(),
    ]
}
