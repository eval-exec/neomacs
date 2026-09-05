use std::io;
use std::io::Write;

use neomacs_display_protocol::SelectionOwner;
use neomacs_display_runtime::thread_comm::{LifecycleCommand, RenderCommand};
use neovm_core::emacs_core::{DisplayHost, GuiFrameHostRequest, PopupMenuRequest};

// Re-export the cross-platform TTY input reader from display-runtime.
pub use neomacs_display_runtime::tty_input::TtyInputReader;

// ── TTY terminal host (suspend/resume/delete) ────────────────────────────

/// Implements `TerminalHost` for the TTY frontend so the Lisp-level
/// `suspend-tty`, `resume-tty`, and `delete-terminal` functions can
/// send commands to the render thread.
pub struct TtyTerminalHost {
    pub cmd_tx: crossbeam_channel::Sender<RenderCommand>,
}

pub struct TtyPopupDisplayHost {
    force_full_redraw: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl TtyPopupDisplayHost {
    pub fn new(force_full_redraw: std::sync::Arc<std::sync::atomic::AtomicBool>) -> Self {
        Self { force_full_redraw }
    }
}

impl DisplayHost for TtyPopupDisplayHost {
    fn realize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn resize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn set_clipboard_text(&mut self, _text: Option<&str>) -> Result<(), String> {
        Err("system clipboard is unavailable in TTY mode".to_owned())
    }

    fn clipboard_text(&mut self) -> Result<Option<String>, String> {
        Err("system clipboard is unavailable in TTY mode".to_owned())
    }

    fn set_primary_selection_text(&mut self, _text: Option<&str>) -> Result<(), String> {
        Err("PRIMARY selection is unavailable in TTY mode".to_owned())
    }

    fn primary_selection_text(&mut self) -> Result<Option<String>, String> {
        Err("PRIMARY selection is unavailable in TTY mode".to_owned())
    }

    fn primary_selection_owner(&mut self) -> Result<SelectionOwner, String> {
        Err("PRIMARY selection is unavailable in TTY mode".to_owned())
    }

    fn show_popup_menu(&mut self, menu: PopupMenuRequest) -> Result<(), String> {
        let mut stdout = io::stdout();
        let origin = menu
            .placement
            .preferred_origin(neomacs_display_protocol::Size::ZERO);
        let row = origin.y.max(0.0) as usize + 2;
        let col = origin.x.max(0.0) as usize + 1;
        let visible_rows = self
            .popup_menu_visible_rows(origin.x, origin.y, menu.entries.len())
            .unwrap_or(menu.entries.len());
        for (idx, entry) in menu.entries.iter().take(visible_rows).enumerate() {
            let marker = if idx == menu.selected { ">" } else { " " };
            write!(
                stdout,
                "\x1b[{};{}H\x1b[7m{} {}\x1b[0m",
                row + idx,
                col,
                marker,
                entry.label
            )
            .map_err(|err| format!("failed to render TTY popup menu: {err}"))?;
        }
        stdout
            .flush()
            .map_err(|err| format!("failed to flush TTY popup menu: {err}"))
    }

    fn popup_menu_visible_rows(&self, _x: f32, y: f32, entry_count: usize) -> Option<usize> {
        let (_, rows) = super::tty_init::query_terminal_size_cells()?;
        let first_popup_row = y.max(0.0) as usize + 2;
        let visible = (rows as usize).saturating_sub(first_popup_row);
        Some(visible.min(entry_count))
    }

    fn hide_popup_menu(&mut self) -> Result<(), String> {
        self.force_full_redraw
            .store(true, std::sync::atomic::Ordering::Release);
        Ok(())
    }
}

impl neovm_core::emacs_core::terminal::pure::TerminalHost for TtyTerminalHost {
    fn suspend_tty(&mut self) -> Result<(), String> {
        self.cmd_tx
            .send(RenderCommand::Lifecycle(LifecycleCommand::SuspendTty))
            .map_err(|err| format!("failed to suspend tty frontend: {err}"))
    }

    fn resume_tty(&mut self) -> Result<(), String> {
        self.cmd_tx
            .send(RenderCommand::Lifecycle(LifecycleCommand::ResumeTty))
            .map_err(|err| format!("failed to resume tty frontend: {err}"))
    }

    fn delete_terminal(&mut self) -> Result<(), String> {
        self.cmd_tx
            .send(RenderCommand::Lifecycle(LifecycleCommand::Shutdown))
            .map_err(|err| format!("failed to delete tty terminal frontend: {err}"))
    }
}
