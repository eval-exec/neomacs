use crate::emacs_core::emacs_char::EmacsChar;
use crate::emacs_core::error::LispCondition;
use crate::emacs_core::{
    error::{Flow, signal},
    intern::resolve_sym,
    value::{Value, ValueKind, VecLikeType, list_to_vec},
};

pub(crate) const KEY_CHAR_META: i64 = 0x8000000;
pub(crate) const KEY_CHAR_CTRL: i64 = 0x4000000;
pub(crate) const KEY_CHAR_SHIFT: i64 = 0x2000000;
pub(crate) const KEY_CHAR_SUPER: i64 = 0x0800000;
pub(crate) const KEY_CHAR_HYPER: i64 = 0x1000000;
pub(crate) const KEY_CHAR_ALT: i64 = 0x0400000;
pub(crate) const KEY_CHAR_MOD_MASK: i64 =
    KEY_CHAR_META | KEY_CHAR_CTRL | KEY_CHAR_SHIFT | KEY_CHAR_SUPER | KEY_CHAR_HYPER | KEY_CHAR_ALT;
pub(crate) const KEY_CHAR_CODE_MASK: i64 = 0x3FFFFF;
const EVENT_MOD_UP: i64 = 1 << 0;
const EVENT_MOD_DOWN: i64 = 1 << 1;
const EVENT_MOD_DRAG: i64 = 1 << 2;
const EVENT_MOD_CLICK: i64 = 1 << 3;
const EVENT_MOD_DOUBLE: i64 = 1 << 4;
const EVENT_MOD_TRIPLE: i64 = 1 << 5;

fn event_char_code(event: &Value) -> Option<i64> {
    match event.kind() {
        ValueKind::Fixnum(ch) => Some(i64::from(ch as u32)),
        _ => None,
    }
}

fn event_char_fits_in_gnu_event_string(code: i64) -> bool {
    let string_char_mask = KEY_CHAR_META - 1;
    (code & string_char_mask) < 0o200
}

pub(crate) fn make_event_array_value(events: &[Value]) -> Value {
    let mut bytes = Vec::with_capacity(events.len());

    for event in events {
        let Some(code) = event_char_code(event) else {
            return Value::vector(events.to_vec());
        };
        if !event_char_fits_in_gnu_event_string(code) {
            return Value::vector(events.to_vec());
        }

        let mut byte = (code & (KEY_CHAR_META - 1)) as u8;
        if (code & KEY_CHAR_META) != 0 {
            byte |= 0x80;
        }
        bytes.push(byte);
    }

    Value::heap_string(crate::heap_types::LispString::from_unibyte(bytes))
}

fn invalid_single_key_error() -> Flow {
    signal(
        "error",
        vec![Value::string(
            "KEY must be an integer, cons, symbol, or string",
        )],
    )
}

fn control_char_suffix(code: i64) -> Option<char> {
    match code {
        0 => Some('@'),
        1..=26 => char::from_u32((code as u32) + 96),
        28 => Some('\\'),
        29 => Some(']'),
        30 => Some('^'),
        31 => Some('_'),
        _ => None,
    }
}

fn named_char_name(code: i64) -> Option<&'static str> {
    match code {
        9 => Some("TAB"),
        13 => Some("RET"),
        27 => Some("ESC"),
        32 => Some("SPC"),
        127 => Some("DEL"),
        _ => None,
    }
}

