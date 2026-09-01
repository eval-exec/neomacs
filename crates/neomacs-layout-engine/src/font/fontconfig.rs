//! Fontconfig-based font discovery aligned more closely with GNU Emacs.
//!
//! GNU Emacs does not use `fc-match` as its primary font fallback pipeline for
//! character lookup.  Instead, `fontset.c` chooses an ordered font-spec list,
//! `font.c` expands family alternatives and scores style closeness, and
//! `ftfont.c` discovers candidates with `FcFontList`.
//!
//! This module mirrors that shape:
//! - generic family resolution uses Fontconfig directly
//! - character fallback uses Neovm's shared fontset state
//! - candidate discovery uses Fontconfig's `FcFontList`
//! - style selection is scored in Rust instead of delegated to Fontconfig

use crate::font::policy::{
    combined_query_langs, query_charset_ranges, registry_language, representative_char_for_spec,
};
use crate::font::selection::{CandidateSelectionScore, candidate_selection_score};
pub use crate::font::subpixel::FontconfigSubpixelOrder;
use crate::font_backend::{FontFamilyName, PlatformFontSize};
#[cfg(unix)]
use fontconfig_sys::constants::{
    FC_RGBA, FC_RGBA_BGR, FC_RGBA_NONE, FC_RGBA_RGB, FC_RGBA_VBGR, FC_RGBA_VRGB,
};
use neomacs_display_protocol::font::FontVariationCoord;
use neovm_core::emacs_core::font::alternative_font_families;
use neovm_core::emacs_core::fontset::{
    FontSpecEntry, StoredFontSpec, fontset_generation, matching_entries_for_char,
};
use neovm_core::emacs_core::intern::{intern, resolve_sym};
use neovm_core::face::{FontSlant, FontWeight, FontWidth};
use std::collections::HashMap;
#[cfg(unix)]
use std::ffi::{CStr, CString};
#[cfg(not(unix))]
use std::process::Command;
#[cfg(unix)]
use std::ptr;
use std::sync::{Mutex, OnceLock};
#[cfg(unix)]
use {fontconfig::Pattern, fontconfig_sys};

/// Generation-local Fontconfig query results. One owner makes catalog
/// invalidation exhaustive instead of requiring every new query cache to grow
/// another unrelated global reset hook.
#[derive(Default)]
struct FontconfigCaches {
    aliases: Option<HashMap<String, String>>,
    spacing: HashMap<String, Option<i32>>,
    subpixel_order: Option<FontconfigSubpixelOrder>,
    char_matches: HashMap<CharMatchCacheKey, Option<FontMatch>>,
}

static FC_CACHES: OnceLock<Mutex<FontconfigCaches>> = OnceLock::new();
#[cfg(unix)]
static FC_HANDLE: OnceLock<Option<fontconfig::Fontconfig>> = OnceLock::new();

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FontMatch {
    pub family: String,
    pub file: Option<String>,
    /// Fontconfig's exact `FC_INDEX`, including the named-instance bits for
    /// variable fonts (for example Noto Sans Bold is `7 << 16`).
    pub face_index: u32,
    /// Exact variable-font coordinates selected by Fontconfig.
    pub variation_coords: Vec<FontVariationCoord>,
    pub postscript_name: Option<String>,
    pub weight: Option<u16>,
    pub slant: FontSlant,
    pub size: PlatformFontSize,
}

fn fontconfig_caches() -> &'static Mutex<FontconfigCaches> {
    FC_CACHES.get_or_init(|| Mutex::new(FontconfigCaches::default()))
}

/// Drop every answer derived from the previous native catalog generation.
pub(crate) fn invalidate_catalog_caches() {
    *fontconfig_caches()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = FontconfigCaches::default();
}

impl FontMatch {
    pub fn is_italic(&self) -> bool {
        self.slant.is_italic()
    }

