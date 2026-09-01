//! Emacs coding system support.
//!
//! Since Rust natively uses UTF-8, this module is primarily for API
//! compatibility. The coding system infrastructure tracks registered
//! systems and their aliases but all actual encoding/decoding passes
//! through as UTF-8 identity operations.
//!
//! Contains:
//! - CodingSystemManager: registry of coding systems, aliases, priority list
//! - CodingSystemInfo: per-system metadata (name, type, mnemonic, EOL)
//! - Pure builtins: coding-system-list, coding-system-aliases, coding-system-get,
//!   coding-system-put, coding-system-base, coding-system-eol-type,
//!   coding-system-type, coding-system-change-eol-conversion,
//!   coding-system-change-text-conversion,
//!   detect-coding-string, detect-coding-region, keyboard-coding-system,
//!   terminal-coding-system, set-keyboard-coding-system,
//!   set-terminal-coding-system, coding-system-priority-list

use super::error::{EvalResult, Flow, signal};
use super::eval::Context;
use super::intern::{SymId, intern, lookup_interned, resolve_sym};
use super::symbol::Obarray;
use super::value::*;
use crate::buffer::{BufferManager, EmacsByteRange, LispCharPos1};
use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::{expect_args, expect_max_args, expect_min_args};
use num_enum::{IntoPrimitive, TryFromPrimitive};
use std::collections::{HashMap, HashSet};
use strum::{EnumString, IntoStaticStr};

// ---------------------------------------------------------------------------
// Argument helpers (local to this module)
// ---------------------------------------------------------------------------

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn expect_integer_or_marker(val: &Value) -> Result<(), Flow> {
    if val.is_marker() {
        return Ok(());
    }
    match val.kind() {
        ValueKind::Fixnum(_) => Ok(()),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integer-or-marker-p"), *val],
        )),
    }
}

fn is_known_or_derived_coding_system(mgr: &CodingSystemManager, name: &str) -> bool {
    resolve_runtime_name(mgr, name).is_some()
}

fn normalize_keyboard_coding_system(name: &str) -> String {
    if let Some(eol) = EolType::from_suffix(name) {
        let base = strip_eol_suffix(name);
        return match eol {
            EolType::Unix => match base {
                "binary" | "no-conversion" => base.to_string(),
                _ => format!("{base}-unix"),
            },
            EolType::Dos | EolType::Mac => normalize_keyboard_coding_system(base),
            EolType::Undecided => unreachable!("suffix-based eol cannot be undecided"),
        };
    }
    match name {
        "binary" | "no-conversion" => name.to_string(),
        "emacs-internal" => "emacs-internal".to_string(),
        "ascii" | "us-ascii" => "us-ascii-unix".to_string(),
        "latin-1" | "iso-8859-1" | "iso-latin-1" => "iso-latin-1-unix".to_string(),
        "latin-5" | "iso-8859-9" | "iso-latin-5" => "iso-latin-5-unix".to_string(),
        "latin-0" | "latin-9" | "iso-8859-15" | "iso-latin-9" => "iso-latin-9-unix".to_string(),
        _ => format!("{name}-unix"),
    }
}

/// Extract a coding system name from a symbol or string argument.
#[cfg(test)]
fn coding_system_name(val: &Value) -> Result<String, Flow> {
    match val.kind() {
        ValueKind::Symbol(id) => Ok(resolve_sym(id).to_owned()),
        ValueKind::String => coding_runtime_string(val),
        ValueKind::Nil => Ok("nil".to_string()),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), *val],
        )),
    }
}

/// Extract a coding system name from a symbol-like argument.
/// Accepts symbols, keywords, nil, and t.
fn coding_symbol_name(val: &Value) -> Result<String, Flow> {
    match val.as_symbol_name() {
        Some(name) => Ok(name.to_string()),
        None => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), *val],
        )),
    }
}

fn coding_runtime_string(value: &Value) -> Result<String, Flow> {
    value
        .as_lisp_string()
        .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
        .ok_or_else(|| {
            signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("stringp"), *value],
            )
        })
}

// ---------------------------------------------------------------------------
// EOL types
// ---------------------------------------------------------------------------

/// End-of-line conversion types matching Emacs conventions.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr, IntoPrimitive, TryFromPrimitive,
)]
#[repr(i8)]
#[strum(serialize_all = "kebab-case")]
pub enum EolType {
    /// LF (Unix) -- value 0
    Unix = 0,
    /// CRLF (DOS/Windows) -- value 1
    Dos = 1,
    /// CR (Classic Mac) -- value 2
    Mac = 2,
    /// Undecided / detect automatically
    Undecided = -1,
}

impl EolType {
    pub fn from_specified_symbol_name(name: &str) -> Option<Self> {
        match name.parse().ok()? {
            EolType::Unix => Some(EolType::Unix),
            EolType::Dos => Some(EolType::Dos),
            EolType::Mac => Some(EolType::Mac),
            EolType::Undecided => None,
        }
    }

    pub fn from_specified_symbol_value(value: &Value) -> Option<Self> {
        Self::from_specified_symbol_name(value.as_symbol_name()?)
    }

    pub fn name(self) -> &'static str {
        self.into()
    }

    pub fn to_int(self) -> i64 {
        self.specified_index().unwrap_or(0)
    }

    pub fn to_symbol(self) -> Value {
        Value::symbol(self.name())
    }

    pub fn specified_index(self) -> Option<i64> {
        match self {
            EolType::Undecided => None,
            specified => Some(i8::from(specified) as i64),
        }
    }

    pub fn suffix(self) -> &'static str {
        match self {
            EolType::Unix => "-unix",
            EolType::Dos => "-dos",
            EolType::Mac => "-mac",
            EolType::Undecided => "",
        }
    }

    pub fn from_suffix(name: &str) -> Option<EolType> {
        if name.ends_with("-unix") {
            Some(EolType::Unix)
        } else if name.ends_with("-dos") {
            Some(EolType::Dos)
        } else if name.ends_with("-mac") {
            Some(EolType::Mac)
        } else {
            None
        }
    }

    /// The eol type an ENCODE converts with.
    ///
    /// GNU spends the decision before any encoder runs, and it spends an
    /// undecided one as `unix`: `consume_chars` (src/coding.c:7623-7625) and
    /// `encode_coding_iso_2022` (src/coding.c:4384-4386) both open with
    ///
    /// ```c
    ///   eol_type = inhibit_eol_conversion ? Qunix : CODING_ID_EOL_TYPE (coding->id);
    ///   if (VECTORP (eol_type))
    ///     eol_type = Qunix;
    /// ```
    ///
    /// so encoding never detects.  Only decoding does.  The first term of that
    /// expression is [`EolConversion`], which is why it is a parameter and not
    /// a lookup: see that type for why it cannot be a global here.
    pub(crate) fn for_encode(self, eol_conversion: EolConversion) -> ResolvedEol {
        match eol_conversion {
            EolConversion::Inhibited => ResolvedEol::Unix,
            EolConversion::Enabled => match self {
                EolType::Dos => ResolvedEol::Dos,
                EolType::Mac => ResolvedEol::Mac,
                EolType::Unix | EolType::Undecided => ResolvedEol::Unix,
            },
        }
    }

    /// The eol type a DECODE converts with, given the text the decoder produced.
    ///
    /// GNU's `decode_eol` (src/coding.c:6760) never converts with a vector: its
    /// first act is to resolve one by scanning the decoded text
    /// (src/coding.c:6783-6806) and calling `adjust_coding_eol_type`.  An
    /// undecided eol type therefore means DETECT, not "leave alone" -- the
    /// distinction this whole type exists to keep visible.
    pub(crate) fn for_decode(self, decoded: &[u8], eol_conversion: EolConversion) -> ResolvedEol {
        self.resolve_for_decode(decoded, eol_conversion).eol()
    }

    /// The same resolution, keeping GNU's OTHER half of it.
    ///
    /// `adjust_coding_eol_type` (src/coding.c:6471-6497) does two things in one
    /// call: it picks the eol type `decode_eol` converts with, and it REPLACES
    /// `coding->id` with the subsidiary that carries it.  Only the VECTOR case
    /// does the second -- `decode_eol` guards the call with
    /// `if (eol_seen != EOL_SEEN_NONE)` (src/coding.c:6805) -- and every
    /// reporting slot in Emacs is downstream of that rewrite:
    /// `last-coding-system-used` (src/coding.c:9497),
    /// `(process-coding-system P)` (src/process.c:6421-6425) and, through it,
    /// what the NEXT run of the same process decodes with.
    ///
    /// [`for_decode`](Self::for_decode) throws the second half away, which is
    /// safe only where nothing reports.  Anything that reports must take this
    /// one, because a `ResolvedEol` on its own cannot say whether the name
    /// moved with it.
    pub(crate) fn resolve_for_decode(
        self,
        decoded: &[u8],
        eol_conversion: EolConversion,
    ) -> DecodeEolResolution {
        // `decode_eol`'s first line, before the VECTOR branch and before the
        // conversion (src/coding.c:6767):
        //
        //   if (EQ (eol_type, Qunix) || inhibit_eol_conversion)
        //     return;
        //
        // so the flag suppresses `adjust_coding_eol_type` -- the NAME -- as
        // well as the byte transformation.  Measured under GNU 31.0.90:
        // `(let ((inhibit-eol-conversion t)) (decode-coding-string "a\r\n" 'utf-8))`
        // leaves `last-coding-system-used' at `utf-8', not `utf-8-dos'.
        if eol_conversion == EolConversion::Inhibited {
            return DecodeEolResolution::Inhibited;
        }
        match self {
            EolType::Unix => DecodeEolResolution::Specified(ResolvedEol::Unix),
            EolType::Dos => DecodeEolResolution::Specified(ResolvedEol::Dos),
            EolType::Mac => DecodeEolResolution::Specified(ResolvedEol::Mac),
            EolType::Undecided => match detected_decoded_eol(decoded) {
                Some(resolved) => DecodeEolResolution::Adjusted(resolved),
                None => DecodeEolResolution::NotSeen,
            },
        }
    }
}

/// GNU's `inhibit-eol-conversion` (`DEFVAR_BOOL`, src/coding.c:12022), carried
/// as a VALUE.
///
/// GNU reads a process-wide C global at every end-of-line decision -- eight
/// decoders' `eol_dos` (src/coding.c:1250-1251 and seven copies),
/// `consume_chars` (:7625), `decode_eol` (:6767), `decode_coding` (:7481),
/// `setup_coding_system` (:5681), `check_ascii` (:6181), `detect_coding`
/// (:6569) and `code_convert_string`'s identity fast path (:9619).  A global is
/// not available here and must not be faked: the obarray belongs to a
/// `Context`, `LispBoolFwd` cells are copied per thread
/// (`crates/neovm-core/src/emacs_core/runtime/forward/mod.rs`), and the unit suite runs many
/// `Context`s on parallel threads, so a static would be read by the wrong
/// session.
///
/// It is therefore a REQUIRED parameter of the two functions that resolve an
/// [`EolType`] -- [`EolType::for_encode`] and [`EolType::resolve_for_decode`]
/// -- which entry 139 made the only two ways to get a [`ResolvedEol`].  No
/// conversion in the editor can resolve an end-of-line type without having
/// been told what this variable holds, because there is no spelling that
/// omits it.
///
/// It is read at CONVERSION time, never at resolution time, because that is
/// where GNU reads it.  Measured under GNU 31.0.90: a subprocess created inside
/// `(let ((inhibit-eol-conversion t)) ...)` and read outside it CONVERTS, and
/// one created outside and read inside does NOT -- the binding that counts is
/// the one live when the bytes arrive, not the one live when the coding system
/// was chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum EolConversion {
    /// `inhibit-eol-conversion` is nil.
    Enabled,
    /// `inhibit-eol-conversion` is non-nil.
    Inhibited,
}

impl EolConversion {
    /// The variable's Lisp value, read the way GNU's C code sees a
    /// `DEFVAR_BOOL`: any non-nil value inhibits.
    pub(crate) fn from_lisp(value: Value) -> Self {
        if value.is_nil() {
            EolConversion::Enabled
        } else {
            EolConversion::Inhibited
        }
    }
}

/// What GNU's `decode_eol` (src/coding.c:6760-6806) decided, in one value:
/// which end-of-line type the conversion runs with, AND whether the coding
/// system's NAME moved with it.
///
/// The two are one decision in GNU because they are one call --
/// `adjust_coding_eol_type` returns the eol type and rewrites `coding->id` on
/// the way past.  Splitting them is how a decoder ends up converting CR LF
/// while still reporting the undecided name it started from, which is entry
/// 131's `Vlast_coding_system_used` residual and entry 134's per-chunk
/// detection, both fixed by returning this instead of a bare [`ResolvedEol`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DecodeEolResolution {
    /// The coding system's eol type was already concrete, so
    /// `adjust_coding_eol_type` never ran and the name does not move.
    Specified(ResolvedEol),
    /// The eol type was a VECTOR and the scan saw a terminator: GNU replaces
    /// `coding->id` with the subsidiary carrying this type.
    Adjusted(ResolvedEol),
    /// The eol type was a VECTOR and the decoded text held no terminator at all
    /// (GNU `EOL_SEEN_NONE`).  Nothing converts, and -- because
    /// `adjust_coding_eol_type` is skipped -- the name stays undecided.
    NotSeen,
    /// `inhibit-eol-conversion` was non-nil, so `decode_eol` returned on its
    /// first line (src/coding.c:6767) -- before the VECTOR branch and before
    /// the conversion.  Nothing converts and the name does not move, whatever
    /// the coding system's own eol type was.
    ///
    /// This is a state of its own rather than [`Self::NotSeen`] because the two
    /// have different causes and only one of them is a property of the text:
    /// `NotSeen` says the decoded bytes held no terminator, `Inhibited` says
    /// nobody was allowed to look.
    Inhibited,
}

impl DecodeEolResolution {
    /// The eol type the conversion runs with.  `NotSeen` and `Inhibited`
    /// convert nothing, which is `Qunix` -- both are ways for `decode_eol` to
    /// finish without touching a byte (src/coding.c:6765 for the second,
    /// :6805's `eol_seen != EOL_SEEN_NONE` guard for the first).
    pub(crate) fn eol(self) -> ResolvedEol {
        match self {
            DecodeEolResolution::Specified(eol) | DecodeEolResolution::Adjusted(eol) => eol,
            DecodeEolResolution::NotSeen | DecodeEolResolution::Inhibited => ResolvedEol::Unix,
        }
    }

    /// The subsidiary GNU rewrote `coding->id` to, when it rewrote one.
    pub(crate) fn adjusted(self) -> Option<ResolvedEol> {
        match self {
            DecodeEolResolution::Adjusted(eol) => Some(eol),
            DecodeEolResolution::Specified(_)
            | DecodeEolResolution::NotSeen
            | DecodeEolResolution::Inhibited => None,
        }
    }
}

/// The end-of-line type a conversion actually runs with.
///
/// GNU's `eol_type` slot may hold a VECTOR of the three subsidiaries, which is
/// `EolType::Undecided` here; every conversion site in `coding.c` resolves that
/// vector before it converts a single byte -- encoders by forcing `Qunix`
/// (src/coding.c:7623), the decoder by detecting (src/coding.c:6785).  This type
/// is what remains after the resolution, so an unresolved eol type cannot reach
/// a byte transformation: there is no `Undecided` variant to pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ResolvedEol {
    /// `Qunix`: `decode_eol` returns immediately (src/coding.c:6765).
    Unix,
    /// `Qdos`: CR LF collapses to LF (src/coding.c:6815-6847).
    Dos,
    /// `Qmac`: every CR becomes LF (src/coding.c:6809-6813).
    Mac,
}

impl ResolvedEol {
    /// The name suffix of the subsidiary `adjust_coding_eol_type` selects
    /// (src/coding.c:6480-6493, `AREF (eol_type, 0/1/2)`).
    pub(crate) fn suffix(self) -> &'static str {
        match self {
            ResolvedEol::Unix => "-unix",
            ResolvedEol::Dos => "-dos",
            ResolvedEol::Mac => "-mac",
        }
    }
}

/// GNU's `EOL_SEEN_*` bitmask (src/coding.c:1099-1102) reduced to the four
/// outcomes `decode_eol` can end its scan with.
///
/// `decode_eol` ORs a flag per line terminator over the WHOLE decoded text and
/// then folds the mask down: CR LF together with a stray CR is a DOS text
/// (src/coding.c:6798-6801), and any other mixture is unix
/// (src/coding.c:6800-6804).  Representing the fold's *result* rather than the
/// raw mask is what stops a caller from converting with a mixture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DecodeEolSeen {
    Lf,
    Crlf,
    Cr,
}

impl DecodeEolSeen {
    /// GNU `adjust_coding_eol_type` (src/coding.c:6471-6497): LF wins over
    /// CR LF wins over CR.  The fold in `detect_decoded_eol` has already made
    /// the outcome a single flag, so the priority is a total function here.
    fn adjust(self) -> ResolvedEol {
        match self {
            DecodeEolSeen::Lf => ResolvedEol::Unix,
            DecodeEolSeen::Crlf => ResolvedEol::Dos,
            DecodeEolSeen::Cr => ResolvedEol::Mac,
        }
    }
}

/// Port of the VECTOR branch of GNU `decode_eol` (src/coding.c:6783-6806).
///
/// Note what this is NOT: `detect_eol` (src/coding.c:6373) stops after
/// `MAX_EOL_CHECK_COUNT` (3) terminators and resolves a disagreement the moment
/// it meets one.  That function serves `Fdetect_coding_region`
/// (src/coding.c:8944) and answers a different question -- measured under GNU
/// 31.0.90, `(detect-coding-string "a\r\nb\r\nc\r\nd\r\ne\nf")` is
/// `(undecided-dos)` while `(decode-coding-string "a\r\nb\r\nc\r\nd\r\ne\nf"
/// 'undecided)` leaves every CR in place, because the decoder saw the fifth,
/// bare LF and called the text mixed.  Returns `None` for GNU's
/// `EOL_SEEN_NONE`, which leaves the coding system's eol type undecided -- the
/// case where `decode_eol` skips `adjust_coding_eol_type` altogether
/// (src/coding.c:6805) and the coding system's reported NAME keeps no suffix.
pub(crate) fn detected_decoded_eol(bytes: &[u8]) -> Option<ResolvedEol> {
    detect_decoded_eol_seen(bytes).map(DecodeEolSeen::adjust)
}

fn detect_decoded_eol_seen(bytes: &[u8]) -> Option<DecodeEolSeen> {
    const SEEN_LF: u8 = 1;
    const SEEN_CR: u8 = 2;
    const SEEN_CRLF: u8 = 4;
    let mut eol_seen = 0u8;
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'\n' => eol_seen |= SEEN_LF,
            b'\r' => {
                if bytes.get(i + 1) == Some(&b'\n') {
                    eol_seen |= SEEN_CRLF;
                    i += 1;
                } else {
                    eol_seen |= SEEN_CR;
                }
            }
            _ => {}
        }
        i += 1;
    }
    // "Handle DOS-style EOLs in a file with stray ^M characters."
    if eol_seen & SEEN_CRLF != 0 && eol_seen & SEEN_CR != 0 && eol_seen & SEEN_LF == 0 {
        return Some(DecodeEolSeen::Crlf);
    }
    match eol_seen {
        0 => None,
        SEEN_LF => Some(DecodeEolSeen::Lf),
        SEEN_CRLF => Some(DecodeEolSeen::Crlf),
        SEEN_CR => Some(DecodeEolSeen::Cr),
        // Any other mixture: GNU falls back to EOL_SEEN_LF.
        _ => Some(DecodeEolSeen::Lf),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
pub(crate) enum TextQuotingStyle {
    Grave,
    Straight,
    Curve,
}

impl TextQuotingStyle {
    pub(crate) fn from_symbol_value(value: Value) -> Option<Self> {
        value.as_symbol_name()?.parse().ok()
    }

    pub(crate) fn symbol_name(self) -> &'static str {
        self.into()
    }

    pub(crate) fn to_symbol(self) -> Value {
        Value::symbol(self.symbol_name())
    }
}

/// Resolve the effective text-quoting style from `text-quoting-style`,
/// mirroring GNU `Ftext_quoting_style` (`src/doc.c:652-678`): `grave',
/// `straight', and `curve' are returned verbatim; nil (and any other value)
/// resolve to `curve' (the display-capability fallback always picks `curve'
/// in batch/UTF-8, matching `builtin_text_quoting_style').
pub(crate) fn effective_text_quoting_style(obarray: &Obarray) -> TextQuotingStyle {
    let var = obarray
        .symbol_value("text-quoting-style")
        .copied()
        .unwrap_or(Value::NIL);
    if var.is_nil() {
        return TextQuotingStyle::Curve;
    }
    TextQuotingStyle::from_symbol_value(var).unwrap_or(TextQuotingStyle::Curve)
}

/// Requote the grave accent (`` ` ``) and apostrophe (`'`) in a C-level error
/// message according to STYLE, mirroring GNU `doprnt` (`src/doprnt.c:490-505`),
/// which every `error()`/`verror()` message passes through:
///   - `curve'    : `` ` `` -> ‘ (U+2018), `'` -> ’ (U+2019)
///   - `straight' : `` ` `` -> `'`, `'` unchanged
///   - `grave'    : unchanged
pub(crate) fn requote_c_error_message(msg: &str, style: TextQuotingStyle) -> String {
    match style {
        TextQuotingStyle::Grave => msg.to_string(),
        TextQuotingStyle::Straight => msg.replace('`', "'"),
        TextQuotingStyle::Curve => {
            let mut out = String::with_capacity(msg.len());
            for ch in msg.chars() {
                match ch {
                    '`' => out.push('\u{2018}'),
                    '\'' => out.push('\u{2019}'),
                    other => out.push(other),
                }
            }
            out
        }
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn non_nil_symbol_id(value: &Value) -> Option<SymId> {
    if value.is_nil() {
        None
    } else {
        value.as_symbol_id()
    }
}

/// Validate a coding-system hook argument exactly as GNU `CHECK_SYMBOL` does for
/// `:post-read-conversion` / `:pre-write-conversion`: `nil` and any symbol are
/// accepted (`nil` yields `None`, i.e. "no hook"); any other value — most often
/// a `lambda` — signals `wrong-type-argument (symbolp VALUE)`.  GNU performs
/// this check unconditionally in both `Fdefine_coding_system_internal`
/// (src/coding.c:11083, 11087) and `Fcoding_system_put` (11562, 11567).  neomacs
/// previously coerced a non-symbol silently to `None` via `non_nil_symbol_id`,
/// so e.g. `(define-coding-system … :post-read-conversion (lambda …))` defined
/// without error and then ignored the hook, whereas GNU rejects it at
/// definition time.
fn check_symbol_hook_arg(value: &Value) -> Result<Option<SymId>, Flow> {
    if value.is_nil() {
        Ok(None)
    } else if let Some(id) = value.as_symbol_id() {
        Ok(Some(id))
    } else {
        Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), *value],
        ))
    }
}

// ---------------------------------------------------------------------------
// CodingSystemInfo
// ---------------------------------------------------------------------------

/// Information about a single coding system.
/// Reserved `int_properties` keys carrying a coding system's ISO-2022
/// designation/flags data. `int_properties` is pdump-serialized but not
/// surfaced by `coding-system-plist`, and these out-of-range keys cannot
/// collide with any integer a `coding-system-put` would use.
const ISO2022_KEY_INITIAL: i64 = i64::MIN;
const ISO2022_KEY_REQUEST: i64 = i64::MIN + 1;
const ISO2022_KEY_FLAGS: i64 = i64::MIN + 2;
const ISO2022_KEY_REG_USAGE: i64 = i64::MIN + 3;
/// Reserved int_properties key holding the verbatim define-coding-system plist
/// (arg 11), used to reproduce GNU's stored plist order in coding-system-plist.
const PLIST_VERBATIM_KEY: i64 = i64::MIN + 4;
/// Reserved int_properties key holding the ordered plist of properties set via
/// `coding-system-put`.  GNU stores a coding system's plist directly on its
/// shared spec (`CODING_ATTR_PLIST`, src/coding.c) and `coding-system-put` does
/// `plist_put` on it; `coding-system-get`/`coding-system-plist` read it back.
/// neomacs reconstructs the bulk of that plist from typed fields, so the
/// put-time overrides are kept here as a live Lisp plist (preserving GNU's
/// `plist_put` order: in-place update for an existing key, append for a new
/// one) and folded onto every reconstructed `coding-system-plist`.
const PUT_OVERRIDES_KEY: i64 = i64::MIN + 5;
const CCL_KEY_DECODER: i64 = i64::MIN + 6;
const CCL_KEY_ENCODER: i64 = i64::MIN + 7;
const CCL_KEY_VALIDS: i64 = i64::MIN + 8;

/// The compiled-program designators and byte-validity table carried by a CCL
/// coding system. These are the three type-specific attributes GNU's
/// `define-coding-system` passes after the common coding attributes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CclCodingSpec {
    pub(crate) decoder: Value,
    pub(crate) encoder: Value,
    pub(crate) valids: Value,
}

pub(crate) fn ccl_coding_spec(info: &CodingSystemInfo) -> Option<CclCodingSpec> {
    Some(CclCodingSpec {
        decoder: *info.int_properties.get(&CCL_KEY_DECODER)?,
        encoder: *info.int_properties.get(&CCL_KEY_ENCODER)?,
        valids: *info.int_properties.get(&CCL_KEY_VALIDS)?,
    })
}

/// ISO-2022 control flags, one variant per bit. Bit values match
/// `coding-system-iso-2022-flags` (mule.el) and `CODING_ISO_FLAG_*` (coding.c).
/// Modelled as a real enum (via enumflags2) so individual flags can be matched
/// and a flag set is a `BitFlags<IsoFlag>`.
#[enumflags2::bitflags]
#[repr(u32)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IsoFlag {
    LongForm = 0x0001,
    AsciiAtEol = 0x0002,
    AsciiAtCntl = 0x0004,
    SevenBits = 0x0008,
    LockingShift = 0x0010,
    SingleShift = 0x0020,
    Designation = 0x0040,
    Revision = 0x0080,
    InitAtBol = 0x0200,
    DesignateAtBol = 0x0400,
    Composition = 0x2000,
    UseRoman = 0x8000,
    UseOldjis = 0x10000,
}