pub(crate) fn split_symbol_modifiers(mut name: &str) -> (String, &str) {
    let mut prefix = String::new();
    let is_single_char = |s: &str| {
        let mut chars = s.chars();
        chars.next().is_some() && chars.next().is_none()
    };
    loop {
        if let Some(rest) = name.strip_prefix("C-") {
            if is_single_char(rest) {
                break;
            }
            prefix.push_str("C-");
            name = rest;
            continue;
        }
        if let Some(rest) = name.strip_prefix("M-") {
            if is_single_char(rest) {
                break;
            }
            prefix.push_str("M-");
            name = rest;
            continue;
        }
        if let Some(rest) = name.strip_prefix("S-") {
            if is_single_char(rest) {
                break;
            }
            prefix.push_str("S-");
            name = rest;
            continue;
        }
        if let Some(rest) = name.strip_prefix("s-") {
            if is_single_char(rest) {
                break;
            }
            prefix.push_str("s-");
            name = rest;
            continue;
        }
        if let Some(rest) = name.strip_prefix("H-") {
            if is_single_char(rest) {
                break;
            }
            prefix.push_str("H-");
            name = rest;
            continue;
        }
        if let Some(rest) = name.strip_prefix("A-") {
            if is_single_char(rest) {
                break;
            }
            prefix.push_str("A-");
            name = rest;
            continue;
        }
        break;
    }
    (prefix, name)
}

fn describe_symbol_key(name: &str, no_angles: bool) -> Vec<u8> {
    let (prefix, base) = split_symbol_modifiers(name);
    if no_angles {
        return format!("{prefix}{base}").into_bytes();
    }
    format!("{prefix}<{base}>").into_bytes()
}

/// Append CODE to OUT in Emacs's internal multibyte encoding, GNU's
/// `CHAR_STRING` (character.c:101-140).
///
/// A key description is Emacs TEXT, not a Rust `String`: an eight-bit
/// raw-byte character (`#x3FFF80`..`#x3FFFFF`) and a non-Unicode code are
/// perfectly good Emacs characters but are not Unicode scalar values, so
/// pushing them through `char` or `String` is exactly the loss `EmacsChar`
/// exists to prevent (issue #131). GNU encodes a raw byte back to its single
/// byte via `CHAR_TO_BYTE8` + `BYTE8_STRING` and the description holds that
/// one character.
fn push_emacs_char(out: &mut Vec<u8>, code: EmacsChar) {
    let mut buf = [0u8; crate::emacs_core::emacs_char::MAX_MULTIBYTE_LENGTH];
    let len = code.char_string(&mut buf);
    out.extend_from_slice(&buf[..len]);
}

fn describe_int_key(code: i64) -> Result<Vec<u8>, Flow> {
    let mods = code & KEY_CHAR_MOD_MASK;
    let base = code & !KEY_CHAR_MOD_MASK;
    if !(0..=KEY_CHAR_CODE_MASK).contains(&base) {
        return Err(invalid_single_key_error());
    }

    let ctrl = (mods & KEY_CHAR_CTRL) != 0;
    let meta = (mods & KEY_CHAR_META) != 0;
    let shift = (mods & KEY_CHAR_SHIFT) != 0;
    let super_ = (mods & KEY_CHAR_SUPER) != 0;

    let push_prefixes = |out: &mut Vec<u8>, with_ctrl: bool| {
        if (mods & KEY_CHAR_ALT) != 0 {
            out.extend_from_slice(b"A-");
        }
        if with_ctrl {
            out.extend_from_slice(b"C-");
        }
        if (mods & KEY_CHAR_HYPER) != 0 {
            out.extend_from_slice(b"H-");
        }
        if meta {
            out.extend_from_slice(b"M-");
        }
        if shift {
            out.extend_from_slice(b"S-");
        }
        if super_ {
            out.extend_from_slice(b"s-");
        }
    };

    let mut out = Vec::new();

    // Emacs renders M-TAB style integer events through control notation (`C-M-i`),
    // while plain/shift/super/alt TAB keeps named `TAB` rendering.
    let tab_meta_control_notation = base == 9 && meta;
    if !tab_meta_control_notation && let Some(name) = named_char_name(base) {
        push_prefixes(&mut out, ctrl);
        out.extend_from_slice(name.as_bytes());
        return Ok(out);
    }

    if let Some(sfx) = control_char_suffix(base) {
        push_prefixes(&mut out, true);
        out.push(sfx.to_ascii_lowercase() as u8);
        return Ok(out);
    }

    push_prefixes(&mut out, ctrl);
    // GNU reaches `CHAR_STRING` for every remaining code, raw byte or not
    // (keymap.c:2296-2301); `base` is already range-checked above.
    let Some(base_char) = EmacsChar::from_code(base as u32) else {
        return Err(invalid_single_key_error());
    };
    push_emacs_char(&mut out, base_char);
    Ok(out)
}

