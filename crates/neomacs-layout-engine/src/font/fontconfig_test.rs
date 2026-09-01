use super::{
    FONT_SPACING_MONO, FONT_SPACING_PROPORTIONAL, FcCharSetGuard, FcFontSetGuard, FcLangSetGuard,
    FcPatternGuard, GnuEntityProjection, ListedFont, SpacingClass, build_candidate_object_set,
    candidate_score, family_affinity_score, family_search_order, fc_list_candidates,
    fontconfig_handle, listed_font_from_raw_pattern, normalize_spacing, parse_fontconfig_weight,
    select_find_font_candidate, spacing_score, style_weight,
};
use crate::font::policy::{
    combined_query_langs, query_charset_ranges, registry_hint, registry_query_chars,
    representative_char_for_spec, wildcard_casefold_match,
};
use crate::font::sizing::{fallback_frame_res_y, points_to_layout_pixels};
use crate::font_backend::PlatformFontSize;
use neovm_core::emacs_core::fontset::{FontRepertory, StoredFontSpec};
use neovm_core::emacs_core::intern::{intern, resolve_sym};
use neovm_core::face::{FontSlant, FontWeight, FontWidth};
#[cfg(unix)]
use std::ffi::CString;
#[cfg(unix)]
use std::ptr;

fn font_sym(name: &str) -> neovm_core::emacs_core::SymId {
    intern(name)
}

#[test]
fn registry_hint_matches_wildcard_patterns() {
    let hint = registry_hint("JISX0208*").expect("jisx0208 wildcard");
    assert_eq!(hint.lang, Some("ja"));
    assert_eq!(hint.uniquifiers, &[0x4E55]);
}

#[test]
fn registry_hint_matches_case_insensitively() {
    let hint = registry_hint("GB2312.1980-0").expect("gb2312");
    assert_eq!(hint.lang, Some("zh-cn"));
    assert_eq!(hint.uniquifiers, &[0x4E13]);
}

#[test]
fn registry_query_chars_use_gnu_uniquifiers() {
    assert_eq!(
        registry_query_chars(Some("gb2312.1980-0"), '好'),
        vec![0x4E13]
    );
    assert_eq!(
        registry_query_chars(Some("cns11643.1992-2"), '好'),
        vec![0x4E33, 0x7934]
    );
    assert_eq!(registry_query_chars(None, '好'), vec!['好' as u32]);
}

#[test]
fn query_charset_ranges_follow_gnu_registry_uniquifiers_when_registry_is_present() {
    let spec = StoredFontSpec {
        family: None,
        registry: Some(font_sym("gb2312.1980-0")),
        lang: None,
        weight: None,
        slant: None,
        width: None,
        repertory: Some(FontRepertory::CharTableRanges(vec![(0x4E00, 0x9FFF)])),
    };

    assert_eq!(
        query_charset_ranges(&spec, '好'),
        vec![(0x4E13, 0x4E13), ('好' as u32, '好' as u32)]
    );
}

#[test]
fn query_charset_ranges_use_repertory_when_registry_is_absent() {
    let spec = StoredFontSpec {
        family: None,
        registry: None,
        lang: None,
        weight: None,
        slant: None,
        width: None,
        repertory: Some(FontRepertory::CharTableRanges(vec![(0x4E00, 0x9FFF)])),
    };

    assert_eq!(query_charset_ranges(&spec, '好'), vec![(0x4E00, 0x9FFF)]);
}

#[test]
fn query_charset_ranges_skip_generic_gnu_registries() {
    for registry in ["ascii-0", "iso10646-1", "unicode-bmp"] {
        let spec = StoredFontSpec {
            family: None,
            registry: Some(font_sym(registry)),
            lang: None,
            weight: None,
            slant: None,
            width: None,
            repertory: None,
        };
        assert!(query_charset_ranges(&spec, '好').is_empty(), "{registry}");
    }
}