/// The ISO-2022 designation state of a coding system: the charset initially
/// loaded into each graphic register G0-G3 (`initial`), the charset->register
/// map consulted while encoding (`request`), and the control `flags`.
pub struct Iso2022Spec {
    pub initial: [Option<SymId>; 4],
    pub request: Vec<(SymId, u8)>,
    /// `(reg94, reg96)` — the graphic register that 94- and 96-character sets
    /// are designated to while encoding (GNU `coding_attr_iso_usage`). A value
    /// of 4 means "no fixed register"; such charsets fall back to G0.
    pub reg_usage: (u8, u8),
    pub flags: enumflags2::BitFlags<IsoFlag>,
}

impl Iso2022Spec {
    /// The graphic register (0-3) a charset is designated to while encoding.
    pub fn register_of(&self, charset: SymId) -> Option<u8> {
        self.request
            .iter()
            .find(|(cs, _)| *cs == charset)
            .map(|(_, reg)| *reg)
    }

    /// The graphic register a charset of the given set size is encoded into,
    /// following GNU's reg-usage rule (`setup_iso_safe_charsets`): a 96-set
    /// uses `reg96`, any other set uses `reg94`; a register of 4 ("none")
    /// falls back to G0.
    pub fn encode_register(&self, chars_96: bool) -> usize {
        let reg = if chars_96 {
            self.reg_usage.1
        } else {
            self.reg_usage.0
        };
        if reg < 4 { usize::from(reg) } else { 0 }
    }
}

/// Parse a coding system's stored ISO-2022 designation/flags data, if present.
pub(crate) fn iso2022_spec(info: &CodingSystemInfo) -> Option<Iso2022Spec> {
    let initial_val = info.int_properties.get(&ISO2022_KEY_INITIAL)?;
    let request_val = info.int_properties.get(&ISO2022_KEY_REQUEST)?;
    let flags_bits = info.int_properties.get(&ISO2022_KEY_FLAGS)?.as_int()?;

    let mut initial = [None; 4];
    if let Some(vec) = initial_val.as_vector_data() {
        for (slot, elem) in initial.iter_mut().zip(vec.iter()) {
            *slot = if elem.is_nil() {
                None
            } else {
                elem.as_symbol_id()
            };
        }
    }

    let mut request = Vec::new();
    if let Some(pairs) = super::value::list_to_vec(request_val) {
        for pair in pairs {
            if let (Some(cs), Some(reg)) =
                (pair.cons_car().as_symbol_id(), pair.cons_cdr().as_int())
                && (0..4).contains(&reg)
            {
                request.push((cs, reg as u8));
            }
        }
    }

    let reg_usage = info
        .int_properties
        .get(&ISO2022_KEY_REG_USAGE)
        .map(|v| {
            let car = v.cons_car().as_int().unwrap_or(4);
            let cdr = v.cons_cdr().as_int().unwrap_or(4);
            (car.clamp(0, 255) as u8, cdr.clamp(0, 255) as u8)
        })
        .unwrap_or((4, 4));

    Some(Iso2022Spec {
        initial,
        request,
        reg_usage,
        flags: enumflags2::BitFlags::from_bits_truncate(flags_bits as u32),
    })
}

#[derive(Clone, Debug)]
pub struct CodingSystemInfo {
    /// Canonical name of the coding system (e.g. "utf-8").
    pub name: SymId,
    /// Type category (e.g. "utf-8", "charset", "raw-text", "undecided").
    pub coding_type: SymId,
    /// Mnemonic character shown in the mode line.
    pub mnemonic: char,
    /// End-of-line conversion type.
    pub eol_type: EolType,
    /// Whether this coding system is ASCII compatible.
    pub ascii_compatible_p: bool,
    /// Charset list (names of supported charsets).
    pub charset_list: Vec<SymId>,
    /// Post-read conversion function name.
    pub post_read_conversion: Option<SymId>,
    /// Pre-write conversion function name.
    pub pre_write_conversion: Option<SymId>,
    /// Default character for encoding.
    pub default_char: Option<char>,
    /// Whether this is for unibyte buffers.
    pub for_unibyte: bool,
    /// Arbitrary property list for coding-system-get / coding-system-put.
    pub properties: HashMap<SymId, Value>,
    /// Integer property slots used by coding-system-get / coding-system-put.
    pub int_properties: HashMap<i64, Value>,
}

impl CodingSystemInfo {
    fn new(name: &str, coding_type: &str, mnemonic: char, eol_type: EolType) -> Self {
        Self {
            name: intern(name),
            coding_type: intern(coding_type),
            mnemonic,
            eol_type,
            ascii_compatible_p: false,
            charset_list: Vec::new(),
            post_read_conversion: None,
            pre_write_conversion: None,
            default_char: None,
            for_unibyte: false,
            properties: HashMap::new(),
            int_properties: HashMap::new(),
        }
    }

    /// Return the base name (strip -unix/-dos/-mac suffix).
    #[cfg(test)]
    fn base_name(&self) -> String {
        let name = resolve_sym(self.name);
        for suffix in &["-unix", "-dos", "-mac"] {
            if name.ends_with(suffix) {
                return name[..name.len() - suffix.len()].to_string();
            }
        }
        name.to_string()
    }
}

// ---------------------------------------------------------------------------
// CodingSystemManager
// ---------------------------------------------------------------------------

/// Central registry for all coding systems and their aliases.
pub struct CodingSystemManager {
    /// Registered coding systems, keyed by canonical name.
    pub systems: HashMap<SymId, CodingSystemInfo>,
    /// Alias -> canonical name mapping.
    pub aliases: HashMap<SymId, SymId>,
    /// GNU stores aliases in each coding-system spec and appends new aliases
    /// at the tail.  Keep the same order for `coding-system-aliases`.
    pub alias_order: HashMap<SymId, Vec<SymId>>,
    /// Detection priority list (ordered list of system names).
    pub priority: Vec<SymId>,
    /// Current keyboard coding system.
    keyboard_coding: SymId,
    /// Current terminal coding system.
    terminal_coding: SymId,
}

impl CodingSystemManager {
    /// Create a new manager pre-populated with the standard coding systems.
    pub fn new() -> Self {
        let mut mgr = Self {
            systems: HashMap::new(),
            aliases: HashMap::new(),
            alias_order: HashMap::new(),
            priority: Vec::new(),
            keyboard_coding: intern("utf-8-unix"),
            terminal_coding: intern("utf-8-unix"),
        };

        // Register standard coding systems
        mgr.register(CodingSystemInfo::new(
            "utf-8",
            "utf-8",
            'U',
            EolType::Undecided,
        ));
        mgr.register(CodingSystemInfo::new(
            "utf-8-unix",
            "utf-8",
            'U',
            EolType::Unix,
        ));
        mgr.register(CodingSystemInfo::new(
            "utf-8-dos",
            "utf-8",
            'U',
            EolType::Dos,
        ));
        mgr.register(CodingSystemInfo::new(
            "utf-8-mac",
            "utf-8",
            'U',
            EolType::Mac,
        ));
        for (name, eol) in [
            ("utf-8-with-signature", EolType::Undecided),
            ("utf-8-with-signature-unix", EolType::Unix),
            ("utf-8-with-signature-dos", EolType::Dos),
            ("utf-8-with-signature-mac", EolType::Mac),
        ] {
            let mut info = CodingSystemInfo::new(name, "utf-8", 'U', eol);
            info.charset_list = vec![intern("unicode")];
            info.properties.insert(intern(":bom"), Value::T);
            mgr.register(info);
        }
        mgr.register(CodingSystemInfo::new(
            "iso-latin-1",
            "charset",
            'l',
            EolType::Undecided,
        ));
        mgr.register(CodingSystemInfo::new(
            "iso-latin-1-unix",
            "charset",
            'l',
            EolType::Unix,
        ));
        mgr.register(CodingSystemInfo::new(
            "iso-latin-1-dos",
            "charset",
            'l',
            EolType::Dos,
        ));
        mgr.register(CodingSystemInfo::new(
            "iso-latin-1-mac",
            "charset",
            'l',
            EolType::Mac,
        ));
        mgr.register(CodingSystemInfo::new(
            "iso-latin-5",
            "charset",
            '9',
            EolType::Undecided,
        ));
        mgr.register(CodingSystemInfo::new(
            "iso-latin-5-unix",
            "charset",
            '9',
            EolType::Unix,
        ));
        mgr.register(CodingSystemInfo::new(
            "iso-latin-5-dos",
            "charset",
            '9',
            EolType::Dos,
        ));
        mgr.register(CodingSystemInfo::new(
            "iso-latin-5-mac",
            "charset",
            '9',
            EolType::Mac,
        ));
        mgr.register(CodingSystemInfo::new(
            "iso-latin-9",
            "charset",
            '0',
            EolType::Undecided,
        ));
        mgr.register(CodingSystemInfo::new(
            "iso-latin-9-unix",
            "charset",
            '0',
            EolType::Unix,
        ));
        mgr.register(CodingSystemInfo::new(
            "iso-latin-9-dos",
            "charset",
            '0',
            EolType::Dos,
        ));
        mgr.register(CodingSystemInfo::new(
            "iso-latin-9-mac",
            "charset",
            '0',
            EolType::Mac,
        ));
        mgr.register(CodingSystemInfo::new(
            "us-ascii",
            "charset",
            'A',
            EolType::Undecided,
        ));
        mgr.register(CodingSystemInfo::new(
            "us-ascii-unix",
            "charset",
            'A',
            EolType::Unix,
        ));
        mgr.register(CodingSystemInfo::new(
            "us-ascii-dos",
            "charset",
            'A',
            EolType::Dos,
        ));
        mgr.register(CodingSystemInfo::new(
            "us-ascii-mac",
            "charset",
            'A',
            EolType::Mac,
        ));
        mgr.register(CodingSystemInfo::new(
            "raw-text",
            "raw-text",
            '=',
            EolType::Undecided,
        ));
        mgr.register(CodingSystemInfo::new(
            "raw-text-unix",
            "raw-text",
            '=',
            EolType::Unix,
        ));
        mgr.register(CodingSystemInfo::new(
            "raw-text-dos",
            "raw-text",
            '=',
            EolType::Dos,
        ));
        mgr.register(CodingSystemInfo::new(
            "raw-text-mac",
            "raw-text",
            '=',
            EolType::Mac,
        ));
        mgr.register(CodingSystemInfo::new(
            "undecided",
            "undecided",
            '-',
            EolType::Undecided,
        ));
        mgr.register(CodingSystemInfo::new(
            "undecided-unix",
            "undecided",
            '-',
            EolType::Unix,
        ));
        mgr.register(CodingSystemInfo::new(
            "undecided-dos",
            "undecided",
            '-',
            EolType::Dos,
        ));
        mgr.register(CodingSystemInfo::new(
            "undecided-mac",
            "undecided",
            '-',
            EolType::Mac,
        ));
        mgr.register(CodingSystemInfo::new(
            "utf-8-emacs",
            "utf-8",
            'U',
            EolType::Undecided,
        ));
        mgr.register(CodingSystemInfo::new(
            "utf-8-emacs-unix",
            "utf-8",
            'U',
            EolType::Unix,
        ));
        mgr.register(CodingSystemInfo::new(
            "utf-8-emacs-dos",
            "utf-8",
            'U',
            EolType::Dos,
        ));
        mgr.register(CodingSystemInfo::new(
            "utf-8-emacs-mac",
            "utf-8",
            'U',
            EolType::Mac,
        ));
        mgr.register(CodingSystemInfo::new(
            "no-conversion",
            "raw-text",
            '=',
            EolType::Unix,
        ));
        for (name, eol) in [
            ("utf-8-auto", EolType::Undecided),
            ("utf-8-auto-unix", EolType::Unix),
            ("utf-8-auto-dos", EolType::Dos),
            ("utf-8-auto-mac", EolType::Mac),
        ] {
            mgr.register(CodingSystemInfo::new(name, "utf-8", 'U', eol));
        }
        mgr.register(CodingSystemInfo::new(
            "utf-16",
            "utf-16",
            'U',
            EolType::Undecided,
        ));
        mgr.register(CodingSystemInfo::new(
            "utf-16be",
            "utf-16",
            'U',
            EolType::Undecided,
        ));
        mgr.register(CodingSystemInfo::new(
            "utf-16le",
            "utf-16",
            'U',
            EolType::Undecided,
        ));
        mgr.register(CodingSystemInfo::new(
            "utf-16be-with-signature",
            "utf-16",
            'U',
            EolType::Undecided,
        ));
        mgr.register(CodingSystemInfo::new(
            "utf-16le-with-signature",
            "utf-16",
            'U',
            EolType::Undecided,
        ));
        for (name, eol) in [
            ("prefer-utf-8", EolType::Undecided),
            ("prefer-utf-8-unix", EolType::Unix),
            ("prefer-utf-8-dos", EolType::Dos),
            ("prefer-utf-8-mac", EolType::Mac),
        ] {
            mgr.register(CodingSystemInfo::new(name, "undecided", '-', eol));
        }
        for (name, eol) in [
            ("chinese-iso-8bit", EolType::Undecided),
            ("chinese-iso-8bit-unix", EolType::Unix),
            ("chinese-iso-8bit-dos", EolType::Dos),
            ("chinese-iso-8bit-mac", EolType::Mac),
        ] {
            let mut info = CodingSystemInfo::new(name, "iso-2022", 'c', eol);
            info.ascii_compatible_p = true;
            info.charset_list = vec![intern("ascii"), intern("chinese-gb2312")];
            mgr.register(info);
        }
        for (name, eol) in [
            ("chinese-big5", EolType::Undecided),
            ("chinese-big5-unix", EolType::Unix),
            ("chinese-big5-dos", EolType::Dos),
            ("chinese-big5-mac", EolType::Mac),
        ] {
            let mut info = CodingSystemInfo::new(name, "big5", 'B', eol);
            info.ascii_compatible_p = true;
            info.charset_list = vec![intern("ascii"), intern("big5")];
            mgr.register(info);
        }
        for (name, eol) in [
            ("chinese-big5-hkscs", EolType::Undecided),
            ("chinese-big5-hkscs-unix", EolType::Unix),
            ("chinese-big5-hkscs-dos", EolType::Dos),
            ("chinese-big5-hkscs-mac", EolType::Mac),
        ] {
            let mut info = CodingSystemInfo::new(name, "charset", 'B', eol);
            info.ascii_compatible_p = true;
            info.charset_list = vec![intern("ascii"), intern("big5-hkscs")];
            mgr.register(info);
        }
        for (name, eol) in [
            ("chinese-gbk", EolType::Undecided),
            ("chinese-gbk-unix", EolType::Unix),
            ("chinese-gbk-dos", EolType::Dos),
            ("chinese-gbk-mac", EolType::Mac),
        ] {
            let mut info = CodingSystemInfo::new(name, "charset", 'c', eol);
            info.ascii_compatible_p = true;
            info.charset_list = vec![intern("ascii"), intern("chinese-gbk")];
            mgr.register(info);
        }
        for (name, eol) in [
            ("chinese-gb18030", EolType::Undecided),
            ("chinese-gb18030-unix", EolType::Unix),
            ("chinese-gb18030-dos", EolType::Dos),
            ("chinese-gb18030-mac", EolType::Mac),
        ] {
            let mut info = CodingSystemInfo::new(name, "charset", 'c', eol);
            info.ascii_compatible_p = true;
            info.charset_list = vec![
                intern("ascii"),
                intern("gb18030-2-byte"),
                intern("gb18030-4-byte-bmp"),
                intern("gb18030-4-byte-smp"),
                intern("gb18030-4-byte-ext-1"),
                intern("gb18030-4-byte-ext-2"),
            ];
            mgr.register(info);
        }

        // Common aliases
        mgr.add_alias("mule-utf-8", "utf-8");
        mgr.add_alias("cp65001", "utf-8");
        mgr.add_alias("iso-8859-1", "iso-latin-1");
        mgr.add_alias("latin-1", "iso-latin-1");
        mgr.add_alias("iso-8859-9", "iso-latin-5");
        mgr.add_alias("latin-5", "iso-latin-5");
        mgr.add_alias("iso-8859-15", "iso-latin-9");
        mgr.add_alias("latin-9", "iso-latin-9");
        mgr.add_alias("latin-0", "iso-latin-9");
        mgr.add_alias("iso-safe", "us-ascii");
        mgr.add_alias("ascii", "us-ascii");
        mgr.add_alias("cn-gb-2312", "chinese-iso-8bit");
        mgr.add_alias("euc-china", "chinese-iso-8bit");
        mgr.add_alias("euc-cn", "chinese-iso-8bit");
        mgr.add_alias("cn-gb", "chinese-iso-8bit");
        mgr.add_alias("gb2312", "chinese-iso-8bit");
        mgr.add_alias("big5", "chinese-big5");
        mgr.add_alias("cn-big5", "chinese-big5");
        mgr.add_alias("cp950", "chinese-big5");
        mgr.add_alias("big5-hkscs", "chinese-big5-hkscs");
        mgr.add_alias("cn-big5-hkscs", "chinese-big5-hkscs");
        mgr.add_alias("gbk", "chinese-gbk");
        mgr.add_alias("cp936", "chinese-gbk");
        mgr.add_alias("windows-936", "chinese-gbk");
        mgr.add_alias("gb18030", "chinese-gb18030");
        mgr.add_alias("binary", "no-conversion");
        mgr.add_alias("emacs-internal", "utf-8-emacs-unix");
        mgr.add_alias("utf-16-le", "utf-16le-with-signature");
        mgr.add_alias("utf-16-be", "utf-16be-with-signature");

        // Default detection priority list.  GNU keeps one entry per coding
        // *category* (coding.c `coding_priorities`/`coding_categories`).  We
        // seed it with the post-startup order GNU reaches after
        // `reset-language-environment` runs `set-coding-system-priority`
        // (utf-8, iso-2022-7bit, iso-latin-1, ... first), so this matches the
        // booted runtime's `coding-system-priority-list`.  `coding-category-ccl`
        // has no bound coding system and is omitted (GNU skips id<0 entries),
        // giving 20 entries like GNU.  `set-coding-system-priority` reorders
        // these by category and is idempotent on this order.
        mgr.priority = vec![
            intern("utf-8"),                   // coding-category-utf-8
            intern("iso-2022-7bit"),           // coding-category-iso-7
            intern("iso-latin-1"),             // coding-category-charset
            intern("iso-2022-7bit-lock"),      // coding-category-iso-7-else
            intern("iso-2022-8bit-ss2"),       // coding-category-iso-8-else
            intern("emacs-mule"),              // coding-category-emacs-mule
            intern("raw-text"),                // coding-category-raw-text
            intern("iso-2022-jp"),             // coding-category-iso-7-tight
            intern("in-is13194-devanagari"),   // coding-category-iso-8-1
            intern("chinese-iso-8bit"),        // coding-category-iso-8-2
            intern("utf-8-auto"),              // coding-category-utf-8-auto
            intern("utf-8-with-signature"),    // coding-category-utf-8-sig
            intern("utf-16"),                  // coding-category-utf-16-auto
            intern("utf-16be-with-signature"), // coding-category-utf-16-be
            intern("utf-16le-with-signature"), // coding-category-utf-16-le
            intern("utf-16be"),                // coding-category-utf-16-be-nosig
            intern("utf-16le"),                // coding-category-utf-16-le-nosig
            intern("japanese-shift-jis"),      // coding-category-sjis
            intern("chinese-big5"),            // coding-category-big5
            intern("undecided"),               // coding-category-undecided
        ];

        mgr
    }

    /// Register a coding system.
    fn register(&mut self, info: CodingSystemInfo) {
        let name = info.name;
        self.systems.insert(name, info);
        self.alias_order.entry(name).or_insert_with(|| vec![name]);
    }

    /// Resolve a name through the alias table to a canonical name.
    /// Returns either the input name (if it's a direct system) or the
    /// canonical name from the alias table.
    pub fn resolve(&self, name: &str) -> Option<SymId> {
        let name = lookup_interned(name)?;
        if self.systems.contains_key(&name) {
            Some(name)
        } else {
            self.aliases
                .get(&name)
                .copied()
                .filter(|canonical| self.systems.contains_key(canonical))
        }
    }

    /// Look up a coding system by name (resolving aliases).
    pub fn get(&self, name: &str) -> Option<&CodingSystemInfo> {
        let canonical = self.resolve(name)?;
        self.systems.get(&canonical)
    }

    /// Look up a coding system mutably by name (resolving aliases).
    pub fn get_mut(&mut self, name: &str) -> Option<&mut CodingSystemInfo> {
        let canonical = self.resolve(name)?;
        self.systems.get_mut(&canonical)
    }

    /// Check if a name is a known coding system (or alias).
    pub fn is_known(&self, name: &str) -> bool {
        self.resolve(name).is_some()
    }

    /// Check if a name is a known coding system, alias, or derived EOL variant.
    pub fn is_known_or_derived(&self, name: &str) -> bool {
        is_known_or_derived_coding_system(self, name)
    }

    /// Whether the named coding system is ASCII-compatible (GNU
    /// `CODING_ATTR_ASCII_COMPAT`).  EOL variants share their base's value.
    /// Unknown coding systems are treated as ASCII-compatible (the byte-faithful
    /// default), matching how `call-process` only downgrades to `raw-text` when
    /// it can prove the coding is not ASCII-compatible.
    pub fn is_ascii_compatible(&self, name: &str) -> bool {
        let resolved = self
            .canonical_runtime_name(name)
            .unwrap_or_else(|| name.to_string());
        match runtime_bucket_name(self, &resolved).and_then(|bucket| self.get(&bucket)) {
            Some(info) => compute_coding_ascii_compat(info),
            None => true,
        }
    }

    /// Return the canonical runtime name for a coding system or alias.
    pub(crate) fn canonical_runtime_name(&self, name: &str) -> Option<String> {
        canonical_runtime_name(self, name)
    }

    pub(crate) fn contains_runtime_symbol(&self, symbol: SymId) -> bool {
        self.systems.iter().any(|(name, info)| {
            *name == symbol
                || info.name == symbol
                || info.coding_type == symbol
                || info.charset_list.contains(&symbol)
                || info.post_read_conversion == Some(symbol)
                || info.pre_write_conversion == Some(symbol)
                || info.properties.contains_key(&symbol)
                || info
                    .int_properties
                    .values()
                    .any(|value| matches!(value.kind(), ValueKind::Symbol(id) if id == symbol))
                || info
                    .properties
                    .values()
                    .any(|value| matches!(value.kind(), ValueKind::Symbol(id) if id == symbol))
        }) || self
            .aliases
            .iter()
            .any(|(alias, target)| *alias == symbol || *target == symbol)
            || self
                .alias_order
                .iter()
                .any(|(base, aliases)| *base == symbol || aliases.contains(&symbol))
            || self.priority.contains(&symbol)
            || self.keyboard_coding == symbol
            || self.terminal_coding == symbol
    }

    /// Return the canonical coding system with EOL detected from file bytes.
    pub(crate) fn canonical_name_for_detected_eol(
        &self,
        name: &str,
        eol_suffix: &str,
    ) -> Option<String> {
        let normalized = normalize_coding_name_for_lookup(name);
        if EolType::from_suffix(normalized).is_some() {
            return resolve_runtime_name(self, normalized);
        }

        // A coding system whose eol_type is already a concrete value (e.g. the
        // bare `unix`/`dos`/`mac` EOL codings, or `coding-system-for-read 'unix`)
        // must NOT have its EOL overridden by detection: GNU only rewrites the
        // EOL when eol_type is a vector (undecided). Otherwise reading a CRLF
        // file with an explicit `unix` coding would wrongly strip the CR.
        if let Some(info) = self.get(normalized)
            && info.eol_type.specified_index().is_some()
        {
            return canonical_runtime_name(self, normalized);
        }

        let eol = match eol_suffix {
            "-unix" => 0,
            "-dos" => 1,
            "-mac" => 2,
            _ => return canonical_runtime_name(self, normalized),
        };
        let canonical_base = self.resolve(normalized)?;
        derive_coding_for_eol(resolve_sym(canonical_base), eol)
            .or_else(|| canonical_runtime_name(self, normalized))
    }

    /// Add an alias mapping.
    pub fn add_alias(&mut self, alias: &str, target: &str) {
        let alias_id = intern(alias);
        let target_id = self.resolve(target).unwrap_or_else(|| intern(target));
        self.aliases.insert(alias_id, target_id);
        let aliases = self
            .alias_order
            .entry(target_id)
            .or_insert_with(|| vec![target_id]);
        if !aliases.contains(&alias_id) {
            aliases.push(alias_id);
        }
    }

    /// Get all aliases that point to a given canonical name.
    pub fn aliases_for(&self, canonical: SymId) -> Vec<SymId> {
        self.alias_order
            .get(&canonical)
            .cloned()
            .unwrap_or_else(|| vec![canonical])
    }

    /// List all registered coding system names (canonical only).
    pub fn list_all(&self) -> Vec<SymId> {
        let mut names: Vec<SymId> = self.systems.keys().copied().collect();
        names.sort_by(|left, right| resolve_sym(*left).cmp(resolve_sym(*right)));
        names
    }

    pub(crate) fn keyboard_coding_sym(&self) -> SymId {
        self.keyboard_coding
    }
    pub(crate) fn terminal_coding_sym(&self) -> SymId {
        self.terminal_coding
    }
    pub(crate) fn dump_keyboard_coding_sym(&self) -> SymId {
        self.keyboard_coding
    }
    pub(crate) fn dump_terminal_coding_sym(&self) -> SymId {
        self.terminal_coding
    }
    pub(crate) fn from_dump(
        systems: HashMap<SymId, CodingSystemInfo>,
        aliases: HashMap<SymId, SymId>,
        alias_order: HashMap<SymId, Vec<SymId>>,
        priority: Vec<SymId>,
        keyboard_coding: SymId,
        terminal_coding: SymId,
    ) -> Self {
        Self {
            systems,
            aliases,
            alias_order,
            priority,
            keyboard_coding,
            terminal_coding,
        }
    }

    /// Collect GC roots from coding system properties.
    pub fn trace_roots(&self, roots: &mut Vec<Value>) {
        for info in self.systems.values() {
            for value in info.properties.values() {
                roots.push(*value);
            }
            for value in info.int_properties.values() {
                roots.push(*value);
            }
        }
    }
}

fn cons_coding_system_variable(obarray: &mut Obarray, name: SymId) {
    let var = intern("coding-system-list");
    let current = obarray.symbol_value_id_or_nil(var);
    obarray.set_symbol_value_id(var, Value::cons(Value::from_sym_id(name), current));
}

fn coding_system_alist_has_name(mut alist: Value, name: &str) -> bool {
    while alist.is_cons() {
        let entry = alist.cons_car();
        if entry.is_cons()
            && let Some(entry_name) = entry.cons_car().as_lisp_string()
            && entry_name.as_bytes() == name.as_bytes()
        {
            return true;
        }
        alist = alist.cons_cdr();
    }
    false
}