    pub fn is_oblique(&self) -> bool {
        matches!(self.slant, FontSlant::Oblique | FontSlant::ReverseOblique)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpecFontMatch {
    pub family: String,
    pub foundry: Option<String>,
    pub registry: Option<String>,
    pub file: Option<String>,
    pub face_index: u32,
    pub variation_coords: Vec<FontVariationCoord>,
    pub weight: Option<u16>,
    pub slant: FontSlant,
    pub width: Option<FontWidth>,
    pub spacing: Option<i32>,
    pub postscript_name: Option<String>,
}

/// Enumerate Fontconfig families using GNU `ftfont_list_family`'s projection:
/// an empty pattern and an object set containing only `FC_FAMILY`.
#[cfg(unix)]
pub(crate) fn list_families() -> Vec<FontFamilyName> {
    if fontconfig_handle().is_none() {
        return Vec::new();
    }
    let pattern = unsafe { fontconfig_sys::FcPatternCreate() };
    if pattern.is_null() {
        return Vec::new();
    }
    let pattern = FcPatternGuard(pattern);
    let object_set = unsafe { fontconfig_sys::FcObjectSetCreate() };
    if object_set.is_null() {
        return Vec::new();
    }
    let object_set = FcObjectSetGuard(object_set);
    if unsafe { fontconfig_sys::FcObjectSetAdd(object_set.0, fontconfig::FC_FAMILY.as_ptr()) } == 0
    {
        return Vec::new();
    }
    let fontset = unsafe {
        FcFontSetGuard(fontconfig_sys::FcFontList(
            ptr::null_mut(),
            pattern.0,
            object_set.0,
        ))
    };
    if fontset.0.is_null() {
        return Vec::new();
    }
    let nfont = unsafe { (*fontset.0).nfont };
    if nfont <= 0 {
        return Vec::new();
    }
    let fonts = unsafe { (*fontset.0).fonts };
    if fonts.is_null() {
        return Vec::new();
    }
    unsafe { std::slice::from_raw_parts(fonts, nfont as usize) }
        .iter()
        .filter_map(|&pattern| raw_pattern_string(pattern, fontconfig::FC_FAMILY))
        .filter_map(FontFamilyName::new)
        .collect()
}

#[cfg(not(unix))]
pub(crate) fn list_families() -> Vec<FontFamilyName> {
    let output = match Command::new("fc-list")
        .arg(":")
        .arg("--format=%{family}\n")
        .output()
    {
        Ok(output) if output.status.success() => output,
        _ => return Vec::new(),
    };
    String::from_utf8(output.stdout)
        .ok()
        .into_iter()
        .flat_map(|output| {
            output
                .lines()
                .flat_map(|line| line.split(','))
                .filter_map(FontFamilyName::new)
                .collect::<Vec<_>>()
        })
        .collect()
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct CharMatchCacheKey {
    family: String,
    codepoint: u32,
    prefer_monospace: bool,
    requested_weight: u16,
    italic: bool,
    fontset_generation: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ListedFont {
    pub(crate) matched: FontMatch,
    style: String,
    weight_css: Option<u16>,
    pub(crate) width: Option<FontWidth>,
    pub(crate) spacing: Option<i32>,
    /// FC_FOUNDRY (e.g. "GOOG"); GNU carries it on font entities
    /// (src/ftfont.c ftfont_pattern_entity reads FC_FOUNDRY).
    pub(crate) foundry: Option<String>,
}

const FONT_SPACING_PROPORTIONAL: i32 = 0;
const FONT_SPACING_DUAL: i32 = 90;
const FONT_SPACING_MONO: i32 = 100;
const FONT_SPACING_CHARCELL: i32 = 110;
const FONT_SPACING_MONO_MAX: i32 = FONT_SPACING_CHARCELL - 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SpacingClass {
    Proportional,
    Dual,
    Mono,
    Charcell,
}

/// Resolve a generic font family name through fontconfig.
///
/// For generic names ("Monospace", "Serif", "Sans Serif"), queries fontconfig
/// to find the concrete family name. Returns the original name unchanged for
/// non-generic families or if fontconfig is unavailable.
pub fn resolve_family(generic_name: &str) -> String {
    let lower = generic_name.to_lowercase();
    let cached = fontconfig_caches()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .aliases
        .as_ref()
        .map(|aliases| aliases.get(&lower).cloned());
    if let Some(resolved) = cached {
        return resolved.unwrap_or_else(|| generic_name.to_owned());
    }

    let aliases = {
        let mut map = HashMap::new();
        for generic in &["monospace", "serif", "sans-serif", "sans serif"] {
            if let Some(concrete) = fc_match_family(generic) {
                tracing::info!("fontconfig: {} -> {}", generic, concrete);
                map.insert(generic.to_string(), concrete);
            }
        }
        map
    };
    let resolved = aliases
        .get(&lower)
        .cloned()
        .unwrap_or_else(|| generic_name.to_owned());
    let mut caches = fontconfig_caches()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    caches.aliases.get_or_insert(aliases);
    resolved
}

/// Return true when FAMILY should prefer a monospace fallback for uncovered
/// characters, matching GNU Emacs' fontset behavior more closely than
/// cosmic-text's generic fallback.
pub fn family_prefers_monospace(family: &str) -> bool {
    let lower = family.to_lowercase();
    if matches!(lower.as_str(), "" | "mono" | "monospace")
        || lower.contains(" mono")
        || lower.ends_with("mono")
    {
        return true;
    }

    family_spacing(family) == Some(100)
}

/// Resolve a character-capable fallback font through shared fontset state and
/// a Fontconfig candidate list, approximating GNU Emacs' `face_for_char`
/// pipeline.
pub fn match_font_for_char(
    family: &str,
    ch: char,
    prefer_monospace: bool,
    requested_weight: u16,
    italic: bool,
) -> Option<FontMatch> {
    if ch.is_ascii() {
        return None;
    }

    let key = CharMatchCacheKey {
        family: family.to_string(),
        codepoint: ch as u32,
        prefer_monospace,
        requested_weight,
        italic,
        fontset_generation: fontset_generation(),
    };
    if let Some(cached) = fontconfig_caches()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .char_matches
        .get(&key)
    {
        return cached.clone();
    }

    let matched =
        match_font_for_char_uncached(family, ch, prefer_monospace, requested_weight, italic);

    fontconfig_caches()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .char_matches
        .insert(key, matched.clone());

    matched
}

pub fn find_font_for_spec(
    family: Option<&str>,
    registry: Option<&str>,
    lang: Option<&str>,
    weight: Option<FontWeight>,
    slant: Option<FontSlant>,
    width: Option<FontWidth>,
) -> Option<SpecFontMatch> {
    let resolved_family = family
        .map(str::trim)
        .filter(|family| !family.is_empty())
        .map(resolve_family)
        .map(|family| intern(&family));
    let spec = StoredFontSpec {
        family: resolved_family,
        registry: registry.map(|registry| intern(&registry.to_ascii_lowercase())),
        lang: lang.map(|lang| intern(&lang.to_ascii_lowercase())),
        weight,
        slant,
        width,
        repertory: None,
    };
    let representative = representative_char_for_spec(&spec);
    let query_charset_ranges = query_charset_ranges(&spec, representative);
    let registry_lang = spec.registry.map(resolve_sym).and_then(registry_language);
    let query_langs = combined_query_langs(registry_lang, spec.lang.map(resolve_sym));
    let candidates = fc_list_candidates(
        spec.family.map(resolve_sym),
        &query_charset_ranges,
        None,
        &query_langs,
    );

    select_find_font_candidate(candidates, &spec).map(|mut candidate| {
        enrich_selected_font_match(&mut candidate.matched);
        SpecFontMatch {
            family: candidate.matched.family,
            foundry: candidate.foundry,
            registry: Some("iso10646-1".to_string()),
            file: candidate.matched.file,
            face_index: candidate.matched.face_index,
            variation_coords: candidate.matched.variation_coords,
            weight: candidate
                .weight_css
                .or_else(|| style_weight(&candidate.style)),
            slant: candidate.matched.slant,
            width: candidate.width,
            spacing: candidate.spacing,
            postscript_name: candidate.matched.postscript_name,
        }
    })
}

fn enrich_selected_font_match(matched: &mut FontMatch) {
    let Some(file) = matched.file.as_deref() else {
        return;
    };
    if matched.variation_coords.is_empty() && (matched.face_index >> 16) & 0x7fff != 0 {
        matched.variation_coords =
            crate::font::probe::named_instance_variation_coords(file, matched.face_index);
    }
    if matched.postscript_name.is_none() {
        matched.postscript_name = crate::font::probe::postscript_name(file, matched.face_index);
    }
}

fn select_find_font_candidate(
    candidates: Vec<ListedFont>,
    spec: &StoredFontSpec,
) -> Option<ListedFont> {
    let requested_family = spec.family.map(resolve_sym);
    candidates
        .into_iter()
        .enumerate()
        .filter(|(_, candidate)| candidate_matches_find_font_spec(candidate, spec))
        .min_by_key(|(ordinal, candidate)| {
            (
                family_affinity_score(requested_family, &candidate.matched.family),
                requested_width_distance(spec.width, candidate.width),
                *ordinal,
            )
        })
        .map(|(_, candidate)| candidate)
}

fn requested_width_distance(requested: Option<FontWidth>, candidate: Option<FontWidth>) -> u16 {
    requested.map_or(0, |requested| {
        candidate
            .unwrap_or(FontWidth::Normal)
            .gnu_numeric()
            .abs_diff(requested.gnu_numeric())
    })
}

fn match_font_for_char_uncached(
    family: &str,
    ch: char,
    prefer_monospace: bool,
    requested_weight: u16,
    italic: bool,
) -> Option<FontMatch> {
    for entry in matching_entries_for_char(ch) {
        match entry {
            FontSpecEntry::ExplicitNone => {
                tracing::debug!(
                    "fontconfig: U+{:04X} hit explicit nil fontset entry",
                    ch as u32
                );
                return None;
            }
            FontSpecEntry::Font(spec) => {
                if let Some(matched) = match_font_from_spec(
                    family,
                    ch,
                    prefer_monospace,
                    requested_weight,
                    italic,
                    &spec,
                ) {
                    tracing::debug!(
                        "fontconfig: fontset matched U+{:04X} req_family={} spec={:?} -> {:?}",
                        ch as u32,
                        family,
                        spec,
                        matched
                    );
                    return Some(matched);
                }
            }
        }
    }

    let fallback_spec = StoredFontSpec {
        family: None,
        registry: None,
        lang: None,
        weight: None,
        slant: None,
        width: None,
        repertory: None,
    };
    let matched = match_font_from_spec(
        family,
        ch,
        prefer_monospace,
        requested_weight,
        italic,
        &fallback_spec,
    );
    if let Some(ref matched) = matched {
        tracing::debug!(
            "fontconfig: generic fallback matched U+{:04X} req_family={} -> {:?}",
            ch as u32,
            family,
            matched
        );
    }
    matched
}

fn match_font_from_spec(
    requested_family: &str,
    ch: char,
    prefer_monospace: bool,
    requested_weight: u16,
    italic: bool,
    spec: &StoredFontSpec,
) -> Option<FontMatch> {
    let effective_weight = spec
        .weight
        .map(FontWeight::css_weight)
        .unwrap_or(requested_weight);
    let query_charset_ranges = query_charset_ranges(spec, ch);
    let registry_lang = spec.registry.map(resolve_sym).and_then(registry_language);
    let query_langs = combined_query_langs(registry_lang, spec.lang.map(resolve_sym));
    let requested_spacing = requested_spacing(spec);

    for family_option in family_search_order(requested_family, spec) {
        let candidates = fc_list_candidates(
            family_option.as_deref(),
            &query_charset_ranges,
            Some(ch as u32),
            &query_langs,
        );
        if candidates.is_empty() {
            continue;
        }

        let best = best_candidate_for_pass(
            candidates,
            effective_weight,
            italic,
            spec.slant,
            requested_spacing,
            prefer_monospace,
            family_option.as_deref(),
        )?;
        return Some(best);
    }

    None
}

fn candidate_matches_find_font_spec(candidate: &ListedFont, spec: &StoredFontSpec) -> bool {
    if let Some(weight) = spec.weight
        && candidate
            .weight_css
            .or_else(|| style_weight(&candidate.style))
            != Some(weight.css_weight())
    {
        return false;
    }

    if let Some(slant) = spec.slant
        && candidate.matched.slant != slant
    {
        return false;
    }

    true
}

fn best_candidate_for_pass(
    candidates: Vec<ListedFont>,
    requested_weight: u16,
    italic: bool,
    requested_slant: Option<neovm_core::face::FontSlant>,
    requested_spacing: Option<i32>,
    prefer_monospace: bool,
    queried_family: Option<&str>,
) -> Option<FontMatch> {
    candidates
        .into_iter()
        .enumerate()
        .map(|(ordinal, candidate)| {
            let score = candidate_score(
                &candidate,
                requested_weight,
                italic,
                requested_slant,
                requested_spacing,
                prefer_monospace,
                queried_family,
            );
            (ordinal, score, candidate.matched)
        })
        .min_by_key(|(ordinal, score, _)| (*score, *ordinal))
        .map(|(_, _, matched)| matched)
}

fn family_search_order(requested_family: &str, spec: &StoredFontSpec) -> Vec<Option<String>> {
    if let Some(spec_family) = spec.family.map(resolve_sym) {
        return vec![Some(resolve_family(spec_family))];
    }

    if requested_family.is_empty() {
        return vec![None];
    }

    // GNU font_find_for_lface consults face-alternative-font-family-alist
    // before retrying with an unspecified family.
    let mut order = Vec::new();
    for family in alternative_font_families(requested_family) {
        let resolved = resolve_family(&family);
        if resolved == family {
            order.push(Some(family));
        } else {
            order.push(Some(resolved));
            order.push(Some(family));
        }
    }
    order.push(None);
    order
}

fn candidate_score(
    candidate: &ListedFont,
    requested_weight: u16,
    italic: bool,
    requested_slant: Option<neovm_core::face::FontSlant>,
    requested_spacing: Option<i32>,
    prefer_monospace: bool,
    queried_family: Option<&str>,
) -> CandidateSelectionScore {
    let style = candidate.style.to_ascii_lowercase();
    let candidate_weight = candidate
        .weight_css
        .or_else(|| style_weight(&style))
        .unwrap_or(400);
    let candidate_slant = style_slant(&style);
    let requested_slant = requested_slant.unwrap_or(if italic {
        neovm_core::face::FontSlant::Italic
    } else {
        neovm_core::face::FontSlant::Normal
    });

    let compatibility = spacing_score(requested_spacing, candidate.spacing, prefer_monospace)
        + family_affinity_score(queried_family, &candidate.matched.family);
    candidate_selection_score(
        compatibility,
        0,
        requested_weight,
        requested_slant,
        None,
        candidate_weight,
        candidate_slant,
        candidate.width,
        PlatformFontSize::Scalable,
    )
    .expect("this legacy listing path scores only scalable candidates")
}

fn requested_spacing(_spec: &StoredFontSpec) -> Option<i32> {
    // GNU ftfont only filters by FC_SPACING when the font-spec itself requests
    // spacing.  Inferring "monospace" from the current face family over-weights
    // Fontconfig spacing metadata and makes variable CJK companions lose to
    // unrelated fixed-pitch families.
    None
}

fn spacing_score(
    requested_spacing: Option<i32>,
    candidate_spacing: Option<i32>,
    prefer_monospace: bool,
) -> u32 {
    let requested = requested_spacing.and_then(normalize_spacing);
    let candidate = candidate_spacing.and_then(normalize_spacing);

    match (requested, candidate) {
        (Some(requested), Some(candidate)) if requested == candidate => 0,
        (Some(SpacingClass::Mono | SpacingClass::Charcell), Some(SpacingClass::Dual))
            if prefer_monospace =>
        {
            25
        }
        (Some(SpacingClass::Dual), Some(SpacingClass::Mono | SpacingClass::Charcell))
            if prefer_monospace =>
        {
            25
        }
        (Some(_), None) if prefer_monospace => 800,
        (Some(requested), Some(candidate)) => spacing_distance(requested, candidate),
        _ => 0,
    }
}

fn family_affinity_score(queried_family: Option<&str>, candidate_family: &str) -> u32 {
    let Some(queried_family) = queried_family.filter(|family| !family.is_empty()) else {
        return 0;
    };

    let queried = queried_family.to_ascii_lowercase();
    let candidate = candidate_family.to_ascii_lowercase();

    if candidate == queried {
        return 0;
    }

    if candidate.starts_with(&queried) || queried.starts_with(&candidate) {
        return 5;
    }

    if candidate.contains(&queried) || queried.contains(&candidate) {
        return 15;
    }

    80
}

fn normalize_spacing(spacing: i32) -> Option<SpacingClass> {
    match spacing {
        i32::MIN..=-1 => None,
        FONT_SPACING_PROPORTIONAL..=89 => Some(SpacingClass::Proportional),
        FONT_SPACING_DUAL..=99 => Some(SpacingClass::Dual),
        FONT_SPACING_MONO..=FONT_SPACING_MONO_MAX => Some(SpacingClass::Mono),
        _ => Some(SpacingClass::Charcell),
    }
}

fn spacing_distance(requested: SpacingClass, candidate: SpacingClass) -> u32 {
    use SpacingClass::{Charcell, Dual, Mono, Proportional};

    match (requested, candidate) {
        (Proportional, Dual) | (Dual, Proportional) => 500,
        (Proportional, Mono) | (Mono, Proportional) => 800,
        (Proportional, Charcell) | (Charcell, Proportional) => 1_000,
        (Dual, Mono) | (Mono, Dual) => 200,
        (Dual, Charcell) | (Charcell, Dual) => 250,
        (Mono, Charcell) | (Charcell, Mono) => 100,
        _ => 0,
    }
}

fn style_weight(style: &str) -> Option<u16> {
    let normalized = style.to_ascii_lowercase().replace([' ', '-'], "");
    for (needle, weight) in [
        ("thin", 100u16),
        ("ultralight", 100),
        ("extralight", 200),
        ("light", 300),
        ("semibold", 600),
        ("demibold", 600),
        ("bold", 700),
        ("extrabold", 800),
        ("ultrabold", 800),
        ("regular", 400),
        ("book", 400),
        ("medium", 500),
        ("black", 900),
        ("heavy", 900),
    ] {
        if normalized.contains(needle) {
            return Some(weight);
        }
    }
    None
}

fn style_slant(style: &str) -> neovm_core::face::FontSlant {
    if style.contains("oblique") {
        FontSlant::Oblique
    } else if style.contains("italic") {
        FontSlant::Italic
    } else {
        FontSlant::Normal
    }
}

#[cfg(unix)]
fn fontconfig_handle() -> Option<&'static fontconfig::Fontconfig> {
    FC_HANDLE.get_or_init(fontconfig::Fontconfig::new).as_ref()
}

#[cfg(unix)]
struct FcObjectSetGuard(*mut fontconfig_sys::FcObjectSet);

#[cfg(unix)]
impl Drop for FcObjectSetGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { fontconfig_sys::FcObjectSetDestroy(self.0) };
        }
    }
}

