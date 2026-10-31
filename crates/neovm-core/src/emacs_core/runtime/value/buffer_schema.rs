//! Value-layer buffer vocabulary: slot indices, slot-write predicates, and
//! buffer identity. Owned by the value layer so the lower modules can name
//! these types without referencing the buffer layer.
use super::{Value, ValueKind};

// ---------------------------------------------------------------------------
// BUFFER_SLOT_COUNT — sized to mirror GNU's `MAX_PER_BUFFER_VARS = 50`.
// ---------------------------------------------------------------------------

/// Number of `BUFFER_OBJFWD` slots in the buffer slot array. Mirrors GNU's
/// `MAX_PER_BUFFER_VARS = 50` limit on per-buffer C-side variables
/// (`buffer.h:311`). Bumped to 64 in Phase 10D so the conditional
/// `BUFFER_OBJFWD` slots (mode-line-format, fill-column, …) have room
/// alongside the always-local set already migrated in Phase 10A-C.
/// Sized to a power of two so the buffer local-flags bitmap (a `u64`)
/// covers exactly one bit per slot. Bump again only after a
/// careful audit — the number bounds every Buffer's memory footprint.
pub const BUFFER_SLOT_COUNT: usize = 64;

/// Index into the buffer slot array.  This is a compact Rust representation of
/// GNU's per-buffer slot domain: GNU stores byte offsets into `struct buffer`,
/// while Neomacs stores dense slot indices into a fixed `[Value; 64]`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BufferSlot(u8);

impl BufferSlot {
    pub const fn new(index: usize) -> Self {
        assert!(index < BUFFER_SLOT_COUNT);
        Self(index as u8)
    }

    pub const fn index(self) -> usize {
        self.0 as usize
    }

    pub const fn as_u16(self) -> u16 {
        self.0 as u16
    }

    pub const fn local_flags_idx(self) -> i16 {
        self.0 as i16
    }

    pub fn from_u16(index: u16) -> Option<Self> {
        if usize::from(index) < BUFFER_SLOT_COUNT {
            Some(Self(index as u8))
        } else {
            None
        }
    }
}

/// The closed predicate domain accepted by GNU's `DEFVAR_PER_BUFFER`.
///
/// GNU stores this as `enum Lisp_Fwd_Predicate`; keeping the same domain as a
/// Rust enum prevents misspelled predicate names and makes new upstream
/// variants an explicit exhaustiveness failure instead of a silent no-op.
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum BufferSlotPredicate {
    Unrestricted,
    String,
    Symbol,
    Integer,
    Number,
    Fraction,
    VerticalScrollBar,
    OverwriteMode,
}

/// A predicate failure independent of the evaluator's non-local-control-flow
/// representation.  The evaluator maps this typed result to GNU-compatible
/// Lisp signal data at the storage boundary.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum BufferSlotPredicateError {
    WrongType(&'static str),
    Choice(&'static str),
    Range(&'static str),
}

impl BufferSlotPredicate {
    /// Check a value before it is stored in a live buffer slot.
    ///
    /// GNU's `store_symval_forwarding` bypasses every predicate for nil, even
    /// predicates such as `stringp` and `integerp` that would reject nil when
    /// called as ordinary Lisp functions.
    pub fn check(self, value: Value) -> Result<(), BufferSlotPredicateError> {
        use BufferSlotPredicateError::{Choice, Range, WrongType};

        if value.is_nil() || self == Self::Unrestricted {
            return Ok(());
        }

        match self {
            Self::Unrestricted => Ok(()),
            Self::String if value.is_string() => Ok(()),
            Self::String => Err(WrongType("stringp")),
            Self::Symbol if value.is_bare_symbol() => Ok(()),
            Self::Symbol => Err(WrongType("symbolp")),
            Self::Integer if value.is_integer() => Ok(()),
            Self::Integer => Err(WrongType("integerp")),
            Self::Number if value.is_number() => Ok(()),
            Self::Number => Err(WrongType("numberp")),
            Self::Fraction if !value.is_number() => Err(WrongType("numberp")),
            Self::Fraction => {
                let in_range = match value.kind() {
                    ValueKind::Fixnum(number) => (0..=1).contains(&number),
                    ValueKind::Float => (0.0..=1.0).contains(&value.xfloat()),
                    ValueKind::Veclike(crate::tagged::header::VecLikeType::Bignum) => value
                        .as_bignum()
                        .is_some_and(|number| (&0..=&1).contains(&number)),
                    _ => unreachable!("numberp was checked above"),
                };
                if in_range {
                    Ok(())
                } else {
                    Err(Range("Value should be from 0.0 to 1.0"))
                }
            }
            Self::VerticalScrollBar
                if value.is_t()
                    || value.is_bare_symbol_named("left")
                    || value.is_bare_symbol_named("right") =>
            {
                Ok(())
            }
            Self::VerticalScrollBar => {
                Err(Choice("One of nil, t, left or right should be specified"))
            }
            Self::OverwriteMode
                if value.is_bare_symbol_named("overwrite-mode-textual")
                    || value.is_bare_symbol_named("overwrite-mode-binary") =>
            {
                Ok(())
            }
            Self::OverwriteMode => Err(Choice(
                "One of nil, overwrite-mode-textual or overwrite-mode-binary should be specified",
            )),
        }
    }
}

/// Opaque, cheaply-copyable identifier for a buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BufferId(pub u64);