fn cons_coding_system_alist(obarray: &mut Obarray, name: SymId) {
    let var = intern("coding-system-alist");
    let current = obarray.symbol_value_id_or_nil(var);
    let name_str = resolve_sym(name);
    if coding_system_alist_has_name(current, name_str) {
        return;
    }
    let entry = Value::cons(Value::string(name_str), Value::NIL);
    obarray.set_symbol_value_id(var, Value::cons(entry, current));
}

fn record_coding_system_name(obarray: &mut Obarray, name: SymId) {
    cons_coding_system_variable(obarray, name);
    cons_coding_system_alist(obarray, name);
}

/// Mirror GNU `define-coding-system-internal`'s updates to
/// `Vcoding_system_list` and `Vcoding_system_alist` in `src/coding.c`.
/// When EOL is unspecified, GNU records the three subsidiary systems first
/// and then the base coding system, all by consing onto the front.
pub(crate) fn record_lisp_define_coding_system_internal(obarray: &mut Obarray, args: &[Value]) {
    if args.len() < 13 {
        return;
    }
    let Some(name_id) = args[0].as_symbol_id() else {
        return;
    };
    let name = resolve_sym(name_id);
    let eol_unspecified = args[12].is_nil();
    if eol_unspecified && EolType::from_suffix(name).is_none() {
        for suffix in ["-unix", "-dos", "-mac"] {
            record_coding_system_name(obarray, intern(&format!("{name}{suffix}")));
        }
    }
    record_coding_system_name(obarray, name_id);
}

/// Mirror GNU `define-coding-system-alias`: append alias metadata in the
/// runtime registry, then cons the alias onto the Lisp-visible list and alist.
pub(crate) fn record_lisp_define_coding_system_alias(obarray: &mut Obarray, args: &[Value]) {
    if args.len() != 2 {
        return;
    }
    let Some(alias_id) = args[0].as_symbol_id() else {
        return;
    };
    record_coding_system_name(obarray, alias_id);
}

impl crate::gc_trace::GcTrace for CodingSystemManager {
    fn trace_roots(&self, roots: &mut Vec<Value>) {
        CodingSystemManager::trace_roots(self, roots);
    }
}

fn property_lookup(info: &CodingSystemInfo, prop: SymId) -> Option<Value> {
    if let Some(value) = info.properties.get(&prop) {
        return Some(*value);
    }
    let prop_name = resolve_sym(prop);
    if !prop_name.starts_with(':') {
        let colon_key = intern(&format!(":{prop_name}"));
        return info.properties.get(&colon_key).copied();
    }
    None
}

fn plist_push_key(plist: &mut Vec<Value>, key: SymId, value: Value) {
    plist.push(Value::from_sym_id(key));
    plist.push(value);
}

/// Fold the properties set via `coding-system-put` onto a freshly reconstructed
/// `coding-system-plist`.
///
/// GNU keeps one mutable plist on the shared coding spec; `coding-system-put`
/// does `plist_put` on it (src/coding.c `Fcoding_system_put`), so an
/// overwrite of an already-present property (e.g. `:mime-charset`) replaces it
/// in place while a brand-new property is appended at the tail.  neomacs
/// reconstructs the computed part of the plist, then applies each stored
/// override with the same `plist_put` semantics to match GNU's ordering for
/// both built-in (`define-coding-system-internal`) and reconstructed systems.
fn apply_put_overrides(info: &CodingSystemInfo, mut reconstructed: Vec<Value>) -> Value {
    let Some(overrides) = info.int_properties.get(&PUT_OVERRIDES_KEY) else {
        return Value::list(reconstructed);
    };
    let Some(pairs) = super::value::list_to_vec(overrides) else {
        return Value::list(reconstructed);
    };
    if pairs.is_empty() {
        return Value::list(reconstructed);
    }
    let mut plist = Value::list(std::mem::take(&mut reconstructed));
    let mut i = 0;
    while i + 1 < pairs.len() {
        if let Ok((next, _)) = super::plist::plist_put(plist, pairs[i], pairs[i + 1]) {
            plist = next;
        }
        i += 2;
    }
    plist
}

fn first_emacs_char_code(value: Value) -> Option<i64> {
    match value.kind() {
        ValueKind::Fixnum(c) => Some(c),
        ValueKind::String => {
            let string = value.as_lisp_string()?;
            if string.is_empty() {
                return Some(0);
            }
            let (ch, _) = if string.is_multibyte() {
                super::emacs_char::string_char(string.as_bytes())
            } else {
                (string.as_bytes()[0] as u32, 1)
            };
            Some(ch as i64)
        }
        _ => None,
    }
}

impl Default for CodingSystemManager {
    fn default() -> Self {
        Self::new()
    }
}

// ===========================================================================
// Pure builtins
// ===========================================================================

/// `(coding-system-list &optional BASE-ONLY)` -- return a list of all coding systems.
/// If BASE-ONLY is non-nil, only return base systems (no -unix/-dos/-mac variants).
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_coding_system_list(
    mgr: &CodingSystemManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("coding-system-list", &args, 1)?;
    let base_only = args.first().is_some_and(|v| v.is_truthy());
    let names = mgr.list_all();
    let filtered: Vec<Value> = names
        .into_iter()
        .filter(|id| {
            let n = resolve_sym(*id);
            if base_only {
                !n.ends_with("-unix") && !n.ends_with("-dos") && !n.ends_with("-mac")
            } else {
                true
            }
        })
        .map(Value::symbol)
        .collect();
    Ok(Value::list(filtered))
}

/// `(coding-system-aliases CODING-SYSTEM)` -- return a list of aliases for a
/// coding system (including the name itself as the first element).
pub(crate) fn builtin_coding_system_aliases(
    mgr: &CodingSystemManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("coding-system-aliases", &args, 1)?;
    if args[0].is_string() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), args[0]],
        ));
    }
    let raw_name = coding_symbol_name(&args[0])?;
    let resolved_name = resolve_runtime_name(mgr, &raw_name)
        .ok_or_else(|| signal(LispCondition::CodingSystemError, vec![args[0]]))?;
    let base = strip_eol_suffix(&resolved_name);

    if matches!(base, "binary" | "no-conversion") {
        return Ok(Value::list(vec![
            Value::symbol("no-conversion"),
            Value::symbol("binary"),
        ]));
    }

    let suffix = EolType::from_suffix(&resolved_name)
        .map(|eol| eol.suffix())
        .unwrap_or("");
    let canonical = runtime_bucket_name(mgr, &resolved_name)
        .ok_or_else(|| signal(LispCondition::CodingSystemError, vec![args[0]]))?;
    let display = display_base_name(strip_eol_suffix(&resolved_name)).to_string();
    let canonical_id = mgr
        .resolve(&canonical)
        .ok_or_else(|| signal(LispCondition::CodingSystemError, vec![args[0]]))?;
    let aliases = mgr.aliases_for(canonical_id);
    let mut names = vec![format!("{display}{suffix}")];
    for alias in aliases {
        let alias = resolve_sym(alias);
        if alias != display {
            names.push(format!("{alias}{suffix}"));
        }
    }
    if canonical != display
        && !names
            .iter()
            .any(|name| name == &format!("{canonical}{suffix}"))
    {
        names.push(format!("{canonical}{suffix}"));
    }
    Ok(Value::list(names.into_iter().map(Value::symbol).collect()))
}

/// `(coding-system-get CODING-SYSTEM PROP)` -- get a property of a coding system.
/// Recognized built-in properties: :name, :type, :mnemonic, :eol-type.
/// Other properties are looked up from the per-system property list.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_coding_system_get(mgr: &CodingSystemManager, args: Vec<Value>) -> EvalResult {
    expect_args("coding-system-get", &args, 2)?;
    let coding_name = coding_symbol_name(&args[0])?;
    let resolved_name = resolve_runtime_name(mgr, &coding_name)
        .ok_or_else(|| signal(LispCondition::CodingSystemError, vec![args[0]]))?;
    let bucket = runtime_bucket_name(mgr, &resolved_name)
        .ok_or_else(|| signal(LispCondition::CodingSystemError, vec![args[0]]))?;
    let info = mgr
        .get(&bucket)
        .ok_or_else(|| signal(LispCondition::CodingSystemError, vec![args[0]]))?;

    if let Some(prop_id) = args[1].as_symbol_id() {
        let prop_name = resolve_sym(prop_id);
        if let Some(value) = property_lookup(info, prop_id) {
            return Ok(value);
        }
        return match prop_name {
            ":name" | "name" => Ok(Value::symbol(display_base_name(strip_eol_suffix(
                &resolved_name,
            )))),
            ":coding-type" | "coding-type" => Ok(Value::symbol(
                coding_type_for_base(strip_eol_suffix(&resolved_name))
                    .unwrap_or(resolve_sym(info.coding_type)),
            )),
            ":type" | "type" => Ok(Value::NIL),
            ":mnemonic" | "mnemonic" => Ok(Value::fixnum(
                default_mnemonic_for_base(strip_eol_suffix(&resolved_name))
                    .unwrap_or(info.mnemonic as i64),
            )),
            ":charset-list" | "charset-list" => Ok(Value::list(
                info.charset_list
                    .iter()
                    .copied()
                    .map(Value::from_sym_id)
                    .collect(),
            )),
            ":post-read-conversion" | "post-read-conversion" => Ok(info
                .post_read_conversion
                .map(Value::from_sym_id)
                .unwrap_or(Value::NIL)),
            ":pre-write-conversion" | "pre-write-conversion" => Ok(info
                .pre_write_conversion
                .map(Value::from_sym_id)
                .unwrap_or(Value::NIL)),
            ":eol-type" | "eol-type" => Ok(Value::NIL),
            _ => Ok(Value::NIL),
        };
    }

    if let Some(int_key) = args[1].as_int()
        && let Some(value) = info.int_properties.get(&int_key)
    {
        return Ok(*value);
    }

    Err(signal(
        LispCondition::WrongTypeArgument,
        vec![Value::symbol("symbolp"), args[1]],
    ))
}

fn coding_category_for_base(base: &str) -> &'static str {
    match base {
        "utf-8" | "utf-8-emacs" | "emacs-internal" => "coding-category-utf-8",
        "utf-8-auto" => "coding-category-utf-8-auto",
        "utf-8-with-signature" => "coding-category-utf-8-sig",
        // `chinese-iso-8bit` is `:coding-type iso-2022` with G1 = chinese-gb2312
        // (a dimension-2 charset), so GNU classifies it as `coding-category-iso-8-2`
        // (coding.c `setup_coding_system`), NOT charset.  Keeping it under charset
        // collides with `iso-latin-1` and gets it dropped from the priority list
        // when `set-coding-system-priority` fronts the charset category.
        "cn-gb-2312" | "euc-china" | "euc-cn" | "cn-gb" | "gb2312" | "chinese-iso-8bit" => {
            "coding-category-iso-8-2"
        }
        // `chinese-big5` is `:coding-type big5`, which GNU maps to
        // `coding-category-big5` (a distinct category from charset).  Note the
        // HKSCS variant (`chinese-big5-hkscs`) is `:coding-type charset` in GNU,
        // so it stays under the charset arm below.
        "big5" | "cn-big5" | "cp950" | "chinese-big5" => "coding-category-big5",
        "latin-1" | "iso-8859-1" | "iso-latin-1" | "latin-5" | "iso-8859-9" | "iso-latin-5"
        | "latin-0" | "latin-9" | "iso-8859-15" | "iso-latin-9" | "ascii" | "us-ascii"
        | "big5-hkscs" | "cn-big5-hkscs" | "chinese-big5-hkscs" => "coding-category-charset",
        "raw-text" | "binary" | "no-conversion" => "coding-category-raw-text",
        "undecided" | "prefer-utf-8" => "coding-category-undecided",
        _ => "coding-category-undecided",
    }
}

fn coding_docstring_for_base(base: &str) -> Option<&'static str> {
    match base {
        "utf-8" | "utf-8-emacs" | "emacs-internal" => Some("UTF-8 (no signature (BOM))"),
        "utf-8-auto" => Some("UTF-8 (auto-detect signature (BOM))"),
        "utf-8-with-signature" => Some("UTF-8 (with signature (BOM))"),
        "latin-1" | "iso-8859-1" | "iso-latin-1" => {
            Some("ISO 2022 based 8-bit encoding for Latin-1 (MIME:ISO-8859-1).")
        }
        "latin-5" | "iso-8859-9" | "iso-latin-5" => {
            Some("ISO 2022 based 8-bit encoding for Latin-5 (MIME:ISO-8859-9).")
        }
        "latin-0" | "latin-9" | "iso-8859-15" | "iso-latin-9" => {
            Some("ISO 2022 based 8-bit encoding for Latin-9 (MIME:ISO-8859-15).")
        }
        "cn-gb-2312" | "euc-china" | "euc-cn" | "cn-gb" | "gb2312" | "chinese-iso-8bit" => {
            Some("ISO 2022 based EUC encoding for Chinese GB2312 (MIME:GB2312).")
        }
        "big5" | "cn-big5" | "cp950" | "chinese-big5" => {
            Some("BIG5 8-bit encoding for Chinese (MIME:Big5)")
        }
        "big5-hkscs" | "cn-big5-hkscs" | "chinese-big5-hkscs" => {
            Some("BIG5-HKSCS 8-bit encoding for Chinese, Hong Kong supplement (MIME:Big5-HKSCS)")
        }
        "ascii" | "us-ascii" => Some("ASCII encoding."),
        "no-conversion" | "binary" | "raw-text" => Some("Do no conversion."),
        "undecided" => Some("Automatic conversion on decode."),
        _ => None,
    }
}

fn coding_charset_list_for_base(base: &str) -> Option<Vec<Value>> {
    match base {
        "utf-8" | "utf-8-emacs" | "utf-8-auto" | "utf-8-with-signature" | "emacs-internal" => {
            Some(vec![Value::symbol("unicode")])
        }
        "latin-1" | "iso-8859-1" | "iso-latin-1" => Some(vec![Value::symbol("iso-8859-1")]),
        "latin-5" | "iso-8859-9" | "iso-latin-5" => Some(vec![Value::symbol("iso-8859-9")]),
        "latin-0" | "latin-9" | "iso-8859-15" | "iso-latin-9" => {
            Some(vec![Value::symbol("iso-8859-15")])
        }
        "cn-gb-2312" | "euc-china" | "euc-cn" | "cn-gb" | "gb2312" | "chinese-iso-8bit" => {
            Some(vec![
                Value::symbol("ascii"),
                Value::symbol("chinese-gb2312"),
            ])
        }
        "big5" | "cn-big5" | "cp950" | "chinese-big5" => {
            Some(vec![Value::symbol("ascii"), Value::symbol("big5")])
        }
        "big5-hkscs" | "cn-big5-hkscs" | "chinese-big5-hkscs" => {
            Some(vec![Value::symbol("ascii"), Value::symbol("big5-hkscs")])
        }
        "ascii" | "us-ascii" => Some(vec![Value::symbol("ascii")]),
        _ => None,
    }
}

fn coding_mime_charset_for_base(base: &str) -> Option<&'static str> {
    match base {
        "utf-8" | "utf-8-emacs" | "utf-8-auto" | "emacs-internal" => Some("utf-8"),
        "latin-1" | "iso-8859-1" | "iso-latin-1" => Some("iso-8859-1"),
        "latin-5" | "iso-8859-9" | "iso-latin-5" => Some("iso-8859-9"),
        "latin-0" | "latin-9" | "iso-8859-15" | "iso-latin-9" => Some("iso-8859-15"),
        "cn-gb-2312" | "euc-china" | "euc-cn" | "cn-gb" | "gb2312" | "chinese-iso-8bit" => {
            Some("gb2312")
        }
        "big5" | "cn-big5" | "cp950" | "chinese-big5" => Some("big5"),
        "big5-hkscs" | "cn-big5-hkscs" | "chinese-big5-hkscs" => Some("big5-hkscs"),
        "ascii" | "us-ascii" => Some("us-ascii"),
        _ => None,
    }
}

/// Whether the coding system carries a byte-order mark (`:bom` non-nil).
fn coding_has_bom(info: &CodingSystemInfo) -> bool {
    info.properties
        .get(&intern(":bom"))
        .is_some_and(|v| v.is_truthy())
}

/// `:bom` shape, mirroring GNU's `CONSP(bom)` / `NILP(bom)` discrimination in
/// `Fdefine_coding_system_internal`: a cons `(le-sig . be-sig)` means auto-detect
/// the signature, `t`/with-signature means always-signature, nil/absent means no
/// signature.
enum BomKind {
    None,
    Auto,
    Sig,
}

fn coding_bom_kind(info: &CodingSystemInfo) -> BomKind {
    match info.properties.get(&intern(":bom")).copied() {
        Some(v) if v.is_cons() => BomKind::Auto,
        Some(v) if v.is_truthy() => BomKind::Sig,
        _ => BomKind::None,
    }
}

/// An explicit `:ascii-compatible-p` value carried in the coding system's
/// property list — either passed to `define-coding-system` or set afterward via
/// `coding-system-put`.  GNU's `Fdefine_coding_system_internal` auto-sets the
/// attribute to `t` for a BOM-less utf-8 coding (src/coding.c:11420), and codings
/// that must NOT be treated as ASCII-compatible (utf-7, utf-7-imap, chinese-hz)
/// override it back to nil with a `coding-system-put` in their Lisp definitions
/// (mule-conf.el / chinese.el).  `vietnamese-viqr` does no such override, so it
/// keeps the auto-set `t` — which is why a pure-ASCII VIQR encode/decode is an
/// identity pass-through.
fn coding_explicit_ascii_compat(info: &CodingSystemInfo) -> Option<bool> {
    info.properties
        .get(&intern(":ascii-compatible-p"))
        .map(|v| v.is_truthy())
}

/// Whether `:charset-list` is the FULL_SUPPORT marker symbol `iso-2022`.
fn coding_is_iso2022_full_support(info: &CodingSystemInfo) -> bool {
    info.charset_list.len() == 1 && resolve_sym(info.charset_list[0]) == "iso-2022"
}

/// Compute the detector category symbol for a coding system, mirroring GNU's
/// `Fdefine_coding_system_internal` (coding.c).
fn compute_coding_category(info: &CodingSystemInfo) -> &'static str {
    match resolve_sym(info.coding_type) {
        "charset" => "coding-category-charset",
        "ccl" => "coding-category-ccl",
        "utf-8" => match coding_bom_kind(info) {
            BomKind::Auto => "coding-category-utf-8-auto",
            BomKind::Sig => "coding-category-utf-8-sig",
            BomKind::None => "coding-category-utf-8",
        },
        "utf-16" => {
            // GNU defaults endian to big when unspecified.
            let little = info
                .properties
                .get(&intern(":endian"))
                .and_then(|v| v.as_symbol_id())
                .map(resolve_sym)
                == Some("little");
            match coding_bom_kind(info) {
                BomKind::Auto => "coding-category-utf-16-auto",
                BomKind::None if little => "coding-category-utf-16-le-nosig",
                BomKind::None => "coding-category-utf-16-be-nosig",
                BomKind::Sig if little => "coding-category-utf-16-le",
                BomKind::Sig => "coding-category-utf-16-be",
            }
        }
        "iso-2022" => {
            let Some(spec) = iso2022_spec(info) else {
                return "coding-category-undecided";
            };
            let full = coding_is_iso2022_full_support(info);
            let lock_or_single = spec.flags.contains(IsoFlag::LockingShift)
                || spec.flags.contains(IsoFlag::SingleShift);
            if spec.flags.contains(IsoFlag::SevenBits) {
                if lock_or_single {
                    "coding-category-iso-7-else"
                } else if full {
                    "coding-category-iso-7"
                } else {
                    "coding-category-iso-7-tight"
                }
            } else {
                let g1_dim = spec.initial[1].and_then(super::charset::charset_dimension_by_sym);
                if spec.flags.contains(IsoFlag::LockingShift) || full || spec.initial[1].is_none() {
                    "coding-category-iso-8-else"
                } else if g1_dim == Some(1) {
                    "coding-category-iso-8-1"
                } else {
                    "coding-category-iso-8-2"
                }
            }
        }
        "emacs-mule" => "coding-category-emacs-mule",
        "shift-jis" => "coding-category-sjis",
        "big5" => "coding-category-big5",
        "raw-text" => "coding-category-raw-text",
        _ => "coding-category-undecided",
    }
}

/// Return the detection category symbol name for a coding system named `name`,
/// mirroring exactly the `:category` value that `coding-system-plist` reports
/// (which `coding-system-category` reads).  This is the runtime equivalent of
/// GNU's `XFIXNUM (CODING_ATTR_CATEGORY (attrs))` in coding.c.
///
/// Returns `None` if `name` does not resolve to a known coding system.
fn coding_category_of(mgr: &CodingSystemManager, name: &str) -> Option<&'static str> {
    let resolved = resolve_runtime_name(mgr, name)?;
    let bucket = runtime_bucket_name(mgr, &resolved)?;
    let info = mgr.get(&bucket)?;
    // Systems defined via `define-coding-system-internal` carry a verbatim
    // plist; their category is computed from the full coding-type info.
    if info.int_properties.contains_key(&PLIST_VERBATIM_KEY) {
        return Some(compute_coding_category(info));
    }
    // Built-in `no-conversion`/`undecided` and the statically registered
    // systems fall back to the per-coding-type mapping used by the generic
    // `coding-system-plist` reconstruction (see `builtin_coding_system_plist`).
    let base = strip_eol_suffix(&resolved);
    let coding_type = coding_type_for_base(base).unwrap_or(resolve_sym(info.coding_type));
    Some(if coding_type == "charset" {
        "coding-category-charset"
    } else {
        coding_category_for_base(base)
    })
}

/// Compute `:ascii-compatible-p` for a coding system, mirroring GNU's
/// per-type overrides (the stored value is the `:ascii-compatible-p` argument).
fn compute_coding_ascii_compat(info: &CodingSystemInfo) -> bool {
    let charsets = &info.charset_list;
    let any_ascii = || {
        charsets
            .iter()
            .any(|&c| super::charset::charset_is_ascii_compatible(c))
    };
    let first_ascii = || {
        charsets
            .first()
            .is_some_and(|&c| super::charset::charset_is_ascii_compatible(c))
    };
    match resolve_sym(info.coding_type) {
        "charset" => any_ascii() || info.ascii_compatible_p,
        // GNU auto-sets `:ascii-compatible-p t` for a BOM-less utf-8 coding
        // (utf-8-with-signature / utf-8-auto carry a BOM and are not), unless the
        // coding's Lisp definition explicitly overrode it (utf-7, utf-7-imap and
        // chinese-hz `coding-system-put` it back to nil; vietnamese-viqr does
        // not).
        "utf-8" => coding_explicit_ascii_compat(info).unwrap_or_else(|| !coding_has_bom(info)),
        "utf-16" => false,
        "iso-2022" => matches!(
            compute_coding_category(info),
            "coding-category-iso-8-1" | "coding-category-iso-8-2"
        ),
        "emacs-mule" | "raw-text" => true,
        "shift-jis" | "big5" => first_ascii(),
        // `undecided` is defined in C with `:ascii-compatible-p t`
        // (src/coding.c `syms_of_coding`), which is what
        // `coding-system-plist` already reports for it here.  Its attribute has
        // to agree, because `code_convert_string`'s identity fast path tests
        // `CODING_ATTR_ASCII_COMPAT` (src/coding.c:9610) and an `undecided`
        // decode of pure ASCII must take it.  `prefer-utf-8` shares the
        // `undecided` coding TYPE but is defined in mule-conf.el WITHOUT the
        // attribute -- measured, GNU 31.0.90 answers t for `undecided` and nil
        // for `prefer-utf-8` -- so this is keyed on the name, not the type.
        "undecided" if strip_eol_suffix(resolve_sym(info.name)) == "undecided" => true,
        _ => info.ascii_compatible_p, // ccl, prefer-utf-8
    }
}

/// The exact `coding-system-plist` for the C-defined `no-conversion` and
/// `undecided` coding systems, mirroring GNU `coding.c` `syms_of_coding`
/// (field order, doubled `:ascii-compatible-p`, docstrings, `:eol-type`).
fn c_init_coding_system_plist(name: &str) -> Option<Vec<Value>> {
    match name {
        "no-conversion" => Some(vec![
            Value::symbol(":ascii-compatible-p"),
            Value::T,
            Value::symbol(":category"),
            Value::symbol("coding-category-raw-text"),
            Value::symbol(":name"),
            Value::symbol("no-conversion"),
            Value::symbol(":mnemonic"),
            Value::fixnum(b'=' as i64),
            Value::symbol(":coding-type"),
            Value::symbol("raw-text"),
            Value::symbol(":ascii-compatible-p"),
            Value::T,
            Value::symbol(":default-char"),
            Value::fixnum(0),
            Value::symbol(":for-unibyte"),
            Value::T,
            Value::symbol(":docstring"),
            Value::string(
                "Do no conversion.\n\nWhen you visit a file with this coding, the file is read into a\nunibyte buffer as is, thus each byte of a file is treated as a\ncharacter.",
            ),
            Value::symbol(":eol-type"),
            Value::symbol("unix"),
        ]),
        "undecided" => Some(vec![
            Value::symbol(":ascii-compatible-p"),
            Value::T,
            Value::symbol(":category"),
            Value::symbol("coding-category-undecided"),
            Value::symbol(":name"),
            Value::symbol("undecided"),
            Value::symbol(":mnemonic"),
            Value::fixnum(b'-' as i64),
            Value::symbol(":coding-type"),
            Value::symbol("undecided"),
            Value::symbol(":ascii-compatible-p"),
            Value::T,
            Value::symbol(":charset-list"),
            Value::list(vec![Value::symbol("ascii")]),
            Value::symbol(":for-unibyte"),
            Value::NIL,
            Value::symbol(":docstring"),
            Value::string("No conversion on encoding, automatic conversion on decoding."),
            Value::symbol(":eol-type"),
            Value::NIL,
        ]),
        _ => None,
    }
}

