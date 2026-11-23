//! One fallback for a line when two pinned faces both cover every character.
//!
//! GNU `fontset_find_font` reuses an opened font that still covers the next
//! character before it lists again. This row drives the layout entry
//! (`font_request_for_char`) through the native fontconfig catalog. It asserts
//! that the characters share one realized font, not which of the covering
//! faces that font is.

use super::*;
use crate::font::metrics::test_catalog::TestFontCatalog;
use crate::font_backend::{
    FontBackend, FontCandidateQuery, FontCandidateScope, RequiredFontCoverage, TextDirection,
};
use std::collections::BTreeSet;

const LINE: [char; 3] = ['検', 'リ', 'ピ'];

fn coverage_query(ch: char) -> FontCandidateQuery {
    FontCandidateQuery {
        scope: FontCandidateScope::All,
        required: RequiredFontCoverage::Character(ch),
        charset_ranges: Vec::new(),
        languages: Vec::new(),
        requested_weight: 400,
        requested_slant: FontSlant::Normal,
        requested_width: FontWidth::Normal,
        direction: TextDirection::LeftToRight,
    }
}

/// Largest set of covering families that share one weight and spacing.
///
/// The variable CJK collection reports weight 100 on every locale face, while
/// LXGW WenKai reports 400. A request at the tied weight makes those locale
/// faces equal scores; a 400 request would leave WenKai as the unique winner.
fn largest_tie(families: &BTreeSet<(String, u16, i32)>) -> BTreeSet<(String, u16, i32)> {
    let mut best = BTreeSet::new();
    let keys: BTreeSet<(u16, i32)> = families
        .iter()
        .map(|(_, weight, spacing)| (*weight, *spacing))
        .collect();
    for key in keys {
        let group: BTreeSet<_> = families
            .iter()
            .filter(|(_, weight, spacing)| (*weight, *spacing) == key)
            .cloned()
            .collect();
        if group.len() > best.len() {
            best = group;
        }
    }
    best
}

/// Families other than the requested face that cover `ch`, keyed so equal
/// weight and spacing can be seen together.
fn covering_families(catalog: &TestFontCatalog, ch: char) -> BTreeSet<(String, u16, i32)> {
    catalog
        .list_candidates(&coverage_query(ch))
        .into_iter()
        .filter_map(|candidate| {
            let metadata = &candidate.matched.metadata;
            if metadata.family.eq_ignore_ascii_case("DejaVu Sans") {
                return None;
            }
            Some((
                metadata.family.clone(),
                metadata.weight.unwrap_or(0),
                metadata.spacing.unwrap_or(0),
            ))
        })
        .collect()
}

#[cfg(target_os = "linux")]
#[test]
fn layout_line_uses_one_pinned_fallback_when_two_faces_cover_it() {
    let dejavu = neomacs_test_fonts::dejavu_2_37();
    let portable = TestFontCatalog::from_files(&[
        dejavu.sans_regular(),
        neomacs_test_fonts::noto_sans_cjk_variable(),
        neomacs_test_fonts::lxgw_wenkai_nerd_regular_1_522(),
    ])
    .expect("pinned DejaVu, Noto Sans CJK, and LXGW WenKai");

    let mut shared: Option<BTreeSet<(String, u16, i32)>> = None;
    for ch in LINE {
        let tied = largest_tie(&covering_families(&portable, ch));
        assert!(
            tied.len() >= 2,
            "{ch} needs two fallbacks with the same weight and spacing, got {tied:?}"
        );
        shared = Some(match shared {
            None => tied,
            Some(previous) => previous.intersection(&tied).cloned().collect(),
        });
    }
    let shared = shared.expect("the line has characters");
    assert!(
        shared.len() >= 2,
        "two equal-score fallbacks must cover every character of the line, got {shared:?}"
    );
    let requested_weight = shared
        .iter()
        .next()
        .map(|(_, weight, _)| *weight)
        .expect("the tied group names a weight");

    let mut svc = portable.to_native_fontconfig().metrics_service();
    let mut identities = Vec::new();
    for ch in LINE {
        let resolved = svc.font_request_for_char(
            ch,
            RealizedFaceFontSelection::new("DejaVu Sans", requested_weight, false, 16.0),
        );
        let platform = resolved
            .platform
            .unwrap_or_else(|| panic!("{ch} resolved to a pinned fallback"));
        assert!(
            !platform.family().eq_ignore_ascii_case("DejaVu Sans"),
            "{ch} stayed on the face that does not cover it"
        );
        identities.push(platform.identity.clone());
    }
    assert!(
        identities.windows(2).all(|pair| pair[0] == pair[1]),
        "every character of the line uses one fallback, got {identities:?}"
    );
}
