//! SVG image specification behavior remains available on terminal frames.

use crate::support::*;
use std::time::Duration;

#[test]
fn static_svg_explicit_nil_animation_preserves_frame_selection() {
    let (mut gnu, mut neo) = boot_pair("");
    eval_expression(
        &mut gnu,
        &mut neo,
        r#"(progn (require 'image) (let ((image (create-image "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"10\" height=\"10\"><circle r=\"1\"><animate attributeName=\"r\" from=\"1\" to=\"4\" dur=\"1s\" repeatCount=\"indefinite\"/></circle></svg>" 'svg t :animation nil :index 19))) (message "svg-static %S" (list (car image) (plist-get (cdr image) :type) (plist-get (cdr image) :animation) (image-current-frame image) (image-animate-timer image)))))"#,
    );
    let expected = "svg-static (image svg nil 19 nil)";
    let ready = |grid: &[String]| grid.iter().any(|row| row.contains(expected));
    gnu.read_until(Duration::from_secs(6), ready);
    neo.read_until(Duration::from_secs(8), ready);
    for (label, session) in [("GNU", &gnu), ("Neomacs", &neo)] {
        assert!(
            ready(&session.text_grid()),
            "{label} must accept a static SVG spec with a nonzero index:\n{}",
            session.text_grid().join("\n")
        );
    }
    assert_pair_exact_display("static_svg_explicit_nil_animation", &gnu, &neo);
}
