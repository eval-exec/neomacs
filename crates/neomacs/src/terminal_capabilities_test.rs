//! Tests for terminfo → [`TtyAttributeCapabilities`] resolution.

use super::*;
use neomacs_display_protocol::face::UnderlineStyle;
use neomacs_display_protocol::tty_capabilities::{TtyCapability, TtyItalicRendition};
use std::collections::HashMap;

/// A capability database standing in for terminfo, so the resolution can be
/// tested against known entries without a real terminal.
struct FakeCapabilityDatabase {
    strings: HashMap<&'static str, &'static str>,
    numbers: HashMap<&'static str, i32>,
    flags: std::collections::HashSet<&'static str>,
}

impl FakeCapabilityDatabase {
    /// `screen-256color`: standout, underline, bold and dim, but NO `sitm`
    /// (italics) and no `smxx` (strike-through) — the entry that made GNU render
    /// `:slant italic` as dim while neomacs emitted an italic escape.
    ///
    /// `op` is here because the real entry has it (`infocmp -1 -x
    /// screen-256color` answers `op=\E[39;49m`), and because without it the
    /// fixture would not be `screen-256color` at all: GNU reads `Co` INSIDE
    /// `if (tty->TS_orig_pair)` (src/term.c:4604-4616), so an entry with no
    /// `op` has no colour count either -- and this fixture's own
    /// `assert_eq!(caps.color_cells(), 256)` is the pty measurement of GNU
    /// 31.0.90 on the real terminal (ledger 193).
    fn screen_256color() -> Self {
        Self {
            strings: HashMap::from([
                ("so", "\x1b[3m"),
                ("se", "\x1b[23m"),
                ("us", "\x1b[4m"),
                ("md", "\x1b[1m"),
                ("mh", "\x1b[2m"),
                ("op", "\x1b[39;49m"),
                ("AF", "\x1b[3%p1%dm"),
                ("AB", "\x1b[4%p1%dm"),
            ]),
            numbers: HashMap::from([("Co", 256), ("NC", -1)]),
            flags: std::collections::HashSet::new(),
        }
    }

    /// An empty entry to build capability shapes on.
    fn bare() -> Self {
        Self {
            strings: HashMap::new(),
            numbers: HashMap::new(),
            flags: std::collections::HashSet::new(),
        }
    }

    /// The ANSI update capabilities of an xterm-shaped entry, in the termcap
    /// spellings tgetstr returns (terminfo %p markers already translated).
    fn xterm_like() -> Self {
        Self::bare()
            .with_string("cm", "\x1b[%i%d;%dH")
            .with_string("cs", "\x1b[%i%d;%dr")
            .with_string("SF", "\x1b[%dS")
            .with_string("SR", "\x1b[%dT")
            .with_string("sf", "\n")
            .with_string("sr", "\x1bM")
            .with_string("IC", "\x1b[%d@")
            .with_string("DC", "\x1b[%dP")
            .with_string("ce", "\x1b[K")
            .with_flag("am")
            .with_flag("xn")
    }

    /// vt220 shape: DECSTBM + cursor addressing but NO indn/rin — CSI S/T
    /// is not implemented by this terminal class.
    fn vt220_like() -> Self {
        Self::bare()
            .with_string("cm", "\x1b[%i%d;%dH")
            .with_string("cs", "\x1b[%i%d;%dr")
            .with_string("sf", "\n")
            .with_string("sr", "\x1bM")
            .with_string("ce", "\x1b[K$<3>")
    }

    /// tvi955 shape: insert/delete strings EXIST but are not ANSI.
    fn tvi955_like() -> Self {
        Self::bare()
            .with_string("cm", "\x1b[%i%d;%dH")
            .with_string("IC", "\x1bQ")
            .with_string("DC", "\x1bW")
            .with_string("ce", "\x1bt")
    }

    fn with_flag(mut self, cap: &'static str) -> Self {
        self.flags.insert(cap);
        self
    }

    fn without_string(mut self, cap: &'static str) -> Self {
        self.strings.remove(cap);
        self
    }

    fn with_string(mut self, cap: &'static str, value: &'static str) -> Self {
        self.strings.insert(cap, value);
        self
    }

    fn with_number(mut self, cap: &'static str, value: i32) -> Self {
        self.numbers.insert(cap, value);
        self
    }
}

impl TerminalCapabilityDatabase for FakeCapabilityDatabase {
    /// Both namespaces are one map here, keyed by the capability's own name.
    /// A fake will answer any key, which is exactly why it cannot attest that
    /// the REAL database can find `Smulx` and `smxx`; see
    /// `styled_underline_and_strike_through_come_from_the_terminfo_database`.
    fn get_string(&mut self, cap: StringCapability) -> Option<Vec<u8>> {
        let name = match cap {
            StringCapability::Termcap(name) | StringCapability::Terminfo(name) => name,
        };
        self.strings
            .get(name)
            .map(|value| value.as_bytes().to_vec())
    }

    fn get_termcap_number(&mut self, cap: &str) -> Option<i32> {
        self.numbers.get(cap).copied()
    }

    fn get_flag(&mut self, cap: FlagCapability<'_>) -> bool {
        let cap = match cap {
            FlagCapability::Termcap(name) | FlagCapability::Terminfo(name) => name,
        };
        self.flags.contains(cap)
    }
}

