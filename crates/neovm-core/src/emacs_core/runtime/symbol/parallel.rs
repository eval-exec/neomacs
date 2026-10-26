//! Narrow backing for proof-3 authority. No serial accessor calls this module.
#![allow(dead_code)]

mod functions;
mod roots;
pub(crate) use functions::FunctionUpdate;
mod intern;
mod write;
#[cfg(test)]
pub(crate) use intern::{InitialInternFacts, InitialInternFault};

use super::{Obarray, SymbolTrappedWrite, ValueCell};
use crate::emacs_core::forward::{CanonicalArm, ThreadProjectionTable};
use crate::emacs_core::intern::SymId;
use std::cell::RefCell;
use std::ptr::NonNull;

/// Metadata only: no Value, cell address or descriptor escapes admission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlainSymbolBacking {
    Unprojected,
    Projected,
    VoidProjection,
}

/// Membership is distinct from a refused observation of a stale backing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InitialMembership {
    Member,
    Absent,
}

/// Original World allocation roots, never references derived from a Context
/// view. The era retains the World's Baton even on abandoned/poisoned teardown.
pub(crate) struct SymbolBacking {
    obarray: NonNull<Obarray>,
    projections: NonNull<RefCell<ThreadProjectionTable>>,
    pub(crate) world: usize,
    pub(crate) generation: u64,
    #[cfg(test)]
    intern_fault: std::sync::atomic::AtomicU8,
    #[cfg(test)]
    intern_fault_receipt: std::sync::atomic::AtomicU8,
}

// SAFETY: only the keeper constructs this bridge from retained World roots.
// Every dereference requires this World's held reader/writer word, or matching
// AllStopped. Shared readers never mutate or borrow the projection RefCell;
// only a writer or keeper may borrow that table. No reference, Value or RefCell
// loan leaves the sealed bridge and its private callers. This is
// not a Send/Sync implementation for Obarray, FwdRef, a descriptor or World.
unsafe impl Send for SymbolBacking {}
unsafe impl Sync for SymbolBacking {}

impl SymbolBacking {
    /// Inspect only membership while the matching initial-obarray reader is
    /// held. Neither absent storage nor a nonmember slot is initialized.
    ///
    /// # Safety
    /// The token belongs to this retained World; ordinary views and markers
    /// remain excluded. Absent is a real World-obarray soft miss.
    pub(crate) unsafe fn initial_contains(
        &self,
        id: SymId,
        _authority: &crate::tagged::gc::SymbolReadToken<'_>,
    ) -> Result<InitialMembership, crate::tagged::gc::RegistrationError> {
        // SAFETY: original World root covered by the live reader authority.
        let obarray = unsafe { self.obarray.as_ref() };
        if obarray.generation() != self.generation {
            return Err(crate::tagged::gc::RegistrationError::StaleRegistration);
        }
        Ok(if obarray.is_global_member(id) {
            InitialMembership::Member
        } else {
            InitialMembership::Absent
        })
    }

    /// Copy the function word and explicit-unbound state under one real
    /// symbol reader. This uses the landed interior-mutable function cell;
    /// no builtin fallback, callback, stamp or owner-private cache is touched.
    ///
    /// # Safety
    /// The token's live reader word belongs to this retained World. All
    /// ordinary views and concurrent markers remain excluded by that era.
    pub(crate) unsafe fn read_function(
        &self,
        id: SymId,
        _authority: &crate::tagged::gc::SymbolReadToken<'_>,
    ) -> Option<super::FunctionCellSnapshot> {
        // SAFETY: original retained root, with the matching reader held.
        let obarray = unsafe { self.obarray.as_ref() };
        if obarray.generation() != self.generation {
            return None;
        }
        obarray.get_by_id(id)?;
        Some(obarray.function_cell_snapshot(id))
    }

