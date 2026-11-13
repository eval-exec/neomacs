//! Partial physical-owner observations for the private numeric Rows producer.
//! No public API here grants geometry, coverage or cached-query authority.
use super::Context;
use super::display_query_hooks;
use crate::emacs_core::eval::data::face::{FaceTableIdentity, RealizationEpoch};
use crate::emacs_core::eval::protocol::{InertRowsHostAuthority, actual_rows_host_type};
use neovm_value::display_vocab::FrameId;
use neovm_value::tagged::{
    collection_reads::CollectionReads,
    gc::{self, HeapIdentity, RowsOwnedDomain},
};
use std::any::TypeId;
use std::{marker::PhantomData, ops::Range};

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum RowsObservationRefusal {
    #[error("the display-query hooks are not installed")]
    DisplayQueryHooksUninstalled,
    #[error("the Context is not the installed owning mutator")]
    InactiveContext,
    #[error("GEN0 does not provide the required partial/error collector refusal")]
    GenerationalDisabled,
    #[error("a runtime collector driver is active or unwound")]
    DriverActive,
    #[error("the collector or owner transport is not idle")]
    CollectorBusy,
    #[error("the logical Context, heap, face generation or dump authority changed")]
    RuntimeChanged,
    #[error("a collection completed after observation began")]
    CollectionChanged,
    #[error("a native display host can enter external code")]
    DisplayHostInstalled,
    #[error("only an explicit graphical window-system frame is supported")]
    TtyOrUnknownFrame,
    #[error("face normalization is not already warm")]
    ColdFaces,
    #[error("the actual termcap face lifecycle is unavailable")]
    TtyLifecycleUnavailable,
    #[error("the live terminal identity or output class changed")]
    TerminalChanged,
    #[error("the existing face cache or source-table lifetime changed")]
    FaceRealizationChanged,
    #[error("the installed native host differs from its captured authority")]
    HostTypeChanged,
    #[error("the existing chunk map is disabled")]
    ChunkMapDisabled,
    #[error("a dependency is foreign, new-domain, boxed or unsupported")]
    UnknownOwner,
}

