//! TTY terminal detection, color/background sensing, and raw-mode lifecycle.
//!
//! Mirrors GNU Emacs `src/term.c` (`init_tty`, `tty_default_color_cells`,
//! `tty_default_color_mode`) and the environment probing that normally lives in
//! `init_display_interactive` (`src/dispnew.c`).
//!
//! All terminal I/O is cross-platform via crossterm.

use neomacs_display_protocol::tty_capabilities::TtyAttributeCapabilities;
use neovm_core::emacs_core::terminal::pure::TerminalRuntimeConfig;
use std::io::Write;

#[cfg(test)]
use super::terminal_capabilities::StringCapability;
use super::terminal_capabilities::TerminalCapabilityDatabase;
use super::{FrontendKind, StartupOptions};

/// Return a TTY terminal runtime configuration for interactive sessions.
///
/// ONE resolution, used for everything: `TN_max_colors` is part of the
/// capability record (GNU reads it inside `init_tty`'s `op` gate), so the
/// number Lisp gets from `tty-display-color-cells` and the number the writer
/// gates its colour emission on cannot be two different answers.
pub fn detect_tty_runtime(startup: &StartupOptions) -> TerminalRuntimeConfig {
    TerminalRuntimeConfig::interactive(detect_tty_type(), detect_tty_attribute_capabilities())
        .with_name(detect_tty_name(startup))
}

/// What this terminal can render, from its terminfo entry -- the capabilities GNU
/// reads in `init_tty`, INCLUDING `TN_max_colors`.
///
/// ONE resolution point: the answer goes both to the terminal runtime, where
/// `display-supports-face-attributes-p` and `tty-display-color-cells` read it
/// (GNU `tty_capable_p`, `Ftty_display_color_cells`), and to the renderer,
/// which emits from it (GNU `turn_on_face`).  A terminfo entry that cannot be
/// read is assumed fully capable rather than incapable -- neomacs' choice, so
/// a missing terminfo database does not silently strip every highlight -- and
/// that is the ONLY state whose colour count is not GNU's own, because GNU
/// exits rather than run there.
pub fn detect_tty_attribute_capabilities() -> TtyAttributeCapabilities {
    tty_attribute_capabilities(
        &std::env::var("COLORTERM").unwrap_or_default(),
        &std::env::var("TERM").unwrap_or_default(),
        super::terminal_capabilities::open_terminal_capability_database,
    )
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
        // variable TERM" (src/term.c:4874-4877).  This port keeps running, and
        // answers "no colours" rather than claiming a depth for a terminal it
        // cannot even name.
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
) -> Option<(u32, u32)> {
    let cols = parse_positive_u32(columns)?;
    let rows = parse_positive_u32(lines)?;
    Some((cols, rows))
}

fn terminal_size_from_env() -> Option<(u32, u32)> {
    terminal_size_from_env_values(std::env::var("COLUMNS").ok(), std::env::var("LINES").ok())
}

/// Query the terminal size in character cells via crossterm.
/// Falls back to `COLUMNS`/`LINES` environment variables.
pub fn query_terminal_size_cells() -> Option<(u32, u32)> {
    crossterm::terminal::size()
        .ok()
        .map(|(cols, rows)| (cols as u32, rows as u32))
        .or_else(terminal_size_from_env)
}

// ── TTY terminal lifecycle (raw mode, alt screen) ────────────────────────

/// GNU's init_tty refusal (term.c:4881): before touching terminal modes,
/// verify the terminal can position the cursor. Returns the diagnostic to
/// print (and exit with) when it cannot; checked here, while stdout is
/// still cooked and no alternate-screen byte has been written.
pub fn tty_check_terminal_powerful_enough() -> Result<(), String> {
    match detect_tty_type() {
        Some(term) => super::terminal_capabilities::check_terminal_powerful_enough(&term),
        None => Ok(()),
    }
}

/// Bytes that enter the GNU-compatible interactive TTY state.
pub(crate) fn tty_enter_sequence() -> &'static [u8] {
    // GNU's TTY startup enables the terminal modes its input decoder relies
    // on: application keypad, application cursor keys, and bracketed paste.
    // Keep the inverse sequence in `tty_leave_sequence` so shell state is
    // restored even though the modes do not affect painted cells directly.
    b"\x1b[?1049h\x1b=\x1b[?1h\x1b[?2004h\x1b[?25l\x1b[2J"
}

/// Bytes that restore the terminal state owned by the invoking shell.
pub(crate) fn tty_leave_sequence() -> &'static [u8] {
    b"\x1b[0m\x1b[?25h\x1b[?2004l\x1b[?1l\x1b>\x1b[?1049l"
}

/// Set up the terminal for the TtyRif direct-rendering path:
/// raw mode, alternate screen buffer, GNU input modes, hidden cursor.
pub fn tty_init_terminal() {
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

    if let Err(e) = crossterm::terminal::enable_raw_mode() {
        tracing::error!("tty_init_terminal: enable_raw_mode failed: {}", e);
        return;
    }

    // Enter alternate screen, configure input modes, hide cursor, clear.
    let mut stdout = std::io::stdout();
    let _ = stdout.write_all(tty_enter_sequence());
    let _ = stdout.flush();
    tracing::info!("TTY terminal initialized (raw mode + alt screen)");
}

/// Restore the terminal to its original state: show cursor, leave alt screen,
/// reset SGR, disable raw mode.
pub fn tty_shutdown_terminal() {
    // Show cursor, restore input modes, reset SGR, leave alternate screen.
    let mut stdout = std::io::stdout();
    let _ = stdout.write_all(tty_leave_sequence());
    let _ = stdout.flush();

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
    startup.frontend == FrontendKind::Tty && !startup.noninteractive
}

#[cfg(test)]
#[path = "tty_init_test.rs"]
mod tty_init_test;
