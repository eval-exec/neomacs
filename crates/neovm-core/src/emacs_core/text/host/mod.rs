//! The text layer's host interface (stage-3b R0).
//!
//! Text code must not name `eval::Context`: after the crate split the text
//! crate cannot reach the evaluator's types at all. Text functions that need
//! evaluator, system or display services take `H: TextHost`, monomorphized at
//! `H = Context`; dispatch is static, nothing is installed, and there is no
//! "uninstalled" state. The trait is never `dyn`, and there is exactly one
//! production impl, in `runtime/eval` (mirror of `edit_host`).
//! Precedent: `emacs_core::edit_host::EditHost`.
//!
//! Functions that need only edit state take `&mut EditRuntime` (or
//! `&mut BufferManager`) and stay monomorphic — prefer that form (jitcall
//! RECEIVER-SHAPE 00:26Z). A body that also reads variables opens the
//! `variables()` phase in the narrowest scope; while the view lives no
//! `TextHost` method is callable (borrowck enforces it), and only `Copy`
//! values cross a host phase.
//!
//! The method surface has two owner sections (LEAD 09:2xZ, parallel T3):
//! **gapsurvey module slices** (syntax, search/regex, casefiddle, casetab,
//! chartable) and **cired2/jitcall module slices** (encoding, coding,
//! charset, ccl, category, composite, json, xml, zlib, format,
//! string_escape, code_conversion_workspace). Each method is one future
//! TEXT-HOST ledger row, named in the READY that lands it. An installed
//! `&'static` fn table is correct only where no host is in scope (the
//! C2/C7/C8a seams) — never here.

use neovm_edit::emacs_core::edit_host::EditVariableRead;
use neovm_value::tagged::SymbolMode;

/// The variable reads one text phase may perform (stage-3b T3).
///
/// A marker subtrait of [`EditVariableRead`]: text is above edit and hands
/// its view to edit-owned scan snapshots (`CharPropertyResolver::snapshot`,
/// `buffer_read_only_active_in_state`, …), which take the edit trait. The
/// reads themselves — `obarray`, `value`, `value_by_name`, `value_in_buffer`,
/// `value_or_nil` — are the existing pure `#[inline(always)]` delegation on
/// `VariableView` (`runtime/eval/variable_access.rs`); re-declaring them here
/// would make every `vars.value(..)` under both bounds an E0034 ambiguity.
///
/// `Copy`, and `!Send`/`!Sync` (the evaluator's view asserts this), so a view
/// is re-created per text phase and never outlives it: a stale view cannot
/// exist.
pub trait TextVariableRead<'a>: EditVariableRead<'a> + Copy {}

pub trait TextHost {
    /// The owner's variable-read view for one text phase (see
    /// [`TextVariableRead`]).
    type Vars<'a>: TextVariableRead<'a> + Copy
    where
        Self: 'a;

    /// The text phase opener: the variable view and nothing else, from
    /// DISJOINT field borrows. Open it in the narrowest scope the body
    /// allows; while it lives, no other `TextHost` method may run.
    fn variables(&mut self) -> Self::Vars<'_>;

    /// Symbol interpretation for this mutator's current dynamic binding
    /// (`symbols-with-pos-enabled`). Read-side and `Copy`; every text
    /// module needs it, so it belongs to the shared surface rather than a
    /// module slice.
    fn symbol_mode(&self) -> SymbolMode;

    // -------------------------------------------------------------------------
    // C1 module slices — gapsurvey (syntax, search/regex, casefiddle,
    // casetab, chartable)
    // -------------------------------------------------------------------------
    // Added per slice; each method is one TEXT-HOST ledger row.

    // -------------------------------------------------------------------------
    // C1 module slices — cired2/jitcall (encoding, coding, charset, ccl,
    // category, composite, json, xml, zlib, format, string_escape,
    // code_conversion_workspace)
    // -------------------------------------------------------------------------
    // Empty until those slices land. Nothing here may name a core manager
    // type text does not own (CustomManager -> a query returning the result;
    // charset_map_directory -> the directory value).
}