#[derive(Debug)]
enum RowsFrameAuthority {
    Graphical,
    Tty {
        terminal: u64,
        realization: RealizationEpoch,
        table: FaceTableIdentity,
        host: RowsHostIdentity,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RowsHostIdentity {
    Absent,
    Inert(TypeId),
}

/// Inert pre-read owner-domain authority, never object/source references.
#[derive(Debug)]
pub struct RowsObservationEpoch {
    heap: HeapIdentity,
    collections: usize,
    context: u64,
    frame: FrameId,
    frame_authority: RowsFrameAuthority,
    faces: u64,
    owned: RowsOwnedDomain,
    dump: Option<(usize, Range<usize>)>,
    _local: PhantomData<*const ()>,
}

impl RowsObservationEpoch {
    /// Copied dump-authority range (owner index, mapped bounds) for the
    /// re-homed rows fixture; the `usize` owner key is deliberately not
    /// exported, and the range is plain data.
    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn dump_range_for_test(&self) -> Option<std::ops::Range<usize>> {
        self.dump.as_ref().map(|(_, range)| range.clone())
    }
}

/// Conditional numeric membership and mutation metadata only.
///
/// This public type does NOT bind the supplied read set to a capture interval,
/// retain dependency owners, or grant permission to read any saved pointer.
/// A private consumer must establish that this epoch preceded the WHOLE actual
/// capture before deriving physical-origin/lifetime authority from membership.
#[derive(Debug)]
pub struct RowsDependencyMetadata<'reads> {
    epoch: RowsObservationEpoch,
    reads: &'reads CollectionReads,
    _local: PhantomData<*const ()>,
}
static_assertions::assert_not_impl_any!(RowsObservationEpoch: Send, Sync);
static_assertions::assert_not_impl_any!(RowsDependencyMetadata<'static>: Send, Sync);

impl Context {
    // Closed native checks: no normalization, tracing, service, Lisp or GC.
    #[doc(hidden)]
    pub fn rows_mutator_current(&self) -> Result<(), RowsObservationRefusal> {
        if !gc::tagged_heap_is_current(&self.tagged_heap) {
            return Err(RowsObservationRefusal::InactiveContext);
        }
        if !self.tagged_heap.tagged_heap_generational_enabled() {
            return Err(RowsObservationRefusal::GenerationalDisabled);
        }
        if self.gc_driver_active {
            return Err(RowsObservationRefusal::DriverActive);
        }
        if !self.tagged_heap.rows_observation_idle() {
            return Err(RowsObservationRefusal::CollectorBusy);
        }
        Ok(())
    }

    // The original graphical/default API keeps its exact host/TTY refusals.
    #[doc(hidden)]
    pub fn rows_environment_current(&self, frame: FrameId) -> Result<(), RowsObservationRefusal> {
        self.rows_mutator_current()?;
        if self.display_host.is_some() {
            return Err(RowsObservationRefusal::DisplayHostInstalled);
        }
        let owner = self
            .frames
            .get(frame)
            .ok_or(RowsObservationRefusal::TtyOrUnknownFrame)?;
        // Strict first scope: even initial/dummy terminal frames refuse. This
        // is stronger than frame_is_tty_face_target's real-termcap predicate.
        if owner.effective_window_system().is_none() {
            return Err(RowsObservationRefusal::TtyOrUnknownFrame);
        }
        let warm = if display_query_hooks::face_realization_enabled()
            .map_err(|_| RowsObservationRefusal::DisplayQueryHooksUninstalled)?
        {
            owner
                .face_realization
                .table(self.face_change_count)
                .is_some()
        } else {
            (*self.materialized_face_table_source) == Some((frame, self.face_change_count))
        };
        if !warm {
            return Err(RowsObservationRefusal::ColdFaces);
        }
        Ok(())
    }

    /// Before the WHOLE capture_normalized, including normalization's reads.
    /// Copies native class counts and the actual retained pdump's exact bounds.
    /// It holds no BufferText Ref, Lisp Value, object/header borrow or GC root.
    pub fn begin_static_rows_observation(
        &self,
        frame: FrameId,
    ) -> Result<RowsObservationEpoch, RowsObservationRefusal> {
        self.rows_environment_current(frame)?;
        self.capture_rows_epoch(frame, RowsFrameAuthority::Graphical)
    }

    /// Explicit real-TTY metadata admission. This is not a no-callback seal:
    /// Layout must separately witness the actual canonical warm face footprint.
    /// The legacy graphical/default constructor does not call this method.
    pub fn begin_tty_rows_observation(
        &self,
        frame: FrameId,
        authority: Option<&InertRowsHostAuthority>,
    ) -> Result<RowsObservationEpoch, RowsObservationRefusal> {
        self.rows_mutator_current()?;
        let host = match (self.display_host.as_deref(), authority) {
            (None, Some(_) | None) => RowsHostIdentity::Absent,
            (Some(host), Some(authority)) if authority.accepts(host) => {
                RowsHostIdentity::Inert(authority.host_type())
            }
            (Some(_), Some(_) | None) => {
                return Err(RowsObservationRefusal::DisplayHostInstalled);
            }
        };
        if !display_query_hooks::face_realization_enabled()
            .map_err(|_| RowsObservationRefusal::DisplayQueryHooksUninstalled)?
        {
            return Err(RowsObservationRefusal::TtyLifecycleUnavailable);
        }
        let owner = self
            .frames
            .get(frame)
            .ok_or(RowsObservationRefusal::TtyOrUnknownFrame)?;
        if !display_query_hooks::frame_is_tty_target(owner)
            .map_err(|_| RowsObservationRefusal::DisplayQueryHooksUninstalled)?
        {
            return Err(RowsObservationRefusal::TtyOrUnknownFrame);
        }
        let table = owner
            .face_realization
            .table(self.face_change_count)
            .ok_or(RowsObservationRefusal::ColdFaces)?;
        let realization = owner
            .face_realization
            .realization_epoch()
            .ok_or(RowsObservationRefusal::ColdFaces)?;
        if owner
            .face_realization
            .basic(crate::emacs_core::eval::data::face::BasicFace::Default)
            .is_none()
        {
            return Err(RowsObservationRefusal::ColdFaces);
        }
        self.capture_rows_epoch(
            frame,
            RowsFrameAuthority::Tty {
                terminal: owner.terminal_id,
                realization,
                table: table.rows_snapshot_identity(),
                host,
            },
        )
    }

    #[doc(hidden)]
    pub fn capture_rows_epoch(
        &self,
        frame: FrameId,
        frame_authority: RowsFrameAuthority,
    ) -> Result<RowsObservationEpoch, RowsObservationRefusal> {
        let owned = self
            .tagged_heap
            .capture_rows_owned_domain()
            .ok_or(RowsObservationRefusal::ChunkMapDisabled)?;
        let dump = self.pdump_image.map(|image| {
            (
                image as *const _ as usize,
                image.rows_mapping_address_range(),
            )
        });
        Ok(RowsObservationEpoch {
            heap: self.tagged_heap.heap_identity(),
            collections: self.tagged_heap.gc_collections(),
            context: self.context_instance_id,
            frame,
            frame_authority,
            faces: self.face_change_count,
            owned,
            dump,
            _local: PhantomData,
        })
    }

    /// Must precede numeric-key classification, source lease and header reads.
    pub fn static_rows_observation_current(
        &self,
        epoch: &RowsObservationEpoch,
    ) -> Result<(), RowsObservationRefusal> {
        // Compare current native owners without consulting saved pointer keys.
        if !gc::tagged_heap_is_current(&self.tagged_heap) {
            return Err(RowsObservationRefusal::InactiveContext);
        }
        if self.tagged_heap.heap_identity() != epoch.heap
            || self.context_instance_id != epoch.context
        {
            return Err(RowsObservationRefusal::RuntimeChanged);
        }
        if !self.tagged_heap.tagged_heap_generational_enabled() {
            return Err(RowsObservationRefusal::GenerationalDisabled);
        }
        if self.gc_driver_active {
            return Err(RowsObservationRefusal::DriverActive);
        }
        if !self.tagged_heap.rows_observation_idle() {
            return Err(RowsObservationRefusal::CollectorBusy);
        }
        if self.tagged_heap.gc_collections() != epoch.collections {
            return Err(RowsObservationRefusal::CollectionChanged);
        }
        if self.face_change_count != epoch.faces
            || self.pdump_image.map(|image| image as *const _ as usize)
                != epoch.dump.as_ref().map(|(identity, _)| *identity)
        {
            return Err(RowsObservationRefusal::RuntimeChanged);
        }
        match &epoch.frame_authority {
            RowsFrameAuthority::Graphical => self.rows_environment_current(epoch.frame),
            RowsFrameAuthority::Tty {
                terminal,
                realization,
                table,
                host,
            } => {
                // The sealed concrete-Self default and native identities
                // only; no host-provided service, face normalization,
                // collection or saved-key read.
                let current_host = self
                    .display_host
                    .as_deref()
                    .map(|host| RowsHostIdentity::Inert(actual_rows_host_type(host)))
                    .unwrap_or(RowsHostIdentity::Absent);
                if current_host != *host {
                    return Err(RowsObservationRefusal::HostTypeChanged);
                }
                let owner = self
                    .frames
                    .get(epoch.frame)
                    .ok_or(RowsObservationRefusal::TerminalChanged)?;
                if owner.terminal_id != *terminal
                    || !display_query_hooks::frame_is_tty_target(owner)
                        .map_err(|_| RowsObservationRefusal::DisplayQueryHooksUninstalled)?
                {
                    return Err(RowsObservationRefusal::TerminalChanged);
                }
                let current_table = owner
                    .face_realization
                    .table(self.face_change_count)
                    .ok_or(RowsObservationRefusal::ColdFaces)?;
                if !owner.face_realization.retains_epoch(realization)
                    || !table.matches(current_table)
                    || owner
                        .face_realization
                        .basic(crate::emacs_core::eval::data::face::BasicFace::Default)
                        .is_none()
                {
                    return Err(RowsObservationRefusal::FaceRealizationChanged);
                }
                Ok(())
            }
        }
    }

    /// Safe numeric domain matching only, not lifetime or query admission.
    /// This does not establish that `epoch` preceded these particular `reads`.
    /// The private layout consumer owns that ordering proof at its sole actual
    /// canonical-capture call site. Old/arbitrary read sets cannot derive a
    /// lifetime theorem merely by matching this conditional metadata.
    pub fn finish_static_rows_observation<'reads>(
        &self,
        epoch: RowsObservationEpoch,
        reads: &'reads CollectionReads,
    ) -> Result<RowsDependencyMetadata<'reads>, RowsObservationRefusal> {
        self.static_rows_observation_current(&epoch)?;
        let known = reads.all_dependency_bits(|bits| {
            use neovm_value::tagged::value::{TAG_CONS, TAG_MASK, TAG_STRING, TAG_VECLIKE};
            if !matches!(bits & TAG_MASK, TAG_CONS | TAG_STRING | TAG_VECLIKE) {
                return false;
            }
            let address = bits & !TAG_MASK;
            address != 0
                && (self
                    .tagged_heap
                    .rows_dependency_in_start_domain(&epoch.owned, bits)
                    || epoch
                        .dump
                        .as_ref()
                        .is_some_and(|(_, range)| range.contains(&address)))
        });
        if !known {
            return Err(RowsObservationRefusal::UnknownOwner);
        }
        Ok(RowsDependencyMetadata {
            epoch,
            reads,
            _local: PhantomData,
        })
    }
}

impl RowsDependencyMetadata<'_> {
    /// Recover this receipt's SAME original pre-read epoch by consuming it.
    /// No capture, renewal, owner/root or geometry authority is created. The
    /// caller must still establish the original capture interval and pairing;
    /// public conditional numeric metadata alone cannot certify saved rows.
    pub fn into_original_epoch(self) -> RowsObservationEpoch {
        self.epoch
    }

    /// Recheck existing native owners/collector and numeric mutation metadata.
    /// This performs no saved-key header dereference and grants no pointer
    /// permission after return. It cannot establish the read-set interval or
    /// prevent a later callback from collecting. Only the private, ordered,
    /// inert Layout consumer derives a physical-owner proof from these facts.
    pub fn current_and_observe(&self, context: &Context) -> bool {
        context.static_rows_observation_current(&self.epoch).is_ok()
            && self.reads.unchanged_and_observe()
    }
}

#[cfg(test)]
#[path = "tests/tty_rows_observation_test.rs"]
mod tty_tests;