/// `(coding-system-plist CODING-SYSTEM)` -- return a plist describing
/// CODING-SYSTEM metadata.
pub(crate) fn builtin_coding_system_plist(
    mgr: &CodingSystemManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("coding-system-plist", &args, 1)?;
    if args[0].is_string() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), args[0]],
        ));
    }

    let coding_name = coding_symbol_name(&args[0])?;
    let resolved_name = resolve_runtime_name(mgr, &coding_name)
        .ok_or_else(|| signal(LispCondition::CodingSystemError, vec![args[0]]))?;
    let bucket = runtime_bucket_name(mgr, &resolved_name)
        .ok_or_else(|| signal(LispCondition::CodingSystemError, vec![args[0]]))?;
    let info = mgr
        .get(&bucket)
        .ok_or_else(|| signal(LispCondition::CodingSystemError, vec![args[0]]))?;

    // Coding systems defined via `define-coding-system-internal` carry their
    // verbatim plist (arg 11, already led with :name/:docstring by mule.el).
    // Reproduce GNU's stored plist exactly: prepend `:ascii-compatible-p` then
    // `:category` (computed per coding-type), followed by the verbatim plist.
    if let Some(verbatim) = info.int_properties.get(&PLIST_VERBATIM_KEY) {
        let mut plist = vec![
            Value::symbol(":ascii-compatible-p"),
            if compute_coding_ascii_compat(info) {
                Value::T
            } else {
                Value::NIL
            },
            Value::symbol(":category"),
            Value::symbol(compute_coding_category(info)),
        ];
        if let Some(items) = super::value::list_to_vec(verbatim) {
            plist.extend(items);
        }
        return Ok(apply_put_overrides(info, plist));
    }

    // `no-conversion` and `undecided` are built in C (coding.c `syms_of_coding`)
    // with a fixed plist that the generic reconstruction below cannot reproduce
    // (specific field order, the doubled `:ascii-compatible-p`, the multi-line
    // docstring, and `:eol-type`). Emit GNU's exact plist for them.
    if let Some(plist) = c_init_coding_system_plist(&resolved_name) {
        return Ok(apply_put_overrides(info, plist));
    }

    let base = strip_eol_suffix(&resolved_name);
    let display_name = display_base_name(base);
    let coding_type = coding_type_for_base(base).unwrap_or(resolve_sym(info.coding_type));
    let mnemonic = default_mnemonic_for_base(base).unwrap_or(info.mnemonic as i64);

    let mut plist = Vec::new();
    // NOTE: keep `:ascii-compatible-p` as t here. `coding-system-get` resolves
    // through `coding-system-plist`, and the keyboard-coding suitability check
    // (mule.el `set-keyboard-coding-system`) errors unless the coding system is
    // ascii-compatible; `info.ascii_compatible_p` is not yet computed the way
    // GNU does (it is unreliably nil for utf-8 and EOL variants), so reading it
    // here would make startup fail. Correcting that field is a separate change.
    plist_push_key(&mut plist, intern(":ascii-compatible-p"), Value::T);
    // GNU derives the category from the coding type; charset-type coding
    // systems are `coding-category-charset` (coding.c). Other types keep the
    // per-base mapping.
    let category = if coding_type == "charset" {
        "coding-category-charset"
    } else {
        coding_category_for_base(base)
    };
    plist_push_key(&mut plist, intern(":category"), Value::symbol(category));
    plist_push_key(&mut plist, intern(":name"), Value::symbol(display_name));
    // `:docstring` keeps GNU's position right after `:name`; fall back to the
    // stored property when there is no built-in docstring for this base.
    if let Some(doc) = coding_docstring_for_base(base)
        .map(Value::string)
        .or_else(|| property_lookup(info, intern(":docstring")))
    {
        plist_push_key(&mut plist, intern(":docstring"), doc);
    }
    plist_push_key(
        &mut plist,
        intern(":coding-type"),
        Value::symbol(coding_type),
    );
    plist_push_key(&mut plist, intern(":mnemonic"), Value::fixnum(mnemonic));
    if let Some(charset_list) = coding_charset_list_for_base(base).or_else(|| {
        (!info.charset_list.is_empty()).then(|| {
            info.charset_list
                .iter()
                .copied()
                .map(Value::from_sym_id)
                .collect()
        })
    }) {
        plist_push_key(
            &mut plist,
            intern(":charset-list"),
            Value::list(charset_list),
        );
    }
    if let Some(mime_charset) = coding_mime_charset_for_base(base)
        .map(Value::symbol)
        .or_else(|| property_lookup(info, intern(":mime-charset")))
    {
        plist_push_key(&mut plist, intern(":mime-charset"), mime_charset);
    }
    if let Some(post_read_conversion) = info.post_read_conversion {
        plist_push_key(
            &mut plist,
            intern(":post-read-conversion"),
            Value::from_sym_id(post_read_conversion),
        );
    }
    if let Some(pre_write_conversion) = info.pre_write_conversion {
        plist_push_key(
            &mut plist,
            intern(":pre-write-conversion"),
            Value::from_sym_id(pre_write_conversion),
        );
    }
    if matches!(base, "no-conversion" | "binary" | "raw-text") {
        plist_push_key(&mut plist, intern(":default-char"), Value::fixnum(0));
        plist_push_key(&mut plist, intern(":for-unibyte"), Value::T);
    }

    // Fold in caller-provided properties from `coding-system-put`, preserving
    // GNU's `plist_put` ordering (in-place update for keys already present,
    // append at the tail for new keys).
    Ok(apply_put_overrides(info, plist))
}

/// `(coding-system-put CODING-SYSTEM PROP VAL)` -- set a property of a coding system.
pub(crate) fn builtin_coding_system_put(
    mgr: &mut CodingSystemManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("coding-system-put", &args, 3)?;
    let val = args[2];

    if args[0].is_nil() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("coding-system-p"), Value::NIL],
        ));
    }

    let coding_name = coding_symbol_name(&args[0])?;
    let resolved_name = resolve_runtime_name(mgr, &coding_name)
        .ok_or_else(|| signal(LispCondition::CodingSystemError, vec![args[0]]))?;
    let bucket = runtime_bucket_name(mgr, &resolved_name)
        .ok_or_else(|| signal(LispCondition::CodingSystemError, vec![args[0]]))?;
    let info = mgr
        .get_mut(&bucket)
        .ok_or_else(|| signal(LispCondition::CodingSystemError, vec![args[0]]))?;

    if let Some(prop_id) = args[1].as_symbol_id() {
        let prop_name = resolve_sym(prop_id);
        // GNU `Fcoding_system_put` validates the conversion hooks with
        // CHECK_SYMBOL (src/coding.c:11562, 11567): a non-symbol (e.g. a lambda)
        // is rejected with `wrong-type-argument (symbolp …)`, just as at
        // definition time.
        if matches!(
            prop_name,
            ":post-read-conversion"
                | "post-read-conversion"
                | ":pre-write-conversion"
                | "pre-write-conversion"
        ) {
            check_symbol_hook_arg(&val)?;
        }
        // GNU coerces a `:mnemonic` string to its first character (coding.c
        // `Fcoding_system_put`), and the stored value is the coerced one.
        let stored_val = if matches!(prop_name, ":mnemonic" | "mnemonic") {
            let Some(code) = first_emacs_char_code(val) else {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("characterp"), val],
                ));
            };
            Value::fixnum(code)
        } else {
            val
        };
        info.properties.insert(prop_id, stored_val);
        record_put_override(info, args[1], stored_val);
        return Ok(stored_val);
    }

    if let Some(int_key) = args[1].as_int() {
        info.int_properties.insert(int_key, val);
        record_put_override(info, args[1], val);
        return Ok(val);
    }

    Ok(val)
}

/// Record a `coding-system-put` into the override plist with GNU `plist_put`
/// semantics (in-place update of an existing key, append for a new one), so
/// `coding-system-plist` (and the `coding-system-get` defun that reads it)
/// surfaces it in the same order as GNU's shared spec plist.
fn record_put_override(info: &mut CodingSystemInfo, prop: Value, val: Value) {
    let current = info
        .int_properties
        .get(&PUT_OVERRIDES_KEY)
        .copied()
        .unwrap_or(Value::NIL);
    if let Ok((updated, _)) = super::plist::plist_put(current, prop, val) {
        info.int_properties.insert(PUT_OVERRIDES_KEY, updated);
    }
}

/// `(coding-system-base CODING-SYSTEM)` -- return the base coding system
/// (stripping -unix, -dos, -mac suffixes).
pub(crate) fn builtin_coding_system_base(
    mgr: &CodingSystemManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("coding-system-base", &args, 1)?;
    let name = coding_symbol_name(&args[0])?;
    let resolved_name = resolve_runtime_name(mgr, &name)
        .ok_or_else(|| signal(LispCondition::CodingSystemError, vec![args[0]]))?;
    Ok(Value::symbol(display_base_name(strip_eol_suffix(
        &resolved_name,
    ))))
}

/// `(coding-system-eol-type CODING-SYSTEM)` -- return the EOL type.
/// Returns 0 (unix), 1 (dos), 2 (mac), or a vector of three sub-coding-systems
/// if the EOL type is undecided.
pub(crate) fn builtin_coding_system_eol_type(
    mgr: &CodingSystemManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("coding-system-eol-type", &args, 1)?;
    let Some(name) = args[0].as_symbol_name() else {
        return Ok(Value::NIL);
    };
    let resolved_name = match resolve_runtime_name(mgr, name) {
        Some(resolved) => resolved,
        None => return Ok(Value::NIL),
    };
    if let Some(eol) = EolType::from_suffix(&resolved_name) {
        return Ok(Value::fixnum(eol.to_int()));
    }
    let bucket = match runtime_bucket_name(mgr, &resolved_name) {
        Some(bucket) => bucket,
        None => return Ok(Value::NIL),
    };
    let Some(info) = mgr.get(&bucket) else {
        return Ok(Value::NIL);
    };

    if let Some(index) = info.eol_type.specified_index() {
        return Ok(Value::fixnum(index));
    }
    // Return [base-unix base-dos base-mac] using Emacs display base names.
    let base = eol_vector_base(strip_eol_suffix(&resolved_name));
    let vec = vec![
        Value::symbol(format!("{base}-unix")),
        Value::symbol(format!("{base}-dos")),
        Value::symbol(format!("{base}-mac")),
    ];
    Ok(Value::vector(vec))
}

/// Port of GNU `coding_inherit_eol_type` (src/coding.c) with PARENT == nil.
///
/// If CODING-SYSTEM is nil it becomes `raw-text`.  Then, if its end-of-line
/// type is still undecided (GNU's `VECTORP (AREF (spec, 2))`), it inherits the
/// system EOL (Qunix on Unix/Mac) and is replaced by the unix-suffixed variant
/// (`AREF (eol_type, 0)`, e.g. `latin-1` -> `iso-latin-1-unix`,
/// `raw-text` -> `raw-text-unix`).  A coding system that already carries a
/// concrete EOL is returned unchanged.  Used for the ENCODE side of process
/// coding systems (`set-process-coding-system`), not the DECODE side.
pub(crate) fn coding_inherit_eol_type_unix(
    mgr: &CodingSystemManager,
    coding_system: Value,
) -> Value {
    // GNU: NILP (coding_system) -> coding_system = Qraw_text.
    let coding_system = if coding_system.is_nil() {
        Value::symbol("raw-text")
    } else {
        coding_system
    };
    let Some(name) = coding_system.as_symbol_name() else {
        return coding_system;
    };
    let Some(resolved_name) = resolve_runtime_name(mgr, name) else {
        return coding_system;
    };
    // Concrete EOL already encoded in the name suffix -> unchanged.
    if EolType::from_suffix(&resolved_name).is_some() {
        return coding_system;
    }
    if let Some(bucket) = runtime_bucket_name(mgr, &resolved_name)
        && let Some(info) = mgr.get(&bucket)
    {
        // EOL fixed by the coding-system definition -> unchanged.
        if info.eol_type.specified_index().is_some() {
            return coding_system;
        }
    }
    // Undecided EOL -> inherit system (unix) EOL == AREF (eol_type, 0).
    let base = eol_vector_base(strip_eol_suffix(&resolved_name));
    Value::symbol(format!("{base}-unix"))
}

/// `(coding-system-type CODING-SYSTEM)` -- return the type symbol of the
/// coding system (e.g. utf-8, charset, raw-text, undecided).
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_coding_system_type(
    mgr: &CodingSystemManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("coding-system-type", &args, 1)?;
    let name = coding_symbol_name(&args[0])?;
    let resolved_name = resolve_runtime_name(mgr, &name)
        .ok_or_else(|| signal(LispCondition::CodingSystemError, vec![args[0]]))?;
    let base = strip_eol_suffix(&resolved_name);
    if let Some(kind) = coding_type_for_base(base) {
        return Ok(Value::symbol(kind));
    }
    let bucket = runtime_bucket_name(mgr, &resolved_name)
        .ok_or_else(|| signal(LispCondition::CodingSystemError, vec![args[0]]))?;
    let info = mgr
        .get(&bucket)
        .ok_or_else(|| signal(LispCondition::CodingSystemError, vec![args[0]]))?;
    Ok(Value::from_sym_id(info.coding_type))
}

#[derive(Clone, Copy, Debug)]
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
enum EolConversionRequest {
    Nil,
    Integer(i64),
    Float { value: Value, number: f64 },
    NonNumber(Value),
}

impl EolConversionRequest {
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn from_lisp_value(value: Value) -> Self {
        if value.is_nil() {
            return Self::Nil;
        }
        match value.kind() {
            ValueKind::Fixnum(n) => Self::Integer(n),
            ValueKind::Float => Self::Float {
                value,
                number: value.xfloat(),
            },
            ValueKind::Symbol(_) => EolType::from_specified_symbol_value(&value)
                .map(|eol| Self::Integer(eol.to_int()))
                .unwrap_or(Self::NonNumber(value)),
            _ => Self::NonNumber(value),
        }
    }

    fn vector_index(self) -> Result<i64, Flow> {
        match self {
            Self::Integer(index) => Ok(index),
            Self::Nil => Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("fixnump"), Value::NIL],
            )),
            Self::Float { value, .. } | Self::NonNumber(value) => Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("fixnump"), value],
            )),
        }
    }

    fn numeric_equals(self, rhs: i64) -> Result<bool, Flow> {
        match self {
            Self::Integer(value) => Ok(value == rhs),
            Self::Float { number, .. } => Ok(number == rhs as f64),
            Self::Nil => Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("number-or-marker-p"), Value::NIL],
            )),
            Self::NonNumber(value) => Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("number-or-marker-p"), value],
            )),
        }
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn eol_vector_ref(vector: Value, request: EolConversionRequest) -> EvalResult {
    let index = request.vector_index()?;
    let data = vector
        .as_vector_data()
        .expect("coding-system-eol-type vector value");
    if index < 0 || index as usize >= data.len() {
        return Err(signal(
            LispCondition::ArgsOutOfRange,
            vec![vector, Value::fixnum(index)],
        ));
    }
    Ok(data[index as usize])
}

/// `(coding-system-change-eol-conversion CODING-SYSTEM EOL-TYPE)` -- return
/// a coding system derived from CODING-SYSTEM but with a different EOL type.
/// EOL-TYPE is 0 (unix), 1 (dos), or 2 (mac), or a symbol.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_coding_system_change_eol_conversion(
    mgr: &CodingSystemManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("coding-system-change-eol-conversion", &args, 2)?;
    if args[0].is_string() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), args[0]],
        ));
    }
    let raw_name = coding_symbol_name(&args[0])?;
    if resolve_runtime_name(mgr, &raw_name).is_none() {
        return Err(signal(LispCondition::CodingSystemError, vec![args[0]]));
    }

    let request = EolConversionRequest::from_lisp_value(args[1]);
    let base = builtin_coding_system_base(mgr, vec![args[0]])?;
    let orig_eol_type = builtin_coding_system_eol_type(mgr, vec![args[0]])?;

    if matches!(
        orig_eol_type.kind(),
        ValueKind::Veclike(VecLikeType::Vector)
    ) {
        return match request {
            EolConversionRequest::Nil => Ok(args[0]),
            _ => eol_vector_ref(orig_eol_type, request),
        };
    }

    if matches!(request, EolConversionRequest::Nil) {
        return Ok(base);
    }

    let orig_eol = orig_eol_type
        .as_fixnum()
        .expect("coding-system-eol-type fixed value");
    if request.numeric_equals(orig_eol)? {
        return Ok(args[0]);
    }

    let base_eol_type = builtin_coding_system_eol_type(mgr, vec![base])?;
    if matches!(
        base_eol_type.kind(),
        ValueKind::Veclike(VecLikeType::Vector)
    ) {
        return eol_vector_ref(base_eol_type, request);
    }
    Ok(Value::NIL)
}

/// `(coding-system-change-text-conversion CODING-SYSTEM TEXT-CODING)` -- return
/// a coding system derived from TEXT-CODING but preserving the EOL type of
/// CODING-SYSTEM.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_coding_system_change_text_conversion(
    mgr: &CodingSystemManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("coding-system-change-text-conversion", &args, 2)?;
    let first_eol = match args[0].kind() {
        ValueKind::Nil => Some(0),
        ValueKind::String => None,
        _ => match args[0].as_symbol_name() {
            Some("nil") => Some(0),
            Some(name) => {
                if let Some(resolved) = resolve_runtime_name(mgr, name) {
                    if let Some(eol) = EolType::from_suffix(&resolved) {
                        Some(eol.to_int())
                    } else if let Some(info) = mgr.get(&resolved) {
                        info.eol_type.specified_index()
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            None => None,
        },
    };

    let text_raw = coding_symbol_name(&args[1])?;
    let text_name = if text_raw == "nil" {
        "undecided".to_string()
    } else {
        text_raw.clone()
    };
    let resolved_text = resolve_runtime_name(mgr, &text_name)
        .ok_or_else(|| signal(LispCondition::CodingSystemError, vec![args[1]]))?;
    let resolved_text_base = strip_eol_suffix(&resolved_text);

    if let Some(eol) = first_eol {
        if let Some(derived) = derive_coding_for_eol(resolved_text_base, eol) {
            return Ok(Value::symbol(derived));
        }
        return Ok(Value::NIL);
    }

    if EolType::from_suffix(&text_name).is_some() {
        return Ok(Value::symbol(display_base_name(strip_eol_suffix(
            &text_name,
        ))));
    }

    match text_name.as_str() {
        "binary" => Ok(Value::symbol("no-conversion")),
        "emacs-internal" => Ok(Value::symbol("utf-8-emacs")),
        _ => Ok(Value::symbol(text_name)),
    }
}

/// `(coding-system-p OBJECT)` -- return t when OBJECT names a known coding
/// system or alias, nil otherwise.
pub(crate) fn builtin_coding_system_p(mgr: &CodingSystemManager, args: Vec<Value>) -> EvalResult {
    expect_args("coding-system-p", &args, 1)?;
    let known = match args[0].as_symbol_name() {
        Some("nil") => true,
        Some(name) => is_known_or_derived_coding_system(mgr, name),
        None => false,
    };
    Ok(Value::bool_val(known))
}

/// `(check-coding-system CODING-SYSTEM)` -- validate CODING-SYSTEM.
/// Returns CODING-SYSTEM when valid, nil for nil, and signals on invalid input.
pub(crate) fn builtin_check_coding_system(
    mgr: &CodingSystemManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("check-coding-system", &args, 1)?;
    match args[0].kind() {
        ValueKind::Nil => Ok(Value::NIL),
        ValueKind::Symbol(id) => {
            if is_known_or_derived_coding_system(mgr, resolve_sym(id)) {
                Ok(args[0])
            } else {
                Err(signal(LispCondition::CodingSystemError, vec![args[0]]))
            }
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), args[0]],
        )),
    }
}

fn validate_coding_system_list_cars_for_non_ascii(
    mgr: &CodingSystemManager,
    coding_systems: Value,
) -> Result<(), Flow> {
    let mut tail = coding_systems;
    while tail.is_cons() {
        let coding_system = tail.cons_car();
        let Some(name) = coding_system.as_symbol_name() else {
            return Err(signal(
                LispCondition::CodingSystemError,
                vec![coding_system],
            ));
        };
        if !is_known_or_derived_coding_system(mgr, name) {
            return Err(signal(
                LispCondition::CodingSystemError,
                vec![coding_system],
            ));
        }
        tail = tail.cons_cdr();
    }
    Ok(())
}

/// The effective `:charset-list` symbols of a coding system (alias-resolved),
/// mirroring `CODING_ATTR_CHARSET_LIST` / what `coding-system-get :charset-list`
/// returns.  Used by the encodability scan of `check-coding-systems-region` and
/// `unencodable-char-position`.  Returns `None` for an unknown coding system.
fn coding_system_charset_list_syms(mgr: &CodingSystemManager, name: &str) -> Option<Vec<SymId>> {
    let resolved_name = resolve_runtime_name(mgr, name)?;
    let base = strip_eol_suffix(&resolved_name);
    if let Some(list) = coding_charset_list_for_base(base) {
        return Some(list.iter().filter_map(|v| v.as_symbol_id()).collect());
    }
    let bucket = runtime_bucket_name(mgr, &resolved_name)?;
    let info = mgr.get(&bucket)?;
    Some(info.charset_list.clone())
}

/// The concrete set of charsets through which a coding system can represent
/// characters.  GNU derives both `char_encodable_p` and ASCII compatibility
/// from `CODING_ATTR_CHARSET_LIST`; keeping them together prevents callers from
/// approximating repertoire with coding-system names or code-point ranges.
struct CodingRepertoire {
    charsets: Vec<SymId>,
    ascii_compatible: bool,
}

impl CodingRepertoire {
    fn for_coding_system(mgr: &CodingSystemManager, name: &str) -> Option<Self> {
        let charsets = coding_system_charset_list_syms(mgr, name)?;
        let ascii_compatible = charsets
            .iter()
            .any(|&charset| super::charset::charset_is_ascii_compatible(charset));
        Some(Self {
            charsets,
            ascii_compatible,
        })
    }

    /// GNU `char_encodable_p`: whether `ch` belongs to any charset in this
    /// coding system's effective charset list.
    fn encodes(&self, ch: i64) -> bool {
        self.charsets
            .iter()
            .any(|&charset| super::charset::charset_encode_char_bytes(charset, ch).is_some())
    }
}

/// Scan the multibyte Emacs bytes of `text` (a region/string already known to be
/// multibyte) and collect the 1-based positions (offset by `base_pos`) of every
/// character that `charset_list` cannot encode, mirroring GNU's per-character
/// `char_encodable_p` loop in `Fcheck_coding_systems_region` /
/// `Funencodable_char_position`.  ASCII characters are always encodable when the
/// coding is `ascii_compatible` and are skipped.  Stops after `limit` hits.
fn scan_unencodable_positions(
    text: &[u8],
    repertoire: &CodingRepertoire,
    base_pos: i64,
    limit: usize,
) -> Vec<i64> {
    let mut positions = Vec::new();
    let mut byte = 0usize;
    let mut char_index = 0i64;
    while byte < text.len() {
        let (code, len) = super::emacs_char::string_char(&text[byte..]);
        byte += len;
        let ch = i64::from(code);
        let is_ascii = code < 0x80;
        if !(repertoire.encodes(ch) || is_ascii && repertoire.ascii_compatible) {
            positions.push(base_pos + char_index);
            if positions.len() >= limit {
                break;
            }
        }
        char_index += 1;
    }
    positions
}

/// `(check-coding-systems-region START END CODING-SYSTEMS)` -- check whether
/// CODING-SYSTEMS can encode the region.  The ASCII/unibyte fast paths mirror
/// GNU `Fcheck_coding_systems_region`: they return nil before validating the
/// coding-system list.
pub(crate) fn builtin_check_coding_systems_region(
    eval: &mut Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("check-coding-systems-region", &args, 3)?;

    // Resolve the region/string text plus the 1-based base position GNU reports
    // (0 for a string START).  GNU returns nil early for unibyte / pure-ASCII
    // text *before* touching the coding-system list.
    let (text, base_pos) = if args[0].is_string() {
        let string = args[0]
            .as_lisp_string()
            .expect("string checked above")
            .clone();
        if !args[0].string_is_multibyte() || string.as_bytes().is_ascii() {
            return Ok(Value::NIL);
        }
        (string, 0i64)
    } else {
        let start = marker_or_integer_position(&args[0])?;
        let end = marker_or_integer_position(&args[1])?;

        let buffer = eval
            .buffers
            .current_buffer()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        let buffer_start = 1;
        let buffer_end = buffer.total_char_len().get() as i64 + 1;
        if !(buffer_start <= start && start <= end && end <= buffer_end) {
            return Err(signal(
                LispCondition::ArgsOutOfRange,
                vec![args[0], args[1]],
            ));
        }

        if !buffer.get_multibyte() {
            return Ok(Value::NIL);
        }
        let byte_range = EmacsByteRange::new(
            buffer.lisp_pos_to_full_buffer_emacs_byte_pos(LispCharPos1::new(start)),
            buffer.lisp_pos_to_full_buffer_emacs_byte_pos(LispCharPos1::new(end)),
        );
        let string = buffer.buffer_substring_lisp_string_range(byte_range);
        if string.as_bytes().is_ascii() {
            return Ok(Value::NIL);
        }
        (string, start)
    };

    // Validate the coding-system list (GNU `CODING_SYSTEM_SPEC` would signal
    // `coding-system-error` for a non-coding-system) before scanning.
    validate_coding_system_list_cars_for_non_ascii(&eval.coding_systems, args[2])?;

    // For each coding system, collect the positions of unencodable characters.
    // GNU builds the result preserving the input list order, with positions
    // ascending and only coding systems that have at least one unencodable
    // character included.
    let mut result_entries = Vec::new();
    let mut tail = args[2];
    while tail.is_cons() {
        let coding_system = tail.cons_car();
        tail = tail.cons_cdr();
        let Some(name) = coding_system.as_symbol_name() else {
            continue;
        };
        let Some(repertoire) = CodingRepertoire::for_coding_system(&eval.coding_systems, name)
        else {
            continue;
        };
        let positions =
            scan_unencodable_positions(text.as_bytes(), &repertoire, base_pos, usize::MAX);
        if !positions.is_empty() {
            let mut entry = vec![coding_system];
            entry.extend(positions.into_iter().map(Value::fixnum));
            result_entries.push(Value::list(entry));
        }
    }

    Ok(Value::list(result_entries))
}