/// A single key event's description, as Emacs-encoded BYTES.
///
/// Bytes rather than a `String` because a description can contain characters
/// that are not Unicode scalar values -- an eight-bit raw byte is the whole
/// point of ledger entry 56. Callers that need Lisp text build a multibyte
/// string from these bytes, as GNU's `Fsingle_key_description` does with
/// `make_specified_string (..., multibyte=1)`; callers that only want a Rust
/// string for a message or a log convert lossily at their own call site, where
/// the loss is visible.
pub(crate) fn describe_single_key_value(value: &Value, no_angles: bool) -> Result<Vec<u8>, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => describe_int_key(n),
        ValueKind::Symbol(id) => Ok(describe_symbol_key(resolve_sym(id), no_angles)),
        ValueKind::T => Ok(describe_symbol_key("t", no_angles)),
        ValueKind::Nil => Ok(describe_symbol_key("nil", no_angles)),
        // A string key description is already Emacs text: pass its bytes
        // through rather than decoding, so a raw byte in it survives too.
        ValueKind::String => Ok(value
            .as_lisp_string()
            .map(|ls| ls.as_bytes().to_vec())
            .expect("ValueKind::String must carry LispString payload")),
        ValueKind::Cons => {
            // A cons of two fixnums is AN INTERVAL FROM A MAP-CHAR-TABLE, and
            // GNU renders it FROM..TO (keymap.c:2322-2329). It has no
            // "(MOD . CHAR) modifier event" case: measured on GNU 31.0.90,
            // (single-key-description (cons 1 ?x)) is "C-a..x", not "S-x".
            //
            // describe-bindings depends on this. The widest key in the global
            // section is a raw-byte range from the self-insert-command
            // char-table, so rendering it as one modified character narrows
            // the section and shifts describe-map--align-section's column for
            // every row beneath it.
            if let (Some(_), Some(_)) = (value.cons_car().as_fixnum(), value.cons_cdr().as_fixnum())
            {
                let mut out = describe_single_key_value(&value.cons_car(), no_angles)?;
                out.extend_from_slice(b"..");
                out.extend_from_slice(&describe_single_key_value(&value.cons_cdr(), no_angles)?);
                return Ok(out);
            }
            let items = list_to_vec(value).ok_or_else(invalid_single_key_error)?;
            if items.len() == 1 {
                return describe_single_key_value(&items[0], no_angles);
            }
            // Lucid-style event list, e.g. (meta shift up) — convert first
            if let Some(converted) = convert_lucid_event_list(&items) {
                return describe_single_key_value(&converted, no_angles);
            }
            Err(invalid_single_key_error())
        }
        _ => Err(invalid_single_key_error()),
    }
}

pub(crate) fn key_sequence_values(value: &Value) -> Result<Vec<Value>, Flow> {
    match value.kind() {
        ValueKind::Nil => Ok(vec![]),
        ValueKind::String => {
            let string = value.as_lisp_string().expect("string");
            if !string.is_multibyte() {
                return Ok(string
                    .as_bytes()
                    .iter()
                    .map(|&byte| {
                        let code = if byte & 0x80 != 0 {
                            KEY_CHAR_META | i64::from(byte & 0x7f)
                        } else {
                            i64::from(byte)
                        };
                        Value::fixnum(code)
                    })
                    .collect());
            }
            Ok(crate::emacs_core::builtins::lisp_string_char_codes(string)
                .into_iter()
                .map(|code| Value::fixnum(code as i64))
                .collect())
        }
        ValueKind::Veclike(VecLikeType::Vector) => {
            let elems = value.as_vector_data().unwrap().clone();
            // Convert any Lucid-style event lists inside the vector
            let converted: Vec<Value> = elems
                .into_iter()
                .map(|e| {
                    if e.is_cons()
                        && let Some(items) = list_to_vec(&e)
                        && items.len() > 1
                        && let Some(c) = convert_lucid_event_list(&items)
                    {
                        return c;
                    }
                    e
                })
                .collect();
            Ok(converted)
        }
        ValueKind::Cons => list_to_vec(value).ok_or_else(|| {
            signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("sequencep"), *value],
            )
        }),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("sequencep"), *value],
        )),
    }
}

