//! Complete GNU redisplay owns mini preparation; committed-start callbacks
//! retain recursive-display protection without claiming that preparation.
use super::*;
use std::cell::Cell;
use std::rc::Rc;

struct PreviousBoolHookOwnership {
    active: bool,
    windows: PendingScope,
    mode_lines: PendingScope,
    redisplay_windows: HashSet<WindowId>,
    mode_line_windows: HashSet<WindowId>,
    redisplay_frames: HashSet<FrameId>,
    physical_redraw_frames: HashSet<FrameId>,
    redisplay_text: HashSet<BufferId>,
    last_points: HashMap<WindowId, LispCharPos1>,
    pub(crate) frame_window_change: HashSet<FrameId>,
    pub(crate) dimensions: HashMap<WindowId, HookWindowDimensions>,
    pub(crate) old_selected_frame: Option<FrameId>,
    pub(crate) old_selected_window: Option<WindowId>,
    layout_flow: Option<Flow>,
    echo_geometry_window: Option<WindowId>,
    accepted_serial: u64,
}

const _: () = {
    assert!(
        std::mem::size_of::<RedisplayHookOwnership>()
            == std::mem::size_of::<PreviousBoolHookOwnership>()
    );
    assert!(
        std::mem::align_of::<RedisplayHookOwnership>()
            == std::mem::align_of::<PreviousBoolHookOwnership>()
    );
    assert!(
        std::mem::offset_of!(RedisplayHookOwnership, active)
            == std::mem::offset_of!(PreviousBoolHookOwnership, active)
    );
    assert!(
        std::mem::offset_of!(RedisplayHookOwnership, windows)
            == std::mem::offset_of!(PreviousBoolHookOwnership, windows)
    );
    assert!(
        std::mem::offset_of!(RedisplayHookOwnership, mode_lines)
            == std::mem::offset_of!(PreviousBoolHookOwnership, mode_lines)
    );
    assert!(
        std::mem::offset_of!(RedisplayHookOwnership, redisplay_windows)
            == std::mem::offset_of!(PreviousBoolHookOwnership, redisplay_windows)
    );
    assert!(
        std::mem::offset_of!(RedisplayHookOwnership, mode_line_windows)
            == std::mem::offset_of!(PreviousBoolHookOwnership, mode_line_windows)
    );
    assert!(
        std::mem::offset_of!(RedisplayHookOwnership, redisplay_frames)
            == std::mem::offset_of!(PreviousBoolHookOwnership, redisplay_frames)
    );
    assert!(
        std::mem::offset_of!(RedisplayHookOwnership, physical_redraw_frames)
            == std::mem::offset_of!(PreviousBoolHookOwnership, physical_redraw_frames)
    );
    assert!(
        std::mem::offset_of!(RedisplayHookOwnership, redisplay_text)
            == std::mem::offset_of!(PreviousBoolHookOwnership, redisplay_text)
    );
    assert!(
        std::mem::offset_of!(RedisplayHookOwnership, last_points)
            == std::mem::offset_of!(PreviousBoolHookOwnership, last_points)
    );
    assert!(
        std::mem::offset_of!(RedisplayHookOwnership, frame_window_change)
            == std::mem::offset_of!(PreviousBoolHookOwnership, frame_window_change)
    );
    assert!(
        std::mem::offset_of!(RedisplayHookOwnership, dimensions)
            == std::mem::offset_of!(PreviousBoolHookOwnership, dimensions)
    );
    assert!(
        std::mem::offset_of!(RedisplayHookOwnership, old_selected_frame)
            == std::mem::offset_of!(PreviousBoolHookOwnership, old_selected_frame)
    );
    assert!(
        std::mem::offset_of!(RedisplayHookOwnership, old_selected_window)
            == std::mem::offset_of!(PreviousBoolHookOwnership, old_selected_window)
    );
    assert!(
        std::mem::offset_of!(RedisplayHookOwnership, layout_flow)
            == std::mem::offset_of!(PreviousBoolHookOwnership, layout_flow)
    );
    assert!(
        std::mem::offset_of!(RedisplayHookOwnership, echo_geometry_window)
            == std::mem::offset_of!(PreviousBoolHookOwnership, echo_geometry_window)
    );
    assert!(
        std::mem::offset_of!(RedisplayHookOwnership, accepted_serial)
            == std::mem::offset_of!(PreviousBoolHookOwnership, accepted_serial)
    );
};

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
    tracing::info!(target: "d5::owner_layout", field = "context.gnu_redisplay_hooks", offset = std::mem::offset_of!(Context, gnu_redisplay_hooks), "owner layout metric");
    tracing::info!(target: "d5::owner_layout", field = "context.specpdl", offset = std::mem::offset_of!(Context, specpdl), "owner layout metric");
    tracing::info!(target: "d5::owner_layout", field = "context.jit_bind_stack", offset = std::mem::offset_of!(Context, jit_bind_stack), "owner layout metric");
    tracing::info!(target: "d5::owner_layout", field = "context.depth", offset = std::mem::offset_of!(Context, depth), "owner layout metric");
    tracing::info!(target: "d5::owner_layout", field = "context.max_depth", offset = std::mem::offset_of!(Context, max_depth), "owner layout metric");
    tracing::info!(target: "d5::owner_layout", field = "context.obarray", offset = std::mem::offset_of!(Context, obarray), "owner layout metric");
    tracing::info!(target: "d5::owner_layout", field = "context.jit_stack_limit", offset = std::mem::offset_of!(Context, jit_stack_limit), "owner layout metric");
    tracing::info!(target: "d5::owner_layout", field = "context.jit_stack_scratch", offset = std::mem::offset_of!(Context, jit_stack_scratch), "owner layout metric");
    tracing::info!(target: "d5::owner_layout", field = "context.attention", offset = std::mem::offset_of!(Context, attention), "owner layout metric");
    tracing::info!(target: "d5::owner_layout", field = "context.buffers", offset = std::mem::offset_of!(Context, buffers), "owner layout metric");
    tracing::info!(target: "d5::owner_layout", field = "context.tagged_heap", offset = std::mem::offset_of!(Context, tagged_heap), "owner layout metric");
    let mut eval = Context::new();
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
    let buffer = eval.buffers.current_buffer_id().expect("buffer");
    eval.frames.create_frame("owner-regression", 80, 24, buffer);
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
