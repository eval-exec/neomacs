//! What an owner republishes when it runs again (Phase 0 step 2, round 3b).
//!
//! The Context mirrors a few variables in host state (quit and inhibit
//! flags, compiler overrides, the keyboard maps, the GC threshold) and
//! names one Boolean cell in its admission header (`debug-on-next-call`).
//! Each owner write publishes its own effective value at the write. A
//! canonical write by another owner does not reach this Context, so the
//! owner refreshes on its returning continuation, under the baton:
//! [`Context::refresh_owner_mirrors`]. That is also where it drops bindings
//! whose buffer-local storage another owner killed while it was parked, as
//! GNU's thread switch zaps a `SPECPDL_LET_LOCAL` whose local is gone
//! (`src/eval.c:4204-4228`).
//!
//! A buffer's per-buffer slots always hold the effective value of the
//! owner running in it (design §1.3b), so every `BVAR` reader -- redisplay,
//! insertion, undo recording -- stays a slot load. A `let` of a slot the
//! buffer holds binds in place, a `LetLocal` entry as GNU's, and so does
//! `buffer-undo-list`, which is never projected (its storage is the
//! buffer's shared undo state). These bindings swap out when the owner
//! leaves the baton ([`Context::park_owner`]) and back in when it returns,
//! exactly as GNU's thread switch swaps them (`specpdl_unrewind`,
//! `src/eval.c:4134-4234`); while parked, their values sit in the owner's
//! slab. With several owners running at once, the same swap moves to buffer
//! acquire and release.
//!
//! The handoff is typed: [`Context::park_owner`] gives a [`ParkedOwner`],
//! and only [`Context::resume_owner`] takes it back, so an owner cannot
//! run again without its returning continuation. The green-thread runtime
//! (round 3d) calls them at its baton's release and acquire; until then,
//! tests drive them.

use super::owner_bindings::{Channel, UndoTarget};
use super::{Context, SymId, SymbolValueLookup, Value};
use neovm_value::intern::intern;

/// An owner that left the baton: its in-place buffer bindings are swapped
/// out and its host mirrors may be stale. Neither `Copy` nor `Clone`, and
/// only [`Context::resume_owner`] consumes it, so each park has exactly one
/// returning continuation.
#[must_use = "a parked owner runs again only through `Context::resume_owner`"]
#[derive(Debug)]
pub(crate) struct ParkedOwner {
    /// The obarray generation of the World the owner belongs to.
    generation: u64,
    _thread_confined: std::marker::PhantomData<*const ()>,
}

static_assertions::assert_not_impl_any!(ParkedOwner: Copy, Clone, Send, Sync);

/// Which way a handoff moves an owner's bindings held in shared storage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HostBindingSwap {
    /// The owner returns: its bindings go back into shared storage.
    In,
    /// The owner leaves: what its bindings hide goes back.
    Out,
}

/// `debug-on-next-call`, whose `DEFVAR_BOOL` cell the admission header
/// names.
pub(super) fn debug_on_next_call_symbol() -> SymId {
    static SYMBOL: std::sync::OnceLock<SymId> = std::sync::OnceLock::new();
    *SYMBOL.get_or_init(|| intern("debug-on-next-call"))
}

impl Context {
    /// Swap this owner's parked in-place bindings back in, drop its
    /// bindings whose buffer-local storage is gone, and republish every
    /// host mirror from what it sees now: [`Self::resume_owner`]'s work,
    /// also run alone by tests that simulate another owner's write.
    #[cold]
    #[inline(never)]
    pub(crate) fn refresh_owner_mirrors(&mut self) {
        self.reconcile_aliased_owner_bindings();
        self.swap_host_bindings(HostBindingSwap::In);
        self.retire_dead_owner_locals();
        self.republish_owner_mirrors();
    }

    /// Republish every host mirror and the debugger header from what this
    /// owner sees now. Allocates nothing, so admission's commit runs it too.
    pub(crate) fn republish_owner_mirrors(&mut self) {
        for word_index in 0..self.runtime_projection_mask().len() {
            let mut word = self.runtime_projection_mask()[word_index];
            while word != 0 {
                let bit = word.trailing_zeros() as usize;
                word &= word - 1;
                // SAFETY: round-trip of an interned id stored in the owner mirror (peer seal N8).
                let id = unsafe { SymId::from_raw((word_index * 64 + bit) as u32) };
                let visible = match self.find_symbol_value_by_id(id) {
                    Ok(SymbolValueLookup::Bound(value)) => value,
                    _ => Value::NIL,
                };
                self.sync_cached_runtime_binding_by_id(id, visible);
                self.sync_keyboard_runtime_binding_by_id(id, visible);
            }
        }
        self.sync_gc_threshold_from_runtime_settings();
        self.point_owner_debugger();
    }

    /// This owner leaves the baton: its bindings held in shared host
    /// storage swap out, so the next owner sees what they hide (GNU's
    /// `unbind_for_thread_switch`, `src/thread.c:95-99`).
    #[cold]
    #[inline(never)]
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn park_owner(&mut self) -> ParkedOwner {
        self.swap_host_bindings(HostBindingSwap::Out);
        ParkedOwner {
            generation: self.obarray.generation(),
            _thread_confined: std::marker::PhantomData,
        }
    }

