//! TTY terminal detection, color/background sensing, and raw-mode lifecycle.
//!
//! Mirrors GNU Emacs `src/term.c` (`init_tty`, `tty_default_color_cells`,
//! `tty_default_color_mode`) and the environment probing that normally lives in
//! `init_display_interactive` (`src/dispnew.c`).
//!
//! Raw input uses crossterm. Output uses native terminfo capabilities on Unix
//! and negotiated VT or native screen buffers on Windows.

#[cfg(test)]
use super::terminal_capabilities::StringCapability;
use super::terminal_capabilities::TerminalCapabilityDatabase;
use super::{FrontendKind, StartupOptions};
use std::fmt;

use neomacs_display_protocol::tty_capabilities::TtyAttributeCapabilities;
use neomacs_display_runtime::tty_input::{
    TerminalCellSize, TerminalColumns, TerminalRows, TerminalSizeQuery,
};
use neovm_core::emacs_core::terminal::pure::TerminalRuntimeConfig;

/// Return a TTY terminal runtime configuration for interactive sessions.
///
/// ONE resolution, used for everything: `TN_max_colors` is part of the
/// capability record (GNU reads it inside `init_tty`'s `op` gate), so the
/// number Lisp gets from `tty-display-color-cells` and the number the writer
/// gates its colour emission on cannot be two different answers.
pub fn detect_tty_runtime(startup: &StartupOptions) -> TerminalRuntimeConfig {
    TerminalRuntimeConfig::interactive(detect_tty_type(), detect_tty_attribute_capabilities())
        .with_name(detect_tty_name(startup))
        .into()
}

/// What this terminal can render, from its terminfo entry -- the capabilities GNU
/// reads in `init_tty`, INCLUDING `TN_max_colors`.
///
/// ONE resolution point: the answer goes both to the terminal runtime, where
/// `display-supports-face-attributes-p` and `tty-display-color-cells` read it
/// (GNU `tty_capable_p`, `Ftty_display_color_cells`), and to the renderer,
/// which emits from it (GNU `turn_on_face`). Unix live startup separately
/// requires a readable terminal description. The pure resolver retains its
/// fallback for callers that inspect capabilities before that check.
pub fn detect_tty_attribute_capabilities() -> TtyAttributeCapabilities {
    #[cfg(windows)]
    {
        super::tty_output::windows::attributes()
    }
    #[cfg(not(windows))]
    {
        tty_attribute_capabilities(
            &std::env::var("COLORTERM").unwrap_or_default(),
            &std::env::var("TERM").unwrap_or_default(),
            super::terminal_capabilities::open_terminal_capability_database,
        )
    }
}

/// The rule itself, over an injected terminal database, so it can be measured
/// against a terminfo entry this machine may not have installed.
///
/// Before ledger 193 the colour COUNT was answered here by a rule of its own
/// -- `COLORTERM` first, then `Co`, then a guess from the TERM name -- and the
/// resolved record's own `Co` was overwritten with it one line later.  Neither
/// answer was GNU's: GNU reads `Co` INSIDE the `op` gate and promotes it to
/// 16777216 through four arms, and its `COLORTERM` test is an exact
/// `strcasecmp (bg, "truecolor")` in the LAST of those arms
/// (src/term.c:4602-4674).
pub(crate) fn tty_attribute_capabilities(
    colorterm: &str,
    term: &str,
    open: impl FnOnce(&str) -> Option<Box<dyn TerminalCapabilityDatabase>>,
) -> TtyAttributeCapabilities {
    if term.is_empty() {
        // GNU refuses to start at all here: "Please set the environment
        // variable TERM" (src/term.c:4874-4877). Live startup now does too;
        // inspection before that check answers no colors.
        return TtyAttributeCapabilities::full_with_color_cells(0);
    }
    if let Some(mut database) = open(term) {
        return super::terminal_capabilities::resolve_tty_attribute_capabilities(
            database.as_mut(),
            colorterm,
        );
    }
    tracing::debug!("no terminfo entry for TERM; guessing color cells from the name");
    // GNU exits here too ("Terminal type %s is not defined", src/term.c:4880).
    // The name heuristic survives only for this state, and it is the count
    // that belongs to `TtyColorSource::NoDatabase`'s fixed ANSI writer -- never
    // a second answer beside a resolved one.
    let guessed = if term.to_ascii_lowercase().contains("256color") {
        256
    } else {
        8
    };
    TtyAttributeCapabilities::full_with_color_cells(guessed)
}