#[test]
fn screen_terminfo_reports_no_italics_but_keeps_bold_and_underline() {
    let mut database = FakeCapabilityDatabase::screen_256color();
    let caps = resolve_tty_attribute_capabilities(&mut database, "");

    assert!(caps.italic_sequence.is_none(), "screen has no sitm");
    assert_eq!(
        caps.dim_sequence.as_deref(),
        Some(b"\x1b[2m".as_slice()),
        "screen has mh, so italics fall back to dim"
    );
    assert_eq!(caps.italic_rendition(), TtyItalicRendition::Dim(b"\x1b[2m"));
    assert_eq!(caps.bold(), Some(b"\x1b[1m".as_slice()));
    assert_eq!(caps.underline(), Some(b"\x1b[4m".as_slice()));
    assert_eq!(
        caps.standout_sequence.as_deref(),
        Some(b"\x1b[3m".as_slice())
    );
    assert!(caps.strike_through_sequence.is_none(), "screen has no smxx");
    assert!(caps.styled_underline.is_none(), "screen has no Smulx");
    assert_eq!(caps.color_cells(), 256);
    // GNU: `if (TN_no_color_video == -1) TN_no_color_video = 0'.
    assert_eq!(caps.no_color_video, TtyNoColorVideo::NONE);
}

#[test]
fn complete_standout_sequence_is_preserved() {
    let mut database = FakeCapabilityDatabase::bare()
        .with_string("so", "\x1b[0;1;3m$<2>")
        .with_string("se", "\x1b[27m")
        .with_number("Co", 256);

    let caps = resolve_tty_attribute_capabilities(&mut database, "");

    assert_eq!(
        caps.standout_sequence.as_deref(),
        Some(b"\x1b[0;1;3m$<2>".as_slice()),
    );
    assert!(caps.supports(TtyCapability::Inverse));
}

#[test]
fn capability_names_match_the_ones_gnu_reads_in_init_tty() {
    // GNU term.c: so / us / md / mh / ZH / smxx / Smulx, and tgetnum Co / NC.
    let mut database = FakeCapabilityDatabase::screen_256color()
        .with_string("ZH", "\x1b[3m")
        .with_string("smxx", "\x1b[9m")
        .with_string("Smulx", "\x1b[4:%p1%dm")
        .with_number("NC", 32);
    let caps = resolve_tty_attribute_capabilities(&mut database, "");

    assert_eq!(caps.italic_sequence.as_deref(), Some(b"\x1b[3m".as_slice()));
    assert_eq!(
        caps.italic_rendition(),
        TtyItalicRendition::Italic(b"\x1b[3m")
    );
    assert_eq!(
        caps.strike_through_sequence.as_deref(),
        Some(b"\x1b[9m".as_slice())
    );
    assert!(caps.styled_underline.is_some());
    // ncv bit 1<<5 is GNU's NC_BOLD: bold cannot be combined with colors here.
    assert_eq!(caps.no_color_video, TtyNoColorVideo::BOLD);
    assert!(!caps.supports(TtyCapability::Bold));
    assert!(caps.supports(TtyCapability::Underline));
}

/// GNU has a SECOND source for styled underlines, immediately below the
/// `Smulx` lookup: `if (!tty->TF_set_underline_style && tgetflag ("Su"))
/// tty->TF_set_underline_style = "\x1b[4:%p1%dm";` (src/term.c:4700-4703) --
/// the kitty default its own comment calls "not recommended".  Because
/// `TF_set_underline_style` also gates `TF_set_underline_color` (:4705-4708),
/// the flag turns on both, which is why one boolean models it here.
///
/// Ledger 158 recorded this as LATENT: of the 3,697 entries `toe -a` lists on
/// this machine exactly one carries `Su` -- `xterm-kitty` -- and it ships
/// `Smulx` too, so the `!TF_set_underline_style` guard is false and the
/// fallback fires zero times.  That made it the SECOND capability in this area
/// whose absence from the shipped database proves nothing, the first being the
/// `tgetstr`-vs-`tigetstr` namespace trap 158 fixed for `Smulx` itself.  The
/// prerequisite that trap raises -- can `tgetflag` even SEE an extended
/// terminfo boolean? -- is settled with `tic`, not with a search for a real
/// terminal (`tmp/pw175/pw175.src`, `tmp/pw175/su_probe.c`):
///
/// ```text
/// pw175-su-only        tgetflag(Su)=1  tigetstr(Smulx)=null  => GNU: yes (Su fallback)
/// pw175-su-and-smulx   tgetflag(Su)=1  tigetstr(Smulx)=FOUND => GNU: yes (Smulx)
/// pw175-neither        tgetflag(Su)=0  tigetstr(Smulx)=null  => GNU: no
/// ```
///
/// So `Su` is NOT a second dead lookup: unlike `tgetstr ("Smulx")`, which
/// answers null on every entry that has it, `tgetflag ("Su")` resolves the
/// extended boolean and GNU's fallback works.  Ledger 175.
#[test]
fn the_su_flag_is_a_styled_underline_where_smulx_is_absent_like_gnu() {
    let mut su_only = FakeCapabilityDatabase::screen_256color().with_flag("Su");
    assert!(
        resolve_tty_attribute_capabilities(&mut su_only, "")
            .styled_underline
            .is_some(),
        "Su without Smulx is GNU's kitty-sequence fallback (term.c:4700-4703)"
    );
    assert!(
        resolve_tty_attribute_capabilities(&mut su_only, "")
            .supports(TtyCapability::UnderlineStyled)
    );

    let mut both = FakeCapabilityDatabase::screen_256color()
        .with_string("Smulx", "\x1b[4:%p1%dm")
        .with_flag("Su");
    assert!(
        resolve_tty_attribute_capabilities(&mut both, "")
            .styled_underline
            .is_some(),
        "Smulx alone already answers; the flag is not consulted"
    );

    let mut neither = FakeCapabilityDatabase::screen_256color();
    assert!(
        resolve_tty_attribute_capabilities(&mut neither, "")
            .styled_underline
            .is_none(),
        "neither source: no styled underline, and so no underline colour"
    );
}

