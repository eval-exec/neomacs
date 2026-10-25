//! The TTY realization pin carries the frame's realized faces by shared handle:
//! re-adopting it on the next redisplay must be a refcount bump, not a copy.
use super::*;
use neovm_core::emacs_core::terminal::pure;
use std::sync::Arc;

struct RestoreTerminal;
impl Drop for RestoreTerminal {
    fn drop(&mut self) {
        pure::reset_terminal_host();
        pure::reset_terminal_runtime();
    }
}

#[test]
fn tty_realization_pin_shares_realized_faces_across_redisplays() {
    let mut context = Context::new();
    let _restore = RestoreTerminal;
    context.set_gc_threshold(usize::MAX);
    pure::configure_terminal_runtime(pure::TerminalRuntimeConfig::interactive(
        Some("xterm-256color".to_owned()),
        neomacs_display_protocol::tty_capabilities::TtyAttributeCapabilities::full_with_color_cells(
            256,
        ),
    ));
    let source = context.buffer_manager().current_buffer().unwrap().id();
    let frame = context
        .frame_manager_mut()
        .create_frame("tty-realization-pin", 80, 24, source);
    context.sync_runtime_faces_for_frame(frame);
    let mut resolver = FaceResolver::new(context.face_table(), 0x00ffffff, 0, 14.0, None);
    resolver.prepare_tty_realization(frame, &mut context, None);
    assert!(resolver.has_tty_realization());

    let pin = resolver
        .tty_realization_pin()
        .expect("bound realization pin");
    let default_ptr = Arc::as_ptr(&pin.default_face);
    let faces_ptr = Arc::as_ptr(&pin.basic_faces);

    // A fresh transaction resolver adopts the pin's realized faces.
    let mut next = FaceResolver::new(context.face_table(), 0x00ffffff, 0, 14.0, None);
    next.prepare_tty_realization(frame, &mut context, Some(pin));
    assert_eq!(Arc::as_ptr(&next.default_face), default_ptr);
    assert_eq!(
        Arc::as_ptr(next.realized_basic_faces.as_ref().expect("adopted map")),
        faces_ptr
    );

    // Re-pinning hands the same allocations to the following transaction.
    let repin = next.tty_realization_pin().expect("re-pinned");
    assert_eq!(Arc::as_ptr(&repin.default_face), default_ptr);
    assert_eq!(Arc::as_ptr(&repin.basic_faces), faces_ptr);
}