#[cfg(unix)]
struct FcPatternGuard(*mut fontconfig_sys::FcPattern);

#[cfg(unix)]
impl Drop for FcPatternGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { fontconfig_sys::FcPatternDestroy(self.0) };
        }
    }
}

#[cfg(unix)]
struct FcFontSetGuard(*mut fontconfig_sys::FcFontSet);

#[cfg(unix)]
impl Drop for FcFontSetGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { fontconfig_sys::FcFontSetDestroy(self.0) };
        }
    }
}

#[cfg(unix)]
fn add_string_property(pattern: &mut Pattern<'_>, key: &std::ffi::CStr, value: &str) -> Option<()> {
    let value = CString::new(value).ok()?;
    pattern.add_string(key, &value).ok()
}

#[cfg(unix)]
fn with_matched_family_pattern<T>(
    fc: &fontconfig::Fontconfig,
    family: &str,
    read: impl FnOnce(&Pattern<'_>) -> Option<T>,
) -> Option<T> {
    let mut pattern = Pattern::new(fc).ok()?;
    add_string_property(&mut pattern, fontconfig::FC_FAMILY, family)?;
    let matched = pattern.font_match().ok()?;
    read(&matched)
}

#[cfg(unix)]
struct FcLangSetGuard(*mut fontconfig_sys::FcLangSet);

#[cfg(unix)]
impl Drop for FcLangSetGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { fontconfig_sys::FcLangSetDestroy(self.0) };
        }
    }
}

