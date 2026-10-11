//! GNU window.c select_window marks the old/new windows only for a recorded
//! selection or mark-for-redisplay. Temporary hook selections raise SOME
//! without making another window's mode line dirty. Each fixture owns its
//! Context exclusively and uses only a cfg(test) numeric policy guard.

use super::*;
use crate::emacs_core::eval::RedisplayHookPolicyGuard;

fn fixture() -> (crate::emacs_core::Context, WindowId, WindowId, WindowId) {
    let mut eval = crate::emacs_core::Context::new();
    let buffer = eval.buffers.current_buffer_id().expect("fixture buffer");
    let frame = eval
        .frames
        .create_frame("selection-chrome", 120, 40, buffer);
    assert!(eval.frames.select_frame(frame));
    let old = eval.frames.get(frame).expect("frame").selected_window;
    let new = eval
        .frames
        .split_window(
            frame,
            old,
            crate::window::SplitDirection::Horizontal,
            buffer,
            Some(18),
            crate::window::SplitPlacement::AfterTarget,
        )
        .expect("second leaf");
    let untouched = eval
        .frames
        .get(frame)
        .expect("frame")
        .minibuffer_window
        .expect("mini");
    eval.set_variable("buffer-list-update-hook", Value::NIL);
    for window in [old, new, untouched] {
        eval.note_chrome_generated(window);
    }
    (eval, old, new, untouched)
}

#[test]
fn gnu_temporary_selection_preserves_clean_chrome_and_recorded_selection_marks_only_owners() {
    let _policy = RedisplayHookPolicyGuard::gnu();
    for norecord in [
        Value::T,
        Value::symbol("temporary-selection"),
        Value::NIL,
        Value::symbol("mark-for-redisplay"),
    ] {
        let (mut eval, old, new, untouched) = fixture();
        builtin_select_window(&mut eval, vec![Value::make_window(new.0), norecord])
            .expect("selection");
        let marked = norecord.is_nil() || norecord.is_symbol_named("mark-for-redisplay");
        assert_eq!(
            eval.chrome_dirty().is_dirty(old),
            marked,
            "old owner for {norecord:?}"
        );
        assert_eq!(
            eval.chrome_dirty().is_dirty(new),
            marked,
            "new owner for {norecord:?}"
        );
        assert!(
            !eval.chrome_dirty().is_dirty(untouched),
            "selection cannot dirty the uninvolved minibuffer"
        );
        if !marked {
            builtin_select_window(&mut eval, vec![Value::make_window(old.0), Value::T])
                .expect("temporary restoration");
            assert!(!eval.chrome_dirty().is_dirty(old));
            assert!(!eval.chrome_dirty().is_dirty(new));
        }
    }
}

#[test]
fn legacy_temporary_selection_preserves_global_chrome_invalidation() {
    let _policy = RedisplayHookPolicyGuard::legacy();
    let (mut eval, old, new, untouched) = fixture();
    builtin_select_window(&mut eval, vec![Value::make_window(new.0), Value::T]).expect("selection");
    assert!(
        [old, new, untouched]
            .into_iter()
            .all(|window| eval.chrome_dirty().is_dirty(window))
    );
}
