//! Canonical records for metrics-bearing font instances.
//!
//! GNU `font_open_entity` reuses an opened entity at its pixel size when
//! `cached_font_ok` permits it; `font_clear_cache` invalidates its observations.
//! Here IDs survive for the resolver lifetime, while records expire only on a
//! catalog transition. Keeping both states here prevents independent builders
//! or cache-clearing paths from publishing conflicting records under one ID.

use neomacs_display_protocol::font::{
    FontReplay, FontSlantKind, ResolvedFont, ResolvedFontAdvance, ResolvedFontId,
    ResolvedFontIdentity,
};
use rustc_hash::FxHashMap;

/// Builders provide observations, never the identity fields owned by the key.
/// In particular, a builder cannot choose another ID, replay, or pixel size.
pub(super) struct FontInstanceProperties {
    pub family: String,
    pub full_name: Option<String>,
    pub postscript_name: Option<String>,
    pub weight: u16,
    pub slant: FontSlantKind,
    pub width: u16,
    pub ascent_px: f32,
    pub descent_px: f32,
    pub space_advance_px: f32,
    pub glyph_advance: ResolvedFontAdvance,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct FontInstanceKey {
    identity: ResolvedFontIdentity,
    replay: FontReplay,
    pixel_size_bits: u32,
}

struct InternedFontInstance {
    id: ResolvedFontId,
    record: Option<ResolvedFont>,
}

#[derive(Default)]
pub(super) struct FontInstanceInterner {
    instances: FxHashMap<FontInstanceKey, InternedFontInstance>,
}

impl FontInstanceInterner {
    pub fn intern(
        &mut self,
        identity: ResolvedFontIdentity,
        replay: FontReplay,
        pixel_size: f32,
        observe: impl FnOnce() -> FontInstanceProperties,
    ) -> ResolvedFont {
        let key = FontInstanceKey {
            identity: identity.clone(),
            replay: replay.clone(),
            pixel_size_bits: pixel_size.to_bits(),
        };
        let allocated = self.instances.len();
        let instance = self
            .instances
            .entry(key)
            .or_insert_with(|| InternedFontInstance {
                id: ResolvedFontId(
                    u32::try_from(allocated)
                        .ok()
                        .and_then(|count| count.checked_add(1))
                        .expect("resolved font ID space exhausted"),
                ),
                record: None,
            });
        instance
            .record
            .get_or_insert_with(|| {
                let properties = observe();
                ResolvedFont {
                    id: instance.id,
                    identity,
                    replay,
                    pixel_size,
                    family: properties.family,
                    full_name: properties.full_name,
                    postscript_name: properties.postscript_name,
                    weight: properties.weight,
                    slant: properties.slant,
                    width: properties.width,
                    ascent_px: properties.ascent_px,
                    descent_px: properties.descent_px,
                    space_advance_px: properties.space_advance_px,
                    glyph_advance: properties.glyph_advance,
                }
            })
            .clone()
    }

    /// Old frame snapshots retain their owned records; new observations reuse
    /// the stable ID after a catalog change. Ordinary cache clears do not call
    /// this, so request provenance cannot change a published record.
    pub fn invalidate_catalog_records(&mut self) {
        for instance in self.instances.values_mut() {
            instance.record = None;
        }
    }
}
