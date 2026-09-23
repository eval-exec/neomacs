use super::{
    FrameLeaseId, NativeWpeBufferLease, PendingFrames, ReactorMailbox, RecoveryFrame,
    RecoveryFrames, RenderedWpeBuffer, pick_render_node,
};
use crate::{PixelFrame, WebViewFrame};

#[test]
fn render_node_policy_prefers_non_nvidia_driver_when_a_choice_exists() {
    // Raptor Lake (i915 iGPU) + RTX 4070 Max-Q (nvidia dGPU): WebKit's
    // inference picked the NVIDIA node and crashed; the policy must pick
    // the iGPU node.
    assert_eq!(
        pick_render_node(vec![
            ("renderD128".into(), "nvidia".into()),
            ("renderD129".into(), "i915".into()),
        ]),
        Some("renderD129".into())
    );
}

#[test]
fn render_node_policy_keeps_single_gpu_selection() {
    assert_eq!(
        pick_render_node(vec![("renderD128".into(), "nvidia".into())]),
        Some("renderD128".into())
    );
    assert_eq!(
        pick_render_node(vec![("renderD128".into(), "i915".into())]),
        Some("renderD128".into())
    );
}

#[test]
fn render_node_policy_ignores_unknown_drivers_only_when_alternatives_exist() {
    assert_eq!(
        pick_render_node(vec![
            ("renderD128".into(), String::new()),
            ("renderD129".into(), "i915".into()),
        ]),
        Some("renderD129".into())
    );
}

#[test]
fn frame_mailbox_keeps_only_the_latest_negotiated_frame() {
    let mut frames = PendingFrames::default();
    frames.publish(WebViewFrame::Pixels(PixelFrame::new(vec![1; 4], 1, 1)));
    frames.publish(WebViewFrame::Pixels(PixelFrame::new(vec![2; 4], 1, 1)));

    let Some(WebViewFrame::Pixels(frame)) = frames.take() else {
        panic!("latest pixel frame");
    };
    assert_eq!(frame.pixels(), &[2; 4]);
    assert!(frames.take().is_none());
}

#[test]
fn idle_page_replays_after_each_device_replacement_without_new_damage() {
    let owner = (crate::WebViewId::new(1), crate::WebViewGeneration::new(1));
    let mailbox = ReactorMailbox::default();
    let mut recovery = RecoveryFrames::default();
    recovery.remember(
        owner,
        FrameLeaseId(1),
        RecoveryFrame::Pixels(PixelFrame::new(vec![255, 0, 255, 255], 1, 1)),
    );
    for _ in 0..2 {
        recovery.replay(&mailbox);
        let Some(WebViewFrame::Pixels(frame)) = mailbox.take_frame(owner.0, owner.1) else {
            panic!("static page must be available after cache loss");
        };
        assert_eq!(frame.pixels(), [255, 0, 255, 255]);
    }
    recovery.0.remove(&owner);
    recovery.replay(&mailbox);
    assert!(
        mailbox.take_frame(owner.0, owner.1).is_none(),
        "closed views do not retain recovery frames"
    );
}

#[test]
fn recovery_never_replaces_newer_completed_or_queued_browser_content() {
    let owner = (crate::WebViewId::new(1), crate::WebViewGeneration::new(1));
    let mailbox = ReactorMailbox::default();
    let mut recovery = RecoveryFrames::default();
    for sequence in [2, 1] {
        let accepted = recovery.remember(
            owner,
            FrameLeaseId(sequence),
            RecoveryFrame::Pixels(PixelFrame::new(vec![sequence as u8; 4], 1, 1)),
        );
        assert_eq!(accepted, sequence == 2);
    }
    recovery.replay(&mailbox);
    let Some(WebViewFrame::Pixels(frame)) = mailbox.take_frame(owner.0, owner.1) else {
        panic!("recovery frame");
    };
    assert_eq!(
        frame.pixels(),
        [2; 4],
        "late retirement of frame 1 cannot replace frame 2"
    );
    mailbox.publish_frame(
        owner.0,
        owner.1,
        WebViewFrame::Pixels(PixelFrame::new(vec![3; 4], 1, 1)),
    );
    recovery.replay(&mailbox);
    let Some(WebViewFrame::Pixels(frame)) = mailbox.take_frame(owner.0, owner.1) else {
        panic!("queued frame");
    };
    assert_eq!(frame.pixels(), [3; 4]);
}

#[test]
fn native_wpe_acknowledgement_cannot_cross_threads() {
    static_assertions::assert_not_impl_any!(NativeWpeBufferLease: Send, Sync);
    static_assertions::assert_not_impl_any!(RenderedWpeBuffer: Send, Sync);
}
