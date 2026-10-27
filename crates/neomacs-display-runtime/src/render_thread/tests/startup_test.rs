use super::*;
use crate::render_thread::state::RenderShutdownReason;

#[test]
fn preparation_drains_saturated_channels_and_coalesces_opacity_refreshes() {
    let (sender, receiver) = crossbeam_channel::bounded(COMMAND_CHANNEL_CAPACITY);
    let mut commands = PreparingCommands::default();
    for _ in 0..4 {
        for _ in 0..COMMAND_CHANNEL_CAPACITY {
            sender
                .try_send(RenderCommand::Window(WindowCommand::RefreshFrameOpacity))
                .unwrap();
        }
        assert!(commands.drain(&receiver).is_continue());
    }
    assert!(matches!(
        commands.0.pop_front(),
        Some(RenderCommand::Window(WindowCommand::RefreshFrameOpacity))
    ));
    assert!(commands.0.is_empty());
}

#[test]
fn preparation_preserves_ordered_commands_and_observes_shutdown() {
    let (sender, receiver) = crossbeam_channel::unbounded();
    let mut commands = PreparingCommands::default();
    for title in ["first", "second"] {
        sender
            .send(RenderCommand::Window(WindowCommand::SetWindowTitle {
                title: title.into(),
            }))
            .unwrap();
    }
    assert!(commands.drain(&receiver).is_continue());
    for expected in ["first", "second"] {
        let Some(RenderCommand::Window(WindowCommand::SetWindowTitle { title })) =
            commands.0.pop_front()
        else {
            panic!("ordered title update")
        };
        assert_eq!(title, expected);
    }
    sender
        .send(RenderCommand::Lifecycle(LifecycleCommand::Shutdown))
        .unwrap();
    assert!(commands.drain(&receiver).is_break());
}

#[test]
fn live_gpu_shutdown_preserves_native_event_loop_failure() {
    let mut outcome = Outcome::Running;
    outcome.observe_evaluator_shutdown(true, Some(RenderShutdownReason::EvaluatorShutdown));

    let result = outcome.finish(Err("native dispatch failed"), false);
    assert!(
        matches!(result, Err(RenderLoopError::EventLoop(error)) if error == "native dispatch failed")
    );
}

#[test]
fn gpu_recovery_shutdown_preserves_native_event_loop_failure() {
    // A previously running GPU may be absent after failed device-loss
    // recovery. Shutdown in that state is no longer initial preparation.
    let mut outcome = Outcome::Running;
    outcome.observe_evaluator_shutdown(false, Some(RenderShutdownReason::EvaluatorShutdown));

    let result = outcome.finish(Err("native dispatch failed during device recovery"), false);
    assert!(matches!(result, Err(RenderLoopError::EventLoop(error))
            if error == "native dispatch failed during device recovery"));
}

#[test]
fn pending_gpu_evaluator_shutdown_finishes_gracefully() {
    let mut outcome = Outcome::Preparing;
    outcome.observe_evaluator_shutdown(false, Some(RenderShutdownReason::EvaluatorShutdown));

    assert!(matches!(
        outcome.finish(Err("native loop ended during evaluator exit"), false),
        Ok(RenderLoopExit::Finished)
    ));
}
