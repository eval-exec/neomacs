use super::{StringCapability, TerminalCapabilityDatabase};
use super::{
    terminal_size_from_env_values, tty_attribute_capabilities, tty_enter_sequence,
    tty_erase_char_value, tty_leave_sequence,
};
use neovm_core::emacs_core::value::Value;

/// A terminal database standing in for one terminfo entry's colour block --
/// GNU's `op`, `AF`, `AB` and `Co`, which `init_tty` reads together
/// (src/term.c:4602-4616).
///
/// `op` is a parameter and not a fixture detail: it is the gate, and a database
/// that always had it could not measure the rule.
struct ColorBlockDatabase {
    orig_pair: bool,
    colors: Option<i32>,
}

impl TerminalCapabilityDatabase for ColorBlockDatabase {
    fn get_string(&mut self, cap: StringCapability<'_>) -> Option<Vec<u8>> {
        match cap {
            StringCapability::Termcap("op") if self.orig_pair => Some(b"\x1b[39;49m".to_vec()),
            StringCapability::Termcap("AF") => Some(b"\x1b[3%p1%dm".to_vec()),
            StringCapability::Termcap("AB") => Some(b"\x1b[4%p1%dm".to_vec()),
            _ => None,
        }
    }

    fn get_termcap_number(&mut self, cap: &str) -> Option<i32> {
        (cap == "Co").then_some(self.colors).flatten()
    }

    fn get_termcap_flag(&mut self, _cap: &str) -> bool {
        false
    }
}

fn database(
    colors: Option<i32>,
) -> impl FnOnce(&str) -> Option<Box<dyn TerminalCapabilityDatabase>> {
    move |_term| {
        Some(Box::new(ColorBlockDatabase {
            orig_pair: true,
            colors,
        }) as Box<dyn TerminalCapabilityDatabase>)
    }
}

fn database_without_op(
    colors: Option<i32>,
) -> impl FnOnce(&str) -> Option<Box<dyn TerminalCapabilityDatabase>> {
    move |_term| {
        Some(Box::new(ColorBlockDatabase {
            orig_pair: false,
            colors,
        }) as Box<dyn TerminalCapabilityDatabase>)
    }
}

fn no_database(_term: &str) -> Option<Box<dyn TerminalCapabilityDatabase>> {
    None
}

fn cells(
    colorterm: &str,
    term: &str,
    open: impl FnOnce(&str) -> Option<Box<dyn TerminalCapabilityDatabase>>,
) -> i64 {
    tty_attribute_capabilities(colorterm, term, open).color_cells()
}

/// GNU reads the colour count out of the terminal database -- `init_tty` does
/// `tty->TN_max_colors = tgetnum ("Co")` (src/term.c:4616) -- never out of the
/// TERM name, and that number decides how many entries `tty-color-alist` gets
/// and which `((class color) (min-colors N) ...)` specs match.
///
/// Measured in a PTY with COLORTERM unset, both editors:
///   TERM=rxvt-16color   GNU => cells 16, alist 16;  Neomacs before => 8, 8
///   TERM=linux-16color  GNU => cells 16, alist 8;   Neomacs before => 8, 8
#[test]
fn color_cells_come_from_the_terminal_database_not_the_name() {
    assert_eq!(cells("", "rxvt-16color", database(Some(16))), 16);
    assert_eq!(cells("", "linux-16color", database(Some(16))), 16);
    assert_eq!(cells("", "screen-256color", database(Some(256))), 256);
    assert_eq!(cells("", "xterm", database(Some(8))), 8);
    // A name that says 256 does not make it so: the entry is the authority.
    assert_eq!(cells("", "wrapper-256color", database(Some(8))), 8);
}

/// GNU treats `tgetnum ("Co")` == -1 as "no colours", and a monochrome entry is
/// not a reason to guess from the name either.
#[test]
fn a_terminal_that_reports_no_colors_has_none() {
    assert_eq!(cells("", "vt100", database(None)), 0);
    assert_eq!(cells("", "vt100", database(Some(-1))), 0);
    assert_eq!(cells("", "vt100", database(Some(0))), 0);
}

