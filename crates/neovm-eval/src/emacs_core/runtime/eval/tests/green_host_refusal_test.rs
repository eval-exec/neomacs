//! C4c entry-condition rows: hosted Green start is refused for a display
//! host, an input receiver, or both. The refusal stays in C4b; these rows pin
//! its three shapes so C4c removes exactly one behavior.

use super::GreenStartError;
use crate::emacs_core::eval::host_input::HostInputReceiver;
use crate::emacs_core::eval::{Context, ThreadingMode};
use neomacs_display_protocol::SelectionOwner;

struct NullDisplayHost;

impl crate::emacs_core::eval::protocol::DisplayHost for NullDisplayHost {
    fn realize_gui_frame(
        &mut self,
        _request: crate::emacs_core::eval::GuiFrameHostRequest,
    ) -> Result<(), String> {
        Ok(())
    }

    fn resize_gui_frame(
        &mut self,
        _request: crate::emacs_core::eval::GuiFrameHostRequest,
    ) -> Result<(), String> {
        Ok(())
    }

    fn set_clipboard_text(&mut self, _text: Option<&str>) -> Result<(), String> {
        Ok(())
    }

    fn clipboard_text(&mut self) -> Result<Option<String>, String> {
        Ok(None)
    }

    fn set_primary_selection_text(&mut self, _text: Option<&str>) -> Result<(), String> {
        Ok(())
    }

    fn primary_selection_text(&mut self) -> Result<Option<String>, String> {
        Ok(None)
    }

    fn primary_selection_owner(&mut self) -> Result<SelectionOwner, String> {
        Ok(SelectionOwner::None)
    }
}

fn green_context() -> Context {
    Context::new_with_threading(ThreadingMode::Green, crate::test_utils::unit_runtime())
}

fn detached_input() -> HostInputReceiver {
    let (_publisher, receiver) = crossbeam_channel::unbounded();
    HostInputReceiver::from_receiver(receiver)
}

#[test]
fn hosted_green_start_refuses_display_input_or_both() {
    // Construct every Context before any Green runtime captures the subr
    // tables (bootstrap's debug assertion).
    let mut display_only = green_context();
    display_only.set_display_host(Box::new(NullDisplayHost));
    let mut input_only = green_context();
    input_only.attach_input_receiver(detached_input());
    let mut both = green_context();
    both.set_display_host(Box::new(NullDisplayHost));
    both.attach_input_receiver(detached_input());

    for (name, ctx) in [
        ("display-only", &mut display_only),
        ("input-only", &mut input_only),
        ("display-and-input", &mut both),
    ] {
        match ctx.start_green_runtime() {
            Err(GreenStartError::ThreadAffineHost) => {}
            Err(other) => panic!("{name}: wrong refusal: {other:?}"),
            Ok(_) => panic!("{name}: hosted Green start was admitted"),
        }
    }
}
