//! Cold JIT variable fallbacks with the inline store's bookkeeping contract.
//!
//! The installed Context is exclusively owned by its executing mutator. These
//! wrappers add no TLS identity cache and never change interpreter callers.
//! Shared observation marks remain atomic lifetime metadata; certificates and
//! their revision journal remain local to the executing mutator.

use super::*;

/// Compiled observed stores refine their own empty gaps only at GEN0, with
/// explicit tracking disabled. Every other policy keeps the interpreter tier.
#[inline]
fn observed_compiled_policy() -> bool {
    use crate::tagged::collection_reads::{CompiledJournalMode, compiled_journal_mode};
    compiled_journal_mode() == CompiledJournalMode::Observed
        && !crate::tagged::gc::current_heap_generational_enabled()
        && !crate::tagged::gc::current_write_tracking_enabled()
}

impl Context {
    /// The unchanged variable-cache decisions with compiled cons bookkeeping.
    /// No hit runs Lisp, allocates a Lisp object or reaches a GC safe point.
    #[inline(never)]
    pub(crate) fn try_set_var_cached_compiled(&mut self, id: SymId, value: Value) -> bool {
        if observed_compiled_policy() {
            self.try_set_var_cached_impl::<true>(id, value)
        } else {
            self.try_set_var_cached(id, value)
        }
    }

    /// Save the old binding exactly as inline native code does, without making
    /// that implementation read a Lisp collection dependency of an active
    /// capture. A real read already captured by this mutator still requires
    /// the selected owner store to journal through `set_compiled_cons_cdr`.
    #[inline(never)]
    pub(crate) fn specbind_cached_compiled(&mut self, id: SymId, value: Value) -> bool {
        if observed_compiled_policy() {
            self.specbind_cached_impl::<true>(id, value)
        } else {
            self.specbind_cached(id, value)
        }
    }

    #[inline(always)]
    fn pop_let_local_cached_compiled(
        &mut self,
        id: SymId,
        old: Value,
        buffer_id: crate::buffer::BufferId,
    ) -> bool {
        if observed_compiled_policy() {
            self.pop_let_local_cached_impl::<true>(id, old, buffer_id)
        } else {
            self.pop_let_local_cached(id, old, buffer_id)
        }
    }

    #[inline(always)]
    fn pop_let_default_cached_compiled(&mut self, id: SymId, old: SavedBindingValue) -> bool {
        if observed_compiled_policy() {
            self.pop_let_default_cached_impl::<true>(id, old)
        } else {
            self.pop_let_default_cached(id, old)
        }
    }