    /// # Safety
    /// Roots belong to the same live World, all ordinary views are escrowed,
    /// and the keeper retains that World until all era access ends. All calls
    /// to classify must hold the symbol writer or matching stop-all authority.
    pub(crate) unsafe fn new(
        obarray: NonNull<Obarray>,
        projections: NonNull<RefCell<ThreadProjectionTable>>,
        world: usize,
    ) -> Self {
        // SAFETY: construction is serial, with the original World retained.
        let generation = unsafe { obarray.as_ref() }.generation();
        Self {
            obarray,
            projections,
            world,
            generation,
            #[cfg(test)]
            intern_fault: std::sync::atomic::AtomicU8::new(0),
            #[cfg(test)]
            intern_fault_receipt: std::sync::atomic::AtomicU8::new(0),
        }
    }

    /// # Safety
    /// Exclusive symbol authority, or matching AllStopped, covers every read
    /// and the RefCell borrow. Shared readers must use admitted metadata.
    pub(crate) unsafe fn classify(&self, id: SymId) -> Option<PlainSymbolBacking> {
        // SAFETY: caller's exclusive authority; no whole mutable aggregate.
        let obarray = unsafe { self.obarray.as_ref() };
        if obarray.generation() != self.generation {
            return None;
        }
        let symbol = obarray.get_by_id(id)?;
        if symbol.trapped_write() != SymbolTrappedWrite::Untrapped {
            return None;
        }
        match symbol.value_cell() {
            ValueCell::Plain(_) if !symbol.flags().runtime_projected() => {
                Some(PlainSymbolBacking::Unprojected)
            }
            ValueCell::Forwarded(fwd) => {
                // SAFETY: same retained World, exclusive admission/keeper.
                let table = unsafe { self.projections.as_ref() }.try_borrow().ok()?;
                let projection = table.projection(fwd)?;
                if projection.host_projected() || projection.arm() != CanonicalArm::Plain {
                    return None;
                }
                Some(if projection.is_void() {
                    PlainSymbolBacking::VoidProjection
                } else {
                    PlainSymbolBacking::Projected
                })
            }
            _ => None,
        }
    }

    /// Only a live sealed reader can reach this copy. No projection-table
    /// RefCell borrow: admission checked ownership, and the closed operations
    /// preserve that fact while the era retains the original World.
    ///
    /// # Safety
    /// The token's held word must belong to this retained backing's World.
    /// Membership/arms remain frozen and no legacy view or marker may run.
    pub(crate) unsafe fn read_plain(
        &self,
        id: SymId,
        _authority: &crate::tagged::gc::SymbolReadToken<'_>,
    ) -> Option<super::Value> {
        let obarray = unsafe { self.obarray.as_ref() };
        if obarray.generation() != self.generation {
            return None;
        }
        let symbol = obarray.get_by_id(id)?;
        if symbol.trapped_write() != SymbolTrappedWrite::Untrapped {
            return None;
        }
        match symbol.value_cell() {
            ValueCell::Plain(value) if !symbol.flags().runtime_projected() => Some(value),
            ValueCell::Forwarded(fwd) if symbol.flags().runtime_projected() => {
                let projection = fwd.thread_projection()?;
                (projection.arm() == CanonicalArm::Plain && !projection.host_projected())
                    .then(|| projection.plain_value())
            }
            _ => None,
        }
    }

    #[cfg(test)]
    pub(crate) unsafe fn probe(&self, id: SymId) -> usize {
        // SAFETY: the test holds the real read/write/keeper lease. Only the
        // already-admitted plain backing is read; no RefCell borrow or TLS.
        let symbol = unsafe { self.obarray.as_ref() }.get_by_id(id).unwrap();
        match symbol.value_cell() {
            ValueCell::Plain(value) => value.bits(),
            ValueCell::Forwarded(fwd) => fwd.thread_projection().unwrap().plain_value().bits(),
            _ => panic!("test probe requires admitted Plain backing"),
        }
    }
}

#[cfg(test)]
#[path = "tests/parallel_test.rs"]
mod tests;
