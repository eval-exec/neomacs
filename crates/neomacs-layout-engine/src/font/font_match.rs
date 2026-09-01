//! Shared font matching helpers.
//!
//! Keeps layout and rasterization behavior consistent.

use cosmic_text::FontSystem;
use fontdb::{Database, Style as DbStyle};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::RwLock;
use ttf_parser::{Face as TtfFace, Tag};

#[derive(Clone, Debug, Default)]
struct FamilyWeightInfo {
    discrete_weights: Vec<u16>,
    variable_weight_range: Option<(u16, u16)>,
    /// `wght` values of the family's variable fonts' fvar named instances.
    named_instance_weights: Vec<u16>,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum StyleKey {
    Normal,
    Italic,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct CacheKey {
    db_len: usize,
    family_lower: String,
    style: StyleKey,
}

// Cache family/style weight support because querying/parsing font metadata is expensive
// when done for every glyph.
static FAMILY_WEIGHT_CACHE: Lazy<RwLock<HashMap<CacheKey, FamilyWeightInfo>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CosmicFamilySelection<'a> {
    Name(&'a str),
    Monospace,
    Serif,
    SansSerif,
}

/// Resolve a requested weight to the closest available weight in the same family.
///
/// For explicit family names, this prevents cross-family jumps when a specific
/// weight instance is missing from the requested family.
pub fn resolve_weight_in_family(
    font_system: &FontSystem,
    family: &str,
    requested_weight: u16,
    italic: bool,
) -> u16 {
    let family_lc = family.to_lowercase();
    if matches!(
        family_lc.as_str(),
        "" | "mono" | "monospace" | "serif" | "sans" | "sansserif" | "sans-serif"
    ) {
        return requested_weight;
    }

    let style = if italic {
        StyleKey::Italic
    } else {
        StyleKey::Normal
    };
    let db = font_system.db();
    let key = CacheKey {
        db_len: db.len(),
        family_lower: family_lc,
        style,
    };

    if let Ok(cache) = FAMILY_WEIGHT_CACHE.read()
        && let Some(info) = cache.get(&key)
    {
        return resolve_requested_weight(info, requested_weight);
    }

    let info = family_weight_info_for_style(
        db,
        family,
        match style {
            StyleKey::Italic => DbStyle::Italic,
            StyleKey::Normal => DbStyle::Normal,
        },
    );
    let resolved = resolve_requested_weight(&info, requested_weight);

    if let Ok(mut cache) = FAMILY_WEIGHT_CACHE.write() {
        cache.insert(key, info);
    }

    resolved
}

pub fn family_exists(font_system: &FontSystem, family: &str) -> bool {
    font_system.db().faces().any(|face| {
        face.families
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case(family))
    })
}

pub fn should_use_monospace_fallback(font_system: &FontSystem, family: &str) -> bool {
    if family_exists(font_system, family) {
        return false;
    }
    let family = family.trim().to_ascii_lowercase();
    matches!(family.as_str(), "" | "mono" | "monospace")
        || family.contains(" mono")
        || family.ends_with("mono")
}

pub fn select_cosmic_family<'a>(
    font_system: &FontSystem,
    requested_family: &'a str,
) -> CosmicFamilySelection<'a> {
    let resolved = requested_family.trim();
    let family_lower = resolved.to_ascii_lowercase();
    let is_generic = matches!(
        family_lower.as_str(),
        "monospace" | "mono" | "" | "serif" | "sans-serif" | "sans" | "sansserif"
    );

    if is_generic {
        match family_lower.as_str() {
            "serif" => CosmicFamilySelection::Serif,
            "sans-serif" | "sans" | "sansserif" => CosmicFamilySelection::SansSerif,
            _ => CosmicFamilySelection::Monospace,
        }
    } else if should_use_monospace_fallback(font_system, resolved) {
        CosmicFamilySelection::Monospace
    } else {
        CosmicFamilySelection::Name(resolved)
    }
}

