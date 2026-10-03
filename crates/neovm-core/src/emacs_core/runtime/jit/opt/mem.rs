//! Memory dependencies shared with the baseline effect declarations.
//!
//! Threading: immutable compiler facts, never runtime cache state. Heap facts
//! are conservative across Lisp calls and poll slow paths, including GC hooks.

use crate::emacs_core::intern::SymId;
pub(crate) use crate::emacs_core::subr::leaf::Effects;

/// Memory dependency category; threading: immutable compiler facts, including
/// stable symbol IDs rather than a mutable symbol-cell pointer.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) enum AliasClass {
    #[default]
    None,
    Unknown,
    ConsCar,
    ConsCdr,
    VecElem,
    RecElem,
    StrData,
    SymValue(SymId),
    Buffer,
    Match,
    Bindings,
    Immutable,
}

impl AliasClass {
    /// A precise store kills only its class. Distinct constant vector/record
    /// indices cannot alias, even when object identity is unknown.
    pub(crate) fn store_clobbers(
        self,
        load: Self,
        store_index: Option<i64>,
        load_index: Option<i64>,
    ) -> bool {
        if matches!(load, Self::None | Self::Immutable) || self == Self::None {
            return false;
        }
        if self == Self::Unknown || load == Self::Unknown {
            return true;
        }
        if self != load {
            return false;
        }
        if matches!(self, Self::VecElem | Self::RecElem) {
            if let (Some(a), Some(b)) = (store_index, load_index) {
                return a == b;
            }
        }
        true
    }

    pub(crate) fn clobbered_by(self, effects: Effects) -> bool {
        if matches!(self, Self::None | Self::Immutable) {
            return false;
        }
        if effects.intersects(Effects::MAY_REENTER.with(Effects::MAY_GC)) {
            return true;
        }
        match self {
            Self::ConsCar | Self::ConsCdr | Self::VecElem | Self::RecElem | Self::StrData => {
                effects.intersects(Effects::WRITE_HEAP)
            }
            Self::SymValue(_) | Self::Bindings => effects.intersects(Effects::WRITE_BINDINGS),
            Self::Buffer => effects.intersects(Effects::WRITE_BUFFER),
            Self::Match => effects.intersects(Effects::WRITE_MATCH),
            Self::Unknown => effects.intersects(
                Effects::WRITE_HEAP
                    .with(Effects::WRITE_BUFFER)
                    .with(Effects::WRITE_MATCH)
                    .with(Effects::WRITE_BINDINGS),
            ),
            Self::None | Self::Immutable => false,
        }
    }
}
