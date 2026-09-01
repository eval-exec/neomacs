//! Windows native font catalog adapter.
//!
//! DirectWrite owns installed-font visibility and range fallback. Shared GNU
//! fontset ordering and style scoring remain in the parent module.

mod catalog;

use super::native_asset_cache::NativeFontAssetCache;
use super::{
    FontBackend, FontCandidate, FontCandidateQuery, FontCandidateScope, FontFamilyName,
    PlatformFontCandidate, PlatformFontCandidateLocator, PlatformFontDesignMetrics,
    PlatformFontMatch, PlatformFontMetadata, RequiredFontCoverage, TextDirection,
};
use dwrote::{
    Font, FontCollection, FontFaceType, FontFallback, FontFamily, FontStretch, FontStyle,
    FontWeight, InformationalStringId, TextAnalysisSource, TextAnalysisSourceMethods,
};
use neomacs_display_protocol::font::{FontBackendKind, FontVariationCoord, ResolvedFontIdentity};
use neovm_core::face::{FontSlant, FontWidth};
use std::borrow::Cow;
use std::path::PathBuf;
use std::ptr;
use winapi::shared::winerror::S_OK;
use winapi::um::dwrite::IDWriteLocalizedStrings;
use wio::com::ComPtr;

/// DirectWrite adapter for native Windows matching and system fallback.
#[derive(Debug, Default)]
pub struct DirectWriteBackend {
    catalog: catalog::DirectWriteCatalogMonitor,
    native_assets: NativeFontAssetCache,
}

impl FontBackend for DirectWriteBackend {
    fn kind(&self) -> FontBackendKind {
        FontBackendKind::DirectWrite
    }

    fn list_families(&self) -> Vec<FontFamilyName> {
        let collection = FontCollection::system();
        collection
            .families_iter()
            .flat_map(localized_family_names)
            .filter_map(FontFamilyName::new)
            .filter(|name| {
                collection
                    .font_family_by_name(name.as_str())
                    .is_ok_and(|family| family.is_some())
            })
            .collect()
    }

    fn resolve_family(&self, family: &str) -> String {
        resolve_generic_family(family).unwrap_or_else(|| family.to_string())
    }

    fn family_prefers_monospace(&self, family: &str) -> bool {
        FontCollection::system()
            .font_family_by_name(&self.resolve_family(family))
            .ok()
            .flatten()
            .and_then(|family| family.font(0).ok())
            .and_then(|font| font.is_monospace())
            .unwrap_or_default()
    }

    fn list_candidates(&self, query: &FontCandidateQuery) -> Vec<FontCandidate> {
        match &query.scope {
            FontCandidateScope::Family(family) => {
                let Some(family) = FontCollection::system()
                    .font_family_by_name(family.as_str())
                    .ok()
                    .flatten()
                else {
                    return Vec::new();
                };
                (0..family.get_font_count())
                    .filter_map(|index| family.font(index).ok())
                    .filter(|font| font_supports_required(font, &query.required))
                    .filter_map(font_candidate_from_font)
                    .collect()
            }
            FontCandidateScope::All => FontCollection::system()
                .families_iter()
                .flat_map(|family| {
                    (0..family.get_font_count()).filter_map(move |index| family.font(index).ok())
                })
                .filter(|font| font_supports_required(font, &query.required))
                .filter_map(font_candidate_from_font)
                .collect(),
            FontCandidateScope::NativeFallback { .. } => {
                native_fallback_candidate(query).into_iter().collect()
            }
        }
    }

    fn finalize_match(&self, matched: PlatformFontCandidate) -> Option<PlatformFontMatch> {
        if matches!(&matched.locator, PlatformFontCandidateLocator::File(_)) {
            return matched.into_file_match();
        }
        let font = find_font(matched.family(), &matched.identity)?;
        let face = font.create_font_face();
        let files = face.files().ok()?;
        let [file] = files.as_slice() else {
            return None;
        };
        let face_index = face.get_index();
        if face_index != matched.identity.file_face_index() {
            return None;
        }
        let asset = self
            .native_assets
            .get_or_materialize(&matched.identity, || file.font_file_bytes().ok())?;
        matched.into_memory_match(asset)
    }

    fn design_metrics(&self, matched: &PlatformFontMatch) -> Option<PlatformFontDesignMetrics> {
        find_font(matched.family(), &matched.identity)
            .and_then(|font| font_design_metrics(&font.create_font_face()))
    }

    fn advance_catalog_generation(&mut self) {
        self.native_assets.clear();
    }

    fn poll_catalog_change(&mut self) -> crate::font::catalog::FontCatalogChange {
        self.catalog.poll()
    }
}

