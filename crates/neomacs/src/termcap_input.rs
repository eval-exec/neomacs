//! C-side TTY function-key discovery for `input-decode-map`.
//!
//! GNU Emacs seeds each terminal kboard's `input-decode-map` from
//! termcap/terminfo in `src/term.c:term_get_fkeys` before Lisp startup runs.
//! Lisp terminal files later add xterm/rxvt/etc. defaults through keymap
//! inheritance, but packages loaded from the init file can already inspect and
//! wrap the terminfo-derived ESC prefix map.  Doom/Evil relies on exactly that
//! ordering when it installs its ESC `menu-item :filter`.

//! The terminfo database handle itself lives in
//! [`super::terminal_capabilities`], shared with the output-attribute half.

use super::terminal_capabilities::{
    StringCapability, TerminalCapabilityDatabase, open_terminal_capability_database,
};
use neovm_core::emacs_core::intern::intern;
use neovm_core::emacs_core::keymap::list_keymap_define_seq_in_obarray_ex;
use neovm_core::emacs_core::{Context, Value};

const FKEY_TABLE: &[(&str, &str)] = &[
    ("kh", "home"),
    ("kl", "left"),
    ("ku", "up"),
    ("kr", "right"),
    ("kd", "down"),
    ("%8", "prior"),
    ("%5", "next"),
    ("@7", "end"),
    ("@1", "begin"),
    ("*6", "select"),
    ("%9", "print"),
    ("@4", "execute"),
    ("&8", "undo"),
    ("%0", "redo"),
    ("%7", "menu"),
    ("@0", "find"),
    ("@2", "cancel"),
    ("%1", "help"),
    ("&4", "reset"),
    ("kE", "clearline"),
    ("kA", "insertline"),
    ("kL", "deleteline"),
    ("kI", "insertchar"),
    ("kD", "deletechar"),
    ("kB", "backtab"),
    ("@8", "kp-enter"),
    ("K4", "kp-1"),
    ("K5", "kp-3"),
    ("K2", "kp-5"),
    ("K1", "kp-7"),
    ("K3", "kp-9"),
    ("k1", "f1"),
    ("k2", "f2"),
    ("k3", "f3"),
    ("k4", "f4"),
    ("k5", "f5"),
    ("k6", "f6"),
    ("k7", "f7"),
    ("k8", "f8"),
    ("k9", "f9"),
    ("&0", "S-cancel"),
    ("&9", "S-begin"),
    ("*0", "S-find"),
    ("*1", "S-execute"),
    ("*4", "S-delete"),
    ("*7", "S-end"),
    ("*8", "S-clearline"),
    ("#1", "S-help"),
    ("#2", "S-home"),
    ("#3", "S-insert"),
    ("#4", "S-left"),
    ("%d", "S-menu"),
    ("%c", "S-next"),
    ("%e", "S-prior"),
    ("%f", "S-print"),
    ("%g", "S-redo"),
    ("%i", "S-right"),
    ("!3", "S-undo"),
];

const XTERM_COMPAT_TERMS: &[&str] = &["xterm", "screen", "tmux", "st", "konsole"];

const XTERM_FALLBACK_KEYS: &[(&[u8], &str)] = &[
    (b"\x1b[A", "up"),
    (b"\x1b[B", "down"),
    (b"\x1b[C", "right"),
    (b"\x1b[D", "left"),
    (b"\x1b[2~", "insert"),
    (b"\x1b[3~", "delete"),
    (b"\x1b[5~", "prior"),
    (b"\x1b[6~", "next"),
    (b"\x1b[15~", "f5"),
    (b"\x1b[17~", "f6"),
    (b"\x1b[18~", "f7"),
    (b"\x1b[19~", "f8"),
    (b"\x1b[20~", "f9"),
    (b"\x1b[21~", "f10"),
];

