//! Audited CoreText leaf for the macOS catalog.
//!
//! Apple and CoreFoundation objects never leave this module.  The rest of the
//! font pipeline consumes owned Rust data through the safe functions below.
//! The released `objc2` bindings still mark calls unsafe when their generated
//! signatures cannot express array element types or non-null relationships;
//! those contracts are checked here once rather than at every resolver call.

use super::super::{
    FontCandidateQuery, FontCandidateScope, FontSlant, FontWidth, PlatformFontDesignMetrics,
};
use neomacs_display_protocol::font::FontVariationCoord;
use objc2_core_foundation::{
    CFArray, CFCharacterSet, CFDictionary, CFMutableCharacterSet, CFNumber, CFRange, CFRetained,
    CFString, CFURL, CFURLPathStyle, CGSize,
};
use objc2_core_text::{
    CTFont, CTFontCollection, CTFontDescriptor, CTFontManagerCopyAvailableFontFamilyNames,
    CTFontOrientation, CTFontSymbolicTraits, CTFontTableOptions, CTFontUIFontType,
    kCTFontURLAttribute, kCTFontWeightTrait, kCTFontWidthTrait,
};
use std::path::PathBuf;
use std::ptr::{self, NonNull};

#[derive(Clone, Debug)]
pub(super) struct Candidate {
    pub(super) path: Option<PathBuf>,
    pub(super) postscript_name: String,
    pub(super) family: String,
    pub(super) weight: u16,
    pub(super) slant: FontSlant,
    pub(super) width: FontWidth,
    pub(super) spacing: i32,
    pub(super) variation_coords: Vec<FontVariationCoord>,
}

pub(super) fn available_family_names() -> Vec<String> {
    // SAFETY: CoreText promises an array of CFString values. The typed cast is
    // confined here and every value is retained by `CFArray::get`.
    let families = unsafe {
        let values = CTFontManagerCopyAvailableFontFamilyNames();
        CFRetained::cast_unchecked::<CFArray<CFString>>(values)
    };
    (0..families.len())
        .filter_map(|index| families.get(index))
        .map(|family| family.to_string())
        .collect()
}

pub(super) fn resolve_generic_family(family: &str) -> Option<String> {
    let kind = match family.trim().to_ascii_lowercase().as_str() {
        "fixed" | "mono" | "monospace" => CTFontUIFontType::UserFixedPitch,
        "sans" | "sans-serif" | "sans serif" => CTFontUIFontType::User,
        _ => return None,
    };
    // SAFETY: `None` is explicitly accepted for the optional language and the
    // generated wrapper owns the Create result with `CFRetained`.
    let font = unsafe { CTFont::new_ui_font_for_language(kind, 0.0, None) }?;
    Some(unsafe { font.family_name() }.to_string())
}

pub(super) fn family_prefers_monospace(family: &str) -> bool {
    family_fonts(family)
        .into_iter()
        .next()
        .is_some_and(|font| unsafe {
            font.symbolic_traits()
                .contains(CTFontSymbolicTraits::TraitMonoSpace)
        })
}

pub(super) fn candidates(query: &FontCandidateQuery) -> Vec<Candidate> {
    let Some(coverage) = CoverageFilter::new(query) else {
        return Vec::new();
    };
    let fonts = match &query.scope {
        FontCandidateScope::Family(family) => family_fonts(family.as_str()),
        FontCandidateScope::All => all_fonts(),
        FontCandidateScope::NativeFallback { base_family } => {
            fallback_fonts(base_family.as_str(), query)
        }
    };
    fonts
        .into_iter()
        .filter(|font| coverage.accepts(font))
        .filter(|font| font_supports_query_languages(font, query))
        .filter_map(candidate_from_font)
        .collect()
}

pub(super) fn design_metrics(
    postscript_name: &str,
    coords: &[FontVariationCoord],
) -> Option<PlatformFontDesignMetrics> {
    let name = CFString::from_str(postscript_name);
    // SAFETY: null is the documented identity matrix and Create ownership is
    // represented by `CFRetained`.
    let base = unsafe { CTFont::with_name(&name, 0.0, ptr::null()) };
    let font = font_with_variations(base, coords)?;
    font_design_metrics(&font)
}

