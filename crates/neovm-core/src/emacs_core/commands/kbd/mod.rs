//! `kbd` string parser and event encoder (compatibility subset).
//!
//! This module implements the common `kbd` behaviors used by vm-compat:
//! - plain tokens expand to character sequences,
//! - modifier prefixes (`C-`, `M-`, `S-`, `s-`) on single characters,
//! - angle-bracket symbolic events (`<f1>`, `C-<return>`, ...),
//! - string return when all events are plain chars, otherwise vector.

use super::{
    intern::{intern, resolve_sym},
    keymap::KeyEvent,
    value::{Value, ValueKind, VecLikeType},
};
use strum::{EnumString, IntoStaticStr};

use super::keyboard::pure::{
    KEY_CHAR_ALT as CHAR_ALT, KEY_CHAR_CTRL as CHAR_CTL, KEY_CHAR_HYPER as CHAR_HYPER,
    KEY_CHAR_META as CHAR_META, KEY_CHAR_MOD_MASK as CHAR_MODIFIER_MASK,
    KEY_CHAR_SHIFT as CHAR_SHIFT, KEY_CHAR_SUPER as CHAR_SUPER,
};

#[derive(Clone, Debug)]
pub(crate) enum KeyDesignatorError {
    WrongType(Value),
    Parse(String),
}

#[derive(Clone, Copy, Default)]
struct Modifiers {
    ctrl: bool,
    meta: bool,
    shift: bool,
    super_: bool,
    hyper: bool,
    alt: bool,
}

impl Modifiers {
    fn any(self) -> bool {
        self.ctrl || self.meta || self.shift || self.super_ || self.hyper || self.alt
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
enum EventModifier {
    Control,
    Meta,
    Shift,
    Super,
    Hyper,
    Alt,
}

impl EventModifier {
    const KBD_PREFIX_ORDER: [Self; 6] = [
        Self::Control,
        Self::Meta,
        Self::Shift,
        Self::Super,
        Self::Hyper,
        Self::Alt,
    ];

    fn from_lispy_symbol(name: &str) -> Option<Self> {
        name.parse().ok()
    }

    #[cfg(test)]
    fn lispy_symbol(self) -> &'static str {
        self.into()
    }

    fn kbd_prefix(self) -> &'static str {
        match self {
            Self::Control => "C-",
            Self::Meta => "M-",
            Self::Shift => "S-",
            Self::Super => "s-",
            Self::Hyper => "H-",
            Self::Alt => "A-",
        }
    }

    fn strip_kbd_prefix(self, token: &str) -> Option<&str> {
        let rest = token.strip_prefix(self.kbd_prefix())?;
        (!rest.is_empty()).then_some(rest)
    }