fn font_supports_required(font: &Font, required: &RequiredFontCoverage) -> bool {
    let codepoints: Vec<u32> = required.chars().map(u32::from).collect();
    if codepoints.is_empty() {
        return true;
    }
    font.create_font_face()
        .glyph_indices(&codepoints)
        .is_ok_and(|glyphs| glyphs.iter().all(|glyph| *glyph != 0))
}

fn resolve_generic_family(family: &str) -> Option<String> {
    let candidates: &[&str] = match family.trim().to_ascii_lowercase().as_str() {
        "fixed" | "mono" | "monospace" => &[
            "Cascadia Mono",
            "Cascadia Code",
            "Consolas",
            "Lucida Console",
            "Courier New",
        ],
        "sans" | "sans-serif" | "sans serif" => &["Segoe UI", "Arial"],
        "serif" => &["Times New Roman", "Georgia"],
        _ => return None,
    };
    let collection = FontCollection::system();
    candidates.iter().find_map(|candidate| {
        collection
            .font_family_by_name(candidate)
            .ok()
            .flatten()
            .map(|_| (*candidate).to_string())
    })
}

fn to_directwrite_weight(weight: u16) -> FontWeight {
    FontWeight::from_u32(u32::from(weight.clamp(1, 999)))
}

fn native_fallback_candidate(query: &FontCandidateQuery) -> Option<FontCandidate> {
    let FontCandidateScope::NativeFallback { base_family } = &query.scope else {
        return None;
    };
    let text: String = query.required.chars().collect();
    if text.is_empty() {
        return None;
    }
    let fallback = FontFallback::get_system_fallback()?;
    let utf16: Vec<u16> = text.encode_utf16().collect();
    let text_len = u32::try_from(utf16.len()).ok()?;
    let locale = query
        .languages
        .first()
        .cloned()
        .or_else(user_default_locale)
        .unwrap_or_else(|| "und".to_string());
    let analysis = TextAnalysisSource::from_text(
        Box::new(SingleLocaleAnalysis {
            text_len,
            locale,
            direction: query.direction,
        }),
        Cow::Owned(utf16),
    );
    let collection = FontCollection::system();
    fallback
        .map_characters(
            &analysis,
            0,
            text_len,
            &collection,
            Some(base_family.as_str()),
            to_directwrite_weight(query.requested_weight),
            if query.requested_slant.is_italic() {
                FontStyle::Italic
            } else {
                FontStyle::Normal
            },
            to_directwrite_stretch(query.requested_width),
        )
        .mapped_font
        .and_then(font_candidate_from_font)
}

struct LocalizedStrings(ComPtr<IDWriteLocalizedStrings>);

impl LocalizedStrings {
    fn from_family(family: &FontFamily) -> Option<Self> {
        let mut strings = ptr::null_mut();
        // SAFETY: `family` owns this interface for the duration of the call.
        // On success DirectWrite returns a new, non-null COM reference through
        // `strings`; `ComPtr` takes ownership of that reference below.
        let status = unsafe { (*family.as_ptr()).GetFamilyNames(&mut strings) };
        if status != S_OK || strings.is_null() {
            return None;
        }
        // SAFETY: the successful call above returned an owned, non-null
        // `IDWriteLocalizedStrings` reference. `ComPtr` releases it on drop.
        Some(Self(unsafe { ComPtr::from_raw(strings) }))
    }

    fn names(&self) -> Vec<String> {
        // SAFETY: `self.0` owns a live DirectWrite localized-strings interface.
        let count = unsafe { self.0.GetCount() };
        (0..count).filter_map(|index| self.name(index)).collect()
    }

    fn name(&self, index: u32) -> Option<String> {
        let mut length = 0;
        // SAFETY: `self.0` is live and DirectWrite validates `index` against
        // the collection. The out-parameter points to initialized storage.
        if unsafe { self.0.GetStringLength(index, &mut length) } != S_OK {
            return None;
        }
        let buffer_len = length.checked_add(1)?;
        let mut buffer = vec![0; usize::try_from(buffer_len).ok()?];
        // SAFETY: the buffer has `buffer_len` UTF-16 code units, including the
        // terminator requested by DirectWrite, and remains live for the call.
        if unsafe { self.0.GetString(index, buffer.as_mut_ptr(), buffer_len) } != S_OK {
            return None;
        }
        String::from_utf16(&buffer[..usize::try_from(length).ok()?]).ok()
    }
}

fn localized_family_names(family: FontFamily) -> Vec<String> {
    LocalizedStrings::from_family(&family)
        .map(|strings| strings.names())
        .unwrap_or_default()
}

