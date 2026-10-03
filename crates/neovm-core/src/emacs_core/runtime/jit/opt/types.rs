//! A kind-set lattice with optional fixnum intervals and exact constant bits.
//!
//! Threading: immutable compiler facts, transferable without consulting a Lisp
//! heap or mutator-local interner. Vectorlike constants are classified
//! conservatively from their tag; their precise kind is supplied by the front.

use super::ir::ValueBits;
use crate::emacs_core::value::Value as LispValue;
use crate::tagged::value::{FIXNUM_CHECK_MASK, FIXNUM_CHECK_VALUE, FIXNUM_SHIFT};
use std::fmt;

/// Lisp kind bit; threading: immutable, independent of mutator heap headers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub(crate) enum TypeKind {
    Fixnum,
    Nil,
    T,
    OtherSymbol,
    Cons,
    String,
    Vector,
    Record,
    Float,
    Bignum,
    Marker,
    OtherVeclike,
}

impl TypeKind {
    const fn bit(self) -> u16 {
        1 << self as u8
    }
}

/// Inclusive interval over the exact tagged-fixnum domain. Threading:
/// immutable compiler data; widening depends only on the previous interval.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Range {
    pub lo: i64,
    pub hi: i64,
}

impl Range {
    pub(crate) const FULL: Self = Self {
        lo: LispValue::MOST_NEGATIVE_FIXNUM,
        hi: LispValue::MOST_POSITIVE_FIXNUM,
    };

    pub(crate) fn join(self, other: Self) -> Self {
        Self {
            lo: self.lo.min(other.lo),
            hi: self.hi.max(other.hi),
        }
    }

    pub(crate) fn meet(self, other: Self) -> Option<Self> {
        let result = Self {
            lo: self.lo.max(other.lo),
            hi: self.hi.min(other.hi),
        };
        (result.lo <= result.hi).then_some(result)
    }

    /// An expanding bound immediately reaches the domain boundary, ensuring
    /// finite termination at loop headers without rounding/overflow tricks.
    pub(crate) fn widen(self, next: Self) -> Self {
        Self {
            lo: if next.lo < self.lo {
                Self::FULL.lo
            } else {
                self.lo
            },
            hi: if next.hi > self.hi {
                Self::FULL.hi
            } else {
                self.hi
            },
        }
    }
}

/// The bottom set is empty; top contains every kind. Fixnum intervals apply
/// only to the Fixnum portion. A singleton preserves identity-bearing constant
/// bits without dereferencing them. Threading: immutable compiler-owned facts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TypeSet {
    kinds: u16,
    range: Option<Range>,
    singleton: Option<ValueBits>,
}

impl TypeSet {
    const fn kinds(kinds: u16) -> Self {
        Self {
            kinds,
            range: None,
            singleton: None,
        }
    }

    pub(crate) const BOTTOM: Self = Self::kinds(0);
    pub(crate) const TOP: Self = Self::kinds((1 << 12) - 1);
    pub(crate) const FIXNUM: Self = Self::kind(TypeKind::Fixnum);
    pub(crate) const NIL: Self = Self {
        kinds: TypeKind::Nil.bit(),
        range: None,
        singleton: Some(ValueBits(0)),
    };
    pub(crate) const T: Self = Self {
        kinds: TypeKind::T.bit(),
        range: None,
        singleton: Some(ValueBits(8)),
    };
    pub(crate) const OTHER_SYMBOL: Self = Self::kind(TypeKind::OtherSymbol);
    pub(crate) const CONS: Self = Self::kind(TypeKind::Cons);
    pub(crate) const STRING: Self = Self::kind(TypeKind::String);
    pub(crate) const VECTOR: Self = Self::kind(TypeKind::Vector);
    pub(crate) const RECORD: Self = Self::kind(TypeKind::Record);
    pub(crate) const FLOAT: Self = Self::kind(TypeKind::Float);
    pub(crate) const BIGNUM: Self = Self::kind(TypeKind::Bignum);
    pub(crate) const MARKER: Self = Self::kind(TypeKind::Marker);
    pub(crate) const OTHER_VECLIKE: Self = Self::kind(TypeKind::OtherVeclike);
    pub(crate) const LIST: Self = Self::kinds(TypeKind::Cons.bit() | TypeKind::Nil.bit());
    pub(crate) const SYMBOL: Self =
        Self::kinds(TypeKind::Nil.bit() | TypeKind::T.bit() | TypeKind::OtherSymbol.bit());
    pub(crate) const INTEGER: Self = Self::kinds(TypeKind::Fixnum.bit() | TypeKind::Bignum.bit());
    pub(crate) const NUMBER: Self =
        Self::kinds(TypeKind::Fixnum.bit() | TypeKind::Bignum.bit() | TypeKind::Float.bit());
    pub(crate) const NUMBER_OR_MARKER: Self =
        Self::kinds(Self::NUMBER.kinds | TypeKind::Marker.bit());
    pub(crate) const BOOLEAN: Self = Self::kinds(TypeKind::Nil.bit() | TypeKind::T.bit());
    pub(crate) const HEAP: Self = Self::kinds(
        TypeKind::Cons.bit()
            | TypeKind::String.bit()
            | TypeKind::Vector.bit()
            | TypeKind::Record.bit()
            | TypeKind::Float.bit()
            | TypeKind::Bignum.bit()
            | TypeKind::Marker.bit()
            | TypeKind::OtherVeclike.bit(),
    );