#[cfg(unix)]
fn gb2312_registry_pattern() -> (
    FcPatternGuard,
    FcCharSetGuard,
    FcLangSetGuard,
    Vec<String>,
    Vec<(u32, u32)>,
) {
    let spec = StoredFontSpec {
        family: None,
        registry: Some(font_sym("gb2312.1980-0")),
        lang: None,
        weight: None,
        slant: None,
        width: None,
        repertory: None,
    };
    let representative = representative_char_for_spec(&spec);
    let ranges = query_charset_ranges(&spec, representative);
    let langs = combined_query_langs(
        spec.registry
            .map(resolve_sym)
            .and_then(crate::font::policy::registry_language),
        spec.lang.map(resolve_sym),
    );

    let _ = fontconfig_handle().expect("fontconfig");
    let pattern = unsafe { fontconfig_sys::FcPatternCreate() };
    assert!(!pattern.is_null());
    let pattern = FcPatternGuard(pattern);
    let charset = unsafe { fontconfig_sys::FcCharSetCreate() };
    assert!(!charset.is_null());
    let charset = FcCharSetGuard(charset);
    let langset = unsafe { fontconfig_sys::FcLangSetCreate() };
    assert!(!langset.is_null());
    let langset = FcLangSetGuard(langset);
    assert_ne!(
        unsafe { fontconfig_sys::FcCharSetAddChar(charset.0, 0x4E13) },
        0
    );
    let lang = CString::new("zh-cn").unwrap();
    assert_ne!(
        unsafe { fontconfig_sys::FcLangSetAdd(langset.0, lang.as_ptr().cast()) },
        0
    );
    assert_ne!(
        unsafe {
            fontconfig_sys::FcPatternAddCharSet(
                pattern.0,
                fontconfig::FC_CHARSET.as_ptr(),
                charset.0,
            )
        },
        0
    );
    assert_ne!(
        unsafe {
            fontconfig_sys::FcPatternAddLangSet(pattern.0, fontconfig::FC_LANG.as_ptr(), langset.0)
        },
        0
    );
    (pattern, charset, langset, langs, ranges)
}

#[cfg(unix)]
#[test]
fn registry_charset_queries_keep_gnu_fontconfig_candidate_order() {
    let (pattern, _charset, _langset, langs, ranges) = gb2312_registry_pattern();
    let object_set =
        build_candidate_object_set(GnuEntityProjection::Metadata).expect("gnu object set");
    let fontset = unsafe { fontconfig_sys::FcFontList(ptr::null_mut(), pattern.0, object_set.0) };
    assert!(!fontset.is_null());
    let fontset = FcFontSetGuard(fontset);
    let fonts =
        unsafe { std::slice::from_raw_parts((*fontset.0).fonts, (*fontset.0).nfont as usize) };
    let expected: Vec<(String, String)> = fonts
        .iter()
        .take(8)
        .filter_map(|pattern| listed_font_from_raw_pattern(*pattern))
        .map(|candidate| (candidate.matched.family, candidate.style))
        .collect();
    assert!(
        !expected.is_empty(),
        "raw GNU-shaped fontconfig query should yield candidates"
    );

    let actual: Vec<(String, String)> = fc_list_candidates(None, &ranges, None, &langs)
        .into_iter()
        .take(expected.len())
        .map(|candidate| (candidate.matched.family, candidate.style))
        .collect();

    assert_eq!(actual, expected);
}

#[test]
fn spacing_score_is_neutral_without_requested_spacing() {
    assert_eq!(
        spacing_score(None, Some(FONT_SPACING_PROPORTIONAL), true),
        0
    );
    assert_eq!(spacing_score(None, Some(FONT_SPACING_MONO), true), 0);
}

#[test]
fn registry_and_spec_langs_are_deduplicated() {
    assert_eq!(
        combined_query_langs(Some("zh-cn"), Some("zh-cn")),
        vec!["zh-cn"]
    );
    assert_eq!(
        combined_query_langs(Some("zh-cn"), Some("zh")),
        vec!["zh-cn", "zh"]
    );
}

#[test]
fn wildcard_match_handles_star_and_question() {
    assert!(wildcard_casefold_match("jisx0208*", "jisx0208.1983-0"));
    assert!(wildcard_casefold_match("gb?-0", "gbk-0"));
    assert!(!wildcard_casefold_match("big5-0", "gbk-0"));
}

#[test]
fn registry_only_fontset_specs_try_requested_family_before_unspecified_fallback() {
    let spec = StoredFontSpec {
        family: None,
        registry: Some(font_sym("gb2312.1980-0")),
        lang: None,
        weight: None,
        slant: None,
        width: None,
        repertory: None,
    };
    let order = family_search_order("monospace", &spec);
    assert!(matches!(order.first(), Some(Some(_))));
    assert!(matches!(order.last(), Some(None)));
}

