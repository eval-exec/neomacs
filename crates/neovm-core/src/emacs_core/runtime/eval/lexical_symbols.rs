//! GNU's positioned-symbol lexical-environment comparisons.
//!
//! `eval_sub` and `lexbound_p` use `Fassq` (eval.c:2593, 800), and
//! `let`/`let*` use `Fmemq` for local special declarations (1184, 1110).
//! These scans are selected only while positioned symbols are transparent;
//! the ordinary scans and their caches keep their existing representation.

use crate::emacs_core::intern::SymId;
use crate::emacs_core::value::Value;
use crate::tagged::SymbolMode;

pub(super) fn assq(lexenv: Value, symbol: SymId) -> Option<Value> {
    let mut tail = lexenv;
    while tail.is_cons() {
        let entry = tail.cons_car();
        if entry.is_cons() && entry.cons_car().as_symbol_id(SymbolMode::Positioned) == Some(symbol)
        {
            return Some(entry);
        }
        tail = tail.cons_cdr();
    }
    None
}

pub(super) fn declares_special(lexenv: Value, symbol: SymId) -> bool {
    let mut tail = lexenv;
    while tail.is_cons() {
        if tail.cons_car().as_symbol_id(SymbolMode::Positioned) == Some(symbol) {
            return true;
        }
        tail = tail.cons_cdr();
    }
    false
}
