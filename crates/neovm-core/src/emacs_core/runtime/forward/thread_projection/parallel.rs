//! Explicit symbol-writer routing. No serial owner accessor calls this file.
use super::*;
use crate::tagged::gc::SymbolWriteToken;

impl ThreadProjectionDescriptor {
    pub(crate) fn parallel_store_plain(&self, value: Value, _: &SymbolWriteToken<'_>) -> bool {
        if self.is_void() || self.arm() != CanonicalArm::Plain || self.host_projected() {
            return false;
        }
        // The owning era excludes concurrent marking and traces current World
        // roots at stop-all. Bypass ONLY the legacy TLS SATB note, preserving
        // its serial path and the actual interior-mutable atomic storage.
        self.plain
            .value
            .0
            .store(value.bits(), std::sync::atomic::Ordering::Release);
        true
    }
}

impl ThreadProjectionTable {
    /// Reuse this World's already-created VOID; never consult the process
    /// projection-scope mutex or allocate a descriptor during name publication.
    pub(crate) fn parallel_void_header(&self, _: &SymbolWriteToken<'_>) -> Option<FwdRef> {
        self.void.map(ThreadProjectionDescriptor::header)
    }

    pub(crate) fn parallel_storage_bytes(&self, _: &SymbolWriteToken<'_>) -> usize {
        let slots = self
            .free
            .len()
            .saturating_add(self.issued.len())
            .saturating_add(usize::from(self.void.is_some()));
        slots
            .saturating_mul(std::mem::size_of::<ThreadProjectionDescriptor>())
            .saturating_add(
                self.free
                    .capacity()
                    .saturating_mul(std::mem::size_of::<usize>()),
            )
            .saturating_add(
                self.issued
                    .capacity()
                    .saturating_mul(std::mem::size_of::<usize>()),
            )
    }

    /// Fallible counterpart to serial issue/place. All reservation and index
    /// checks precede publication; neither exhaustion nor a missing free slot
    /// has an expect/panic edge. The writer charges measured storage growth.
    pub(crate) fn issue_plain_parallel(
        &mut self,
        value: Value,
        _: &SymbolWriteToken<'_>,
    ) -> Result<&'static ThreadProjectionDescriptor, ProjectionCapacityError> {
        let next = self
            .issued
            .len()
            .checked_add(2)
            .and_then(|n| u32::try_from(n).ok())
            .and_then(NonZeroU32::new)
            .ok_or(ProjectionCapacityError)?;
        self.try_reserve(1)?;
        let slot = self.free.pop().ok_or(ProjectionCapacityError)?;
        let descriptor: &'static ThreadProjectionDescriptor =
            slot.write(ThreadProjectionDescriptor {
                header: LispBufferObjFwd {
                    ty: super::super::LispFwdType::BufferObj,
                    offset: THREAD_PROJECTION_OFFSET,
                    local_flags_idx: -1,
                    predicate:
                        crate::emacs_core::value::buffer_vocab::BufferSlotPredicate::Unrestricted,
                    default: Value::UNBOUND,
                },
                index: TlsIndex(next),
                plain: super::super::new_objfwd(value),
                arm: std::cell::Cell::new(CanonicalArm::Plain),
                host_projected: false,
            });
        self.issued.push(descriptor);
        Ok(descriptor)
    }
}
