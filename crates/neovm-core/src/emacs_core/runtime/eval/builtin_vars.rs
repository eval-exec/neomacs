//! Builtin front-end diets (U2.8): the knob, and the typed reads of the
//! C-level variables Rust builtins consult on every call.
//!
//! GNU's C reads `case-fold-search`, `inhibit-changing-match-data`,
//! `parse-sexp-lookup-properties` and the like as a global or a buffer slot
//! (`Vinhibit_changing_match_data`, `BVAR (current_buffer, ...)`): a load.
//! The search, syntax and text-property builtins name each such variable by
//! a closed enum (`SearchStateVariable`, `SyntaxStateVariable`, ...) and read
//! it with [`Context::builtin_var_value`], which answers the plain,
//! buffer-local and forwarded shapes from the cell, the loaded BLV or the
//! forwarder -- P1.4 Stage A's cached tiers ([`Context::read_var_cached`]) --
//! and hands everything else (an alias, a BLV miss, a void value) to the
//! general reader, unchanged.
//!
//! # Knob
//!
//! `NEOVM_BUILTIN_FRONTEND` gates every U2.8 fast path, for a same-binary
//! A/B: unset, `1`, `on` or `all` enables them; `0`, `off` or `none` restores
//! the general reads and the old per-call preparation. Read once per
//! process, on the first builtin that asks.
use super::*;
use crate::emacs_core::symbol::SymbolRedirect;
use std::sync::atomic::{AtomicU8, Ordering};

const KNOB_UNREAD: u8 = 0;
const KNOB_OFF: u8 = 1;
const KNOB_ON: u8 = 2;

static BUILTIN_FRONTEND: AtomicU8 = AtomicU8::new(KNOB_UNREAD);

/// Whether a value of `NEOVM_BUILTIN_FRONTEND` enables the front-end diets.
pub(crate) fn parse_builtin_frontend_knob(value: Option<&str>) -> bool {
    match value.map(str::trim) {
        None | Some("") => true,
        Some(word) => match word.to_ascii_lowercase().as_str() {
            "1" | "on" | "all" | "true" => true,
            "0" | "off" | "none" | "false" => false,
            other => {
                tracing::warn!(
                    value = other,
                    "NEOVM_BUILTIN_FRONTEND: unknown value, front-end diets stay on"
                );
                true
            }
        },
    }
}

#[cfg(test)]
thread_local! {
    static KNOB_TEST_OVERRIDE: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

/// Force the knob on this thread (tests only); `None` returns to the
/// environment.
#[cfg(test)]
pub(crate) fn set_builtin_frontend_for_test(on: Option<bool>) {
    KNOB_TEST_OVERRIDE.with(|cell| cell.set(on));
}

/// Whether the U2.8 front-end fast paths are enabled. One relaxed byte load
/// once the knob has been read.
#[inline(always)]
pub(crate) fn builtin_frontend_on() -> bool {
    #[cfg(test)]
    if let Some(on) = KNOB_TEST_OVERRIDE.with(|cell| cell.get()) {
        return on;
    }
    match BUILTIN_FRONTEND.load(Ordering::Relaxed) {
        KNOB_ON => true,
        KNOB_OFF => false,
        _ => read_builtin_frontend_knob(),
    }
}

#[cold]
#[inline(never)]
fn read_builtin_frontend_knob() -> bool {
    let on = parse_builtin_frontend_knob(std::env::var("NEOVM_BUILTIN_FRONTEND").ok().as_deref());
    BUILTIN_FRONTEND.store(if on { KNOB_ON } else { KNOB_OFF }, Ordering::Relaxed);
    tracing::debug!(on, "NEOVM_BUILTIN_FRONTEND read");
    on
}

impl Context {
    /// GNU `find_symbol_value` of the C-level variable ID, as a builtin reads
    /// it: `None` when it is void.
    ///
    /// With the front-end diets on, a bound plain cell answers from the cell
    /// and a buffer-local or forwarded variable from Stage A's cached tiers;
    /// any shape those refuse, and every read with the knob off, takes
    /// [`Self::special_variable_value_by_id`]. ID must name a C-level
    /// variable (never `buffer-undo-list`, whose value lives outside its
    /// symbol).
    #[inline]
    pub(crate) fn builtin_var_value(&self, id: SymId) -> Option<Value> {
        if builtin_frontend_on()
            && let Some(value) = self.builtin_var_value_cached(id)
        {
            return Some(value);
        }
        self.special_variable_value_by_id(id)
    }

    #[inline(always)]
    fn builtin_var_value_cached(&self, id: SymId) -> Option<Value> {
        let sym = self.obarray.get_by_id(id)?;
        match sym.redirect() {
            SymbolRedirect::Plainval => {
                // SAFETY: redirect=Plainval selects the plain-value arm.
                let value = unsafe { sym.val.plain };
                (!value.is_unbound()).then_some(value)
            }
            SymbolRedirect::Localized | SymbolRedirect::Forwarded => self.read_var_cached(id),
            SymbolRedirect::Varalias => None,
        }
    }
}