#[test]
fn constrained_fontset_specs_without_family_try_requested_family_first() {
    let spec = StoredFontSpec {
        family: None,
        registry: None,
        lang: Some(font_sym("zh-cn")),
        weight: Some(FontWeight::SEMI_BOLD),
        slant: None,
        width: None,
        repertory: None,
    };
    let order = family_search_order("monospace", &spec);
    assert!(matches!(order.first(), Some(Some(_))));
    assert!(matches!(order.last(), Some(None)));
}

#[test]
fn parse_fontconfig_weight_maps_known_ranges() {
    assert_eq!(parse_fontconfig_weight("0"), Some(100));
    assert_eq!(parse_fontconfig_weight("40"), Some(200));
    assert_eq!(parse_fontconfig_weight("50"), Some(300));
    assert_eq!(parse_fontconfig_weight("80"), Some(400));
    assert_eq!(parse_fontconfig_weight("100"), Some(500));
    assert_eq!(parse_fontconfig_weight("180"), Some(600));
    assert_eq!(parse_fontconfig_weight("200"), Some(700));
    assert_eq!(parse_fontconfig_weight("205"), Some(800));
    assert_eq!(parse_fontconfig_weight("210"), Some(900));
    assert_eq!(parse_fontconfig_weight("[80 200]"), None);
}

#[test]
fn points_to_pixels_rounds_like_gnu_point_to_pixel() {
    assert_eq!(points_to_layout_pixels(10.0, 100.0), 14.0);
    assert_eq!(points_to_layout_pixels(12.0, 100.0), 17.0);
    assert_eq!(points_to_layout_pixels(16.0, 100.0), 22.0);
    // 22pt @ 100dpi: GNU PT_PER_INCH=72.27 gives 30, not the 31 that a
    // 72.0 divisor would (the font-selection oracle's h220 case).
    assert_eq!(points_to_layout_pixels(22.0, 100.0), 30.0);
}

#[test]
fn frame_res_fallback_uses_display_height_and_mm() {
    let dpi = fallback_frame_res_y(1080, 274);
    assert!((dpi - 100.14).abs() < 0.1);
    assert_eq!(fallback_frame_res_y(1080, 0), 100.0);
}

#[test]
fn style_weight_prefers_semibold_over_regular_alias() {
    assert_eq!(style_weight("SemiBold,Regular"), Some(600));
    assert_eq!(style_weight("SemiBold Italic,Italic"), Some(600));
}

#[test]
fn font_variations_parse_as_exact_opentype_coordinates() {
    let coords = super::parse_font_variations("wght=650,wdth=90.5,broken,toolong=1");

    assert_eq!(coords.len(), 2);
    assert_eq!(coords[0].tag(), u32::from_be_bytes(*b"wght"));
    assert_eq!(coords[0].value(), 650.0);
    assert_eq!(coords[1].tag(), u32::from_be_bytes(*b"wdth"));
    assert_eq!(coords[1].value(), 90.5);
}

#[test]
fn spacing_categories_follow_gnu_numeric_ranges() {
    assert_eq!(normalize_spacing(0), Some(SpacingClass::Proportional));
    assert_eq!(normalize_spacing(90), Some(SpacingClass::Dual));
    assert_eq!(normalize_spacing(100), Some(SpacingClass::Mono));
    assert_eq!(normalize_spacing(110), Some(SpacingClass::Charcell));
}

#[test]
fn monospace_preference_penalizes_proportional_candidates() {
    let mono_candidate = ListedFont {
        foundry: None,
        matched: super::FontMatch {
            family: "Noto Sans Mono CJK SC".to_string(),
            file: None,
            face_index: 0,
            variation_coords: Vec::new(),
            postscript_name: None,
            weight: Some(700),
            slant: FontSlant::Normal,
            size: PlatformFontSize::Scalable,
        },
        style: "Bold".to_string(),
        weight_css: Some(700),
        width: Some(FontWidth::Normal),
        spacing: Some(FONT_SPACING_MONO),
    };
    let proportional_candidate = ListedFont {
        foundry: None,
        matched: super::FontMatch {
            family: "Noto Sans CJK SC".to_string(),
            file: None,
            face_index: 0,
            variation_coords: Vec::new(),
            postscript_name: None,
            weight: Some(700),
            slant: FontSlant::Normal,
            size: PlatformFontSize::Scalable,
        },
        style: "Bold".to_string(),
        weight_css: Some(700),
        width: Some(FontWidth::Normal),
        spacing: Some(FONT_SPACING_PROPORTIONAL),
    };

    let mono_score = candidate_score(
        &mono_candidate,
        800,
        false,
        None,
        Some(FONT_SPACING_MONO),
        true,
        Some("Noto Sans Mono"),
    );
    let proportional_score = candidate_score(
        &proportional_candidate,
        800,
        false,
        None,
        Some(FONT_SPACING_MONO),
        true,
        Some("Noto Sans Mono"),
    );

    assert!(
        mono_score < proportional_score,
        "expected mono candidate to outrank proportional candidate: mono={mono_score:?} proportional={proportional_score:?}"
    );
}