    pub(crate) const fn kind(kind: TypeKind) -> Self {
        match kind {
            TypeKind::Nil => Self::NIL,
            TypeKind::T => Self::T,
            _ => Self::kinds(kind.bit()),
        }
    }

    pub(crate) const fn contains(self, kind: TypeKind) -> bool {
        self.kinds & kind.bit() != 0
    }

    pub(crate) const fn is_bottom(self) -> bool {
        self.kinds == 0
    }

    pub(crate) const fn singleton(self) -> Option<ValueBits> {
        self.singleton
    }

    pub(crate) fn range(self) -> Option<Range> {
        self.contains(TypeKind::Fixnum)
            .then(|| self.range.unwrap_or(Range::FULL))
    }

    pub(crate) fn fixnum_range(range: Range) -> Self {
        Self {
            kinds: TypeKind::Fixnum.bit(),
            range: Some(range),
            singleton: None,
        }
        .normalize()
    }

    pub(crate) fn with_singleton(self, bits: ValueBits) -> Self {
        self.meet(Self::for_constant(bits))
    }

    /// Pure tag classification. Veclike header inspection belongs to the
    /// mutator front; workers must not turn opaque bits into heap references.
    pub(crate) fn for_constant(bits: ValueBits) -> Self {
        if bits.0 == 0 {
            return Self::NIL;
        }
        if bits.0 == 8 {
            return Self::T;
        }
        if bits.0 & FIXNUM_CHECK_MASK as u64 == FIXNUM_CHECK_VALUE as u64 {
            let fix = (bits.0 as i64) >> FIXNUM_SHIFT;
            return Self::fixnum_range(Range { lo: fix, hi: fix });
        }
        let ty = match bits.0 & 7 {
            0 => Self::OTHER_SYMBOL,
            3 => Self::CONS,
            4 => Self::STRING,
            5 => Self::VECTOR
                .join(Self::RECORD)
                .join(Self::BIGNUM)
                .join(Self::MARKER)
                .join(Self::OTHER_VECLIKE),
            7 => Self::FLOAT,
            _ => Self::TOP,
        };
        Self {
            singleton: Some(bits),
            ..ty
        }
        .normalize()
    }

    pub(crate) fn join(self, other: Self) -> Self {
        if self.is_bottom() {
            return other;
        }
        if other.is_bottom() {
            return self;
        }
        let range = match (self.range(), other.range()) {
            (Some(a), Some(b)) => Some(a.join(b)),
            (Some(a), None) | (None, Some(a)) => Some(a),
            (None, None) => None,
        };
        Self {
            kinds: self.kinds | other.kinds,
            range,
            singleton: self.singleton.filter(|bits| other.singleton == Some(*bits)),
        }
        .normalize()
    }

    pub(crate) fn meet(self, other: Self) -> Self {
        if self.is_bottom() || other.is_bottom() {
            return Self::BOTTOM;
        }
        if matches!((self.singleton, other.singleton), (Some(a), Some(b)) if a != b) {
            return Self::BOTTOM;
        }
        let mut kinds = self.kinds & other.kinds;
        let range = match (self.range(), other.range()) {
            (Some(a), Some(b)) => match a.meet(b) {
                Some(range) => Some(range),
                None => {
                    kinds &= !TypeKind::Fixnum.bit();
                    None
                }
            },
            _ => None,
        };
        let result = Self {
            kinds,
            range,
            singleton: self.singleton.or(other.singleton),
        };
        // A singleton fixnum outside the remaining interval is impossible.
        if let Some(bits) = result.singleton {
            if bits.0 & FIXNUM_CHECK_MASK as u64 == FIXNUM_CHECK_VALUE as u64 {
                let value = bits.0 as i64 >> FIXNUM_SHIFT;
                if !result.contains(TypeKind::Fixnum)
                    || result.range().is_some_and(|r| value < r.lo || value > r.hi)
                {
                    return Self::BOTTOM;
                }
            }
        }
        result.normalize()
    }