#[cfg(unix)]
struct FcCharSetGuard(*mut fontconfig_sys::FcCharSet);

#[cfg(unix)]
impl Drop for FcCharSetGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { fontconfig_sys::FcCharSetDestroy(self.0) };
        }
    }
}

#[cfg(unix)]
fn pattern_family(pattern: &Pattern<'_>) -> Option<String> {
    let family = pattern.get_string(fontconfig::FC_FAMILY).ok()?.trim();
    (!family.is_empty()).then(|| family.to_owned())
}

#[cfg(unix)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GnuEntityProjection {
    Metadata,
    MetadataAndCharset,
}

#[cfg(unix)]
fn build_candidate_object_set(projection: GnuEntityProjection) -> Option<FcObjectSetGuard> {
    let object_set = unsafe { fontconfig_sys::FcObjectSetCreate() };
    if object_set.is_null() {
        return None;
    }
    let guard = FcObjectSetGuard(object_set);
    // Keep this projection identical to GNU Emacs `ftfont_list`.  Fontconfig
    // uses the object set while uniquifying/listing patterns, so asking for
    // renderer-enrichment fields here changes entity order and therefore
    // changes GNU's equal-score tie behavior.  Exact variation coordinates
    // are enriched from the named-instance face index after selection.
    let keys: [&CStr; 15] = [
        fontconfig::FC_FOUNDRY,
        fontconfig::FC_FAMILY,
        fontconfig::FC_WEIGHT,
        fontconfig::FC_SLANT,
        fontconfig::FC_WIDTH,
        fontconfig::FC_PIXEL_SIZE,
        fontconfig::FC_SPACING,
        fontconfig::FC_SCALABLE,
        fontconfig::FC_STYLE,
        fontconfig::FC_FILE,
        fontconfig::FC_INDEX,
        fontconfig_sys::constants::FC_CAPABILITY,
        fontconfig::FC_FONTFORMAT,
        fontconfig::FC_COLOR,
        fontconfig::FC_VARIABLE,
    ];
    for key in keys {
        let ok = unsafe { fontconfig_sys::FcObjectSetAdd(object_set, key.as_ptr()) };
        if ok == 0 {
            return None;
        }
    }
    if projection == GnuEntityProjection::MetadataAndCharset {
        let ok =
            unsafe { fontconfig_sys::FcObjectSetAdd(object_set, fontconfig::FC_CHARSET.as_ptr()) };
        if ok == 0 {
            return None;
        }
    }
    Some(guard)
}