#[cfg(unix)]
#[test]
fn family_affinity_prefers_requested_family_over_unrelated_fixed_pitch_fallback() {
    let requested_family_candidate = ListedFont {
        foundry: None,
        matched: super::FontMatch {
            family: "Noto Sans Mono CJK SC".to_string(),
            file: None,
            face_index: 0,
            variation_coords: Vec::new(),
            postscript_name: None,
            weight: Some(400),
            slant: FontSlant::Normal,
            size: PlatformFontSize::Scalable,
        },
        style: "Regular".to_string(),
        weight_css: Some(400),
        width: Some(FontWidth::Normal),
        spacing: None,
    };
    let unrelated_fixed_candidate = ListedFont {
        foundry: None,
        matched: super::FontMatch {
            family: "Sarasa Fixed Slab SC".to_string(),
            file: None,
            face_index: 0,
            variation_coords: Vec::new(),
            postscript_name: None,
            weight: Some(400),
            slant: FontSlant::Normal,
            size: PlatformFontSize::Scalable,
        },
        style: "Regular".to_string(),
        weight_css: Some(400),
        width: Some(FontWidth::Normal),
        spacing: Some(90),
    };

    let requested_score = candidate_score(
        &requested_family_candidate,
        400,
        false,
        None,
        None,
        true,
        Some("Noto Sans Mono"),
    );
    let fallback_score = candidate_score(
        &unrelated_fixed_candidate,
        400,
        false,
        None,
        None,
        true,
        Some("Noto Sans Mono"),
    );

    assert!(
        requested_score < fallback_score,
        "expected requested-family CJK companion to outrank unrelated fixed fallback: requested={requested_score:?} fallback={fallback_score:?}"
    );
}

#[test]
fn family_affinity_treats_cjk_companion_as_close_match() {
    assert_eq!(
        family_affinity_score(Some("Noto Sans Mono"), "Noto Sans Mono CJK SC"),
        5
    );
    assert_eq!(
        family_affinity_score(Some("Noto Sans Mono"), "Sarasa Fixed SC"),
        80
    );
}

#[test]
fn find_font_candidate_prefers_the_requested_family_over_an_earlier_fallback() {
    let candidate = |family: &str, file: &str| ListedFont {
        foundry: None,
        matched: super::FontMatch {
            family: family.to_string(),
            file: Some(file.to_string()),
            face_index: 0,
            variation_coords: Vec::new(),
            postscript_name: None,
            weight: Some(400),
            slant: FontSlant::Normal,
            size: PlatformFontSize::Scalable,
        },
        style: "Regular".to_string(),
        weight_css: Some(400),
        width: Some(FontWidth::Normal),
        spacing: None,
    };
    let spec = StoredFontSpec {
        family: Some(font_sym("Symbols Nerd Font Mono")),
        registry: None,
        lang: None,
        weight: Some(FontWeight::Normal),
        slant: Some(FontSlant::Normal),
        width: None,
        repertory: None,
    };

    let selected = select_find_font_candidate(
        vec![
            candidate("Noto Sans", "noto.ttf"),
            candidate("Symbols Nerd Font Mono", "symbols.ttf"),
        ],
        &spec,
    )
    .expect("matching candidate");

    assert_eq!(selected.matched.file.as_deref(), Some("symbols.ttf"));
}

