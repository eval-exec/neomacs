use expect_test::expect;

use super::ParityBatchCase;

fn anju_middle_truncate_handles_short_long_multiline_unicode_and_invalid_extents() -> ParityBatchCase
{
    ParityBatchCase::value(
        "anju_middle_truncate_handles_short_long_multiline_unicode_and_invalid_extents",
        r##"(mapcar
         (lambda (arguments)
           (catch
               'anju-middle-truncate-exception
             (apply #'anju-middle-truncate arguments)))
         '(("short phrase" "Open")
           ("abcdefghijklmnopqrstuvwxyz0123456789" "Open")
           ("abcdefghijklmnopqrstuvwxyz0123456789" "Open" 20 7)
           ("first line\nmiddle line\nlast line" "Select")
           ("                                  \nbody\n" "Select")
           ("한글과 emoji 🧭 make this label deliberately long" "Go" 24 8)
           ("invalid" "Open" 12 5)))"##,
        expect![[
            r#"OK ("Open “short phrase”" "Open “abcdefghijkl…yz0123456789”" "Open “abcdefg…3456789”" "Select “first line…last line”" "Select “␣…␤”" "Go “한글과 emoj…ely long”" "ERROR: extent (5) and max (12) should conform to extent <= (max/2) - 2")"#
        ]],
    )
}

fn anju_menu_label_uses_the_real_active_region_and_strips_properties() -> ParityBatchCase {
    ParityBatchCase::value(
        "anju_menu_label_uses_the_real_active_region_and_strips_properties",
        r##"(with-temp-buffer
         (insert
          (propertize
           "prefix alpha beta gamma delta epsilon suffix"
           'face 'bold))
         (goto-char 8)
         (set-mark 39)
         (activate-mark)
         (list
          (buffer-substring (region-beginning) (region-end))
          (anju-menu-label "Occur" 22 8)
          (text-properties-at 1)
          (text-properties-at 9)))"##,
        expect![[
            r#"OK (#("alpha beta gamma delta epsilon " 0 31 (face bold)) "Occur “alpha be…epsilon ”" #1=(face bold) #1#)"#
        ]],
    )
}

fn anju_filename_extraction_preserves_real_world_names() -> ParityBatchCase {
    ParityBatchCase::value(
        "anju_filename_extraction_preserves_real_world_names",
        r##"(mapcar
         #'anju-filename-from-path
         '("/workspace/src/lib.rs"
           "/workspace/archive.tar.gz"
           "/workspace/.gitignore"
           "/workspace/trailing."
           "/workspace/no-extension"
           "relative/path/report.final.md"
           "/한글/문서.txt"))"##,
        expect![[
            r#"OK ("lib.rs" "archive.tar.gz" ".gitignore" "trailing." "no-extension" "report.final.md" "문서.txt")"#
        ]],
    )
}

