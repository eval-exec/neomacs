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
//! This module owns the database handle so both halves ask the same terminfo
//! entry the same way.

#[cfg(not(windows))]
use std::ffi::{CStr, CString};
#[cfg(not(windows))]
use std::os::raw::c_char;
#[cfg(not(windows))]
use std::os::raw::c_int;

use neomacs_display_protocol::tty_capabilities::{
    TerminfoExpander, TerminfoParameters, TtyAttributeCapabilities, TtyColorCapabilities,
    TtyColorDepth, TtyColorSource, TtyDirectColorRoute, TtyNoColorVideo, TtyStyledUnderline,
};

#[cfg(not(windows))]
#[cfg_attr(target_os = "linux", link(name = "ncursesw"))]
#[cfg_attr(target_os = "macos", link(name = "ncurses"))]
unsafe extern "C" {
    fn tgetent(buffer: *mut c_char, term: *const c_char) -> c_int;
    fn tgetstr(capability: *const c_char, area: *mut *mut c_char) -> *mut c_char;
    fn tgetnum(capability: *const c_char) -> c_int;
    fn tgetflag(capability: *const c_char) -> c_int;
    fn tigetstr(capability: *const c_char) -> *mut c_char;
    /// GNU's `tparam`.  In a terminfo build `src/terminfo.c:43-55` defines
    /// `tparam` as a thin wrapper over ncurses' `tparm` with the same four
    /// integer arguments, so calling `tparm` here is calling what GNU calls.
    fn tparm(string: *const c_char, ...) -> *mut c_char;
}

/// GNU `tparam (STRING, NULL, 0, ...)` — the three capabilities `turn_on_face`
/// does not emit verbatim: `Smulx`, `setaf` and `setab`.
///
/// One function for all three because GNU makes one call for all three, and
/// the only thing that varies is the parameter list, which
/// [`TerminfoParameters`] names: `Smulx` and a non-`TF_rgb_separate` colour are
/// GNU's one-argument call (src/term.c:2083, :2103), and a `TF_rgb_separate`
/// colour is GNU's three-argument call (src/term.c:2101).
///
/// `tparm` does not need a terminal to have been set up: it is a function of
/// the string, and measured so (`tmp/pw186/tparm_nosetup.c`).  `None` when the
/// expansion fails, which for ncurses means the string is not a well-formed
/// terminfo format -- GNU's `OUTPUT (tty, p)` would then have been handed a
/// null pointer, so emitting nothing is its behaviour too.
#[cfg(not(windows))]
pub(crate) fn expand_capability_parameter(
    sequence: &[u8],
    parameters: TerminfoParameters,
) -> Option<Vec<u8>> {
    let sequence = CString::new(sequence).ok()?;
    let (first, second, third) = match parameters {
        // A palette subscript can exceed `c_int` only for a value Lisp could
        // not have produced; `try_from` refusing it is the same "not a colour"
        // answer `TerminalColor::from_tty_color_desc` gives.
        TerminfoParameters::One(value) => (c_int::try_from(value).ok()?, 0, 0),
        TerminfoParameters::Rgb { r, g, b } => (c_int::from(r), c_int::from(g), c_int::from(b)),
    };
    let expanded = unsafe {
        tparm(
            sequence.as_ptr(),
            first,
            second,
            third,
            0 as c_int,
            0 as c_int,
            0 as c_int,
            0 as c_int,
            0 as c_int,
        )
    };
    if expanded.is_null() {
        return None;
    }
    let bytes = unsafe { CStr::from_ptr(expanded) }.to_bytes().to_vec();
    (!bytes.is_empty()).then_some(bytes)
}

#[cfg(windows)]
pub(crate) fn expand_capability_parameter(
    _sequence: &[u8],
    _parameters: TerminfoParameters,
) -> Option<Vec<u8>> {
    None
}

/// The expander the capability record carries, so that a `setaf` string and the
/// thing that expands it are never separated.  See
/// [`neomacs_display_protocol::tty_capabilities::TerminfoExpander`].
pub(crate) const TERMINFO_EXPANDER: TerminfoExpander =
    TerminfoExpander::new(expand_capability_parameter);