/// Copy one selected native face into a standalone SFNT owned by safe Rust.
///
/// CoreText returns unboxed table tags in a CFArray, so the two raw calls stay
/// in this audited leaf. The shared materializer owns table normalization,
/// checksums, and SFNT serialization. No CoreText/CF handle crosses threads.
pub(super) fn standalone_font_bytes(
    postscript_name: &str,
    coords: &[FontVariationCoord],
) -> Option<Vec<u8>> {
    let name = CFString::from_str(postscript_name);
    // SAFETY: null is CoreText's documented identity matrix.
    let base = unsafe { CTFont::with_name(&name, 0.0, ptr::null()) };
    let font = font_with_variations(base, coords)?;
    let options = CTFontTableOptions::NoOptions;
    // SAFETY: the retained CTFont is live. CoreText returns an array of
    // unboxed uintptr_t table tags, as documented by CTFontCopyAvailableTables.
    let tags = unsafe { font.available_tables(options) }?;
    let mut tables = Vec::with_capacity(tags.len());
    for index in 0..tags.len() {
        // SAFETY: index is bounded by `tags.len()` and the retained immutable
        // array remains alive. Its values are unboxed CTFontTableTag integers.
        let raw = unsafe { tags.value_at_index(index as isize) } as usize;
        let tag = u32::try_from(raw).ok()?;
        // SAFETY: `tag` came from this font's available-table list and the
        // returned CFData is copied immediately into an owned Vec.
        if let Some(data) = unsafe { font.table(tag, options) } {
            tables.push((tag, data.to_vec()));
        }
    }
    neomacs_font_materializer::FontFileCache::standalone_sfnt_from_tables(tables)
}

fn available_collection() -> CFRetained<CTFontCollection> {
    // SAFETY: no options dictionary is supplied, so there are no untyped
    // generic values for CoreText to interpret.
    unsafe { CTFontCollection::from_available_fonts(None) }
}

fn all_fonts() -> Vec<CFRetained<CTFont>> {
    let collection = available_collection();
    // SAFETY: CoreText documents this result as CTFontDescriptor values.
    let descriptors = unsafe { collection.matching_font_descriptors() }
        .map(|values| unsafe { CFRetained::cast_unchecked::<CFArray<CTFontDescriptor>>(values) });
    descriptors.map_or_else(Vec::new, fonts_from_descriptors)
}

fn family_fonts(family: &str) -> Vec<CFRetained<CTFont>> {
    let collection = available_collection();
    let family = CFString::from_str(family);
    // SAFETY: no options dictionary is supplied and CoreText documents this
    // result as CTFontDescriptor values.
    let descriptors = unsafe { collection.matching_font_descriptors_for_family(&family, None) }
        .map(|values| unsafe { CFRetained::cast_unchecked::<CFArray<CTFontDescriptor>>(values) });
    descriptors.map_or_else(Vec::new, fonts_from_descriptors)
}

fn cascade_fonts(family: &str, languages: &[String]) -> Vec<CFRetained<CTFont>> {
    let name = CFString::from_str(family);
    // SAFETY: null is the documented identity matrix.
    let base = unsafe { CTFont::with_name(&name, 0.0, ptr::null()) };
    let languages: Vec<CFRetained<CFString>> = languages
        .iter()
        .map(|language| CFString::from_str(language))
        .collect();
    let languages = CFArray::from_retained_objects(&languages);
    // SAFETY: the typed array proves every language is a CFString. CoreText
    // documents the returned array as CTFontDescriptor values.
    let descriptors =
        unsafe { base.default_cascade_list_for_languages(Some(languages.as_opaque())) }.map(
            |values| unsafe { CFRetained::cast_unchecked::<CFArray<CTFontDescriptor>>(values) },
        );
    descriptors.map_or_else(Vec::new, fonts_from_descriptors)
}

fn fallback_fonts(family: &str, query: &FontCandidateQuery) -> Vec<CFRetained<CTFont>> {
    let name = CFString::from_str(family);
    // SAFETY: null is the documented identity matrix.
    let base = unsafe { CTFont::with_name(&name, 0.0, ptr::null()) };
    let required: String = query.required.chars().collect();
    if required.is_empty() {
        return cascade_fonts(family, &query.languages);
    }

    let required = CFString::from_str(&required);
    let range = CFRange {
        location: 0,
        length: required.length(),
    };
    let language = query
        .languages
        .first()
        .map(|language| CFString::from_str(language));
    // SAFETY: the range covers the complete live CFString, and the optional
    // language is a retained UTS #35 identifier for the duration of the call.
    let fallback = unsafe { base.for_string_with_language(&required, range, language.as_deref()) };
    vec![fallback]
}

fn font_supports_query_languages(font: &CTFont, query: &FontCandidateQuery) -> bool {
    if query.languages.is_empty() {
        return true;
    }
    // SAFETY: CoreText documents this as an array of retained CFString values.
    let languages =
        unsafe { CFRetained::cast_unchecked::<CFArray<CFString>>(font.supported_languages()) };
    let supported: Vec<String> = (0..languages.len())
        .filter_map(|index| languages.get(index))
        .map(|language| language.to_string())
        .collect();
    query.languages_are_satisfied_by(|requested| {
        supported
            .iter()
            .any(|available| language_tags_are_compatible(requested, available))
    })
}