/// GNU `TN_max_colors` for this terminal.
///
/// A named reading of [`detect_tty_attribute_capabilities`] rather than a rule
/// of its own, which is the whole of ledger 193's item 2.
pub fn detect_tty_color_cells() -> i64 {
    detect_tty_attribute_capabilities().color_cells()
}

pub fn detect_tty_type() -> Option<String> {
    std::env::var("TERM").ok().filter(|value| !value.is_empty())
}

/// Detect the update-planner capabilities for the connected terminal.
///
/// Unlike the attribute fallback above (where over-claiming merely styles
/// text a dumb terminal ignores), over-claiming a planner capability emits
/// scroll/shift bytes an unknown terminal may not implement while the
/// screen model assumes it did — permanent corruption. An unset TERM or
/// unreadable terminfo therefore falls to the conservative floor.
pub fn detect_term_caps() -> neomacs_display_runtime::backend::tty::rif::TermCaps {
    detect_tty_type()
        .and_then(|term| super::terminal_capabilities::term_caps_for_term(&term))
        .unwrap_or_else(|| {
            tracing::debug!("no terminfo entry for TERM; refusing planner optimizations");
            neomacs_display_runtime::backend::tty::rif::TermCaps::unknown_terminal()
        })
}

pub(crate) fn default_controlling_tty_name() -> &'static str {
    #[cfg(windows)]
    {
        "CONOUT$"
    }
    #[cfg(not(windows))]
    {
        "/dev/tty"
    }
}

pub fn detect_tty_name(_startup: &StartupOptions) -> String {
    default_controlling_tty_name().to_string()
}

// ── Erase character (GNU `init_sys_modes`, src/sysdep.c) ─────────────────

/// The `tty-erase-char` value for a detected ERASE byte.
///
/// GNU's `init_sys_modes` (src/sysdep.c:1112) starts `Vtty_erase_char` at
/// `Qnil` and assigns `tty.main.c_cc[VERASE]` (src/sysdep.c:1130) only once it
/// has a live tty, so off a terminal the variable stays nil rather than
/// becoming a number. That distinction is load-bearing:
/// `normal-erase-is-backspace-setup-frame` (lisp/simple.el) enables the mode
/// when `(eq tty-erase-char ?\^H)`, which then `key-translate`s C-h to DEL so
/// Backspace deletes instead of opening the help prefix.
pub fn tty_erase_char_value(erase: Option<u8>) -> neovm_core::emacs_core::value::Value {
    use neovm_core::emacs_core::value::Value;
    erase.map_or(Value::NIL, |byte| Value::fixnum(i64::from(byte)))
}

/// Read the terminal's ERASE character, as GNU does from the saved original
/// termios (`emacs_get_tty` into `tty_out->old_tty`, then `c_cc[VERASE]`).
///
/// Must be called before raw mode is entered, so the answer describes the
/// terminal the user configured with stty rather than the modes we impose.
pub fn detect_tty_erase_char() -> Option<u8> {
    #[cfg(unix)]
    {
        // SAFETY: tcgetattr only writes the termios out-parameter, and STDIN
        // is a borrowed descriptor we do not close.
        let mut termios = std::mem::MaybeUninit::<libc::termios>::uninit();
        let ok = unsafe { libc::tcgetattr(libc::STDIN_FILENO, termios.as_mut_ptr()) } == 0;
        if !ok {
            // Not a terminal (redirected stdin, batch): GNU leaves nil.
            return None;
        }
        let termios = unsafe { termios.assume_init() };
        Some(termios.c_cc[libc::VERASE])
    }
    #[cfg(not(unix))]
    {
        None
    }
}

