//! The one production `TextHost` implementation (stage-3b R0).
//!
//! The text layer declares `TextHost` (`emacs_core::text_host`) and calls
//! its methods generically; the evaluator implements it for `Context` right
//! here, so the trait never needs a second impl and the text side never
//! names `Context`. The `Vars` view is `VariableView`: `variables()` builds
//! it from DISJOINT field borrows (`&self.obarray`, `&self.owner`), never
//! through a helper that borrows all of `self` (edit V.2 rule), so the
//! text phase holds only the variable state and no stale view can outlive
//! it. Static dispatch, never `dyn`, one production impl.
//! Precedent: `runtime/eval/edit_host.rs`.

use super::{Context, VariableView};
use crate::emacs_core::text_host::TextHost;
use crate::tagged::SymbolMode;

impl TextHost for Context {
    type Vars<'a> = VariableView<'a>;

    #[inline(always)]
    fn variables(&mut self) -> Self::Vars<'_> {
        VariableView::new(&self.obarray, &self.owner)
    }

    #[inline(always)]
    fn symbol_mode(&self) -> SymbolMode {
        Context::symbol_mode(self)
    }

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
    // Empty until those slices land.
}
