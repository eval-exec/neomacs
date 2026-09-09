//! The terminfo/termcap access point for this terminal.
//!
//! GNU reads every terminal capability it needs in one place — `term.c:init_tty`
//! — and stores the answers on the terminal: function-key sequences for
//! `input-decode-map`, attribute sequences for `turn_on_face`, and the color
//! numbers for `tty_capable_p`. neomacs had only the input half (see
//! `super::termcap_input`) while output attributes were hardcoded in the
//! renderer, so `:slant italic` was emitted as an italic escape even on a
//! terminal whose terminfo has no `sitm` — where GNU emits its dim fallback.
//!
//! This module requests an owned snapshot so both halves ask the same terminfo
//! entry the same way.

use neomacs_display_protocol::tty_capabilities::{
    TerminfoExpander, TerminfoParameters, TtyAttributeCapabilities, TtyColorCapabilities,
    TtyColorDepth, TtyColorSource, TtyDirectColorRoute, TtyNoColorVideo, TtyStyledUnderline,
};

/// The safe dependency validates numeric formats before invoking ncurses.
fn expand_capability_parameter(sequence: &[u8], parameters: TerminfoParameters) -> Option<Vec<u8>> {
    let mut values = [0; 9];
    match parameters {
        TerminfoParameters::One(value) => values[0] = i32::try_from(value).ok()?,
        TerminfoParameters::Rgb { r, g, b } => {
            values[..3].copy_from_slice(&[i32::from(r), i32::from(g), i32::from(b)]);
        }
    }
    let expanded = neomacs_terminfo::expand_numeric(sequence, values).ok()?;
    (!expanded.is_empty()).then_some(expanded)
}

const TERMINFO_EXPANDER: TerminfoExpander = TerminfoExpander::new(expand_capability_parameter);

pub(crate) use neomacs_terminfo::{FlagCapability, StringCapability};