/// GNU's `baud_convert` table (`src/sysdep.c:146-150`): termios speed code to
/// bits per second.
const BAUD_CONVERT: [i64; 16] = [
    0, 50, 75, 110, 135, 150, 200, 300, 600, 1200, 1800, 2400, 4800, 9600, 19200, 38400,
];

/// GNU `init_baud_rate` (`src/sysdep.c:413-437`), for the interactive tty case.
///
/// `init_tty` calls it with the terminal's input descriptor
/// (`src/term.c:4755`, `4923`), so this must run while stdin still describes
/// the user's terminal -- before raw mode, like `detect_tty_erase_char`.
/// GNU reads `cfgetospeed` from the termios, indexes `baud_convert`, falls back
/// to 9600 for a speed code past the end of the table, and substitutes 1200 for
/// a zero result (a hung-up line).
///
/// The `noninteractive` arm of GNU's function is deliberately not ported: under
/// `--batch` GNU never creates a tty terminal, so `init_baud_rate` is never
/// reached at all and `baud-rate` keeps the C global's 0.  Calling this only
/// from the live-tty path is what reproduces that.
pub fn detect_baud_rate() -> i64 {
    #[cfg(unix)]
    {
        // SAFETY: tcgetattr only writes the termios out-parameter, and STDIN is
        // a borrowed descriptor we do not close.
        let mut termios = std::mem::MaybeUninit::<libc::termios>::uninit();
        if unsafe { libc::tcgetattr(libc::STDIN_FILENO, termios.as_mut_ptr()) } != 0 {
            // GNU would have `tcgetattr` fail into the `B9600` it pre-filled
            // `sg.c_cflag` with (`src/sysdep.c:426-431`).
            return 9600;
        }
        let termios = unsafe { termios.assume_init() };
        let ospeed = unsafe { libc::cfgetospeed(&termios) };
        let converted = BAUD_CONVERT.get(ospeed as usize).copied().unwrap_or(9600);
        if converted == 0 { 1200 } else { converted }
    }
    #[cfg(not(unix))]
    {
        9600
    }
}

/// Detect the terminal's background brightness for GNU's `background-mode'
/// TERMINAL parameter (`frame-terminal-default-bg-mode', lisp/frame.el:1588).
///
/// FOUND AND NOT FIXED (DIVERGENCES.md 157): `COLORFGBG' appears nowhere in GNU.
/// GNU's own default is `light' for a `tty-type' matching
/// "^\\(xterm\\|rxvt\\|dtterm\\|eterm\\)" and `dark' otherwise
/// (`frame--current-background-mode', lisp/frame.el:1505-1524), refined only by
/// an actual OSC-11 reply (`xterm--set-background-mode',
/// lisp/term/xterm.el:1309).  157 moved this value onto GNU's channel -- the
/// terminal parameter, so the FRAME parameter is derived by GNU's Lisp -- but
/// the heuristic itself still diverges and belongs to a later entry.
pub fn detect_tty_background_mode() -> &'static str {
    let Some(colorfgbg) = std::env::var("COLORFGBG").ok() else {
        return "dark";
    };
    let Some(background) = colorfgbg
        .split(';')
        .next_back()
        .and_then(|value| value.parse::<i32>().ok())
    else {
        return "dark";
    };

    if (7..=15).contains(&background) {
        "light"
    } else {
        "dark"
    }
}

// ── Terminal size ─────────────────────────────────────────────────────────

fn parse_positive_u32(value: Option<String>) -> Option<u32> {
    value
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|value| *value > 0)
}

fn terminal_size_from_env_values(
    columns: Option<String>,
    lines: Option<String>,
) -> Option<TerminalCellSize> {
    let cols = parse_positive_u32(columns)?;
    let rows = parse_positive_u32(lines)?;
    Some(TerminalCellSize::new(
        TerminalColumns::from(cols),
        TerminalRows::from(rows),
    ))
}

fn terminal_size_from_env() -> Option<TerminalCellSize> {
    terminal_size_from_env_values(std::env::var("COLUMNS").ok(), std::env::var("LINES").ok())
}