#[cfg(unix)]
fn raw_pattern_string(pattern: *mut fontconfig_sys::FcPattern, key: &CStr) -> Option<String> {
    let mut raw_value: *mut fontconfig_sys::FcChar8 = ptr::null_mut();
    let result =
        unsafe { fontconfig_sys::FcPatternGetString(pattern, key.as_ptr(), 0, &mut raw_value) };
    if result != fontconfig_sys::FcResultMatch || raw_value.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(raw_value.cast()) }
        .to_str()
        .ok()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

#[cfg(unix)]
fn raw_pattern_int(pattern: *mut fontconfig_sys::FcPattern, key: &CStr) -> Option<i32> {
    let mut value = 0;
    let result =
        unsafe { fontconfig_sys::FcPatternGetInteger(pattern, key.as_ptr(), 0, &mut value) };
    (result == fontconfig_sys::FcResultMatch).then_some(value)
}

fn parse_font_variations(value: &str) -> Vec<FontVariationCoord> {
    value
        .split(',')
        .filter_map(|assignment| {
            let (tag, value) = assignment.trim().split_once('=')?;
            let tag: [u8; 4] = tag.trim().as_bytes().try_into().ok()?;
            let value = value.trim().parse::<f32>().ok()?;
            FontVariationCoord::try_new(u32::from_be_bytes(tag), value)
        })
        .collect()
}

#[cfg(unix)]
fn raw_pattern_bool(pattern: *mut fontconfig_sys::FcPattern, key: &CStr) -> Option<bool> {
    let mut value = 0;
    let result = unsafe { fontconfig_sys::FcPatternGetBool(pattern, key.as_ptr(), 0, &mut value) };
    (result == fontconfig_sys::FcResultMatch).then_some(value != 0)
}

#[cfg(unix)]
fn raw_pattern_double(pattern: *mut fontconfig_sys::FcPattern, key: &CStr) -> Option<f64> {
    let mut value = 0.0;
    let result =
        unsafe { fontconfig_sys::FcPatternGetDouble(pattern, key.as_ptr(), 0, &mut value) };
    (result == fontconfig_sys::FcResultMatch && value.is_finite()).then_some(value)
}

#[cfg(unix)]
fn raw_pattern_supports_any_char(
    pattern: *mut fontconfig_sys::FcPattern,
    query_chars: &[u32],
) -> bool {
    if query_chars.is_empty() {
        return true;
    }
    let mut charset: *mut fontconfig_sys::FcCharSet = ptr::null_mut();
    let result = unsafe {
        fontconfig_sys::FcPatternGetCharSet(
            pattern,
            fontconfig::FC_CHARSET.as_ptr(),
            0,
            &mut charset,
        )
    };
    if result != fontconfig_sys::FcResultMatch || charset.is_null() {
        return false;
    }
    query_chars.iter().any(|&codepoint| unsafe {
        fontconfig_sys::FcCharSetHasChar(charset, codepoint as fontconfig_sys::FcChar32) != 0
    })
}

