use expect_test::expect;

use super::ParityBatchCase;

fn edits_embedded_lua_in_a_real_auctex_document_and_commits_through_save() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "auctex-lua-edit-save"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (source (expand-file-name "release notes Ω.tex" root))
       (original
        (concat
         "\\documentclass{article}\n"
         "\\usepackage{luacode}\n"
         "\\begin{document}\n"
         "Before release.\n"
         "\\begin{luacode}\n"
         "local greeting = \"old Ω\"\n"
         "for index = 1, 2 do\n"
         "  tex.print(greeting .. index)\n"
         "end\n"
         "\\end{luacode}\n"
         "After release.\n"
         "\\end{document}\n"))
       (default-directory root)
       parent editor messages-start point-before initial-state editor-narrowed result)
  (unwind-protect
      (progn
        (neomacs-auctex-lua-test--cleanup root)
        (make-directory root t)
        (with-temp-file source (insert original))
        (setq parent (find-file-noselect source))
        (switch-to-buffer parent)
        (delete-other-windows)
        (LaTeX-mode)
        (goto-char (point-min))
        (search-forward "local greeting")
        (setq point-before (point))
        (setq messages-start
              (with-current-buffer (messages-buffer) (point-max)))
        (call-interactively #'LaTeX-edit-Lua-code-start)
        (setq editor (current-buffer))
        (setq initial-state
              (list
               :parent (buffer-name parent)
               :editor (buffer-name editor)
               :mode major-mode
               :prog-mode (derived-mode-p 'prog-mode)
               :save-remap (command-remapping #'save-buffer)
               :windows (length (window-list))
               :window-buffers
               (mapcar
                (lambda (window) (buffer-name (window-buffer window)))
                (window-list))
               :stored-parent
               (buffer-name LaTeX-edit-Lua-code-parent-buffer)
               :stored-point LaTeX-edit-Lua-code-parent-buffer-point
               :initial-text
               (buffer-substring-no-properties (point-min) (point-max))))
        (erase-buffer)
        (insert
         "\nlocal greeting = \"release Ω\"\n"
         "for index = 1, 3 do\n"
         "tex.print(greeting .. \":\" .. index)\n"
         "end\n\n")
        (indent-region (point-min) (point-max))
        (font-lock-ensure)
        (let ((edited-text
               (buffer-substring-no-properties (point-min) (point-max)))
              (editor-modified (buffer-modified-p))
              (lua-properties
               (neomacs-auctex-lua-test--token-properties "release Ω"))
              (finish-command (command-remapping #'save-buffer)))
          (narrow-to-region (1+ (point-min)) (1- (point-max)))
          (setq editor-narrowed (buffer-narrowed-p))
          (call-interactively finish-command)
          (let ((parent-text
                 (buffer-substring-no-properties (point-min) (point-max)))
                (parent-modified-before-save (buffer-modified-p))
                (disk-before-save
                 (neomacs-auctex-lua-test--file-text source)))
            (call-interactively #'save-buffer)
            (setq result
                  (list
                   :initial initial-state
                   :edit
                   (list
                    :text edited-text
                    :modified editor-modified
                    :narrowed-before-save editor-narrowed
                    :properties lua-properties)
                   :finish
                   (list
                    :current (buffer-name)
                    :editor-live (buffer-live-p editor)
                    :windows (length (window-list))
                    :window-buffers
                    (mapcar
                     (lambda (window) (buffer-name (window-buffer window)))
                     (window-list))
                    :point (point)
                    :point-restored (= (point) point-before)
                    :environment (LaTeX-current-environment)
                    :parent-modified-before-save parent-modified-before-save
                    :parent-modified-after-save (buffer-modified-p)
                    :buffer parent-text
                    :copied-properties
                    (neomacs-auctex-lua-test--token-properties "release Ω")
                    :disk-before-save disk-before-save
                    :disk-after-save
                    (neomacs-auctex-lua-test--file-text source))
                   :messages
                   (neomacs-auctex-lua-test--messages
                    messages-start root))))))
    (neomacs-auctex-lua-test--cleanup root))
  result)
"####;
    let expect = expect![[
        r#"OK (:initial (:parent "release notes Ω.tex" :editor "*release notes Ω.tex [Lua]*" :mode lua-mode :prog-mode prog-mode :save-remap LaTeX-edit-Lua-code-finish :windows 2 :window-buffers ("*release notes Ω.tex [Lua]*" "release notes Ω.tex") :stored-parent "release notes Ω.tex" :stored-point 109 :initial-text "\nlocal greeting = \"old Ω\"\nfor index = 1, 2 do\n  tex.print(greeting .. index)\nend\n") :edit (:text "\nlocal greeting = \"release Ω\"\nfor index = 1, 3 do\n   tex.print(greeting .. \":\" .. index)\nend\n\n" :modified t :narrowed-before-save t :properties (:face font-lock-string-face :font-lock-face nil :syntax-table nil)) :finish (:current "release notes Ω.tex" :editor-live nil :windows 1 :window-buffers ("release notes Ω.tex") :point 109 :point-restored t :environment "luacode" :parent-modified-before-save t :parent-modified-after-save nil :buffer "\\documentclass{article}\n\\usepackage{luacode}\n\\begin{document}\nBefore release.\n\\begin{luacode}\nlocal greeting = \"release Ω\"\nfor index = 1, 3 do\n   tex.print(greeting .. \":\" .. index)\nend\n\n\\end{luacode}\nAfter release.\n\\end{document}\n" :copied-properties (:face font-lock-string-face :font-lock-face nil :syntax-table nil) :disk-before-save "\\documentclass{article}\n\\usepackage{luacode}\n\\begin{document}\nBefore release.\n\\begin{luacode}\nlocal greeting = \"old \316\251\"\nfor index = 1, 2 do\n  tex.print(greeting .. index)\nend\n\\end{luacode}\nAfter release.\n\\end{document}\n" :disk-after-save "\\documentclass{article}\n\\usepackage{luacode}\n\\begin{document}\nBefore release.\n\\begin{luacode}\nlocal greeting = \"release \316\251\"\nfor index = 1, 3 do\n   tex.print(greeting .. \":\" .. index)\nend\n\n\\end{luacode}\nAfter release.\n\\end{document}\n") :messages ("Mark set" "Indenting region...done" "Mark set"))"#
    ]];
    ParityBatchCase::value(
        "edits_embedded_lua_in_a_real_auctex_document_and_commits_through_save",
        elisp_form,
        expect,
    )
    .fresh_process()
}

fn custom_environment_edits_only_the_selected_lua_block() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "auctex-lua-custom-environment"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (source (expand-file-name "custom.tex" root))
       (original
        (concat
         "\\documentclass{article}\n"
         "\\begin{document}\n"
         "\\begin{luacode}\n"
         "local untouched = {status = \"stable\", count = 1}\n"
         "tex.print(untouched.status)\n"
         "\\end{luacode}\n"
         "Ordinary prose stays unchanged.\n"
         "\\begin{luafunction}\n"
         "local payload = {status = \"draft\", count = 2}\n"
         "return payload\n"
         "\\end{luafunction}\n"
         "Trailing prose stays unchanged.\n"
         "\\end{document}\n"))
       (default-directory root)
       parent editor custom-start result)
  (unwind-protect
      (progn
        (neomacs-auctex-lua-test--cleanup root)
        (make-directory root t)
        (with-temp-file source (insert original))
        (setq parent (find-file-noselect source))
        (switch-to-buffer parent)
        (delete-other-windows)
        (LaTeX-mode)
        (let ((LaTeX-Lua-environments '("luafunction")))
          (goto-char (point-min))
          (search-forward "local payload")
          (setq custom-start
                (with-current-buffer (messages-buffer) (point-max)))
          (call-interactively #'LaTeX-edit-Lua-code-start)
          (setq editor (current-buffer))
          (let ((initial-text
                 (buffer-substring-no-properties (point-min) (point-max))))
            (erase-buffer)
            (insert
             "\nlocal payload = {status = \"released Ω\", count = 3}\n"
             "payload.tags = {\"stable\", \"documented\"}\n"
             "return payload\n\n")
            (call-interactively (command-remapping #'save-buffer))
            (setq result
                  (list
                   :initial-text initial-text
                   :current (buffer-name)
                   :editor-live (buffer-live-p editor)
                   :windows (length (window-list))
                   :environment (LaTeX-current-environment)
                   :buffer (buffer-substring-no-properties
                            (point-min) (point-max))
                   :disk (neomacs-auctex-lua-test--file-text source)
                   :messages
                   (neomacs-auctex-lua-test--messages
                    custom-start root))))))
    (neomacs-auctex-lua-test--cleanup root))
  result)
"####;
    let expect = expect![[
        r#"OK (:initial-text "\nlocal payload = {status = \"draft\", count = 2}\nreturn payload\n" :current "custom.tex" :editor-live nil :windows 1 :environment "luafunction" :buffer "\\documentclass{article}\n\\begin{document}\n\\begin{luacode}\nlocal untouched = {status = \"stable\", count = 1}\ntex.print(untouched.status)\n\\end{luacode}\nOrdinary prose stays unchanged.\n\\begin{luafunction}\nlocal payload = {status = \"released Ω\", count = 3}\npayload.tags = {\"stable\", \"documented\"}\nreturn payload\n\n\\end{luafunction}\nTrailing prose stays unchanged.\n\\end{document}\n" :disk "\\documentclass{article}\n\\begin{document}\n\\begin{luacode}\nlocal untouched = {status = \"stable\", count = 1}\ntex.print(untouched.status)\n\\end{luacode}\nOrdinary prose stays unchanged.\n\\begin{luafunction}\nlocal payload = {status = \"draft\", count = 2}\nreturn payload\n\\end{luafunction}\nTrailing prose stays unchanged.\n\\end{document}\n" :messages ("Mark set [2 times]"))"#
    ]];
    ParityBatchCase::value(
        "custom_environment_edits_only_the_selected_lua_block",
        elisp_form,
        expect,
    )
    .fresh_process()
}

fn abandoning_an_edit_preserves_the_document_and_reopen_starts_from_source() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "auctex-lua-abandon-reopen"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (source (expand-file-name "retry.tex" root))
       (original
        (concat
         "\\documentclass{article}\n"
         "\\begin{document}\n"
         "\\begin{luacode*}\n"
         "local stage = \"original\"\n"
         "tex.print(stage)\n"
         "\\end{luacode*}\n"
         "\\end{document}\n"))
       (default-directory root)
       parent abandoned reopened point-before after-abandon result)
  (unwind-protect
      (progn
        (neomacs-auctex-lua-test--cleanup root)
        (make-directory root t)
        (with-temp-file source (insert original))
        (setq parent (find-file-noselect source))
        (switch-to-buffer parent)
        (delete-other-windows)
        (LaTeX-mode)
        (goto-char (point-min))
        (search-forward "local stage")
        (setq point-before (point))
        (call-interactively #'LaTeX-edit-Lua-code-start)
        (setq abandoned (current-buffer))
        (goto-char (point-max))
        (insert "tex.print(\"abandoned Ω\")\n")
        (set-buffer-modified-p nil)
        (kill-buffer-and-window)
        (switch-to-buffer parent)
        (setq after-abandon
              (list
               :current (buffer-name)
               :editor-live (buffer-live-p abandoned)
               :windows (length (window-list))
               :point (point)
               :point-restored (= (point) point-before)
               :modified (buffer-modified-p)
               :buffer (buffer-substring-no-properties
                        (point-min) (point-max))
               :disk (neomacs-auctex-lua-test--file-text source)))
        (goto-char (point-min))
        (search-forward "local stage")
        (call-interactively #'LaTeX-edit-Lua-code-start)
        (setq reopened (current-buffer))
        (let ((reopened-text
               (buffer-substring-no-properties (point-min) (point-max))))
          (erase-buffer)
          (insert
           "\nlocal stage = \"committed\"\n"
           "tex.print(stage .. \" Ω\")\n\n")
          (call-interactively (command-remapping #'save-buffer))
          (setq result
                (list
                 :after-abandon after-abandon
                 :reopen
                 (list
                  :initial-text reopened-text
                  :editor-live (buffer-live-p reopened)
                  :windows (length (window-list)))
                 :committed
                 (buffer-substring-no-properties (point-min) (point-max))))))
    (neomacs-auctex-lua-test--cleanup root))
  result)
"####;
    let expect = expect![[
        r#"OK (:after-abandon (:current "retry.tex" :editor-live nil :windows 1 :point 58 :point-restored nil :modified nil :buffer "\\documentclass{article}\n\\begin{document}\n\\begin{luacode*}\nlocal stage = \"original\"\ntex.print(stage)\n\\end{luacode*}\n\\end{document}\n" :disk "\\documentclass{article}\n\\begin{document}\n\\begin{luacode*}\nlocal stage = \"original\"\ntex.print(stage)\n\\end{luacode*}\n\\end{document}\n") :reopen (:initial-text "\nlocal stage = \"original\"\ntex.print(stage)\n" :editor-live nil :windows 1) :committed "\\documentclass{article}\n\\begin{document}\n\\begin{luacode*}\nlocal stage = \"committed\"\ntex.print(stage .. \" Ω\")\n\n\\end{luacode*}\n\\end{document}\n")"#
    ]];
    ParityBatchCase::value(
        "abandoning_an_edit_preserves_the_document_and_reopen_starts_from_source",
        elisp_form,
        expect,
    )
    .fresh_process()
}

fn invocation_outside_lua_environment_is_an_exact_noop() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "auctex-lua-outside-noop"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (source (expand-file-name "outside.tex" root))
       (default-directory root)
       parent messages-start before result)
  (unwind-protect
      (progn
        (neomacs-auctex-lua-test--cleanup root)
        (make-directory root t)
        (with-temp-file source
          (insert
           "\\documentclass{article}\n"
           "\\begin{document}\n"
           "Ordinary prose at point.\n"
           "\\begin{luacode}\n"
           "tex.print(\"untouched Ω\")\n"
           "\\end{luacode}\n"
           "\\end{document}\n"))
        (setq parent (find-file-noselect source))
        (switch-to-buffer parent)
        (delete-other-windows)
        (LaTeX-mode)
        (goto-char (point-min))
        (search-forward "Ordinary prose")
        (setq before
              (list
               :current (buffer-name)
               :mode major-mode
               :point (point)
               :mark (mark t)
               :windows
               (mapcar
                (lambda (window) (buffer-name (window-buffer window)))
                (window-list))
               :buffer (buffer-substring-no-properties
                        (point-min) (point-max))))
        (setq messages-start
              (with-current-buffer (messages-buffer) (point-max)))
        (call-interactively #'LaTeX-edit-Lua-code-start)
        (setq result
              (list
               :before before
               :after
               (list
                :current (buffer-name)
                :mode major-mode
                :point (point)
                :mark (mark t)
                :windows
                (mapcar
                 (lambda (window) (buffer-name (window-buffer window)))
                 (window-list))
                :editor
                (get-buffer (format "*%s [Lua]*" (buffer-name parent)))
                :buffer (buffer-substring-no-properties
                         (point-min) (point-max)))
               :messages
               (neomacs-auctex-lua-test--messages messages-start root))))
    (neomacs-auctex-lua-test--cleanup root))
  result)
"####;
    let expect = expect![[
        r#"OK (:before (:current "outside.tex" :mode LaTeX-mode :point 56 :mark nil :windows ("outside.tex") :buffer "\\documentclass{article}\n\\begin{document}\nOrdinary prose at point.\n\\begin{luacode}\ntex.print(\"untouched Ω\")\n\\end{luacode}\n\\end{document}\n") :after (:current "outside.tex" :mode LaTeX-mode :point 56 :mark nil :windows ("outside.tex") :editor nil :buffer "\\documentclass{article}\n\\begin{document}\nOrdinary prose at point.\n\\begin{luacode}\ntex.print(\"untouched Ω\")\n\\end{luacode}\n\\end{document}\n") :messages ("Not in a Lua code environment."))"#
    ]];
    ParityBatchCase::value(
        "invocation_outside_lua_environment_is_an_exact_noop",
        elisp_form,
        expect,
    )
    .fresh_process()
}

fn moving_the_parent_point_replaces_the_environment_selected_at_finish_time() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "auctex-lua-moved-parent"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (source (expand-file-name "two-blocks.tex" root))
       (original
        (concat
         "\\documentclass{article}\n"
         "\\begin{document}\n"
         "\\begin{luacode}\ntex.print('first')\n\\end{luacode}\n"
         "Middle prose remains.\n"
         "\\begin{luacode}\ntex.print('second')\n\\end{luacode}\n"
         "\\end{document}\n"))
       (default-directory root)
       parent editor editor-window parent-window point-before point-at-second result)
  (unwind-protect
      (progn
        (neomacs-auctex-lua-test--cleanup root)
        (make-directory root t)
        (with-temp-file source (insert original))
        (setq parent (find-file-noselect source))
        (switch-to-buffer parent)
        (delete-other-windows)
        (LaTeX-mode)
        (goto-char (point-min))
        (search-forward "first")
        (setq point-before (point))
        (call-interactively #'LaTeX-edit-Lua-code-start)
        (setq editor (current-buffer)
              editor-window (selected-window)
              parent-window (get-buffer-window parent))
        (erase-buffer)
        (insert "tex.print('replacement Ω')")
        (select-window parent-window)
        (goto-char (point-min))
        (search-forward "second")
        (setq point-at-second (point))
        (select-window editor-window)
        (call-interactively (command-remapping #'save-buffer))
        (setq result
              (list
               :point-before point-before
               :point-at-second point-at-second
               :point-after (point)
               :editor-live (buffer-live-p editor)
               :current (buffer-name)
               :windows (length (window-list))
               :buffer (buffer-substring-no-properties
                        (point-min) (point-max))
               :disk (neomacs-auctex-lua-test--file-text source))))
    (neomacs-auctex-lua-test--cleanup root))
  result)
"####;
    let expect = expect![[
        r#"OK (:point-before 74 :point-at-second 146 :point-after 74 :editor-live nil :current "two-blocks.tex" :windows 1 :buffer "\\documentclass{article}\n\\begin{document}\n\\begin{luacode}\ntex.print('first')\n\\end{luacode}\nMiddle prose remains.\n\\begin{luacode}tex.print('replacement Ω')\\end{luacode}\n\\end{document}\n" :disk "\\documentclass{article}\n\\begin{document}\n\\begin{luacode}\ntex.print('first')\n\\end{luacode}\nMiddle prose remains.\n\\begin{luacode}\ntex.print('second')\n\\end{luacode}\n\\end{document}\n")"#
    ]];
    ParityBatchCase::value(
        "moving_the_parent_point_replaces_the_environment_selected_at_finish_time",
        elisp_form,
        expect,
    )
    .fresh_process()
}

fn simultaneous_embedded_edits_follow_the_package_global_parent_state() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "auctex-lua-simultaneous"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (source-a (expand-file-name "chapter-a.tex" root))
       (source-b (expand-file-name "chapter-b.tex" root))
       (default-directory root)
       parent-a parent-b editor-a editor-b before result)
  (unwind-protect
      (progn
        (neomacs-auctex-lua-test--cleanup root)
        (make-directory root t)
        (with-temp-file source-a
          (insert "\\begin{luacode}\nreturn 'A-old'\n\\end{luacode}\n"))
        (with-temp-file source-b
          (insert "\\begin{luacode}\nreturn 'B-old'\n\\end{luacode}\n"))
        (setq parent-a (find-file-noselect source-a)
              parent-b (find-file-noselect source-b))
        (switch-to-buffer parent-a)
        (delete-other-windows)
        (LaTeX-mode)
        (goto-char (point-min))
        (search-forward "A-old")
        (call-interactively #'LaTeX-edit-Lua-code-start)
        (setq editor-a (current-buffer))
        (erase-buffer)
        (insert "return 'A-new Ω'")
        (switch-to-buffer parent-b)
        (LaTeX-mode)
        (goto-char (point-min))
        (search-forward "B-old")
        (call-interactively #'LaTeX-edit-Lua-code-start)
        (setq editor-b (current-buffer))
        (erase-buffer)
        (insert "return 'B-new'")
        (setq before
              (list
               :global-parent (buffer-name LaTeX-edit-Lua-code-parent-buffer)
               :global-point LaTeX-edit-Lua-code-parent-buffer-point
               :a-parent-local
               (with-current-buffer editor-a
                 (local-variable-p 'LaTeX-edit-Lua-code-parent-buffer))
               :b-parent-local
               (with-current-buffer editor-b
                 (local-variable-p 'LaTeX-edit-Lua-code-parent-buffer))
               :window-buffers
               (mapcar
                (lambda (window) (buffer-name (window-buffer window)))
                (window-list))))
        (switch-to-buffer editor-a)
        (call-interactively (command-remapping #'save-buffer))
        (setq result
              (list
               :before before
               :after
               (list
                :current (buffer-name)
                :a-editor-live (buffer-live-p editor-a)
                :b-editor-live (buffer-live-p editor-b)
                :a-buffer
                (with-current-buffer parent-a
                  (buffer-substring-no-properties (point-min) (point-max)))
                :b-buffer
                (with-current-buffer parent-b
                  (buffer-substring-no-properties (point-min) (point-max)))
                :a-disk (neomacs-auctex-lua-test--file-text source-a)
                :b-disk (neomacs-auctex-lua-test--file-text source-b)
                :window-buffers
                (mapcar
                 (lambda (window) (buffer-name (window-buffer window)))
                 (window-list))))))
    (neomacs-auctex-lua-test--cleanup root))
  result)
"####;
    let expect = expect![[
        r#"OK (:before (:global-parent "chapter-b.tex" :global-point 30 :a-parent-local nil :b-parent-local nil :window-buffers ("*chapter-b.tex [Lua]*" "chapter-b.tex")) :after (:current "chapter-b.tex" :a-editor-live nil :b-editor-live t :a-buffer "\\begin{luacode}\nreturn 'A-old'\n\\end{luacode}\n" :b-buffer "\\begin{luacode}return 'A-new Ω'\\end{luacode}\n" :a-disk "\\begin{luacode}\nreturn 'A-old'\n\\end{luacode}\n" :b-disk "\\begin{luacode}\nreturn 'B-old'\n\\end{luacode}\n" :window-buffers ("chapter-b.tex")))"#
    ]];
    ParityBatchCase::value(
        "simultaneous_embedded_edits_follow_the_package_global_parent_state",
        elisp_form,
        expect,
    )
    .fresh_process()
}

fn inherited_save_remap_affects_an_unrelated_lua_file_after_parent_close() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "auctex-lua-shared-save-map"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (tex-source (expand-file-name "embedded.tex" root))
       (lua-source (expand-file-name "ordinary.lua" root))
       (default-directory root)
       parent editor ordinary remap-before remap-after outcome result)
  (unwind-protect
      (progn
        (neomacs-auctex-lua-test--cleanup root)
        (make-directory root t)
        (with-temp-file tex-source
          (insert "\\begin{luacode}\nreturn 'embedded'\n\\end{luacode}\n"))
        (with-temp-file lua-source
          (insert "return 'ordinary disk'\n"))
        (with-temp-buffer
          (lua-mode)
          (setq remap-before (command-remapping #'save-buffer)))
        (setq parent (find-file-noselect tex-source))
        (switch-to-buffer parent)
        (delete-other-windows)
        (LaTeX-mode)
        (goto-char (point-min))
        (search-forward "embedded")
        (call-interactively #'LaTeX-edit-Lua-code-start)
        (setq editor (current-buffer))
        (call-interactively (command-remapping #'save-buffer))
        (with-current-buffer parent (set-buffer-modified-p nil))
        (kill-buffer parent)
        (setq ordinary (find-file-noselect lua-source))
        (switch-to-buffer ordinary)
        (delete-other-windows)
        (lua-mode)
        (goto-char (point-max))
        (insert "return 'ordinary unsaved Ω'\n")
        (setq remap-after (command-remapping #'save-buffer))
        (setq outcome
              (condition-case error-data
                  (progn
                    (call-interactively remap-after)
                    (list :ok t))
                (error
                 (list :error (car error-data) :data (cdr error-data)))))
        (setq result
              (list
               :remap-before remap-before
               :remap-after remap-after
               :parent-variable-bufferp
               (bufferp LaTeX-edit-Lua-code-parent-buffer)
               :parent-variable-live
               (buffer-live-p LaTeX-edit-Lua-code-parent-buffer)
               :ordinary-live (buffer-live-p ordinary)
               :embedded-editor-live (buffer-live-p editor)
               :outcome outcome
               :current (buffer-name)
               :window-buffers
               (mapcar
                (lambda (window) (buffer-name (window-buffer window)))
                (window-list))
               :ordinary-disk
               (neomacs-auctex-lua-test--file-text lua-source))))
    (neomacs-auctex-lua-test--cleanup root))
  result)
"####;
    let expect = expect![[
        r#"OK (:remap-before nil :remap-after LaTeX-edit-Lua-code-finish :parent-variable-bufferp t :parent-variable-live nil :ordinary-live nil :embedded-editor-live nil :outcome (:error error :data ("Attempt to delete minibuffer or sole ordinary window")) :current "*scratch*" :window-buffers ("*scratch*") :ordinary-disk "return 'ordinary disk'\n")"#
    ]];
    ParityBatchCase::value(
        "inherited_save_remap_affects_an_unrelated_lua_file_after_parent_close",
        elisp_form,
        expect,
    )
    .fresh_process()
}

fn reopening_an_active_edit_buffer_appends_a_second_source_copy() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "auctex-lua-reenter"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (source (expand-file-name "reenter.tex" root))
       (default-directory root)
       parent editor first-state result)
  (unwind-protect
      (progn
        (neomacs-auctex-lua-test--cleanup root)
        (make-directory root t)
        (with-temp-file source
          (insert "\\begin{luacode}\nfirst()\n\\end{luacode}\n"))
        (setq parent (find-file-noselect source))
        (switch-to-buffer parent)
        (delete-other-windows)
        (LaTeX-mode)
        (goto-char (point-min))
        (search-forward "first")
        (call-interactively #'LaTeX-edit-Lua-code-start)
        (setq editor (current-buffer))
        (goto-char (point-max))
        (insert "EDITED-BUT-UNSAVED\n")
        (setq first-state
              (buffer-substring-no-properties (point-min) (point-max)))
        (switch-to-buffer parent)
        (goto-char (point-min))
        (search-forward "first")
        (call-interactively #'LaTeX-edit-Lua-code-start)
        (setq result
              (list
               :same-editor (eq editor (current-buffer))
               :first-state first-state
               :reopened-state
               (buffer-substring-no-properties (point-min) (point-max))
               :parent
               (with-current-buffer parent
                 (buffer-substring-no-properties (point-min) (point-max)))
               :windows (length (window-list)))))
    (neomacs-auctex-lua-test--cleanup root))
  result)
"####;
    let expect = expect![[
        r#"OK (:same-editor t :first-state "\nfirst()\nEDITED-BUT-UNSAVED\n" :reopened-state "\nfirst()\nEDITED-BUT-UNSAVED\n\nfirst()\n" :parent "\\begin{luacode}\nfirst()\n\\end{luacode}\n" :windows 2)"#
    ]];
    ParityBatchCase::value(
        "reopening_an_active_edit_buffer_appends_a_second_source_copy",
        elisp_form,
        expect,
    )
    .fresh_process()
}

fn unterminated_lua_environment_reports_the_exact_auctex_error() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "auctex-lua-unterminated"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (source (expand-file-name "unterminated.tex" root))
       (default-directory root)
       parent editor-name editor outcome result)
  (unwind-protect
      (progn
        (neomacs-auctex-lua-test--cleanup root)
        (make-directory root t)
        (with-temp-file source
          (insert
           "\\documentclass{article}\n"
           "\\begin{document}\n"
           "\\begin{luacode}\n"
           "tex.print(\"missing end Ω\")\n"))
        (setq parent (find-file-noselect source))
        (switch-to-buffer parent)
        (delete-other-windows)
        (LaTeX-mode)
        (goto-char (point-min))
        (search-forward "missing end")
        (setq editor-name (format "*%s [Lua]*" (buffer-name parent)))
        (setq outcome
              (condition-case error-data
                  (progn
                    (call-interactively #'LaTeX-edit-Lua-code-start)
                    (list :ok t))
                (error
                 (list :error (car error-data) :data (cdr error-data)))))
        (setq editor (get-buffer editor-name))
        (setq result
              (list
               :outcome outcome
               :current (buffer-name)
               :point (point)
               :point-max (point-max)
               :windows (length (window-list))
               :editor-created (buffer-live-p editor)
               :editor-text
               (and editor
                    (with-current-buffer editor
                      (buffer-substring-no-properties
                       (point-min) (point-max))))
               :source
               (buffer-substring-no-properties (point-min) (point-max)))))
    (neomacs-auctex-lua-test--cleanup root))
  result)
"####;
    let expect = expect![[
        r#"OK (:outcome (:error error :data ("Can’t locate end of current environment")) :current "unterminated.tex" :point 80 :point-max 85 :windows 1 :editor-created t :editor-text "" :source "\\documentclass{article}\n\\begin{document}\n\\begin{luacode}\ntex.print(\"missing end Ω\")\n")"#
    ]];
    ParityBatchCase::value(
        "unterminated_lua_environment_reports_the_exact_auctex_error",
        elisp_form,
        expect,
    )
    .fresh_process()
}

pub(super) fn practical_workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        edits_embedded_lua_in_a_real_auctex_document_and_commits_through_save(),
        custom_environment_edits_only_the_selected_lua_block(),
        abandoning_an_edit_preserves_the_document_and_reopen_starts_from_source(),
        invocation_outside_lua_environment_is_an_exact_noop(),
        moving_the_parent_point_replaces_the_environment_selected_at_finish_time(),
        simultaneous_embedded_edits_follow_the_package_global_parent_state(),
        inherited_save_remap_affects_an_unrelated_lua_file_after_parent_close(),
        reopening_an_active_edit_buffer_appends_a_second_source_copy(),
        unterminated_lua_environment_reports_the_exact_auctex_error(),
    ]
}