/// Query the terminal size, retaining the legacy tuple boundary for callers.
/// Falls back to positive COLUMNS/LINES values after native/platform failure.
pub fn query_terminal_size_cells() -> Option<(u32, u32)> {
    query_terminal_size_cells_with(&TerminalSizeQuery::new()).map(legacy_terminal_columns_rows)
}

/// GNU `init_tty` picks this text when the controlling terminal cannot be
/// used (`term.c:4464-4470`). `open_failed` is `fd < 0`. The other arm is an
/// fd that opened and then failed `isatty`. `name` is `DEV_TTY`, `/dev/tty`
/// (`keyboard.c:94`). The initial terminal passes `must_succeed`, so `vfatal`
/// prints `emacs: ` and this string.
pub(crate) fn initial_tty_open_diagnostic(open_failed: bool) -> &'static str {
    if open_failed {
        "Could not open file: /dev/tty"
    } else {
        "Not a tty device: /dev/tty"
    }
}

/// `None` when `/dev/tty` opened and is a terminal.
///
/// GNU's initial terminal is `init_tty` with a null name, so it opens
/// `dev_tty` and fatals on failure (`term.c:4404-4470`). A later
/// `make-terminal-frame` does not use this check.
pub(crate) fn initial_tty_open_failure() -> Option<&'static str> {
    #[cfg(unix)]
    {
        match open_dev_tty() {
            Err(_) => Some(initial_tty_open_diagnostic(true)),
            Ok(file) => {
                let fd = std::os::fd::AsRawFd::as_raw_fd(&file);
                // SAFETY: `fd` is owned by `file` for this call. `isatty`
                // does not retain it.
                let is_tty = unsafe { libc::isatty(fd) } == 1;
                if is_tty {
                    None
                } else {
                    Some(initial_tty_open_diagnostic(false))
                }
            }
        }
    }
    #[cfg(not(unix))]
    {
        None
    }
}

#[cfg(unix)]
fn open_dev_tty() -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NOCTTY)
        .open("/dev/tty")
}

/// Winsize for a root `make-terminal-frame` on the initial terminal.
///
/// GNU `get_tty_size` (`src/sysdep.c:1396-1408`) is one `TIOCGWINSZ` on the
/// initial terminal's input fd (`/dev/tty`, opened by `init_tty`). Failure
/// and a successful 0×0 are both `(0, 0)`. There is no crossterm or `tput`
/// fallback. The frame clamp turns that into a live frame.
pub fn initial_tty_winsize_cells() -> (u32, u32) {
    #[cfg(unix)]
    {
        let Ok(file) = std::fs::File::open("/dev/tty") else {
            return (0, 0);
        };
        winsize_from_fd(std::os::fd::AsRawFd::as_raw_fd(&file))
    }
    #[cfg(not(unix))]
    {
        (0, 0)
    }
}

/// One `TIOCGWINSZ` on `fd`. Same shape as the secondary terminal's raw read:
/// a failed ioctl is `(0, 0)`, and a successful 0×0 stays `(0, 0)`.
#[cfg(unix)]
fn winsize_from_fd(fd: std::os::fd::RawFd) -> (u32, u32) {
    let mut size = unsafe { std::mem::zeroed::<libc::winsize>() };
    // SAFETY: `fd` is borrowed for this call. `TIOCGWINSZ` writes only the
    // zeroed `winsize` and keeps no pointer. A failed call leaves `size` zero,
    // and the branch below discards it.
    if unsafe { libc::ioctl(fd, libc::TIOCGWINSZ as _, &mut size) } == 0 {
        (u32::from(size.ws_col), u32::from(size.ws_row))
    } else {
        (0, 0)
    }
}

/// GNU `init_tty` refuses this screen (`term.c:4810-4814`, `vfatal` at 4936).
///
/// `cols` and `rows` are the signed sizes after the `co`/`li` fallback, in
/// that order. `tgetnum` returns -1 when the capability is missing, and the
/// message prints that value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct InitialTtySizeError {
    cols: i32,
    rows: i32,
}

impl fmt::Display for InitialTtySizeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "Screen size {}x{} is too small",
            self.cols, self.rows
        )
    }
}

