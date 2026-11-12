use super::*;
fn query(family: &str) -> Query {
    Query::new(&CandidateQuery {
        family: Some(family),
        query_charset_ranges: &[],
        required_char: None,
        langs: &[],
        script_coverage: None,
        scalable: None,
        otf: None,
        kind: FcQueryKind::List,
    })
    .unwrap()
}
#[test]
fn complete_query_key_distinguishes_native_inputs() {
    let base = query("test");
    for other in [
        Query::new(&CandidateQuery {
            family: None,
            query_charset_ranges: &[],
            required_char: None,
            langs: &[],
            script_coverage: None,
            scalable: None,
            otf: None,
            kind: FcQueryKind::List,
        })
        .unwrap(),
        Query::new(&CandidateQuery {
            family: Some("test"),
            query_charset_ranges: &[(1, 3)],
            required_char: None,
            langs: &[],
            script_coverage: None,
            scalable: None,
            otf: None,
            kind: FcQueryKind::List,
        })
        .unwrap(),
        Query::new(&CandidateQuery {
            family: Some("test"),
            query_charset_ranges: &[],
            required_char: Some(65),
            langs: &[],
            script_coverage: None,
            scalable: None,
            otf: None,
            kind: FcQueryKind::List,
        })
        .unwrap(),
        Query::new(&CandidateQuery {
            family: Some("test"),
            query_charset_ranges: &[],
            required_char: None,
            langs: &["en".into()],
            script_coverage: None,
            scalable: None,
            otf: None,
            kind: FcQueryKind::List,
        })
        .unwrap(),
        Query::new(&CandidateQuery {
            family: Some("test"),
            query_charset_ranges: &[],
            required_char: None,
            langs: &[],
            script_coverage: None,
            scalable: None,
            otf: None,
            kind: FcQueryKind::Match,
        })
        .unwrap(),
    ] {
        assert_ne!(base, other);
    }
}

#[test]
fn otf_listing_key_accounts_for_owned_feature_payload_and_budget() {
    use crate::font_backend::{NativeFontOtfFeature, NativeFontOtfRequirement};
    let mut requirement = NativeFontOtfRequirement {
        script: *b"latn",
        language: None,
        gsub: vec![NativeFontOtfFeature::Required(*b"liga"); 1000],
        gpos: vec![],
    };
    let key = Query::new(&CandidateQuery {
        family: Some("test"),
        query_charset_ranges: &[],
        required_char: None,
        langs: &[],
        script_coverage: None,
        scalable: None,
        otf: Some(&requirement),
        kind: FcQueryKind::List,
    })
    .unwrap();
    let mut cache = CandidateQueries::with_limits(8, 1024);
    cache.insert(key.clone(), &[]);
    assert!(
        cache.get(&key).is_none(),
        "feature payload counts toward cached bytes"
    );
    requirement
        .gsub
        .resize(20000, NativeFontOtfFeature::Required(*b"liga"));
    assert!(
        Query::new(&CandidateQuery {
            family: Some("test"),
            query_charset_ranges: &[],
            required_char: None,
            langs: &[],
            script_coverage: None,
            scalable: None,
            otf: Some(&requirement),
            kind: FcQueryKind::List,
        })
        .is_none(),
        "oversized feature requests bypass key allocation and reusable storage"
    );
}
#[test]
fn bounded_cache_reuses_negative_answers_and_evicts_least_recent_query() {
    let mut cache = CandidateQueries::with_limits(2, 4096);
    cache.insert(query("a"), &[]);
    cache.insert(query("b"), &[]);
    assert_eq!(cache.get(&query("a")), Some(vec![]));
    cache.insert(query("c"), &[]);
    assert_eq!(cache.get(&query("b")), None);
    assert_eq!(cache.get(&query("a")), Some(vec![]));
    assert!(cache.bytes <= 4096);
}
#[test]
fn byte_limit_evicts_and_oversized_keys_are_not_retained() {
    let mut cache = CandidateQueries::with_limits(128, query("a").bytes());
    cache.insert(query("a"), &[]);
    cache.insert(query("b"), &[]);
    assert_eq!(cache.entries.len(), 1);
    assert_eq!(cache.get(&query("a")), None);
    cache.insert(query("too long"), &[]);
    assert_eq!(cache.entries.len(), 1);
    assert!(
        Query::new(&CandidateQuery {
            family: Some(&"x".repeat(65537)),
            query_charset_ranges: &[],
            required_char: None,
            langs: &[],
            script_coverage: None,
            scalable: None,
            otf: None,
            kind: FcQueryKind::List,
        })
        .is_none()
    );
}

