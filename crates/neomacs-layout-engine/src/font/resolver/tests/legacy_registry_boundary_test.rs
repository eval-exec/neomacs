use super::*;
use std::sync::{Arc, Mutex};

struct RegistryBoundaryBackend {
    kind: FontBackendKind,
    queries: Arc<Mutex<Vec<FontCandidateQuery>>>,
}

impl FontBackend for RegistryBoundaryBackend {
    fn kind(&self) -> FontBackendKind {
        self.kind
    }
    fn list_families(&self) -> Vec<FontFamilyName> {
        Vec::new()
    }
    fn resolve_family(&self, name: &str) -> String {
        name.to_owned()
    }
    fn family_prefers_monospace(&self, _: &str) -> bool {
        false
    }
    fn list_candidates(&self, query: &FontCandidateQuery) -> Vec<FontCandidate> {
        self.queries.lock().unwrap().push(query.clone());
        vec![candidate("Registry Boundary", 400, FontSlant::Normal, 100)]
    }
    fn advance_catalog_generation(&mut self) {}
    fn poll_catalog_change(&mut self) -> crate::font::catalog::FontCatalogChange {
        crate::font::catalog::FontCatalogChange::Unchanged
    }
}

fn boundary(kind: FontBackendKind) -> (FontResolver, Arc<Mutex<Vec<FontCandidateQuery>>>) {
    let queries = Arc::new(Mutex::new(Vec::new()));
    (
        FontResolver::new(Box::new(RegistryBoundaryBackend {
            kind,
            queries: Arc::clone(&queries),
        })),
        queries,
    )
}

#[test]
fn windows_unknown_registry_reaches_the_driver_without_fontconfig_constraints() {
    let (resolver, queries) = boundary(FontBackendKind::DirectWrite);
    let query =
        FontEntityQuery::new(FontFamilyName::new("Registry Boundary")).with_registry("unknown");
    assert_eq!(
        resolver
            .list_entities(&query, &Default::default(), None, Some(1))
            .len(),
        1
    );
    let queries = queries.lock().unwrap();
    assert_eq!(queries.len(), 1);
    assert!(
        queries[0].charset_ranges.is_empty(),
        "unknown does not invent a Latin-a requirement"
    );
    assert!(queries[0].languages.is_empty());
}

#[test]
fn windows_unrecognized_literal_registries_do_not_enumerate_fonts() {
    for registry in ["n736-unrecognized-registry", "["] {
        let (resolver, queries) = boundary(FontBackendKind::DirectWrite);
        let query =
            FontEntityQuery::new(FontFamilyName::new("Registry Boundary")).with_registry(registry);
        assert_eq!(
            resolver.validate_font_entity_listing(Some(registry), None),
            Ok(false)
        );
        assert!(
            resolver
                .list_entities(&query, &Default::default(), None, None)
                .is_empty()
        );
        assert!(queries.lock().unwrap().is_empty());
    }
}

#[test]
fn registry_query_builder_preserves_raw_whitespace_and_empty_patterns() {
    for registry in [" iso8859-1 ", ""] {
        let query = FontEntityQuery::new(None).with_registry(registry);
        assert_eq!(query.registry.as_deref(), Some(registry));
    }
}

#[test]
fn fontconfig_invalid_registry_preserves_the_regexp_compile_error() {
    let (resolver, queries) = boundary(FontBackendKind::Fontconfig);
    let result = resolver.validate_font_entity_listing(Some("["), None);
    assert!(
        matches!(result,
        Err(neovm_core::emacs_core::regex::EmacsRegexpError::Compile(ref message))
        if message == "Unmatched [ or [^"),
        "{result:?}"
    );
    assert!(queries.lock().unwrap().is_empty());
}

#[test]
fn inactive_font_driver_rejects_before_parsing_a_registry_regexp() {
    let (resolver, queries) = boundary(FontBackendKind::Fontconfig);
    assert_eq!(
        resolver.validate_font_entity_listing(Some("["), Some(intern("n736-inactive-driver"))),
        Ok(false)
    );
    assert!(queries.lock().unwrap().is_empty());
}

#[test]
fn coretext_unknown_and_regexp_metacharacter_registries_do_not_use_fontconfig_rules() {
    for registry in ["unknown", "[", "n736-unrecognized-registry"] {
        let (resolver, queries) = boundary(FontBackendKind::CoreText);
        let query =
            FontEntityQuery::new(FontFamilyName::new("Registry Boundary")).with_registry(registry);
        assert_eq!(
            resolver.validate_font_entity_listing(Some(registry), None),
            Ok(true)
        );
        assert_eq!(
            resolver
                .list_entities(&query, &Default::default(), None, Some(1))
                .len(),
            1
        );
        let queries = queries.lock().unwrap();
        assert_eq!(queries.len(), 1);
        assert!(queries[0].charset_ranges.is_empty());
        assert!(queries[0].languages.is_empty());
    }
}

#[test]
fn windows_registry_shortcuts_do_not_create_fontconfig_charsets() {
    for registry in [
        "unknown",
        "ascii-0",
        "iso10646-1",
        "unicode-bmp",
        "unicode-sip",
    ] {
        let (resolver, queries) = boundary(FontBackendKind::DirectWrite);
        let query =
            FontEntityQuery::new(FontFamilyName::new("Registry Boundary")).with_registry(registry);
        assert_eq!(
            resolver
                .list_entities(&query, &Default::default(), None, Some(1))
                .len(),
            1
        );
        assert!(queries.lock().unwrap()[0].charset_ranges.is_empty());
    }
}

#[test]
fn native_option_selection_skips_a_bad_regexp_before_driver_enumeration() {
    let (resolver, queries) = boundary(FontBackendKind::Fontconfig);
    let query = FontEntityQuery::new(FontFamilyName::new("Registry Boundary")).with_registry("[");
    assert!(resolver.resolve_entity(&query).is_none());
    assert!(queries.lock().unwrap().is_empty());
}

#[test]
fn coretext_preserves_the_existing_recognized_legacy_constraints() {
    let (resolver, queries) = boundary(FontBackendKind::CoreText);
    let query =
        FontEntityQuery::new(FontFamilyName::new("Registry Boundary")).with_registry("iso8859-2");
    assert_eq!(
        resolver
            .list_entities(&query, &Default::default(), None, Some(1))
            .len(),
        1
    );
    let queries = queries.lock().unwrap();
    assert_eq!(
        queries[0].charset_ranges,
        vec![(0xa0, 0xa0), (0x10e, 0x10e)]
    );
}