fn language_tags_are_compatible(first: &str, second: &str) -> bool {
    first
        .split('-')
        .next()
        .zip(second.split('-').next())
        .is_some_and(|(first, second)| first.eq_ignore_ascii_case(second))
}

fn fonts_from_descriptors(
    descriptors: CFRetained<CFArray<CTFontDescriptor>>,
) -> Vec<CFRetained<CTFont>> {
    (0..descriptors.len())
        .filter_map(|index| descriptors.get(index))
        .map(|descriptor| unsafe {
            // SAFETY: descriptor is retained and null selects the identity
            // transform; the wrapper owns the Create result.
            CTFont::with_font_descriptor(&descriptor, 0.0, ptr::null())
        })
        .collect()
}

enum CoverageFilter {
    Any,
    Required(CFRetained<CFCharacterSet>),
}

impl CoverageFilter {
    fn new(query: &FontCandidateQuery) -> Option<Self> {
        if matches!(&query.required, super::super::RequiredFontCoverage::Any)
            && query.charset_ranges.is_empty()
        {
            return Some(Self::Any);
        }
        // SAFETY: `None` requests CoreFoundation's default allocator.
        let required = unsafe { CFMutableCharacterSet::new(None) }?;
        for &(first, second) in &query.charset_ranges {
            add_scalar_range(&required, first, second)?;
        }
        for ch in query.required.chars() {
            add_scalar_range(&required, u32::from(ch), u32::from(ch))?;
        }
        // SAFETY: copying a valid retained mutable set produces an immutable
        // set with independent Create ownership.
        let required = unsafe { CFCharacterSet::new_copy(None, Some(&required)) }?;
        Some(Self::Required(required))
    }

    fn accepts(&self, font: &CTFont) -> bool {
        let Self::Required(required) = self else {
            return true;
        };
        // SAFETY: `font` is a live retained CoreText object. The generated
        // Copy wrapper returns an independently retained character set.
        let available = unsafe { font.character_set() };
        // SAFETY: `Some` proves the argument is non-null and of the exact type.
        unsafe { available.is_superset_of_set(Some(required)) }
    }
}

fn add_scalar_range(set: &CFMutableCharacterSet, first: u32, second: u32) -> Option<()> {
    let from = first.min(second);
    let to = first.max(second);
    if to > 0x10ffff {
        return None;
    }
    for (segment_start, segment_end) in [(from, to.min(0xd7ff)), (from.max(0xe000), to)] {
        if segment_start > segment_end {
            continue;
        }
        let range = CFRange {
            location: segment_start as isize,
            length: (segment_end - segment_start + 1) as isize,
        };
        // SAFETY: set is non-null and retained; the normalized inclusive range
        // is within Unicode scalar bounds and converted to location + length.
        unsafe { CFMutableCharacterSet::add_characters_in_range(Some(set), range) };
    }
    Some(())
}

fn candidate_from_font(font: CFRetained<CTFont>) -> Option<Candidate> {
    let descriptor = unsafe { font.font_descriptor() };
    let path = descriptor_path(&descriptor);
    let postscript_name = unsafe { font.post_script_name() }.to_string();
    let family = unsafe { font.family_name() }.to_string();
    let symbolic = unsafe { font.symbolic_traits() };
    let traits = typed_number_dictionary(unsafe { font.traits() });
    let weight = traits
        .get(unsafe { kCTFontWeightTrait })
        .and_then(|value| value.as_f64())
        .map(core_text_weight_to_css)
        .unwrap_or(400);
    let width = traits
        .get(unsafe { kCTFontWidthTrait })
        .and_then(|value| value.as_f64())
        .map(core_text_width)
        .unwrap_or(FontWidth::Normal);
    Some(Candidate {
        path,
        postscript_name,
        family,
        weight,
        slant: if symbolic.contains(CTFontSymbolicTraits::TraitItalic) {
            FontSlant::Italic
        } else {
            FontSlant::Normal
        },
        width,
        spacing: if symbolic.contains(CTFontSymbolicTraits::TraitMonoSpace) {
            100
        } else {
            0
        },
        variation_coords: realized_variations(&font),
    })
}

fn descriptor_path(descriptor: &CTFontDescriptor) -> Option<PathBuf> {
    let value = unsafe { descriptor.attribute(kCTFontURLAttribute) }?;
    let url = value.downcast::<CFURL>().ok()?;
    let path = url.file_system_path(CFURLPathStyle::CFURLPOSIXPathStyle)?;
    Some(PathBuf::from(path.to_string()))
}

fn typed_number_dictionary(
    values: CFRetained<CFDictionary>,
) -> CFRetained<CFDictionary<CFString, CFNumber>> {
    // SAFETY: CoreText's font-traits contract defines CFString keys and
    // CFNumber values. This helper is the only place that assertion is made.
    unsafe { CFRetained::cast_unchecked(values) }
}

