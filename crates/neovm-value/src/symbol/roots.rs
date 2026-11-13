//! One root-enumeration definition for serial policy and stopped keeper scans.
use super::*;

/// Serial termination can omit barrier-covered cells; a parallel stop cannot.
pub(super) enum SymbolRootSet {
    Complete,
    Pools,
}

impl Obarray {
    #[inline(always)]
    pub(super) fn append_symbol_roots(&self, selection: SymbolRootSet, roots: &mut Vec<Value>) {
        for sym in self.symbols.iter().filter(|s| s.is_present()) {
            if matches!(selection, SymbolRootSet::Pools) {
                continue;
            }
            // Acquire loads: a concurrent mutator may publish into the
            // same word (`CellWrite`).
            match sym.value_cell_acquire() {
                ValueCell::Plain(v) => {
                    if v != Value::UNBOUND {
                        roots.push(v);
                    }
                }
                // Alias:     a SymId, not a heap ref.
                // Forwarded: 'static forwarder metadata (owned values are
                //            traced from `value_fwds` below).
                // Localized: BLV contents traced via self.blvs below.
                ValueCell::Alias(_) | ValueCell::Forwarded(_) | ValueCell::Localized(_) => {}
            }
            roots.push(sym.function.load_atomic());
            roots.push(sym.plist.load_atomic());
        }
        // BLV contents for LOCALIZED symbols. Unchanged.
        for &blv_ptr in &self.blvs {
            // SAFETY: the pool's records live as long as `self`.
            let blv = unsafe { &*blv_ptr.as_ptr() };
            roots.push(blv.defcell.load_atomic());
            roots.push(blv.valcell.load_atomic());
            roots.push(blv.where_buf.load_atomic());
        }
        // Value-owning forwarder slots. GNU's `DEFVAR_INT` slot is an
        // `intmax_t` and needs no marking, but Neomacs stores the Lisp integer
        // (a heap object once it leaves fixnum range); GNU's `DEFVAR_LISP` and
        // `DEFVAR_KBOARD` slots live in `struct emacs_globals` / `struct
        // KBOARD`, which `staticpro` and `mark_kboards` root, and here they
        // live in the descriptor -- so in all three cases the descriptor is
        // the root. Traced from this list rather than from the symbol walk
        // because a symbol that was later localized no longer points at its
        // descriptor while the BLV still forwards through it. Like the BLV
        // pool, this loop always runs -- the SATB barrier in each `set`
        // covers the mark window, and the start seed / STW collection need
        // the full list.
        for fwd in &self.value_fwds {
            if let Some(value) = fwd.owned_value() {
                roots.push(value);
            }
            if let Some(keyboard) = fwd.as_kboard_obj_fwd() {
                keyboard.trace_parked_roots(roots);
            }
        }
    }
}
