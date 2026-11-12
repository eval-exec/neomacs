//! Private admission checks for GNU ftfont_list's HarfBuzz OTF constraint.
use crate::font_backend::{NativeFontOtfFeature, NativeFontOtfRequirement};
use std::io::Read;
use ttf_parser::opentype_layout::{LanguageSystem, LayoutTable};

#[cfg(test)]
thread_local! {
    pub(super) static NATIVE_FILE_CHECKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

pub(super) fn capability_matches(capability: &[u8], script: [u8; 4]) -> bool {
    // GNU uses strstr, not token equality. Its C tag string ends at the first
    // zero byte; retain that behavior without interpreting arbitrary tag bytes.
    let mut needle = *b"otlayout:\0\0\0\0";
    needle[9..].copy_from_slice(&script);
    let length = needle
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(needle.len());
    capability
        .windows(length)
        .any(|window| window == &needle[..length])
}

pub(super) fn matches_file(file: &str, requirement: &NativeFontOtfRequirement) -> bool {
    #[cfg(test)]
    NATIVE_FILE_CHECKS.with(|count| count.set(count.get() + 1));
    let Ok(mut source) = std::fs::File::open(file) else {
        return false;
    };
    let mut magic = [0; 4];
    if source.read_exact(&mut magic).is_err() {
        return false;
    }
    if magic == *b"wOFF" || magic == *b"wOF2" {
        // GNU's FreeType face opener decodes these containers. Reuse the
        // materializer's decoder and exact source-face selection instead of
        // passing container bytes to ttf-parser. The temporary database owns
        // the standalone SFNT only for this query, not for the cache lifetime.
        drop(source);
        let mut database = fontdb::Database::new();
        let Ok(faces) = neomacs_font_materializer::FontFileCache::open_file(&mut database, file, 0)
        else {
            return false;
        };
        return faces.into_iter().any(|face| {
            database
                .with_face_data(face, |bytes, _| matches_bytes(bytes, requirement))
                .unwrap_or(false)
        });
    }
    let mut data = magic.to_vec();
    if source.read_to_end(&mut data).is_err() {
        return false;
    }
    matches_bytes(&data, requirement)
}

fn matches_bytes(data: &[u8], requirement: &NativeFontOtfRequirement) -> bool {
    // GNU hbotf_open (ftfont.c) opens file face zero even when the native
    // candidate is another collection face or a named variable instance.
    // This is only the listing gate; the selected candidate identity is intact.
    let Ok(face) = ttf_parser::Face::parse(data, 0) else {
        return false;
    };
    let tables = face.tables();
    table_matches(tables.gsub, requirement, &requirement.gsub)
        && table_matches(tables.gpos, requirement, &requirement.gpos)
}

fn table_matches(
    table: Option<LayoutTable<'_>>,
    requirement: &NativeFontOtfRequirement,
    features: &[NativeFontOtfFeature],
) -> bool {
    let Some(table) = table else {
        return false;
    };
    let Some(script) = table
        .scripts
        .find(ttf_parser::Tag::from_bytes(&requirement.script))
    else {
        return false;
    };
    let language: Option<LanguageSystem<'_>> = match requirement.language {
        None | Some([0, 0, 0, 0]) => script.default_language,
        Some(tag) => {
            // The pinned GNU HarfBuzz adapter bounds this search by the SCRIPT
            // count, then compares against the LANGUAGE count. Preserve its
            // observable index behavior using checked access, never its C
            // out-of-bounds reads. An unavailable index has no feature tags.
            let index = (0..table.scripts.len())
                .find(|&index| {
                    script
                        .languages
                        .get(index)
                        .is_some_and(|language| language.tag.to_bytes() == tag)
                })
                .unwrap_or(table.scripts.len());
            if index == script.languages.len() {
                return false;
            }
            script.languages.get(index)
        }
    };
    features.iter().all(|feature| {
        let (tag, forbidden) = match feature {
            NativeFontOtfFeature::Required(tag) => (tag, false),
            NativeFontOtfFeature::Forbidden(tag) => (tag, true),
        };
        if *tag == [0; 4] {
            return true;
        }
        // hb_ot_layout_language_find_feature checks the ordinary feature
        // indices, not the separate LangSys required_feature index.
        let present = language.is_some_and(|language| {
            language
                .feature_indices
                .into_iter()
                .filter_map(|index| table.features.get(index))
                .any(|feature| feature.tag.to_bytes() == *tag)
        });
        present != forbidden
    })
}

#[cfg(test)]
#[path = "tests/otf_requirement_parser_test.rs"]
mod tests;
