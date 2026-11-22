//! `kill-buffer` retires the evaluator's per-buffer redisplay revisions.
//!
//! `body_redisplay_by_buffer` is Neomacs' side table for GNU's per-buffer
//! `clip_changed` and `prevent_redisplay_optimizations_p`.  GNU keeps those
//! flags on the buffer, so they die with it; the side table must drop the
//! same ids or a later buffer that reuses a `BufferId` inherits the dead
//! buffer's revisions and refuses matrix reuse.

use super::*;

#[test]
fn kill_buffer_prunes_redisplay_revisions_of_the_retired_buffer_only() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    // Keep the kill path out of window.el: the bare unit runtime has no
    // `replace-buffer-in-windows`, and this row is about the revision table.
    eval.set_variable("kill-buffer-quit-windows", Value::T);
    let doomed = eval.edit.buffers.create_buffer("*redisplay-doomed*");
    let survivor = eval.edit.buffers.create_buffer("*redisplay-survivor*");
    // kill-buffer asserts that a retirement leaves some current buffer when
    // the target was not current; select the survivor before the kill.
    assert!(eval.edit.buffers.switch_current_unrecorded(survivor));
    for buffer in [doomed, survivor] {
        eval.mark_buffer_body_redisplay(buffer);
        eval.gnu_mark_buffer_prevent_redisplay_optimizations(buffer);
        assert!(
            eval.tracks_buffer_redisplay_revisions(buffer),
            "the probe must record both revisions"
        );
    }

    let result =
        builtin_kill_buffer(&mut eval, vec![Value::make_buffer(doomed)]).expect("kill-buffer");
    assert_eq!(result, Value::T);
    assert!(eval.edit.buffers.get(doomed).is_none());
    assert!(
        !eval.tracks_buffer_redisplay_revisions(doomed),
        "a killed buffer's redisplay revisions must be pruned"
    );
    assert_eq!(
        eval.prevent_redisplay_optimizations_revision(doomed),
        Default::default()
    );
    assert!(
        eval.tracks_buffer_redisplay_revisions(survivor),
        "an unrelated live buffer keeps its revisions"
    );
}