/// `(unencodable-char-position START END CODING-SYSTEM &optional COUNT STRING)`
///
/// GNU `Funencodable_char_position` (src/coding.c): return the position of the
/// first character between START and END that CODING-SYSTEM cannot encode, or
/// nil if it encodes the whole region.  With COUNT non-nil, return a list of at
/// most COUNT such positions.  With STRING non-nil, START and END index STRING
/// (`substring`-style) and the returned positions are 0-based char indices.
pub(crate) fn builtin_unencodable_char_position(
    eval: &mut Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("unencodable-char-position", &args, 3)?;
    expect_max_args("unencodable-char-position", &args, 5)?;

    // GNU calls `setup_coding_system (Fcheck_coding_system (coding_system))`:
    // validate first (signalling on an unknown coding system), then short-circuit
    // for raw-text which encodes every byte verbatim.
    let coding_name = coding_symbol_name(&args[2])?;
    if !is_known_or_derived_coding_system(&eval.coding_systems, &coding_name) {
        return Err(signal(LispCondition::CodingSystemError, vec![args[2]]));
    }
    let resolved = resolve_runtime_name(&eval.coding_systems, &coding_name)
        .unwrap_or_else(|| coding_name.clone());
    if coding_type_for_base(strip_eol_suffix(&resolved)) == Some("raw-text") {
        return Ok(Value::NIL);
    }
    let Some(repertoire) = CodingRepertoire::for_coding_system(&eval.coding_systems, &coding_name)
    else {
        return Ok(Value::NIL);
    };

    let string_arg = args.get(4).copied().unwrap_or(Value::NIL);
    let count = args.get(3).copied().unwrap_or(Value::NIL);

    // COUNT nil => find one (and return it bare via car); else a fixnat limit.
    let (limit, return_list) = if count.is_nil() {
        (1usize, false)
    } else {
        match count.kind() {
            ValueKind::Fixnum(n) if n >= 0 => (n as usize, true),
            _ => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("natnump"), count],
                ));
            }
        }
    };

    let (text, base_pos) = if !string_arg.is_nil() {
        let string = eval.lisp_string(string_arg).ok_or_else(|| {
            signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("stringp"), string_arg],
            )
        })?;
        let size = string.schars() as i64;
        let (from, to) = substring_char_bounds(string_arg, args[0], args[1], size)?;
        if !string_arg.string_is_multibyte() {
            return Ok(Value::NIL);
        }
        // Slice the [from, to) char range out of the string's Emacs bytes.
        let bytes = string_char_range_bytes(string.as_bytes(), from as usize, to as usize);
        (bytes, from)
    } else {
        let start_raw = marker_or_integer_position(&args[0])?;
        let end_raw = marker_or_integer_position(&args[1])?;
        let (start, end) = if start_raw <= end_raw {
            (start_raw, end_raw)
        } else {
            (end_raw, start_raw)
        };
        let buffer = eval
            .buffers
            .current_buffer()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        let buffer_end = buffer.total_char_len().get() as i64 + 1;
        if !(1 <= start && start <= end && end <= buffer_end) {
            return Err(signal(
                LispCondition::ArgsOutOfRange,
                vec![args[0], args[1]],
            ));
        }
        if !buffer.get_multibyte() {
            return Ok(Value::NIL);
        }
        let byte_range = EmacsByteRange::new(
            buffer.lisp_pos_to_full_buffer_emacs_byte_pos(LispCharPos1::new(start)),
            buffer.lisp_pos_to_full_buffer_emacs_byte_pos(LispCharPos1::new(end)),
        );
        let string = buffer.buffer_substring_lisp_string_range(byte_range);
        // GNU returns nil for an ASCII-compatible coding when the region has no
        // multibyte characters (byte length == char length).
        if repertoire.ascii_compatible && string.as_bytes().is_ascii() {
            return Ok(Value::NIL);
        }
        (string.as_bytes().to_vec(), start)
    };

    let positions = scan_unencodable_positions(&text, &repertoire, base_pos, limit);

    if return_list {
        Ok(Value::list(
            positions.into_iter().map(Value::fixnum).collect(),
        ))
    } else {
        Ok(positions
            .first()
            .map(|p| Value::fixnum(*p))
            .unwrap_or(Value::NIL))
    }
}

/// `substring`-style bounds for a string of `size` chars: nil START => 0, nil END
/// => size, negative indices count from the end; signals `args-out-of-range` when
/// the result is not `0 <= from <= to <= size`.
fn substring_char_bounds(
    array: Value,
    from: Value,
    to: Value,
    size: i64,
) -> Result<(i64, i64), Flow> {
    fn normalize(value: Value, default: i64, size: i64) -> Result<i64, Flow> {
        if value.is_nil() {
            return Ok(default);
        }
        let raw = match value.kind() {
            ValueKind::Fixnum(n) => n,
            _ => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("integerp"), value],
                ));
            }
        };
        Ok(if raw < 0 { raw + size } else { raw })
    }
    let from_idx = normalize(from, 0, size)?;
    let to_idx = normalize(to, size, size)?;
    if !(0 <= from_idx && from_idx <= to_idx && to_idx <= size) {
        return Err(signal(LispCondition::ArgsOutOfRange, vec![array, from, to]));
    }
    Ok((from_idx, to_idx))
}

/// The Emacs multibyte bytes of the `[from_char, to_char)` char slice of `bytes`.
fn string_char_range_bytes(bytes: &[u8], from_char: usize, to_char: usize) -> Vec<u8> {
    let mut byte = 0usize;
    let mut char_index = 0usize;
    let mut start_byte = bytes.len();
    let mut end_byte = bytes.len();
    while byte < bytes.len() {
        if char_index == from_char {
            start_byte = byte;
        }
        if char_index == to_char {
            end_byte = byte;
            break;
        }
        let (_, len) = super::emacs_char::string_char(&bytes[byte..]);
        byte += len;
        char_index += 1;
    }
    if char_index <= from_char {
        start_byte = bytes.len();
    }
    bytes[start_byte.min(end_byte)..end_byte].to_vec()
}

/// `(find-coding-system CODING-SYSTEM)` -- resolve CODING-SYSTEM to a known
/// canonical symbol, or return nil when unknown.
#[cfg(test)]
pub(crate) fn builtin_find_coding_system(
    mgr: &CodingSystemManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("find-coding-system", &args, 1)?;
    let name = coding_system_name(&args[0])?;
    if name == "nil" {
        return Ok(Value::NIL);
    }
    match mgr.resolve(&name) {
        Some(canonical) => Ok(Value::symbol(canonical)),
        None => Ok(Value::NIL),
    }
}

/// `(define-coding-system-internal NAME MNEMONIC CODING-TYPE CHARSET-LIST
///    ASCII-COMPAT DECODE-TL ENCODE-TL POST-READ PRE-WRITE DEFAULT-CHAR
///    FOR-UNIBYTE PLIST EOL-TYPE &rest TYPE-SPECIFIC-ATTRS)`
///
/// Internal entry point for registering a coding system.
/// Called by the `define-coding-system` macro in mule.el with ≥13 positional args.
pub(crate) fn builtin_define_coding_system_internal(
    mgr: &mut CodingSystemManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("define-coding-system-internal", &args, 13)?;

    // arg[0]: name (symbol)
    let name = coding_symbol_name_required(&args[0])?;

    // arg[1]: mnemonic (char)
    let mnemonic = match args[1].kind() {
        ValueKind::Fixnum(c) => super::builtins::character_code_to_rust_char(c).unwrap_or('?'),
        _ => '?',
    };

    // arg[2]: coding-type (symbol)
    let coding_type = match args[2].kind() {
        ValueKind::Symbol(id) => id,
        _ => intern("undecided"),
    };

    // arg[3]: charset-list (list of symbols, or special symbol like 'iso-2022)
    let charset_list = match args[3].kind() {
        ValueKind::Symbol(id) => vec![id],
        _ => {
            if let Some(items) = super::value::list_to_vec(&args[3]) {
                items.iter().filter_map(|v| v.as_symbol_id()).collect()
            } else {
                Vec::new()
            }
        }
    };

    // arg[4]: ascii-compatible-p
    let ascii_compatible_p = args[4].is_truthy();

    // arg[5]: decode-translation-table (ignored for now)
    // arg[6]: encode-translation-table (ignored for now)

    // arg[7]: post-read-conversion (GNU `CHECK_SYMBOL`, coding.c:11083)
    let post_read_conversion = check_symbol_hook_arg(&args[7])?;

    // arg[8]: pre-write-conversion (GNU `CHECK_SYMBOL`, coding.c:11087)
    let pre_write_conversion = check_symbol_hook_arg(&args[8])?;

    // arg[9]: default-char
    let default_char = match args[9].kind() {
        ValueKind::Fixnum(c) => char::from_u32(c as u32),
        _ => None,
    };

    // arg[10]: for-unibyte
    let for_unibyte = args[10].is_truthy();

    // arg[11]: plist
    let mut properties = HashMap::new();
    if let Some(items) = super::value::list_to_vec(&args[11]) {
        let mut i = 0;
        while i + 1 < items.len() {
            if let Some(key) = items[i].as_symbol_id() {
                properties.insert(key, items[i + 1]);
            }
            i += 2;
        }
    }

    // arg[12]: eol-type (symbol: unix/dos/mac, nil, or vector for auto-detect)
    let eol_type = match args[12].kind() {
        ValueKind::Nil => EolType::Undecided,
        ValueKind::Symbol(_) => EolType::from_specified_symbol_value(&args[12])
            .ok_or_else(|| signal("error", vec![Value::string("Invalid eol-type")]))?,
        ValueKind::Veclike(VecLikeType::Vector) => EolType::Undecided,
        _ => return Err(signal("error", vec![Value::string("Invalid eol-type")])),
    };

    // Build the base coding system info.
    let mut info = CodingSystemInfo::new(&name, resolve_sym(coding_type), mnemonic, eol_type);
    info.ascii_compatible_p = ascii_compatible_p;
    info.charset_list = charset_list;
    info.post_read_conversion = post_read_conversion;
    info.pre_write_conversion = pre_write_conversion;
    info.default_char = default_char;
    info.for_unibyte = for_unibyte;
    info.properties = properties;

    // Stash the verbatim plist (arg 11) so coding-system-plist can reproduce
    // GNU's stored plist order exactly: GNU prepends `:ascii-compatible-p` and
    // `:category` onto this list (which mule.el already led with :name and
    // :docstring). See `builtin_coding_system_plist`.
    if args.len() > 11 {
        info.int_properties.insert(PLIST_VERBATIM_KEY, args[11]);
    }

    // arg[13..17] (iso-2022 only): [initial-designation-vector, reg-usage,
    // request-alist, flags-bitmask].  Stash them so the ISO-2022 codec can read
    // the G0-G3 designations; see `iso2022_spec`.
    if resolve_sym(coding_type) == "iso-2022" && args.len() > 16 {
        info.int_properties.insert(ISO2022_KEY_INITIAL, args[13]);
        info.int_properties.insert(ISO2022_KEY_REG_USAGE, args[14]);
        info.int_properties.insert(ISO2022_KEY_REQUEST, args[15]);
        info.int_properties.insert(ISO2022_KEY_FLAGS, args[16]);
    }

    // arg[13..16] (CCL only): decoder program, encoder program, and the
    // optional 256-entry byte-validity table. Keep the raw Lisp designators in
    // pdump-backed internal slots and expose them through `CclCodingSpec`.
    if resolve_sym(coding_type) == "ccl" && args.len() > 15 {
        info.int_properties.insert(CCL_KEY_DECODER, args[13]);
        info.int_properties.insert(CCL_KEY_ENCODER, args[14]);
        info.int_properties.insert(CCL_KEY_VALIDS, args[15]);
    }

    // Register the base coding system.
    mgr.register(info);

    // Auto-create EOL variants (-unix, -dos, -mac) unless the eol_type
    // is already specific or the name already has an EOL suffix.
    if matches!(eol_type, EolType::Undecided) && EolType::from_suffix(&name).is_none() {
        for (suffix, et) in [
            ("-unix", EolType::Unix),
            ("-dos", EolType::Dos),
            ("-mac", EolType::Mac),
        ] {
            let variant_name = format!("{name}{suffix}");
            if !mgr.is_known(&variant_name) {
                let variant =
                    CodingSystemInfo::new(&variant_name, resolve_sym(coding_type), mnemonic, et);
                mgr.register(variant);
            }
        }
    }

    Ok(Value::NIL)
}

/// Extract a coding system name from a symbol argument, signaling on non-symbol.
fn coding_symbol_name_required(val: &Value) -> Result<String, Flow> {
    match val.kind() {
        ValueKind::Symbol(id) => Ok(resolve_sym(id).to_owned()),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), *val],
        )),
    }
}

/// `(define-coding-system-alias ALIAS CODING-SYSTEM)` -- register ALIAS for
/// CODING-SYSTEM and return nil.
pub(crate) fn builtin_define_coding_system_alias(
    mgr: &mut CodingSystemManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("define-coding-system-alias", &args, 2)?;

    let alias = match args[0].kind() {
        ValueKind::Symbol(id) => resolve_sym(id).to_owned(),
        ValueKind::Nil => "nil".to_string(),
        _ => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("symbolp"), args[0]],
            ));
        }
    };

    let target = match args[1].kind() {
        ValueKind::Symbol(id) => resolve_sym(id).to_owned(),
        ValueKind::Nil => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("coding-system-p"), Value::NIL],
            ));
        }
        _ => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("symbolp"), args[1]],
            ));
        }
    };

    let canonical = mgr.resolve(&target).ok_or_else(|| {
        signal(
            LispCondition::CodingSystemError,
            vec![Value::symbol(&target)],
        )
    })?;
    mgr.add_alias(&alias, resolve_sym(canonical));
    Ok(Value::NIL)
}

/// `(set-coding-system-priority &rest CODING-SYSTEMS)` -- assign higher
/// priority to the categories of CODING-SYSTEMS, in order.  Mirrors GNU's
/// `Fset_coding_system_priority` (coding.c): the priority list has one entry
/// per detection *category*; the named coding systems' categories move to the
/// front (the first system seen for a category wins, later ones of the same
/// category are ignored), and the remaining categories keep their prior order.
/// A category is also rebound to the named coding system (GNU `setup_coding_system`).
pub(crate) fn builtin_set_coding_system_priority(
    mgr: &mut CodingSystemManager,
    args: Vec<Value>,
) -> EvalResult {
    if args.is_empty() {
        return Ok(Value::NIL);
    }

    // Validate and resolve each argument to its category + the coding system
    // name to bind to that category (GNU `CHECK_CODING_SYSTEM_GET_SPEC`).
    let mut requested: Vec<(&'static str, SymId)> = Vec::with_capacity(args.len());
    for arg in &args {
        if arg.is_nil() {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("coding-system-p"), Value::NIL],
            ));
        }
        let Some(name) = arg.as_symbol_name().map(|s| s.to_string()) else {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("symbolp"), *arg],
            ));
        };
        // Resolve through aliases / EOL variants; GNU prefers the base name.
        resolve_runtime_name(mgr, &name)
            .ok_or_else(|| signal(LispCondition::CodingSystemError, vec![*arg]))?;
        let category = coding_category_of(mgr, &name)
            .ok_or_else(|| signal(LispCondition::CodingSystemError, vec![*arg]))?;
        // GNU stores the coding system's *base* name in the priority list.
        let base = coding_system_base_name(mgr, &name);
        requested.push((category, intern(&base)));
    }

    // Determine, per priority entry, the category it currently occupies.
    let entry_categories: Vec<Option<&'static str>> = mgr
        .priority
        .iter()
        .map(|&sym| coding_category_of(mgr, resolve_sym(sym)))
        .collect();

    // Front part: requested categories, first occurrence wins (GNU `changed[]`).
    let mut fronted: HashSet<&'static str> = HashSet::with_capacity(requested.len());
    let mut reordered: Vec<SymId> = Vec::with_capacity(mgr.priority.len());
    for (category, bound) in &requested {
        if fronted.insert(category) {
            // GNU rebinds the category to the named system.
            reordered.push(*bound);
        }
    }

    // Tail: remaining priority entries in their prior order, skipping the ones
    // whose category was just fronted.
    for (idx, &sym) in mgr.priority.iter().enumerate() {
        match entry_categories[idx] {
            Some(cat) if fronted.contains(cat) => {}
            _ => reordered.push(sym),
        }
    }

    mgr.priority = reordered;
    Ok(Value::NIL)
}

/// Return the base (no-EOL) coding-system name that GNU would store in the
/// priority list for `name`, resolving aliases and EOL variants.
fn coding_system_base_name(mgr: &CodingSystemManager, name: &str) -> String {
    let resolved = resolve_runtime_name(mgr, name)
        .unwrap_or_else(|| normalize_coding_name_for_lookup(name).to_string());
    let base = strip_eol_suffix(&resolved);
    mgr.resolve(base)
        .map(|id| resolve_sym(id).to_string())
        .unwrap_or_else(|| base.to_string())
}

// ===========================================================================
// Coding-system detection (port of GNU coding.c `detect_coding_system` and the
// per-category detectors, for unibyte source bytes).
//
// `detect-coding-string`/`detect-coding-region` return the list of coding
// systems (one per detection category) that *could* have produced the bytes,
// ordered by priority.  The category result for given bytes is computed by
// running each category's byte-level detector and collecting found/rejected
// bits, exactly as GNU does.  See coding.c:8690 `detect_coding_system`.
// ===========================================================================

/// The 21 detection categories, in `enum coding_category` order (coding.c:476).
#[derive(Clone, Copy, PartialEq, Eq)]
enum CodingCat {
    Iso7,
    Iso7Tight,
    Iso81,
    Iso82,
    Iso7Else,
    Iso8Else,
    Utf8Auto,
    Utf8Nosig,
    Utf8Sig,
    Utf16Auto,
    Utf16Be,
    Utf16Le,
    Utf16BeNosig,
    Utf16LeNosig,
    Charset,
    Sjis,
    Big5,
    Ccl,
    EmacsMule,
    RawText,
    Undecided,
}

const CODING_CAT_MAX: usize = 21;
const CODING_CAT_RAW_TEXT: usize = CodingCat::RawText as usize;

/// Map a category symbol name (`:category` value) to the enum index.
fn coding_cat_index(category: &str) -> Option<usize> {
    Some(match category {
        "coding-category-iso-7" => CodingCat::Iso7 as usize,
        "coding-category-iso-7-tight" => CodingCat::Iso7Tight as usize,
        "coding-category-iso-8-1" => CodingCat::Iso81 as usize,
        "coding-category-iso-8-2" => CodingCat::Iso82 as usize,
        "coding-category-iso-7-else" => CodingCat::Iso7Else as usize,
        "coding-category-iso-8-else" => CodingCat::Iso8Else as usize,
        "coding-category-utf-8-auto" => CodingCat::Utf8Auto as usize,
        "coding-category-utf-8" => CodingCat::Utf8Nosig as usize,
        "coding-category-utf-8-sig" => CodingCat::Utf8Sig as usize,
        "coding-category-utf-16-auto" => CodingCat::Utf16Auto as usize,
        "coding-category-utf-16-be" => CodingCat::Utf16Be as usize,
        "coding-category-utf-16-le" => CodingCat::Utf16Le as usize,
        "coding-category-utf-16-be-nosig" => CodingCat::Utf16BeNosig as usize,
        "coding-category-utf-16-le-nosig" => CodingCat::Utf16LeNosig as usize,
        "coding-category-charset" => CodingCat::Charset as usize,
        "coding-category-sjis" => CodingCat::Sjis as usize,
        "coding-category-big5" => CodingCat::Big5 as usize,
        "coding-category-ccl" => CodingCat::Ccl as usize,
        "coding-category-emacs-mule" => CodingCat::EmacsMule as usize,
        "coding-category-raw-text" => CodingCat::RawText as usize,
        "coding-category-undecided" => CodingCat::Undecided as usize,
        _ => return None,
    })
}

/// Map a category enum index back to its category symbol name (inverse of
/// `coding_cat_index`).  Used to build `coding-category-list`.
fn coding_cat_name(cat: usize) -> Option<&'static str> {
    Some(match cat {
        x if x == CodingCat::Iso7 as usize => "coding-category-iso-7",
        x if x == CodingCat::Iso7Tight as usize => "coding-category-iso-7-tight",
        x if x == CodingCat::Iso81 as usize => "coding-category-iso-8-1",
        x if x == CodingCat::Iso82 as usize => "coding-category-iso-8-2",
        x if x == CodingCat::Iso7Else as usize => "coding-category-iso-7-else",
        x if x == CodingCat::Iso8Else as usize => "coding-category-iso-8-else",
        x if x == CodingCat::Utf8Auto as usize => "coding-category-utf-8-auto",
        x if x == CodingCat::Utf8Nosig as usize => "coding-category-utf-8",
        x if x == CodingCat::Utf8Sig as usize => "coding-category-utf-8-sig",
        x if x == CodingCat::Utf16Auto as usize => "coding-category-utf-16-auto",
        x if x == CodingCat::Utf16Be as usize => "coding-category-utf-16-be",
        x if x == CodingCat::Utf16Le as usize => "coding-category-utf-16-le",
        x if x == CodingCat::Utf16BeNosig as usize => "coding-category-utf-16-be-nosig",
        x if x == CodingCat::Utf16LeNosig as usize => "coding-category-utf-16-le-nosig",
        x if x == CodingCat::Charset as usize => "coding-category-charset",
        x if x == CodingCat::Sjis as usize => "coding-category-sjis",
        x if x == CodingCat::Big5 as usize => "coding-category-big5",
        x if x == CodingCat::Ccl as usize => "coding-category-ccl",
        x if x == CodingCat::EmacsMule as usize => "coding-category-emacs-mule",
        x if x == CodingCat::RawText as usize => "coding-category-raw-text",
        x if x == CodingCat::Undecided as usize => "coding-category-undecided",
        _ => return None,
    })
}

/// Build the value of the `coding-category-list` variable: all detection
/// categories (one per `enum coding_category`) ordered by the current
/// detection priority, mirroring GNU's `Vcoding_category_list`
/// (coding.c `Fset_coding_system_priority`).  The priority list stores coding
/// systems; we map each to its category (first occurrence wins) and append any
/// categories not represented, in enum order, so the result always covers all
/// `CODING_CAT_MAX` categories like GNU's fixed-size `coding_priorities` array.
pub(crate) fn coding_category_priority_list(mgr: &CodingSystemManager) -> Vec<SymId> {
    let mut order: Vec<usize> = Vec::with_capacity(CODING_CAT_MAX);
    for &sym in &mgr.priority {
        if let Some(cat) = coding_category_of(mgr, resolve_sym(sym)).and_then(coding_cat_index)
            && !order.contains(&cat)
        {
            order.push(cat);
        }
    }
    insert_unbound_categories(&mut order);
    order
        .into_iter()
        .filter_map(|cat| coding_cat_name(cat).map(intern))
        .collect()
}

/// Insert every detection category missing from `order` (those with no bound
/// coding system, e.g. `coding-category-ccl`) so that `order` covers all
/// `CODING_CAT_MAX` categories.  GNU's fixed-size `coding_priorities` array
/// always carries every category (Fset_coding_system_priority); categories not
/// fronted by `set-coding-system-priority` keep their prior — i.e. `enum
/// coding_category` — order, forming an ascending run at the tail.  Insert each
/// missing category into that ascending tail just after the last entry with a
/// smaller enum index (scanning from the right so the fronted, non-ascending
/// prefix never captures it).  For the lone unbound `ccl` (index 17) this
/// yields GNU's tail `... big5 ccl undecided`.
fn insert_unbound_categories(order: &mut Vec<usize>) {
    for cat in 0..CODING_CAT_MAX {
        if order.contains(&cat) {
            continue;
        }
        let pos = order.iter().rposition(|&c| c < cat).map_or(0, |i| i + 1);
        order.insert(pos, cat);
    }
}

// Category bit-mask helpers (coding.c:504).
const fn cat_mask(c: CodingCat) -> u32 {
    1 << (c as u32)
}
const MASK_UTF_8: u32 =
    cat_mask(CodingCat::Utf8Auto) | cat_mask(CodingCat::Utf8Nosig) | cat_mask(CodingCat::Utf8Sig);
const MASK_UTF_16: u32 = cat_mask(CodingCat::Utf16Auto)
    | cat_mask(CodingCat::Utf16Be)
    | cat_mask(CodingCat::Utf16Le)
    | cat_mask(CodingCat::Utf16BeNosig)
    | cat_mask(CodingCat::Utf16LeNosig);
const MASK_ISO_7BIT: u32 = cat_mask(CodingCat::Iso7) | cat_mask(CodingCat::Iso7Tight);
const MASK_ISO_8BIT: u32 = cat_mask(CodingCat::Iso81) | cat_mask(CodingCat::Iso82);
const MASK_ISO_ELSE: u32 = cat_mask(CodingCat::Iso7Else) | cat_mask(CodingCat::Iso8Else);
const MASK_ISO: u32 = MASK_ISO_7BIT | MASK_ISO_8BIT | MASK_ISO_ELSE;
const MASK_ANY: u32 = MASK_ISO
    | MASK_UTF_8
    | MASK_UTF_16
    | cat_mask(CodingCat::Charset)
    | cat_mask(CodingCat::Sjis)
    | cat_mask(CodingCat::Big5)
    | cat_mask(CodingCat::Ccl)
    | cat_mask(CodingCat::EmacsMule);

/// Bookkeeping for the detectors (coding.c `struct coding_detection_info`).
#[derive(Default, Clone, Copy)]
struct DetectInfo {
    checked: u32,
    found: u32,
    rejected: u32,
}

/// `latin-extra-code-table` entry test (mule-conf.el sets 0x91-0x96).
fn latin_extra_code_p(c: u8) -> bool {
    matches!(c, 0x91..=0x96)
}

/// `emacs_mule_bytes[c]`: bytes consumed by an emacs-mule leading code
/// (coding.c:11725 + charset.c:1183).  Built from the charset registry's
/// emacs-mule ids; default 1.
fn emacs_mule_bytes(c: u8) -> i32 {
    // Private composition leading codes (charset.h:538).
    match c {
        0x9A | 0x9B => return 3,
        0x9C | 0x9D => return 4,
        _ => {}
    }
    super::charset::emacs_mule_leading_code_bytes(c).unwrap_or(1)
}

/// A source cursor implementing GNU's `ONE_MORE_BYTE` semantics (coding.c:633).
/// In unibyte mode every byte is yielded verbatim.  In multibyte mode an
/// eight-bit raw byte (stored as a 0xC0/0xC1 2-byte lead) is yielded as its
/// 0x80-0xFF value, and any other multibyte character is yielded *negated*
/// (the detectors treat negative codes specially).
struct DetectSrc<'a> {
    bytes: &'a [u8],
    pos: usize,
    multibytep: bool,
}

impl<'a> DetectSrc<'a> {
    fn new(bytes: &'a [u8], pos: usize, multibytep: bool) -> Self {
        Self {
            bytes,
            pos,
            multibytep,
        }
    }