    /// Conservative subtraction, used on false type-test edges. Excluding a
    /// particular heap identity cannot remove its whole kind. An interval
    /// with a hole is not representable and therefore retains its old range.
    pub(crate) fn without(self, other: Self) -> Self {
        let mut remove = other.kinds;
        if other.singleton.is_some() {
            if self.singleton == other.singleton {
                return Self::BOTTOM;
            }
            remove &= TypeKind::Nil.bit() | TypeKind::T.bit();
        }
        if other.range().is_some_and(|r| r != Range::FULL) {
            remove &= !TypeKind::Fixnum.bit();
        }
        Self {
            kinds: self.kinds & !remove,
            ..self
        }
        .normalize()
    }

    pub(crate) fn is_subset(self, other: Self) -> bool {
        if self.is_bottom() {
            return true;
        }
        if self.kinds & !other.kinds != 0 {
            return false;
        }
        if let Some(expected) = other.singleton {
            if self.singleton != Some(expected) {
                return false;
            }
        }
        match (self.range(), other.range()) {
            (Some(a), Some(b)) => a.lo >= b.lo && a.hi <= b.hi,
            _ => true,
        }
    }

    pub(crate) fn may_need_root(self) -> bool {
        !self.meet(Self::HEAP).is_bottom()
    }

    pub(crate) fn widen(self, next: Self) -> Self {
        let union = self.join(next);
        match (self.range(), next.range()) {
            (Some(a), Some(b)) => Self {
                range: Some(a.widen(b)),
                singleton: None,
                ..union
            }
            .normalize(),
            _ => union,
        }
    }

    /// These predicates may consult `symbols-with-pos-enabled` for a veclike
    /// symbol-with-position. Type facts alone do not authorize their folding.
    pub(crate) fn permits_symbol_identity_folding(self) -> bool {
        !self.contains(TypeKind::OtherVeclike)
    }

    pub(crate) fn inverse_car_non_nil(self, loaded: Self) -> Self {
        if self.is_subset(Self::LIST) && !loaded.contains(TypeKind::Nil) {
            self.meet(Self::CONS)
        } else {
            self
        }
    }

    fn normalize(mut self) -> Self {
        if self.kinds == 0 {
            return Self::BOTTOM;
        }
        if !self.contains(TypeKind::Fixnum) {
            self.range = None;
        } else if let Some(range) = self.range {
            let lo = range.lo.max(Range::FULL.lo);
            let hi = range.hi.min(Range::FULL.hi);
            if lo > hi {
                self.kinds &= !TypeKind::Fixnum.bit();
                self.range = None;
                return self.normalize();
            }
            self.range = (Range { lo, hi } != Range::FULL).then_some(Range { lo, hi });
            if self.kinds == TypeKind::Fixnum.bit() && lo == hi {
                self.singleton = Some(ValueBits(
                    ((lo as u64) << FIXNUM_SHIFT) | FIXNUM_CHECK_VALUE as u64,
                ));
            }
        }
        if self.kinds == TypeKind::Nil.bit() {
            return Self::NIL;
        }
        if self.kinds == TypeKind::T.bit() {
            return Self::T;
        }
        self
    }
}

impl fmt::Display for TypeSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_bottom() {
            return write!(f, "Bottom");
        }
        let mut separator = "";
        for kind in [
            TypeKind::Fixnum,
            TypeKind::Nil,
            TypeKind::T,
            TypeKind::OtherSymbol,
            TypeKind::Cons,
            TypeKind::String,
            TypeKind::Vector,
            TypeKind::Record,
            TypeKind::Float,
            TypeKind::Bignum,
            TypeKind::Marker,
            TypeKind::OtherVeclike,
        ] {
            if self.contains(kind) {
                write!(f, "{separator}{kind:?}")?;
                separator = "|";
            }
        }
        if let Some(range) = self.range {
            write!(f, "[{}..={}]", range.lo, range.hi)?;
        }
        if let Some(bits) = self.singleton {
            write!(f, "@0x{:016x}", bits.0)?;
        }
        Ok(())
    }
}
