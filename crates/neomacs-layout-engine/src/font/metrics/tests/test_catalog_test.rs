use super::*;

#[test]
fn standard_catalog_reuses_one_owner_and_has_distinct_real_generics() {
    assert_eq!(standard_catalog(), standard_catalog());
    let catalog = standard_catalog();
    let mono = catalog.resolve_family("monospace");
    let sans = catalog.resolve_family("sans-serif");
    let serif = catalog.resolve_family("serif");
    assert_eq!(mono, "Noto Sans Mono");
    assert_eq!(sans, "Noto Sans");
    assert_eq!(serif, "Noto Serif");
    assert_ne!(mono, sans);
    assert_ne!(sans, serif);
    for family in [&mono, &sans, &serif] {
        assert!(
            catalog
                .0
                .candidates
                .iter()
                .any(|(_, candidate)| &candidate.metadata.family == family)
        );
    }
    let engine = layout_engine();
    assert_eq!(
        engine.font_metrics.as_ref().unwrap().test_catalog(),
        TestFontCatalogSource::Fixture(catalog.clone())
    );
}

#[test]
fn named_generic_family_constructor_refuses_a_missing_proportional_face() {
    let files = [neomacs_test_fonts::noto_sans_mono_variable()];
    assert!(matches!(
        TestFontCatalog::from_files_with_sans_and_serif(
            &files,
            &FontFamilyName::new("Missing Sans").unwrap(),
            &FontFamilyName::new("Noto Sans Mono").unwrap(),
        ),
        Err(TestFontCatalogError::MissingSans(_))
    ));
}