/// Which of the two capability namespaces a string capability name lives in.
///
/// They do not overlap, and a name from one is invisible to the other.
/// `tgetstr` resolves TERMCAP names, which are two letters and nothing else;
/// `tigetstr` resolves TERMINFO names, which are the long ones.  `us` exists
/// only for `tgetstr` (its terminfo spelling is `smul`) and `Smulx` exists only
/// for `tigetstr` (it has no termcap spelling at all).
///
/// GNU splits its lookups the same way: `init_tty` reads `so`, `us`, `md`, `mh`
/// and `ZH` with `tgetstr`, and reads the two long names with `tigetstr` --
/// `smxx` at src/term.c:4587 and `Smulx` at :4694, each guarded by
/// `#ifdef TERMINFO` with a `tgetstr` fallback for a build that has no terminfo
/// at all.  GNU's own comment on that fallback doubts it:
///
/// ```text
///   /* FIXME: Is calling tgetstr here for non-terminfo case correct,
///      even though "smxx" is more than 2 characters?  */
/// ```
///
/// (src/term.c:4591-4592.)  The doubt is right.  Measured against ncurses on
/// tmux-256color, whose `infocmp -x` carries both long names:
///
/// ```text
///   Smulx    tgetstr=null   tigetstr=FOUND
///   smxx     tgetstr=null   tigetstr=FOUND
///   us       tgetstr=FOUND  tigetstr=null
/// ```
///
/// A `&str` capability name cannot carry that distinction, so a terminfo name
/// asked of `tgetstr` answers "this terminal has no such capability" rather
/// than failing, and the attribute is then silently never emitted for the rest
/// of the program's life.  Naming the namespace in the type is what stops the
/// next capability from being added to the wrong one.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StringCapability<'a> {
    /// A two-letter termcap name, read with `tgetstr`.
    Termcap(&'a str),
    /// A terminfo capability name, read with `tigetstr`.
    Terminfo(&'a str),
}

/// A source of terminal capabilities — terminfo in production, a table in tests.
pub(crate) trait TerminalCapabilityDatabase {
    /// A string capability, read from the namespace its name belongs to (GNU
    /// `tgetstr` or `tigetstr`).  `None` when the entry lacks it.
    fn get_string(&mut self, cap: StringCapability<'_>) -> Option<Vec<u8>>;

    /// A numeric capability (GNU `tgetnum`). `None`, like GNU's `-1`, when the
    /// entry lacks it.  Every number GNU reads -- `Co`, `NC` -- has a
    /// two-letter termcap name, so there is no terminfo variant here.
    fn get_termcap_number(&mut self, cap: &str) -> Option<i32>;

    /// A boolean capability (GNU `tgetflag`). Termcap two-letter names, e.g.
    /// `ut` for back-color-erase (terminfo `bce`).
    fn get_termcap_flag(&mut self, cap: &str) -> bool;
}

pub(crate) fn open_terminal_capability_database(
    term: &str,
) -> Option<Box<dyn TerminalCapabilityDatabase>> {
    open_platform_terminal_capability_database(term)
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
                .get_termcap_flag("Su")
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
                expand_capability_parameter(&smulx, TerminfoParameters::One(u32::from(style)))
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
    if database.get_termcap_flag("RGB") {
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
    if database.get_termcap_flag("Tc") || colorterm.eq_ignore_ascii_case("truecolor") {
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
    use neomacs_display_runtime::backend::tty::rif::{BlankTailMethod, RegionScrollMethod};

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
        scroll_region,
        insert_delete_char: termcap_cap_is(database, "IC", b"\x1b[%d@")
            && termcap_cap_is(database, "DC", b"\x1b[%dP"),
        blank_tail: if !database.get_termcap_flag("in") && termcap_cap_is(database, "ce", b"\x1b[K")
        {
            BlankTailMethod::EraseToEol {
                back_color_erase: database.get_termcap_flag("ut"),
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

#[cfg(not(windows))]
struct UnixTermcapDatabase {
    _termcap_buffer: Vec<c_char>,
    _string_area: Vec<c_char>,
    string_area_ptr: *mut c_char,
}

#[cfg(not(windows))]
impl UnixTermcapDatabase {
    fn open(term: &str) -> Option<Self> {
        let term = CString::new(term).ok()?;
        let mut termcap_buffer = vec![0 as c_char; 16384];
        let ok = unsafe { tgetent(termcap_buffer.as_mut_ptr(), term.as_ptr()) };
        if ok <= 0 {
            return None;
        }
        let mut string_area = vec![0 as c_char; 32768];
        let string_area_ptr = string_area.as_mut_ptr();
        Some(Self {
            _termcap_buffer: termcap_buffer,
            _string_area: string_area,
            string_area_ptr,
        })
    }
}

#[cfg(not(windows))]
impl TerminalCapabilityDatabase for UnixTermcapDatabase {
    fn get_string(&mut self, cap: StringCapability<'_>) -> Option<Vec<u8>> {
        let raw = match cap {
            StringCapability::Termcap(name) => {
                let name = CString::new(name).ok()?;
                unsafe { tgetstr(name.as_ptr(), &mut self.string_area_ptr) }
            }
            // `tgetent` is what sets ncurses' `cur_term`, so `tigetstr` reads
            // the entry this database already opened; no second `setupterm` is
            // needed.  `tigetstr` reports a name that is not a string
            // capability as (char *) -1 rather than NULL, and that is not a
            // capability this terminal has either.
            StringCapability::Terminfo(name) => {
                let name = CString::new(name).ok()?;
                let raw = unsafe { tigetstr(name.as_ptr()) };
                if raw as isize == -1 {
                    return None;
                }
                raw
            }
        };
        if raw.is_null() {
            return None;
        }
        let bytes = unsafe { CStr::from_ptr(raw) }.to_bytes().to_vec();
        (!bytes.is_empty()).then_some(bytes)
    }

    fn get_termcap_number(&mut self, cap: &str) -> Option<i32> {
        let cap = CString::new(cap).ok()?;
        let value = unsafe { tgetnum(cap.as_ptr()) };
        (value != -1).then_some(value)
    }

    fn get_termcap_flag(&mut self, cap: &str) -> bool {
        let Ok(cap) = CString::new(cap) else {
            return false;
        };
        unsafe { tgetflag(cap.as_ptr()) != 0 }
    }
}

#[cfg(not(windows))]
fn open_platform_terminal_capability_database(
    term: &str,
) -> Option<Box<dyn TerminalCapabilityDatabase>> {
    UnixTermcapDatabase::open(term)
        .map(|database| Box::new(database) as Box<dyn TerminalCapabilityDatabase>)
}

#[cfg(windows)]
fn open_platform_terminal_capability_database(
    _term: &str,
) -> Option<Box<dyn TerminalCapabilityDatabase>> {
    // GNU Emacs' native Windows console backend does not link termcap.
    // nt/inc/ms-w32.h redirects tgetstr to sys_tgetstr, and
    // src/w32console.c implements that hook as a NULL capability lookup.
    None
}

#[cfg(test)]
#[path = "terminal_capabilities_test.rs"]
mod tests;