fn resolve_requested_weight(info: &FamilyWeightInfo, requested_weight: u16) -> u16 {
    if !info.named_instance_weights.is_empty() {
        // GNU/fontconfig expose a variable font's fvar named instances as
        // concrete font entities and match a request to the nearest one, so
        // the opened weight is always a real instance (e.g. semi-light 350
        // opens Light 300), never a synthesized off-instance value. Snap to
        // the union of named instances and any static sibling faces.
        let mut candidates = info.named_instance_weights.clone();
        candidates.extend_from_slice(&info.discrete_weights);
        candidates.sort_unstable();
        candidates.dedup();
        return pick_nearest_css_weight(&candidates, requested_weight);
    }
    if let Some((min_w, max_w)) = info.variable_weight_range {
        // Variable font without enumerable named instances: clamp to the
        // axis range (synthesize intermediate weights).
        return requested_weight.clamp(min_w, max_w);
    }
    if info.discrete_weights.is_empty() {
        return requested_weight;
    }
    pick_nearest_css_weight(&info.discrete_weights, requested_weight)
}

fn family_weight_info_for_style(db: &Database, family: &str, style: DbStyle) -> FamilyWeightInfo {
    let style_pref = match style {
        DbStyle::Italic => [DbStyle::Italic, DbStyle::Oblique, DbStyle::Normal],
        DbStyle::Oblique => [DbStyle::Oblique, DbStyle::Italic, DbStyle::Normal],
        DbStyle::Normal => [DbStyle::Normal, DbStyle::Oblique, DbStyle::Italic],
    };

    for preferred_style in style_pref {
        let matching_faces: Vec<_> = db
            .faces()
            .filter(|face| face.style == preferred_style)
            .filter(|face| {
                face.families
                    .iter()
                    .any(|(name, _)| name.eq_ignore_ascii_case(family))
            })
            .collect();

        if matching_faces.is_empty() {
            continue;
        }

        let mut discrete_weights: Vec<u16> =
            matching_faces.iter().map(|face| face.weight.0).collect();
        discrete_weights.sort_unstable();
        discrete_weights.dedup();

        let mut variable_weight_range: Option<(u16, u16)> = None;
        let mut named_instance_weights: Vec<u16> = Vec::new();
        for face in matching_faces {
            if let Some((min_w, max_w)) = face_weight_axis_range(db, face.id) {
                variable_weight_range = Some(match variable_weight_range {
                    None => (min_w, max_w),
                    Some((cur_min, cur_max)) => (cur_min.min(min_w), cur_max.max(max_w)),
                });
                if let Some((file, index)) = face_file_and_index(face) {
                    named_instance_weights
                        .extend(crate::font::probe::named_instance_wght_values(&file, index));
                }
            }
        }
        named_instance_weights.sort_unstable();
        named_instance_weights.dedup();

        return FamilyWeightInfo {
            discrete_weights,
            variable_weight_range,
            named_instance_weights,
        };
    }

    FamilyWeightInfo::default()
}

fn face_file_and_index(face: &fontdb::FaceInfo) -> Option<(String, u32)> {
    match &face.source {
        fontdb::Source::File(path) | fontdb::Source::SharedFile(path, _) => {
            Some((path.display().to_string(), face.index))
        }
        fontdb::Source::Binary(_) => None,
    }
}

fn face_weight_axis_range(db: &Database, id: fontdb::ID) -> Option<(u16, u16)> {
    db.with_face_data(id, |font_data, face_index| {
        let face = TtfFace::parse(font_data, face_index).ok()?;
        let wght = Tag::from_bytes(b"wght");
        for axis in face.variation_axes() {
            if axis.tag == wght {
                let min_w = axis.min_value.round().clamp(1.0, 1000.0) as u16;
                let max_w = axis.max_value.round().clamp(1.0, 1000.0) as u16;
                return Some((min_w.min(max_w), min_w.max(max_w)));
            }
        }
        None
    })
    .flatten()
}

// Generic same-family weight fallback for static faces:
// 1) exact match when available
// 2) otherwise choose the closest available weight
//
// GNU Emacs ultimately scores entities by absolute style distance in `font.c`,
// so a requested semi-bold face should prefer a bold instance over a regular
// one when the family only provides 400/700.
fn pick_nearest_css_weight(weights: &[u16], requested_weight: u16) -> u16 {
    if weights.contains(&requested_weight) {
        return requested_weight;
    }
    weights
        .iter()
        .copied()
        .min_by_key(|weight| {
            (
                weight.abs_diff(requested_weight),
                (*weight > requested_weight) as u8,
            )
        })
        .unwrap_or(requested_weight)
}

#[cfg(test)]
#[path = "font_match_test.rs"]
mod tests;