#[cfg(unix)]
fn listed_font_from_raw_pattern(pattern: *mut fontconfig_sys::FcPattern) -> Option<ListedFont> {
    if pattern.is_null() {
        return None;
    }
    let file = raw_pattern_string(pattern, fontconfig::FC_FILE)?;
    let face_index = u32::try_from(raw_pattern_int(pattern, fontconfig::FC_INDEX)?).ok()?;
    let variation_coords =
        raw_pattern_string(pattern, fontconfig_sys::constants::FC_FONT_VARIATIONS)
            .map(|value| parse_font_variations(&value))
            .unwrap_or_default();
    if raw_pattern_bool(pattern, fontconfig::FC_VARIABLE).unwrap_or(false)
        && raw_pattern_int(pattern, fontconfig::FC_WEIGHT).is_none()
    {
        return None;
    }
    let style = raw_pattern_string(pattern, fontconfig::FC_STYLE).unwrap_or_default();
    let weight_css = raw_pattern_int(pattern, fontconfig::FC_WEIGHT).map(map_fontconfig_weight_raw);
    let spacing = raw_pattern_int(pattern, fontconfig::FC_SPACING);
    let matched_family = raw_pattern_string(pattern, fontconfig::FC_FAMILY)?;
    Some(ListedFont {
        matched: FontMatch {
            family: matched_family,
            file: Some(file),
            face_index,
            variation_coords,
            postscript_name: raw_pattern_string(pattern, fontconfig::FC_POSTSCRIPT_NAME),
            weight: weight_css,
            slant: raw_pattern_int(pattern, fontconfig::FC_SLANT)
                .map(map_fontconfig_slant_raw)
                .unwrap_or_else(|| style_slant(&style)),
            size: match raw_pattern_bool(pattern, fontconfig::FC_SCALABLE) {
                Some(true) => PlatformFontSize::Scalable,
                Some(false) => raw_pattern_double(pattern, fontconfig::FC_PIXEL_SIZE)
                    .map(|size| PlatformFontSize::Fixed {
                        device_ppem_26_6: (size * 64.0).round().clamp(1.0, u32::MAX as f64) as u32,
                    })
                    .unwrap_or(PlatformFontSize::Unknown),
                None => PlatformFontSize::Unknown,
            },
        },
        style,
        weight_css,
        width: raw_pattern_int(pattern, fontconfig::FC_WIDTH).map(map_fontconfig_width_raw),
        spacing,
        foundry: raw_pattern_string(pattern, fontconfig::FC_FOUNDRY),
    })
}

#[cfg(unix)]
fn map_fontconfig_weight_raw(weight: i32) -> u16 {
    match weight {
        i32::MIN..=20 => 100,
        21..=45 => 200,
        46..=62 => 300,
        63..=89 => 400,
        90..=139 => 500,
        140..=189 => 600,
        190..=204 => 700,
        205..=209 => 800,
        210..=212 => 900,
        _ => 900,
    }
}

#[cfg(unix)]
fn map_fontconfig_slant_raw(slant: i32) -> FontSlant {
    match slant {
        fontconfig::FC_SLANT_ITALIC => FontSlant::Italic,
        fontconfig::FC_SLANT_OBLIQUE => FontSlant::Oblique,
        _ => FontSlant::Normal,
    }
}

#[cfg(unix)]
fn map_fontconfig_width_raw(width: i32) -> FontWidth {
    match width {
        i32::MIN..=56 => FontWidth::UltraCondensed,
        57..=69 => FontWidth::ExtraCondensed,
        70..=81 => FontWidth::Condensed,
        82..=93 => FontWidth::SemiCondensed,
        94..=106 => FontWidth::Normal,
        107..=119 => FontWidth::SemiExpanded,
        120..=137 => FontWidth::Expanded,
        138..=175 => FontWidth::ExtraExpanded,
        _ => FontWidth::UltraExpanded,
    }
}

/// FC_FOUNDRY for a concrete font file (e.g. "GOOG" for Noto), as GNU
/// carries on font entities/objects. Queried once per file and cached.
pub fn foundry_for_file(file: &str) -> Option<String> {
    static CACHE: OnceLock<Mutex<std::collections::HashMap<String, Option<String>>>> =
        OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
    if let Ok(cache) = cache.lock()
        && let Some(cached) = cache.get(file)
    {
        return cached.clone();
    }
    let found = foundry_for_file_uncached(file);
    if let Ok(mut cache) = cache.lock() {
        cache.insert(file.to_string(), found.clone());
    }
    found
}

#[cfg(unix)]
fn foundry_for_file_uncached(file: &str) -> Option<String> {
    fontconfig_handle()?;
    let pattern = unsafe { fontconfig_sys::FcPatternCreate() };
    if pattern.is_null() {
        return None;
    }
    let pattern = FcPatternGuard(pattern);
    let value = CString::new(file).ok()?;
    let ok = unsafe {
        fontconfig_sys::FcPatternAddString(
            pattern.0,
            fontconfig::FC_FILE.as_ptr(),
            value.as_ptr().cast(),
        )
    };
    if ok == 0 {
        return None;
    }
    let object_set = unsafe { fontconfig_sys::FcObjectSetCreate() };
    if object_set.is_null() {
        return None;
    }
    let object_set = FcObjectSetGuard(object_set);
    let ok =
        unsafe { fontconfig_sys::FcObjectSetAdd(object_set.0, fontconfig::FC_FOUNDRY.as_ptr()) };
    if ok == 0 {
        return None;
    }
    let fontset = unsafe {
        FcFontSetGuard(fontconfig_sys::FcFontList(
            ptr::null_mut(),
            pattern.0,
            object_set.0,
        ))
    };
    if fontset.0.is_null() {
        return None;
    }
    let nfont = unsafe { (*fontset.0).nfont } as usize;
    for i in 0..nfont {
        let candidate = unsafe { *(*fontset.0).fonts.add(i) };
        if let Some(foundry) = raw_pattern_string(candidate, fontconfig::FC_FOUNDRY) {
            return Some(foundry);
        }
    }
    None
}