fn realized_variations(font: &CTFont) -> Vec<FontVariationCoord> {
    // SAFETY: CoreText defines the variation dictionary as CFNumber axis keys
    // and CFNumber values. A missing dictionary means the default instance.
    let Some(values) = (unsafe { font.variation() }) else {
        return Vec::new();
    };
    let values: CFRetained<CFDictionary<CFNumber, CFNumber>> =
        unsafe { CFRetained::cast_unchecked(values) };
    let (tags, values) = values.to_vecs();
    let mut coords: Vec<_> = tags
        .into_iter()
        .zip(values)
        .filter_map(|(tag, value)| {
            FontVariationCoord::try_new(u32::try_from(tag.as_i64()?).ok()?, value.as_f32()?)
        })
        .collect();
    coords.sort_unstable_by_key(|coord| (coord.tag(), coord.value_bits()));
    coords
}

fn font_with_variations(
    base: CFRetained<CTFont>,
    coords: &[FontVariationCoord],
) -> Option<CFRetained<CTFont>> {
    if coords.is_empty() {
        return Some(base);
    }
    let mut descriptor = unsafe { base.font_descriptor() };
    for coord in coords {
        let axis = CFNumber::new_i64(i64::from(coord.tag()));
        // SAFETY: the axis identifier is a retained CFNumber and the finite
        // value was validated by `FontVariationCoord::try_new`.
        descriptor = unsafe { descriptor.copy_with_variation(&axis, f64::from(coord.value())) };
    }
    Some(unsafe { CTFont::with_font_descriptor(&descriptor, 0.0, ptr::null()) })
}

fn font_design_metrics(font: &CTFont) -> Option<PlatformFontDesignMetrics> {
    let units_per_em = unsafe { font.units_per_em() };
    let point_size = unsafe { font.size() };
    if units_per_em == 0 || point_size <= 0.0 {
        return None;
    }
    let to_design = |value: f64| (value * f64::from(units_per_em) / point_size).round() as i32;
    let mut chars: Vec<u16> = (32_u16..=126).collect();
    let mut glyphs = vec![0_u16; chars.len()];
    let chars_ptr = NonNull::new(chars.as_mut_ptr())?;
    let glyphs_ptr = NonNull::new(glyphs.as_mut_ptr())?;
    // SAFETY: both buffers contain `len` writable/readable elements and remain
    // alive for the duration of the CoreText calls.
    unsafe {
        font.glyphs_for_characters(chars_ptr, glyphs_ptr, chars.len() as isize);
    }
    let mut advances = vec![CGSize::default(); glyphs.len()];
    unsafe {
        font.advances_for_glyphs(
            CTFontOrientation::Horizontal,
            glyphs_ptr,
            advances.as_mut_ptr(),
            glyphs.len() as isize,
        );
    }
    let advances: Vec<i32> = advances
        .into_iter()
        .map(|advance| to_design(advance.width))
        .filter(|advance| *advance > 0)
        .collect();
    let max_advance = advances.iter().copied().max().unwrap_or(0);
    let average_advance = if advances.is_empty() {
        0
    } else {
        advances.iter().sum::<i32>() / advances.len() as i32
    };
    Some(PlatformFontDesignMetrics {
        units_per_em,
        ascent: to_design(unsafe { font.ascent() }),
        descent: to_design(unsafe { font.descent() }),
        line_gap: to_design(unsafe { font.leading() }),
        max_advance,
        space_advance: advances.first().copied().unwrap_or(0),
        average_advance,
    })
}

fn core_text_weight_to_css(weight: f64) -> u16 {
    const MAPPING: [f64; 9] = [-0.7, -0.5, -0.23, 0.0, 0.2, 0.3, 0.4, 0.6, 0.8];
    let upper = MAPPING.partition_point(|candidate| *candidate < weight);
    let index = if upper == 0 {
        0.0
    } else if upper >= MAPPING.len() {
        MAPPING.len() as f64
    } else {
        let lower = upper - 1;
        lower as f64 + (weight - MAPPING[lower]) / (MAPPING[upper] - MAPPING[lower])
    };
    ((index + 1.0) * 100.0).round().clamp(100.0, 900.0) as u16
}

fn core_text_width(width: f64) -> FontWidth {
    match width {
        ..=-0.75 => FontWidth::UltraCondensed,
        ..=-0.5 => FontWidth::ExtraCondensed,
        ..=-0.25 => FontWidth::Condensed,
        ..=-0.1 => FontWidth::SemiCondensed,
        ..=0.1 => FontWidth::Normal,
        ..=0.25 => FontWidth::SemiExpanded,
        ..=0.5 => FontWidth::Expanded,
        ..=0.75 => FontWidth::ExtraExpanded,
        _ => FontWidth::UltraExpanded,
    }
}
