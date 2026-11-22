//! The renderer's exit retirement routine, driven through its real entry
//! point with an acknowledgement queued and the evaluator's sender clone
//! alive. The row runs in an owned child process so a regression fails by
//! timeout instead of leaving a hung thread inside the test binary.

use crate::render_thread::{
    ImageRenderState, RenderExitCancellation, RendererPhase, SharedImageRenderState,
};
use crate::thread_comm::{AssetCommand, RenderCommand};
use std::sync::Arc;

const CHILD_ENV: &str = "NEOMACS_RETIRE_OBLIGATIONS_CHILD";
const TEST_NAME: &str = "tests::exit_cancellation_test::exit_cancellation_retires_a_queued_ack";

#[test]
fn exit_cancellation_retires_a_queued_ack() {
    if std::env::var_os(CHILD_ENV).is_some() {
        child_body();
        return;
    }
    let exe = std::env::current_exe().expect("test binary path");
    let mut child = std::process::Command::new(exe)
        .args(["--exact", TEST_NAME, "--nocapture"])
        .env(CHILD_ENV, "1")
        .spawn()
        .expect("spawn the owned child");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        match child.try_wait().expect("child status") {
            Some(status) => {
                assert!(status.success(), "retirement child failed: {status}");
                return;
            }
            None if std::time::Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("retiring a queued acknowledgement hung; child killed after 10s");
            }
            None => std::thread::sleep(std::time::Duration::from_millis(10)),
        }
    }
}

fn child_body() {
    let image_metadata: SharedImageRenderState = Arc::new(ImageRenderState::default());
    image_metadata.mark_renderer_ready();
    let (command_tx, command_rx) = crossbeam_channel::bounded(8);
    // The evaluator keeps its sender clone: that is exactly why the buffered
    // payload (and its ack sender) survives a receiver drop. The test also
    // keeps a receiver clone so the acks can only be released by the guard's
    // drain, not by the channel being torn down.
    let _evaluator_sender = command_tx.clone();
    let retained_rx = command_rx.clone();
    let (first_ack, first_wait) = std::sync::mpsc::sync_channel(1);
    let (second_ack, second_wait) = std::sync::mpsc::sync_channel(1);
    for ack in [first_ack, second_ack] {
        command_tx
            .send(RenderCommand::Asset(AssetCommand::PublishImageCacheUsage {
                ack,
            }))
            .expect("queue a publish");
    }
    // The real event-loop exit wiring: dropping the RAII guard must publish
    // the exit and drain the raw queue.
    let guard = RenderExitCancellation {
        image_metadata: Arc::clone(&image_metadata),
        commands: command_rx,
    };
    drop(guard);
    assert_eq!(
        image_metadata.renderer_phase(),
        RendererPhase::Exited,
        "the exit guard did not publish the renderer phase"
    );
    for (name, wait) in [("first", first_wait), ("second", second_wait)] {
        assert!(
            matches!(
                wait.recv_timeout(std::time::Duration::from_secs(1)),
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected)
            ),
            "the {name} buffered ack sender was not dropped by the exit drain"
        );
    }
    assert!(
        retained_rx.try_recv().is_err(),
        "the exit guard left a queued command behind"
    );
}
