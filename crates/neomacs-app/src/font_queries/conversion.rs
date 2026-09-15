use super::*;
pub fn font_otf_capability_for_file(
    file: &str,
    face_index: u32,
) -> Option<neovm_core::emacs_core::eval::FontOtfCapability> {
    neomacs_layout_engine::font::probe::otf_capability(file, face_index)
        .map(core_font_otf_capability)
}

pub(super) fn font_otf_capability_for_asset(
    asset: &neomacs_display_protocol::font::FontOutlineAsset,
) -> Option<neovm_core::emacs_core::eval::FontOtfCapability> {
    match asset {
        neomacs_display_protocol::font::FontOutlineAsset::File(file) => {
            font_otf_capability_for_file(file.path(), file.face_index())
        }
        neomacs_display_protocol::font::FontOutlineAsset::Memory(memory) => {
            neomacs_layout_engine::font::probe::otf_capability_from_bytes(
                memory.bytes(),
                memory.face_index(),
            )
            .map(core_font_otf_capability)
        }
    }
}

fn core_font_otf_capability(
    caps: neomacs_layout_engine::font::probe::OtfCapability,
) -> neovm_core::emacs_core::eval::FontOtfCapability {
    let side = |scripts: Vec<neomacs_layout_engine::font::probe::OtfScript>| {
        scripts
            .into_iter()
            .map(|script| {
                (
                    script.tag,
                    script
                        .lang_syses
                        .into_iter()
                        .map(|lang| (lang.tag, lang.features))
                        .collect(),
                )
            })
            .collect()
    };
    neovm_core::emacs_core::eval::FontOtfCapability {
        gsub: side(caps.gsub),
        gpos: side(caps.gpos),
    }
}

pub(super) fn core_font_px_metrics(
    metrics: neomacs_layout_engine::font::probe::FontPxMetrics,
) -> neovm_core::emacs_core::eval::FontPxProbeResult {
    neovm_core::emacs_core::eval::FontPxProbeResult {
        pixel_size: metrics.pixel_size,
        height: metrics.height,
        ascent: metrics.ascent,
        descent: metrics.descent,
        max_width: metrics.max_width,
        space_width: metrics.space_width,
        average_width: metrics.average_width,
    }
}

/// Cross the layout/core boundary for one exact host-selected font.
///
/// Keeping this projection in one place makes the Lisp font object, frame
/// geometry, glyph lookup, and OTF capability describe the same realization.
pub fn core_opened_font_from_selection(
    font: SelectedFontInfo,
    mut capability_for_file: impl FnMut(&str, u32) -> Option<FontOtfCapability>,
) -> ResolvedOpenedFont {
    let identity = &font.resolved.identity;
    let capability = identity
        .file_path
        .as_deref()
        .and_then(|file| capability_for_file(file, identity.file_face_index()))
        .or_else(|| {
            font.resolved
                .replay
                .outline_asset()
                .and_then(font_otf_capability_for_asset)
        });
    ResolvedOpenedFont {
        resolved: font.resolved,
        foundry: font.foundry.as_deref().map(LispString::from_utf8),
        slant: font.slant,
        metrics: core_font_px_metrics(font.metrics),
        capability,
    }
}