    /// `ONE_MORE_BYTE`: return the next code, or `None` at end of source.
    fn next(&mut self) -> Option<i32> {
        if self.pos >= self.bytes.len() {
            return None;
        }
        let c = self.bytes[self.pos];
        self.pos += 1;
        if self.multibytep && (c & 0x80) != 0 {
            if (c & 0xFE) == 0xC0 {
                // Eight-bit raw byte: ((c & 1) << 6) | next.
                let next = self.bytes.get(self.pos).copied().unwrap_or(0);
                self.pos += 1;
                return Some(i32::from(((c & 1) << 6) | next));
            }
            // A genuine multibyte character: decode and negate.
            self.pos -= 1;
            let ch = crate::emacs_core::emacs_char::string_char_advance(self.bytes, &mut self.pos);
            return Some(-(ch as i32));
        }
        Some(i32::from(c))
    }

    /// Peek the raw byte at the current position without consuming it.  Only
    /// meaningful for the unibyte fast paths that inspect the next raw byte
    /// (e.g. CR/LF and the ISO-8-2 run length).
    fn peek_raw(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn at_end(&self) -> bool {
        self.pos >= self.bytes.len()
    }
}

/// GNU's `CODING_MODE_LAST_BLOCK` (src/coding.h:264-267) as a required
/// argument of detection: "the decoding/encoding routines treat the current
/// data as the last block of the whole text to be converted".
///
/// It is the one thing the bytes cannot tell a detector.  Four of GNU's
/// detectors end on `if (src_base < src && coding->mode & CODING_MODE_LAST_BLOCK)`
/// -- UTF-8 at src/coding.c:1215, `emacs-mule` at :1910, Shift-JIS at :4620 and
/// Big5 at :4667 -- and the conjunct is the whole difference between "these
/// bytes are not this coding system" and "this chunk stopped in the middle of a
/// character and the rest is coming".  Dropping it, which is what this port did
/// until DIVERGENCES.md entry 151, is invisible for a string or a region and
/// wrong for a subprocess, whose reads split wherever the kernel split them:
/// measured under GNU 31.0.90, a child writing `caf <c3>` and then `<a9> CR LF`
/// detects `utf-8` and holds the `<c3>` back, where
/// `(decode-coding-string "caf\303" 'undecided)` on the very same bytes answers
/// `iso-latin-1`.
///
/// A FIFTH detector reads the flag and does not end on that line:
/// `detect_coding_utf_16` spends it at the TOP of its body, on an odd byte count
/// (:1505-1511).  And a file is NOT a `Last` block, which entry 151's version of
/// this comment got wrong: `insert-file-contents` reaches `decode_coding_gap`,
/// which calls `detect_coding` at :7927-7928 and raises the flag only at :8009.
/// See DIVERGENCES.md entry 156 and `CodingEntry::detection_block`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SourceBlock {
    /// More bytes may follow: a subprocess read that is not the EOF read.  A
    /// trailing partial character is CARRYOVER, not evidence.
    More,
    /// The source is complete -- a string, a region, `call-process` output
    /// after the child exited, or a process at EOF -- so a trailing partial
    /// character really is malformed.  A FILE is not one of these; see
    /// `CodingEntry::detection_block`.
    Last,
}

impl SourceBlock {
    /// GNU's `coding->mode & CODING_MODE_LAST_BLOCK` as a bool, spelled out at
    /// the sites that test it so the C reads the same as the Rust.
    fn is_last(self) -> bool {
        matches!(self, Self::Last)
    }
}

/// Port of coding.c `detect_coding_utf_8`.
fn detect_utf_8(
    bytes: &[u8],
    head_ascii: usize,
    multibytep: bool,
    block: SourceBlock,
    di: &mut DetectInfo,
) -> bool {
    di.checked |= MASK_UTF_8;
    let nbytes = bytes.len();
    let mut nchars = head_ascii;
    let mut bom_found = false;
    let mut start = head_ascii;
    if start == 0 && start + 3 < nbytes && bytes[0] == 0xEF && bytes[1] == 0xBB && bytes[2] == 0xBF
    {
        bom_found = true;
        start += 3;
        nchars += 1;
    }
    let mut src = DetectSrc::new(bytes, start, multibytep);

    loop {
        let src_base = src.pos;
        let Some(c) = src.next() else {
            return detect_utf_8_no_more(nbytes, src_base, src.pos, bom_found, nchars, block, di);
        };
        if c < 0 || c < 0x80 {
            nchars += 1;
            if c == i32::from(b'\r') && src.peek_raw() == Some(b'\n') {
                src.pos += 1;
                nchars += 1;
            }
            continue;
        }
        // c is a positive 8-bit lead; read continuation octets as raw bytes.
        let c = c as u8;
        let Some(c1) = src.next() else {
            return detect_utf_8_no_more(nbytes, src_base, src.pos, bom_found, nchars, block, di);
        };
        if c1 < 0 || (c1 as u8 & 0xC0) != 0x80 {
            di.rejected |= MASK_UTF_8;
            return false;
        }
        if (c & 0xE0) == 0xC0 {
            nchars += 1;
            continue;
        }
        let Some(c2) = src.next() else {
            return detect_utf_8_no_more(nbytes, src_base, src.pos, bom_found, nchars, block, di);
        };
        if c2 < 0 || (c2 as u8 & 0xC0) != 0x80 {
            di.rejected |= MASK_UTF_8;
            return false;
        }
        if (c & 0xF0) == 0xE0 {
            nchars += 1;
            continue;
        }
        let Some(c3) = src.next() else {
            return detect_utf_8_no_more(nbytes, src_base, src.pos, bom_found, nchars, block, di);
        };
        if c3 < 0 || (c3 as u8 & 0xC0) != 0x80 {
            di.rejected |= MASK_UTF_8;
            return false;
        }
        if (c & 0xF8) == 0xF0 {
            nchars += 1;
            continue;
        }
        let Some(c4) = src.next() else {
            return detect_utf_8_no_more(nbytes, src_base, src.pos, bom_found, nchars, block, di);
        };
        if c4 < 0 || (c4 as u8 & 0xC0) != 0x80 {
            di.rejected |= MASK_UTF_8;
            return false;
        }
        // 5-octet leads are above MAX_MULTIBYTE_LEADING_CODE -> reject.
        di.rejected |= MASK_UTF_8;
        return false;
    }
}

/// The coding.c `no_more_source:` tail of `detect_coding_utf_8`, reached when
/// the source ends mid-character (`src_base < src`).
fn detect_utf_8_no_more(
    nbytes: usize,
    src_base: usize,
    src: usize,
    bom_found: bool,
    nchars: usize,
    block: SourceBlock,
    di: &mut DetectInfo,
) -> bool {
    // `if (src_base < src && coding->mode & CODING_MODE_LAST_BLOCK)`, :1215.
    if src_base < src && block.is_last() {
        di.rejected |= MASK_UTF_8;
        return false;
    }
    if bom_found {
        di.found |= MASK_UTF_8;
    } else {
        di.rejected |= cat_mask(CodingCat::Utf8Sig);
        if nchars < nbytes {
            di.found |= cat_mask(CodingCat::Utf8Auto) | cat_mask(CodingCat::Utf8Nosig);
        }
    }
    true
}

/// Port of coding.c `detect_coding_utf_16` (src/coding.c:1494).  Operates on raw
/// bytes (the macro `TWO_MORE_BYTES` skips heading multibyte characters, but
/// UTF-16 detection inspects the literal byte pairs).
///
/// BLOCK is `CODING_MODE_LAST_BLOCK`, and it is a REQUIRED argument for the same
/// reason it is required by the five detectors that spend it at
/// `no_more_source:`.  This one spends it at the TOP instead:
///
/// ```c
///   if (coding->mode & CODING_MODE_LAST_BLOCK
///       && (coding->src_chars & 1))
///     { detect_info->rejected |= CATEGORY_MASK_UTF_16; return 0; }
/// ```
///
/// (src/coding.c:1505-1511.)  That different position is why DIVERGENCES.md
/// entry 151 -- which found the conjunct missing at the `no_more_source:` sites
/// and fixed all five -- did not find it here.  An odd byte count refutes UTF-16
/// only when the source is COMPLETE; in a subprocess read it is an ordinary
/// chunk boundary.  Measured under GNU Emacs 31.0.90 on the five bytes
/// `FF FE 61 00 0D`: `(decode-coding-string ... 'undecided)` answers
/// `no-conversion` and keeps all five bytes, while a pipe delivering the same
/// five bytes answers `utf-16le-with-signature-mac` and produces `(97 10)`.
fn detect_utf_16(bytes: &[u8], src_chars: usize, block: SourceBlock, di: &mut DetectInfo) -> bool {
    di.checked |= MASK_UTF_16;
    if block.is_last() && src_chars & 1 != 0 {
        di.rejected |= MASK_UTF_16;
        return false;
    }
    if bytes.len() < 2 {
        // TWO_MORE_BYTES would hit no_more_source.
        return true;
    }
    let c1 = bytes[0];
    let c2 = bytes[1];
    if c1 == 0xFF && c2 == 0xFE {
        di.found |= cat_mask(CodingCat::Utf16Le) | cat_mask(CodingCat::Utf16Auto);
        di.rejected |= cat_mask(CodingCat::Utf16Be)
            | cat_mask(CodingCat::Utf16BeNosig)
            | cat_mask(CodingCat::Utf16LeNosig);
        return true;
    } else if c1 == 0xFE && c2 == 0xFF {
        di.found |= cat_mask(CodingCat::Utf16Be) | cat_mask(CodingCat::Utf16Auto);
        di.rejected |= cat_mask(CodingCat::Utf16Le)
            | cat_mask(CodingCat::Utf16BeNosig)
            | cat_mask(CodingCat::Utf16LeNosig);
        return true;
    }
    // Dispersion heuristic for the no-signature variants.
    let mut e = [false; 256];
    let mut o = [false; 256];
    let mut e_num = 1u32;
    let mut o_num = 1u32;
    e[c1 as usize] = true;
    o[c2 as usize] = true;
    di.rejected |= cat_mask(CodingCat::Utf16Auto)
        | cat_mask(CodingCat::Utf16Be)
        | cat_mask(CodingCat::Utf16Le);
    let mut i = 2;
    while (di.rejected & MASK_UTF_16) != MASK_UTF_16 {
        if i + 1 >= bytes.len() {
            break;
        }
        let c1 = bytes[i] as usize;
        let c2 = bytes[i + 1] as usize;
        i += 2;
        if !e[c1] {
            e[c1] = true;
            e_num += 1;
            if e_num >= 128 {
                di.rejected |= cat_mask(CodingCat::Utf16BeNosig);
            }
        }
        if !o[c2] {
            o[c2] = true;
            o_num += 1;
            if o_num >= 128 {
                di.rejected |= cat_mask(CodingCat::Utf16LeNosig);
            }
        }
    }
    true
}

/// The `no_more_source:` tail that `detect_coding_emacs_mule` (src/coding.c:1908),
/// `detect_coding_sjis` (:4618) and `detect_coding_big5` (:4665) share, spelled
/// ONCE.
///
/// In C the sharing is free: every `ONE_MORE_BYTE` in those loops is a `goto
/// no_more_source`, so the LAST_BLOCK conjunct guards the LEAD byte's
/// exhaustion and the TRAIL byte's alike.  Ported by hand it was not free, and
/// DIVERGENCES.md entry 151 -- which added the conjunct to these detectors --
/// added it only at the lead-byte site of each.  A source ending on a lone
/// valid lead byte therefore came back "this is Big5" where GNU says "this is
/// not", measured under GNU Emacs 31.0.90 on `hello caf <c3> <a9> world caf <c3>`:
///
/// ```elisp
/// (detect-coding-string "hello caf\303\251 world caf\303")
/// ;; GNU     => (iso-latin-1 emacs-mule in-is13194-devanagari chinese-iso-8bit
/// ;;             japanese-shift-jis iso-2022-8bit-ss2)
/// ;; Neomacs => (... japanese-shift-jis chinese-big5 iso-2022-8bit-ss2)
/// ```
///
/// SRC_BASE is GNU's `src_base`, assigned once per LOOP ITERATION at the top --
/// not once per byte read -- which is what makes `src_base < src` mean "the
/// source ran out in the middle of a character" at every site alike.
fn detector_no_more_source(
    cat: CodingCat,
    src_base: usize,
    src_pos: usize,
    found: u32,
    block: SourceBlock,
    di: &mut DetectInfo,
) -> bool {
    if src_base < src_pos && block.is_last() {
        di.rejected |= cat_mask(cat);
        return false;
    }
    di.found |= found;
    true
}

/// Port of coding.c `detect_coding_emacs_mule`.
fn detect_emacs_mule(
    bytes: &[u8],
    head_ascii: usize,
    multibytep: bool,
    block: SourceBlock,
    di: &mut DetectInfo,
) -> bool {
    di.checked |= cat_mask(CodingCat::EmacsMule);
    let mut src = DetectSrc::new(bytes, head_ascii, multibytep);
    let mut found = 0u32;
    loop {
        let src_base = src.pos;
        let Some(mut c) = src.next() else {
            return detector_no_more_source(
                CodingCat::EmacsMule,
                src_base,
                src.pos,
                found,
                block,
                di,
            );
        };
        if c < 0 {
            continue;
        }
        if c == 0x80 {
            // Perhaps the start of a composite character.
            loop {
                let src_start = src.pos;
                loop {
                    match src.next() {
                        None => {
                            return detector_no_more_source(
                                CodingCat::EmacsMule,
                                src_base,
                                src.pos,
                                found,
                                block,
                                di,
                            );
                        }
                        Some(v) => {
                            c = v;
                            if c < 0xA0 {
                                break;
                            }
                        }
                    }
                }
                if src.pos - 1 - src_start <= 4 {
                    di.rejected |= cat_mask(CodingCat::EmacsMule);
                    return false;
                }
                found = cat_mask(CodingCat::EmacsMule);
                if c == 0x80 {
                    continue;
                }
                break;
            }
        }
        if c < 0x80 {
            if c < 0x20 && (c == 0x1B || c == 0x0F || c == 0x0E) {
                di.rejected |= cat_mask(CodingCat::EmacsMule);
                return false;
            }
        } else {
            let mut more_bytes = emacs_mule_bytes(c as u8) - 1;
            while more_bytes > 0 {
                let before = src.pos;
                match src.next() {
                    None => {
                        // GNU's `ONE_MORE_BYTE` jumps to `no_more_source` here.
                        return detector_no_more_source(
                            CodingCat::EmacsMule,
                            src_base,
                            src.pos,
                            found,
                            block,
                            di,
                        );
                    }
                    Some(v) => {
                        c = v;
                        if c < 0xA0 {
                            src.pos = before; // unread the last byte
                            break;
                        }
                    }
                }
                more_bytes -= 1;
            }
            if more_bytes != 0 {
                di.rejected |= cat_mask(CodingCat::EmacsMule);
                return false;
            }
            found = cat_mask(CodingCat::EmacsMule);
        }
    }
}

/// Port of coding.c `detect_coding_sjis` (japanese-shift-jis has 2 charsets, so
/// the max first byte of a 2-byte code is 0xEF).
fn detect_sjis(
    bytes: &[u8],
    head_ascii: usize,
    multibytep: bool,
    block: SourceBlock,
    di: &mut DetectInfo,
) -> bool {
    di.checked |= cat_mask(CodingCat::Sjis);
    let max_first = 0xEF;
    let mut src = DetectSrc::new(bytes, head_ascii, multibytep);
    let mut found = 0u32;
    loop {
        let src_base = src.pos;
        let Some(c) = src.next() else {
            return detector_no_more_source(CodingCat::Sjis, src_base, src.pos, found, block, di);
        };
        if c < 0x80 {
            continue;
        }
        let c = c as u32;
        if (0x81..=0x9F).contains(&c) || (0xE0..=max_first).contains(&c) {
            let Some(c) = src.next() else {
                return detector_no_more_source(
                    CodingCat::Sjis,
                    src_base,
                    src.pos,
                    found,
                    block,
                    di,
                );
            };
            if c < 0x40 || c == 0x7F || c > 0xFC {
                di.rejected |= cat_mask(CodingCat::Sjis);
                return false;
            }
            found = cat_mask(CodingCat::Sjis);
        } else if (0xA0..0xE0).contains(&c) {
            found = cat_mask(CodingCat::Sjis);
        } else {
            di.rejected |= cat_mask(CodingCat::Sjis);
            return false;
        }
    }
}

/// Port of coding.c `detect_coding_big5`.
fn detect_big5(
    bytes: &[u8],
    head_ascii: usize,
    multibytep: bool,
    block: SourceBlock,
    di: &mut DetectInfo,
) -> bool {
    di.checked |= cat_mask(CodingCat::Big5);
    let mut src = DetectSrc::new(bytes, head_ascii, multibytep);
    let mut found = 0u32;
    loop {
        let src_base = src.pos;
        let Some(c) = src.next() else {
            return detector_no_more_source(CodingCat::Big5, src_base, src.pos, found, block, di);
        };
        if c < 0x80 {
            continue;
        }
        if c >= 0xA1 {
            let Some(c) = src.next() else {
                return detector_no_more_source(
                    CodingCat::Big5,
                    src_base,
                    src.pos,
                    found,
                    block,
                    di,
                );
            };
            if c < 0x40 || (0x7F..=0xA0).contains(&c) {
                return false;
            }
            found = cat_mask(CodingCat::Big5);
        } else {
            di.rejected |= cat_mask(CodingCat::Big5);
            return false;
        }
    }
}

/// Port of coding.c `detect_coding_charset` specialised for the bound
/// `iso-latin-1` coding system (the charset category's coding system at
/// startup).  iso-latin-1 is a 1-dimension charset covering the whole byte
/// range; 0x80-0x9F are valid only if `latin-extra-code-table` says so.
fn detect_charset_latin1(
    bytes: &[u8],
    head_ascii: usize,
    multibytep: bool,
    di: &mut DetectInfo,
) -> bool {
    di.checked |= cat_mask(CodingCat::Charset);
    let mut src = DetectSrc::new(bytes, head_ascii, multibytep);
    let mut found = 0u32;
    loop {
        let src_base = src.pos;
        let Some(c) = src.next() else {
            if src_base < src.pos {
                di.rejected |= cat_mask(CodingCat::Charset);
                return false;
            }
            di.found |= found;
            return true;
        };
        if c < 0 {
            // A decoded multibyte char: iso-latin-1's valids only cover bytes,
            // so a non-eight-bit char means this is not iso-latin-1.
            di.rejected |= cat_mask(CodingCat::Charset);
            return false;
        }
        if c >= 0x80 {
            if c < 0xA0 && !latin_extra_code_p(c as u8) {
                di.rejected |= cat_mask(CodingCat::Charset);
                return false;
            }
            found = cat_mask(CodingCat::Charset);
        }
    }
}

/// A bare `1 << coding_category_iso_7` etc. for the four categories a valid
/// ISO-2022 *escape designation* can indicate (coding.c `CATEGORY_MASK_ISO_ESCAPE`).
const MASK_ISO_ESCAPE: u32 = cat_mask(CodingCat::Iso7)
    | cat_mask(CodingCat::Iso7Tight)
    | cat_mask(CodingCat::Iso7Else)
    | cat_mask(CodingCat::Iso8Else);

/// Validate an ISO-2022 charset designation final byte, mirroring GNU's
/// `iso_charset_table[dim][chars_96][final]` lookup in `detect_coding_iso_2022`.
/// Returns `true` when FINAL designates a known charset of the given dimension
/// and 94/96-char register (so GNU treats the designation as valid).
fn iso_2022_designation_known(final_byte: u8, dimension: i64, chars_96: bool) -> bool {
    super::charset::charset_by_iso_final(i64::from(final_byte), dimension, chars_96).is_some()
}

/// Record a valid ISO-2022 charset designation, mirroring the tail of GNU's
/// `detect_coding_iso_2022` ESC handling.  A valid designation rejects the
/// 8-bit categories (`CATEGORY_MASK_ISO_8BIT`) and is `found` for each of the
/// four escape-indicated categories whose coding system can safely encode the
/// designated charset.  GNU consults `SAFE_CHARSET_P` per category; the default
/// `iso-2022-7bit` / `iso-2022-7bit-tight` / `iso-2022-7bit-lock` /
/// `iso-2022-8bit-ss2` categories all accept the standard ISO-2022 charsets, so
/// every recognized designation marks all four found.
fn iso_designation_found(rejected: &mut u32, found: &mut u32) {
    *rejected |= MASK_ISO_8BIT;
    *found |= MASK_ISO_ESCAPE;
}

/// Port of coding.c `detect_coding_iso_2022`.  Handles both the escape-sequence
/// path (ESC designations / locking shifts), which is what `detect-coding-string`
/// exercises for 7-bit ISO-2022 input, and the GL/GR high-byte path used for
/// 8-bit input.
fn detect_iso_2022(bytes: &[u8], head_ascii: usize, multibytep: bool, di: &mut DetectInfo) -> bool {
    di.checked |= MASK_ISO;
    let mut src = DetectSrc::new(bytes, head_ascii, multibytep);
    let mut rejected = 0u32;
    let mut found = 0u32;
    while rejected != MASK_ISO {
        let Some(c) = src.next() else {
            // no_more_source
            di.rejected |= rejected;
            di.found |= found & !rejected;
            return true;
        };
        match c {
            // ISO_CODE_ESC: a designation or shift sequence.  GNU parses the
            // sequence rather than rejecting all of ISO_7BIT|ISO_8BIT.
            0x1B => {
                let Some(c) = src.next() else {
                    di.rejected |= rejected;
                    di.found |= found & !rejected;
                    return true;
                };
                if c == i32::from(b'N') || c == i32::from(b'O') {
                    // ESC N / ESC O: SS2 / SS3.
                    rejected |= MASK_ISO_7BIT | MASK_ISO_8BIT;
                } else if c == i32::from(b'1') {
                    // End of composition.
                    found |= MASK_ISO;
                } else if (i32::from(b'0')..=i32::from(b'4')).contains(&c) {
                    // ESC <Fp>: start/end composition -- no effect on masks.
                } else if (i32::from(b'(')..=i32::from(b'/')).contains(&c) {
                    // Designation sequence for a charset of dimension 1.
                    let chars_96 = c >= i32::from(b',');
                    match src.next() {
                        None => {
                            di.rejected |= rejected;
                            di.found |= found & !rejected;
                            return true;
                        }
                        Some(c1) => {
                            if c1 < i32::from(b' ')
                                || c1 >= 0x80
                                || !iso_2022_designation_known(c1 as u8, 1, chars_96)
                            {
                                // Invalid designation; just ignore (reject 7bit
                                // categories only when the final byte is 8-bit).
                                if c1 >= 0x80 {
                                    rejected |= MASK_ISO_7BIT | cat_mask(CodingCat::Iso7Else);
                                }
                            } else {
                                iso_designation_found(&mut rejected, &mut found);
                            }
                        }
                    }
                } else if c == i32::from(b'$') {
                    // Designation sequence for a charset of dimension 2.
                    match src.next() {
                        None => {
                            di.rejected |= rejected;
                            di.found |= found & !rejected;
                            return true;
                        }
                        Some(c) => {
                            let valid = if (i32::from(b'@')..=i32::from(b'B')).contains(&c) {
                                // JISX0208.1978, GB2312, or JISX0208.
                                iso_2022_designation_known(c as u8, 2, false)
                            } else if (i32::from(b'(')..=i32::from(b'/')).contains(&c) {
                                let chars_96 = c >= i32::from(b',');
                                match src.next() {
                                    None => {
                                        di.rejected |= rejected;
                                        di.found |= found & !rejected;
                                        return true;
                                    }
                                    Some(c1) => {
                                        if c1 < i32::from(b' ')
                                            || c1 >= 0x80
                                            || !iso_2022_designation_known(c1 as u8, 2, chars_96)
                                        {
                                            if c1 >= 0x80 {
                                                rejected |=
                                                    MASK_ISO_7BIT | cat_mask(CodingCat::Iso7Else);
                                            }
                                            false
                                        } else {
                                            true
                                        }
                                    }
                                }
                            } else {
                                if c >= 0x80 {
                                    rejected |= MASK_ISO_7BIT | cat_mask(CodingCat::Iso7Else);
                                }
                                false
                            };
                            if valid {
                                iso_designation_found(&mut rejected, &mut found);
                            }
                        }
                    }
                } else {
                    // Invalid escape sequence; just ignore it.
                    if c >= 0x80 {
                        rejected |= MASK_ISO_7BIT | cat_mask(CodingCat::Iso7Else);
                    }
                }
            }
            // ISO_CODE_SO / ISO_CODE_SI: locking shift out/in.
            0x0E | 0x0F => {
                rejected |= MASK_ISO_7BIT | MASK_ISO_8BIT;
            }
            _ => {
                if c < 0x80 {
                    // ASCII or a decoded multibyte char (c < 0): no effect.
                    continue;
                }
                let c = c as u32;
                rejected |= MASK_ISO_7BIT | cat_mask(CodingCat::Iso7Else);
                if c >= 0xA0 {
                    found |= cat_mask(CodingCat::Iso81);
                    if (rejected & cat_mask(CodingCat::Iso82)) == 0 {
                        let mut len = 1usize;
                        while let Some(cc) = src.peek_raw() {
                            if cc < 0xA0 {
                                break;
                            }
                            src.pos += 1;
                            len += 1;
                        }
                        if len & 1 != 0 && !src.at_end() {
                            rejected |= cat_mask(CodingCat::Iso82);
                        } else {
                            found |= cat_mask(CodingCat::Iso82);
                        }
                    }
                } else if !latin_extra_code_p(c as u8) {
                    rejected = MASK_ISO;
                } else {
                    rejected |= cat_mask(CodingCat::Iso81) | cat_mask(CodingCat::Iso82);
                }
            }
        }
    }
    di.rejected |= MASK_ISO;
    false
}

/// Detect the coding system(s) of unibyte `bytes` (`src_chars` characters),
/// returning the value `detect-coding-string`/`detect-coding-region` produce.
/// Direct port of coding.c `detect_coding_system` with `coding_system = nil`
/// (`undecided`) and no EOL conversion (inputs without CR/LF keep base names).
/// GNU's two detection globals, `coding_priorities` and `coding_categories`
/// (src/coding.c:586, :590), read out of the manager that owns them here.  Both
/// doors need exactly this pair, so it is built in one place rather than in
/// each.
fn coding_category_bindings(
    mgr: &CodingSystemManager,
) -> (Vec<usize>, [Option<SymId>; CODING_CAT_MAX]) {
    let mut cat_system: [Option<SymId>; CODING_CAT_MAX] = [None; CODING_CAT_MAX];
    let mut priorities: Vec<usize> = Vec::with_capacity(CODING_CAT_MAX);
    for &sym in &mgr.priority {
        if let Some(cat) = coding_category_of(mgr, resolve_sym(sym)).and_then(coding_cat_index) {
            if cat_system[cat].is_none() {
                cat_system[cat] = Some(sym);
            }
            priorities.push(cat);
        }
    }
    // Append any categories not represented in the priority list, in enum order
    // (so the priority walk covers all categories like GNU's fixed-size array).
    for cat in 0..CODING_CAT_MAX {
        if !priorities.contains(&cat) {
            priorities.push(cat);
        }
    }
    (priorities, cat_system)
}

