//! Context entry points of the per-terminal display registry (remote-display
//! M3; see `protocol/terminals.rs`).

use super::*;
use crate::host_effect::terminal_device::{TerminalDeviceReply, TerminalDeviceRequest};
use neovm_value::display_vocab::FrameId;

impl Context {
    /// Install HOST, which provides both display halves, as the process
    /// display on the initial terminal's window-system connection. This is
    /// the GUI host's and the test hosts' installer (the single slot before
    /// M3).
    pub fn set_display_host(&mut self, host: Box<dyn DisplayHost>) {
        self.display_terminals.install_host(
            TerminalNumber::INITIAL,
            DisplayHostPlacement::WindowSystem,
            host,
        );
        self.display_installed(TerminalNumber::INITIAL);
    }

    /// Install HOST, which provides both display halves, on TERMINAL.
    /// `DisplayHostPlacement::Termcap` makes it the popup and selection half of
    /// a text terminal beside that terminal's device (the primary `-nw` TTY).
    pub fn install_display_host(
        &mut self,
        terminal: TerminalNumber,
        placement: DisplayHostPlacement,
        host: Box<dyn DisplayHost>,
    ) {
        self.display_terminals
            .install_host(terminal, placement, host);
        self.display_installed(terminal);
    }

    /// Install the process display as separate halves: SERVICES, and
    /// BACKING on TERMINAL.
    pub fn install_display(
        &mut self,
        services: Box<dyn DisplayServices>,
        terminal: TerminalNumber,
        backing: TerminalBacking,
    ) {
        self.display_terminals
            .install_split(services, terminal, backing);
        self.display_installed(terminal);
    }

    /// Register BACKING for TERMINAL beside the process display: another
    /// window-system connection, or a text terminal's device. A connection
    /// receives every display setting Lisp has published.
    pub fn register_display_terminal(
        &mut self,
        terminal: TerminalNumber,
        backing: TerminalBacking,
    ) {
        self.display_terminals.set_backing(terminal, backing);
        self.replay_display_settings(terminal);
    }

    /// TERMINAL was deleted: release its native device.
    pub fn remove_display_terminal(&mut self, terminal: TerminalNumber) -> Result<(), Flow> {
        self.tty_device_request(terminal, TerminalDeviceRequest::Release)
            .map(|_released| ())
    }

    /// Perform REQUEST on TERMINAL's text-terminal device.
    ///
    /// The registry that holds every device is main's
    /// (`docs/design/host-effect-projections.md`: native backing stays on
    /// main). A Green worker's own registry is empty, so its request crosses
    /// to main, which performs it on that registry: every owner reaches the
    /// same devices, as every owner did while the devices travelled in the
    /// World-carried terminal manager. Only a routed request can fail as a
    /// request (an interruption or a closed service), with the signal the
    /// requester receives.
    pub fn tty_device_request(
        &mut self,
        terminal: TerminalNumber,
        request: TerminalDeviceRequest,
    ) -> Result<TerminalDeviceReply, Flow> {
        if self.is_green_worker_owner() {
            return self.green_host_terminal_device(terminal, request);
        }
        Ok(self.display_terminals.perform_tty_device(terminal, request))
    }

    fn display_installed(&mut self, terminal: TerminalNumber) {
        self.replay_display_settings(terminal);
        // Cut-4 flip. Only a Green World activates, so the default GUI/TTY
        // path stays on the serial installer until S3. A re-attach keeps the
        // installed arm and uses the current host; a worker is refused.
        match self.activate_host_session() {
            Ok(()) => {}
            Err(super::activation_owner::HostSessionActivationError::NotGreen) => {}
            Err(error) => {
                tracing::warn!(?error, "host session activation refused");
            }
        }
    }

    /// Bring TERMINAL's connection up to date (class 4 replay): the visual
    /// configuration and `ns-confirm-quit` from their evaluator state, then
    /// every setting the registry recorded.
    fn replay_display_settings(&mut self, terminal: TerminalNumber) {
        let visual_config = self.visual_config.clone();
        // A dump or an earlier init form can set the variable before the
        // native host exists. Reattachment must inherit its current value.
        let confirm_quit = self
            .builtin_var_value_by_name("ns-confirm-quit")
            .unwrap_or(Value::NIL)
            .is_truthy();
        if let Some(connection) = self.display_terminals.connection_mut(terminal) {
            let _ = connection.set_visual_config(visual_config);
            connection.set_ns_confirm_quit(confirm_quit);
        }
        self.display_terminals.replay_published(terminal);
    }

    /// The terminal FRAME is on, if FRAME is live.
    pub fn frame_terminal(&self, frame: FrameId) -> Option<TerminalNumber> {
        self.frames
            .get(frame)
            .map(|frame| TerminalNumber::new(frame.terminal_id))
    }

    /// Class 1: the display connection of FRAME's terminal.
    pub fn frame_display_connection_mut(
        &mut self,
        frame: FrameId,
    ) -> Option<&mut (dyn DisplayConnection + 'static)> {
        let terminal = self.frame_terminal(frame)?;
        self.display_terminals.connection_mut(terminal)
    }

    pub fn frame_display_connection(
        &self,
        frame: FrameId,
    ) -> Option<&(dyn DisplayConnection + 'static)> {
        self.display_terminals
            .connection(self.frame_terminal(frame)?)
    }

    /// The selected frame's terminal: the World half of GNU
    /// `check_x_display_info (Qnil)`, which the display registry completes. A
    /// Green worker sends it with its routed display requests, because main
    /// owns the registry and only the requester may read the World.
    pub(crate) fn selected_frame_terminal(&self) -> Option<TerminalNumber> {
        self.frames
            .selected_frame()
            .map(|frame| TerminalNumber::new(frame.terminal_id))
    }

    /// Class 2: the terminal a frame-less display request addresses (GNU
    /// `check_x_display_info (Qnil)`; see `DisplayTerminals::default_terminal`).
    pub fn default_display_terminal(&self) -> Option<TerminalNumber> {
        self.display_terminals
            .default_terminal(self.selected_frame_terminal())
    }

    pub fn default_display_connection_mut(
        &mut self,
    ) -> Option<&mut (dyn DisplayConnection + 'static)> {
        let selected = self.selected_frame_terminal();
        self.display_terminals.default_connection_mut(selected)
    }

    /// The display that shows FRAME's popup menus: FRAME's terminal's display,
    /// else the frame-less default. GNU dispatches menus through the frame's
    /// terminal (`menu_show_hook`); a frame whose terminal has no display keeps
    /// the pre-M3 single-slot answer until M5 routes menus strictly by frame.
    pub fn popup_display_connection_mut(
        &mut self,
        frame: FrameId,
    ) -> Option<&mut (dyn DisplayConnection + 'static)> {
        let terminal = self
            .display_terminals
            .default_terminal(self.frame_terminal(frame))?;
        self.display_terminals.connection_mut(terminal)
    }

    pub fn popup_display_connection(
        &self,
        frame: FrameId,
    ) -> Option<&(dyn DisplayConnection + 'static)> {
        let terminal = self
            .display_terminals
            .default_terminal(self.frame_terminal(frame))?;
        self.display_terminals.connection(terminal)
    }

    /// Class 5: what layout reads for FRAME: the process services and
    /// FRAME's terminal connection.
    pub fn frame_display(&self, frame: FrameId) -> Option<FrameDisplay<'_>> {
        self.display_terminals
            .frame_display(self.frame_terminal(frame))
    }

    pub fn default_display_connection(&self) -> Option<&(dyn DisplayConnection + 'static)> {
        self.display_terminals
            .connection(self.default_display_terminal()?)
    }
}
