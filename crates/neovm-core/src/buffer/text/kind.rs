use num_enum::{IntoPrimitive, TryFromPrimitive};
use strum::{EnumIter, EnumString, IntoEnumIterator, IntoStaticStr};

#[repr(u8)]
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    Hash,
    EnumIter,
    EnumString,
    IntoPrimitive,
    IntoStaticStr,
    TryFromPrimitive,
)]
#[strum(serialize_all = "kebab-case")]
pub enum BufferTextBackendKind {
    #[default]
    GapBuffer = 0,
    PieceTree = 1,
    Rope = 2,
}

/// Buffer backend kind after rejecting unsupported public choices.
///
/// Keep this as a private wrapper instead of a second enum: Lisp-visible
/// symbol spelling and pdump tag values belong to `BufferTextBackendKind`,
/// while internal constructors require this validated type.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub(crate) struct ImplementedBufferTextBackendKind(BufferTextBackendKind);

impl BufferTextBackendKind {
    pub fn variants() -> impl Iterator<Item = Self> {
        Self::iter()
    }

    pub fn implemented_variants() -> impl Iterator<Item = Self> {
        Self::variants().filter(|kind| kind.is_implemented())
    }

    pub fn non_gap_implemented_variants() -> impl Iterator<Item = Self> {
        Self::implemented_variants().filter(|kind| !kind.is_gap_buffer())
    }

    pub fn symbol_name(self) -> &'static str {
        self.into()
    }

    pub const fn is_gap_buffer(self) -> bool {
        matches!(self, Self::GapBuffer)
    }

    pub fn is_implemented(self) -> bool {
        self.implemented().is_some()
    }

    pub(crate) fn implemented(self) -> Option<ImplementedBufferTextBackendKind> {
        match self {
            Self::GapBuffer => Some(ImplementedBufferTextBackendKind::GAP_BUFFER),
            Self::PieceTree => Some(ImplementedBufferTextBackendKind::PIECE_TREE),
            Self::Rope => Some(ImplementedBufferTextBackendKind::ROPE),
        }
    }
}

impl ImplementedBufferTextBackendKind {
    pub(crate) const GAP_BUFFER: Self = Self(BufferTextBackendKind::GapBuffer);
    pub(crate) const PIECE_TREE: Self = Self(BufferTextBackendKind::PieceTree);
    pub(crate) const ROPE: Self = Self(BufferTextBackendKind::Rope);

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn variants() -> impl Iterator<Item = Self> {
        BufferTextBackendKind::implemented_variants().filter_map(BufferTextBackendKind::implemented)
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn non_gap_variants() -> impl Iterator<Item = Self> {
        Self::variants().filter(|kind| !kind.is_gap_buffer())
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn symbol_name(self) -> &'static str {
        self.0.symbol_name()
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) const fn is_gap_buffer(self) -> bool {
        self.0.is_gap_buffer()
    }

    pub(crate) fn public_kind(self) -> BufferTextBackendKind {
        self.0
    }
}

impl TryFrom<BufferTextBackendKind> for ImplementedBufferTextBackendKind {
    type Error = BufferTextBackendKind;

    fn try_from(kind: BufferTextBackendKind) -> Result<Self, Self::Error> {
        kind.implemented().ok_or(kind)
    }
}

impl From<ImplementedBufferTextBackendKind> for BufferTextBackendKind {
    fn from(kind: ImplementedBufferTextBackendKind) -> Self {
        kind.public_kind()
    }
}