    /// The returning continuation of a PARKED owner, on the thread it runs
    /// on now: the owner's thread state is installed there, the owner's
    /// in-place buffer bindings swap back in, its bindings whose local
    /// another owner killed retire (design §5.4), and every host mirror
    /// shows what it sees again.
    #[cold]
    #[inline(never)]
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn resume_owner(&mut self, parked: ParkedOwner) {
        debug_assert_eq!(
            parked.generation,
            self.obarray.generation(),
            "an owner resumes in its own World"
        );
        self.obarray.refresh_admission();
        self.setup_owner_thread_locals();
        self.refresh_owner_mirrors();
    }

    /// Admitted owners keep the ordinary heap, epoch, cache and table
    /// activation, using borrowed terminal IDs instead of a fresh snapshot.
    #[inline(always)]
    pub(crate) fn setup_owner_thread_locals(&mut self) {
        self.setup_thread_locals_inner::<false>();
    }

    /// Swap each live in-place buffer binding (`LetLocal` entries: a
    /// per-buffer slot's, `buffer-undo-list`'s; `LetDefault` entries of a
    /// per-buffer variable: its default) with the buffer's storage,
    /// GNU's `specpdl_unrewind` on that channel: innermost first going out,
    /// outermost first coming back in. A binding whose buffer or local is
    /// gone turns into a no-op entry, as GNU's does, which drops its parked
    /// value. While the owner is parked its binding values sit in its slab,
    /// which the collector traces from the World (design §1.3b).
    pub(crate) fn swap_host_bindings(&mut self, direction: HostBindingSwap) {
        let mut entries: Vec<usize> = self
            .specpdl
            .iter()
            .enumerate()
            .filter(|(_, entry)| match entry {
                super::SpecBinding::LetLocal { .. } => true,
                super::SpecBinding::LetDefault { sym_id, .. } => self.binds_in_place(*sym_id),
                _ => false,
            })
            .map(|(index, _)| index)
            .collect();
        if entries.is_empty() {
            return;
        }
        if direction == HostBindingSwap::Out {
            entries.reverse();
        }
        for &index in &entries {
            if matches!(
                self.specpdl.get(index),
                Some(super::SpecBinding::LetDefault { .. })
            ) {
                // GNU's thread switch never fails for a value the variable
                // already held.
                let swapped = self.swap_let_binding_for_thread_switch(index);
                debug_assert!(swapped.is_ok(), "a default binding swaps back");
            } else {
                self.swap_local_let_binding_for_thread_switch(index);
            }
        }
        match direction {
            HostBindingSwap::Out => {
                // Each live entry now saves its binding's value.
                let parked: Vec<Value> = entries
                    .iter()
                    .filter_map(|&index| match self.specpdl.get(index) {
                        Some(super::SpecBinding::LetLocal { old_value, .. }) => Some(*old_value),
                        Some(super::SpecBinding::LetDefault { old_value, .. }) => {
                            Some(old_value.as_plain())
                        }
                        _ => None,
                    })
                    .collect();
                self.ensure_owner_slab().park(parked);
            }
            HostBindingSwap::In => {
                if let Some(slab) = self.owner.slab_mut() {
                    slab.unpark();
                }
            }
        }
    }

    /// GNU's thread-switch zap of a `let` whose buffer-local binding is
    /// gone (`src/eval.c:4204-4228`): the record restores nothing at its
    /// pop, and the owner's slot for it goes.
    pub(crate) fn retire_dead_owner_locals(&mut self) {
        let Some(slab) = self.owner.slab() else {
            return;
        };
        let dead: Vec<_> = slab
            .live_records()
            .filter(|(_, record)| {
                matches!(record.channel, Channel::BufferLocal(_))
                    && !matches!(record.undo, UndoTarget::Retired)
                    && self
                        .obarray
                        .thread_projection_of(record.sym)
                        .is_some_and(|projection| {
                            !self.owner_channel_live(record.sym, projection, record.channel)
                        })
            })
            .map(|(id, record)| (id, record.index, record.channel))
            .collect();
        for (id, index, channel) in dead {
            if let Some(slab) = self.owner.slab_mut() {
                slab.replace_undo(id, UndoTarget::Retired);
            }
            self.clear_owner_slot(index, channel);
        }
    }

    /// Name this owner's own `debug-on-next-call` cell in its admission
    /// header while the owner binds the variable's default, and the
    /// canonical cell otherwise.
    #[cold]
    #[inline(never)]
    pub(crate) fn point_owner_debugger(&mut self) {
        let id = debug_on_next_call_symbol();
        let index = self
            .obarray
            .thread_projection_of(id)
            .map(|projection| projection.index());
        let cell = match (index, self.owner.slab_mut()) {
            (Some(index), Some(slab)) if slab.slot(index, Channel::Default).value().is_some() => {
                Some(slab.debugger_cell(index))
            }
            _ => None,
        };
        self.obarray.point_debugger(cell);
    }

    /// The debugger disarmed `debug-on-next-call` through the header's
    /// cell (GNU `debug_on_next_call = 0`, `src/eval.c:298, 338`). When the
    /// cell is this owner's, its binding becomes nil too.
    #[cold]
    #[inline(never)]
    pub(crate) fn owner_debugger_disarmed(&mut self) {
        let id = debug_on_next_call_symbol();
        let Some(index) = self
            .obarray
            .thread_projection_of(id)
            .map(|projection| projection.index())
        else {
            return;
        };
        if let Some(slab) = self.owner.slab_mut()
            && slab.slot(index, Channel::Default).value().is_some()
        {
            slab.set_slot(
                index,
                Channel::Default,
                super::owner_bindings::SlotState::Bound(super::owner_bindings::SlotValue::new(
                    Value::NIL,
                )),
            );
        }
    }
}

#[cfg(test)]
#[path = "tests/owner_mirrors_test.rs"]
mod tests;
