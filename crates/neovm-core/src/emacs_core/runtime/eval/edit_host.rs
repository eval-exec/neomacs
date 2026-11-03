//! The one production `EditHost` implementation (stage-3 R0).
//!
//! The edit layer declares `EditHost` (`emacs_core::edit_host`) and
//! calls its methods generically; the evaluator implements it for
//! `Context` right here, so the trait never needs a second impl and the
//! edit side never names `Context`. The C13 slice delegates to
//! `hook_runtime`, which stays `pub(crate)` in core (PEER-RULING-STAGE3
//! C13: nothing is installed, so there is no PC2 row).

use super::Context;
use crate::emacs_core::edit_host::EditHost;
use crate::emacs_core::error::EvalResult;
use crate::emacs_core::hook_runtime;
use crate::emacs_core::intern::SymId;
use crate::emacs_core::value::Value;

impl EditHost for Context {
    fn hook_symbol_by_id(&self, hook_sym: SymId) -> SymId {
        hook_runtime::hook_symbol_by_id(self, hook_sym)
    }

    fn hook_symbol_by_name(&self, hook_name: &str) -> SymId {
        hook_runtime::hook_symbol_by_name(self, hook_name)
    }

    fn hook_value_by_id(&self, hook_sym: SymId) -> Option<Value> {
        hook_runtime::hook_value_by_id(self, hook_sym)
    }

    fn run_hook_value(
        &mut self,
        hook_sym: SymId,
        hook_value: Value,
        hook_args: &[Value],
        inherit_global: bool,
    ) -> EvalResult {
        hook_runtime::run_hook_value(self, hook_sym, hook_value, hook_args, inherit_global)
    }

    fn run_hook_value_until_failure(
        &mut self,
        hook_sym: SymId,
        hook_value: Value,
        hook_args: &[Value],
        inherit_global: bool,
    ) -> EvalResult {
        hook_runtime::run_hook_value_until_failure(
            self,
            hook_sym,
            hook_value,
            hook_args,
            inherit_global,
        )
    }
}
