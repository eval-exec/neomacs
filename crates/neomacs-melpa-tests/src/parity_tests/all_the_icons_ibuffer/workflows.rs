use expect_test::expect;

use super::ParityBatchCase;

/// The documented installation: enable the mode in an Ibuffer.  It swaps
/// `ibuffer-formats` for its own layout, so the listing gains the package's
/// header and every line gains an icon column -- a glyph carrying display and
/// face properties, followed by the half-width spacer the package appends.
fn enabling_the_mode_swaps_the_formats_and_adds_an_icon_column() -> ParityBatchCase {
    ParityBatchCase::value(
        "enabling_the_mode_swaps_the_formats_and_adds_an_icon_column",
        r##"(atib-test-in-ibuffer
 (let ((before-formats-were-default (equal ibuffer-formats
                                           all-the-icons-ibuffer-old-formats)))
   (all-the-icons-ibuffer-mode 1)
   (list :before-formats-were-default before-formats-were-default
         :formats-swapped (equal ibuffer-formats all-the-icons-ibuffer-formats)
         :header (substring-no-properties
                  (buffer-substring (point-min) (min (point-max) (+ (point-min) 69))))
         :icons (atib-test-icon-cells))))"##,
        expect![[
            r#"OK (:before-formats-were-default t :formats-swapped t :header " MRL   Name                    Size Mode             Filename/Process" :icons (("atib-code.el" icon-glyph display face 32 ((space :relative-width 0.5))) ("atib-large" icon-glyph display face 32 ((space :relative-width 0.5))) ("atib-org" icon-glyph display face 32 ((space :relative-width 0.5))) ("atib-plain" icon-glyph display face 32 ((space :relative-width 0.5))) ("atib-script.py" icon-glyph display face 32 ((space :relative-width 0.5)))))"#
        ]],
    )
}