/// The capability record carries the entry's BYTES, not a flag, because that
/// is what GNU emits: `OUTPUT1_IF (tty, tty->TS_enter_bold_mode)`
/// (src/term.c:2061) is one field answering both "does this terminal have
/// bold?" and "what is bold spelled as here?".
///
/// Terminfo padding survives until output: `OUTPUT1` is `tputs`,
/// which turns `$<..>` into a DELAY and does no parameter expansion at all.
/// That is a different rule from [`canonical_cap`], which also strips `%pN` so
/// the update planner can compare a terminfo spelling with its termcap
/// translation -- a normalization that would corrupt a string being EMITTED.
#[test]
fn every_rendition_capability_carries_the_entrys_own_bytes() {
    let mut database = FakeCapabilityDatabase::bare()
        .with_string("so", "\x1b[7;31m")
        .with_string("se", "\x1b[27m")
        .with_string("us", "\x1bG8$<10>")
        .with_string("md", "\x1b[1;43m")
        .with_string("mh", "\x1bGp")
        .with_string("ZH", "\x1b[3;44m")
        .with_string("smxx", "\x1bG@")
        .with_number("Co", 8);
    let caps = resolve_tty_attribute_capabilities(&mut database, "");

    assert_eq!(
        caps.standout_sequence.as_deref(),
        Some(b"\x1b[7;31m".as_slice())
    );
    assert_eq!(
        caps.underline_sequence.as_deref(),
        Some(b"\x1bG8$<10>".as_slice()),
        "padding must survive until tputs emits this control"
    );
    assert_eq!(
        caps.bold_sequence.as_deref(),
        Some(b"\x1b[1;43m".as_slice())
    );
    assert_eq!(caps.dim_sequence.as_deref(), Some(b"\x1bGp".as_slice()));
    assert_eq!(
        caps.italic_sequence.as_deref(),
        Some(b"\x1b[3;44m".as_slice())
    );
    assert_eq!(
        caps.strike_through_sequence.as_deref(),
        Some(b"\x1bG@".as_slice())
    );
}

/// GNU runs `Smulx` through `tparam` (src/term.c:2083), which in a terminfo
/// build IS ncurses' `tparm` (src/terminfo.c:43-55), so a terminal whose
/// `Smulx` is not the kitty spelling gets its own sequence.  This port emitted
/// a fixed `ESC [ 4 : N m`.
///
/// Ledger 158 recorded this as invisible and ledger 186 measured it: of the
/// 1,862 unique entries `toe -a` lists here, 25 carry `Smulx` and all 25 spell
/// it `\E[4:%p1%dm`.  So the divergence is only observable against an entry
/// built for the purpose -- `tic -x tmp/pw186/ti/pw186.src`, whose
/// `pw186-smulx-semicolon` answers `tparm ("\E[4;%p1%dm", 3)` = `\E[4;3m`
/// (`tmp/pw186/smulx_probe.c`).  The expansion below runs through the SAME
/// ncurses `tparm`, so what is pinned is the expander GNU uses and not a
/// re-implementation of terminfo's format language.
#[test]
fn the_styled_underline_is_smulx_expanded_by_ncurses_tparm() {
    let mut kitty = FakeCapabilityDatabase::screen_256color().with_string("Smulx", "\x1b[4:%p1%dm");
    let styled = resolve_tty_attribute_capabilities(&mut kitty, "")
        .styled_underline
        .expect("Smulx present");
    assert_eq!(
        styled.sequence(UnderlineStyle::Wave),
        Some(b"\x1b[4:3m".as_slice())
    );

    let mut semicolon =
        FakeCapabilityDatabase::screen_256color().with_string("Smulx", "\x1b[4;%p1%dm");
    let styled = resolve_tty_attribute_capabilities(&mut semicolon, "")
        .styled_underline
        .expect("Smulx present");
    for (style, expected) in [
        (UnderlineStyle::Double, b"\x1b[4;2m".as_slice()),
        (UnderlineStyle::Wave, b"\x1b[4;3m".as_slice()),
        (UnderlineStyle::Dotted, b"\x1b[4;4m".as_slice()),
        (UnderlineStyle::Dashed, b"\x1b[4;5m".as_slice()),
    ] {
        assert_eq!(styled.sequence(style), Some(expected), "{style:?}");
    }
    // The two styles that never reach `Smulx` in GNU have no expansion here.
    assert_eq!(styled.sequence(UnderlineStyle::Line), None);
    assert_eq!(styled.sequence(UnderlineStyle::None), None);

    // A private-mode spelling that shares no bytes at all with the rule this
    // port used to emit (`pw186-smulx-private`).
    let mut private =
        FakeCapabilityDatabase::screen_256color().with_string("Smulx", "\x1b[>4%p1%dw");
    let styled = resolve_tty_attribute_capabilities(&mut private, "")
        .styled_underline
        .expect("Smulx present");
    assert_eq!(
        styled.sequence(UnderlineStyle::Wave),
        Some(b"\x1b[>43w".as_slice())
    );

    // GNU's `Su` fallback installs its own literal and expands THAT
    // (src/term.c:4700-4703), so the kitty spelling comes back.
    let mut su_only = FakeCapabilityDatabase::screen_256color().with_flag("Su");
    let styled = resolve_tty_attribute_capabilities(&mut su_only, "")
        .styled_underline
        .expect("Su is the second source");
    assert_eq!(
        styled.sequence(UnderlineStyle::Dotted),
        Some(b"\x1b[4:4m".as_slice())
    );
}

#[test]
fn a_terminal_with_no_attribute_strings_supports_nothing() {
    let mut database = FakeCapabilityDatabase::bare().with_number("Co", 0);
    let caps = resolve_tty_attribute_capabilities(&mut database, "");

    assert_eq!(caps.italic_rendition(), TtyItalicRendition::None);
    for capability in [
        TtyCapability::Bold,
        TtyCapability::Dim,
        TtyCapability::Italic,
        TtyCapability::Underline,
        TtyCapability::UnderlineStyled,
        TtyCapability::Inverse,
        TtyCapability::StrikeThrough,
    ] {
        assert!(!caps.supports(capability), "{capability:?} must be absent");
    }
}