pub(crate) fn resolve_control_code(code: i64) -> Option<i64> {
    match code {
        32 => Some(0),               // SPC
        63 => Some(127),             // ?
        64 => Some(0),               // @
        65..=90 => Some(code - 64),  // A-Z
        91 => Some(27),              // [
        92 => Some(28),              // \
        93 => Some(29),              // ]
        94 => Some(30),              // ^
        95 => Some(31),              // _
        97..=122 => Some(code - 96), // a-z
        _ => None,
    }
}

pub(crate) fn event_modifier_bit(symbol: &str) -> Option<i64> {
    match symbol {
        "C" | "ctrl" | "control" => Some(KEY_CHAR_CTRL),
        "M" | "meta" => Some(KEY_CHAR_META),
        "S" | "shift" => Some(KEY_CHAR_SHIFT),
        "s" | "super" => Some(KEY_CHAR_SUPER),
        "H" | "hyper" => Some(KEY_CHAR_HYPER),
        "A" | "alt" => Some(KEY_CHAR_ALT),
        "up" => Some(EVENT_MOD_UP),
        "down" => Some(EVENT_MOD_DOWN),
        "drag" => Some(EVENT_MOD_DRAG),
        "click" => Some(EVENT_MOD_CLICK),
        "double" => Some(EVENT_MOD_DOUBLE),
        "triple" => Some(EVENT_MOD_TRIPLE),
        _ => None,
    }
}

fn parse_written_event_modifier(name: &str) -> Option<(i64, &str)> {
    [
        ("A-", KEY_CHAR_ALT),
        ("C-", KEY_CHAR_CTRL),
        ("H-", KEY_CHAR_HYPER),
        ("M-", KEY_CHAR_META),
        ("S-", KEY_CHAR_SHIFT),
        ("s-", KEY_CHAR_SUPER),
        ("double-", EVENT_MOD_DOUBLE),
        ("triple-", EVENT_MOD_TRIPLE),
        ("up-", EVENT_MOD_UP),
        ("down-", EVENT_MOD_DOWN),
        ("drag-", EVENT_MOD_DRAG),
    ]
    .into_iter()
    .find_map(|(prefix, bit)| name.strip_prefix(prefix).map(|rest| (bit, rest)))
}

pub(crate) fn reorder_event_symbol_modifiers(value: Value) -> Value {
    let Some(name) = value.as_symbol_name() else {
        return value;
    };

    let mut rest = name;
    let mut modifiers = 0;
    let mut found_modifier = false;
    while let Some((modifier, next)) = parse_written_event_modifier(rest) {
        modifiers |= modifier;
        rest = next;
        found_modifier = true;
    }

    if !found_modifier {
        return value;
    }

    let canonical = format!("{}{}", event_modifier_prefix(modifiers), rest);
    if canonical == name {
        value
    } else {
        Value::symbol(canonical)
    }
}

fn lucid_symbol_char_base(symbol: &str) -> Option<i64> {
    let mut chars = symbol.chars();
    let ch = chars.next()?;
    if chars.next().is_none() {
        Some(ch as i64)
    } else {
        None
    }
}

