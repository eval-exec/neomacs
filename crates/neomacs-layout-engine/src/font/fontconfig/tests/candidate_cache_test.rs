use super::*;
fn query(family: &str) -> Query {
    Query::new(Some(family), &[], None, &[], FcQueryKind::List).unwrap()
}
#[test]
fn complete_query_key_distinguishes_native_inputs() {
    let base = query("test");
    for other in [
        Query::new(None, &[], None, &[], FcQueryKind::List).unwrap(),
        Query::new(Some("test"), &[(1, 3)], None, &[], FcQueryKind::List).unwrap(),
        Query::new(Some("test"), &[], Some(65), &[], FcQueryKind::List).unwrap(),
        Query::new(Some("test"), &[], None, &["en".into()], FcQueryKind::List).unwrap(),
        Query::new(Some("test"), &[], None, &[], FcQueryKind::Match).unwrap(),
    ] {
        assert_ne!(base, other);
    }
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
    assert!(Query::new(Some(&"x".repeat(65537)), &[], None, &[], FcQueryKind::List).is_none());
}

#[test]
fn font_payload_counts_toward_limit_and_returned_answers_are_independent() {
    let font = ListedFont {
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
