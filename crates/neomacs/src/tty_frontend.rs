use std::io;
use std::io::Write;

use neomacs_display_protocol::SelectionOwner;
use neomacs_display_runtime::thread_comm::{LifecycleCommand, RenderCommand, RenderCommandSender};
use neovm_core::emacs_core::{
    DisplayConnection, DisplayServices, GuiFrameHostRequest, PopupMenuRequest,
};

// Re-export the cross-platform TTY input reader from display-runtime.
pub use neomacs_display_runtime::tty_input::TtyInputReader;

// ── TTY terminal host (suspend/resume/delete) ────────────────────────────

/// Implements `TerminalHost` for the TTY frontend so the Lisp-level
/// `suspend-tty`, `resume-tty`, and `delete-terminal` functions can
/// send commands to the render thread.
pub struct TtyTerminalHost {
    cmd_tx: RenderCommandSender,
}

impl TtyTerminalHost {
    #[inline(always)]
    pub(crate) fn new(cmd_tx: RenderCommandSender) -> Self {
        Self { cmd_tx }
    }
}

/// The primary TTY's display half: popup menus drawn on this terminal and the
/// Lisp-visible selection refusals. It sits beside the TTY's device in the
/// initial terminal's termcap backing (remote-display M3).
///
/// Popup I/O is separate from the inherited absent geometry/image/font
/// services. `frame_layout::tty_rows_host_authority` audits those services for
/// every instance state; adding a geometry service must revisit that contract.
pub struct TtyPopupDisplayHost {
    force_full_redraw: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl TtyPopupDisplayHost {
    pub fn new(force_full_redraw: std::sync::Arc<std::sync::atomic::AtomicBool>) -> Self {
        Self { force_full_redraw }
    }
}

impl DisplayConnection for TtyPopupDisplayHost {
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
        use neomacs_display_protocol::menu::MenuItemKind;

        let mut stdout = io::stdout();
        let origin = menu
            .placement
            .preferred_origin(neomacs_display_protocol::Size::ZERO);
        // GNU `tty_menu_show` clamps the anchor to the first usable row and
        // column (src/term.c:4053-4055), then `tty_menu_activate`
        // (src/term.c:3594-3599) draws the pane name one row above the first
        // item: a menu-bar drop-down marks its heading on the menu-bar row
        // ("Help >") and starts its items on the row below.
        let first_row = (origin.y.max(0.0) as usize).max(1);
        let first_col = (origin.x.max(0.0) as usize).max(1);
        let visible_rows = self
            .popup_menu_visible_rows(origin.x, origin.y, menu.entries.len())
            .unwrap_or(menu.entries.len())
            .min(menu.entries.len());
        let entries = &menu.entries[..visible_rows];

        // `tty_menu_show` pads every item name in the pane to the widest
        // name, then appends the key-equivalent description, which GNU
        // prefixes with two spaces (src/keyboard.c:9093-9100,
        // src/term.c:3995-4020).  A TTY submenu's name already carries its
        // " >" marker (src/menu.c:407-413).  The pane width is therefore the
        // widest padded name plus description; names without one are not
        // padded, so they only matter through the widest-name measure.
        let name_width = entries
            .iter()
            .map(|entry| entry.label.chars().count())
            .max()
            .unwrap_or(0);
        let menu_width = entries
            .iter()
            .map(|entry| {
                if entry.shortcut.is_empty() {
                    entry.label.chars().count()
                } else {
                    name_width + 2 + entry.shortcut.chars().count()
                }
            })
            .max()
            .unwrap_or(0);

        // GNU `display_tty_menu_item` (src/xdisp.c:27947-28025) writes one
        // leading space, the padded item, and pads the row to the pane width.
        let write_row =
            |row: usize, content: &str, width: usize, inverse: bool| -> Result<(), String> {
                super::tty_output::popup_line(
                    row,
                    first_col - 1,
                    &format!(" {content:<width$}"),
                    inverse,
                )
                .map_err(|err| format!("failed to render TTY popup menu: {err}"))
            };

        // GNU draws the menu title with `tty-menu-selected-face` always
        // (src/term.c:3564-3567); it is one pane name wide, not pane-wide.
        if let Some(title) = menu.title.as_deref() {
            write_row(first_row - 1, title, title.chars().count() + 1, true)?;
        }
        for (idx, entry) in entries.iter().enumerate() {
            let selected = idx == menu.selected;
            let content = match entry.kind {
                MenuItemKind::Separator => {
                    let glyph = match entry.label.as_str() {
                        "--space" => ' ',
                        "--double-line" => '=',
                        _ => '-',
                    };
                    std::iter::repeat_n(glyph, menu_width).collect()
                }
                _ if entry.shortcut.is_empty() => entry.label.clone(),
                _ => format!(
                    "{:<name_width$}  {}",
                    entry.label,
                    entry.shortcut,
                    name_width = name_width
                ),
            };
            write_row(first_row + idx, &content, menu_width + 1, selected)?;
        }
        stdout
            .flush()
            .map_err(|err| format!("failed to flush TTY popup menu: {err}"))
    }

    fn popup_menu_visible_rows(&self, _x: f32, y: f32, entry_count: usize) -> Option<usize> {
        let (_, rows) = super::tty_init::query_terminal_size_cells()?;
        // GNU tty_menu_display keeps one screen line (the echo area) free and
        // starts at the clamped first-item row (src/term.c:3227-3230).
        let first_popup_row = (y.max(0.0) as usize).max(1);
        let visible = (rows as usize)
            .saturating_sub(first_popup_row)
            .saturating_sub(1);
        Some(visible.min(entry_count))
    }

    fn hide_popup_menu(
        &mut self,
        _token: neomacs_display_protocol::menu::MenuToken,
    ) -> Result<(), String> {
        self.force_full_redraw
            .store(true, std::sync::atomic::Ordering::Release);
        Ok(())
    }
}

/// Every font, image and geometry service keeps its inert trait default.
impl DisplayServices for TtyPopupDisplayHost {}

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

    // GNU writes `tty->output' directly (src/dispnew.c:6838-6843); here that
    // stream is stdout, and this builtin already runs on the same evaluator
    // thread that performs every other TtyRif stdout write, so ordering with
    // redisplay output is inherent.  Same precedent as the popup-menu path.
    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), String> {
        use std::io::Write as _;
        let mut stdout = io::stdout();
        stdout
            .write_all(bytes)
            .and_then(|()| stdout.flush())
            .map_err(|err| format!("failed to write to the terminal: {err}"))
    }
}
