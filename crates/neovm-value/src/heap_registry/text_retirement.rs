//! Cold, local cache retirement. Outcomes describe an observed effect. They
//! are not World or GC authority, and they are not proof that later code has
//! left the cache empty.
//!
//! Retire disposable entries before the final source census. A later
//! collecting drain could otherwise free cache-only words already captured by
//! an unregistered frozen root. Canonical Context fields, not these outcome
//! records, retain the standard tables. This module names no text type.

use crate::tagged::gc::HeapIdentity;
use crate::value::Value;
use std::cell::{Cell, RefCell, RefMut};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[doc(hidden)]
pub enum TextCacheRetirement {
    Retired(HeapIdentity),
    AlreadyInactive(HeapIdentity),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[doc(hidden)]
pub enum TextCacheSlot {
    CaseTable,
    SyntaxTable,
    SyntaxCodes,
    CategoryTable,
    RegexSearch,
    RegexLisp,
    RegexLiteral,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[doc(hidden)]
pub enum TextCacheRetireError {
    Borrowed(TextCacheSlot),
}

enum PreparedValueSlot<'a> {
    Inactive,
    Active {
        value: RefMut<'a, Option<Value>>,
        owner: &'a Cell<usize>,
    },
}

/// Holds a matching slot loan without changing state. Preparing every
/// component before committing makes a multi-slot refusal transactional.
///
/// The loan is the right. There is no constructor besides [`Self::prepare`].
/// The value is not `Send`, `Sync`, `Clone`, or `Copy`.
#[must_use]
#[doc(hidden)]
pub struct PreparedValueCacheRetirement<'a> {
    heap: HeapIdentity,
    slot: PreparedValueSlot<'a>,
}

impl<'a> PreparedValueCacheRetirement<'a> {
    #[doc(hidden)]
    pub fn prepare(
        value: &'a RefCell<Option<Value>>,
        owner: &'a Cell<usize>,
        heap: HeapIdentity,
        kind: TextCacheSlot,
    ) -> Result<Self, TextCacheRetireError> {
        let slot = if owner.get() == heap.get() {
            PreparedValueSlot::Active {
                value: value
                    .try_borrow_mut()
                    .map_err(|_| TextCacheRetireError::Borrowed(kind))?,
                owner,
            }
        } else {
            PreparedValueSlot::Inactive
        };
        Ok(Self { heap, slot })
    }

    #[doc(hidden)]
    pub fn commit(self) -> TextCacheRetirement {
        match self.slot {
            PreparedValueSlot::Inactive => TextCacheRetirement::AlreadyInactive(self.heap),
            PreparedValueSlot::Active { mut value, owner } => {
                let retired = value.take();
                owner.set(0);
                drop(value);
                // `Value` is a non-owning word and has no destructor. No heap
                // dereference runs while another prepared slot is still live.
                let _ = retired;
                TextCacheRetirement::Retired(self.heap)
            }
        }
    }
}

/// The plain word text crates store beside a cache. [`HeapIdentity::get`]
/// stays crate-private.
#[doc(hidden)]
pub fn heap_identity_word(heap: HeapIdentity) -> usize {
    heap.get()
}

static_assertions::assert_not_impl_any!(PreparedValueCacheRetirement<'static>: Send, Sync, Clone, Copy);