    /// Mirror the original suffix-pop loop, changing only its two localized
    /// cached calls. Deliberate duplication leaves the protected interpreter
    /// callers and their original implementation untouched, and keeps plain,
    /// forwarded and trivial JIT pops free of observation-policy checks or
    /// one-entry calls. Each successful entry is retired before considering
    /// the next; refusal leaves that entry for the original full unwinder.
    /// No arm runs Lisp, allocates a Lisp object or reaches a GC safe point.
    #[inline(never)]
    pub(crate) fn pop_compiled_specpdl_suffix(&mut self, count: usize) {
        use crate::emacs_core::symbol::{SymbolRedirect, SymbolTrappedWrite};
        while self.specpdl.len() > count {
            let Some(top) = self.specpdl.last() else {
                break;
            };
            match top {
                SpecBinding::Let { sym_id, old_value } => {
                    let (sym_id, old_value) = (*sym_id, *old_value);
                    // GNU `do_one_unbind`'s SPECPDL_LET arm: decide and store
                    // on one slot.  The entry owns nothing (const-asserted
                    // beside `trivial_spec_binding_pop`), so retiring it is
                    // GNU's `--specpdl_ptr`, not a 32-byte move plus drop
                    // glue.  The old value reaches the cell before the entry
                    // stops rooting it, and nothing between can collect.
                    if self
                        .obarray
                        .swap_plain_untrapped_value_id(sym_id, old_value.as_plain())
                        .is_some()
                    {
                        // SAFETY: the top entry is this `Let`; see above.
                        unsafe { self.specpdl.set_len(self.specpdl.len() - 1) };
                        continue;
                    }
                    let Some(sym) = self.obarray.get_by_id(sym_id) else {
                        break;
                    };
                    if sym.redirect() != SymbolRedirect::Plainval
                        || sym.trapped_write() != SymbolTrappedWrite::Untrapped
                    {
                        // A forwarder holding its own value: the typed store
                        // (P1.4 A4).
                        if self.pop_forwarded_let_cached(sym_id, old_value) {
                            continue;
                        }
                        break;
                    }
                    self.specpdl.pop();
                    // `UNBOUND` stored to a plain cell is `makunbound`. The
                    // cell was plain one check ago and nothing ran since.
                    let restored = self
                        .obarray
                        .store_plain_value_id(sym_id, old_value.as_plain());
                    debug_assert!(restored.is_ok(), "the cell left the plain arm unseen");
                    self.sync_cached_runtime_binding_by_id(
                        sym_id,
                        old_value.get().unwrap_or(Value::NIL),
                    );
                }
                // A buffer-local binding whose buffer's BLV cache still holds
                // it, and a buffer-local default: one cons store (P1.4 A4).
                SpecBinding::LetLocal {
                    sym_id,
                    old_value,
                    buffer_id,
                } => {
                    let (sym_id, old_value, buffer_id) = (*sym_id, *old_value, *buffer_id);
                    if self.pop_let_local_cached_compiled(sym_id, old_value, buffer_id) {
                        continue;
                    }
                    break;
                }
                SpecBinding::LetDefault {
                    sym_id, old_value, ..
                } => {
                    let (sym_id, old_value) = (*sym_id, *old_value);
                    if self.pop_let_default_cached_compiled(sym_id, old_value) {
                        continue;
                    }
                    break;
                }
                // GNU's `unbind_to` for the
                // `specbind (Qinternal_interpreter_environment, ...)` a
                // lexically-bound `let` makes: one store.  Without this arm
                // the fast pop stops here, because `sf_let` pushes it at the
                // BOTTOM of the suffix, under every root and dynamic
                // binding.
                SpecBinding::LexicalEnv { old_lexenv } => {
                    let old_lexenv = *old_lexenv;
                    let top_idx = self.specpdl.len() - 1;
                    self.lexenv = old_lexenv;
                    debug_assert_eq!(
                        self.specpdl.len(),
                        top_idx + 1,
                        "a fast restore must not push a specbinding"
                    );
                    // SAFETY: the entry is a plain `Value`, which owns
                    // nothing (const-asserted beside `trivial_spec_binding_pop`).
                    unsafe { self.specpdl.set_len(top_idx) };
                }
                other => match trivial_spec_binding_pop(other) {
                    Some(TrivialSpecBindingPop::BacktraceArgs(args)) => {
                        self.release_backtrace_args(&args);
                        // SAFETY: as in the inline fast path -- the closed
                        // proof says the entry owns nothing else.
                        unsafe { self.specpdl.set_len(self.specpdl.len() - 1) };
                    }
                    Some(TrivialSpecBindingPop::NoOwnedArgs) => {
                        // SAFETY: same proof; nothing owned at all.
                        unsafe { self.specpdl.set_len(self.specpdl.len() - 1) };
                    }
                    None => break,
                },
            }
        }
    }
}

/// The saved binding is implementation bookkeeping, matching the inline
/// native load rather than a Lisp value read. A valid live BLV supplies this
/// Cons; the installed mutator has exclusive Context access, and no callback,
/// collection or safe point can occur before the old value is pushed/rooted.
/// This narrow contract allows an active capture without using the general
/// `cons_cdr_unobserved` traversal API, whose precondition is no active capture.
#[inline(always)]
pub(super) fn read_saved_binding_cdr(cell: Value) -> Value {
    assert!(cell.is_cons());
    let owner =
        (cell.bits() & !crate::tagged::value::TAG_MASK) as *const crate::tagged::header::ConsCell;
    // SAFETY: the live BLV/cache and no-safe-point contract above retain the
    // selected owner and the returned value through the binding push.
    unsafe { (*owner).load_cdr() }
}
