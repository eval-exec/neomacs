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
//! The `NEOVM_BUILTIN_FRONTEND` knob moved to
//! [`neovm_value::builtin_frontend`] with the stage-3b frontend-service row;
//! the reads below go through the eval re-export.
use super::var_fast::CachedVarReadTier;

pub(crate) use neovm_value::builtin_frontend::builtin_frontend_on;
#[cfg(test)]
pub(crate) use neovm_value::builtin_frontend::{
    parse_builtin_frontend_knob, set_builtin_frontend_for_test,
};

use super::*;
use neovm_value::symbol::ValueCell;

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
    #[doc(hidden)]
    pub fn builtin_var_value(&self, id: SymId) -> Option<Value> {
        if builtin_frontend_on()
            && let Some(value) = self.builtin_var_value_cached(id)
        {
            return Some(value);
        }
        self.special_variable_value_by_id(id)
    }

    /// [`Self::builtin_var_value`] by name, interning it: GNU
    /// `find_symbol_value` of the named variable, `None` when void. This is
    /// the visible read for callers that hold a name instead of a pre-interned
    /// id; hot paths should pre-intern and use the id form.
    #[inline]
    #[doc(hidden)]
    pub fn builtin_var_value_by_name(&self, name: &str) -> Option<Value> {
        self.builtin_var_value(neovm_value::intern::intern(name))
    }

    #[inline(always)]
    #[doc(hidden)]
    pub fn builtin_var_value_cached(&self, id: SymId) -> Option<Value> {
        let sym = self.obarray.get_by_id(id)?;
        match sym.value_cell() {
            ValueCell::Plain(value) => (!value.is_unbound()).then_some(value),
            ValueCell::Localized(_) | ValueCell::Forwarded(_) => {
                let tier = CachedVarReadTier::admit()?;
                self.read_var_cached_symbol(sym, tier)
            }
            ValueCell::Alias(_) => None,
        }
    }
}