/// `COLORTERM` does NOT stay ahead of the database, and that is ledger 193's
/// correction to this pin.
///
/// GNU reads it in the LAST arm of a chain that lives inside the `op` gate,
/// and compares it with `strcasecmp (bg, "truecolor")` -- so it cannot promote
/// a terminal that has no `op`, and it is not read at all for any other
/// spelling.  Measured in a pty against GNU 31.0.90:
///
/// ```text
///   TERM=xterm      COLORTERM=24bit      GNU 8   this port, before 16777216
///   TERM=amiga-vnc  COLORTERM=truecolor  GNU 0   this port, before 16777216
/// ```
///
/// The previous spelling of this test asserted the 16777216 for `24bit`, which
/// was the divergence rather than the rule -- ledger 180's "a pin can be
/// asserting the divergence".
#[test]
fn colorterm_is_gnus_last_arm_and_not_a_shortcut_past_the_database() {
    assert_eq!(
        cells("truecolor", "screen-256color", database(Some(256))),
        16_777_216
    );
    assert_eq!(cells("TrueColor", "xterm", database(Some(8))), 16_777_216);
    // GNU's test is an exact strcasecmp, so these are not truecolor.
    assert_eq!(cells("24bit", "xterm", database(Some(8))), 8);
    assert_eq!(cells("rxvt", "xterm", database(Some(8))), 8);
    // And no COLORTERM opens GNU's `op` gate.
    assert_eq!(cells("truecolor", "xterm", database_without_op(Some(8))), 0);

    // An unset TERM: GNU exits with "Please set the environment variable TERM"
    // (src/term.c:4874-4877); this port keeps running and claims no colour.
    assert_eq!(cells("", "", database(Some(8))), 0);
    assert_eq!(cells("truecolor", "", database(Some(8))), 0);
}

/// `dumb` needs no special case: GNU refuses to run on it at all
/// ("Terminal type \"dumb\" is not powerful enough to run Emacs", measured in a
/// pty), and its terminfo entry has no `op`, so the `op` gate answers zero for
/// it the same way it answers zero for `amiga-vnc`.
#[test]
fn a_terminal_with_no_op_is_monochrome_whatever_co_says() {
    assert_eq!(cells("", "dumb", database_without_op(Some(8))), 0);
    assert_eq!(cells("", "amiga-vnc", database_without_op(Some(16))), 0);
    assert!(
        !tty_attribute_capabilities("", "amiga-vnc", database_without_op(Some(16)))
            .supports_color()
    );
}

/// The name heuristic survives only where GNU would have refused to start at
/// all ("Terminal type X is not defined"), so it is a fallback, not the rule --
/// and it is now the count carried by `TtyColorSource::NoDatabase`, which is
/// the same state the writer's fixed ANSI rule belongs to, rather than a
/// second answer beside a resolved one.
#[test]
fn an_unreadable_entry_falls_back_to_the_name() {
    assert_eq!(cells("", "screen-256color", no_database), 256);
    assert_eq!(cells("", "rxvt-16color", no_database), 8);
    assert!(
        tty_attribute_capabilities("", "rxvt-16color", no_database)
            .colors
            .allows_ansi_fallback(),
        "the guessed count belongs to the state that has no `setaf` to spell with"
    );
}

#[test]
fn terminal_size_from_env_values_uses_positive_columns_and_lines() {
    assert_eq!(
        terminal_size_from_env_values(Some("160".to_string()), Some("50".to_string())),
        Some((160, 50))
    );
}

#[test]
fn terminal_size_from_env_values_rejects_missing_zero_or_invalid_values() {
    assert_eq!(
        terminal_size_from_env_values(None, Some("50".to_string())),
        None
    );
    assert_eq!(
        terminal_size_from_env_values(Some("160".to_string()), Some("0".to_string())),
        None
    );
    assert_eq!(
        terminal_size_from_env_values(Some("wide".to_string()), Some("50".to_string())),
        None
    );
}

#[test]
fn tty_lifecycle_enables_and_restores_gnu_input_modes() {
    assert_eq!(
        tty_enter_sequence(),
        b"\x1b[?1049h\x1b=\x1b[?1h\x1b[?2004h\x1b[?25l\x1b[2J"
    );
    assert_eq!(
        tty_leave_sequence(),
        b"\x1b[0m\x1b[?25h\x1b[?2004l\x1b[?1l\x1b>\x1b[?1049l"
    );
}

#[test]
fn tty_erase_char_value_mirrors_init_sys_modes() {
    // GNU src/sysdep.c init_sys_modes starts Vtty_erase_char at Qnil (1112)
    // and assigns c_cc[VERASE] only once it has a live tty (1130). Off a
    // terminal the value stays nil rather than becoming a number, which is
    // what `normal-erase-is-backspace-setup-frame' compares against ?\^H.
    assert_eq!(tty_erase_char_value(None), Value::NIL);
    // The two erase characters a terminal actually reports: DEL and C-h. On a
    // ^H terminal GNU enables normal-erase-is-backspace-mode and translates
    // C-h to DEL, so Backspace deletes instead of opening help.
    assert_eq!(tty_erase_char_value(Some(0x7f)), Value::fixnum(127));
    assert_eq!(tty_erase_char_value(Some(0x08)), Value::fixnum(8));
}
