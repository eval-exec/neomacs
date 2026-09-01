use expect_test::expect;

use super::ParityBatchCase;

fn ace_jump_buffer_lists_every_buffer_and_one_avy_key_switches_to_it() -> ParityBatchCase {
    ParityBatchCase::value(
        "ace_jump_buffer_lists_every_buffer_and_one_avy_key_switches_to_it",
        r#"
    ;; The package's whole story: four working buffers are open, the user
    ;; presses the `ace-jump-buffer' binding, gets a bare list of every buffer
    ;; with one avy label per line, and a single key lands in the buffer on that
    ;; line.  Here `f' is the fourth line, "project plan.md".
    (ajb-test-with-workspace
      (let ((before (ajb-test-windows))
            snapshot)
        (let ((avy-translate-char-function
               (lambda (char) (setq snapshot (ajb-test-menu-snapshot)) char)))
          (execute-kbd-macro (vconcat (kbd "C-c j") (kbd "f"))))
        (list before
              (plist-get snapshot :mode)
              (plist-get snapshot :text)
              (plist-get snapshot :point)
              (plist-get snapshot :window-buffer)
              (plist-get snapshot :header-lines)
              (plist-get snapshot :max-height)
              (ajb-test-labels snapshot)
              (plist-get snapshot :overlays)
              (buffer-name (current-buffer))
              (buffer-name (window-buffer (selected-window)))
              (ajb-test-windows)
              (point)
              (and (get-buffer-window "*buffer-selection*") t)
              (ajb-test-visible-buffers))))
"#,
        expect![[
            r#"OK (("notes.org") bs-mode "    *scratch*      \n    *Messages*     \n  . notes.org      \n    project plan.md\n    server.py      \n    résumé.tex     " 1 "*buffer-selection*" 0 20 ((1 . "a") (2 . "s") (3 . "d") (4 . "f") (5 . "g") (6 . "h")) ((1 1 2 #("a" 0 1 (face avy-lead-face)) "*buffer-selection*") (2 21 22 #("s" 0 1 (face avy-lead-face)) "*buffer-selection*") (3 41 42 #("d" 0 1 (face avy-lead-face)) "*buffer-selection*") (4 61 62 #("f" 0 1 (face avy-lead-face)) "*buffer-selection*") (5 81 82 #("g" 0 1 (face avy-lead-face)) "*buffer-selection*") (6 101 102 #("h" 0 1 (face avy-lead-face)) "*buffer-selection*")) "project plan.md" "project plan.md" ("project plan.md") 33 nil ("project plan.md" "notes.org" "*scratch*" "*Messages*" "server.py" "résumé.tex" "*buffer-selection*"))"#
        ]],
    )
}

fn ace_jump_buffer_other_window_opens_the_target_beside_the_original_buffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "ace_jump_buffer_other_window_opens_the_target_beside_the_original_buffer",
        r#"
    ;; `ace-jump-buffer-other-window' must split the frame instead of replacing
    ;; the current buffer: the target becomes the selected window and the buffer
    ;; the user came from stays visible next to it.  `h' is the sixth line,
    ;; "résumé.tex".
    (ajb-test-with-workspace
      (let ((before (ajb-test-windows))
            snapshot)
        (let ((avy-translate-char-function
               (lambda (char) (setq snapshot (ajb-test-menu-snapshot)) char)))
          (execute-kbd-macro (vconcat (kbd "C-c o") (kbd "h"))))
        (list before
              (plist-get snapshot :text)
              (ajb-test-labels snapshot)
              (buffer-name (current-buffer))
              (buffer-name (window-buffer (selected-window)))
              (ajb-test-windows)
              (length (window-list nil 'never))
              ajb/other-window
              (and (get-buffer-window "*buffer-selection*") t))))
"#,
        expect![[
            r#"OK (("notes.org") "    *scratch*      \n    *Messages*     \n  . notes.org      \n    project plan.md\n    server.py      \n    résumé.tex     " ((1 . "a") (2 . "s") (3 . "d") (4 . "f") (5 . "g") (6 . "h")) "résumé.tex" "résumé.tex" ("résumé.tex" "notes.org") 2 nil nil)"#
        ]],
    )
}

