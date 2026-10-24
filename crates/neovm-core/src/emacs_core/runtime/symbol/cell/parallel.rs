//! The era's sealed, marker-excluded store seam. Serial CellWrite is unchanged.
use super::*;
use crate::tagged::gc::SymbolWriteToken;

impl LispSymbol {
    /// Construct detached initial membership from guarded name metadata. The
    /// ordinary constructors resolve the registry again and cannot run below
    /// the C5 registry guard. The real slot still publishes arms before name.
    pub(crate) fn parallel_initial_member(
        name: NameId,
        void: Option<FwdRef>,
        _: &SymbolWriteToken<'_>,
    ) -> Self {
        let mut symbol = Self::empty();
        symbol.name.store(name.0, Ordering::Relaxed);
        symbol.interned_global = true;
        symbol.set_interned(SymbolInterned::InternedInInitial);
        if let Some(header) = void {
            symbol.flags.set_redirect(SymbolRedirect::Forwarded);
            symbol.flags.set_runtime_projected(true);
            symbol.val = CellWord::forwarded(header);
        }
        symbol
    }
    /// Existing nonmembers retain every value/function/plist/flag arm except
    /// the membership bits GNU intern changes. No serial write hook runs.
    pub(crate) fn parallel_mark_initial(&mut self, _: &SymbolWriteToken<'_>) {
        self.interned_global = true;
        self.set_interned(SymbolInterned::InternedInInitial);
    }

    pub(crate) fn parallel_store_plain(&self, value: Value, _: &SymbolWriteToken<'_>) -> bool {
        if !self.flags.is_plain_untrapped_unprojected() {
            return false;
        }
        self.val.store_release(value.bits());
        true
    }

    /// VOID materialization keeps the Forwarded tag. Only the descriptor
    /// identity changes; no shared VOID storage is ever written.
    pub(crate) fn parallel_replace_projection(
        &self,
        previous: FwdRef,
        next: FwdRef,
        _: &SymbolWriteToken<'_>,
    ) -> bool {
        if !self.flags.runtime_projected()
            || self.trapped_write() != SymbolTrappedWrite::Untrapped
            || self.forwarded_descriptor().map(FwdRef::address) != Some(previous.address())
        {
            return false;
        }
        self.val.store_release(next.address());
        true
    }
}
