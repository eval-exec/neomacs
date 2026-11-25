//! Assets: fonts and pixels by content (DESIGN §6.11).
//!
//! An asset is offered before any presentation requires it; the frontend
//! answers `Have` or `Want` for offered ids only, and verifies the SHA-256 of
//! a `Content` asset before it is used or persisted, so a peer can never
//! poison a cache other sessions share.

use std::collections::BTreeMap;
use std::num::NonZeroU64;

use neomacs_display_protocol::ImageId;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::msg::{ChunkEnd, Lane};
use crate::payload::Payload;

/// A content asset's identity: the SHA-256 of its canonical bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ContentId(pub [u8; 32]);

impl ContentId {
    #[must_use]
    pub fn of(bytes: &[u8]) -> Self {
        Self(Sha256::digest(bytes).into())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum AssetId {
    /// A font file or a decoded pixel buffer with its format header.
    Content(ContentId),
    /// Shader-surface pixels and other per-session buffers.
    Session(NonZeroU64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FontContainer {
    Sfnt,
    Collection,
    Pcf,
    Bdf,
    Woff,
    Woff2,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PixelFormat {
    Rgba8,
    Gray8,
    Mask1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AssetKind {
    FontFile {
        container: FontContainer,
        faces: u32,
    },
    ImagePixels {
        format: PixelFormat,
        width: u32,
        height: u32,
        stride: u32,
    },
    SurfacePixels {
        width: u32,
        height: u32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WantPriority {
    /// Fonts of visible glyphs of the newest presentation.
    Visible,
    Coverage,
    Background,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum AssetDown {
    Offer {
        id: AssetId,
        kind: AssetKind,
        len: u64,
    },
    Forget {
        id: AssetId,
    },
    ImageRealize {
        image: ImageId,
        pixels: AssetId,
        width: u32,
        height: u32,
    },
    ImageDrop {
        image: ImageId,
    },
    Put {
        id: AssetId,
        offset: u64,
        bytes: Payload<[u8]>,
        end: ChunkEnd,
    },
}

impl AssetDown {
    #[must_use]
    pub const fn lane(&self) -> Lane {
        match self {
            Self::Put { .. } => Lane::Bulk,
            Self::Offer { .. }
            | Self::Forget { .. }
            | Self::ImageRealize { .. }
            | Self::ImageDrop { .. } => Lane::Control,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetUp {
    Have {
        ids: Vec<AssetId>,
    },
    Want {
        ids: Vec<AssetId>,
        priority: WantPriority,
    },
    Stored {
        id: AssetId,
    },
    Evicted {
        id: AssetId,
    },
    ResidencyLost {
        image: ImageId,
    },
}

/// A violation of the asset rules; the session closes with
/// `Goodbye{ProtocolViolation}`.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum AssetError {
    #[error("asset {0:?} was not offered")]
    NotOffered(AssetId),
    #[error("asset of {len} bytes exceeds the {limit}-byte asset limit")]
    TooLarge { len: u64, limit: u64 },
    #[error("asset bytes arrived at offset {got}, expected {expected}")]
    OutOfOrder { expected: u64, got: u64 },
    #[error("asset bytes exceed the offered {len} bytes")]
    Overrun { len: u64 },
    #[error("asset ended after {got} of the offered {len} bytes")]
    Short { len: u64, got: u64 },
    #[error("asset bytes do not hash to their content id")]
    DigestMismatch,
}

/// The evaluator's offer state: only offered ids may be wanted.
#[derive(Debug, Default)]
pub struct AssetOffers {
    offered: BTreeMap<AssetId, (AssetKind, u64, OfferState)>,
}

/// Where one offered asset stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OfferState {
    Offered,
    Held,
    Wanted(WantPriority),
    Stored,
}

impl AssetOffers {
    /// The `Offer` to send, or `None` when the id is already offered.
    pub fn offer(&mut self, id: AssetId, kind: AssetKind, len: u64) -> Option<AssetDown> {
        if self.offered.contains_key(&id) {
            return None;
        }
        self.offered.insert(id, (kind, len, OfferState::Offered));
        Some(AssetDown::Offer { id, kind, len })
    }

    pub fn state(&self, id: AssetId) -> Option<OfferState> {
        self.offered.get(&id).map(|(_, _, state)| *state)
    }

    /// Apply the frontend's answer.
    pub fn on_uplink(&mut self, message: &AssetUp) -> Result<(), AssetError> {
        let mut set = |id: AssetId, state: OfferState| {
            self.offered
                .get_mut(&id)
                .map(|entry| entry.2 = state)
                .ok_or(AssetError::NotOffered(id))
        };
        match message {
            AssetUp::Have { ids } => ids.iter().try_for_each(|id| set(*id, OfferState::Held)),
            AssetUp::Want { ids, priority } => ids
                .iter()
                .try_for_each(|id| set(*id, OfferState::Wanted(*priority))),
            AssetUp::Stored { id } => set(*id, OfferState::Stored),
            AssetUp::Evicted { id } => {
                // Re-offered on next reference.
                self.offered
                    .remove(id)
                    .map(drop)
                    .ok_or(AssetError::NotOffered(*id))
            }
            AssetUp::ResidencyLost { .. } => Ok(()),
        }
    }
}

/// An asset whose bytes arrived whole and, for content ids, hashed to their
/// id.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedAsset {
    pub id: AssetId,
    pub kind: AssetKind,
    pub bytes: Vec<u8>,
}

/// The frontend's reassembly of offered assets.
#[derive(Debug)]
pub struct AssetReceiver {
    limit: u64,
    offered: BTreeMap<AssetId, (AssetKind, u64, Vec<u8>)>,
}

impl AssetReceiver {
    #[must_use]
    pub fn new(limit: u64) -> Self {
        Self {
            limit,
            offered: BTreeMap::new(),
        }
    }

    pub fn on_offer(&mut self, id: AssetId, kind: AssetKind, len: u64) -> Result<(), AssetError> {
        if len > self.limit {
            return Err(AssetError::TooLarge {
                len,
                limit: self.limit,
            });
        }
        self.offered.insert(id, (kind, len, Vec::new()));
        Ok(())
    }

    /// Append one chunk; the last one yields the verified asset.
    pub fn on_put(
        &mut self,
        id: AssetId,
        offset: u64,
        bytes: &[u8],
        end: ChunkEnd,
    ) -> Result<Option<VerifiedAsset>, AssetError> {
        let (_, len, assembled) = self
            .offered
            .get_mut(&id)
            .ok_or(AssetError::NotOffered(id))?;
        if offset != assembled.len() as u64 {
            return Err(AssetError::OutOfOrder {
                expected: assembled.len() as u64,
                got: offset,
            });
        }
        if assembled.len() as u64 + bytes.len() as u64 > *len {
            return Err(AssetError::Overrun { len: *len });
        }
        assembled.extend_from_slice(bytes);
        if end == ChunkEnd::More {
            return Ok(None);
        }
        let (kind, len, assembled) = self.offered.remove(&id).expect("found above");
        if assembled.len() as u64 != len {
            return Err(AssetError::Short {
                len,
                got: assembled.len() as u64,
            });
        }
        if let AssetId::Content(content) = id
            && ContentId::of(&assembled) != content
        {
            return Err(AssetError::DigestMismatch);
        }
        Ok(Some(VerifiedAsset {
            id,
            kind,
            bytes: assembled,
        }))
    }
}