#[test]
fn an_absent_color_count_is_monochrome_like_gnu() {
    // GNU only sets up colors when `op' (TS_orig_pair) exists; a terminfo entry
    // without `Co' has no colors, and then `ncv' never applies.
    let mut database = FakeCapabilityDatabase::bare()
        .with_string("md", "\x1b[1m")
        .with_number("Co", -1)
        .with_number("NC", 32);
    let caps = resolve_tty_attribute_capabilities(&mut database, "");

    assert_eq!(caps.color_cells(), 0);
    assert!(
        caps.supports(TtyCapability::Bold),
        "a monochrome terminal ignores ncv"
    );
}

// ---------------------------------------------------------------------------
// Update-planner capabilities (TermCaps): the gate must attest the exact
// bytes the encoder emits, not mere capability presence.
// ---------------------------------------------------------------------------

use neomacs_display_runtime::backend::tty::rif::{
    BlankTailMethod, RegionScrollMethod, RightMarginBehavior,
};

#[test]
fn xterm_shaped_entry_resolves_every_planner_capability() {
    let mut database = FakeCapabilityDatabase::xterm_like().with_flag("ut");
    let caps = resolve_term_caps(&mut database);
    assert_eq!(caps.scroll_region, Some(RegionScrollMethod::SuSd));
    assert_eq!(caps.right_margin, RightMarginBehavior::MagicWrap);
    assert!(caps.insert_delete_char);
    assert_eq!(
        caps.blank_tail,
        BlankTailMethod::EraseToEol {
            back_color_erase: true,
        }
    );
    assert!(caps.synchronized_output);
}

#[test]
fn right_margin_behavior_is_one_exhaustive_capability() {
    let mut no_wrap = FakeCapabilityDatabase::bare();
    let mut auto_wrap = FakeCapabilityDatabase::bare().with_flag("am");
    let mut magic_wrap = FakeCapabilityDatabase::bare()
        .with_flag("am")
        .with_flag("xn");

    assert_eq!(
        resolve_term_caps(&mut no_wrap).right_margin,
        RightMarginBehavior::NoAutoWrap
    );
    assert_eq!(
        resolve_term_caps(&mut auto_wrap).right_margin,
        RightMarginBehavior::AutoWrap
    );
    assert_eq!(
        resolve_term_caps(&mut magic_wrap).right_margin,
        RightMarginBehavior::MagicWrap
    );
}

#[test]
fn vt220_shaped_entry_scrolls_by_index_never_su_sd() {
    // The SU/SD-on-vt220 trap: cs attests DECSTBM, but CSI S/T is VT420+.
    // The entry's own sf/sr (LF and ESC M) attest the index form instead.
    let mut database = FakeCapabilityDatabase::vt220_like();
    let caps = resolve_term_caps(&mut database);
    assert_eq!(caps.scroll_region, Some(RegionScrollMethod::Index));
}

#[test]
fn decstbm_without_reverse_index_refuses_region_scrolls() {
    let mut database = FakeCapabilityDatabase::vt220_like().without_string("sr");
    let caps = resolve_term_caps(&mut database);
    assert_eq!(caps.scroll_region, None);
}

#[test]
fn missing_cursor_addressing_refuses_region_scrolls() {
    let mut database = FakeCapabilityDatabase::xterm_like().without_string("cm");
    let caps = resolve_term_caps(&mut database);
    assert_eq!(caps.scroll_region, None);
}

#[test]
fn non_ansi_insert_delete_strings_refuse_ich_dch() {
    // tvi955 HAS insert/delete-char capabilities; they are just not the
    // ANSI bytes the encoder hardcodes. Presence-gating would corrupt it.
    let mut database = FakeCapabilityDatabase::tvi955_like();
    let caps = resolve_term_caps(&mut database);
    assert!(!caps.insert_delete_char);
    assert_eq!(
        caps.blank_tail,
        BlankTailMethod::WriteSpaces,
        "tvi955 ce is not ESC[K"
    );
}

#[test]
fn back_color_erase_comes_from_the_ut_flag_alone() {
    let mut with_ut = FakeCapabilityDatabase::xterm_like().with_flag("ut");
    let mut without_ut = FakeCapabilityDatabase::xterm_like();
    assert_eq!(
        resolve_term_caps(&mut with_ut).blank_tail,
        BlankTailMethod::EraseToEol {
            back_color_erase: true,
        }
    );
    assert_eq!(
        resolve_term_caps(&mut without_ut).blank_tail,
        BlankTailMethod::EraseToEol {
            back_color_erase: false,
        }
    );
}

#[test]
fn insert_null_glitch_requires_written_blank_tails() {
    let mut database = FakeCapabilityDatabase::xterm_like().with_flag("in");

    assert_eq!(
        resolve_term_caps(&mut database).blank_tail,
        BlankTailMethod::WriteSpaces,
        "GNU's must_write_spaces comes directly from termcap `in`"
    );
}

#[test]
fn padding_and_parameter_markers_do_not_defeat_recognition() {
    // vt100-style entries carry delay padding ($<5>) on csr and terminfo
    // %p markers survive in some spellings; both canonicalize away.
    let mut database = FakeCapabilityDatabase::xterm_like()
        .with_string("cs", "\x1b[%i%p1%d;%p2%dr$<5>")
        .with_string("cm", "\x1b[%i%p1%d;%p2%dH");
    let caps = resolve_term_caps(&mut database);
    assert_eq!(caps.scroll_region, Some(RegionScrollMethod::SuSd));
}

