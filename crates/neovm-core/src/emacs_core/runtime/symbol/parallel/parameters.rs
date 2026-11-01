//! Cold ordinary-parameter checks on the original retained World backing.
use super::*;
use crate::emacs_core::intern::resolve_name_lisp_string;
use crate::tagged::gc::SymbolWriteToken;
use crate::tagged::gc::function_write_hooks::CallbackParameterKind;

impl SymbolBacking {
    /// # Safety
    /// The matching exclusive admission writer retains this World before any
    /// owner registers; no ordinary view or concurrent marker can overlap.
    pub(crate) unsafe fn callback_parameter_kind_at_admission(
        &self,
        id: SymId,
        _authority: &SymbolWriteToken<'_>,
    ) -> CallbackParameterKind {
        // SAFETY: this is the original retained root covered by the caller's
        // matching exclusive admission writer, never a Context-derived view.
        let obarray = unsafe { self.obarray.as_ref() };
        if obarray.generation() != self.generation || !obarray.is_global_member(id) {
            return CallbackParameterKind::Unsupported;
        }
        let Some(symbol) = obarray.get_by_id(id) else {
            return CallbackParameterKind::Unsupported;
        };
        if symbol.trapped_write() != SymbolTrappedWrite::Untrapped
            || symbol.flags().runtime_projected()
            || symbol.flags().declared_special()
            || !matches!(symbol.value_cell(), ValueCell::Plain(_))
        {
            return CallbackParameterKind::Unsupported;
        }
        // Resolve the slot's existing NameId directly. This cold check neither
        // interns a name nor enters a thread-local symbol-name cache.
        let name = resolve_name_lisp_string(symbol.name()).as_bytes();
        if name == b"nil" || name == b"t" || name.starts_with(b":") || name.starts_with(b"&") {
            CallbackParameterKind::Unsupported
        } else {
            CallbackParameterKind::Ordinary
        }
    }
}