/// Convert a Lucid-style event list (e.g. `(meta shift up)`) to a single
/// event value.  Returns `None` when the list is not a valid Lucid event
/// (i.e. it contains non-modifier, non-base elements, or has no base).
/// This mirrors GNU Emacs `Fevent_convert_list` (keyboard.c).
pub(crate) fn convert_lucid_event_list(items: &[Value]) -> Option<Value> {
    if items.is_empty() {
        return None;
    }

    let mut mod_bits = 0i64;
    let mut base: Option<Value> = None;
    for (idx, item) in items.iter().enumerate() {
        if base.is_none() {
            if let Some(sym) = item.as_symbol_name()
                && idx + 1 < items.len()
                && let Some(bit) = event_modifier_bit(sym)
            {
                mod_bits |= bit;
                continue;
            }
            base = Some(*item);
        } else {
            // More than one non-modifier element — not a valid Lucid list
            return None;
        }
    }

    let mut base = base?;

    if let ValueKind::Symbol(id) = base.kind()
        && let Some(code) = lucid_symbol_char_base(resolve_sym(id))
    {
        base = Value::fixnum(code);
    }

    match base.kind() {
        ValueKind::Fixnum(_) => {
            let mut code = match base.kind() {
                ValueKind::Fixnum(i) => i,
                _ => unreachable!(),
            };

            let ctrl = (mod_bits & KEY_CHAR_CTRL) != 0;
            let shift = (mod_bits & KEY_CHAR_SHIFT) != 0;

            if shift && !ctrl && (97..=122).contains(&code) {
                code -= 32;
                mod_bits &= !KEY_CHAR_SHIFT;
            }
            if ctrl && code <= 31 {
                mod_bits &= !KEY_CHAR_CTRL;
            }
            if ctrl
                && code != 32
                && code != 63
                && let Some(resolved) = resolve_control_code(code)
            {
                if (65..=90).contains(&code) {
                    mod_bits |= KEY_CHAR_SHIFT;
                }
                code = resolved;
                mod_bits &= !KEY_CHAR_CTRL;
            }
            Some(Value::fixnum(code | mod_bits))
        }
        ValueKind::Symbol(id) => {
            let name = resolve_sym(id);
            if mod_bits == 0 {
                Some(Value::symbol(name))
            } else {
                Some(Value::symbol(format!(
                    "{}{}",
                    event_modifier_prefix(mod_bits),
                    name
                )))
            }
        }
        _ => None,
    }
}