#[cfg(not(unix))]
fn foundry_for_file_uncached(_file: &str) -> Option<String> {
    None
}

#[cfg(unix)]
pub(crate) fn fc_list_candidates(
    family: Option<&str>,
    query_charset_ranges: &[(u32, u32)],
    required_char: Option<u32>,
    langs: &[String],
) -> Vec<ListedFont> {
    if fontconfig_handle().is_none() {
        return Vec::new();
    }
    let mut candidates = Vec::new();
    let query_langs = if langs.is_empty() {
        vec![None]
    } else {
        langs.iter().map(|lang| Some(lang.as_str())).collect()
    };

    for lang in query_langs {
        let pattern = unsafe { fontconfig_sys::FcPatternCreate() };
        if pattern.is_null() {
            continue;
        }
        let pattern = FcPatternGuard(pattern);
        if let Some(family) = family.filter(|family| !family.is_empty()) {
            let Ok(value) = CString::new(family) else {
                continue;
            };
            let ok = unsafe {
                fontconfig_sys::FcPatternAddString(
                    pattern.0,
                    fontconfig::FC_FAMILY.as_ptr(),
                    value.as_ptr().cast(),
                )
            };
            if ok == 0 {
                continue;
            }
        }
        let query_charset = if query_charset_ranges.is_empty() {
            None
        } else {
            let charset = unsafe { fontconfig_sys::FcCharSetCreate() };
            if charset.is_null() {
                continue;
            }
            let charset = FcCharSetGuard(charset);
            let mut ok = true;
            for &(from, to) in query_charset_ranges {
                for codepoint in from.min(to)..=from.max(to) {
                    let added = unsafe {
                        fontconfig_sys::FcCharSetAddChar(
                            charset.0,
                            codepoint as fontconfig_sys::FcChar32,
                        )
                    };
                    if added == 0 {
                        ok = false;
                        break;
                    }
                }
                if !ok {
                    break;
                }
            }
            if !ok {
                continue;
            }
            let added = unsafe {
                fontconfig_sys::FcPatternAddCharSet(
                    pattern.0,
                    fontconfig::FC_CHARSET.as_ptr(),
                    charset.0,
                )
            };
            if added == 0 {
                continue;
            }
            Some(charset)
        };
        let query_langset = if let Some(lang) = lang.filter(|lang| !lang.is_empty()) {
            let Ok(lang) = CString::new(lang) else {
                continue;
            };
            let langset = unsafe { fontconfig_sys::FcLangSetCreate() };
            if langset.is_null() {
                continue;
            }
            let langset = FcLangSetGuard(langset);
            let added = unsafe { fontconfig_sys::FcLangSetAdd(langset.0, lang.as_ptr().cast()) };
            if added == 0 {
                continue;
            }
            let added = unsafe {
                fontconfig_sys::FcPatternAddLangSet(
                    pattern.0,
                    fontconfig::FC_LANG.as_ptr(),
                    langset.0,
                )
            };
            if added == 0 {
                continue;
            }
            Some(langset)
        } else {
            None
        };
        let _keep_charset_alive = query_charset;
        let _keep_langset_alive = query_langset;
        let projection = if required_char.is_some() && query_charset_ranges.is_empty() {
            GnuEntityProjection::MetadataAndCharset
        } else {
            GnuEntityProjection::Metadata
        };
        let Some(object_set) = build_candidate_object_set(projection) else {
            continue;
        };
        let fontset = unsafe {
            FcFontSetGuard(fontconfig_sys::FcFontList(
                ptr::null_mut(),
                pattern.0,
                object_set.0,
            ))
        };
        if fontset.0.is_null() {
            continue;
        }
        let nfont = unsafe { (*fontset.0).nfont };
        tracing::trace!(
            family = family.unwrap_or(""),
            lang = lang.unwrap_or(""),
            query_charset_ranges = ?query_charset_ranges,
            required_char,
            nfont,
            "fontconfig raw candidate list"
        );
        if nfont <= 0 {
            continue;
        }
        let fonts = unsafe { (*fontset.0).fonts };
        if fonts.is_null() {
            continue;
        }
        let patterns = unsafe { std::slice::from_raw_parts(fonts, nfont as usize) };
        let before = candidates.len();

        for &candidate_pattern in patterns {
            if let Some(required_char) = required_char
                && !raw_pattern_supports_any_char(candidate_pattern, &[required_char])
            {
                continue;
            }
            let Some(candidate) = listed_font_from_raw_pattern(candidate_pattern) else {
                continue;
            };
            candidates.push(candidate);
        }
        tracing::trace!(
            family = family.unwrap_or(""),
            lang = lang.unwrap_or(""),
            query_charset_ranges = ?query_charset_ranges,
            required_char,
            added = candidates.len().saturating_sub(before),
            total = candidates.len(),
            "fontconfig filtered candidate list"
        );
    }
    candidates
}