fn anju_buffer_filters_classify_a_real_mixed_editor_session() -> ParityBatchCase {
    ParityBatchCase::value(
        "anju_buffer_filters_classify_a_real_mixed_editor_session",
        r##"(let* ((root
                  (file-name-as-directory
                   (expand-file-name
                    "filters/project"
                    (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
                 (other
                  (file-name-as-directory
                   (expand-file-name
                    "filters/other"
                    (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
                 (buffers
                  (list
                   (anju-test-buffer "notes.org" #'org-mode root)
                   (anju-test-buffer "README.md" #'markdown-mode root)
                   (anju-test-buffer "*Help*" #'help-mode root)
                   (anju-test-buffer "*info*" #'Info-mode root)
                   (anju-test-buffer "*eshell*" #'eshell-mode root)
                   (anju-test-buffer "*shell*" #'shell-mode root)
                   (anju-test-buffer "*compilation*" #'compilation-mode root)
                   (anju-test-buffer "*grep*" #'grep-mode root)
                   (anju-test-buffer "*xref*" #'xref--xref-buffer-mode root)
                   (anju-test-buffer "outside.txt" #'text-mode other)
                   (anju-test-buffer "merge~variant~" #'text-mode root))))
         (unwind-protect
             (cl-labels
                 ((names
                   (items)
                   (mapcar #'buffer-name items)))
               (list
                (names (anju-buffer-list-plain-filter buffers))
                (names (anju-buffer-list-plain-filter buffers 1))
                (names (anju-buffer-list-info-filter buffers))
                (names (anju-buffer-list-help-filter buffers))
                (names (anju-buffer-list-eshell-filter buffers))
                (names (anju-buffer-list-shell-filter buffers))
                (names (anju-buffer-list-compilation-filter buffers))
                (names (anju-buffer-list-grep-filter buffers))
                (names (anju-buffer-list-xref-filter buffers))
                (names (anju-filter-buffers-in-directory buffers root))))
           (anju-test-kill-buffers buffers)))"##,
        expect![[
            r#"OK (("notes.org" "README.md" "outside.txt") ("notes.org") ("*info*") ("*Help*") ("*eshell*") ("*shell*") ("*compilation*") ("*grep*") ("*xref*") ("notes.org" "README.md" "*Help*" "*info*" "*eshell*" "*shell*" "*compilation*" "*grep*" "*xref*" "merge~variant~"))"#
        ]],
    )
}

fn anju_configured_buffer_filter_pipeline_preserves_order_duplicates_and_limits() -> ParityBatchCase
{
    ParityBatchCase::value(
        "anju_configured_buffer_filter_pipeline_preserves_order_duplicates_and_limits",
        r##"(let* ((root
                  (file-name-as-directory
                   (expand-file-name
                    "pipeline"
                    (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
                 (buffers
                  (list
                   (anju-test-buffer "alpha.txt" #'text-mode root)
                   (anju-test-buffer "beta.txt" #'text-mode root)
                   (anju-test-buffer "*Help*" #'help-mode root)))
                 (anju-buffer-list-filter-functions
                  '((anju-buffer-list-plain-filter . 2)
                    (anju-buffer-list-help-filter . 1)
                    (anju-buffer-list-plain-filter . 1)
                    (anju-test-missing-filter . 9)))
                 messages)
         (unwind-protect
             (cl-letf (((symbol-function 'message)
                        (lambda (format-string &rest arguments)
                          (push (apply #'format format-string arguments)
                                messages))))
               (list
                (mapcar
                 #'buffer-name
                 (anju-process-buffer-list-filter-functions buffers))
                (nreverse messages)))
           (anju-test-kill-buffers buffers)))"##,
        expect![[
            r#"OK (("alpha.txt" "beta.txt" "*Help*" "alpha.txt") ("WARNING: anju-test-missing-filter is undefined."))"#
        ]],
    )
}

fn anju_transform_fill_center_and_rectangle_menu_contracts_are_exact() -> ParityBatchCase {
    ParityBatchCase::value(
        "anju_transform_fill_center_and_rectangle_menu_contracts_are_exact",
        r##"(list
         (anju-test-menu-entries anju-transform-text-menu)
         (anju-test-menu-entries anju-center-text-menu)
         (anju-test-menu-entries anju-fill-text-menu)
         (anju-test-menu-entries anju-rectangle-menu))"##,
        expect![[
            r#"OK (((Make\ Upper\ Case "Make Upper Case" upcase-region :enable nil :visible nil :style nil :selected nil :help "Convert selected region to upper case") (Make\ Lower\ Case "Make Lower Case" downcase-region :enable nil :visible nil :style nil :selected nil :help "Convert selected region to lower case") (Capitalize "Capitalize" capitalize-region :enable nil :visible nil :style nil :selected nil :help "Convert the selected region to capitalized form")) ((Line "Line" center-line :enable nil :visible nil :style nil :selected nil :help "Center the line point is on, within the width specified by ‘fill-column’") (Region "Region" center-region :enable (use-region-p) :visible nil :style nil :selected nil :help "Center each nonblank line starting in the region") (Paragraph "Paragraph" center-paragraph :enable nil :visible nil :style nil :selected nil :help "Center each nonblank line in the paragraph at or after point")) ((Paragraph "Paragraph" fill-paragraph :enable nil :visible nil :style nil :selected nil :help "Fill paragraph at or after point") (Region "Region" fill-region :enable (use-region-p) :visible nil :style nil :selected nil :help "Fill each of the paragraphs in the region") (Region\ as\ paragraph "Region as paragraph" fill-region-as-paragraph :enable (use-region-p) :visible nil :style nil :selected nil :help "Fill the region as if it were a single paragraph") (Individual\ paragraphs "Individual paragraphs" fill-individual-paragraphs :enable (use-region-p) :visible nil :style nil :selected nil :help "Fill paragraphs of uniform indentation within the region") (Non-uniform\ paragraphs "Non-uniform paragraphs" fill-nonuniform-paragraphs :enable (use-region-p) :visible nil :style nil :selected nil :help "Fill paragraphs within the region, allowing varying indentation within each")) ((Cut "Cut" kill-rectangle :enable (and (not buffer-read-only) (anju-rectangle-selected-p)) :visible nil :style nil :selected nil :help "Delete the region-rectangle and save it as the last killed one") (Copy "Copy" copy-rectangle-as-kill :enable (anju-rectangle-selected-p) :visible nil :style nil :selected nil :help "Copy the region-rectangle and save it as the last killed one") (Paste "Paste" yank-rectangle :enable (and (not buffer-read-only) (boundp 'killed-rectangle) killed-rectangle) :visible nil :style nil :selected nil :help "Yank the last killed rectangle with upper left corner at point") (Delete "Delete" delete-rectangle :enable (and (not buffer-read-only) (anju-rectangle-selected-p)) :visible nil :style nil :selected nil :help "Delete rectangle") (Replace… "Replace…" string-rectangle :enable (and (not buffer-read-only) (anju-rectangle-selected-p)) :visible nil :style nil :selected nil :help "Replace rectangle contents with STRING on each line") (Insert… "Insert…" string-insert-rectangle :enable (and (not buffer-read-only) (anju-rectangle-selected-p)) :visible nil :style nil :selected nil :help "Insert STRING on each line of region-rectangle, shifting text right") (Number "Number" rectangle-number-lines :enable (and (not buffer-read-only) (anju-rectangle-selected-p)) :visible nil :style nil :selected nil :help "Insert numbers in front of the region-rectangle") (Clear "Clear" clear-rectangle :enable (and (not buffer-read-only) (anju-rectangle-selected-p)) :visible nil :style nil :selected nil :help "Blank out the region-rectangle") (Blank "Blank" open-rectangle :enable (and (not buffer-read-only) (anju-rectangle-selected-p)) :visible nil :style nil :selected nil :help "Blank out the region-rectangle, shifting text right") (Delete\ leading\ spaces "Delete leading spaces" delete-whitespace-rectangle :enable (and (not buffer-read-only) (anju-rectangle-selected-p)) :visible nil :style nil :selected nil :help "Delete all whitespace following a specified column in each line")))"#
        ]],
    )
}

fn anju_unsets_every_legacy_mouse_binding_without_touching_mouse_two_yank() -> ParityBatchCase {
    ParityBatchCase::value(
        "anju_unsets_every_legacy_mouse_binding_without_touching_mouse_two_yank",
        r##"(let ((mode-line-buffer-identification-keymap
                (copy-keymap mode-line-buffer-identification-keymap))
               (global-map (copy-keymap global-map)))
         (dolist
             (key
              '("<mode-line> C-<mouse-2>"
                "<vertical-scroll-bar> C-<mouse-2>"
                "<horizontal-scroll-bar> C-<mouse-2>"
                "<vertical-line> C-<mouse-2>"
                "<right-divider> C-<mouse-2>"
                "<bottom-divider> C-<mouse-2>"
                "<mode-line> <mouse-2>"
                "<mode-line> <mouse-3>"
                "<mode-line> <double-mouse-1>"))
           (keymap-global-set key #'ignore))
         (keymap-set
          mode-line-buffer-identification-keymap
          "<mode-line> <mouse-1>"
          #'ignore)
         (keymap-set
          mode-line-buffer-identification-keymap
          "<mode-line> <mouse-3>"
          #'ignore)
         (anju-utils--unset-legacy-mouse-bindings)
         (list
          (mapcar
           (lambda (key)
             (key-binding (kbd key)))
           '("<mode-line> C-<mouse-2>"
             "<vertical-scroll-bar> C-<mouse-2>"
             "<horizontal-scroll-bar> C-<mouse-2>"
             "<vertical-line> C-<mouse-2>"
             "<right-divider> C-<mouse-2>"
             "<bottom-divider> C-<mouse-2>"
             "<mode-line> <mouse-2>"
             "<mode-line> <mouse-3>"
             "<mode-line> <double-mouse-1>"))
          (lookup-key
           mode-line-buffer-identification-keymap
           (kbd "<mode-line> <mouse-1>"))
          (lookup-key
           mode-line-buffer-identification-keymap
           (kbd "<mode-line> <mouse-3>"))
          (key-binding (kbd "<mouse-2>"))))"##,
        expect!["OK ((nil nil nil nil nil nil nil nil nil) nil nil mouse-yank-primary)"],
    )
    .fresh_process()
}

fn anju_new_frame_command_prefixes_and_dispatches_interactively() -> ParityBatchCase {
    ParityBatchCase::value(
        "anju_new_frame_command_prefixes_and_dispatches_interactively",
        r##"(let (events)
         (cl-letf (((symbol-function 'other-frame-prefix)
                    (lambda ()
                      (push 'other-frame-prefix events)))
                   ((symbol-function 'call-interactively)
                    (lambda (command &optional record keys)
                      (push
                       (list 'call-interactively command record keys)
                       events)
                      'called)))
           (list
            (anju-utils--command-in-new-frame #'info)
            (nreverse events))))"##,
        expect!["OK (called (other-frame-prefix (call-interactively info nil nil)))"],
    )
}

pub(super) fn utils_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        anju_middle_truncate_handles_short_long_multiline_unicode_and_invalid_extents(),
        anju_menu_label_uses_the_real_active_region_and_strips_properties(),
        anju_filename_extraction_preserves_real_world_names(),
        anju_buffer_filters_classify_a_real_mixed_editor_session(),
        anju_configured_buffer_filter_pipeline_preserves_order_duplicates_and_limits(),
        anju_transform_fill_center_and_rectangle_menu_contracts_are_exact(),
        anju_unsets_every_legacy_mouse_binding_without_touching_mouse_two_yank(),
        anju_new_frame_command_prefixes_and_dispatches_interactively(),
    ]
}