fn the_icon_column_is_empty_when_either_gate_is_closed() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_icon_column_is_empty_when_either_gate_is_closed",
        r##"(atib-test-in-ibuffer
 (let ((graphic (let ((all-the-icons-ibuffer-display-predicate #'display-graphic-p))
                  (all-the-icons-ibuffer-mode 1)
                  (list (funcall #'display-graphic-p) (atib-test-icon-cells)))))
   (all-the-icons-ibuffer-mode -1)
   (let ((icon-off (let ((all-the-icons-ibuffer-icon nil))
                     (all-the-icons-ibuffer-mode 1)
                     (atib-test-icon-cells))))
     (list :with-graphic-predicate graphic :with-icon-disabled icon-off))))"##,
        expect![[
            r#"OK (:with-graphic-predicate (nil (("atib-code.el" 32 no-display no-face 32 nil) ("atib-large" 32 no-display no-face 32 nil) ("atib-org" 32 no-display no-face 32 nil) ("atib-plain" 32 no-display no-face 32 nil) ("atib-script.py" 32 no-display no-face 32 nil))) :with-icon-disabled (("atib-code.el" 32 no-display no-face 32 nil) ("atib-large" 32 no-display no-face 32 nil) ("atib-org" 32 no-display no-face 32 nil) ("atib-plain" 32 no-display no-face 32 nil) ("atib-script.py" 32 no-display no-face 32 nil)))"#
        ]],
    )
}

fn the_size_column_honours_the_human_readable_setting() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_size_column_honours_the_human_readable_setting",
        r##"(atib-test-in-ibuffer
 (all-the-icons-ibuffer-mode 1)
 (let ((human (atib-test-columns)))
   (let ((all-the-icons-ibuffer-human-readable-size nil))
     (ibuffer-update nil t)
     (list :human-readable human
           :raw (atib-test-columns)
           :mode-line-empty (format-mode-line mode-name nil nil (get-buffer "atib-org"))))))"##,
        expect![[
            r#"OK (:human-readable (("atib-code.el" "atib-code.el" "24" "[ORACLE-SANDBOX]/atib-code.el") ("atib-large" "atib-large" "2k") ("atib-org" "atib-org" "8") ("atib-plain" "atib-plain" "15") ("atib-script.py" "atib-script.py" "15" "[ORACLE-SANDBOX]/atib-script.py")) :raw (("atib-code.el" "atib-code.el" "24" "[ORACLE-SANDBOX]/atib-code.el") ("atib-large" "atib-large" "2048") ("atib-org" "atib-org" "8") ("atib-plain" "atib-plain" "15") ("atib-script.py" "atib-script.py" "15" "[ORACLE-SANDBOX]/atib-script.py")) :mode-line-empty "")"#
        ]],
    )
}

fn ibuffer_update_picks_up_a_new_buffer_and_drops_a_killed_one() -> ParityBatchCase {
    ParityBatchCase::value(
        "ibuffer_update_picks_up_a_new_buffer_and_drops_a_killed_one",
        r##"(atib-test-in-ibuffer
 (all-the-icons-ibuffer-mode 1)
 (let ((initial (mapcar #'car (atib-test-icon-cells))))
   (with-current-buffer (get-buffer-create "atib-neu")
     (fundamental-mode) (erase-buffer) (insert "neu\n"))
   (ibuffer-update nil t)
   (let ((after-add (atib-test-icon-cells)))
     (let ((kill-buffer-query-functions nil))
       (with-current-buffer "atib-plain" (set-buffer-modified-p nil))
       (kill-buffer "atib-plain"))
     (ibuffer-update nil t)
     (list :initial initial
           :after-add (mapcar #'car after-add)
           :new-line-has-icon (cdr (assoc "atib-neu" after-add))
           :after-kill (mapcar #'car (atib-test-icon-cells))))))"##,
        expect![[
            r#"OK (:initial ("atib-code.el" "atib-large" "atib-org" "atib-plain" "atib-script.py") :after-add ("atib-code.el" "atib-large" "atib-neu" "atib-org" "atib-plain" "atib-script.py") :new-line-has-icon (icon-glyph display face 32 ((space :relative-width 0.5))) :after-kill ("atib-code.el" "atib-large" "atib-neu" "atib-org" "atib-script.py"))"#
        ]],
    )
}

fn turning_the_mode_off_restores_the_previous_formats() -> ParityBatchCase {
    ParityBatchCase::value(
        "turning_the_mode_off_restores_the_previous_formats",
        r##"(atib-test-in-ibuffer
 (all-the-icons-ibuffer-mode 1)
 (let ((on-icons (atib-test-icon-cells))
       (on-formats (equal ibuffer-formats all-the-icons-ibuffer-formats)))
   (all-the-icons-ibuffer-mode -1)
   (list :on-formats on-formats
         :on-icons on-icons
         :off-formats-restored (equal ibuffer-formats all-the-icons-ibuffer-old-formats)
         :off-icons (atib-test-icon-cells)
         :mode-flag (and all-the-icons-ibuffer-mode t))))"##,
        expect![[
            r#"OK (:on-formats t :on-icons (("atib-code.el" icon-glyph display face 32 ((space :relative-width 0.5))) ("atib-large" icon-glyph display face 32 ((space :relative-width 0.5))) ("atib-org" icon-glyph display face 32 ((space :relative-width 0.5))) ("atib-plain" icon-glyph display face 32 ((space :relative-width 0.5))) ("atib-script.py" icon-glyph display face 32 ((space :relative-width 0.5)))) :off-formats-restored t :off-icons (("atib-code.el" 97 no-display no-face 116 nil) ("atib-large" 97 no-display no-face 116 nil) ("atib-org" 97 no-display no-face 116 nil) ("atib-plain" 97 no-display no-face 116 nil) ("atib-script.py" 97 no-display no-face 116 nil)) :mode-flag nil)"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        enabling_the_mode_swaps_the_formats_and_adds_an_icon_column(),
        the_icon_column_is_empty_when_either_gate_is_closed(),
        the_size_column_honours_the_human_readable_setting(),
        ibuffer_update_picks_up_a_new_buffer_and_drops_a_killed_one(),
        turning_the_mode_off_restores_the_previous_formats(),
    ]
}