/// A source of terminal capabilities — terminfo in production, a table in tests.
pub(crate) trait TerminalCapabilityDatabase {
    /// A string capability, read from the namespace its name belongs to (GNU
    /// `tgetstr` or `tigetstr`).  `None` when the entry lacks it.
    fn get_string(&mut self, cap: StringCapability<'_>) -> Option<Vec<u8>>;

    /// A numeric capability (GNU `tgetnum`). `None`, like GNU's `-1`, when the
    /// entry lacks it.  Every number GNU reads -- `Co`, `NC` -- has a
    /// two-letter termcap name, so there is no terminfo variant here.
    fn get_termcap_number(&mut self, cap: &str) -> Option<i32>;

    /// A boolean capability in its explicit namespace: termcap `ut` for
    /// back-color-erase, or terminfo `RGB`/`Tc` for direct color.
    fn get_flag(&mut self, cap: FlagCapability<'_>) -> bool;
}

pub(crate) fn open_terminal_capability_database(
    term: &str,
) -> Option<Box<dyn TerminalCapabilityDatabase>> {
    use StringCapability::{Termcap, Terminfo};
    use neomacs_terminfo::Query;
    // The application chooses what it needs; the dependency snapshots these
    // queries together so later opens cannot change an existing database.
    let mut queries = vec![Query::TermcapNumber("Co"), Query::TermcapNumber("NC")];
    for name in ["Su", "xn", "am", "in", "ut"] {
        queries.push(Query::Flag(FlagCapability::Termcap(name)));
    }
    for name in [
        "so", "us", "md", "mh", "ZH", "me", "ue", "op", "AF", "AB", "Sf", "Sb", "cs", "cm", "SF",
        "SR", "sf", "sr", "IC", "DC", "ce",
    ] {
        queries.push(Query::String(Termcap(name)));
    }
    for name in ["RGB", "Tc"] {
        queries.push(Query::Flag(FlagCapability::Terminfo(name)));
    }
    for name in ["Smulx", "smxx", "setf24", "setb24", "setrgbf", "setrgbb"] {
        queries.push(Query::String(Terminfo(name)));
    }
    let key_names = super::termcap_input::terminal_key_capabilities();
    queries.extend(key_names.iter().map(|name| Query::String(Termcap(name))));
    match neomacs_terminfo::Database::load(term, &queries) {
        Ok(database) => Some(Box::new(database)),
        Err(error) => {
            tracing::debug!(%error, term, "could not load terminal capabilities");
            None
        }
    }
}

/// Resolve what this terminal can render, reading the same capability names GNU
/// reads in `init_tty`:
///
/// | capability | namespace | GNU field | meaning |
/// |---|---|---|---|
/// | `so` | termcap | `TS_standout_mode` | inverse video |
/// | `us` | termcap | `TS_enter_underline_mode` | underline |
/// | `Smulx` | terminfo | `TF_set_underline_style` | styled underline |
/// | `Su` | termcap flag | `TF_set_underline_style` | styled underline, kitty default |
/// | `md` | termcap | `TS_enter_bold_mode` | bold |
/// | `mh` | termcap | `TS_enter_dim_mode` | dim (and GNU's italic fallback) |
/// | `ZH` | termcap | `TS_enter_italic_mode` | italic (`sitm`) |
/// | `smxx` | terminfo | `TS_enter_strike_through_mode` | strike-through |
/// | `Co` | termcap | `TN_max_colors` | color cells |
/// | `NC` | termcap | `TN_no_color_video` | attributes unusable with colors |
///
/// The namespace column is not decoration; see [`StringCapability`].  Reading
/// `Smulx` and `smxx` out of termcap answers "absent" on every terminal that
/// has ever existed.
pub(crate) fn resolve_tty_attribute_capabilities(
    database: &mut dyn TerminalCapabilityDatabase,
    colorterm: &str,
) -> TtyAttributeCapabilities {
    use StringCapability::{Termcap, Terminfo};

    // GNU stores the capability's STRING and emits it (`OUTPUT1_IF`), so the
    // record carries bytes rather than a flag: presence is `is_some`.
    let sequence = |database: &mut dyn TerminalCapabilityDatabase, cap: StringCapability<'_>| {
        database
            .get_string(cap)
            .filter(|value| !value.is_empty())
            .map(|value| rendition_sequence(&value))
            .filter(|value| !value.is_empty())
    };
    // GNU: `TN_no_color_video = tgetnum ("NC"); if (== -1) TN_no_color_video = 0'.
    let no_color_video = database
        .get_termcap_number("NC")
        .filter(|ncv| *ncv > 0)
        .map_or(TtyNoColorVideo::NONE, |ncv| TtyNoColorVideo(ncv as u16));
    // GNU takes styled underlines from EITHER source, `Smulx` first:
    // `if (!tty->TF_set_underline_style && tgetflag ("Su"))
    //    tty->TF_set_underline_style = "\x1b[4:%p1%dm";`
    // (src/term.c:4700-4703).  Because that field also gates
    // `TF_set_underline_color` (:4705-4708), one answer carries both.
    // `Su` is a flag, not a string, so it is read with `tgetflag` -- and
    // unlike `tgetstr ("Smulx")`, `tgetflag` really does resolve an
    // extended terminfo boolean (ledger 175, measured with a `tic`-built
    // entry because no shipped entry has `Su` without `Smulx`).
    //
    // What the field holds is the ENTRY's own string, and `turn_on_face`
    // expands it with `tparam` (src/term.c:2083); the `Su` arm expands the
    // literal GNU installs for it.  Every `Smulx` ncurses ships is spelled
    // `\E[4:%p1%dm`, so this is invisible on the shipped database and
    // measurable only against a `tic`-built entry (ledger 186,
    // `tmp/pw186/ti/pw186.src`).
    let styled_underline_source = database
        .get_string(Terminfo("Smulx"))
        .filter(|value| !value.is_empty())
        .or_else(|| {
            database
                .get_flag(FlagCapability::Termcap("Su"))
                .then(|| b"\x1b[4:%p1%dm".to_vec())
        });

    TtyAttributeCapabilities {
        standout_sequence: sequence(database, Termcap("so")),
        underline_sequence: sequence(database, Termcap("us")),
        bold_sequence: sequence(database, Termcap("md")),
        dim_sequence: sequence(database, Termcap("mh")),
        italic_sequence: sequence(database, Termcap("ZH")),
        strike_through_sequence: sequence(database, Terminfo("smxx")),
        styled_underline: styled_underline_source.and_then(|smulx| {
            TtyStyledUnderline::expand_all(|style| {
                TERMINFO_EXPANDER.expand(&smulx, TerminfoParameters::One(u32::from(style)))
            })
        }),
        // GNU `TS_exit_attribute_mode = tgetstr ("me")` (src/term.c:4585) and
        // `TS_exit_underline_mode = tgetstr ("ue")` (:4578).  The string that
        // matters is what TERMCAP answers, not what `infocmp` prints for
        // `sgr0`: ncurses' termcap layer normalises it, and `Eterm`'s `sgr0` is
        // `\E[m\017` while its `me` is `\E[0m` (ledger 188).
        exit_attribute_mode: sequence(database, Termcap("me")),
        exit_underline_mode: sequence(database, Termcap("ue")),
        // GNU reads `Co` INSIDE this block and nowhere else, so the count
        // comes back with the setters rather than beside them (ledger 193).
        colors: resolve_tty_color_capabilities(database, colorterm),
        no_color_video,
    }
}

/// GNU's colour block of `init_tty` (src/term.c:4602-4674), whole.
///
/// The structure is the rule and it is why this returns ONE answer rather than
/// four independently-absent fields: GNU reads `op` FIRST and reads nothing
/// else unless it is there --
///
/// ```c
///   /* SVr4/ANSI color support.  If "op" isn't available, don't support
///      color because we can't switch back to the default foreground and
///      background.  */
///   tty->TS_orig_pair = tgetstr ("op", address);
///   if (tty->TS_orig_pair)
///     {
///       tty->TS_set_foreground = tgetstr ("AF", address);
///       ...
/// ```
///
/// Three of the 927 terminfo entries this port will start on have a colour
/// count and no `op` -- `amiga-vnc`, `djgpp204`, `vwmterm` -- and GNU renders
/// them monochrome for exactly this reason.
///
/// Inside the gate the precedence is GNU's too: `AF`/`AB`, falling back to
/// SVr4 `Sf`/`Sb`; then the four 24-bit routes in GNU's order, of which
/// `setf24` and `setrgbf` replace the setters with the ENTRY's own strings and
/// `Tc`/`COLORTERM` installs GNU's own literal.  `RGB` replaces nothing: the
/// entry's `setaf` keeps its spelling and receives the packed pixel, which is
/// what the 20 reachable `*-direct` entries do.
fn resolve_tty_color_capabilities(
    database: &mut dyn TerminalCapabilityDatabase,
    colorterm: &str,
) -> TtyColorSource {
    resolve_tty_color_entry(database, colorterm)
        .map_or(TtyColorSource::Absent, TtyColorSource::Entry)
}

/// The block itself, as an `Option` so GNU's `?`-shaped gates read as GNU's.
/// `None` here is GNU's `TN_max_colors == 0`, which
/// [`TtyColorSource::Absent`] names -- never the no-database state, which only
/// [`TtyAttributeCapabilities::full`] produces.
fn resolve_tty_color_entry(
    database: &mut dyn TerminalCapabilityDatabase,
    colorterm: &str,
) -> Option<TtyColorCapabilities> {
    use StringCapability::{Termcap, Terminfo};

    let orig_pair = rendition_capability(database, Termcap("op"))?;
    let mut set_foreground = rendition_capability(database, Termcap("AF"));
    let mut set_background = rendition_capability(database, Termcap("AB"));
    // `tty->TN_max_colors = tgetnum ("Co")` (src/term.c:4616) -- INSIDE the
    // gate, which is why it is read here and not with `NC`.  GNU's `-1` for an
    // absent `Co` becomes 0, since `TN_max_colors > 0` is the only question
    // asked of it.
    let indexed = TtyColorDepth::Indexed(
        database
            .get_termcap_number("Co")
            .filter(|colors| *colors > 0)
            .unwrap_or(0)
            .unsigned_abs(),
    );
    // GNU's fallback is tested on the FOREGROUND alone and replaces both:
    // `if (!tty->TS_set_foreground) { /* SVr4. */ ... }` (src/term.c:4609-4614).
    // Testing the pair instead would differ for an entry with `AF` and no
    // `AB`; ncurses ships none, measured (`tmp/pw188/asym.py`), but a rule
    // that happens to be unobservable is still the wrong rule.
    if set_foreground.is_none() {
        set_foreground = rendition_capability(database, Termcap("Sf"));
        set_background = rendition_capability(database, Termcap("Sb"));
    }

    // GNU's own non-standard 24-bit support, then the standard one, then the
    // de-facto one -- in GNU's order, because they are `else if`s.
    if let (Some(fg), Some(bg)) = (
        rendition_capability(database, Terminfo("setf24")),
        rendition_capability(database, Terminfo("setb24")),
    ) {
        return Some(TtyColorCapabilities::new(
            orig_pair,
            Some(fg),
            Some(bg),
            false,
            TtyColorDepth::Direct(TtyDirectColorRoute::Setf24),
            TERMINFO_EXPANDER,
        ));
    }
    if let (Some(fg), Some(bg)) = (
        rendition_capability(database, Terminfo("setrgbf")),
        rendition_capability(database, Terminfo("setrgbb")),
    ) {
        return Some(TtyColorCapabilities::new(
            orig_pair,
            Some(fg),
            Some(bg),
            true,
            TtyColorDepth::Direct(TtyDirectColorRoute::Setrgbf),
            TERMINFO_EXPANDER,
        ));
    }
    // `RGB` replaces no STRING in GNU -- the setters keep the entry's own
    // spelling and take the packed pixel -- but it does replace the COUNT
    // (`tty->TN_max_colors = 16777216`, src/term.c:4651), which is the whole
    // content of the arm.
    if database.get_flag(FlagCapability::Terminfo("RGB")) {
        return Some(TtyColorCapabilities::new(
            orig_pair,
            set_foreground,
            set_background,
            false,
            TtyColorDepth::Direct(TtyDirectColorRoute::RgbFlag),
            TERMINFO_EXPANDER,
        ));
    }
    // "Fall back to direct colour by RGB value (semicolon version) if Tc is set
    // (de-facto standard introduced by tmux) or if requested by the COLORTERM
    // environment variable" (src/term.c:4655-4667).  GNU installs its OWN
    // literal here rather than the entry's, and these are the exact bytes.
    //
    // GNU's COLORTERM test is `strcasecmp (bg, "truecolor") == 0` -- an EXACT
    // match, case-insensitively.  A substring test would take this arm for
    // `COLORTERM=24bit`, which GNU does not read at all (ledger 193).
    if database.get_flag(FlagCapability::Terminfo("Tc"))
        || colorterm.eq_ignore_ascii_case("truecolor")
    {
        return Some(TtyColorCapabilities::new(
            orig_pair,
            Some(b"\x1b[38;2;%p1%d;%p2%d;%p3%d%;m".to_vec()),
            Some(b"\x1b[48;2;%p1%d;%p2%d;%p3%d%;m".to_vec()),
            true,
            TtyColorDepth::Direct(TtyDirectColorRoute::TcOrColorterm),
            TERMINFO_EXPANDER,
        ));
    }
    Some(TtyColorCapabilities::new(
        orig_pair,
        set_foreground,
        set_background,
        false,
        indexed,
        TERMINFO_EXPANDER,
    ))
}

/// One capability's bytes with terminfo padding removed, or `None` when the
/// entry does not carry it.  The same reading [`rendition_sequence`] does for
/// the appearance capabilities, which is what GNU's `tgetstr` gives it.
fn rendition_capability(
    database: &mut dyn TerminalCapabilityDatabase,
    cap: StringCapability<'_>,
) -> Option<Vec<u8>> {
    database
        .get_string(cap)
        .filter(|value| !value.is_empty())
        .map(|value| rendition_sequence(&value))
        .filter(|value| !value.is_empty())
}

/// One rendition capability's bytes, as GNU emits them.
///
/// `turn_on_face` emits these with `OUTPUT1` / `OUTPUT1_IF`, which is `tputs`:
/// it turns a `$<..>` padding marker into a DELAY rather than into bytes, and
/// it does no parameter expansion at all -- `tparam` is a separate call GNU
/// makes only for `cup`, `setaf`/`setab` and `Smulx`.  So the bytes to keep are
/// the entry's own with padding removed, and a `%` construct (three entries in
/// ncurses' database carry one in a rendition string) is passed through exactly
/// as GNU passes it through.
///
/// This is deliberately NOT [`canonical_cap`], which also strips `%pN`: that
/// normalization exists so the update planner can compare a terminfo spelling
/// against its termcap translation, and it would corrupt a string that is
/// emitted rather than compared.
fn rendition_sequence(entry: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(entry.len());
    let mut i = 0;
    while i < entry.len() {
        if entry[i] == b'$' && entry.get(i + 1) == Some(&b'<') {
            match entry[i + 2..].iter().position(|byte| *byte == b'>') {
                Some(close) => {
                    i += close + 3;
                    continue;
                }
                None => break,
            }
        }
        out.push(entry[i]);
        i += 1;
    }
    out
}

/// Canonicalize a termcap/terminfo capability string for byte comparison:
/// strip padding/delay markers (`$<..>`) and parameter-position markers
/// (`%p1`..`%p9`), so terminfo `\E[%i%p1%d;%p2%dr` and its termcap
/// translation `\E[%i%d;%dr` canonicalize to the same bytes.
fn canonical_cap(entry: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(entry.len());
    let mut i = 0;
    while i < entry.len() {
        if entry[i] == b'$' && entry.get(i + 1) == Some(&b'<') {
            match entry[i + 2..].iter().position(|byte| *byte == b'>') {
                Some(close) => {
                    i += close + 3;
                    continue;
                }
                None => break,
            }
        }
        if entry[i] == b'%'
            && entry.get(i + 1) == Some(&b'p')
            && entry.get(i + 2).is_some_and(u8::is_ascii_digit)
        {
            i += 3;
            continue;
        }
        out.push(entry[i]);
        i += 1;
    }
    out
}

/// Does the entry's termcap `cap` string canonicalize to exactly `expected`?
///
/// Every capability the update planner consults is a cursor-movement or
/// erase-and-scroll capability, and all of those have two-letter termcap names.
/// A capability with only a terminfo name goes through
/// [`StringCapability::Terminfo`] instead.
fn termcap_cap_is(
    database: &mut dyn TerminalCapabilityDatabase,
    cap: &'static str,
    expected: &[u8],
) -> bool {
    database
        .get_string(StringCapability::Termcap(cap))
        .is_some_and(|value| canonical_cap(&value) == expected)
}

/// Resolve the update-planner capabilities ([`TermCaps`]).
///
/// GNU (`term.c:4908`) gates on the PRESENCE of capabilities because it
/// emits the entry's own strings through tparam. neomacs' encoder emits
/// hardcoded ANSI bytes, so presence is not enough: each capability is
/// claimed only when the entry's string IS the byte form the encoder
/// produces (in either its terminfo or termcap spelling). A terminal whose
/// `ic` exists but is not `ESC[@` (tvi955) must refuse ICH, and a terminal
/// whose `cs` attests DECSTBM but that lacks `indn`/`rin` (vt220, the Linux
/// console) must scroll with IND/RI, never CSI S/T. Synchronized output
/// (DECSET 2026) has no terminfo name and is spec-safe to over-claim, so it
/// stays enabled unconditionally.
pub(crate) fn resolve_term_caps(
    database: &mut dyn TerminalCapabilityDatabase,
) -> neomacs_display_runtime::backend::tty::rif::TermCaps {
    use neomacs_display_runtime::backend::tty::rif::{
        BlankTailMethod, RegionScrollMethod, RightMarginBehavior,
    };

    let decstbm = termcap_cap_is(database, "cs", b"\x1b[%i%d;%dr");
    let cursor_address = termcap_cap_is(database, "cm", b"\x1b[%i%d;%dH");
    let su_sd =
        termcap_cap_is(database, "SF", b"\x1b[%dS") && termcap_cap_is(database, "SR", b"\x1b[%dT");
    // GNU defaults TS_fwd_scroll to plain cursor-down (LF) when `sf` is
    // absent (term.c:4820), and requires `sr` for the reverse direction
    // (term.c:4912). IND and RI are what the encoder emits; LF at the
    // bottom margin indexes identically on every DECSTBM terminal.
    let fwd_index = match database.get_string(StringCapability::Termcap("sf")) {
        None => true,
        Some(sf) => matches!(canonical_cap(&sf).as_slice(), b"\n" | b"\x1bD"),
    };
    let rev_index = termcap_cap_is(database, "sr", b"\x1bM");
    let scroll_region = if decstbm && cursor_address {
        if su_sd {
            Some(RegionScrollMethod::SuSd)
        } else if fwd_index && rev_index {
            Some(RegionScrollMethod::Index)
        } else {
            None
        }
    } else {
        None
    };

    neomacs_display_runtime::backend::tty::rif::TermCaps {
        right_margin: if database.get_flag(FlagCapability::Termcap("xn")) {
            RightMarginBehavior::MagicWrap
        } else if database.get_flag(FlagCapability::Termcap("am")) {
            RightMarginBehavior::AutoWrap
        } else {
            RightMarginBehavior::NoAutoWrap
        },
        scroll_region,
        insert_delete_char: termcap_cap_is(database, "IC", b"\x1b[%d@")
            && termcap_cap_is(database, "DC", b"\x1b[%dP"),
        blank_tail: if !database.get_flag(FlagCapability::Termcap("in"))
            && termcap_cap_is(database, "ce", b"\x1b[K")
        {
            BlankTailMethod::EraseToEol {
                back_color_erase: database.get_flag(FlagCapability::Termcap("ut")),
            }
        } else {
            BlankTailMethod::WriteSpaces
        },
        synchronized_output: true,
    }
}

/// [`resolve_term_caps`] for the terminal named by `TERM`; `None` when the
/// terminfo entry cannot be read (the caller then falls back to
/// [`TermCaps::unknown_terminal`]'s conservative floor — over-claiming
/// scroll or shift bytes on an unknown terminal corrupts its screen
/// permanently, while refusing merely costs bytes).
///
/// [`TermCaps::unknown_terminal`]: neomacs_display_runtime::backend::tty::rif::TermCaps::unknown_terminal
pub(crate) fn term_caps_for_term(
    term: &str,
) -> Option<neomacs_display_runtime::backend::tty::rif::TermCaps> {
    let mut database = open_terminal_capability_database(term)?;
    Some(resolve_term_caps(database.as_mut()))
}

/// GNU's "powerful enough" check (term.c:4881): a terminal whose entry can
/// be read but that cannot position the cursor cannot run a full-screen
/// editor. neomacs additionally requires the ANSI form, because every byte
/// the renderer emits hardcodes `CSI r;cH`. `Ok` when TERM is unset or the
/// entry is unreadable (the conservative-caps fallback handles those).
pub(crate) fn check_terminal_powerful_enough(term: &str) -> Result<(), String> {
    let Some(mut database) = open_terminal_capability_database(term) else {
        return Ok(());
    };
    if termcap_cap_is(database.as_mut(), "cm", b"\x1b[%i%d;%dH") {
        return Ok(());
    }
    Err(format!(
        "Terminal type \"{term}\" is not powerful enough to run Emacs.\n\
It lacks the ability to position the cursor (ANSI cursor addressing).\n\
If that is not the actual type of terminal you have,\n\
use the Bourne shell command 'TERM=...; export TERM' (C-shell:\n\
'setenv TERM ...') to specify the correct type."
    ))
}

impl TerminalCapabilityDatabase for neomacs_terminfo::Database {
    fn get_string(&mut self, cap: StringCapability<'_>) -> Option<Vec<u8>> {
        self.string(cap)
            .filter(|value| !value.is_empty())
            .map(<[u8]>::to_vec)
    }

    fn get_termcap_number(&mut self, cap: &str) -> Option<i32> {
        self.termcap_number(cap)
    }

    fn get_flag(&mut self, cap: FlagCapability<'_>) -> bool {
        self.flag(cap)
    }
}

#[cfg(test)]
#[path = "terminal_capabilities_test.rs"]
mod tests;
