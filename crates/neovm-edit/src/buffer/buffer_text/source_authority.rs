//! Read-only authority for motion observations of one shared live text owner.
//! Raw property-table installation retains its existing paired-byte-edit or
//! fresh-construction precondition; see BufferText::text_props_replace.
use super::BufferTextStorage;
use crate::buffer::Buffer;
use std::cell::{Ref, RefCell};
use std::rc::{Rc, Weak};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SourceRevision {
    content_epoch: u64,
    all_properties: u64,
}

impl SourceRevision {
    fn of(source: &BufferTextStorage) -> Self {
        Self {
            content_epoch: source.content_epoch,
            all_properties: source.text_props.mutation_tick(),
        }
    }
}

/// Motion-observation authority for one mutator's shared live text owner.
/// This owns no inner byte/property storage or Lisp Value, adds no edit work,
/// and is not a root/lifetime certificate for nested Lisp dependencies.
#[derive(Clone, Debug)]
pub struct BufferSourceAuthority {
    owner: Weak<RefCell<BufferTextStorage>>,
    revision: SourceRevision,
}

/// Short read phase only; do not retain across Lisp, edits or safepoints.
/// Opaque until stage3 closes the read/lifetime contract. In particular this
/// provides no public dereference, raw Value, mutable borrow or owner escape.
#[must_use]
pub struct BufferSourceReadLease<'source> {
    // Borrow from the live source Buffer, not from the local upgraded Rc.
    // The field lifetime makes this non-self-referential and the actual Ref
    // excludes writes through any indirect alias of the same outer owner.
    _storage: Ref<'source, BufferTextStorage>,
    _owner: Rc<RefCell<BufferTextStorage>>,
}

impl Buffer {
    /// Capture existing text/property authority; performs no mutation.
    pub fn capture_motion_source_authority(&self) -> Option<BufferSourceAuthority> {
        if self.name_value().is_nil() {
            return None;
        }
        let storage = self.text.storage.try_borrow().ok()?;
        let revision = SourceRevision::of(&storage);
        Some(BufferSourceAuthority {
            owner: Rc::downgrade(&self.text.storage),
            revision,
        })
    }
}

impl BufferSourceAuthority {
    /// Validate against the requested current live source. Query context,
    /// logical buffer/window owner, restrictions and source dependencies remain
    /// separately mandatory; indirect buffers may share this text authority.
    ///
    /// # Safety
    /// The caller must establish a closed inert native read phase and keep
    /// the returned lease entirely within it. No Lisp, callbacks, reentry,
    /// collection, safepoints or source mutation may occur while that lease
    /// is alive. Drop the lease before leaving the phase. Its opaque Ref
    /// prevents checked storage writes but does not exclude the collector's
    /// existing raw marker-head access or establish nested Lisp lifetimes.
    pub unsafe fn validate<'source>(
        &self,
        source: &'source Buffer,
    ) -> Option<BufferSourceReadLease<'source>> {
        if source.name_value().is_nil() {
            return None;
        }
        let owner = self.owner.upgrade()?;
        if !Rc::ptr_eq(&owner, &source.text.storage) {
            return None;
        }
        let storage = source.text.storage.try_borrow().ok()?;
        if SourceRevision::of(&storage) != self.revision {
            return None;
        }
        Some(BufferSourceReadLease {
            _storage: storage,
            _owner: owner,
        })
    }
}

static_assertions::assert_not_impl_any!(BufferSourceAuthority: Send, Sync);
static_assertions::assert_not_impl_any!(BufferSourceReadLease<'static>: Send, Sync);

impl std::fmt::Debug for BufferSourceReadLease<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BufferSourceReadLease")
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
#[path = "tests/source_authority_test.rs"]
mod tests;
