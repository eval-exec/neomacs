use std::io;
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use neomacs_display_protocol::SelectionOwner;
use neomacs_display_runtime::thread_comm::{LifecycleCommand, RenderCommand, RenderCommandSender};
use neovm_core::emacs_core::{DisplayHost, GuiFrameHostRequest, PopupMenuRequest};

// Re-export the cross-platform TTY input reader and pause handle from display-runtime.
pub use neomacs_display_runtime::tty_input::{TtyInputPause, TtyInputReader};

// ── TTY terminal host (suspend/resume/delete/emergency-escape) ───────────

/// Implements `TerminalHost` for the TTY frontend so the Lisp-level
/// `suspend-tty`, `resume-tty`, and `delete-terminal` functions can
/// send commands to the render thread, and so controlling-TTY 2nd-`C-g`
/// emergency escape can pause raw input, suspend via `SIGTSTP`, run the
/// GNU resume prompts, and re-enter raw mode.
pub struct TtyTerminalHost {
    cmd_tx: RenderCommandSender,
    input_pause: TtyInputPause,
    force_full_redraw: Arc<AtomicBool>,
}

impl TtyTerminalHost {
    #[cfg(test)]
    #[inline(always)]
    pub(crate) fn new(cmd_tx: RenderCommandSender) -> Self {
        Self::with_emergency_escape(
            cmd_tx,
            TtyInputPause::new(),
            Arc::new(AtomicBool::new(false)),
        )
    }

    #[inline(always)]
    pub(crate) fn with_emergency_escape(
        cmd_tx: RenderCommandSender,
        input_pause: TtyInputPause,
        force_full_redraw: Arc<AtomicBool>,
    ) -> Self {
        Self {
            cmd_tx,
            input_pause,
            force_full_redraw,
        }
    }
}

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

impl neovm_core::emacs_core::terminal::pure::TerminalHost for TtyTerminalHost {
    fn suspend_tty(&mut self, reset_strings: &[u8]) -> Result<(), String> {
        self.cmd_tx
            .send(RenderCommand::Lifecycle(LifecycleCommand::SuspendTty))
            .map_err(|err| format!("failed to suspend tty frontend: {err}"))?;
        super::tty_init::tty_suspend_terminal(reset_strings);
        Ok(())
    }

    fn resume_tty(&mut self, set_strings: &[u8]) -> Result<(), String> {
        super::tty_init::tty_resume_terminal(set_strings)?;
        self.force_full_redraw
            .store(true, std::sync::atomic::Ordering::Release);
        self.cmd_tx
            .send(RenderCommand::Lifecycle(LifecycleCommand::ResumeTty))
            .map_err(|err| format!("failed to resume tty frontend: {err}"))
    }

    fn delete_terminal(&mut self, reset_strings: &[u8]) -> Result<(), String> {
        self.cmd_tx
            .send(RenderCommand::Lifecycle(LifecycleCommand::Shutdown))
            .map_err(|err| format!("failed to delete tty terminal frontend: {err}"))?;
        super::tty_init::tty_shutdown_terminal_with(reset_strings);
        Ok(())
    }