fn detect_coding_systems(
    mgr: &CodingSystemManager,
    bytes: &[u8],
    src_chars: usize,
    multibytep: bool,
    highest: bool,
    block: SourceBlock,
) -> Value {
    let (priorities, cat_system) = coding_category_bindings(mgr);

    let detected = detect_categories(
        &priorities,
        &cat_system,
        bytes,
        src_chars,
        multibytep,
        highest,
        block,
    );

    // GNU `detect_coding_system` finishes by detecting the end-of-line format
    // (src/coding.c:8932) and rewriting every result coding system whose
    // eol_type is still a subsidiary VECTOR (e.g. `undecided`) to its detected
    // `-unix`/`-dos`/`-mac` variant.  Apply the same here so an all-ASCII string
    // with CRLF/CR terminators reports `undecided-dos` / `undecided-mac` rather
    // than the bare `undecided`.
    apply_detected_eol(mgr, detected, bytes, highest)
}

/// The coding system a DECODE of raw BYTES re-bases itself to: GNU
/// `detect_coding` (src/coding.c:6502), the detector `decode_coding_object`
/// runs, NOT the one `detect-coding-string` reports.
///
/// The two are separate functions in GNU and are separate functions here, for
/// the reason [`scan_undecided`] records: their tails disagree about a null
/// byte, and a decode that borrowed the reporting tail answered
/// `no-conversion` for every UTF-16 signature.  They cannot be confused for one
/// another by accident any more, because they no longer have the same result
/// type -- this one answers ONE base coding system or GNU's nil, and the
/// reporting one answers a Lisp list.
///
/// `None` is GNU's `found == nil` after the end-of-line rewrite: nothing
/// re-bases the coding system.  Callers spell that `undecided`, which is what
/// `apply_detected_eol` turns into the `-dos` / `-mac` subsidiary GNU's
/// `adjust_coding_eol_type` produces on the other axis.
pub(crate) fn detect_highest_coding_system_for_unibyte_bytes(
    mgr: &CodingSystemManager,
    bytes: &[u8],
    block: SourceBlock,
) -> Option<SymId> {
    let (priorities, cat_system) = coding_category_bindings(mgr);
    let scan = scan_undecided(
        &priorities,
        &cat_system,
        bytes,
        bytes.len(),
        false,
        block,
        WalkStop::AtFirstFound,
    );
    let base = match detect_coding_found(mgr, &priorities, &cat_system, &scan) {
        DetectedBase::Rebase(sym) => Value::symbol(resolve_sym(sym)),
        // GNU leaves `coding` alone; this engine's spelling of that is the name
        // the caller already holds, which for every caller of this function is
        // `undecided`.
        DetectedBase::Unchanged => Value::symbol("undecided"),
    };
    let detected = apply_detected_eol(mgr, base, bytes, true);
    (!detected.is_nil()).then(|| {
        detected
            .as_symbol_id()
            .expect("a non-nil coding-system detection result is a symbol")
    })
}

/// GNU `EOL_SEEN_*` summary of the line terminators found in DATA.
#[derive(Clone, Copy, PartialEq, Eq)]
enum DetectedEol {
    None,
    Lf,
    Crlf,
    Cr,
}

/// Port of GNU `detect_eol` (src/coding.c:6375) for the non-UTF-16 categories:
/// scan up to `MAX_EOL_CHECK_COUNT` line terminators and classify the file as
/// LF, CRLF, or CR, with the stray-^M tolerance GNU applies for DOS files.
fn detect_eol_seen(bytes: &[u8]) -> DetectedEol {
    const MAX_EOL_CHECK_COUNT: usize = 3;
    let mut eol_seen = DetectedEol::None;
    let mut total = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        let c = bytes[i];
        i += 1;
        if c == b'\n' || c == b'\r' {
            let this_eol = if c == b'\n' {
                DetectedEol::Lf
            } else if i >= bytes.len() || bytes[i] != b'\n' {
                DetectedEol::Cr
            } else {
                i += 1;
                DetectedEol::Crlf
            };
            if eol_seen == DetectedEol::None {
                eol_seen = this_eol;
            } else if eol_seen != this_eol {
                if (eol_seen == DetectedEol::Cr && this_eol == DetectedEol::Crlf)
                    || (eol_seen == DetectedEol::Crlf && this_eol == DetectedEol::Cr)
                {
                    eol_seen = DetectedEol::Crlf;
                } else {
                    eol_seen = DetectedEol::Lf;
                    break;
                }
            }
            total += 1;
            if total == MAX_EOL_CHECK_COUNT {
                break;
            }
        }
    }
    eol_seen
}

/// Rewrite each detected coding system to its detected EOL subsidiary, mirroring
/// the tail of GNU `detect_coding_system`.  Only coding systems with an
/// undecided (vector) eol_type are rewritten; a coding system that already
/// carries a concrete EOL keeps its name.
fn apply_detected_eol(
    mgr: &CodingSystemManager,
    detected: Value,
    bytes: &[u8],
    highest: bool,
) -> Value {
    let eol_suffix = match detect_eol_seen(bytes) {
        DetectedEol::Lf => Some("-unix"),
        DetectedEol::Crlf => Some("-dos"),
        DetectedEol::Cr => Some("-mac"),
        DetectedEol::None => None,
    };
    let Some(eol_suffix) = eol_suffix else {
        return detected;
    };
    let rewrite = |sym: Value| -> Value {
        let Some(name) = sym.as_symbol_name() else {
            return sym;
        };
        // A coding system that already names a concrete EOL is left untouched
        // (GNU rewrites only when `VECTORP (eol_type)`).
        if EolType::from_suffix(name).is_some() {
            return sym;
        }
        match mgr.canonical_name_for_detected_eol(name, eol_suffix) {
            Some(resolved) => Value::symbol(resolved),
            None => sym,
        }
    };
    if highest {
        return rewrite(detected);
    }
    let Some(items) = super::value::list_to_vec(&detected) else {
        return detected;
    };
    Value::list(items.into_iter().map(rewrite).collect())
}

/// Whether the category priority walk stops at the first category it finds.
///
/// GNU's two undecided detectors differ here and the difference is not
/// cosmetic: `detect_coding` always stops, because it wants ONE coding system
/// to become (src/coding.c:6642-6643, a `break` guarded only by
/// `detect_info.found & (1 << category)`); `detect_coding_system` stops only
/// under HIGHEST, because otherwise it has to run every detector to report the
/// whole list (src/coding.c:8818-8831).  It is a required argument so that a
/// new caller has to say which of the two it is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum WalkStop {
    /// GNU's unconditional `break`, and its `highest` one.
    AtFirstFound,
    /// GNU's `highest == 0`: keep going and collect every category's verdict.
    RunEveryDetector,
}

/// GNU `detect_coding`'s `found` (src/coding.c:6507, tested at :6743): what a
/// decode-time detection can answer.
///
/// `Unchanged` is GNU's nil, and it is a DIFFERENT statement from "the answer
/// is `undecided`": it says the detector declined to replace the coding system,
/// which is the normal outcome for text that is nothing but ASCII.  Keeping the
/// two apart in the type is what stops `undecided` -- a name whose whole meaning
/// is "this is not the coding system yet" -- from being returned as though it
/// were a detection result, the same lie entry 151 removed from
/// `ProcessOutputDecoding`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum DetectedBase {
    /// `setup_coding_system (found, coding)` replaces the whole object
    /// (src/coding.c:6751).
    Rebase(SymId),
    /// `found` stayed nil: the decode keeps the coding system it was given.
    Unchanged,
}

/// GNU's detection state at the point where its two undecided detectors' result
/// rules diverge -- everything the two share, and nothing either decides.
struct UndecidedScan {
    di: DetectInfo,
    /// `null_byte_found` (src/coding.c:6523, :8702).  Both tails read it; they
    /// do NOT read it the same way, which is this entry's divergence.
    null_byte_found: bool,
    /// GNU's `category` / `this` where the priority walk broke -- the
    /// `i < coding_category_raw_text` test both tails make afterwards.  `None`
    /// is GNU's `i == coding_category_raw_text`: the walk ran to the end
    /// without settling on one.
    found_at: Option<usize>,
}

/// The half of GNU's undecided detection that is literally the same code in
/// both of its detectors: the ASCII / ISO-escape scan and the category priority
/// walk.
///
/// GNU has TWO undecided detectors, and they are different functions with
/// different answers -- not one function called twice:
///
/// * `detect_coding` (src/coding.c:6502) is the one a DECODE runs, through
///   `decode_coding_object`'s `if (CODING_REQUIRE_DETECTION (coding))`
///   (:8128-8129).  Its result is a coding system the decode then BECOMES
///   (`setup_coding_system (found, coding)`, :6751), and `found` may be nil,
///   which means "nothing re-bases this".
/// * `detect_coding_system` (src/coding.c:8686) is the one
///   `detect-coding-string` and `detect-coding-region` REPORT.  Its result is a
///   list, and it always has one.
///
/// Their scan loops (:6529-6594 and :8731-8773) and their priority walks
/// (:6622-6645 and :8801-8832) agree line for line.  Their TAILS do not, and
/// the difference is observable from Lisp -- both rows measured under GNU Emacs
/// 31.0.90:
///
/// ```elisp
/// (decode-coding-string "\377\376a\0\r\0\n\0" 'undecided)
/// ;; => "a\n", last-coding-system-used  utf-16le-with-signature-dos
/// (detect-coding-string "\377\376a\0\r\0\n\0" t)
/// ;; => no-conversion
/// ```
///
/// `detect_coding_system` forces `no-conversion` whenever a null byte was seen,
/// unconditionally (src/coding.c:8836-8842); `detect_coding` reaches that answer
/// only as a FALLBACK, after a priority walk that the null byte NARROWED to the
/// UTF-16 categories rather than closed (:6614-6618 narrowing, :6683-6684
/// fallback).
fn scan_undecided(
    priorities: &[usize],
    cat_system: &[Option<SymId>; CODING_CAT_MAX],
    bytes: &[u8],
    src_chars: usize,
    multibytep: bool,
    block: SourceBlock,
    stop: WalkStop,
) -> UndecidedScan {
    let mut di = DetectInfo::default();
    let mut null_byte_found = false;
    let mut eight_bit_found = false;
    let mut head_ascii = 0usize;

    // ASCII skip loop (coding.c:8732).  ISO escape early-detection runs the ISO
    // detector when an ESC/SI/SO appears before any 8-bit byte.
    let mut i = 0usize;
    while i < bytes.len() {
        let c = bytes[i];
        if c & 0x80 != 0 {
            eight_bit_found = true;
            if null_byte_found {
                break;
            }
        } else if c < 0x20 {
            if (c == 0x1B || c == 0x0F || c == 0x0E) && di.checked == 0 {
                if detect_iso_2022(bytes, 0, multibytep, &mut di) {
                    if (di.rejected & cat_mask(CodingCat::Iso7Else)) == 0 {
                        i = bytes.len();
                        head_ascii = i;
                    }
                    di.rejected |= !(MASK_ISO_7BIT | MASK_ISO_ELSE);
                    break;
                }
            } else if c == 0 {
                null_byte_found = true;
                if eight_bit_found {
                    break;
                }
            }
            if !eight_bit_found {
                head_ascii += 1;
            }
        } else if !eight_bit_found {
            head_ascii += 1;
        }
        i += 1;
    }

    let mut found_at = None;
    if null_byte_found || eight_bit_found || head_ascii < bytes.len() || di.found != 0 {
        if head_ascii == bytes.len() {
            // "As all bytes are 7-bit, we can ignore non-ISO-2022 codings": no
            // detector runs, and the walk only looks for what the ISO escape
            // scan above already found (coding.c:6603-6612, :8781-8788).
            found_at = priorities
                .iter()
                .take(CODING_CAT_RAW_TEXT)
                .copied()
                .find(|&cat| di.found & (1 << cat) != 0);
        } else {
            if null_byte_found {
                // GNU NARROWS here, it does not decide (coding.c:6614-6618,
                // :8790-8794).  The four UTF-16 categories stay unchecked and
                // unrejected, and the walk below still runs -- which is how a
                // UTF-16 signature survives a source full of null bytes.  The
                // previous port stopped here and let the tail answer
                // `no-conversion`, which is `detect_coding_system`'s rule
                // applied to `detect_coding`'s door.
                di.checked |= !MASK_UTF_16;
                di.rejected |= !MASK_UTF_16;
            }
            // Run each category detector for the first `coding_category_raw_text`
            // PRIORITY POSITIONS (coding.c:6622 / :8801
            // `i < coding_category_raw_text`), skipping categories whose enum
            // value is >= raw_text (:6631 / :8813) and categories with no bound
            // coding system (:6626 / :8808).
            let scan = priorities.len().min(CODING_CAT_RAW_TEXT);
            for &cat in &priorities[..scan] {
                if cat_system[cat].is_none() {
                    di.rejected |= 1 << cat;
                    continue;
                }
                if cat >= CODING_CAT_RAW_TEXT {
                    continue;
                }
                let accepted = if di.checked & (1 << cat) != 0 {
                    // Already answered by an earlier detector; GNU does not run
                    // it again and treats the recorded verdict as its return
                    // value (coding.c:6632-6636, :8815-8820).
                    true
                } else {
                    run_detector(
                        cat, bytes, head_ascii, src_chars, multibytep, block, &mut di,
                    )
                };
                if stop == WalkStop::AtFirstFound && accepted && di.found & (1 << cat) != 0 {
                    found_at = Some(cat);
                    break;
                }
            }
        }
    }

    UndecidedScan {
        di,
        null_byte_found,
        found_at,
    }
}

/// GNU `detect_coding`'s result rule (src/coding.c:6647-6699): the coding system
/// a DECODE re-bases itself to.
fn detect_coding_found(
    mgr: &CodingSystemManager,
    priorities: &[usize],
    cat_system: &[Option<SymId>; CODING_CAT_MAX],
    scan: &UndecidedScan,
) -> DetectedBase {
    if let Some(cat) = scan.found_at {
        let Some(this) = cat_system[cat] else {
            return DetectedBase::Unchanged;
        };
        // The two "auto" categories do not answer with their own name.  Their
        // `:bom` attribute is a CONS of the two concrete coding systems the
        // signature chooses between, and `detect_coding` returns one of those
        // (src/coding.c:6649-6680); only a non-cons `:bom` falls back to the
        // category's own name.
        if cat == CodingCat::Utf8Auto as usize {
            return DetectedBase::Rebase(match coding_bom_auto_pair(mgr, this) {
                Some((sig, nosig)) => {
                    if scan.di.found & cat_mask(CodingCat::Utf8Sig) != 0 {
                        sig
                    } else {
                        nosig
                    }
                }
                None => this,
            });
        }
        if cat == CodingCat::Utf16Auto as usize {
            return match coding_bom_auto_pair(mgr, this) {
                Some((le, be)) => {
                    if scan.di.found & cat_mask(CodingCat::Utf16Le) != 0 {
                        DetectedBase::Rebase(le)
                    } else if scan.di.found & cat_mask(CodingCat::Utf16Be) != 0 {
                        DetectedBase::Rebase(be)
                    } else {
                        // GNU's `found` stays nil: the cons was there and
                        // neither endianness was signalled.
                        DetectedBase::Unchanged
                    }
                }
                None => DetectedBase::Rebase(this),
            };
        }
        return DetectedBase::Rebase(this);
    }
    if scan.null_byte_found {
        // The FALLBACK, not the rule (src/coding.c:6683-6684): reached only
        // because the UTF-16-narrowed walk above settled on nothing.
        return DetectedBase::Rebase(intern("no-conversion"));
    }
    if (scan.di.rejected & MASK_ANY) == MASK_ANY {
        // `found = Qraw_text` (src/coding.c:6686).  `detect_coding_system`
        // answers `no-conversion` for the same state (:8836-8842); with
        // `coding-category-raw-text` in the priority list neither is reachable,
        // because that category is never rejected and the clause below claims
        // it first.
        return DetectedBase::Rebase(intern("raw-text"));
    }
    if scan.di.rejected != 0 {
        // "the highest-priority category that was not rejected"
        // (src/coding.c:6687-6699).
        for &cat in priorities.iter().take(CODING_CAT_RAW_TEXT) {
            if scan.di.rejected & (1 << cat) == 0
                && let Some(sym) = cat_system[cat]
            {
                return DetectedBase::Rebase(sym);
            }
        }
    }
    DetectedBase::Unchanged
}

/// GNU's `AREF (CODING_ID_ATTRS (id), coding_attr_utf_bom)` when it is a CONS:
/// the pair `define-coding-system`'s `:bom` argument stores, e.g.
/// `(utf-16le-with-signature . utf-16be-with-signature)` for `utf-16`
/// (lisp/international/mule-conf.el:1463).  `None` is GNU's `! CONSP`.
fn coding_bom_auto_pair(mgr: &CodingSystemManager, name: SymId) -> Option<(SymId, SymId)> {
    let bom = *mgr
        .get(resolve_sym(name))?
        .properties
        .get(&intern(":bom"))?;
    if !bom.is_cons() {
        return None;
    }
    Some((
        bom.cons_car().as_symbol_id()?,
        bom.cons_cdr().as_symbol_id()?,
    ))
}

/// The pure detection core for `detect-coding-string` / `detect-coding-region`:
/// GNU `detect_coding_system`'s result rule (src/coding.c:8836-8886) over the
/// shared scan.  Split out from `detect_coding_systems` so it can be
/// unit-tested against GNU's bindings without a fully-booted coding manager.
fn detect_categories(
    priorities: &[usize],
    cat_system: &[Option<SymId>; CODING_CAT_MAX],
    bytes: &[u8],
    src_chars: usize,
    multibytep: bool,
    highest: bool,
    block: SourceBlock,
) -> Value {
    let UndecidedScan {
        di,
        null_byte_found,
        found_at: _,
    } = scan_undecided(
        priorities,
        cat_system,
        bytes,
        src_chars,
        multibytep,
        block,
        if highest {
            WalkStop::AtFirstFound
        } else {
            WalkStop::RunEveryDetector
        },
    );

    // Result construction (coding.c:8838).
    let mut val: Vec<SymId> = Vec::new();
    if (di.rejected & MASK_ANY) == MASK_ANY || null_byte_found {
        // Binary / undetectable -> no-conversion.
        val.push(intern("no-conversion"));
    } else if di.rejected == 0 && di.found == 0 {
        val.push(intern("undecided"));
    } else {
        // The `highest == nil` branch (coding.c:8868), iterating the first
        // `coding_category_raw_text` PRIORITY POSITIONS (NOT filtered by the
        // category enum value -- so the coding system bound to e.g. the
        // raw-text category at a high priority position is included).
        let scan = priorities.len().min(CODING_CAT_RAW_TEXT);
        let mask = di.rejected | di.found;
        // Tail: GNU's first reverse loop overwrites `val = list1i(id)` for every
        // "neither rejected nor found" position with id >= 0, so the final tail
        // is the single highest-priority such category.
        let mut neither: Option<SymId> = None;
        for &cat in priorities[..scan].iter().rev() {
            if mask & (1 << cat) == 0
                && let Some(sym) = cat_system[cat]
            {
                neither = Some(sym);
            }
        }
        // Found categories, prepended in reverse -> priority order.
        let mut found_list: Vec<SymId> = Vec::new();
        for &cat in priorities[..scan].iter().rev() {
            if di.found & (1 << cat) != 0
                && let Some(sym) = cat_system[cat]
            {
                found_list.insert(0, sym);
            }
        }
        val = found_list;
        if let Some(sym) = neither {
            val.push(sym);
        }
    }

    if highest {
        return val
            .first()
            .map_or(Value::NIL, |&s| Value::symbol(resolve_sym(s)));
    }
    Value::list(
        val.into_iter()
            .map(|s| Value::symbol(resolve_sym(s)))
            .collect(),
    )
}

/// GNU's `(*(this->detector)) (coding, &detect_info)` (src/coding.c:6641,
/// :8827).  The RETURN VALUE is half of the walk's break condition -- GNU
/// requires the detector to return non-zero AND the category's `found` bit --
/// so it is returned here rather than dropped.
fn run_detector(
    cat: usize,
    bytes: &[u8],
    head_ascii: usize,
    src_chars: usize,
    multibytep: bool,
    block: SourceBlock,
    di: &mut DetectInfo,
) -> bool {
    if cat == CodingCat::Utf8Nosig as usize
        || cat == CodingCat::Utf8Auto as usize
        || cat == CodingCat::Utf8Sig as usize
    {
        detect_utf_8(bytes, head_ascii, multibytep, block, di)
    } else if (CodingCat::Utf16Auto as usize..=CodingCat::Utf16LeNosig as usize).contains(&cat) {
        detect_utf_16(bytes, src_chars, block, di)
    } else if cat == CodingCat::EmacsMule as usize {
        detect_emacs_mule(bytes, head_ascii, multibytep, block, di)
    } else if cat == CodingCat::Sjis as usize {
        detect_sjis(bytes, head_ascii, multibytep, block, di)
    } else if cat == CodingCat::Big5 as usize {
        detect_big5(bytes, head_ascii, multibytep, block, di)
    } else if cat == CodingCat::Charset as usize {
        detect_charset_latin1(bytes, head_ascii, multibytep, di)
    } else if (CodingCat::Iso7 as usize..=CodingCat::Iso8Else as usize).contains(&cat) {
        detect_iso_2022(bytes, head_ascii, multibytep, di)
    } else {
        // ccl has no byte-level detector; it stays unchecked.
        false
    }
}

/// `(detect-coding-string STRING &optional HIGHEST)` -- detect the coding
/// system(s) of STRING.  See `detect_coding_systems` (port of coding.c).
pub(crate) fn builtin_detect_coding_string(
    mgr: &CodingSystemManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("detect-coding-string", &args, 1)?;
    expect_max_args("detect-coding-string", &args, 2)?;
    let Some(s) = args[0].as_lisp_string() else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), args[0]],
        ));
    };
    let bytes = crate::encoding::lisp_string_coding_source_bytes(s);
    let src_chars = s.schars();
    let multibytep = s.is_multibyte();
    let highest = args.get(1).is_some_and(|v| v.is_truthy());
    // A string is complete by construction, which is GNU setting
    // `CODING_MODE_LAST_BLOCK` in `Fdetect_coding_string` (src/coding.c:8716).
    Ok(detect_coding_systems(
        mgr,
        &bytes,
        src_chars,
        multibytep,
        highest,
        SourceBlock::Last,
    ))
}

/// `(detect-coding-region START END &optional HIGHEST)` -- detect the encoding
/// of a buffer region. Stub: always returns utf-8.
fn validate_detect_coding_region(buffers: &BufferManager, args: &[Value]) -> Result<(), Flow> {
    let start = crate::emacs_core::position::fix_position_with_buffers(buffers, &args[0])?;
    let end = crate::emacs_core::position::fix_position_with_buffers(buffers, &args[1])?;
    let (beg, end) = if end < start {
        (end, start)
    } else {
        (start, end)
    };
    let buf = buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let point_min = buf.point_min_lisp_char_pos().as_i64();
    let point_max = buf.point_max_lisp_char_pos().as_i64();
    if !(point_min <= beg && end <= point_max) {
        return Err(signal(
            LispCondition::ArgsOutOfRange,
            vec![Value::make_buffer(buf.id), args[0], args[1]],
        ));
    }
    Ok(())
}

pub(crate) fn builtin_detect_coding_region(
    mgr: &CodingSystemManager,
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("detect-coding-region", &args, 2)?;
    expect_max_args("detect-coding-region", &args, 3)?;
    validate_detect_coding_region(buffers, &args)?;
    let highest = args.get(2).is_some_and(|v| v.is_truthy());

    let start = crate::emacs_core::position::fix_position_with_buffers(buffers, &args[0])?;
    let end = crate::emacs_core::position::fix_position_with_buffers(buffers, &args[1])?;
    let (start, end) = if end < start {
        (end, start)
    } else {
        (start, end)
    };
    let buffer = buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let byte_range = EmacsByteRange::new(
        buffer.lisp_pos_to_full_buffer_emacs_byte_pos(LispCharPos1::new(start)),
        buffer.lisp_pos_to_full_buffer_emacs_byte_pos(LispCharPos1::new(end)),
    );
    let string = buffer.buffer_substring_lisp_string_range(byte_range);
    let bytes = crate::encoding::lisp_string_coding_source_bytes(&string);
    let src_chars = string.schars();
    let multibytep = string.is_multibyte();
    // Likewise `Fdetect_coding_region` (src/coding.c:8009).
    Ok(detect_coding_systems(
        mgr,
        &bytes,
        src_chars,
        multibytep,
        highest,
        SourceBlock::Last,
    ))
}

/// `(keyboard-coding-system &optional TERMINAL)` -- return the current
/// keyboard coding system. The TERMINAL argument is ignored.
pub(crate) fn builtin_keyboard_coding_system(
    mgr: &CodingSystemManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("keyboard-coding-system", &args, 1)?;
    Ok(Value::symbol(mgr.keyboard_coding))
}

/// `(terminal-coding-system &optional TERMINAL)` -- return the current
/// terminal coding system. The TERMINAL argument is ignored.
pub(crate) fn builtin_terminal_coding_system(
    mgr: &CodingSystemManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("terminal-coding-system", &args, 1)?;
    Ok(Value::symbol(mgr.terminal_coding))
}

/// `(set-keyboard-coding-system CODING-SYSTEM &optional TERMINAL)` -- set the
/// keyboard coding system. Raw TTY byte batches are decoded incrementally with
/// this coding system before entering the key-sequence translation maps.
pub(crate) fn builtin_set_keyboard_coding_system(
    mgr: &mut CodingSystemManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("set-keyboard-coding-system", &args, 1)?;
    expect_max_args("set-keyboard-coding-system", &args, 2)?;
    if args[0].is_nil() {
        mgr.keyboard_coding = intern("no-conversion");
        return Ok(Value::NIL);
    }
    let Some(name) = args[0].as_symbol_name().map(|s| s.to_string()) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), args[0]],
        ));
    };
    if !is_known_or_derived_coding_system(mgr, &name) {
        return Err(signal(LispCondition::CodingSystemError, vec![args[0]]));
    }
    let normalization_input =
        if matches!(EolType::from_suffix(&name), Some(EolType::Unix)) || name == "emacs-internal" {
            name.clone()
        } else {
            canonical_runtime_name(mgr, &name)
                .ok_or_else(|| signal(LispCondition::CodingSystemError, vec![args[0]]))?
        };
    let base = strip_eol_suffix(&normalization_input);
    if matches!(base, "utf-8-auto" | "prefer-utf-8") {
        return Err(signal(
            "error",
            vec![Value::string(format!(
                "Unsuitable coding system for keyboard: {name}"
            ))],
        ));
    }
    if base == "undecided" {
        return Err(signal(
            "error",
            vec![Value::string(format!(
                "Unsupported coding system for keyboard: {normalization_input}"
            ))],
        ));
    }
    let normalized = normalize_keyboard_coding_system(&normalization_input);
    mgr.keyboard_coding = intern(&normalized);
    Ok(Value::symbol(mgr.keyboard_coding))
}