/// Initial-terminal cells from an ioctl report and termcap `co`/`li`.
///
/// GNU `init_tty` (`term.c:4800-4814`) keeps a positive ioctl side and, only
/// when that side is `<= 0`, replaces it with `tgetnum("co")` or
/// `tgetnum("li")`. Either side below 3 is fatal. This is the initial frame
/// only. `make-terminal-frame` keeps [`initial_tty_winsize_cells`], including
/// a raw `(0, 0)`.
pub(crate) fn initial_tty_cells_from_ioctl(
    ioctl_cols: i32,
    ioctl_rows: i32,
    co: i32,
    li: i32,
) -> Result<(u32, u32), InitialTtySizeError> {
    let cols = if ioctl_cols > 0 { ioctl_cols } else { co };
    let rows = if ioctl_rows > 0 { ioctl_rows } else { li };
    if cols < 3 || rows < 3 {
        return Err(InitialTtySizeError { cols, rows });
    }
    Ok((
        u32::try_from(cols).unwrap_or(u32::MAX),
        u32::try_from(rows).unwrap_or(u32::MAX),
    ))
}

/// Size of the initial interactive terminal.
///
/// A positive ioctl wins, so an 80×24 pty stays 80×24. A non-positive side
/// reads `co` or `li` through neomacs-terminfo (`tgetnum`). A missing number
/// is -1, the same answer GNU's `tgetnum` gives, and then fails the 3×3 check.
pub(crate) fn initial_tty_startup_cells() -> Result<(u32, u32), InitialTtySizeError> {
    let (ioctl_cols, ioctl_rows) = initial_tty_winsize_cells();
    let ioctl_cols = reported_tty_dimension(ioctl_cols);
    let ioctl_rows = reported_tty_dimension(ioctl_rows);
    let (co, li) = if ioctl_cols <= 0 || ioctl_rows <= 0 {
        termcap_co_li()
    } else {
        (0, 0)
    };
    initial_tty_cells_from_ioctl(ioctl_cols, ioctl_rows, co, li)
}

fn reported_tty_dimension(value: u32) -> i32 {
    i32::try_from(value).unwrap_or(i32::MAX)
}

/// Termcap `co` and `li` for `TERM`, or -1 when the name or the number is absent.
fn termcap_co_li() -> (i32, i32) {
    let Some(term) = detect_tty_type() else {
        return (-1, -1);
    };
    let Ok(database) = neomacs_terminfo::Database::load(
        &term,
        &[
            neomacs_terminfo::Query::TermcapNumber("co"),
            neomacs_terminfo::Query::TermcapNumber("li"),
        ],
    ) else {
        return (-1, -1);
    };
    (
        database.termcap_number("co").unwrap_or(-1),
        database.termcap_number("li").unwrap_or(-1),
    )
}

fn legacy_terminal_columns_rows(size: TerminalCellSize) -> (u32, u32) {
    (u32::from(size.columns()), u32::from(size.rows()))
}

/// Query an existing resource without caching the returned dimensions.
pub(crate) fn query_terminal_size_cells_with(
    query: &TerminalSizeQuery,
) -> Option<TerminalCellSize> {
    terminal_size_or_fallback(query.query(), terminal_size_from_env)
}

fn terminal_size_or_fallback(
    query_result: std::io::Result<TerminalCellSize>,
    fallback: impl FnOnce() -> Option<TerminalCellSize>,
) -> Option<TerminalCellSize> {
    query_result.ok().or_else(fallback)
}

// ── TTY terminal lifecycle (raw mode, alt screen) ────────────────────────

/// GNU's init_tty refusal (term.c:4881): before touching terminal modes,
/// verify the terminal can position the cursor. Returns the diagnostic to
/// print (and exit with) when it cannot; checked here, while stdout is
/// still cooked and no alternate-screen byte has been written.
pub fn tty_check_terminal_powerful_enough() -> Result<(), String> {
    #[cfg(windows)]
    {
        super::tty_output::windows::check()
    }
    #[cfg(not(windows))]
    {
        super::tty_output::primary().map(|_| ())
    }
}