    fn sys_suspend(
        &mut self,
        request: neovm_core::emacs_core::terminal::pure::SuspendRequest<'_>,
    ) -> Result<(), String> {
        super::tty_init::tty_sys_suspend(request.stuffstring, request.cannot_suspend, request.cwd)
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

    fn run_emergency_escape(
        &mut self,
        ops: &mut dyn neovm_core::emacs_core::terminal::pure::EmergencyEscapeOps,
    ) -> bool {
        let _pause_guard = self.input_pause.pause();
        #[cfg(unix)]
        let _sigint_guard = SigintBlockGuard::block();

        let _ = io::stdout().flush();
        // Scope note: resets/re-initializes only the primary controlling TTY
        // (GNU's `reset_all_sys_modes`/`init_all_sys_modes` also cover
        // secondary TTYs).
        super::tty_init::tty_shutdown_terminal();
        let _ = io::stdout().flush();

        #[cfg(unix)]
        let mut suspend = sys_suspend_process_group;
        run_emergency_escape_session(
            read_stdin_unbuffered_byte,
            write_stdout_unbuffered,
            #[cfg(unix)]
            Some(&mut suspend),
            #[cfg(not(unix))]
            None,
            |action| match action {
                EmergencyEscapeAction::AutoSave => ops.auto_save(),
                EmergencyEscapeAction::Abort => ops.abort_orderly(),
            },
        );

        if let Err(err) = super::tty_init::tty_init_terminal() {
            tracing::warn!(%err, "emergency escape failed to reinitialize TTY");
        }
        self.force_full_redraw.store(true, Ordering::Release);
        true
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EmergencyEscapeAction {
    AutoSave,
    Abort,
}

/// GNU `handle_interrupt` emergency escape prompt sequence
/// (`src/keyboard.c:12638-12710`).
///
/// Scope note: GNU's `gc_in_progress` branch (`src/keyboard.c:12681-12693`) is
/// intentionally omitted because Neomacs enters emergency escape only at an
/// evaluator safe point, never asynchronously from inside GC.
fn run_emergency_escape_session(
    mut read_byte: impl FnMut() -> Option<u8>,
    mut write_bytes: impl FnMut(&[u8]),
    suspend: Option<&mut dyn FnMut()>,
    mut on_action: impl FnMut(EmergencyEscapeAction),
) {
    if let Some(suspend_fn) = suspend {
        suspend_fn();
    } else {
        write_bytes(
            b"No support for stopping a process on this operating system;\nyou can continue or abort.\n",
        );
    }

    write_bytes(b"Emacs is resuming after an emergency escape.\n");

    write_bytes(b"Auto-save? (y or n) ");
    let first = read_byte();
    if matches!(first, Some(b'y' | b'Y')) {
        on_action(EmergencyEscapeAction::AutoSave);
        write_bytes(b"Auto-save done\n");
    }
    drain_to_newline(first, &mut read_byte);

    write_bytes(b"Abort (and dump core)? (y or n) ");
    let first = read_byte();
    if matches!(first, Some(b'y' | b'Y')) {
        on_action(EmergencyEscapeAction::Abort);
    }
    drain_to_newline(first, &mut read_byte);

    write_bytes(b"Continuing...\n");
}

/// Deliberate deviation from GNU `src/keyboard.c:12678-12679,12703-12704`:
/// GNU's `read_stdin ()` (`:12591-12596`) returns `EOF` on EOF or error and
/// `while (c != '\n')` spins forever if stdin closes mid-prompt. Neomacs stops
/// draining on `None` (EOF) so a closed PTY cannot spin.
fn drain_to_newline(mut last: Option<u8>, read_byte: &mut impl FnMut() -> Option<u8>) {
    while !matches!(last, None | Some(b'\n')) {
        last = read_byte();
    }
}

#[cfg(unix)]
struct SigintBlockGuard {
    old_mask: libc::sigset_t,
    restored: bool,
}

#[cfg(unix)]
impl SigintBlockGuard {
    fn block() -> Self {
        let mut blocked = std::mem::MaybeUninit::<libc::sigset_t>::uninit();
        let mut old_mask = std::mem::MaybeUninit::<libc::sigset_t>::uninit();
        // SAFETY: `blocked` and `old_mask` are valid writable `sigset_t`
        // pointers; `pthread_sigmask` only modifies this thread's signal mask.
        let ok = unsafe {
            libc::sigemptyset(blocked.as_mut_ptr()) == 0
                && libc::sigaddset(blocked.as_mut_ptr(), libc::SIGINT) == 0
                && libc::pthread_sigmask(libc::SIG_BLOCK, blocked.as_ptr(), old_mask.as_mut_ptr())
                    == 0
        };
        if ok {
            Self {
                // SAFETY: `pthread_sigmask` returned 0 and initialized `old_mask`.
                old_mask: unsafe { old_mask.assume_init() },
                restored: false,
            }
        } else {
            Self {
                // SAFETY: `sigset_t` is plain C data; unused when `restored == true`.
                old_mask: unsafe { std::mem::zeroed() },
                restored: true,
            }
        }
    }
}

#[cfg(unix)]
impl Drop for SigintBlockGuard {
    fn drop(&mut self) {
        if !self.restored {
            // SAFETY: `old_mask` was initialized by `pthread_sigmask`.
            unsafe {
                libc::pthread_sigmask(libc::SIG_SETMASK, &self.old_mask, std::ptr::null_mut());
            }
            self.restored = true;
        }
    }
}

#[cfg(unix)]
fn sys_suspend_process_group() {
    // GNU `sys_suspend` (`src/sysdep.c:632`): `kill (0, SIGTSTP);`.
    // SAFETY: sending `SIGTSTP` to the current process group is a standard
    // POSIX job-control suspend; `SIGTSTP` disposition is `SIG_DFL`.
    unsafe {
        libc::kill(0, libc::SIGTSTP);
    }
}

#[cfg(unix)]
fn write_stdout_unbuffered(bytes: &[u8]) {
    let mut remaining = bytes;
    while !remaining.is_empty() {
        // SAFETY: `remaining` points to valid readable memory of `remaining.len()`.
        let written = unsafe {
            libc::write(
                libc::STDOUT_FILENO,
                remaining.as_ptr().cast(),
                remaining.len(),
            )
        };
        if written > 0 {
            remaining = &remaining[written as usize..];
        } else if written < 0 && io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
            continue;
        } else {
            break;
        }
    }
}

#[cfg(not(unix))]
fn write_stdout_unbuffered(bytes: &[u8]) {
    let mut stdout = io::stdout();
    let _ = stdout.write_all(bytes);
    let _ = stdout.flush();
}

/// Unbuffered 1-byte stdin read for emergency-escape prompts.
///
/// Deliberate deviation from GNU `read_stdin ()` (`src/keyboard.c:12591-12596`):
/// GNU returns `EOF` on `EINTR` as well as real EOF; Neomacs retries `EINTR`
/// (and `WouldBlock`) so an intervening signal (for example `SIGWINCH`) does
/// not spuriously answer "not y" to the prompt.
#[cfg(unix)]
fn read_stdin_unbuffered_byte() -> Option<u8> {
    let mut byte = 0u8;
    loop {
        // SAFETY: `&mut byte` is valid writable memory of 1 byte.
        let count = unsafe { libc::read(libc::STDIN_FILENO, (&mut byte as *mut u8).cast(), 1) };
        if count == 1 {
            return Some(byte);
        }
        if count == 0 {
            return None;
        }
        let err = io::Error::last_os_error();
        match err.kind() {
            io::ErrorKind::Interrupted => continue,
            io::ErrorKind::WouldBlock => {
                let mut pfd = libc::pollfd {
                    fd: libc::STDIN_FILENO,
                    events: libc::POLLIN,
                    revents: 0,
                };
                // SAFETY: `pfd` is a valid initialized `pollfd` entry.
                let _ = unsafe { libc::poll(&mut pfd, 1, -1) };
            }
            _ => return None,
        }
    }
}

#[cfg(not(unix))]
fn read_stdin_unbuffered_byte() -> Option<u8> {
    use std::io::Read as _;
    let mut byte = [0u8; 1];
    match io::stdin().read(&mut byte) {
        Ok(1) => Some(byte[0]),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    fn run_scripted(input: &[u8], supports_suspend: bool) -> (String, usize, usize, usize) {
        let mut bytes: VecDeque<u8> = input.iter().copied().collect();
        let mut output = Vec::new();
        let mut suspends = 0usize;
        let mut auto_saves = 0usize;
        let mut aborts = 0usize;
        let mut record_suspend = || suspends += 1;
        run_emergency_escape_session(
            || bytes.pop_front(),
            |chunk| output.extend_from_slice(chunk),
            supports_suspend.then_some(&mut record_suspend as &mut dyn FnMut()),
            |action| match action {
                EmergencyEscapeAction::AutoSave => auto_saves += 1,
                EmergencyEscapeAction::Abort => aborts += 1,
            },
        );
        (
            String::from_utf8(output).expect("ASCII prompts"),
            suspends,
            auto_saves,
            aborts,
        )
    }

    #[test]
    fn emergency_escape_autosave_yes_abort_no_matches_gnu_prompts() {
        let (out, suspends, auto_saves, aborts) = run_scripted(b"y\nn\n", true);
        assert_eq!(suspends, 1);
        assert_eq!(auto_saves, 1);
        assert_eq!(aborts, 0);
        assert_eq!(
            out,
            concat!(
                "Emacs is resuming after an emergency escape.\n",
                "Auto-save? (y or n) ",
                "Auto-save done\n",
                "Abort (and dump core)? (y or n) ",
                "Continuing...\n",
            )
        );
    }

    #[test]
    fn emergency_escape_uppercase_y_drains_rest_of_line() {
        let (out, suspends, auto_saves, aborts) = run_scripted(b"Yes please\nno thanks\n", true);
        assert_eq!(suspends, 1);
        assert_eq!(auto_saves, 1);
        assert_eq!(aborts, 0);
        assert!(out.contains("Auto-save done\n"));
        assert!(out.ends_with("Continuing...\n"));
    }

    #[test]
    fn emergency_escape_abort_yes_invokes_abort_without_autosave() {
        let (out, suspends, auto_saves, aborts) = run_scripted(b"n\nY\n", true);
        assert_eq!(suspends, 1);
        assert_eq!(auto_saves, 0);
        assert_eq!(aborts, 1);
        assert!(!out.contains("Auto-save done"));
        assert!(out.contains("Abort (and dump core)? (y or n) "));
    }

    #[test]
    fn emergency_escape_without_suspend_prints_gnu_no_sigtstp_notice() {
        let (out, suspends, auto_saves, aborts) = run_scripted(b"n\nn\n", false);
        assert_eq!(suspends, 0);
        assert_eq!(auto_saves, 0);
        assert_eq!(aborts, 0);
        assert!(out.starts_with(
            "No support for stopping a process on this operating system;\nyou can continue or abort.\nEmacs is resuming after an emergency escape.\n"
        ));
    }

    #[test]
    fn emergency_escape_eof_mid_prompt_does_not_spin() {
        let (out, suspends, auto_saves, aborts) = run_scripted(b"n", true);
        assert_eq!(suspends, 1);
        assert_eq!(auto_saves, 0);
        assert_eq!(aborts, 0);
        assert!(out.ends_with("Continuing...\n"));
    }
}