/// Run native fixtures in a child process so TERMINFO is selected before
/// threads start. This keeps environment mutation and additional unsafe code
/// out of the tests, while making missing tools/entries an explicit failure.
#[cfg(unix)]
pub(crate) fn run_native_fixture_child() -> bool {
    let thread = std::thread::current();
    let name = thread.name().expect("named Rust test thread");
    if std::env::var("NEOMACS_APP_TERMINFO_FIXTURE_TEST").as_deref() == Ok(name) {
        return false;
    }
    let directory = tempfile::tempdir().unwrap();
    let compiled = std::process::Command::new("tic")
        .args(["-x", "-o"])
        .arg(directory.path())
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/terminal-capabilities.src"
        ))
        .output()
        .expect("ncurses tic is required for native fixtures");
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env("NEOMACS_APP_TERMINFO_FIXTURE_TEST", name)
        .env("TERMINFO", directory.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    true
}

/// Extended names must reach native tigetstr, rather than termcap lookup.
#[test]
#[cfg(unix)]
fn styled_underline_and_strike_through_come_from_the_terminfo_database() {
    if run_native_fixture_child() {
        return;
    }
    let styled = entry("neo-app-styled", "").expect("compiled styled entry");
    assert_eq!(
        styled
            .styled_underline
            .as_ref()
            .unwrap()
            .sequence(UnderlineStyle::Wave),
        Some(b"\x1b[4:3m".as_slice())
    );
    assert_eq!(
        styled.strike_through_sequence.as_deref(),
        Some(b"\x1b[9m".as_slice())
    );
    let plain = entry("neo-app-common", "").expect("compiled plain entry");
    assert_eq!(
        plain.strike_through_sequence.as_deref(),
        Some(b"\x1b[9m".as_slice())
    );
    assert!(plain.styled_underline.is_none());
}

/// The two-letter names must keep going to termcap.
///
/// `tigetstr` is the mirror image of `tgetstr`: it resolves TERMINFO names and
/// answers NULL for `us`, `so` and `ZH`, whose terminfo spellings are `smul`,
/// `smso` and `sitm`.  Moving every lookup to terminfo would break the other
/// direction just as silently.
#[test]
fn two_letter_capability_names_still_come_from_termcap() {
    let Some(xterm) = entry("xterm-256color", "") else {
        return;
    };
    // The bytes, not just the presence: this is the one test in the file that
    // reads a REAL terminfo entry, so it is where the entry's own spelling can
    // be pinned against something other than a fake table.
    assert_eq!(
        xterm.underline_sequence.as_deref(),
        Some(b"\x1b[4m".as_slice()),
        "xterm-256color has us"
    );
    assert_eq!(
        xterm.bold_sequence.as_deref(),
        Some(b"\x1b[1m".as_slice()),
        "xterm-256color has md"
    );
    assert_eq!(
        xterm.italic_sequence.as_deref(),
        Some(b"\x1b[3m".as_slice()),
        "xterm-256color has ZH"
    );
    assert_eq!(
        xterm.standout_sequence.as_deref(),
        Some(b"\x1b[7m".as_slice()),
        "xterm-256color has so, and the writer needs its bytes"
    );
    assert_eq!(xterm.color_cells(), 256, "xterm-256color has Co#256");
}

/// Resolve a native entry with explicit COLORTERM so the environment cannot
/// replace its own color spelling with GNU's truecolor fallback.
fn entry(term: &str, colorterm: &str) -> Option<TtyAttributeCapabilities> {
    let mut database = open_terminal_capability_database(term)?;
    Some(resolve_tty_attribute_capabilities(
        database.as_mut(),
        colorterm,
    ))
}

#[test]
#[cfg(unix)]
fn a_colour_is_the_entrys_own_setaf_expanded_by_tparm() {
    use neomacs_display_protocol::TerminalColor;
    use neomacs_display_protocol::tty_capabilities::ColorGround;
    if run_native_fixture_child() {
        return;
    }
    let rgb = TerminalColor::Direct {
        r: 205,
        g: 0,
        b: 17,
    };
    for (term, ground, color, expected) in [
        (
            "neo-app-colon",
            ColorGround::Foreground,
            TerminalColor::Indexed(100),
            "\x1b[38:5:100m",
        ),
        (
            "neo-app-linux16",
            ColorGround::Foreground,
            TerminalColor::Indexed(1),
            "\x1b[31;22m",
        ),
        (
            "neo-app-linux16",
            ColorGround::Background,
            TerminalColor::Indexed(4),
            "\x1b[44;25m",
        ),
        (
            "neo-app-svr4",
            ColorGround::Foreground,
            TerminalColor::Indexed(1),
            "\x1b[34m",
        ),
        (
            "neo-app-separate",
            ColorGround::Foreground,
            rgb,
            "\x1b[38:2:205:0:17m",
        ),
        (
            "neo-app-packed",
            ColorGround::Foreground,
            rgb,
            "\x1b[38:2::205:0:17m",
        ),
    ] {
        let caps = entry(term, "").expect("compiled color entry");
        let colors = caps
            .colors
            .entry()
            .expect("fixture supplies color capabilities");
        assert_eq!(
            colors.ground_sequence(ground, color),
            Some(expected.as_bytes().to_vec()),
            "{term}"
        );
        assert_eq!(colors.orig_pair(), b"\x1b[39;49m");
    }
    let indexed = entry("neo-app-indexed", "").unwrap();
    let colors = indexed.colors.entry().unwrap();
    for (index, expected) in [
        (0, "\x1b[30m"),
        (7, "\x1b[37m"),
        (8, "\x1b[90m"),
        (15, "\x1b[97m"),
        (100, "\x1b[38;5;100m"),
    ] {
        assert_eq!(
            colors.ground_sequence(ColorGround::Foreground, TerminalColor::Indexed(index)),
            Some(expected.as_bytes().to_vec()),
            "index {index}"
        );
    }
}

