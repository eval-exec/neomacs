//! One fallback for a line when two pinned faces both cover every character.
//!
//! GNU `fontset_find_font` reuses an opened font that still covers the next
//! character before it lists again. This row drives the layout entry
//! (`font_request_for_char`) through the native fontconfig catalog. No fontset
//! is installed, so the generic fallback group is one group, and the line
//! shares one realized font. The row does not say which covering face that is.

use super::*;
use crate::font::metrics::test_catalog::TestFontCatalog;
use crate::font_backend::{
    FontBackend, FontCandidateQuery, FontCandidateScope, RequiredFontCoverage, TextDirection,
};
use neovm_core::window::{DisplayFrameExt, DisplayFrameManagerExt};
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

/// Pinned DejaVu + Noto CJK + LXGW, and the equal-score fallbacks that cover the line.
fn pinned_tie() -> (TestFontCatalog, BTreeSet<(String, u16, i32)>, u16) {
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
    (portable, shared, requested_weight)
}

#[cfg(target_os = "linux")]
#[test]
fn fc_font_sort_is_reused_until_the_catalog_changes() {
    let (portable, _, _) = pinned_tie();
    let native = portable.to_native_fontconfig();
    let owner =
        || crate::font::fontconfig::test_config::QueryOwner::Fixture(native.native_fontconfig());
    crate::font::fontconfig::reset_sort_queries_for_test();
    let first = crate::font::fontconfig::fc_sorted_families(owner(), "DejaVu Sans", "ja");
    assert!(
        !first.is_empty(),
        "FcFontSort returned no families for DejaVu Sans:lang=ja"
    );
    let after_first = crate::font::fontconfig::sort_query_count();
    let second = crate::font::fontconfig::fc_sorted_families(owner(), "DejaVu Sans", "ja");
    assert_eq!(second, first);
    assert_eq!(
        crate::font::fontconfig::sort_query_count(),
        after_first,
        "the same family and language must not sort the catalog again"
    );
    crate::font::fontconfig::invalidate_catalog_caches(owner());
    let _third = crate::font::fontconfig::fc_sorted_families(owner(), "DejaVu Sans", "ja");
    assert!(
        crate::font::fontconfig::sort_query_count() > after_first,
        "a catalog epoch change sorts again"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn layout_line_uses_one_pinned_fallback_when_two_faces_cover_it() {
    let (portable, _, requested_weight) = pinned_tie();
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
    assert_eq!(
        identities[0], identities[1],
        "the line shares one opened fallback, got {identities:?}"
    );
    assert_eq!(
        identities[1], identities[2],
        "the line shares one opened fallback, got {identities:?}"
    );
}

/// Equal scores take `FcFontSort` order for the default face and `lang=ja`.
///
/// The ranking unit test feeds synthetic ranks. This row asks the same pinned
/// catalog the layout entry uses, and checks the realized family against the
/// sort of that catalog.
#[cfg(target_os = "linux")]
#[test]
fn layout_line_picks_the_earliest_fc_sorted_pinned_fallback() {
    let (portable, _, requested_weight) = pinned_tie();
    let native = portable.to_native_fontconfig();
    let owner =
        || crate::font::fontconfig::test_config::QueryOwner::Fixture(native.native_fontconfig());
    let codepoint = u32::from('リ');
    let listed = crate::font::fontconfig::fc_list_candidates(
        owner(),
        None,
        &[(codepoint, codepoint)],
        Some(codepoint),
        &[],
    );
    // Spacing is not scored when the font spec leaves it unset, so every
    // normal face at the requested weight is one equal-score set.
    let mut tied = BTreeSet::new();
    for candidate in &listed {
        let family = candidate.matched.family.as_str();
        if family.eq_ignore_ascii_case("DejaVu Sans") {
            continue;
        }
        if candidate.matched.weight != Some(requested_weight)
            || candidate.matched.slant != FontSlant::Normal
            || candidate
                .width
                .is_some_and(|width| width != FontWidth::Normal)
        {
            continue;
        }
        tied.insert(family.to_owned());
    }
    assert!(
        tied.len() >= 2,
        "リ needs two equal-score fontconfig faces at weight {requested_weight}, got {tied:?}"
    );

    let sorted = crate::font::fontconfig::fc_sorted_families(owner(), "DejaVu Sans", "ja");
    assert!(
        !sorted.is_empty(),
        "FcFontSort returned no families for DejaVu Sans:lang=ja"
    );
    let mut best: Option<(usize, String)> = None;
    for family in &tied {
        let Some(index) = sorted
            .iter()
            .position(|name| name.eq_ignore_ascii_case(family))
        else {
            continue;
        };
        if best
            .as_ref()
            .is_none_or(|(best_index, _)| index < *best_index)
        {
            best = Some((index, family.clone()));
        }
    }
    let (index, expected) =
        best.expect("a tied covering family must appear in the FcFontSort list");

    let mut svc = native.metrics_service();
    let resolved = svc.font_request_for_char(
        'リ',
        RealizedFaceFontSelection::new("DejaVu Sans", requested_weight, false, 16.0),
    );
    let chosen = resolved
        .platform
        .unwrap_or_else(|| panic!("リ resolved to a pinned fallback"))
        .family()
        .to_owned();
    assert!(
        chosen.eq_ignore_ascii_case(&expected),
        "equal-score fallback should be the earliest FcFontSort family {expected} at {index}, got {chosen}; sort={sorted:?}; tied={tied:?}"
    );
}

/// Families at `requested_weight` that cover `ch`, excluding the face family.
#[cfg(target_os = "linux")]
fn tied_covering_families(
    native: &TestFontCatalog,
    ch: char,
    requested_weight: u16,
) -> BTreeSet<String> {
    let codepoint = u32::from(ch);
    let owner =
        || crate::font::fontconfig::test_config::QueryOwner::Fixture(native.native_fontconfig());
    let listed = crate::font::fontconfig::fc_list_candidates(
        owner(),
        None,
        &[(codepoint, codepoint)],
        Some(codepoint),
        &[],
    );
    let mut tied = BTreeSet::new();
    for candidate in &listed {
        let family = candidate.matched.family.as_str();
        if family.eq_ignore_ascii_case("DejaVu Sans") {
            continue;
        }
        if candidate.matched.weight != Some(requested_weight)
            || candidate.matched.slant != FontSlant::Normal
            || candidate
                .width
                .is_some_and(|width| width != FontWidth::Normal)
        {
            continue;
        }
        tied.insert(family.to_owned());
    }
    tied
}

#[cfg(target_os = "linux")]
fn earliest_sorted_family(
    native: &TestFontCatalog,
    families: &BTreeSet<String>,
    lang: &str,
) -> String {
    let owner =
        || crate::font::fontconfig::test_config::QueryOwner::Fixture(native.native_fontconfig());
    let sorted = crate::font::fontconfig::fc_sorted_families(owner(), "DejaVu Sans", lang);
    assert!(
        !sorted.is_empty(),
        "FcFontSort returned no families for lang={lang}"
    );
    families
        .iter()
        .filter_map(|family| {
            sorted
                .iter()
                .position(|name| name.eq_ignore_ascii_case(family))
                .map(|index| (index, family.clone()))
        })
        .min_by_key(|(index, _)| *index)
        .map(|(_, family)| family)
        .unwrap_or_else(|| {
            panic!("a tied covering family must appear in the {lang} sort: {sorted:?}")
        })
}

/// The default fontset's first Han spec is GB2312 (`zh-cn`) and its first
/// kana spec is JISX0208 (`ja`) (`fontset.el`). This row installs those
/// registries and checks the realized family against that language's sort.
#[cfg(target_os = "linux")]
#[test]
fn layout_line_resolves_han_and_kana_through_their_default_fontset_registries() {
    let (portable, _, requested_weight) = pinned_tie();
    let native = portable.to_native_fontconfig();
    let mut eval = neovm_core::emacs_core::eval_hook_installers::new_context();
    eval.eval_str(
        r#"(let ((font-encoding-alist '((".*" . unicode))))
            (set-fontset-font t ?検 '(nil . "gb2312.1980-0"))
            (set-fontset-font t ?検 '(nil . "jisx0208*") nil 'append)
            (set-fontset-font t ?リ '(nil . "jisx0208*"))
            (set-fontset-font t ?ピ '(nil . "jisx0208*")))"#,
    )
    .expect("han and kana registry specs");

    let mut svc = native.metrics_service();
    let resolve = |svc: &mut FontMetricsService, ch: char| {
        svc.font_request_for_char(
            ch,
            RealizedFaceFontSelection::new("DejaVu Sans", requested_weight, false, 16.0),
        )
        .platform
        .unwrap_or_else(|| panic!("{ch} resolved to a pinned fallback"))
        .family()
        .to_owned()
    };
    let han = resolve(&mut svc, '検');
    let kana = resolve(&mut svc, 'リ');
    let kana_again = resolve(&mut svc, 'ピ');
    let han_expected = earliest_sorted_family(
        &native,
        &tied_covering_families(&native, '検', requested_weight),
        "zh-cn",
    );
    let kana_expected = earliest_sorted_family(
        &native,
        &tied_covering_families(&native, 'リ', requested_weight),
        "ja",
    );
    assert!(
        han.eq_ignore_ascii_case(&han_expected),
        "検 should be the earliest zh-cn family {han_expected}, got {han}"
    );
    assert!(
        kana.eq_ignore_ascii_case(&kana_expected),
        "リ should be the earliest ja family {kana_expected}, got {kana}"
    );
    assert!(
        kana_again.eq_ignore_ascii_case(&kana),
        "ピ reuses the kana fallback {kana}, got {kana_again}"
    );
}

/// A second redisplay of the same frame must not list or sort again.
#[cfg(target_os = "linux")]
#[test]
fn second_layout_pass_makes_no_new_listing_or_font_sort() {
    let (portable, _, _) = pinned_tie();
    let mut engine = crate::LayoutEngine::new_without_font_metrics();
    engine.font_metrics = Some(portable.to_native_fontconfig().metrics_service());

    let mut eval = neovm_core::emacs_core::eval_hook_installers::new_context();
    let buffer_id = eval
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("fallback-pass", 640, 160, buffer_id);
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        frame.set_window_system(Some(neovm_core::emacs_core::value::Value::symbol("neo")));
        frame.install_gnu_gui_default_parameters();
    }
    assert!(eval.frame_manager_mut().select_frame(frame_id));
    let minibuffer = eval
        .buffer_manager_mut()
        .find_buffer_by_name(" *Minibuf-0*")
        .unwrap_or_else(|| eval.buffer_manager_mut().create_buffer(" *Minibuf-0*"));
    if let Some(frame) = eval.frame_manager_mut().get_mut(frame_id)
        && let Some(leaf) = frame.minibuffer_leaf.as_mut()
    {
        leaf.set_buffer(minibuffer);
    }
    eval.eval_str(
        r#"(progn
            (internal-set-lisp-face-attribute 'default :family "DejaVu Sans" (selected-frame))
            (internal-set-lisp-face-attribute 'default :height 110 (selected-frame))
            (insert "リピ"))"#,
    )
    .expect("kana line");

    crate::font::fontconfig::reset_list_queries_for_test();
    crate::font::fontconfig::reset_sort_queries_for_test();
    engine.layout_frame_rust(&mut eval, frame_id);
    let lists = crate::font::fontconfig::list_query_count();
    let sorts = crate::font::fontconfig::sort_query_count();
    assert!(lists > 0, "the first pass must list candidates");
    assert!(sorts > 0, "the first pass must sort a fallback");
    engine.layout_frame_rust(&mut eval, frame_id);
    assert_eq!(
        crate::font::fontconfig::list_query_count(),
        lists,
        "the second pass must not list candidates again"
    );
    assert_eq!(
        crate::font::fontconfig::sort_query_count(),
        sorts,
        "the second pass must not call FcFontSort again"
    );
}
