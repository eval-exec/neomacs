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
    pub definition: Option<FontDefinitionMetadata>,
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
