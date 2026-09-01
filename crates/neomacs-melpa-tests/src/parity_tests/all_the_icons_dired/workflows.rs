use expect_test::expect;

use super::ParityBatchCase;

/// Turning the mode on in a real Dired buffer.  Every file gets a three
/// character `" X "` display property on the character before its name, with
/// the middle character carrying the icon's own properties; `.` and `..` get a
/// four space placeholder instead.  The buffer text itself is untouched --
/// icons are display properties, not inserted characters.
fn turning_the_mode_on_puts_a_display_icon_before_every_filename() -> ParityBatchCase {
    ParityBatchCase::value(
        "turning_the_mode_on_puts_a_display_icon_before_every_filename",
        r##"(atid-test-in-dired
 (let ((before-lines (atid-test-lines))
       (before-count (atid-test-display-count))
       (before-text (atid-test-text)))
   (all-the-icons-dired-mode 1)
   (font-lock-ensure)
   (list :mode-on (and all-the-icons-dired-mode t)
         :lighter all-the-icons-dired-lighter
         :before-count before-count
         :before-lines before-lines
         :after-count (atid-test-display-count)
         :after-lines (atid-test-lines)
         :text-unchanged (string= before-text (atid-test-text)))))"##,
        expect![[
            r#"OK (:mode-on t :lighter " all-the-icons-dired-mode" :before-count 1 :before-lines (("." none) (".." none) (".hidden-config" none) ("README.md" none) ("notes.org" none) ("script.py" none) ("subdir" none)) :after-count 7 :after-lines (("." (string 4 "    " plain)) (".." (string 4 "    " plain)) (".hidden-config" (string 3 "  " icon-props)) ("README.md" (string 3 "  " icon-props)) ("notes.org" (string 3 "  " icon-props)) ("script.py" (string 3 "  " icon-props)) ("subdir" (string 3 "  " icon-props))) :text-unchanged t)"#
        ]],
    )
}

fn reverting_the_listing_reapplies_every_icon() -> ParityBatchCase {
    ParityBatchCase::value(
        "reverting_the_listing_reapplies_every_icon",
        r##"(atid-test-in-dired
 (all-the-icons-dired-mode 1)
 (font-lock-ensure)
 (let ((before (atid-test-lines)) (before-text (atid-test-text)))
   (revert-buffer)
   (font-lock-ensure)
   (list :after-revert (atid-test-lines)
         :count (atid-test-display-count)
         :same-as-before (equal before (atid-test-lines))
         :text-unchanged (string= before-text (atid-test-text)))))"##,
        expect![[
            r#"OK (:after-revert (("." (string 4 "    " plain)) (".." (string 4 "    " plain)) (".hidden-config" (string 3 "  " icon-props)) ("README.md" (string 3 "  " icon-props)) ("notes.org" (string 3 "  " icon-props)) ("script.py" (string 3 "  " icon-props)) ("subdir" (string 3 "  " icon-props))) :count 7 :same-as-before t :text-unchanged t)"#
        ]],
    )
}

fn inserting_a_subdirectory_gets_icons_on_its_lines_and_its_dot_entries() -> ParityBatchCase {
    ParityBatchCase::value(
        "inserting_a_subdirectory_gets_icons_on_its_lines_and_its_dot_entries",
        r##"(atid-test-in-dired
 (all-the-icons-dired-mode 1)
 (font-lock-ensure)
 (let ((before-count (length (atid-test-lines))))
   (goto-char (point-min))
   (search-forward "subdir")
   (dired-maybe-insert-subdir (expand-file-name "subdir" atid-test-tree))
   (font-lock-ensure)
   (list :before-count before-count
         :after (atid-test-lines)
         :display-count (atid-test-display-count))))"##,
        expect![[
            r#"OK (:before-count 7 :after (("." (string 4 "    " plain)) (".." (string 4 "    " plain)) (".hidden-config" (string 3 "  " icon-props)) ("README.md" (string 3 "  " icon-props)) ("notes.org" (string 3 "  " icon-props)) ("script.py" (string 3 "  " icon-props)) ("subdir" (string 3 "  " icon-props)) ("subdir/." (string 3 "  " icon-props)) ("subdir/.." (string 3 "  " icon-props)) ("subdir/nested.el" (string 3 "  " icon-props))) :display-count 10)"#
        ]],
    )
}

fn turning_the_mode_off_removes_every_display_property_including_dired_s_own() -> ParityBatchCase {
    ParityBatchCase::value(
        "turning_the_mode_off_removes_every_display_property_including_dired_s_own",
        r##"(atid-test-in-dired
 (let ((pristine-text (atid-test-text))
       (pristine-count (atid-test-display-count)))
   (all-the-icons-dired-mode 1)
   (font-lock-ensure)
   (let ((on-count (atid-test-display-count)))
     (all-the-icons-dired-mode -1)
     (font-lock-ensure)
     (list :pristine-count pristine-count
           :on-count on-count
           :off-count (atid-test-display-count)
           :off-lines (atid-test-lines)
           :mode-off (and all-the-icons-dired-mode t)
           :text-identical (string= pristine-text (atid-test-text))))))"##,
        expect![[
            r#"OK (:pristine-count 1 :on-count 7 :off-count 0 :off-lines (("." none) (".." none) (".hidden-config" none) ("README.md" none) ("notes.org" none) ("script.py" none) ("subdir" none)) :mode-off nil :text-identical t)"#
        ]],
    )
}

fn enabling_the_mode_outside_dired_changes_nothing_but_the_flag() -> ParityBatchCase {
    ParityBatchCase::value(
        "enabling_the_mode_outside_dired_changes_nothing_but_the_flag",
        r##"(let ((buffer (generate-new-buffer "*nicht-dired*")))
  (unwind-protect
      (with-current-buffer buffer
        (fundamental-mode)
        (insert "nur Text\n")
        (let ((before (atid-test-text))
              (fontifier font-lock-fontify-region-function))
          (all-the-icons-dired-mode 1)
          (list :mode-flag (and all-the-icons-dired-mode t)
                :fontifier-unchanged (eq fontifier font-lock-fontify-region-function)
                :extra-props font-lock-extra-managed-props
                :text-unchanged (string= before (atid-test-text))
                :display-count (atid-test-display-count)
                :lighter (cdr (assq 'all-the-icons-dired-mode minor-mode-alist)))))
    (kill-buffer buffer)))"##,
        expect![[
            r#"OK (:mode-flag t :fontifier-unchanged t :extra-props nil :text-unchanged t :display-count 0 :lighter (all-the-icons-dired-lighter))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        turning_the_mode_on_puts_a_display_icon_before_every_filename(),
        reverting_the_listing_reapplies_every_icon(),
        inserting_a_subdirectory_gets_icons_on_its_lines_and_its_dot_entries(),
        turning_the_mode_off_removes_every_display_property_including_dired_s_own(),
        enabling_the_mode_outside_dired_changes_nothing_but_the_flag(),
    ]
}