fn font_candidate_from_font(font: Font) -> Option<FontCandidate> {
    let face = font.create_font_face();
    let files = face.files().ok()?;
    let [file] = files.as_slice() else {
        return None;
    };
    let face_type = face.get_type();
    let Some(replay) = DirectWriteReplaySource::classify(file.font_file_path().ok(), face_type)
    else {
        tracing::warn!(
            target: "font_boundary",
            family = %font.family_name(),
            ?face_type,
            "URL-less DirectWrite stream format cannot be replayed by the shared font materializer"
        );
        return None;
    };
    let variations = font_variations(&face)?;
    let postscript_name = font.informational_string(InformationalStringId::PostscriptName);
    let spacing = if font.is_monospace().unwrap_or_default() {
        100
    } else {
        0
    };
    let width = from_directwrite_stretch(font.stretch());
    let metadata = PlatformFontMetadata {
        foundry: None,
        family: font.family_name(),
        weight: Some(font.weight().to_u32().clamp(1, u32::from(u16::MAX)) as u16),
        slant: match font.style() {
            FontStyle::Italic => FontSlant::Italic,
            FontStyle::Oblique => FontSlant::Oblique,
            FontStyle::Normal => FontSlant::Normal,
        },
        width: Some(width),
        spacing: Some(spacing),
        design_metrics: None,
        size: replay.size(),
    };
    let matched = match replay {
        DirectWriteReplaySource::File(path) => PlatformFontCandidate::from_platform_file(
            FontBackendKind::DirectWrite,
            &path,
            face.get_index(),
            postscript_name,
            variations,
            metadata,
        )?,
        DirectWriteReplaySource::NativeOutline => PlatformFontCandidate {
            identity: directwrite_native_identity(
                &font,
                face.get_index(),
                postscript_name,
                variations,
            ),
            locator: PlatformFontCandidateLocator::Native,
            metadata,
        },
    };
    Some(FontCandidate { matched })
}

#[derive(Debug, Eq, PartialEq)]
enum DirectWriteReplaySource {
    File(PathBuf),
    NativeOutline,
}

impl DirectWriteReplaySource {
    fn classify(file_path: Option<PathBuf>, face_type: FontFaceType) -> Option<Self> {
        if let Some(path) = file_path {
            return Some(Self::File(path));
        }
        match face_type {
            FontFaceType::Cff | FontFaceType::TrueType | FontFaceType::TrueTypeCollection => {
                Some(Self::NativeOutline)
            }
            FontFaceType::Unknown
            | FontFaceType::RawCff
            | FontFaceType::Type1
            | FontFaceType::Vector
            | FontFaceType::Bitmap => None,
        }
    }

    fn size(&self) -> super::PlatformFontSize {
        match self {
            // The shared materializer opens the exact file after discovery so
            // it can distinguish a scalable outline from a fixed strike.
            Self::File(_) => super::PlatformFontSize::Unknown,
            // A native locator is constructed only for replayable outline
            // formats, so it needs no file-based capability probe.
            Self::NativeOutline => super::PlatformFontSize::Scalable,
        }
    }
}

fn find_font(family_name: &str, identity: &ResolvedFontIdentity) -> Option<Font> {
    let family = FontCollection::system()
        .font_family_by_name(family_name)
        .ok()
        .flatten()?;
    (0..family.get_font_count())
        .filter_map(|index| family.font(index).ok())
        .find(|font| font_matches(font, identity))
}

fn font_matches(font: &Font, identity: &ResolvedFontIdentity) -> bool {
    if identity.file_path.is_none() {
        return native_identity_from_font(font).as_ref() == Some(identity);
    }
    let face = font.create_font_face();
    if face.get_index() != identity.file_face_index() {
        return false;
    }
    if let Some(expected_name) = identity.postscript_name.as_deref()
        && font
            .informational_string(InformationalStringId::PostscriptName)
            .as_deref()
            != Some(expected_name)
    {
        return false;
    }
    let Some(variations) = font_variations(&face) else {
        return false;
    };
    if variations.as_slice() != identity.variation_coords.as_slice() {
        return false;
    }
    let Some(expected_path) = identity.file_path.as_deref() else {
        return false;
    };
    let Ok(files) = face.files() else {
        return false;
    };
    files.len() == 1
        && files[0]
            .font_file_path()
            .is_ok_and(|path| path.as_os_str() == std::ffi::OsStr::new(expected_path))
}

fn native_identity_from_font(font: &Font) -> Option<ResolvedFontIdentity> {
    let face = font.create_font_face();
    Some(directwrite_native_identity(
        font,
        face.get_index(),
        font.informational_string(InformationalStringId::PostscriptName),
        font_variations(&face)?,
    ))
}