/// The application must preserve native termcap normalization and custom
/// reset bytes. Installed `linux` and `xterm` entries vary across ncurses releases.
#[test]
#[cfg(unix)]
fn the_exit_attribute_string_is_the_entrys_own_me() {
    if run_native_fixture_child() {
        return;
    }
    let normalized = entry("neo-app-normalized-reset", "").unwrap();
    assert_eq!(
        normalized.exit_attribute_mode.as_deref(),
        Some(b"\x1b[0m".as_slice()),
        "native termcap strips the alternate-charset reset"
    );
    assert_eq!(
        normalized.exit_underline_mode.as_deref(),
        Some(b"\x1b[24m".as_slice())
    );
    let custom = entry("neo-app-custom-reset", "").unwrap();
    assert_eq!(
        custom.exit_attribute_mode.as_deref(),
        Some(b"CUSTOM-RESET".as_slice())
    );
    assert_eq!(
        custom.exit_underline_mode.as_deref(),
        Some(b"UNDERLINE-OFF".as_slice())
    );
}

/// GNU reads the whole colour block behind `if (tty->TS_orig_pair)` and
/// supports no colour without it (src/term.c:4602-4606).
#[test]
fn a_colourless_entry_is_one_absent_op_away_like_gnu() {
    let mut with_op = FakeCapabilityDatabase::bare()
        .with_string("op", "\x1b[39;49m")
        .with_string("AF", "\x1b[3%p1%dm")
        .with_string("AB", "\x1b[4%p1%dm")
        .with_number("Co", 8);
    assert!(
        resolve_tty_attribute_capabilities(&mut with_op, "")
            .colors
            .entry()
            .is_some()
    );

    let mut without_op = FakeCapabilityDatabase::bare()
        .with_string("AF", "\x1b[3%p1%dm")
        .with_string("AB", "\x1b[4%p1%dm")
        .with_number("Co", 8);
    assert!(
        resolve_tty_attribute_capabilities(&mut without_op, "")
            .colors
            .entry()
            .is_none(),
        "no `op` is no colour, whatever `Co` says"
    );

    // GNU's `Tc`/COLORTERM arm installs its OWN literal rather than the
    // entry's, and sets `TF_rgb_separate` (src/term.c:4655-4667).
    let mut truecolor = FakeCapabilityDatabase::bare()
        .with_string("op", "\x1b[39;49m")
        .with_string("AF", "\x1b[3%p1%dm")
        .with_string("AB", "\x1b[4%p1%dm")
        .with_number("Co", 8);
    let colors = resolve_tty_attribute_capabilities(&mut truecolor, "TrueColor")
        .colors
        .entry()
        .cloned()
        .expect("op present");
    assert_eq!(
        colors.ground_sequence(
            neomacs_display_protocol::tty_capabilities::ColorGround::Foreground,
            neomacs_display_protocol::TerminalColor::Direct { r: 1, g: 2, b: 3 }
        ),
        Some(b"\x1b[38;2;1;2;3m".to_vec()),
        "GNU's own literal, expanded through the same tparm"
    );
    // `op` with NO setter is a state GNU is in for three reachable entries --
    // `foot+base`, `kitty+common`, `linux-m` -- and it still emits `op` there,
    // because `TS_orig_pair` gates the block while each setter is tested again
    // at the emission site (`if (face_tty_specified_color (fg) && ts)`,
    // src/term.c:2099).
    let mut op_only = FakeCapabilityDatabase::bare()
        .with_string("op", "\x1b[39;49m")
        .with_number("Co", 8);
    let colors = resolve_tty_attribute_capabilities(&mut op_only, "")
        .colors
        .entry()
        .cloned()
        .expect("op alone is still GNU's colour block");
    assert_eq!(colors.orig_pair(), b"\x1b[39;49m");
    assert_eq!(
        colors.ground_sequence(
            neomacs_display_protocol::tty_capabilities::ColorGround::Foreground,
            neomacs_display_protocol::TerminalColor::Indexed(1)
        ),
        None,
        "no setter, so turn_on_face emits nothing for the colour"
    );

    // GNU's SVr4 fallback is tested on the FOREGROUND alone and replaces BOTH
    // (src/term.c:4609-4614).  No entry ncurses ships has `AF` without `AB`, so
    // this is pinned against a table rather than a terminal.
    let mut af_without_ab = FakeCapabilityDatabase::bare()
        .with_string("op", "\x1b[39;49m")
        .with_string("AF", "\x1b[3%p1%dm")
        .with_string("Sf", "\x1b[SVR4-%p1%dm")
        .with_string("Sb", "\x1b[SVR4BG-%p1%dm")
        .with_number("Co", 8);
    let colors = resolve_tty_attribute_capabilities(&mut af_without_ab, "")
        .colors
        .entry()
        .cloned()
        .expect("op present");
    assert_eq!(
        colors.ground_sequence(
            neomacs_display_protocol::tty_capabilities::ColorGround::Foreground,
            neomacs_display_protocol::TerminalColor::Indexed(1)
        ),
        Some(b"\x1b[31m".to_vec()),
        "AF is present, so the SVr4 fallback must NOT fire"
    );
    assert_eq!(
        colors.ground_sequence(
            neomacs_display_protocol::tty_capabilities::ColorGround::Background,
            neomacs_display_protocol::TerminalColor::Indexed(1)
        ),
        None,
        "and AB stays absent rather than borrowing Sb"
    );

    // And COLORTERM that is not "truecolor" leaves the entry's own spelling
    // alone: GNU's test is `strcasecmp (bg, "truecolor") == 0`.
    let mut other = FakeCapabilityDatabase::bare()
        .with_string("op", "\x1b[39;49m")
        .with_string("AF", "\x1b[3%p1%dm")
        .with_string("AB", "\x1b[4%p1%dm")
        .with_number("Co", 8);
    let colors = resolve_tty_attribute_capabilities(&mut other, "rxvt")
        .colors
        .entry()
        .cloned()
        .expect("op present");
    assert_eq!(
        colors.ground_sequence(
            neomacs_display_protocol::tty_capabilities::ColorGround::Foreground,
            neomacs_display_protocol::TerminalColor::Indexed(1)
        ),
        Some(b"\x1b[31m".to_vec())
    );
}