/// `(set-terminal-coding-system CODING-SYSTEM &optional TERMINAL)` -- set the
/// terminal coding system. Stub: records the value but has no functional effect.
pub(crate) fn builtin_set_terminal_coding_system(
    mgr: &mut CodingSystemManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("set-terminal-coding-system", &args, 1)?;
    expect_max_args("set-terminal-coding-system", &args, 3)?;
    if args[0].is_nil() {
        mgr.terminal_coding = intern("nil");
        return Ok(Value::NIL);
    }
    let Some(name) = args[0].as_symbol_name().map(|s| s.to_string()) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), args[0]],
        ));
    };
    if !is_known_or_derived_coding_system(mgr, &name) {
        return Err(signal(LispCondition::CodingSystemError, vec![args[0]]));
    }
    mgr.terminal_coding = intern(&name);
    Ok(Value::NIL)
}

/// `(set-keyboard-coding-system-internal CODING-SYSTEM &optional TERMINAL)` --
/// internal keyboard coding setter. Mirrors the surface validation but always
/// returns nil.
pub(crate) fn builtin_set_keyboard_coding_system_internal(
    mgr: &mut CodingSystemManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("set-keyboard-coding-system-internal", &args, 1)?;
    expect_max_args("set-keyboard-coding-system-internal", &args, 2)?;
    let _ = builtin_set_keyboard_coding_system(mgr, args)?;
    Ok(Value::NIL)
}

/// `(set-terminal-coding-system-internal CODING-SYSTEM &optional TERMINAL)` --
/// internal terminal coding setter.
pub(crate) fn builtin_set_terminal_coding_system_internal(
    mgr: &mut CodingSystemManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("set-terminal-coding-system-internal", &args, 1)?;
    expect_max_args("set-terminal-coding-system-internal", &args, 2)?;
    let _ = builtin_set_terminal_coding_system(mgr, args)?;
    Ok(Value::NIL)
}

/// `(set-safe-terminal-coding-system-internal CODING-SYSTEM)` -- internal safe
/// terminal coding setter.
pub(crate) fn builtin_set_safe_terminal_coding_system_internal(
    mgr: &mut CodingSystemManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("set-safe-terminal-coding-system-internal", &args, 1)?;
    let _ = builtin_set_terminal_coding_system(mgr, args)?;
    Ok(Value::NIL)
}

/// `(text-quoting-style)` -- return the current effective text quoting style.
///
/// Mirrors GNU `Ftext_quoting_style` (`src/doc.c:652-678`):
///   - If `text-quoting-style' is `grave', `straight', or `curve', return it.
///   - If nil (the default), return `grave' when curved quotes cannot be
///     displayed, otherwise `curve'.
///   - Any other value is treated as `curve'.
///   - Never returns nil.
///
/// The display-capability fallback (GNU's `default_to_grave_quoting_style')
/// is currently a stub that always picks `curve' — neomacs does not yet
/// query the active display table for curved-quote support. This matches
/// GNU's behavior on a graphical/UTF-8 terminal.
pub(crate) fn builtin_text_quoting_style(
    eval: &super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("text-quoting-style", &args, 0)?;
    let var = eval
        .eval_symbol_by_id(crate::emacs_core::intern::intern("text-quoting-style"))
        .unwrap_or(Value::NIL);
    if var.is_nil() {
        // GNU `default_to_grave_quoting_style' inspects the standard
        // display table to decide whether curved quotes are renderable.
        // Stub: always pick `curve'. Bringing in real display-capability
        // detection is a separate task.
        return Ok(TextQuotingStyle::Curve.to_symbol());
    }
    if let Some(style) = TextQuotingStyle::from_symbol_value(var) {
        return Ok(style.to_symbol());
    }
    Ok(TextQuotingStyle::Curve.to_symbol())
}

/// `(set-text-conversion-style STYLE &optional WHERE)` -- set conversion style.
/// NeoVM currently accepts all values and returns nil.
pub(crate) fn builtin_set_text_conversion_style(args: Vec<Value>) -> EvalResult {
    expect_min_args("set-text-conversion-style", &args, 1)?;
    expect_max_args("set-text-conversion-style", &args, 2)?;
    Ok(Value::NIL)
}

/// `(coding-system-priority-list &optional HIGHESTP)` -- return the current
/// priority list of coding systems for detection. If HIGHESTP is non-nil,
/// return only the highest-priority system.
pub(crate) fn builtin_coding_system_priority_list(
    mgr: &CodingSystemManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("coding-system-priority-list", &args, 1)?;
    let highest_only = args.first().is_some_and(|v| v.is_truthy());
    if highest_only {
        // GNU `Fcoding_system_priority_list` returns the BARE base-name symbol
        // of the highest-priority category when HIGHESTP is non-nil
        // (`return CODING_ATTR_BASE_NAME (attrs)`), not a one-element list.
        if let Some(first) = mgr.priority.first() {
            Ok(Value::symbol(*first))
        } else {
            Ok(Value::NIL)
        }
    } else {
        let items: Vec<Value> = mgr.priority.iter().map(|id| Value::symbol(*id)).collect();
        Ok(Value::list(items))
    }
}

// ---------------------------------------------------------------------------
// Helper functions
// ---------------------------------------------------------------------------

/// Strip -unix, -dos, or -mac suffix from a coding system name.
fn strip_eol_suffix(name: &str) -> &str {
    for suffix in &["-unix", "-dos", "-mac"] {
        if let Some(base) = name.strip_suffix(suffix) {
            return base;
        }
    }
    name
}

fn normalize_coding_name_for_lookup(name: &str) -> &str {
    if name == "nil" { "no-conversion" } else { name }
}

fn display_base_name(base: &str) -> &str {
    match base {
        "latin-1" | "iso-8859-1" | "iso-latin-1" => "iso-latin-1",
        "latin-5" | "iso-8859-9" | "iso-latin-5" => "iso-latin-5",
        "latin-0" | "latin-9" | "iso-8859-15" | "iso-latin-9" => "iso-latin-9",
        "cn-gb-2312" | "euc-china" | "euc-cn" | "cn-gb" | "gb2312" | "chinese-iso-8bit" => {
            "chinese-iso-8bit"
        }
        "big5" | "cn-big5" | "cp950" | "chinese-big5" => "chinese-big5",
        "big5-hkscs" | "cn-big5-hkscs" | "chinese-big5-hkscs" => "chinese-big5-hkscs",
        "gbk" | "cp936" | "windows-936" | "chinese-gbk" => "chinese-gbk",
        "gb18030" | "chinese-gb18030" => "chinese-gb18030",
        "ascii" | "us-ascii" => "us-ascii",
        "binary" | "no-conversion" | "nil" => "no-conversion",
        "emacs-internal" | "utf-8-emacs" => "utf-8-emacs",
        "mule-utf-8" => "utf-8",
        other => other,
    }
}

fn coding_type_for_base(base: &str) -> Option<&'static str> {
    match base {
        "utf-8"
        | "mule-utf-8"
        | "utf-8-auto"
        | "utf-8-with-signature"
        | "emacs-internal"
        | "utf-8-emacs" => Some("utf-8"),
        "latin-1" | "iso-8859-1" | "iso-latin-1" | "latin-5" | "iso-8859-9" | "iso-latin-5"
        | "latin-0" | "latin-9" | "iso-8859-15" | "iso-latin-9" | "ascii" | "us-ascii"
        | "big5-hkscs" | "cn-big5-hkscs" | "chinese-big5-hkscs" => Some("charset"),
        "cn-gb-2312" | "euc-china" | "euc-cn" | "cn-gb" | "gb2312" | "chinese-iso-8bit" => {
            Some("iso-2022")
        }
        "big5" | "cn-big5" | "cp950" | "chinese-big5" => Some("big5"),
        "gbk" | "cp936" | "windows-936" | "chinese-gbk" | "gb18030" | "chinese-gb18030" => {
            Some("charset")
        }
        "raw-text" | "binary" | "no-conversion" => Some("raw-text"),
        "undecided" | "prefer-utf-8" => Some("undecided"),
        _ => None,
    }
}

fn default_mnemonic_for_base(base: &str) -> Option<i64> {
    match base {
        "utf-8"
        | "mule-utf-8"
        | "utf-8-auto"
        | "utf-8-with-signature"
        | "emacs-internal"
        | "utf-8-emacs" => Some('U' as i64),
        "latin-1" | "iso-8859-1" | "iso-latin-1" => Some('1' as i64),
        "latin-5" | "iso-8859-9" | "iso-latin-5" => Some('9' as i64),
        "latin-0" | "latin-9" | "iso-8859-15" | "iso-latin-9" => Some('0' as i64),
        "cn-gb-2312" | "euc-china" | "euc-cn" | "cn-gb" | "gb2312" | "chinese-iso-8bit" => {
            Some('c' as i64)
        }
        "big5" | "cn-big5" | "cp950" | "chinese-big5" | "big5-hkscs" | "cn-big5-hkscs"
        | "chinese-big5-hkscs" => Some('B' as i64),
        "gbk" | "cp936" | "windows-936" | "chinese-gbk" | "gb18030" | "chinese-gb18030" => {
            Some('c' as i64)
        }
        "ascii" | "us-ascii" | "undecided" | "prefer-utf-8" => Some('-' as i64),
        "raw-text" => Some('t' as i64),
        "binary" | "no-conversion" => Some('=' as i64),
        _ => None,
    }
}

fn properties_bucket_base(base: &str) -> &str {
    match base {
        "latin-1" | "iso-8859-1" | "iso-latin-1" => "iso-latin-1",
        "latin-5" | "iso-8859-9" | "iso-latin-5" => "iso-latin-5",
        "latin-0" | "latin-9" | "iso-8859-15" | "iso-latin-9" => "iso-latin-9",
        "cn-gb-2312" | "euc-china" | "euc-cn" | "cn-gb" | "gb2312" | "chinese-iso-8bit" => {
            "chinese-iso-8bit"
        }
        "big5" | "cn-big5" | "cp950" | "chinese-big5" => "chinese-big5",
        "big5-hkscs" | "cn-big5-hkscs" | "chinese-big5-hkscs" => "chinese-big5-hkscs",
        "gbk" | "cp936" | "windows-936" | "chinese-gbk" => "chinese-gbk",
        "gb18030" | "chinese-gb18030" => "chinese-gb18030",
        "ascii" | "us-ascii" => "us-ascii",
        "binary" | "no-conversion" | "nil" => "no-conversion",
        "emacs-internal" | "utf-8-emacs" => "utf-8-emacs",
        "mule-utf-8" => "utf-8",
        other => other,
    }
}

fn eol_vector_base(base: &str) -> &str {
    match base {
        "latin-1" | "iso-8859-1" | "iso-latin-1" => "iso-latin-1",
        "latin-5" | "iso-8859-9" | "iso-latin-5" => "iso-latin-5",
        "latin-0" | "latin-9" | "iso-8859-15" | "iso-latin-9" => "iso-latin-9",
        "cn-gb-2312" | "euc-china" | "euc-cn" | "cn-gb" | "gb2312" | "chinese-iso-8bit" => {
            "chinese-iso-8bit"
        }
        "big5" | "cn-big5" | "cp950" | "chinese-big5" => "chinese-big5",
        "big5-hkscs" | "cn-big5-hkscs" | "chinese-big5-hkscs" => "chinese-big5-hkscs",
        "gbk" | "cp936" | "windows-936" | "chinese-gbk" => "chinese-gbk",
        "gb18030" | "chinese-gb18030" => "chinese-gb18030",
        "ascii" | "us-ascii" => "us-ascii",
        "mule-utf-8" => "utf-8",
        "emacs-internal" | "utf-8-emacs" => "utf-8-emacs",
        other => other,
    }
}

fn derive_coding_for_eol(base: &str, eol: i64) -> Option<String> {
    let suffix = match eol {
        0 => "-unix",
        1 => "-dos",
        2 => "-mac",
        _ => return None,
    };
    let derived = match base {
        "latin-1" | "iso-8859-1" | "iso-latin-1" => format!("iso-latin-1{suffix}"),
        "latin-5" | "iso-8859-9" | "iso-latin-5" => format!("iso-latin-5{suffix}"),
        "latin-0" | "latin-9" | "iso-8859-15" | "iso-latin-9" => {
            format!("iso-latin-9{suffix}")
        }
        "ascii" | "us-ascii" => format!("us-ascii{suffix}"),
        "cn-gb-2312" | "euc-china" | "euc-cn" | "cn-gb" | "gb2312" | "chinese-iso-8bit" => {
            format!("chinese-iso-8bit{suffix}")
        }
        "big5" | "cn-big5" | "cp950" | "chinese-big5" => format!("chinese-big5{suffix}"),
        "big5-hkscs" | "cn-big5-hkscs" | "chinese-big5-hkscs" => {
            format!("chinese-big5-hkscs{suffix}")
        }
        "gbk" | "cp936" | "windows-936" | "chinese-gbk" => format!("chinese-gbk{suffix}"),
        "gb18030" | "chinese-gb18030" => format!("chinese-gb18030{suffix}"),
        "mule-utf-8" | "utf-8" => format!("utf-8{suffix}"),
        "utf-8-auto" => format!("utf-8-auto{suffix}"),
        "utf-8-with-signature" => format!("utf-8-with-signature{suffix}"),
        "prefer-utf-8" => format!("prefer-utf-8{suffix}"),
        "undecided" => format!("undecided{suffix}"),
        "raw-text" => format!("raw-text{suffix}"),
        "utf-8-emacs" => format!("utf-8-emacs{suffix}"),
        "emacs-internal" => match eol {
            0 => "emacs-internal".to_string(),
            1 => "utf-8-emacs-dos".to_string(),
            2 => "utf-8-emacs-mac".to_string(),
            _ => unreachable!("validated above"),
        },
        "no-conversion" => {
            if eol == 0 {
                "no-conversion".to_string()
            } else {
                return None;
            }
        }
        "binary" => {
            if eol == 0 {
                "binary".to_string()
            } else {
                return None;
            }
        }
        // If `other` already carries an EOL suffix (e.g. utf-8-emacs-unix)
        // strip it first to avoid duplicating it (utf-8-emacs-unix-unix).
        // This can happen when the coding system was passed through a path
        // that retained the suffix before requesting a new EOL conversion,
        // or when the canonical name of a derived variant (which includes
        // the suffix) is used as the base for further derivation.
        other => {
            let base = strip_eol_suffix(other);
            format!("{base}{suffix}")
        }
    };
    Some(derived)
}

fn resolve_runtime_name(mgr: &CodingSystemManager, name: &str) -> Option<String> {
    let normalized = normalize_coding_name_for_lookup(name);
    if let Some(canonical) = mgr.resolve(normalized) {
        return Some(resolve_sym(canonical).to_string());
    }

    let eol = EolType::from_suffix(normalized)?;
    let base = strip_eol_suffix(normalized);
    let canonical_base = mgr.resolve(base)?;
    let derived = derive_coding_for_eol(resolve_sym(canonical_base), eol.to_int())?;
    if mgr.resolve(&derived).is_some() && derived.ends_with(eol.suffix()) {
        Some(derived)
    } else {
        None
    }
}

fn canonical_runtime_name(mgr: &CodingSystemManager, name: &str) -> Option<String> {
    let normalized = normalize_coding_name_for_lookup(name);
    if let Some(eol) = EolType::from_suffix(normalized) {
        let base = strip_eol_suffix(normalized);
        let canonical_base = mgr.resolve(base)?;
        let derived = derive_coding_for_eol(resolve_sym(canonical_base), eol.to_int())?;
        return mgr.resolve(&derived).map(|id| resolve_sym(id).to_string());
    }

    mgr.resolve(normalized)
        .map(|id| resolve_sym(id).to_string())
}

fn runtime_bucket_name(mgr: &CodingSystemManager, resolved_name: &str) -> Option<String> {
    let base = strip_eol_suffix(resolved_name);
    let bucket_base = properties_bucket_base(base);
    let bucket_name = mgr
        .resolve(bucket_base)
        .map(|id| resolve_sym(id).to_string())
        .unwrap_or_else(|| bucket_base.to_string());
    if mgr.is_known(bucket_name.as_str()) {
        Some(bucket_name)
    } else {
        None
    }
}

fn coding_exclude_list(exclude: Option<Value>) -> Result<Option<Vec<Value>>, Flow> {
    match exclude {
        None => Ok(None),
        Some(value) if value.is_nil() => Ok(None),
        Some(value) => super::value::list_to_vec(&value).map(Some).ok_or_else(|| {
            signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("listp"), value],
            )
        }),
    }
}

fn raw_coding_candidates(mgr: &CodingSystemManager, exclude: Option<&[Value]>) -> Vec<String> {
    let excluded: HashSet<String> = exclude
        .unwrap_or(&[])
        .iter()
        .filter_map(|value| value.as_symbol_name().map(|name| name.to_string()))
        .collect();

    let mut names: Vec<String> = mgr
        .systems
        .values()
        .filter(|info| info.eol_type == EolType::Undecided)
        .map(|info| display_base_name(strip_eol_suffix(resolve_sym(info.name))).to_string())
        .filter(|name| {
            !matches!(
                name.as_str(),
                "raw-text" | "no-conversion" | "binary" | "undecided"
            )
        })
        .filter(|name| !excluded.contains(name))
        .collect();
    names.sort();
    names.dedup();
    names
}

fn safe_coding_systems_for_text(
    mgr: &CodingSystemManager,
    text: &str,
    multibyte: bool,
    exclude: Option<Value>,
) -> Result<Value, Flow> {
    if !multibyte || text.is_ascii() {
        return Ok(Value::T);
    }

    if !text.is_ascii() {
        let exclude = coding_exclude_list(exclude)?;
        let mut safe_codings = Vec::new();
        for coding in raw_coding_candidates(mgr, exclude.as_deref()) {
            let Some(repertoire) = CodingRepertoire::for_coding_system(mgr, &coding) else {
                continue;
            };
            if text
                .chars()
                .filter(|ch| !ch.is_ascii())
                .all(|ch| repertoire.encodes(ch as i64))
            {
                safe_codings.push(Value::symbol(coding));
            }
        }
        safe_codings.push(Value::symbol("raw-text"));
        safe_codings.push(Value::symbol("no-conversion"));
        return Ok(Value::list(safe_codings));
    }

    Ok(Value::T)
}

fn marker_or_integer_position(value: &Value) -> Result<i64, Flow> {
    if value.is_marker() {
        return super::marker::marker_position_as_int(value);
    }
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(n),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integer-or-marker-p"), *value],
        )),
    }
}

pub(crate) fn builtin_find_coding_systems_region_internal(
    eval: &mut Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("find-coding-systems-region-internal", &args, 2)?;
    expect_max_args("find-coding-systems-region-internal", &args, 3)?;

    if args[0].is_string() {
        let text = coding_runtime_string(&args[0])?;
        let multibyte = args[0].string_is_multibyte();
        return safe_coding_systems_for_text(
            &eval.coding_systems,
            &text,
            multibyte,
            args.get(2).copied(),
        );
    }

    let start = marker_or_integer_position(&args[0])?;
    let end = marker_or_integer_position(&args[1])?;

    let buffer = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    if !buffer.get_multibyte() {
        return Ok(Value::T);
    }

    let char_count = buffer.total_char_len().get() as i64;
    if !(1..=char_count + 1).contains(&start) || !(1..=char_count + 1).contains(&end) || start > end
    {
        return Err(signal(
            LispCondition::ArgsOutOfRange,
            vec![args[0], args[1]],
        ));
    }

    let byte_range = EmacsByteRange::new(
        buffer.lisp_pos_to_full_buffer_emacs_byte_pos(LispCharPos1::new(start)),
        buffer.lisp_pos_to_full_buffer_emacs_byte_pos(LispCharPos1::new(end)),
    );
    // Issue #131: decode the buffer substring with `to_utf8_lossy` so real
    // Unicode (including genuine Private-Use glyphs) is preserved verbatim;
    // only true eight-bit raw bytes collapse to U+FFFD, which the per-coding
    // encode check below rejects anyway (matching GNU's raw-text fallback).
    // The legacy storage-string form mangled real PUA glyphs into eight-bit.
    let text = {
        let string = buffer.buffer_substring_lisp_string_range(byte_range);
        crate::emacs_core::emacs_char::to_utf8_lossy(string.as_bytes())
    };
    safe_coding_systems_for_text(
        &eval.coding_systems,
        &text,
        buffer.get_multibyte(),
        args.get(2).copied(),
    )
}

// ===========================================================================
// Bootstrap variables
// ===========================================================================

/// Initialize coding-system-related variables that official Emacs sets
/// in C code (coding.c syms_of_coding).
pub fn register_bootstrap_vars(obarray: &mut crate::emacs_core::symbol::Obarray) {
    fn defvar_lisp(obarray: &mut crate::emacs_core::symbol::Obarray, name: &str, value: Value) {
        obarray.set_symbol_value(name, value);
        obarray.make_special(name);
    }

    // latin-extra-code-table: 256-element nil vector (coding.c:12065).
    defvar_lisp(
        obarray,
        "latin-extra-code-table",
        Value::vector(vec![Value::NIL; 256]),
    );

    // coding.c:11927 — DEFVAR_LISP (Vcoding_system_list)
    defvar_lisp(obarray, "coding-system-list", Value::NIL);
    // coding.c:11930 — DEFVAR_LISP (Vcoding_system_alist)
    defvar_lisp(obarray, "coding-system-alist", Value::NIL);
    // `syms_of_coding` defines these two coding systems in C before
    // `mule-conf.el` adds aliases and the rest of the language codings.
    record_coding_system_name(obarray, intern("no-conversion"));
    for name in [
        "undecided-unix",
        "undecided-dos",
        "undecided-mac",
        "undecided",
    ] {
        record_coding_system_name(obarray, intern(name));
    }
    // coding.c:11935 — DEFVAR_LISP (Vcoding_category_list)
    defvar_lisp(obarray, "coding-category-list", Value::NIL);
    // coding.c:11941 — DEFVAR_LISP (Vcoding_system_for_read)
    defvar_lisp(obarray, "coding-system-for-read", Value::NIL);
    // coding.c:11949 — DEFVAR_LISP (Vcoding_system_for_write)
    defvar_lisp(obarray, "coding-system-for-write", Value::NIL);
    // coding.c:11956 — DEFVAR_LISP (Vlast_coding_system_used).
    obarray.make_special("last-coding-system-used");
    // coding.c:11959 — DEFVAR_LISP (Vlast_code_conversion_error)
    defvar_lisp(obarray, "last-code-conversion-error", Value::NIL);
    // coding.c:11999 — DEFVAR_LISP (Vlocale_coding_system)
    defvar_lisp(obarray, "locale-coding-system", Value::NIL);
    // coding.c:12014 — DEFVAR_LISP (Veol_mnemonic_unix)
    defvar_lisp(obarray, "eol-mnemonic-unix", Value::string(":"));
    // coding.c:12019 — DEFVAR_LISP (Veol_mnemonic_dos)
    defvar_lisp(obarray, "eol-mnemonic-dos", Value::string("\\"));
    // coding.c:12024 — DEFVAR_LISP (Veol_mnemonic_mac)
    defvar_lisp(obarray, "eol-mnemonic-mac", Value::string("/"));
    // coding.c:12029 — DEFVAR_LISP (Veol_mnemonic_undecided)
    defvar_lisp(obarray, "eol-mnemonic-undecided", Value::string(":"));
    // coding.c:12036 — DEFVAR_LISP (Venable_character_translation)
    defvar_lisp(obarray, "enable-character-translation", Value::T);
    // coding.c:12046 — DEFVAR_LISP (Vstandard_translation_table_for_decode)
    defvar_lisp(obarray, "standard-translation-table-for-decode", Value::NIL);
    // coding.c:12050 — DEFVAR_LISP (Vstandard_translation_table_for_encode)
    defvar_lisp(obarray, "standard-translation-table-for-encode", Value::NIL);
    // coding.c:12054 — DEFVAR_LISP (Vcharset_revision_table)
    defvar_lisp(obarray, "charset-revision-table", Value::NIL);
    // coding.c:12072 — DEFVAR_LISP (Vselect_safe_coding_system_function)
    defvar_lisp(obarray, "select-safe-coding-system-function", Value::NIL);
    // coding.c:12085 — DEFVAR_LISP (Vtranslation_table_for_input)
    defvar_lisp(obarray, "translation-table-for-input", Value::NIL);
    // coding.c:11993 — DEFVAR_LISP (Vnetwork_coding_system_alist)
    defvar_lisp(obarray, "network-coding-system-alist", Value::NIL);
    // coding.c:11996 — DEFVAR_LISP (Vprocess_coding_system_alist)
    defvar_lisp(obarray, "process-coding-system-alist", Value::NIL);
    // coding.c:12008 — DEFVAR_LISP (Vfile_coding_system_alist)
    defvar_lisp(obarray, "file-coding-system-alist", Value::NIL);

    let target_idx = intern("target-idx");
    for (operation, index) in [
        ("insert-file-contents", 0),
        ("write-region", 2),
        ("call-process", 0),
        ("call-process-region", 2),
        ("start-process", 2),
        ("open-network-stream", 3),
    ] {
        let _ = obarray.put_property_id(intern(operation), target_idx, Value::fixnum(index));
    }
}

// `set-buffer-file-coding-system' is not here.  GNU has no C version: it is
// `(defun set-buffer-file-coding-system (coding-system &optional force
// nomodify) ...)' at lisp/international/mule.el:1302, and its body does four
// things beyond the assignment -- `check-coding-system', `merge-coding-systems'
// when FORCE is nil, `buffer-file-coding-system-explicit', and
// `set-buffer-modified-p' unless NOMODIFY.  The Rust subr did the assignment
// and documented FORCE and NOMODIFY as "accepted for arity compatibility but
// currently ignored" (DIVERGENCES.md 152).

// ===========================================================================
// Tests
// ===========================================================================
#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
