//! Shared display-crate prelude and fixtures for the re-homed host tests.
//!
//! The glob re-export mirrors the old `display::display` parent module, so a
//! moved file's `use super::*` and its `super::super::builtin_x_popup_menu`
//! spellings resolve exactly as they did in display.

pub use crate::window::{DisplayFrameExt, DisplayFrameManagerExt, DisplayWindowExt};
pub use crate::window::{FrameFullscreen, FrameParam, FrameVisibility};
pub use neovm_core::emacs_core::display::*;
pub use neovm_core::emacs_core::window_cmds::builtin_x_create_frame;
pub use neovm_core::emacs_core::xfaces::{
    builtin_xw_color_defined_p_ctx, builtin_xw_color_values_ctx,
};
pub use neovm_display::test_utils::window_cmds_host::{
    RecordingDisplayHost, due_gnu_timer, remapped_mono_font_metrics, resolved_frame_font,
};
pub use neovm_eval::emacs_core::eval::{FontPxProbeResult, GuiFrameHostSize, ResolvedFrameFont};
pub use neovm_system::emacs_core::timer::TimerContextExt;

pub use neovm_eval::emacs_core::eval::{
    Context, DisplayHost, GuiFrameHostRequest, PopupMenuEntry, PopupMenuRequest,
};
pub use neovm_value::error::{EvalError, EvalResult, Flow, FlowKind, FlowRef, FlowResultExt as _};
pub use neovm_value::intern::intern;
pub use neovm_value::value::{Value, list_to_vec};
pub use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub struct RecordingPopupHost {
    shown: Arc<Mutex<Vec<PopupMenuRequest>>>,
    hidden: Arc<Mutex<usize>>,
}

impl DisplayHost for RecordingPopupHost {
    fn realize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn resize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn show_popup_menu(&mut self, menu: PopupMenuRequest) -> Result<(), String> {
        self.shown.lock().unwrap().push(menu);
        Ok(())
    }

    fn hide_popup_menu(
        &mut self,
        _token: neomacs_display_protocol::menu::MenuToken,
    ) -> Result<(), String> {
        *self.hidden.lock().unwrap() += 1;
        Ok(())
    }
}

/// The display fixture entry: the public x-popup-menu entry, which takes the
/// interactive dispatch when a display host and input receiver are installed
/// (the fixture always installs both and queues a MenuSelection).
pub fn builtin_x_popup_menu(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    crate::emacs_core::display::builtin_x_popup_menu(eval, args)
}

#[path = "display_suite/inner.rs"]
pub mod inner;
