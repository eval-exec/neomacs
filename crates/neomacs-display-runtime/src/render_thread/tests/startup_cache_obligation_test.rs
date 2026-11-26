//! Parked command ownership, using the production queue owners and handlers.
//!
//! These CPU-only controls do not run StartingApp's native callbacks, GPU
//! startup, or the upper image catalog. In particular, a headless usage publish
//! proves handler/acknowledgement progress, not GPU-cache clear accounting.

use super::*;
use crate::thread_comm::{AssetCommand, EmacsComms, RenderCommandSender, ThreadComms};
use std::num::NonZeroUsize;
use std::sync::{Arc, Condvar, Mutex, mpsc};

fn commands() -> (RenderCommandSender, Receiver<RenderCommand>) {
    RenderCommandSender::bounded(NonZeroUsize::new(COMMAND_CHANNEL_CAPACITY).unwrap())
}

fn clear() -> RenderCommand {
    // These parking controls submit no CPU image jobs before the clear.
    RenderCommand::Asset(AssetCommand::ClearImageCache { through: 0 })
}

fn publish(ack: mpsc::SyncSender<()>) -> RenderCommand {
    RenderCommand::Asset(AssetCommand::PublishImageCacheUsage { ack })
}

fn title(value: &str) -> RenderCommand {
    RenderCommand::Window(WindowCommand::SetWindowTitle {
        title: value.to_owned(),
    })
}

fn headless_app() -> (RenderApp, EmacsComms) {
    let (emacs, render) = ThreadComms::new().split();
    let app = RenderApp::new(
        render,
        800,
        600,
        "startup cache ownership".to_owned(),
        Arc::new(crate::render_thread::ImageRenderState::default()),
        Arc::new((Mutex::new(Vec::new()), Condvar::new())),
        false,
        #[cfg(feature = "neo-term")]
        crate::terminal::new_shared_terminals(),
    );
    assert!(app.renderer.is_none());
    (app, emacs)
}

#[test]
fn preparing_queue_parks_clear_and_publish_in_fifo_until_owner_drop() {
    let (sender, receiver) = commands();
    let retained_sender = sender.clone();
    let (ack, reply) = mpsc::sync_channel(1);
    sender.send(clear()).unwrap();
    sender.send(publish(ack)).unwrap();

    let mut parked = PreparingCommands::default();
    assert!(parked.drain(&receiver).is_continue());
    assert!(receiver.is_empty());
    assert_eq!(parked.0.len(), 2);
    assert!(matches!(
        parked.0.front(),
        Some(RenderCommand::Asset(AssetCommand::ClearImageCache {
            through: 0
        }))
    ));
    assert!(matches!(
        parked.0.back(),
        Some(RenderCommand::Asset(
            AssetCommand::PublishImageCacheUsage { .. }
        ))
    ));
    assert_eq!(reply.try_recv(), Err(mpsc::TryRecvError::Empty));

    drop(parked);
    assert_eq!(reply.try_recv(), Err(mpsc::TryRecvError::Disconnected));
    // Cancellation came from the parked owner, not command-channel teardown.
    retained_sender.try_send(title("still connected")).unwrap();
}

#[test]
fn preparation_shutdown_leaves_parked_reply_owned_until_phase_retirement() {
    let (sender, receiver) = commands();
    let retained_sender = sender.clone();
    let (ack, reply) = mpsc::sync_channel(1);
    sender.send(clear()).unwrap();
    sender.send(publish(ack)).unwrap();
    sender
        .send(RenderCommand::Lifecycle(LifecycleCommand::Shutdown))
        .unwrap();

    let mut parked = PreparingCommands::default();
    assert!(parked.drain(&receiver).is_break());
    assert_eq!(parked.0.len(), 2);
    assert!(receiver.is_empty());
    assert_eq!(reply.try_recv(), Err(mpsc::TryRecvError::Empty));

    // StartingApp retires its Preparing phase after this production break.
    // Exercise the queue owner's retirement, not a fabricated success ack.
    drop(parked);
    assert_eq!(reply.try_recv(), Err(mpsc::TryRecvError::Disconnected));
    retained_sender.try_send(title("still connected")).unwrap();
}

