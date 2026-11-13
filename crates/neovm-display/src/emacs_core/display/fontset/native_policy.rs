//! Immutable rendering authority retained by detached native font workers.
//!
//! The fontset owner resolves symbol spellings at mutation/restore. Capturing a
//! character clones two shared slices; it neither copies nor limits the chain.

use super::{DEFAULT_FONTSET_NAME, FontSpecEntry, FontsetData, fontset_name_spelling, registry};
use crate::emacs_core::fontset::StoredFontSpecExt;
use crate::face::{FontSlant, FontWeight, FontWidth};
use neovm_value::intern::{SymId, resolve_sym_lisp_string};
use std::borrow::Cow;
use std::num::NonZeroU64;
use std::ops::Deref;
use std::sync::{Arc, Mutex, PoisonError, RwLock};

/// Native spelling of a Lisp font name, without exposing storage sentinels.
///
/// Exact Lisp bytes remain in the informational fontset owner. Native APIs
/// accept Unicode text, so non-Unicode Emacs characters use the canonical
/// lossy conversion at this one boundary. Ordinary UTF-8 names are borrowed.
pub fn native_font_name(symbol: SymId) -> Cow<'static, str> {
    let name = resolve_sym_lisp_string(symbol);
    match name.as_utf8_str() {
        Some(text) => Cow::Borrowed(text),
        None => Cow::Owned(neovm_value::emacs_char::to_utf8_lossy(name.as_bytes())),
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
#[derive(Clone, Debug, Default)]
pub(super) struct SharedFontsetEntries(Arc<FontsetChain>);

#[derive(Debug, Default)]
pub(super) struct FontsetChain {
    entries: Box<[StoredFontsetEntry]>,
    /// The chain in charset-priority order for one preference revision, so
    /// every range and capture that shares this chain shares one ordering.
    /// `None` inside means the entries are already in that order.
    ordered: Mutex<Option<(u64, Option<SharedFontsetEntries>)>>,
}

impl PartialEq for SharedFontsetEntries {
    fn eq(&self, other: &Self) -> bool {
        self.0.entries == other.0.entries
    }
}

impl Eq for SharedFontsetEntries {}

/// The preferred charsets by canonical name: GNU's `Vcharset_ordered_list`
/// up to `Vcharset_non_preferred_head`, published by the charset registry
/// whenever its order changes (GNU bumps `charset_ordered_list_tick` there).
/// Fontset encodings are canonicalized when the entry is defined, as GNU
/// stores a charset id, so names compare directly.  Kept outside the fontset
/// registry so a fontset restore cannot drop it, and process-wide like that
/// registry so detached native workers read the same order.
#[derive(Default)]
struct CharsetPreference {
    revision: u64,
    charsets: Vec<SymId>,
}

static CHARSET_PREFERENCE: RwLock<CharsetPreference> = RwLock::new(CharsetPreference {
    revision: 0,
    charsets: Vec::new(),
});

/// Record a new charset preference.  The preference and the font-selection
/// revision change under one fontset-registry write lock, so a capture, which
/// holds the registry read lock, never pairs the new order with the old
/// revision.  Lock order: registry, then preference.
pub(crate) fn publish_charset_preference(preferred: Vec<SymId>) {
    // Both values are always complete, so a poisoned lock still holds usable
    // ones.
    let mut registry = registry().write().unwrap_or_else(PoisonError::into_inner);
    let mut preference = CHARSET_PREFERENCE
        .write()
        .unwrap_or_else(PoisonError::into_inner);
    if preference.charsets == preferred {
        return;
    }
    preference.charsets = preferred;
    preference.revision = preference.revision.wrapping_add(1);
    registry.generation = registry.generation.wrapping_add(1);
}

impl SharedFontsetEntries {
    pub(super) fn from_stored_entries(entries: Vec<StoredFontsetEntry>) -> Self {
        Self(Arc::new(FontsetChain {
            entries: entries.into(),
            ordered: Mutex::new(None),
        }))
    }

    /// The entries in the order GNU's `reorder_font_vector` (src/fontset.c)
    /// tries them: an entry whose encoding charset comes earlier among the
    /// preferred charsets goes first, every non-preferred encoding ranks after
    /// all preferred ones, and ties keep their definition order.  So with the
    /// Japanese language environment a `jisx0208` entry is tried before a
    /// `gb2312` entry listed ahead of it.
    ///
    /// Only GNU's encoding rule is applied.  Its exemptions for a spec that
    /// matches the face's ASCII font or has `:otf`, and its `:lang` rule for
    /// specs without an encoding, are not.  An `ExplicitNone` ends the chain,
    /// so only the entries before it are reordered.  The result is computed
    /// once per preference revision and shared by every holder of this chain.
    pub(super) fn in_charset_order(&self) -> Self {
        let preference = CHARSET_PREFERENCE
            .read()
            .unwrap_or_else(PoisonError::into_inner);
        let mut cache = self
            .0
            .ordered
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some((revision, ordered)) = cache.as_ref()
            && *revision == preference.revision
        {
            return ordered.clone().unwrap_or_else(|| self.clone());
        }
        let ordered = self.sorted_by(&preference.charsets);
        *cache = Some((preference.revision, ordered.clone()));
        ordered.unwrap_or_else(|| self.clone())
    }

    /// A copy sorted by charset rank, or `None` when already in that order.
    fn sorted_by(&self, preferred: &[SymId]) -> Option<Self> {
        let entries = &self.0.entries;
        let end = entries
            .iter()
            .position(|entry| matches!(entry.original(), FontSpecEntry::ExplicitNone))
            .unwrap_or(entries.len());
        let ranks: Vec<usize> = entries[..end]
            .iter()
            .map(|entry| match entry.original() {
                FontSpecEntry::Font(spec) => spec.definition.as_ref().map_or(0, |definition| {
                    preferred
                        .iter()
                        .position(|charset| *charset == definition.encoding)
                        .unwrap_or(preferred.len())
                }),
                FontSpecEntry::ExplicitNone => 0,
            })
            .collect();
        if ranks.is_sorted() {
            return None;
        }
        let mut ordered: Vec<_> = ranks.into_iter().zip(entries[..end].iter()).collect();
        ordered.sort_by_key(|(rank, _)| *rank);
        let mut ordered: Vec<_> = ordered
            .into_iter()
            .map(|(_, entry)| entry.clone())
            .collect();
        ordered.extend_from_slice(&entries[end..]);
        Some(Self::from_stored_entries(ordered))
    }

    pub(super) fn as_slice(&self) -> &[StoredFontsetEntry] {
        &self.0.entries
    }

    pub(super) fn to_original_entries(&self) -> Vec<FontSpecEntry> {
        self.0
            .entries
            .iter()
            .map(|entry| entry.original.clone())
            .collect()
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
            specific: data.specific_in_charset_order(ch as u32),
            fallback: data.fallback_in_charset_order(),
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