pub(crate) fn seed_input_decode_map_from_terminal(eval: &mut Context) {
    let Some(term) = std::env::var("TERM")
        .ok()
        .filter(|term| !term.is_empty() && term != "dumb")
    else {
        return;
    };
    let Some(mut db) = open_terminal_capability_database(&term) else {
        tracing::debug!("term_get_fkeys: no termcap/terminfo database for TERM={term:?}");
        return;
    };
    let Some(input_decode_map) = eval.obarray().symbol_value("input-decode-map").copied() else {
        return;
    };

    for (cap, name) in FKEY_TABLE {
        if let Some(sequence) = db.get_string(StringCapability::Termcap(cap))
            && !define_terminal_key(eval, input_decode_map, &sequence, name)
        {
            return;
        }
    }

    let k_semi = db.get_string(StringCapability::Termcap("k;"));
    let k0 = db.get_string(StringCapability::Termcap("k0"));
    if let Some(sequence) = k_semi {
        if let Some(k0_sequence) = k0
            && !define_terminal_key(eval, input_decode_map, &k0_sequence, "f0")
        {
            return;
        }
        if !define_terminal_key(eval, input_decode_map, &sequence, "f10") {
            return;
        }
    } else if let Some(sequence) = k0
        && !define_terminal_key(eval, input_decode_map, &sequence, "f10")
    {
        return;
    }

    for i in 11..64 {
        let Some(cap) = numbered_function_key_capability(i) else {
            continue;
        };
        if let Some(sequence) = db.get_string(StringCapability::Termcap(&cap)) {
            let name = format!("f{i}");
            if !define_terminal_key(eval, input_decode_map, &sequence, &name) {
                return;
            }
        }
    }

    conditional_reassign(eval, input_decode_map, db.as_mut(), "%5", "kN", "next");
    conditional_reassign(eval, input_decode_map, db.as_mut(), "%8", "kP", "prior");
    conditional_reassign(eval, input_decode_map, db.as_mut(), "kD", "kI", "insert");
    conditional_reassign(eval, input_decode_map, db.as_mut(), "@7", "kH", "end");

    if xterm_compatible_term(&term) {
        seed_xterm_fallback_keys(eval, input_decode_map);
    }
}

fn xterm_compatible_term(term: &str) -> bool {
    XTERM_COMPAT_TERMS
        .iter()
        .any(|prefix| term == *prefix || term.starts_with(&format!("{prefix}-")))
}

fn seed_xterm_fallback_keys(eval: &mut Context, input_decode_map: Value) {
    for (sequence, name) in XTERM_FALLBACK_KEYS {
        if !define_terminal_key(eval, input_decode_map, sequence, name) {
            return;
        }
    }
}

fn define_terminal_key(eval: &mut Context, keymap: Value, sequence: &[u8], name: &str) -> bool {
    let events = sequence
        .iter()
        .map(|byte| Value::fixnum(i64::from(*byte)))
        .collect::<Vec<_>>();
    let definition = Value::vector(vec![Value::symbol(intern(name))]);
    // `define-key' with REMOVE nil, which is what the deleted wrapper was
    // (DIVERGENCES.md 152).
    match list_keymap_define_seq_in_obarray_ex(eval.obarray(), keymap, &events, definition, false) {
        Ok(()) => true,
        Err(err) => {
            tracing::debug!(
                "term_get_fkeys: ignoring terminal key definition error for {name}: {err}"
            );
            false
        }
    }
}

fn conditional_reassign(
    eval: &mut Context,
    keymap: Value,
    db: &mut dyn TerminalCapabilityDatabase,
    missing_cap: &str,
    fallback_cap: &str,
    name: &str,
) {
    if db
        .get_string(StringCapability::Termcap(missing_cap))
        .is_some()
    {
        return;
    }
    if let Some(sequence) = db.get_string(StringCapability::Termcap(fallback_cap)) {
        define_terminal_key(eval, keymap, &sequence, name);
    }
}

fn numbered_function_key_capability(number: u8) -> Option<String> {
    let suffix = match number {
        11..=19 => char::from(b'1' + number - 11),
        20..=45 => char::from(b'A' + number - 20),
        46..=63 => char::from(b'a' + number - 46),
        _ => return None,
    };
    Some(format!("F{suffix}"))
}

#[cfg(test)]
mod tests {
    use super::{numbered_function_key_capability, xterm_compatible_term};

    #[test]
    fn numbered_function_key_capabilities_match_gnu_ranges() {
        assert_eq!(numbered_function_key_capability(10), None);
        assert_eq!(numbered_function_key_capability(11).as_deref(), Some("F1"));
        assert_eq!(numbered_function_key_capability(19).as_deref(), Some("F9"));
        assert_eq!(numbered_function_key_capability(20).as_deref(), Some("FA"));
        assert_eq!(numbered_function_key_capability(45).as_deref(), Some("FZ"));
        assert_eq!(numbered_function_key_capability(46).as_deref(), Some("Fa"));
        assert_eq!(numbered_function_key_capability(63).as_deref(), Some("Fr"));
        assert_eq!(numbered_function_key_capability(64), None);
    }

    #[test]
    fn xterm_compatible_terms_match_gnu_terminal_aliases() {
        assert!(xterm_compatible_term("xterm"));
        assert!(xterm_compatible_term("xterm-256color"));
        assert!(xterm_compatible_term("screen-256color"));
        assert!(xterm_compatible_term("tmux-256color"));
        assert!(xterm_compatible_term("st-256color"));
        assert!(xterm_compatible_term("konsole-256color"));
        assert!(!xterm_compatible_term("vt100"));
    }
}