// ---------------------------------------------------------------------------
// `TN_max_colors`: ledger 188's handed-over residual (its "Found and NOT
// fixed"), which is the COUNT rather than the spelling.
// ---------------------------------------------------------------------------

/// GNU computes `TN_max_colors` **once**, inside `init_tty`'s `op` gate, and
/// the same `else if` chain that picks `TS_set_foreground` picks it
/// (src/term.c:4602-4674):
///
/// ```c
///   tty->TS_orig_pair = tgetstr ("op", address);
///   if (tty->TS_orig_pair)
///     {
///       ...
///       tty->TN_max_colors = tgetnum ("Co");
///       if (setf24 && setb24)            tty->TN_max_colors = 16777216;
///       else if (setrgbf && setrgbb)     tty->TN_max_colors = 16777216;
///       else if (tigetflag ("RGB") > 0)  tty->TN_max_colors = 16777216;
///       else if (tigetflag ("Tc") > 0
///                || (getenv ("COLORTERM")
///                    && strcasecmp (bg, "truecolor") == 0))
///                                        tty->TN_max_colors = 16777216;
///     }
/// ```
///
/// So a `Co` read outside the gate is not `TN_max_colors`, and neither is a
/// `Co` that one of the four 24-bit arms replaced.  Measured end-to-end in a
/// pty, GNU 31.0.90 against this port's merge-base binary,
/// `(display-color-cells)`:
///
/// ```text
///   TERM=xterm-kitty  COLORTERM unset   GNU 16777216   this port 256
///   TERM=amiga-vnc    COLORTERM unset   GNU 0          this port 16
///   TERM=djgpp204     COLORTERM unset   GNU 0          this port 8
///   TERM=vwmterm      COLORTERM unset   GNU 0          this port 8
///   TERM=xterm        COLORTERM=24bit   GNU 8          this port 16777216
/// ```
#[test]
fn the_colour_count_is_read_inside_gnus_op_gate() {
    // Base: `op` present, `Co` is the answer.
    let mut plain = FakeCapabilityDatabase::bare()
        .with_string("op", "\x1b[39;49m")
        .with_string("AF", "\x1b[3%p1%dm")
        .with_string("AB", "\x1b[4%p1%dm")
        .with_number("Co", 8);
    assert_eq!(
        resolve_tty_attribute_capabilities(&mut plain, "").color_cells(),
        8
    );

    // `amiga-vnc`, `djgpp204`, `vwmterm`: a colour count and no `op`.  GNU
    // never reaches `tgetnum ("Co")` for them, so `TN_max_colors` keeps its
    // zero and `tty-display-color-p` answers nil.
    let mut no_op = FakeCapabilityDatabase::bare()
        .with_string("AF", "\x1b[3%p1%dm")
        .with_string("AB", "\x1b[4%p1%dm")
        .with_number("Co", 16);
    let caps = resolve_tty_attribute_capabilities(&mut no_op, "");
    assert_eq!(
        caps.color_cells(),
        0,
        "no `op` is no colour, and that includes the COUNT"
    );
    assert!(!caps.supports_color());

    // And COLORTERM cannot open the gate either: GNU reads it inside the
    // block, never before it.
    let mut no_op_truecolor = FakeCapabilityDatabase::bare()
        .with_string("AF", "\x1b[3%p1%dm")
        .with_number("Co", 16);
    assert_eq!(
        resolve_tty_attribute_capabilities(&mut no_op_truecolor, "truecolor").color_cells(),
        0
    );
}

/// GNU's four 24-bit arms each set `TN_max_colors = 16777216`, in this order
/// (src/term.c:4625-4667).  `xterm-kitty` takes the `setrgbf` one with
/// COLORTERM unset, which is why GNU answers 16777216 there and this port
/// answered `Co`.
#[test]
fn the_four_direct_colour_arms_promote_the_count_like_gnu() {
    let base = || {
        FakeCapabilityDatabase::bare()
            .with_string("op", "\x1b[39;49m")
            .with_string("AF", "\x1b[3%p1%dm")
            .with_string("AB", "\x1b[4%p1%dm")
            .with_number("Co", 8)
    };

    // 1. GNU's own non-standard `setf24`/`setb24`.
    let mut setf24 = base()
        .with_string("setf24", "\x1b[38;2;%p1%d;%p2%d;%p3%dm")
        .with_string("setb24", "\x1b[48;2;%p1%d;%p2%d;%p3%dm");
    assert_eq!(
        resolve_tty_attribute_capabilities(&mut setf24, "").color_cells(),
        16_777_216
    );

    // 2. `setrgbf`/`setrgbb` -- `xterm-kitty`'s route.
    let mut setrgbf = base()
        .with_string("setrgbf", "\x1b[38:2:%p1%d:%p2%d:%p3%dm")
        .with_string("setrgbb", "\x1b[48:2:%p1%d:%p2%d:%p3%dm");
    assert_eq!(
        resolve_tty_attribute_capabilities(&mut setrgbf, "").color_cells(),
        16_777_216
    );

    // 3. The standard `RGB` flag -- the `*-direct` entries.  GNU replaces
    //    nothing here but the COUNT, which is the whole point of the arm.
    let mut rgb = base().with_flag("RGB");
    assert_eq!(
        resolve_tty_attribute_capabilities(&mut rgb, "").color_cells(),
        16_777_216
    );

    // 4. `Tc`, and COLORTERM as its equivalent.
    let mut tc = base().with_flag("Tc");
    assert_eq!(
        resolve_tty_attribute_capabilities(&mut tc, "").color_cells(),
        16_777_216
    );
    let mut colorterm = base();
    assert_eq!(
        resolve_tty_attribute_capabilities(&mut colorterm, "TrueColor").color_cells(),
        16_777_216
    );
}

