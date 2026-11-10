//! Complete GNU redisplay owns mini preparation; committed-start callbacks
//! retain recursive-display protection without claiming that preparation.
//! Field offsets of the ownership record are a production const assert.
use crate::emacs_core::eval::redisplay_hooks::RedisplayHookOwnership;
use crate::emacs_core::eval::{Context, RedisplayHookPolicyGuard};
use crate::window::DisplayFrameManagerExt;
use std::cell::Cell;
use std::rc::Rc;

fn actual_transaction_active(eval: &Context) -> bool {
    eval.gnu_redisplay_transaction_active()
}

#[test]
fn transaction_owner_excludes_standalone_scroll_and_restores_nested_and_unwound_passes() {
    crate::test_utils::init_test_tracing();
    let _policy = RedisplayHookPolicyGuard::gnu();
    // Capture these before the semantic RED, then compare every numeric value
    // with the identical output from the repaired native binary. This includes
    // all Context fields baked into generated code or the AOT ABI hash.
    tracing::info!(target: "d5::owner_layout", hook_size = std::mem::size_of::<RedisplayHookOwnership>(), hook_align = std::mem::align_of::<RedisplayHookOwnership>(), context_size = std::mem::size_of::<Context>(), context_align = std::mem::align_of::<Context>(), "owner layout sizes");
    let mut eval = crate::test_utils::new_context();
    assert!(!actual_transaction_active(&eval));
    eval.redisplay_fn = Some(Box::new(|_| {
        panic!("committed-scroll reentry must be suppressed")
    }));
    {
        let guard = eval.gnu_guard_committed_scroll();
        assert!(
            !actual_transaction_active(guard.eval),
            "a standalone committed-start callback has no GNU mini preparation owner"
        );
        guard
            .eval
            .redisplay_with_force_flow(true)
            .expect("committed-scroll reentry suppressed");
    }
    assert!(!actual_transaction_active(&eval));
    let buffer = eval.edit.buffers.current_buffer_id().expect("buffer");
    eval.frame_manager_mut()
        .create_frame("owner-regression", 80, 24, buffer);
    let called = Rc::new(Cell::new(false));
    let observed = called.clone();
    eval.redisplay_fn = Some(Box::new(move |eval| {
        assert!(actual_transaction_active(eval));
        {
            let guard = eval.gnu_guard_committed_scroll();
            assert!(
                actual_transaction_active(guard.eval),
                "nested committed-start work retains the outer transaction owner"
            );
        }
        assert!(actual_transaction_active(eval));
        observed.set(true);
    }));
    eval.redisplay_with_force_flow(true)
        .expect("owned frontend callback");
    assert!(called.get());
    assert!(!actual_transaction_active(&eval));
    assert!(eval.redisplay_fn.is_some());
    eval.redisplay_fn = Some(Box::new(|eval| {
        assert!(actual_transaction_active(eval));
        panic!("exercise owned frontend unwind");
    }));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = eval.redisplay_with_force_flow(true);
    }));
    assert!(result.is_err());
    assert!(!actual_transaction_active(&eval));
    assert!(eval.redisplay_fn.is_some());
}