pub(crate) fn event_modifier_prefix(bits: i64) -> String {
    let mut out = String::new();
    if (bits & KEY_CHAR_ALT) != 0 {
        out.push_str("A-");
    }
    if (bits & KEY_CHAR_CTRL) != 0 {
        out.push_str("C-");
    }
    if (bits & KEY_CHAR_HYPER) != 0 {
        out.push_str("H-");
    }
    if (bits & KEY_CHAR_META) != 0 {
        out.push_str("M-");
    }
    if (bits & KEY_CHAR_SHIFT) != 0 {
        out.push_str("S-");
    }
    if (bits & KEY_CHAR_SUPER) != 0 {
        out.push_str("s-");
    }
    if (bits & EVENT_MOD_DOUBLE) != 0 {
        out.push_str("double-");
    }
    if (bits & EVENT_MOD_TRIPLE) != 0 {
        out.push_str("triple-");
    }
    if (bits & EVENT_MOD_UP) != 0 {
        out.push_str("up-");
    }
    if (bits & EVENT_MOD_DOWN) != 0 {
        out.push_str("down-");
    }
    if (bits & EVENT_MOD_DRAG) != 0 {
        out.push_str("drag-");
    }
    // The `click' modifier is denoted by the absence of down/drag/etc.
    out
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn basic_char_code(mut code: i64) -> i64 {
    code &= KEY_CHAR_CODE_MASK;
    match code {
        0 => 64,
        1..=26 => code + 96,
        27..=31 => code + 64,
        65..=90 => code + 32,
        _ => code,
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn symbol_has_modifier_prefix(name: &str) -> bool {
    name.starts_with("C-")
        || name.starts_with("M-")
        || name.starts_with("S-")
        || name.starts_with("s-")
        || name.starts_with("H-")
        || name.starts_with("A-")
}

pub(crate) fn register_bootstrap_vars(obarray: &mut crate::emacs_core::symbol::Obarray) {
    use crate::emacs_core::value::Value;

    // keyboard.c:13991 DEFVAR_LISP at `13988',
    // `XSETINT (menu_prompt_more_char, ' ')' -- the character code 32, not the
    // string " ": `read_char_minibuf_menu_prompt' tests the event with `EQ'
    // and then, via `FIXNUMP' + `Ctl (XFIXNUM (...))', accepts its control
    // variant too (`keyboard.c:10370-10372'), so only a fixnum works.
    obarray.define_special_variable("menu-prompt-more-char", Value::fixnum(32));
    // keyboard.c:14147 DEFVAR_KBOARD, `kset_system_key_alist (kb, Qnil)' at
    // `keyboard.c:13123' for every new kboard.  GNU gives it a separate binding
    // per terminal device; this port has no per-kboard storage, so it is
    // modelled as one global special the way `overriding-terminal-local-map'
    // (the other `DEFVAR_KBOARD' in this file) already is.  The per-terminal
    // half of the semantics is a known gap, not something a nil default hides.
    obarray.define_special_variable("system-key-alist", Value::NIL);
    // GNU `src/keyboard.c` defines these with DEFVAR_LISP.
    obarray.set_symbol_value("help-char", Value::fixnum(8));
    obarray.make_special("help-char");
    obarray.set_symbol_value("help-form", Value::NIL);
    obarray.make_special("help-form");
    obarray.set_symbol_value("help-event-list", Value::NIL);
    obarray.make_special("help-event-list");
    obarray.set_symbol_value("suggest-key-bindings", Value::T);
    // keyboard.c:14228 / 14224 -- DEFVAR_LISP, init nil.
    obarray.define_special_variable("timer-idle-list", Value::NIL);
    obarray.define_special_variable("timer-list", Value::NIL);
    obarray.set_symbol_value("input-method-previous-message", Value::NIL);
    obarray.make_special("input-method-previous-message");
    // keyboard.c:13841 DEFVAR_INT, init 300.
    obarray.define_int_variable("auto-save-interval", 300);
    // keyboard.c:13850 DEFVAR_LISP, XSETFASTINT 30.
    obarray.define_special_variable("auto-save-timeout", Value::fixnum(30));
    obarray.set_symbol_value("echo-keystrokes", Value::fixnum(1));
    obarray.make_special("echo-keystrokes");
    // keyboard.c:13869 DEFVAR_LISP, make_float (2.0) -- a float, not a fixnum.
    obarray.define_special_variable("polling-period", Value::make_float(2.0));
    // keyboard.c:13876 DEFVAR_LISP, make_fixnum 500.
    obarray.define_special_variable("double-click-time", Value::fixnum(500));
    // keyboard.c:13886 DEFVAR_INT, init 3.
    obarray.define_int_variable("double-click-fuzz", 3);
    // keyboard.c:13897 / 13903 DEFVAR_INT, init 0.
    obarray.define_int_variable("num-input-keys", 0);
    obarray.define_int_variable("num-nonmacro-input-events", 0);
    // keyboard.c:13908 DEFVAR_LISP, init nil.
    obarray.define_special_variable("last-event-frame", Value::NIL);
    // keyboard.c:13913 DEFVAR_LISP, init nil.
    obarray.define_special_variable("last-event-device", Value::NIL);
    // keyboard.c:13925 DEFVAR_LISP, with the comment "This variable is set up
    // in sysdep.c": `init_sys_modes' (src/sysdep.c:1112) starts it at Qnil and
    // assigns c_cc[VERASE] only for a live tty (src/sysdep.c:1130). Off a
    // terminal GNU reads nil, so a seeded 0 is a value it never has, and
    // `normal-erase-is-backspace-setup-frame' (lisp/simple.el) compares it
    // against ?\^H. The live value is supplied by the tty init path.
    obarray.set_symbol_value("tty-erase-char", Value::NIL);
    obarray.make_special("tty-erase-char");
    // keyboard.c:13993 DEFVAR_INT, init 0.
    obarray.define_int_variable("extra-keyboard-modifiers", 0);
    obarray.set_symbol_value("inhibit-local-menu-bar-menus", Value::NIL);
    // keyboard.c:13777 DEFVAR_LISP, XSETINT 033.
    obarray.define_special_variable("meta-prefix-char", Value::fixnum(27));
    // keyboard.c:14319 DEFVAR_LISP, init nil.
    obarray.define_special_variable("enable-disabled-menus-and-buttons", Value::NIL);
    // GNU `src/keyboard.c` defines this with DEFVAR_LISP and initializes it to Qt.
    obarray.set_symbol_value("select-active-regions", Value::T);
    obarray.make_special("select-active-regions");
    // keyboard.c:14340 DEFVAR_LISP, init nil.
    obarray.define_special_variable("saved-region-selection", Value::NIL);
    // keyboard.c:14446 DEFVAR_LISP, init nil.
    obarray.define_c_hook_variable("post-select-region-hook");
    // keyboard.c:14459 DEFVAR_LISP, init nil.
    obarray.define_special_variable("current-key-remap-sequence", Value::NIL);
    // keyboard.c:14358 DEFVAR_LISP, init Qsigusr2.
    obarray.define_special_variable("debug-on-event", Value::symbol("sigusr2"));
    // keyboard.c:14422 DEFVAR_LISP, init nil.
    obarray.define_c_hook_variable("display-monitors-changed-functions");
    // keyboard.c:14287 DEFVAR_LISP, make_fixnum 2.
    obarray.define_special_variable("minibuffer-message-timeout", Value::fixnum(2));
    // keyboard.c:13834 DEFVAR_LISP, init nil.
    obarray.define_special_variable("this-original-command", Value::NIL);
    // keyboard.c:13744 DEFVAR_LISP, zero-initialized to nil.
    obarray.define_special_variable("last-nonmenu-event", Value::NIL);
    // keyboard.c:13803 DEFVAR_KBOARD; kboard slots start nil. NeoVM does not
    // yet split keyboard state per terminal, so model it as a global special.
    obarray.define_special_variable("last-repeatable-command", Value::NIL);
    // macros.c:427 / macros.c:442 DEFVAR_KBOARD; kboard slots start nil.
    obarray.define_special_variable("defining-kbd-macro", Value::NIL);
    obarray.define_special_variable("last-kbd-macro", Value::NIL);
    // GNU `src/keyboard.c` initializes this to nil and makes it buffer-local-on-set.
    obarray.set_symbol_value("deactivate-mark", Value::NIL);
    obarray.make_special("deactivate-mark");
    obarray.make_buffer_local("deactivate-mark", true);
    obarray.set_symbol_value("input-method-function", Value::symbol("list"));
    obarray.make_special("input-method-function");
    obarray.set_symbol_value("tab-bar-separator-image-expression", Value::NIL);
    obarray.make_special("tab-bar-separator-image-expression");
    obarray.set_symbol_value("tool-bar-separator-image-expression", Value::NIL);
    obarray.make_special("tool-bar-separator-image-expression");
    obarray.set_symbol_value(
        "selection-inhibit-update-commands",
        Value::list(vec![
            Value::symbol("handle-switch-frame"),
            Value::symbol("handle-select-window"),
            Value::symbol("handle-focus-in"),
            Value::symbol("handle-focus-out"),
        ]),
    );
    obarray.set_symbol_value("minor-mode-map-alist", Value::NIL);
    obarray.make_special("minor-mode-map-alist");
    obarray.set_symbol_value("minor-mode-overriding-map-alist", Value::NIL);
    obarray.make_special("minor-mode-overriding-map-alist");
    obarray.set_symbol_value("emulation-mode-map-alists", Value::NIL);
    obarray.make_special("emulation-mode-map-alists");
}
#[cfg(test)]
#[path = "tests/pure.rs"]
mod tests;
