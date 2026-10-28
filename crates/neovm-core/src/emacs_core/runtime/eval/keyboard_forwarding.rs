//! Selected-terminal keyboard forwarding and graphical connection lifetimes.

use super::{Context, Value, intern};

impl Context {
    /// Synchronize keyboard-owned state after the display host assigns a
    /// terminal to the selected frame. Ordinary Lisp frame selection already
    /// performs this step; reselecting the same frame intentionally does not.
    pub fn sync_keyboard_terminal_owner(&mut self) {
        let terminal_id = self
            .frames
            .selected_frame()
            .map(|frame| frame.terminal_id)
            .unwrap_or(crate::emacs_core::terminal::pure::TERMINAL_ID);
        let window_system = self
            .frames
            .selected_frame()
            .and_then(|frame| frame.effective_window_system())
            .unwrap_or(Value::NIL);
        if let Some(forwarder) = self.window_system_keyboard_forwarder() {
            forwarder.select_keyboard(terminal_id, window_system);
        }
        self.command_loop.keyboard.select_terminal(terminal_id);
    }

    /// Establish a fresh graphical connection's keyboard before Lisp creates
    /// its first frame. GNU's backend init_kboard sets Vwindow_system here;
    /// a cached initial TTY keyboard is a different lifetime even at id zero.
    pub fn initialize_graphical_terminal(
        &mut self,
        identity: neomacs_display_protocol::GraphicalDisplayIdentity,
    ) {
        use crate::emacs_core::terminal::pure::{
            TERMINAL_ID, TerminalRuntimeConfig, configure_terminal_runtime,
        };
        configure_terminal_runtime(TerminalRuntimeConfig::window_system(identity));
        if let Some(forwarder) = self.window_system_keyboard_forwarder() {
            forwarder.forget_keyboard(TERMINAL_ID);
            forwarder.select_keyboard(
                TERMINAL_ID,
                Value::symbol(crate::emacs_core::display::gui_window_system_symbol()),
            );
        }
    }

    pub(crate) fn window_system_keyboard_forwarder(
        &self,
    ) -> Option<&'static crate::emacs_core::forward::LispKboardObjFwd> {
        self.obarray
            .forwarder(intern("window-system"))?
            .as_kboard_obj_fwd()
    }

    pub(crate) fn forget_window_system_keyboard(&self, terminal: u64) {
        if let Some(forwarder) = self.window_system_keyboard_forwarder() {
            forwarder.forget_keyboard(terminal);
        }
    }
}
