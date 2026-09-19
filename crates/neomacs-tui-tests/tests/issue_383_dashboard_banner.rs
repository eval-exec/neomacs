//! Dashboard covers its text banner with an image display property. GNU's
//! terminal redisplay ignores the image and preserves the underlying text.

use crate::support::*;
use std::time::Duration;

#[test]
fn terminal_image_property_preserves_multiline_text_banner() {
    let (mut gnu, mut neo) = boot_pair("");
    eval_expression(
        &mut gnu,
        &mut neo,
        r##"(progn
          (switch-to-buffer (get-buffer-create "*banner*"))
          (erase-buffer)
          (insert "⣀⣤⣶⣿⣶⣤⣀\n⣿  Emacs  ⣿\n⠉⠛⠿⣿⠿⠛⠉\n")
          (add-text-properties (point-min) (point-max)
            '(face (:foreground "magenta")
              display (image :type xbm :width 1 :height 1 :data "\0")))
          (insert "Welcome to Emacs!")
          (goto-char (point-min)))"##,
    );
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(5), |grid| {
        grid.iter().any(|row| row.contains("Welcome to Emacs!"))
    });
    for (label, session) in [("GNU", &gnu), ("Neomacs", &neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|row| row.contains("⣿  Emacs  ⣿")),
            "{label} must show the underlying banner, not an image placeholder:\n{}",
            grid.join("\n")
        );
    }
    assert_pair_exact_display(
        "terminal_image_property_preserves_multiline_text_banner",
        &gnu,
        &neo,
    );
}

#[test]
fn terminal_image_specs_leave_text_and_later_display_alternatives_available() {
    let (mut gnu, mut neo) = boot_pair("");
    // An ignored image must neither claim replacement ownership nor stop
    // evaluation of the later conditional spec on the overlay string.
    eval_expression(
        &mut gnu,
        &mut neo,
        r##"(progn
          (switch-to-buffer (get-buffer-create "*banner-alternatives*"))
          (erase-buffer)
          (let ((image '(image :type xbm :width 1 :height 1 :data "\0")))
            (insert (propertize "covered" 'display (list image "BUFFER-FALLBACK")) "\n")
            (insert (propertize "MARGIN-TEXT" 'display (list '(margin left-margin) image)) "\n")
            (let ((overlay (make-overlay (point) (point))))
              (overlay-put overlay 'before-string
                (concat (propertize "OVERLAY-TEXT" 'display image) "\n"
                        (propertize "covered" 'display
                          (vector image '(when (display-graphic-p) . "WRONG-GRAPHICAL-CLAUSE")
                                  "STRING-FALLBACK")) "\n")))
            (insert "BANNER-END"))
          (goto-char (point-min)) nil)"##,
    );
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(5), |grid| {
        grid.iter().any(|row| row.contains("BANNER-END"))
    });
    for (label, session) in [("GNU", &gnu), ("Neomacs", &neo)] {
        let grid = session.text_grid();
        for expected in [
            "BUFFER-FALLBACK",
            "MARGIN-TEXT",
            "OVERLAY-TEXT",
            "STRING-FALLBACK",
        ] {
            assert!(
                grid.iter().any(|row| row.contains(expected)),
                "{label} must show {expected}:\n{}",
                grid.join("\n")
            );
        }
    }
    assert_pair_exact_display(
        "terminal_image_specs_leave_text_and_later_display_alternatives_available",
        &gnu,
        &neo,
    );
}
