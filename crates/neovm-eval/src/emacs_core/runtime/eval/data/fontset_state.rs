//! Plain fontset registry snapshot data (stage-4 K2 display).
//!
//! These records are pure data over symbol ids, Lisp strings and font
//! attributes.  They live below the evaluator so pdump can name the snapshot
//! without importing the display fontset module; the registry itself and the
//! snapshot/restore bodies stay display-side and are published through the
//! eval lifecycle FontsetHooks group.

use neovm_value::display_vocab::{FontSlant, FontWeight, FontWidth};
use neovm_value::heap_types::LispString;
use neovm_value::intern::SymId;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredFontSpec {
    pub family: Option<SymId>,
    pub registry: Option<SymId>,
    pub lang: Option<SymId>,
    pub weight: Option<FontWeight>,
    pub slant: Option<FontSlant>,
    pub width: Option<FontWidth>,
    /// GNU `:otf` requirement (`src/font.c` `font_prop_validate_otf` shape):
    /// `(SCRIPT [LANGSYS [GSUB-FEATURES [GPOS-FEATURES]]])`. A font must
    /// advertise the named OpenType script/lang-system features or it does not
    /// match the spec, exactly as GNU's `font_check_otf` decides.
    pub otf: Option<StoredOtfSpec>,
    pub definition: Option<FontDefinitionMetadata>,
}

/// The OpenType script/lang-system/feature requirement of a `:otf` spec.
///
/// Tags are owned text because the layout engine compares them against the
/// capability it reads from the font file, outside the evaluator.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct StoredOtfSpec {
    pub script: String,
    pub langsys: StoredOtfLangsys,
    pub gsub: Option<StoredOtfFeatures>,
    pub gpos: Option<StoredOtfFeatures>,
}

/// The second element of a `:otf` spec.
///
/// GNU's `font_prop_validate_otf` re-tests `XCAR (val)` for the second slot
/// (`src/font.c:678`), so a non-symbol LANGSYS is accepted by `font-spec`.
/// `font_check_otf_features` then compares it with `eq` against langsys tag
/// symbols, which can never match, so every feature requirement rejects all
/// fonts. `Unmatchable` preserves that observable outcome.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub enum StoredOtfLangsys {
    /// nil: prefer the default langsys, else the script's first langsys.
    #[default]
    Default,
    Named(String),
    Unmatchable,
}

/// One `:otf` feature list: `(FEATURE ... [nil FEATURE ...])`.
///
/// Features before the `nil` separator must be present; features after it must
/// be absent (`src/font.c` `font_check_otf_features`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct StoredOtfFeatures {
    pub required: Vec<String>,
    pub forbidden: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FontSpecEntry {
    Font(StoredFontSpec),
    ExplicitNone,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FontRepertory {
    Charset(SymId),
    CharTableRanges(Vec<(u32, u32)>),
}

/// GNU's font definition records encoding and optional repertory separately.
/// Encoding determines informational pattern queries; neither field proves
/// that an opened font contains a glyph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontDefinitionMetadata {
    pub encoding: SymId,
    pub repertory: Option<FontRepertory>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub struct FontsetRangeEntrySnapshot {
    pub from: u32,
    pub to: u32,
    pub entries: Vec<FontSpecEntry>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[doc(hidden)]
pub struct FontsetDataSnapshot {
    pub ranges: Vec<FontsetRangeEntrySnapshot>,
    pub fallback: Option<Vec<FontSpecEntry>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub struct FontsetRegistrySnapshot {
    pub ordered_names: Vec<LispString>,
    pub alias_to_name: Vec<(LispString, LispString)>,
    pub fontsets: Vec<(LispString, FontsetDataSnapshot)>,
    pub generation: u64,
}

static_assertions::assert_not_impl_any!(FontsetRegistrySnapshot: Send, Sync);