/// GNU's COLORTERM test is `strcasecmp (bg, "truecolor") == 0`
/// (src/term.c:4659-4661) -- an EXACT match, case-insensitively, and nothing
/// else.  This port matched a SUBSTRING and also accepted `24bit`, which no
/// arm of GNU reads.  Measured in a pty: `TERM=xterm COLORTERM=24bit` answers
/// 8 in GNU 31.0.90 and answered 16777216 here.
#[test]
fn only_the_exact_colorterm_gnu_reads_promotes_the_count() {
    let base = || {
        FakeCapabilityDatabase::bare()
            .with_string("op", "\x1b[39;49m")
            .with_string("AF", "\x1b[3%p1%dm")
            .with_string("AB", "\x1b[4%p1%dm")
            .with_number("Co", 8)
    };
    for spelling in ["truecolor", "TRUECOLOR", "TrueColor"] {
        let mut database = base();
        assert_eq!(
            resolve_tty_attribute_capabilities(&mut database, spelling).color_cells(),
            16_777_216,
            "GNU compares with strcasecmp, so {spelling:?} is truecolor"
        );
    }
    for spelling in ["24bit", "24-bit", "truecolor-ish", "rxvt", ""] {
        let mut database = base();
        assert_eq!(
            resolve_tty_attribute_capabilities(&mut database, spelling).color_cells(),
            8,
            "GNU reads no arm for COLORTERM={spelling:?}, so the count stays `Co`"
        );
    }
}

/// The colour count and the colour SPELLING are one decision in GNU, so they
/// must be one value here: an entry with no `op` cannot carry a count, and an
/// entry that took a 24-bit arm cannot carry `Co`.
///
/// This is the state ledger 188 left representable -- `colors: Absent` beside
/// `color_cells: 16` -- which is exactly what `amiga-vnc` was in.
#[test]
fn a_colourless_source_can_carry_no_count() {
    let mut no_op = FakeCapabilityDatabase::bare().with_number("Co", 16);
    let caps = resolve_tty_attribute_capabilities(&mut no_op, "");
    assert_eq!(caps.colors, TtyColorSource::Absent);
    assert_eq!(caps.color_cells(), 0);

    // ...and the record with no terminfo entry at all is the one state GNU
    // cannot be in, so it carries its own count rather than borrowing one.
    assert_eq!(
        TtyAttributeCapabilities::none().color_cells(),
        0,
        "a `dumb'-shaped entry is monochrome"
    );
}

/// The real entries, so the rule is measured against the terminfo database
/// this machine actually has rather than against a fake of it.
///
/// Every row is the pty measurement of GNU 31.0.90 in this entry's §1 table.
/// A machine without one of these entries makes that row vacuous rather than
/// red, which is the one condition under which "absent" is the right answer.
#[test]
fn real_entries_answer_gnus_own_colour_count() {
    for (term, colorterm, cells) in [
        ("xterm-kitty", "", 16_777_216),
        ("xterm-kitty", "truecolor", 16_777_216),
        ("amiga-vnc", "", 0),
        ("djgpp204", "", 0),
        ("vwmterm", "", 0),
        ("xterm", "truecolor", 16_777_216),
        ("xterm", "24bit", 8),
        ("xterm", "", 8),
        ("linux", "", 8),
        ("linux-16color", "", 16),
        ("rxvt-16color", "", 16),
        ("screen-256color", "", 256),
        ("xterm-256color", "", 256),
        ("xterm-direct", "", 16_777_216),
    ] {
        let Some(caps) = entry(term, colorterm) else {
            continue;
        };
        assert_eq!(
            caps.color_cells(),
            cells,
            "TERM={term} COLORTERM={colorterm:?}: GNU 31.0.90 answers \
             (display-color-cells) {cells} in a pty"
        );
    }
}

#[test]
fn standout_requires_an_exit_and_falls_back_after_cookie_checks() {
    let mut db = FakeCapabilityDatabase::bare()
        .with_string("so", "SO")
        .with_string("se", "SE")
        .with_string("us", "US")
        .with_string("ue", "UE");
    let caps = resolve_tty_attribute_capabilities(&mut db, "");
    assert_eq!(caps.standout_sequence.as_deref(), Some(b"SO".as_slice()));
    assert_eq!(caps.exit_standout_mode.as_deref(), Some(b"SE".as_slice()));
    for cookie in [0, 1, 2] {
        db.numbers.insert("sg", cookie);
        let caps = resolve_tty_attribute_capabilities(&mut db, "");
        assert_eq!(caps.standout_sequence.as_deref(), Some(b"US".as_slice()));
        assert_eq!(caps.exit_standout_mode.as_deref(), Some(b"UE".as_slice()));
    }
    db.numbers.insert("ug", 0);
    let caps = resolve_tty_attribute_capabilities(&mut db, "");
    assert!(!caps.supports(TtyCapability::Inverse));
    assert!(!caps.supports(TtyCapability::Underline));
    assert!(caps.exit_underline_mode.is_none());
    db.numbers.clear();
    db.strings.remove("se");
    assert!(!resolve_tty_attribute_capabilities(&mut db, "").supports(TtyCapability::Inverse));
    db.strings.insert("me", "ME");
    let caps = resolve_tty_attribute_capabilities(&mut db, "");
    assert_eq!(caps.standout_sequence.as_deref(), Some(b"SO".as_slice()));
    assert_eq!(caps.exit_standout_mode.as_deref(), Some(b"ME".as_slice()));
}