fn ace_jump_buffer_in_one_window_collapses_a_split_onto_the_target_buffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "ace_jump_buffer_in_one_window_collapses_a_split_onto_the_target_buffer",
        r#"
    ;; `ace-jump-buffer-in-one-window' is the opposite variant: the user is
    ;; looking at a two-window layout and wants the target buffer alone.
    (ajb-test-with-workspace
      (let ((other (split-window-below)))
        (set-window-buffer other (get-buffer "server.py"))
        (let ((before (ajb-test-windows))
              snapshot)
          (let ((avy-translate-char-function
                 (lambda (char) (setq snapshot (ajb-test-menu-snapshot)) char)))
            (execute-kbd-macro (vconcat (kbd "C-c 1") (kbd "h"))))
          (list before
                (plist-get snapshot :text)
                (plist-get snapshot :window-buffer)
                (ajb-test-labels snapshot)
                (buffer-name (current-buffer))
                (buffer-name (window-buffer (selected-window)))
                (ajb-test-windows)
                (length (window-list nil 'never))
                ajb/in-one-window))))
"#,
        expect![[
            r#"OK (("notes.org" "server.py") "    *scratch*      \n    *Messages*     \n  . notes.org      \n    project plan.md\n    server.py      \n    résumé.tex     " "*buffer-selection*" ((1 . "a") (2 . "s") (3 . "d") (4 . "f") (5 . "g") (6 . "h")) "résumé.tex" "résumé.tex" ("résumé.tex") 1 nil)"#
        ]],
    )
    .fresh_process()
}

fn make_ace_jump_buffer_function_builds_filtered_jump_commands() -> ParityBatchCase {
    ParityBatchCase::value(
        "make_ace_jump_buffer_function_builds_filtered_jump_commands",
        r#"
    ;; `make-ace-jump-buffer-function' is the documented way to get a filtered
    ;; jump command.  A "prose" filter keeps only .org/.md buffers; the bundled
    ;; "same-mode" command keeps only buffers sharing the caller's major mode;
    ;; and a filter that rejects everything still leaves bs' own current-buffer
    ;; entry, which avy jumps to without asking for a key at all.
    (ajb-test-with-workspace
      (make-ace-jump-buffer-function "prose"
        (with-current-buffer buffer
          (not (string-match-p "\\.\\(org\\|md\\)\\'" (buffer-name)))))
      (make-ace-jump-buffer-function "nothing" t)
      (global-set-key (kbd "C-c p") #'ace-jump-prose-buffers)
      (global-set-key (kbd "C-c m") #'ace-jump-same-mode-buffers)
      (global-set-key (kbd "C-c n") #'ace-jump-nothing-buffers)
      (let (prose same nothing landed-prose landed-same)
        (let ((avy-translate-char-function
               (lambda (char) (setq prose (ajb-test-menu-snapshot)) char)))
          (execute-kbd-macro (vconcat (kbd "C-c p") (kbd "s"))))
        (setq landed-prose (list (buffer-name (current-buffer)) major-mode))
        (let ((avy-translate-char-function
               (lambda (char) (setq same (ajb-test-menu-snapshot)) char)))
          (execute-kbd-macro (vconcat (kbd "C-c m") (kbd "d"))))
        (setq landed-same (list (buffer-name (current-buffer)) major-mode))
        (let ((avy-translate-char-function
               (lambda (char) (setq nothing (ajb-test-menu-snapshot)) char)))
          (execute-kbd-macro (kbd "C-c n")))
        (list (assoc "prose" bs-configurations)
              (assoc "nothing" bs-configurations)
              (assoc "same-mode" bs-configurations)
              (list (commandp 'ace-jump-prose-buffers)
                    (commandp 'ace-jump-nothing-buffers)
                    (commandp 'ace-jump-same-mode-buffers))
              (plist-get prose :text)
              (ajb-test-labels prose)
              landed-prose
              (plist-get same :text)
              (ajb-test-labels same)
              landed-same
              nothing
              (buffer-name (current-buffer))
              (ajb-test-windows)
              ajb-bs-configuration)))
"#,
        expect![[
            r#"OK (("prose" nil nil nil ajb/filter-prose-buffers . #1=(nil)) ("nothing" nil nil nil ajb/filter-nothing-buffers . #1#) ("same-mode" nil nil nil ajb/filter-same-mode-buffers . #1#) (t t t) "  . notes.org      \n    project plan.md" ((1 . "a") (2 . "s")) ("project plan.md" text-mode) "  . project plan.md\n    notes.org      \n    résumé.tex     " ((1 . "a") (2 . "s") (3 . "d")) ("résumé.tex" text-mode) nil "résumé.tex" ("résumé.tex") "all")"#
        ]],
    )
    .fresh_process()
}

fn ace_jump_buffer_abort_leaves_the_buffers_and_the_layout_untouched() -> ParityBatchCase {
    ParityBatchCase::value(
        "ace_jump_buffer_abort_leaves_the_buffers_and_the_layout_untouched",
        r#"
    ;; Pressing a key that avy has not assigned, or C-g, or RET, must tear the
    ;; menu down and leave the user exactly where they were.  All three go
    ;; through `ajb/exit', which kills the bs window and the menu buffer.
    (ajb-test-with-workspace
      (cl-flet ((abort-with (keys)
                  (let (snapshot)
                    (let ((avy-translate-char-function
                           (lambda (char) (setq snapshot (ajb-test-menu-snapshot)) char)))
                      (execute-kbd-macro (vconcat (kbd "C-c j") keys)))
                    (list (plist-get snapshot :window-buffer)
                          (ajb-test-labels snapshot)
                          (buffer-name (current-buffer))
                          (buffer-name (window-buffer (selected-window)))
                          (ajb-test-windows)
                          (point)
                          (and (get-buffer "*buffer-selection*") t)))))
        (list (abort-with (kbd "z"))
              (abort-with (kbd "C-g"))
              (abort-with (kbd "RET"))
              (buffer-name (current-buffer))
              (ajb-test-visible-buffers))))
"#,
        expect![[
            r#"OK (("*buffer-selection*" ((1 . "a") (2 . "s") (3 . "d") (4 . "f") (5 . "g") (6 . "h")) "notes.org" "notes.org" ("notes.org") 39 nil) ("*buffer-selection*" ((1 . "a") (2 . "s") (3 . "d") (4 . "f") (5 . "g") (6 . "h")) "notes.org" "notes.org" ("notes.org") 39 nil) ("*buffer-selection*" ((1 . "a") (2 . "s") (3 . "d") (4 . "f") (5 . "g") (6 . "h")) "notes.org" "notes.org" ("notes.org") 39 nil) "notes.org" ("notes.org" "*scratch*" "*Messages*" "project plan.md" "server.py" "résumé.tex"))"#
        ]],
    )
    .fresh_process()
}

fn ace_jump_buffer_menu_drops_the_bs_header_and_honours_the_sort_option() -> ParityBatchCase {
    ParityBatchCase::value(
        "ace_jump_buffer_menu_drops_the_bs_header_and_honours_the_sort_option",
        r#"
    ;; Two of the package's advices are only visible by comparison: the menu it
    ;; shows has no `bs' header and only four narrow columns, while the very same
    ;; buffer list shown by plain `bs-show' keeps the header and every column.
    ;; `ajb-sort-function' then reorders the menu, which also reassigns the avy
    ;; labels, and `ajb-max-window-height' is installed buffer locally.
    (ajb-test-with-workspace
      (let ((bs-default-configuration "all")
            plain ace)
        (bs-show nil)
        (setq plain (with-current-buffer "*buffer-selection*"
                      (list (buffer-substring-no-properties (point-min) (point-max))
                            bs-header-lines-length
                            (length (default-value 'bs-attributes-list)))))
        (bs-kill)
        (when (get-buffer "*buffer-selection*") (kill-buffer "*buffer-selection*"))
        (let ((ajb-sort-function 'bs--sort-by-name)
              (ajb-max-window-height 3))
          (let ((avy-translate-char-function
                 (lambda (char) (setq ace (ajb-test-menu-snapshot)) char)))
            (execute-kbd-macro (vconcat (kbd "C-c j") (kbd "g")))))
        (list plain
              (plist-get ace :text)
              (plist-get ace :header-lines)
              (plist-get ace :max-height)
              (plist-get ace :sort)
              (ajb-test-labels ace)
              ajb/bs-attributes-list
              (buffer-name (current-buffer))
              bs-buffer-sort-function)))
"#,
        expect![[
            r#"OK ((" MR Buffer              Size         Mode  File          \n -- ------              ----         ----  ----          \n    *scratch*              0                             \n *% *Messages*             0                             \n.*  notes.org             38                             \n *  project plan.md       32                             \n *  server.py             25                             \n *  résumé.tex            24                             " 2 11) "    *Messages*     \n    *scratch*      \n  . notes.org      \n    project plan.md\n    résumé.tex     \n    server.py      " 0 3 bs--sort-by-name ((1 . "a") (2 . "s") (3 . "d") (4 . "f") (5 . "g") (6 . "h")) (("" 2 2 left " ") ("" 1 1 left bs--get-marked-string) ("" 1 1 left " ") ("Buffer" bs--get-name-length 10 left bs--get-name)) "résumé.tex" bs--sort-by-name)"#
        ]],
    )
    .fresh_process()
}

fn ace_jump_buffer_with_configuration_offers_every_registered_configuration() -> ParityBatchCase {
    ParityBatchCase::value(
        "ace_jump_buffer_with_configuration_offers_every_registered_configuration",
        r#"
    ;; `ace-jump-buffer-with-configuration' asks which bs configuration to use.
    ;; The minibuffer is the one boundary faked here: `completing-read' records
    ;; its arguments and answers "prose"; everything after it - the bs menu, the
    ;; avy overlays, the key and the jump - is real.  The stand-in deliberately
    ;; does not maintain `ajb/configuration-history', so the history variable
    ;; still holds exactly what the caller seeded it with.
    (ajb-test-with-workspace
      (make-ace-jump-buffer-function "prose"
        (with-current-buffer buffer
          (not (string-match-p "\\.\\(org\\|md\\)\\'" (buffer-name)))))
      (let ((ajb/configuration-history '("same-mode" "all"))
            prompt collection require-match history default snapshot)
        (cl-letf (((symbol-function 'completing-read)
                   (lambda (p c &optional _predicate rm _initial hist def &rest _)
                     (setq prompt p collection c require-match rm
                           history hist default def)
                     "prose")))
          (let ((avy-translate-char-function
                 (lambda (char) (setq snapshot (ajb-test-menu-snapshot)) char)))
            (execute-kbd-macro (vconcat (kbd "C-c c") (kbd "s")))))
        (list prompt
              collection
              require-match
              history
              default
              (plist-get snapshot :text)
              (ajb-test-labels snapshot)
              (buffer-name (current-buffer))
              ajb-bs-configuration
              ajb/configuration-history)))
"#,
        expect![[
            r#"OK ("Ace jump buffer with configuration: " ("prose" "same-mode" "all" "files" "files-and-scratch" "all-intern-last") t ajb/configuration-history "same-mode" "  . notes.org      \n    project plan.md" ((1 . "a") (2 . "s")) "project plan.md" "all" ("same-mode" "all"))"#
        ]],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        ace_jump_buffer_lists_every_buffer_and_one_avy_key_switches_to_it(),
        ace_jump_buffer_other_window_opens_the_target_beside_the_original_buffer(),
        ace_jump_buffer_in_one_window_collapses_a_split_onto_the_target_buffer(),
        make_ace_jump_buffer_function_builds_filtered_jump_commands(),
        ace_jump_buffer_abort_leaves_the_buffers_and_the_layout_untouched(),
        ace_jump_buffer_menu_drops_the_bs_header_and_honours_the_sort_option(),
        ace_jump_buffer_with_configuration_offers_every_registered_configuration(),
    ]
}
