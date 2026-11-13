//! display_test tests re-homed to the core upper-layer harness.
//!
//! Re-homed from neovm-display (display upper-layer re-home).
use neovm_eval::emacs_core::eval::Context;
use neovm_value::error::{FlowKind, FlowResultExt as _};

#[test]
fn x_popup_menu_position_window_slot_accepts_a_frame_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let menu = r#"'("Title" ("Pane" ("item" . 1)))"#;
    let probe = |eval: &mut Context, position: &str| -> String {
        let form = format!(
            r#"(condition-case e (x-popup-menu {position} {menu})
                 (error (list (car e) (cdr e))))"#
        );
        neovm_eval::emacs_core::print::print_value(
            &eval.eval_str(&form).expect("probe should evaluate"),
        )
    };

    // A frame in the window slot: GNU puts the menu at the frame origin.
    assert_eq!(
        probe(&mut eval, "(list (list 0 0) (selected-frame))"),
        "nil"
    );
    assert_eq!(
        probe(&mut eval, "(list (list 5 7) (selected-frame))"),
        "nil"
    );
    // A live window is equally acceptable.
    assert_eq!(
        probe(&mut eval, "(list (list 5 7) (selected-window))"),
        "nil"
    );
    // Both coordinates nil: GNU never inspects the designator at all.
    assert_eq!(
        probe(&mut eval, "(list (list nil nil) 'not-a-window)"),
        "nil"
    );
    assert_eq!(probe(&mut eval, "(list (list nil nil))"), "nil");
    // Anything that is neither a frame nor a window is still `windowp`.
    assert_eq!(
        probe(&mut eval, "(list (list 0 0) 'not-a-window)"),
        "(wrong-type-argument (windowp not-a-window))"
    );
    assert_eq!(
        probe(&mut eval, "(list (list 0 0))"),
        "(wrong-type-argument (windowp nil))"
    );
    // A valid but non-live (internal) window is `window-live-p`, not `windowp`:
    // GNU reaches it through `CHECK_LIVE_WINDOW`.
    let internal = probe(
        &mut eval,
        "(list (list 0 0) (window-parent ((lambda (old size side normal) (set-window-new-pixel old (- (if (memq side '(t left right)) (window-pixel-width old) (window-pixel-height old)) size)) (split-window-internal old size side normal)) nil (/ (window-pixel-height nil) 2) nil nil)))",
    );
    assert!(
        internal.starts_with("(wrong-type-argument (window-live-p "),
        "an internal window must fail the liveness check, not the type check: {internal}"
    );
}