#[cfg(not(unix))]
pub(crate) fn fc_list_candidates(
    family: Option<&str>,
    query_charset_ranges: &[(u32, u32)],
    required_char: Option<u32>,
    langs: &[String],
) -> Vec<ListedFont> {
    let mut candidates = Vec::new();
    let query_langs = if langs.is_empty() {
        vec![None]
    } else {
        langs.iter().map(|lang| Some(lang.as_str())).collect()
    };

    for lang in query_langs {
        let mut pattern = String::from(":charset=");
        let mut all_query_chars = Vec::new();
        for &(from, to) in query_charset_ranges {
            all_query_chars.extend(from.min(to)..=from.max(to));
        }
        if let Some(required_char) = required_char
            && !all_query_chars.contains(&required_char)
        {
            all_query_chars.push(required_char);
        }
        for (index, codepoint) in all_query_chars.iter().enumerate() {
            if index > 0 {
                pattern.push(' ');
            }
            pattern.push_str(&format!("0x{:x}", codepoint));
        }
        if let Some(family) = family.filter(|family| !family.is_empty()) {
            pattern.push_str(":family=");
            pattern.push_str(family);
        }
        if let Some(lang) = lang.filter(|lang| !lang.is_empty()) {
            pattern.push_str(":lang=");
            pattern.push_str(lang);
        }

        let output = match Command::new("fc-list")
            .arg(pattern)
            .arg(
                "--format=%{family}\t%{style}\t%{weight}\t%{spacing}\t%{postscriptname}\t%{file}\n",
            )
            .output()
        {
            Ok(output) if output.status.success() => output,
            _ => continue,
        };

        let Ok(stdout) = String::from_utf8(output.stdout) else {
            continue;
        };

        for line in stdout.lines() {
            let mut fields = line.splitn(6, '\t');
            let Some(families) = fields.next() else {
                continue;
            };
            let style = fields.next().unwrap_or_default().trim().to_string();
            let weight_css = parse_fontconfig_weight(fields.next().unwrap_or_default());
            let spacing = fields
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .and_then(|value| value.parse::<i32>().ok());
            let postscript_name = fields
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
            let file = fields
                .next()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
            let Some(matched_family) = choose_display_family(families, family) else {
                continue;
            };
            let candidate = ListedFont {
                matched: FontMatch {
                    family: matched_family,
                    file,
                    face_index: 0,
                    variation_coords: Vec::new(),
                    postscript_name,
                    weight: weight_css,
                    slant: style_slant(&style),
                    size: PlatformFontSize::Unknown,
                },
                style,
                weight_css,
                width: None,
                spacing,
                foundry: None,
            };
            candidates.push(candidate);
        }
    }
    candidates
}

#[cfg(not(unix))]
fn choose_display_family(families: &str, requested_family: Option<&str>) -> Option<String> {
    let mut names = families
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if let Some(requested) = requested_family
        && let Some(exact) = names
            .clone()
            .find(|candidate| candidate.eq_ignore_ascii_case(requested))
    {
        return Some(exact.to_string());
    }
    names.next().map(ToOwned::to_owned)
}

#[cfg(any(not(unix), test))]
fn parse_fontconfig_weight(raw: &str) -> Option<u16> {
    let raw = raw.trim();
    if raw.is_empty() || raw.starts_with('[') {
        return None;
    }
    let weight = raw.parse::<i32>().ok()?;
    Some(match weight {
        i32::MIN..=20 => 100,
        21..=45 => 200,
        46..=62 => 300,
        63..=89 => 400,
        90..=139 => 500,
        140..=189 => 600,
        190..=204 => 700,
        205..=209 => 800,
        210..=212 => 900,
        _ => 900,
    })
}

#[cfg(unix)]
/// Query Fontconfig for the concrete family name matching a generic family.
fn fc_match_family(generic: &str) -> Option<String> {
    let fc = fontconfig_handle()?;
    with_matched_family_pattern(fc, generic, pattern_family)
}

#[cfg(not(unix))]
/// Query `fc-match` for the concrete family name matching a generic family.
fn fc_match_family(generic: &str) -> Option<String> {
    let output = Command::new("fc-match")
        .arg("--format=%{family}")
        .arg(generic)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let family = String::from_utf8(output.stdout).ok()?;
    let family = family.trim().to_string();
    let family = family.split(',').next()?.trim().to_string();

    if family.is_empty() {
        None
    } else {
        Some(family)
    }
}

#[cfg(unix)]
fn family_spacing(family: &str) -> Option<i32> {
    if let Some(cached) = fontconfig_caches()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .spacing
        .get(family)
    {
        return *cached;
    }

    let spacing = fontconfig_handle().and_then(|fc| {
        with_matched_family_pattern(fc, family, |matched| {
            matched.get_int(fontconfig::FC_SPACING).ok()
        })
    });

    fontconfig_caches()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .spacing
        .insert(family.to_string(), spacing);

    spacing
}

#[cfg(not(unix))]
fn family_spacing(family: &str) -> Option<i32> {
    if let Some(cached) = fontconfig_caches()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .spacing
        .get(family)
    {
        return *cached;
    }

    let spacing = Command::new("fc-match")
        .arg("--format=%{spacing}")
        .arg(family)
        .output()
        .ok()
        .and_then(|output| {
            if !output.status.success() {
                return None;
            }
            String::from_utf8(output.stdout).ok()
        })
        .and_then(|stdout| stdout.trim().parse::<i32>().ok());

    fontconfig_caches()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .spacing
        .insert(family.to_string(), spacing);

    spacing
}

pub fn default_subpixel_order() -> FontconfigSubpixelOrder {
    if let Some(order) = fontconfig_caches()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .subpixel_order
    {
        return order;
    }
    let order = query_default_subpixel_order();
    fontconfig_caches()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .subpixel_order = Some(order);
    order
}

#[cfg(unix)]
fn query_default_subpixel_order() -> FontconfigSubpixelOrder {
    let Some(fc) = fontconfig_handle() else {
        return FontconfigSubpixelOrder::Unknown;
    };

    let rgba =
        with_matched_family_pattern(fc, "Monospace", |matched| matched.get_int(FC_RGBA).ok());
    match rgba {
        Some(FC_RGBA_NONE) => FontconfigSubpixelOrder::None,
        Some(FC_RGBA_RGB) => FontconfigSubpixelOrder::Rgb,
        Some(FC_RGBA_BGR) => FontconfigSubpixelOrder::Bgr,
        Some(FC_RGBA_VRGB) => FontconfigSubpixelOrder::VRgb,
        Some(FC_RGBA_VBGR) => FontconfigSubpixelOrder::VBgr,
        _ => FontconfigSubpixelOrder::Unknown,
    }
}

#[cfg(not(unix))]
fn query_default_subpixel_order() -> FontconfigSubpixelOrder {
    FontconfigSubpixelOrder::Unknown
}

#[cfg(test)]
#[path = "fontconfig_test.rs"]
mod tests;