fn directwrite_native_identity(
    font: &Font,
    face_index: u32,
    postscript_name: Option<String>,
    variations: Vec<FontVariationCoord>,
) -> ResolvedFontIdentity {
    let stable_key = match postscript_name.as_deref() {
        Some(name) => format!("directwrite:postscript:{}:{name}#{face_index}", name.len()),
        None => {
            let family = font.family_name();
            format!(
                "directwrite:family:{}:{family}#{face_index}:{}:{}:{}",
                family.len(),
                font.weight().to_u32(),
                font_style_key(font.style()),
                from_directwrite_stretch(font.stretch()).gnu_numeric(),
            )
        }
    };
    ResolvedFontIdentity::from_native_with_variations(
        FontBackendKind::DirectWrite,
        stable_key,
        face_index,
        postscript_name,
        variations,
    )
}

fn font_variations(face: &dwrote::FontFace) -> Option<Vec<FontVariationCoord>> {
    let mut variations = face
        .variations()
        .ok()?
        .into_iter()
        .filter_map(|axis| FontVariationCoord::try_new(axis.axisTag.swap_bytes(), axis.value))
        .collect::<Vec<_>>();
    variations.sort_unstable_by_key(|coord| (coord.tag(), coord.value_bits()));
    Some(variations)
}

fn font_style_key(style: FontStyle) -> &'static str {
    match style {
        FontStyle::Normal => "normal",
        FontStyle::Oblique => "oblique",
        FontStyle::Italic => "italic",
    }
}

fn font_design_metrics(face: &dwrote::FontFace) -> Option<PlatformFontDesignMetrics> {
    let metrics = face.metrics().metrics0();
    let codepoints: Vec<u32> = (32..=126).collect();
    let glyphs = face.glyph_indices(&codepoints).ok()?;
    let advances: Vec<i32> = face
        .design_glyph_metrics(&glyphs, false)
        .ok()?
        .into_iter()
        .map(|metrics| metrics.advanceWidth as i32)
        .filter(|advance| *advance > 0)
        .collect();
    let max_advance = advances.iter().copied().max().unwrap_or(0);
    let average_advance = if advances.is_empty() {
        0
    } else {
        advances.iter().sum::<i32>() / advances.len() as i32
    };
    Some(PlatformFontDesignMetrics {
        units_per_em: u32::from(metrics.designUnitsPerEm),
        ascent: i32::from(metrics.ascent),
        descent: i32::from(metrics.descent),
        line_gap: i32::from(metrics.lineGap),
        max_advance,
        space_advance: advances.first().copied().unwrap_or(0),
        average_advance,
    })
}

fn to_directwrite_stretch(width: FontWidth) -> FontStretch {
    match width {
        FontWidth::UltraCondensed => FontStretch::UltraCondensed,
        FontWidth::ExtraCondensed => FontStretch::ExtraCondensed,
        FontWidth::Condensed => FontStretch::Condensed,
        FontWidth::SemiCondensed => FontStretch::SemiCondensed,
        FontWidth::Normal => FontStretch::Normal,
        FontWidth::SemiExpanded => FontStretch::SemiExpanded,
        FontWidth::Expanded => FontStretch::Expanded,
        FontWidth::ExtraExpanded => FontStretch::ExtraExpanded,
        FontWidth::UltraExpanded => FontStretch::UltraExpanded,
    }
}

fn from_directwrite_stretch(width: FontStretch) -> FontWidth {
    match width {
        FontStretch::Undefined | FontStretch::Normal => FontWidth::Normal,
        FontStretch::UltraCondensed => FontWidth::UltraCondensed,
        FontStretch::ExtraCondensed => FontWidth::ExtraCondensed,
        FontStretch::Condensed => FontWidth::Condensed,
        FontStretch::SemiCondensed => FontWidth::SemiCondensed,
        FontStretch::SemiExpanded => FontWidth::SemiExpanded,
        FontStretch::Expanded => FontWidth::Expanded,
        FontStretch::ExtraExpanded => FontWidth::ExtraExpanded,
        FontStretch::UltraExpanded => FontWidth::UltraExpanded,
    }
}

fn user_default_locale() -> Option<String> {
    sys_locale::get_locale().filter(|locale| !locale.is_empty())
}

struct SingleLocaleAnalysis {
    text_len: u32,
    locale: String,
    direction: TextDirection,
}

impl TextAnalysisSourceMethods for SingleLocaleAnalysis {
    fn get_locale_name(&self, text_position: u32) -> (Cow<'_, str>, u32) {
        (
            Cow::Borrowed(&self.locale),
            self.text_len.saturating_sub(text_position),
        )
    }

    fn get_paragraph_reading_direction(&self) -> winapi::um::dwrite::DWRITE_READING_DIRECTION {
        match self.direction {
            TextDirection::LeftToRight => {
                winapi::um::dwrite::DWRITE_READING_DIRECTION_LEFT_TO_RIGHT
            }
            TextDirection::RightToLeft => {
                winapi::um::dwrite::DWRITE_READING_DIRECTION_RIGHT_TO_LEFT
            }
        }
    }
}

#[cfg(test)]
#[path = "windows_test.rs"]
mod tests;