#[test]
fn font_payload_counts_toward_limit_and_returned_answers_are_independent() {
    let font = ListedFont {
        adstyle: None,
        matched: super::super::FontMatch {
            family: "fixture".into(),
            file: Some("font-file".into()),
            face_index: 0,
            variation_coords: vec![],
            postscript_name: None,
            weight: None,
            slant: neovm_core::face::FontSlant::Normal,
            size: crate::font_backend::PlatformFontSize::Scalable,
        },
        style: "regular".into(),
        weight_css: None,
        width: None,
        spacing: None,
        foundry: None,
    };
    let mut large_adstyle = font.clone();
    large_adstyle.adstyle = Some("ja".repeat(1024));
    let mut bounded = CandidateQueries::with_limits(8, 1024);
    bounded.insert(query("adstyle"), &[large_adstyle]);
    assert!(
        bounded.get(&query("adstyle")).is_none(),
        "additional style payload counts toward the cache budget"
    );
    let bytes = query("a").bytes() + font_bytes(&font);
    let mut cache = CandidateQueries::with_limits(128, bytes);
    cache.insert(query("a"), std::slice::from_ref(&font));
    let mut answer = cache.get(&query("a")).unwrap();
    answer[0].matched.family.clear();
    assert_eq!(cache.get(&query("a")), Some(vec![font.clone()]));
    cache.insert(query("b"), &[font.clone(), font]);
    assert_eq!(cache.get(&query("b")), None);
    assert_eq!(cache.bytes, bytes);
}

#[test]
fn script_query_keys_preserve_mode_characters_and_payload_budget() {
    let all = NativeFontScriptCoverage::All(Vec::new());
    let any = NativeFontScriptCoverage::Any(Vec::new());
    let all_key = Query::new(&CandidateQuery {
        family: Some("script"),
        query_charset_ranges: &[],
        required_char: None,
        langs: &[],
        script_coverage: Some(&all),
        scalable: None,
        otf: None,
        kind: FcQueryKind::List,
    })
    .unwrap();
    let any_key = Query::new(&CandidateQuery {
        family: Some("script"),
        query_charset_ranges: &[],
        required_char: None,
        langs: &[],
        script_coverage: Some(&any),
        scalable: None,
        otf: None,
        kind: FcQueryKind::List,
    })
    .unwrap();
    assert_ne!(
        all_key, any_key,
        "the script mode is semantically significant"
    );
    let large = NativeFontScriptCoverage::All(vec![0x41; 20_000]);
    assert!(
        Query::new(&CandidateQuery {
            family: Some("script"),
            query_charset_ranges: &[],
            required_char: None,
            langs: &[],
            script_coverage: Some(&large),
            scalable: None,
            otf: None,
            kind: FcQueryKind::List,
        })
        .is_none(),
        "oversized script keys bypass reusable storage"
    );
    let payload = NativeFontScriptCoverage::Any(vec![0x41; 1024]);
    let key = Query::new(&CandidateQuery {
        family: Some("script"),
        query_charset_ranges: &[],
        required_char: None,
        langs: &[],
        script_coverage: Some(&payload),
        scalable: None,
        otf: None,
        kind: FcQueryKind::List,
    })
    .unwrap();
    let mut cache = CandidateQueries::with_limits(8, query("script").bytes() + 128);
    cache.insert(key.clone(), &[]);
    assert!(
        cache.get(&key).is_none(),
        "owned representative characters count toward cache bytes"
    );
}
