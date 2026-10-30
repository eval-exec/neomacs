//! Immutable rendering authority retained by detached native font workers.
//!
//! The fontset owner resolves symbol spellings at mutation/restore. Capturing a
//! character clones two shared slices; it neither copies nor limits the chain.

use super::{DEFAULT_FONTSET_NAME, FontSpecEntry, FontsetData, fontset_name_spelling, registry};
use crate::emacs_core::intern::{SymId, resolve_sym_lisp_string};
use crate::face::{FontSlant, FontWeight, FontWidth};
use std::borrow::Cow;
use std::num::NonZeroU64;
use std::ops::Deref;
use std::sync::Arc;

/// Native spelling of a Lisp font name, without exposing storage sentinels.
///
/// Exact Lisp bytes remain in the informational fontset owner. Native APIs
/// accept Unicode text, so non-Unicode Emacs characters use the canonical
/// lossy conversion at this one boundary. Ordinary UTF-8 names are borrowed.
pub fn native_font_name(symbol: SymId) -> Cow<'static, str> {
    let name = resolve_sym_lisp_string(symbol);
    match name.as_utf8_str() {
        Some(text) => Cow::Borrowed(text),
        None => Cow::Owned(crate::emacs_core::emacs_char::to_utf8_lossy(
            name.as_bytes(),
        )),
    }
}

/// Revision of the complete font-selection policy, including family aliases.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FontSelectionRevision(NonZeroU64);

impl FontSelectionRevision {
    /// Legacy query-cache keys store this authority as its numeric generation.
    pub fn get(self) -> u64 {
        self.0.get()
    }
}

impl TryFrom<u64> for FontSelectionRevision {
    type Error = FontsetCaptureError;

    fn try_from(generation: u64) -> Result<Self, Self::Error> {
        NonZeroU64::new(generation)
            .map(Self)
            .ok_or(FontsetCaptureError::InvalidRevision)
    }
}

/// Acquisition failure is independent of the number of font alternatives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum FontsetCaptureError {
    #[error("fontset selection authority is unavailable")]
    Unavailable,
    #[error("the default fontset is missing")]
    MissingDefault,
    #[error("fontset selection authority has no initialized revision")]
    InvalidRevision,
}

/// Rendering fields only: encoding/repertory remain informational owner data.
/// Spellings are owned before detachment; workers never resolve Lisp symbols.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontsetRenderingSpec {
    pub family: Option<Arc<str>>,
    pub registry: Option<Arc<str>>,
    pub lang: Option<Arc<str>>,
    pub weight: Option<FontWeight>,
    pub slant: Option<FontSlant>,
    pub width: Option<FontWidth>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FontsetRenderingEntry {
    Font(FontsetRenderingSpec),
    ExplicitNone,
}

/// The original and detached views can only be constructed together.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct StoredFontsetEntry {
    original: FontSpecEntry,
    rendering: FontsetRenderingEntry,
}

impl From<FontSpecEntry> for StoredFontsetEntry {
    fn from(original: FontSpecEntry) -> Self {
        let rendering = match &original {
            FontSpecEntry::Font(spec) => {
                let spec = spec.for_rendering();
                FontsetRenderingEntry::Font(FontsetRenderingSpec {
                    family: spec
                        .family
                        .map(|name| Arc::from(native_font_name(name).as_ref())),
                    registry: spec
                        .registry
                        .map(|name| Arc::from(native_font_name(name).as_ref())),
                    lang: spec
                        .lang
                        .map(|name| Arc::from(native_font_name(name).as_ref())),
                    weight: spec.weight,
                    slant: spec.slant,
                    width: spec.width,
                })
            }
            FontSpecEntry::ExplicitNone => FontsetRenderingEntry::ExplicitNone,
        };
        Self {
            original,
            rendering,
        }
    }
}

impl StoredFontsetEntry {
    pub(super) fn original(&self) -> &FontSpecEntry {
        &self.original
    }

    fn rendering(&self) -> &FontsetRenderingEntry {
        &self.rendering
    }
}

/// Immutable storage: split ranges and worker captures share an owner's chain.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct SharedFontsetEntries(Arc<[StoredFontsetEntry]>);

impl SharedFontsetEntries {
    pub(super) fn from_stored_entries(entries: Vec<StoredFontsetEntry>) -> Self {
        Self(entries.into())
    }

    pub(super) fn as_slice(&self) -> &[StoredFontsetEntry] {
        &self.0
    }

    pub(super) fn to_original_entries(&self) -> Vec<FontSpecEntry> {
        self.0.iter().map(|entry| entry.original.clone()).collect()
    }
}

impl From<Vec<FontSpecEntry>> for SharedFontsetEntries {
    fn from(entries: Vec<FontSpecEntry>) -> Self {
        Self::from_stored_entries(entries.into_iter().map(StoredFontsetEntry::from).collect())
    }
}

impl Deref for SharedFontsetEntries {
    type Target = [StoredFontsetEntry];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

/// A character's ordered alternatives and the revision that owns their data.
/// No live registry or evaluator is consulted by this object's iterator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CapturedFontsetChain {
    revision: FontSelectionRevision,
    specific: SharedFontsetEntries,
    fallback: SharedFontsetEntries,
}

impl CapturedFontsetChain {
    fn from_data(revision: FontSelectionRevision, data: &FontsetData, ch: char) -> Self {
        Self {
            revision,
            specific: data
                .find_range(ch as u32)
                .map(|range| range.entries.clone())
                .unwrap_or_default(),
            fallback: data.fallback.clone().unwrap_or_default(),
        }
    }

    pub fn revision(&self) -> FontSelectionRevision {
        self.revision
    }

    /// The terminal ExplicitNone is deliberately visible. Consumers distinguish
    /// that stop from exhaustion before deciding to try generic native fallback.
    pub fn iter(&self) -> impl Iterator<Item = &FontsetRenderingEntry> {
        self.specific
            .iter()
            .chain(self.fallback.iter())
            .map(StoredFontsetEntry::rendering)
    }
}

static_assertions::assert_impl_all!(CapturedFontsetChain: Send, Sync, Clone, std::fmt::Debug);
static_assertions::assert_impl_all!(FontsetRenderingEntry: Send, Sync, Clone, std::fmt::Debug);

/// Acquire the default fontset's character-specific and fallback chain by
/// reference. A long configured chain is valid policy, never Unsupported.
pub fn capture_default_fontset_chain(
    ch: char,
) -> Result<CapturedFontsetChain, FontsetCaptureError> {
    let slot = registry()
        .read()
        .map_err(|_| FontsetCaptureError::Unavailable)?;
    let revision = FontSelectionRevision::try_from(slot.generation)?;
    let data = slot
        .fontsets
        .get(&fontset_name_spelling(DEFAULT_FONTSET_NAME))
        .ok_or(FontsetCaptureError::MissingDefault)?;
    Ok(CapturedFontsetChain::from_data(revision, data, ch))
}

#[cfg(test)]
#[path = "tests/native_policy_test.rs"]
mod tests;
