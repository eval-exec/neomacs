//! Regression for the fast-track runtime fix: `EditHost::sync_window_positions`
//! must refresh a displayed window's position caches after an edit.
//!
//! GNU keeps a window's start, point and old point in Lisp markers
//! (`w->start`/`w->pointm`/`w->old_pointm`, src/window.c:179-193 in the
//! pinned GNU Emacs 31.1 tree), and `adjust_markers_for_insert`
//! (src/insdel.c:288) advances markers that sit after an insertion point.
//! Neomacs derives the cached window positions from those markers at
//! `signal_after_change`; the stage-4 eval split left
//! `EditHost::sync_window_positions` delegating to itself, so the first
//! buffer edit recursed without bound instead of refreshing the caches.

use neovm_display::window::DisplayFrameManagerExt;
use neovm_edit::buffer::LispCharPos1;
use neovm_eval::emacs_core::eval::data::window_markers::attach_frame_window_position_markers;
use neovm_eval::emacs_core::eval::data::window_state::Window;
use neovm_value::value::Value;

#[test]
fn inserting_before_a_displayed_window_advances_its_position_caches() {
    let mut eval = neovm_core::emacs_core::eval_hook_installers::new_context();
    let buffer = eval.edit.buffers.create_buffer("*sync-window-positions*");
    eval.edit.buffers.set_current(buffer);

    // Seed text through the buffer manager so this step does not dispatch
    // through the EditHost under test.
    let seed = Value::string("abcdef");
    eval.edit
        .buffers
        .insert_lisp_string_into_buffer(buffer, seed.as_lisp_string().expect("heap string"))
        .expect("seed insert");

    let frame_id = eval
        .frame_manager_mut()
        .create_frame("F1", 800, 600, buffer);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .root_window()
        .id();
    {
        let (frames, buffers) = eval.frame_manager_and_buffers_mut();
        let frame = frames.get_mut(frame_id).expect("frame");
        {
            let window = frame.find_window_mut(window_id).expect("root window");
            let Window::Leaf {
                window_start,
                point,
                old_point,
                ..
            } = window
            else {
                panic!("root window must be a leaf");
            };
            *window_start = LispCharPos1::from_one_based_usize(3);
            *point = LispCharPos1::from_one_based_usize(5);
            *old_point = LispCharPos1::from_one_based_usize(5);
        }
        attach_frame_window_position_markers(buffers, frame);
    }

    // This Lisp edit goes through `signal_after_change` and therefore
    // through `EditHost::sync_window_positions`. Both window positions sit
    // after the insertion point, so GNU marker adjustment moves them by
    // the two inserted characters.
    eval.eval_str("(progn (goto-char 1) (insert \"XY\"))")
        .expect("edit through the EditHost");

    let frame = eval.frame_manager().get(frame_id).expect("frame");
    let window = frame.find_window(window_id).expect("root window");
    let Window::Leaf {
        window_start,
        point,
        old_point,
        ..
    } = window
    else {
        panic!("root window must be a leaf");
    };
    assert_eq!(
        *window_start,
        LispCharPos1::from_one_based_usize(5),
        "window-start cache must follow the edit (3 + 2 inserted characters)"
    );
    assert_eq!(
        *point,
        LispCharPos1::from_one_based_usize(7),
        "window point cache must follow the edit (5 + 2 inserted characters)"
    );
    assert_eq!(
        *old_point,
        LispCharPos1::from_one_based_usize(7),
        "window old-point cache must follow the edit too (5 + 2 inserted characters)"
    );
    assert_eq!(
        eval.eval_str("(buffer-string)").expect("buffer text"),
        Value::string("XYabcdef")
    );
}