#[test]
fn preparing_queue_unwind_retires_parked_reply_with_sender_clones_alive() {
    let (sender, receiver) = commands();
    let retained_sender = sender.clone();
    let (ack, reply) = mpsc::sync_channel(1);
    sender.send(clear()).unwrap();
    sender.send(publish(ack)).unwrap();

    let mut parked = PreparingCommands::default();
    assert!(parked.drain(&receiver).is_continue());
    assert_eq!(reply.try_recv(), Err(mpsc::TryRecvError::Empty));
    let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let _owned_until_unwind = parked;
        panic!("owned preparation queue unwind control");
    }));
    let failure = failure.expect_err("the controlled unwind must execute");
    assert_eq!(
        failure.downcast_ref::<&str>(),
        Some(&"owned preparation queue unwind control")
    );
    assert!(receiver.is_empty());
    assert_eq!(reply.try_recv(), Err(mpsc::TryRecvError::Disconnected));
    retained_sender.try_send(title("still connected")).unwrap();
}

#[test]
fn parked_cache_prefix_is_serviced_before_raw_commands_by_real_app() {
    let (mut app, emacs) = headless_app();
    let retained_sender = emacs.cmd_tx.clone();
    let (ack, reply) = mpsc::sync_channel(1);

    // Use the actual pre-window parking owner and its startup handoff shape.
    emacs.cmd_tx.send(title("parked prefix")).unwrap();
    emacs.cmd_tx.send(clear()).unwrap();
    emacs.cmd_tx.send(publish(ack)).unwrap();
    let mut parked = PreparingCommands::default();
    assert!(parked.drain(&app.comms.cmd_rx).is_continue());
    app.startup_commands.append(&mut parked.0);
    assert!(parked.0.is_empty());
    assert_eq!(reply.try_recv(), Err(mpsc::TryRecvError::Empty));

    retained_sender.send(title("raw suffix")).unwrap();
    retained_sender
        .send(RenderCommand::Lifecycle(LifecycleCommand::Shutdown))
        .unwrap();
    assert!(app.process_commands());
    assert!(app.startup_commands.is_empty());
    assert!(app.comms.cmd_rx.is_empty());
    assert_eq!(reply.try_recv(), Ok(()));
    assert_eq!(reply.try_recv(), Err(mpsc::TryRecvError::Disconnected));
    // Reversing the parked/raw drain order would leave "parked prefix" here.
    assert_eq!(
        app.frame_windows.primary_window().unwrap().chrome().title,
        "raw suffix"
    );
}

#[test]
fn handle_exiting_retires_gpu_pending_parked_reply_before_app_drop() {
    let (mut app, emacs) = headless_app();
    let retained_sender = emacs.cmd_tx.clone();
    let (ack, reply) = mpsc::sync_channel(1);
    app.startup_commands.push_back(clear());
    app.startup_commands.push_back(publish(ack));
    assert_eq!(reply.try_recv(), Err(mpsc::TryRecvError::Empty));

    app.handle_exiting();
    assert!(app.startup_commands.is_empty());
    assert_eq!(reply.try_recv(), Err(mpsc::TryRecvError::Disconnected));
    // The actual app/receiver and caller's endpoints survived the assertion.
    // Do not require new command admission after terminal cancellation.
    drop(retained_sender);
    drop(emacs);
}

#[test]
fn dropping_real_app_retires_parked_reply_with_command_endpoints_retained() {
    let (mut app, emacs) = headless_app();
    let retained_sender = emacs.cmd_tx.clone();
    let retained_receiver = app.comms.cmd_rx.clone();
    let (ack, reply) = mpsc::sync_channel(1);
    app.startup_commands.push_back(clear());
    app.startup_commands.push_back(publish(ack));
    assert_eq!(reply.try_recv(), Err(mpsc::TryRecvError::Empty));

    drop(app);
    assert_eq!(reply.try_recv(), Err(mpsc::TryRecvError::Disconnected));
    // Endpoint destruction cannot be the reason the parked ack was retired.
    drop(retained_sender);
    drop(retained_receiver);
    drop(emacs);
}
