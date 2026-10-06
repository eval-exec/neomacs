//! Values that need no GC root and mean the same thing in every heap.

use std::fmt;

use crate::emacs_core::intern::{intern, is_canonical_id};
use crate::tagged::value::{TaggedValue, ValueKind};

/// A fixnum or an interned (canonical) symbol, `nil` and `t` included.
///
/// These words name no heap object: a fixnum is its own payload, and a
/// canonical symbol's id is process-global and kept alive by the obarray. So
/// unlike a raw [`TaggedValue`], this type may live in statics and cross
/// threads. Heap objects (and uninterned symbols, whose cells belong to one
/// heap) travel in a [`SharedRoot`](super::SharedRoot) instead.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct ImmediateValue(usize);

static_assertions::assert_impl_all!(ImmediateValue: Send, Sync, Copy, fmt::Debug);
static_assertions::assert_eq_size!(ImmediateValue, TaggedValue);
static_assertions::assert_eq_align!(ImmediateValue, TaggedValue);

/// Why a value cannot be an [`ImmediateValue`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum NotImmediate {
    /// The value is a heap object; share it through a `SharedRoot`.
    #[error("a heap object needs a rooted transport")]
    HeapObject,
    /// The symbol is not interned, so its cells are heap-local.
    #[error("an uninterned symbol needs a rooted transport")]
    UninternedSymbol,
}

impl ImmediateValue {
    /// The symbol `nil`.
    pub const NIL: Self = Self(TaggedValue::NIL.0);
    /// The symbol `t`.
    pub const T: Self = Self(TaggedValue::T.0);

    /// The canonical symbol named `name`, interning it if needed.
    pub fn interned(name: &str) -> Self {
        Self(TaggedValue::from_sym_id(intern(name)).bits())
    }

    /// The fixnum `n`, or `None` outside the fixnum range.
    pub fn fixnum(n: i64) -> Option<Self> {
        (TaggedValue::MOST_NEGATIVE_FIXNUM..=TaggedValue::MOST_POSITIVE_FIXNUM)
            .contains(&n)
            .then(|| Self(TaggedValue::fixnum(n).bits()))
    }

    /// `value` when it is a fixnum, `nil` or `t`: the words that need no root
    /// and no symbol-registry lookup.
    #[inline]
    pub(crate) fn untraced(value: TaggedValue) -> Option<Self> {
        (value.is_fixnum() || value.is_nil() || value.is_t()).then_some(Self(value.bits()))
    }

    /// The local value. Free: both types are the same word.
    #[inline]
    pub const fn value(self) -> TaggedValue {
        TaggedValue::from_bits(self.0)
    }
}

impl TryFrom<TaggedValue> for ImmediateValue {
    type Error = NotImmediate;

    fn try_from(value: TaggedValue) -> Result<Self, Self::Error> {
        if let Some(immediate) = Self::untraced(value) {
            return Ok(immediate);
        }
        match value.as_symbol_id() {
            Some(id) if is_canonical_id(id) => Ok(Self(value.bits())),
            Some(_) => Err(NotImmediate::UninternedSymbol),
            None => Err(NotImmediate::HeapObject),
        }
    }
}

impl From<ImmediateValue> for TaggedValue {
    #[inline]
    fn from(immediate: ImmediateValue) -> Self {
        immediate.value()
    }
}

impl fmt::Debug for ImmediateValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.value().kind() {
            ValueKind::Nil => f.write_str("ImmediateValue(nil)"),
            ValueKind::T => f.write_str("ImmediateValue(t)"),
            ValueKind::Fixnum(n) => write!(f, "ImmediateValue({n})"),
            ValueKind::Symbol(id) => write!(f, "ImmediateValue(Symbol({id:?}))"),
            // Unreachable through the validating constructors.
            ValueKind::Cons
            | ValueKind::String
            | ValueKind::Float
            | ValueKind::Subr(_)
            | ValueKind::Veclike(_)
            | ValueKind::Unbound
            | ValueKind::Unknown => write!(f, "ImmediateValue({:#x})", self.0),
        }
    }
}