/// Bytes that enter the GNU-compatible interactive TTY state.
#[cfg(any(windows, test))]
pub(crate) fn tty_enter_sequence() -> &'static [u8] {
    // GNU's TTY startup enables the terminal modes its input decoder relies
    // on: application keypad, application cursor keys, and bracketed paste.
    // Keep the inverse sequence in `tty_leave_sequence` so shell state is
    // restored even though the modes do not affect painted cells directly.
    b"\x1b[?1049h\x1b=\x1b[?1h\x1b[?2004h\x1b[?25l\x1b[2J"
}

/// Bytes that restore the terminal state owned by the invoking shell.
#[cfg(any(windows, test))]
pub(crate) fn tty_leave_sequence() -> &'static [u8] {
    b"\x1b[0m\x1b[?25h\x1b[?2004l\x1b[?1l\x1b>\x1b[?1049l"
}

/// Set up the terminal for the TtyRif direct-rendering path:
/// raw mode, alternate screen buffer, GNU input modes, hidden cursor.
pub fn tty_init_terminal() -> Result<(), String> {
    // Register what this terminal can render, as GNU does in `init_tty`: the
    // terminfo attribute strings (`sitm`, `smul`, `Smulx`, `bold`, `dim`, `smxx`,
    // `ncv`) plus the color depth. The renderer then emits an attribute only when
    // the terminal has it, with GNU's fallbacks -- and the `-nw` face colors are
    // downsampled to a palette the terminal can render instead of always emitting
    // 24-bit truecolor (issue #154).
    //
    // Same record the terminal runtime carries (see
    // `detect_tty_attribute_capabilities`), so the predicate and the renderer
    // cannot disagree about what this terminal can do.
    neomacs_display_runtime::backend::tty::rif::set_capabilities(
        detect_tty_attribute_capabilities(),
    );

    crossterm::terminal::enable_raw_mode().map_err(|error| error.to_string())?;
    #[cfg(windows)]
    let result = super::tty_output::windows::enter();
    #[cfg(not(windows))]
    let result = super::tty_output::primary()
        .map_err(std::io::Error::other)
        .and_then(|caps| {
            let mut stdout = std::io::stdout();
            let (_, height) = query_terminal_size_cells().unwrap_or((80, 24));
            caps.enter(height as usize).write_to(&mut stdout, caps)
        });
    if let Err(error) = result {
        tty_shutdown_terminal();
        return Err(format!("could not initialize TTY output: {error}"));
    }
    tracing::info!("TTY terminal initialized");
    Ok(())
}

/// Restore the terminal to its original state: show cursor, leave alt screen,
/// reset SGR, disable raw mode.
pub fn tty_shutdown_terminal() {
    #[cfg(windows)]
    if let Err(error) = super::tty_output::windows::leave() {
        tracing::warn!(%error, "could not restore console output");
    }
    #[cfg(not(windows))]
    if let Ok(caps) = super::tty_output::primary() {
        let mut stdout = std::io::stdout();
        let _ = caps.leave().write_to(&mut stdout, caps);
    }

    // Restore raw mode
    if let Err(e) = crossterm::terminal::disable_raw_mode() {
        tracing::warn!("tty_shutdown_terminal: disable_raw_mode failed: {}", e);
    } else {
        tracing::info!("TTY terminal restored");
    }
}

/// Returns `true` when the session is an interactive TTY (not batch and
/// not a GUI frontend).
pub fn should_enable_live_tty_io(startup: &StartupOptions) -> bool {
    startup.frontend == FrontendKind::Tty && !startup.noninteractive && startup.daemon.is_none()
}

/// GNU `init_display` returns before `init_tty` when `IS_DAEMON`
/// (`dispnew.c:7222-7237`). The initial `/dev/tty` open, winsize, and
/// termcap `co`/`li` check are that `init_tty` path (`term.c:4464-4470`,
/// `4800-4814`). `--daemon` and `--fg-daemon` both skip it. Batch does too.
pub(crate) fn initial_tty_fatal_checks_apply(noninteractive: bool, daemon: bool) -> bool {
    !noninteractive && !daemon
}

#[cfg(test)]
#[path = "tests/tty_init_test.rs"]
mod tty_init_test;