    fn set(self, mods: &mut Modifiers) {
        match self {
            Self::Control => mods.ctrl = true,
            Self::Meta => mods.meta = true,
            Self::Shift => mods.shift = true,
            Self::Super => mods.super_ = true,
            Self::Hyper => mods.hyper = true,
            Self::Alt => mods.alt = true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum EncodedEvent {
    Char(char),
    Int(i64),
    Symbol(String),
}

pub(crate) fn parse_kbd_string(desc: &str) -> Result<Value, String> {
    let trimmed = desc.trim();
    if trimmed.is_empty() {
        return Ok(Value::string(""));
    }

    let mut encoded = Vec::new();
    for token in trimmed.split_whitespace() {
        parse_token(token, &mut encoded)?;
    }

    if encoded.iter().all(|e| matches!(e, EncodedEvent::Char(_))) {
        let s: String = encoded
            .into_iter()
            .map(|event| match event {
                EncodedEvent::Char(c) => c,
                _ => unreachable!("guarded by all(Char)"),
            })
            .collect();
        return Ok(Value::string(s));
    }

    let values = encoded
        .into_iter()
        .map(|event| match event {
            EncodedEvent::Char(c) => Value::fixnum(c as i64),
            EncodedEvent::Int(n) => Value::fixnum(n),
            EncodedEvent::Symbol(name) => Value::symbol(name),
        })
        .collect();
    Ok(Value::vector(values))
}

pub(crate) fn key_events_from_designator(
    designator: &Value,
) -> Result<Vec<KeyEvent>, KeyDesignatorError> {
    match designator.kind() {
        ValueKind::String => {
            // For strings used as key sequences (define-key, lookup-key, etc.),
            // each character IS a key event — no kbd-style text parsing.
            // Decode the original Lisp-string bytes directly so raw unibyte
            // meta bytes like "\M-v" survive intact instead of turning into
            // U+FFFD via lossy UTF-8 conversion.
            decode_string_key_events(designator).map_err(KeyDesignatorError::Parse)
        }
        ValueKind::Veclike(VecLikeType::Vector) => {
            decode_encoded_key_events(designator).map_err(KeyDesignatorError::Parse)
        }
        _other => Err(KeyDesignatorError::WrongType(*designator)),
    }
}

fn decode_encoded_key_events(encoded: &Value) -> Result<Vec<KeyEvent>, String> {
    match encoded.kind() {
        ValueKind::String => decode_string_key_events(encoded),
        ValueKind::Veclike(VecLikeType::Vector) => {
            let items = encoded.as_vector_data().unwrap().clone();
            items.iter().map(decode_vector_event).collect()
        }
        _other => Err(format!(
            "expected kbd-encoded string or vector, got {}",
            encoded.type_name()
        )),
    }
}

fn decode_string_key_events(value: &Value) -> Result<Vec<KeyEvent>, String> {
    let ls = value
        .as_lisp_string()
        .ok_or_else(|| format!("expected string key designator, got {}", value.type_name()))?;
    let mut out = Vec::with_capacity(ls.schars());

    if ls.is_multibyte() {
        let mut pos = 0usize;
        while pos < ls.as_bytes().len() {
            let (code, len) = super::emacs_char::string_char(&ls.as_bytes()[pos..]);
            out.push(key_event_from_char_code(code));
            pos += len;
        }
    } else {
        for &byte in ls.as_bytes() {
            out.push(key_event_from_char_code(
                super::emacs_char::unibyte_to_char(byte),
            ));
        }
    }

    Ok(out)
}

fn key_event_from_char_code(code: u32) -> KeyEvent {
    if super::emacs_char::char_byte8_p(code) {
        let byte = super::emacs_char::char_to_byte8(code);
        return key_event_from_unibyte(byte);
    }

    // Key sequence strings treat character codes 128..255 as Meta + low 7 bits.
    if (0x80..=0xFF).contains(&code) {
        return key_event_from_unibyte(code as u8);
    }

    KeyEvent::Char {
        code: char::from_u32(code).unwrap_or('\u{FFFD}'),
        ctrl: false,
        meta: false,
        shift: false,
        super_: false,
        hyper: false,
        alt: false,
    }
}

fn key_event_from_unibyte(byte: u8) -> KeyEvent {
    if byte >= 0x80 {
        let base = char::from_u32((byte - 0x80) as u32).expect("ASCII byte must decode");
        KeyEvent::Char {
            code: base,
            ctrl: false,
            meta: true,
            shift: false,
            super_: false,
            hyper: false,
            alt: false,
        }
    } else {
        let base = char::from_u32(byte as u32).expect("byte must decode");
        KeyEvent::Char {
            code: base,
            ctrl: false,
            meta: false,
            shift: false,
            super_: false,
            hyper: false,
            alt: false,
        }
    }
}

fn decode_vector_event(item: &Value) -> Result<KeyEvent, String> {
    match item.kind() {
        ValueKind::Fixnum(n) => decode_int_event(n),
        ValueKind::Symbol(id) => decode_symbol_event(resolve_sym(id)),
        ValueKind::Nil => decode_symbol_event("nil"),
        ValueKind::T => decode_symbol_event("t"),
        // Event modifier list: (MODIFIER... BASE-EVENT)
        // e.g. (control ??) => Ctrl+?, (meta control ?a) => M-C-a
        ValueKind::Cons => decode_event_modifier_list(item),
        _other => Err(format!(
            "invalid key vector element type: {}",
            item.type_name()
        )),
    }
}

/// Decode an event modifier list like `(control ??)` or `(meta control ?a)`.
/// In official Emacs, key vectors can contain lists where leading symbols are
/// modifier names and the last element is the base event (char or symbol).
fn decode_event_modifier_list(list: &Value) -> Result<KeyEvent, String> {
    let mut mods = Modifiers::default();
    let mut cursor = *list;

    // Walk the list, collecting modifier symbols
    loop {
        match cursor.kind() {
            ValueKind::Cons => {
                let pair_car = cursor.cons_car();
                let pair_cdr = cursor.cons_cdr();
                match pair_car.kind() {
                    ValueKind::Symbol(id) => {
                        let name = resolve_sym(id);
                        if let Some(modifier) = EventModifier::from_lispy_symbol(name) {
                            modifier.set(&mut mods);
                            cursor = pair_cdr;
                        } else {
                            // Not a modifier — this symbol IS the base event
                            // and cdr should be nil.
                            if pair_cdr.is_nil() {
                                return Ok(apply_mods_to_event(decode_symbol_event(name)?, mods));
                            }
                            return Err(format!("unknown modifier in event list: {name}"));
                        }
                    }
                    ValueKind::Fixnum(n) => {
                        // Base event is a character code
                        let base = decode_int_event(n)?;
                        return Ok(apply_mods_to_event(base, mods));
                    }
                    _other => {
                        return Err(format!(
                            "invalid base event in modifier list: {}",
                            pair_car.type_name()
                        ));
                    }
                }
            }
            ValueKind::Nil => {
                return Err("empty event modifier list".to_string());
            }
            // Last element as a dotted pair base event
            ValueKind::Fixnum(n) => {
                let base = decode_int_event(n)?;
                return Ok(apply_mods_to_event(base, mods));
            }
            ValueKind::Symbol(id) => {
                return Ok(apply_mods_to_event(
                    decode_symbol_event(resolve_sym(id))?,
                    mods,
                ));
            }
            _other => {
                return Err(format!(
                    "invalid base event in modifier list: {}",
                    cursor.type_name()
                ));
            }
        }
    }
}

/// Apply additional modifiers to a decoded KeyEvent.
fn apply_mods_to_event(event: KeyEvent, mods: Modifiers) -> KeyEvent {
    match event {
        KeyEvent::Char {
            code,
            ctrl,
            meta,
            shift,
            super_,
            hyper,
            alt,
        } => KeyEvent::Char {
            code,
            ctrl: ctrl || mods.ctrl,
            meta: meta || mods.meta,
            shift: shift || mods.shift,
            super_: super_ || mods.super_,
            hyper: hyper || mods.hyper,
            alt: alt || mods.alt,
        },
        KeyEvent::Function {
            name,
            ctrl,
            meta,
            shift,
            super_,
            hyper,
            alt,
        } => KeyEvent::Function {
            name,
            ctrl: ctrl || mods.ctrl,
            meta: meta || mods.meta,
            shift: shift || mods.shift,
            super_: super_ || mods.super_,
            hyper: hyper || mods.hyper,
            alt: alt || mods.alt,
        },
    }
}

fn decode_int_event(code: i64) -> Result<KeyEvent, String> {
    let mods = code & CHAR_MODIFIER_MASK;
    let base = code & !CHAR_MODIFIER_MASK;
    if !(0..=0x10FFFF).contains(&base) {
        return Err(format!("invalid key event code: {code}"));
    }
    let ch =
        char::from_u32(base as u32).ok_or_else(|| format!("invalid key event code: {code}"))?;
    Ok(KeyEvent::Char {
        code: ch,
        ctrl: (mods & CHAR_CTL) != 0,
        meta: (mods & CHAR_META) != 0,
        shift: (mods & CHAR_SHIFT) != 0,
        super_: (mods & CHAR_SUPER) != 0,
        hyper: (mods & CHAR_HYPER) != 0,
        alt: (mods & CHAR_ALT) != 0,
    })
}

fn decode_symbol_event(symbol: &str) -> Result<KeyEvent, String> {
    let (mods, _prefix, remainder) = parse_modifiers(symbol);
    if remainder.is_empty() {
        return Err("invalid empty key symbol".to_string());
    }
    Ok(KeyEvent::Function {
        name: intern(remainder),
        ctrl: mods.ctrl,
        meta: mods.meta,
        shift: mods.shift,
        super_: mods.super_,
        hyper: mods.hyper,
        alt: mods.alt,
    })
}

fn parse_token(token: &str, out: &mut Vec<EncodedEvent>) -> Result<(), String> {
    let (mods, prefix, remainder) = parse_modifiers(token);

    if let Some(name) = parse_angle_symbol(remainder) {
        out.push(EncodedEvent::Symbol(format!("{prefix}{name}")));
        return Ok(());
    }

    if let Some(ch) = named_char_token(remainder) {
        out.push(encode_char(ch, mods, false));
        return Ok(());
    }

    if let Some(ch) = single_char(remainder) {
        out.push(encode_char(ch, mods, true));
        return Ok(());
    }

    if mods.any() {
        return Err(format!(
            "{prefix} must prefix a single character, not {remainder}"
        ));
    }

    out.extend(remainder.chars().map(EncodedEvent::Char));
    Ok(())
}

fn parse_modifiers(mut token: &str) -> (Modifiers, String, &str) {
    let mut mods = Modifiers::default();
    let mut prefix = String::new();

    loop {
        let mut consumed = false;
        for modifier in EventModifier::KBD_PREFIX_ORDER {
            if let Some(rest) = modifier.strip_kbd_prefix(token) {
                modifier.set(&mut mods);
                prefix.push_str(modifier.kbd_prefix());
                token = rest;
                consumed = true;
                break;
            }
        }
        if consumed {
            continue;
        }
        break;
    }

    (mods, prefix, token)
}

fn parse_angle_symbol(token: &str) -> Option<&str> {
    let inner = token.strip_prefix('<')?.strip_suffix('>')?;
    if inner.is_empty() { None } else { Some(inner) }
}

fn named_char_token(token: &str) -> Option<char> {
    match token {
        "NUL" => Some('\0'),
        "RET" | "return" => Some('\r'),
        "LFD" => Some('\n'),
        "TAB" | "tab" => Some('\t'),
        "SPC" | "space" => Some(' '),
        "ESC" | "escape" => Some('\u{1b}'),
        "DEL" | "delete" => Some('\u{7f}'),
        _ => None,
    }
}

fn single_char(token: &str) -> Option<char> {
    let mut chars = token.chars();
    let ch = chars.next()?;
    if chars.next().is_none() {
        Some(ch)
    } else {
        None
    }
}

fn encode_char(ch: char, mods: Modifiers, allow_ctrl_resolution: bool) -> EncodedEvent {
    if !mods.any() {
        return EncodedEvent::Char(ch);
    }

    let mut base = ch as i64;
    let mut ctrl = mods.ctrl;

    if ctrl
        && allow_ctrl_resolution
        && let Some(resolved) = resolve_control_char(ch)
    {
        base = resolved;
        ctrl = false;
    }

    if !mods.meta && !mods.shift && !mods.super_ && !mods.hyper && !mods.alt && !ctrl {
        if let Some(resolved) = char::from_u32(base as u32) {
            return EncodedEvent::Char(resolved);
        }
        return EncodedEvent::Int(base);
    }

    let mut code = base;
    if mods.meta {
        code |= CHAR_META;
    }
    if ctrl {
        code |= CHAR_CTL;
    }
    if mods.shift {
        code |= CHAR_SHIFT;
    }
    if mods.super_ {
        code |= CHAR_SUPER;
    }
    if mods.hyper {
        code |= CHAR_HYPER;
    }
    if mods.alt {
        code |= CHAR_ALT;
    }
    EncodedEvent::Int(code)
}

fn resolve_control_char(ch: char) -> Option<i64> {
    if ch.is_ascii_alphabetic() {
        return Some(((ch.to_ascii_uppercase() as u8) & 0x1F) as i64);
    }
    if ('@'..='_').contains(&ch) && ch != '?' {
        return Some(((ch as u8) & 0x1F) as i64);
    }
    None
}
#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