#[test]
fn find_font_candidate_prefers_requested_width_over_candidate_order() {
    let candidate = |file: &str, width: FontWidth| ListedFont {
        foundry: None,
        matched: super::FontMatch {
            family: "Iosevka".to_string(),
            file: Some(file.to_string()),
            face_index: 0,
            variation_coords: Vec::new(),
            postscript_name: None,
            weight: Some(700),
            slant: FontSlant::Normal,
            size: PlatformFontSize::Scalable,
        },
        style: "Bold".to_string(),
        weight_css: Some(700),
        width: Some(width),
        spacing: None,
    };
    let spec = StoredFontSpec {
        family: Some(font_sym("Iosevka")),
        registry: None,
        lang: None,
        weight: Some(FontWeight::Bold),
        slant: Some(FontSlant::Normal),
        width: Some(FontWidth::Normal),
        repertory: None,
    };

    let selected = select_find_font_candidate(
        vec![
            candidate("Iosevka-ExtendedBold.ttf", FontWidth::Expanded),
            candidate("Iosevka-Bold.ttf", FontWidth::Normal),
        ],
        &spec,
    )
    .expect("matching candidate");

    assert_eq!(selected.matched.file.as_deref(), Some("Iosevka-Bold.ttf"));
}

#[test]
fn best_candidate_for_pass_preserves_each_equal_score_entity_ordinal() {
    let candidates = vec![
        ListedFont {
            foundry: None,
            matched: super::FontMatch {
                family: "Noto Sans Mono CJK SC".to_string(),
                file: Some("mono.ttc".to_string()),
                face_index: 0,
                variation_coords: Vec::new(),
                postscript_name: Some("Mono-Medium".to_string()),
                weight: Some(500),
                slant: FontSlant::Normal,
                size: PlatformFontSize::Scalable,
            },
            style: "Medium".to_string(),
            weight_css: Some(500),
            width: Some(FontWidth::Normal),
            spacing: None,
        },
        ListedFont {
            foundry: None,
            matched: super::FontMatch {
                family: "Noto Sans CJK JP".to_string(),
                file: Some("sans.ttc".to_string()),
                face_index: 0,
                variation_coords: Vec::new(),
                postscript_name: Some("Sans-Regular".to_string()),
                weight: Some(400),
                slant: FontSlant::Normal,
                size: PlatformFontSize::Scalable,
            },
            style: "Regular".to_string(),
            weight_css: Some(400),
            width: Some(FontWidth::Normal),
            spacing: None,
        },
        ListedFont {
            foundry: None,
            matched: super::FontMatch {
                family: "Noto Sans Mono CJK SC".to_string(),
                file: Some("mono.ttc".to_string()),
                face_index: 0,
                variation_coords: Vec::new(),
                postscript_name: Some("Mono-Regular".to_string()),
                weight: Some(400),
                slant: FontSlant::Normal,
                size: PlatformFontSize::Scalable,
            },
            style: "Regular".to_string(),
            weight_css: Some(400),
            width: Some(FontWidth::Normal),
            spacing: None,
        },
    ];

    let matched = super::best_candidate_for_pass(candidates, 400, false, None, None, true, None)
        .expect("best candidate");
    assert_eq!(matched.family, "Noto Sans CJK JP");
    assert_eq!(matched.postscript_name.as_deref(), Some("Sans-Regular"));
}

#[cfg(unix)]
#[test]
fn fc_list_candidates_tolerates_empty_fontsets() {
    let _ = fc_list_candidates(
        Some("definitely-missing-neomacs-font-family"),
        &[(0x10FFFF, 0x10FFFF)],
        Some(0x10FFFF),
        &[String::from("zz-zz")],
    );
}

#[cfg(unix)]
#[test]
fn fontconfig_handle_initializes() {
    assert!(super::fontconfig_handle().is_some(), "fontconfig handle");
}

#[cfg(unix)]
#[test]
fn invalid_fontconfig_family_is_not_classified_as_monospace() {
    assert!(!super::family_prefers_monospace("invalid\0family"));
}

#[test]
fn default_subpixel_order_resolves_to_known_variant() {
    assert!(matches!(
        super::default_subpixel_order(),
        super::FontconfigSubpixelOrder::Unknown
            | super::FontconfigSubpixelOrder::None
            | super::FontconfigSubpixelOrder::Rgb
            | super::FontconfigSubpixelOrder::Bgr
            | super::FontconfigSubpixelOrder::VRgb
            | super::FontconfigSubpixelOrder::VBgr
    ));
}
