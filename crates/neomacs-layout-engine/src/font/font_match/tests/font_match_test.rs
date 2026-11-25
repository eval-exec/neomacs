use super::*;

#[test]
fn cosmic_family_selection_uses_generic_monospace_for_missing_mono_like_family() {
    let font_system = crate::font::metrics::test_catalog::font_system();

    assert_eq!(
        select_cosmic_family(&font_system, "Definitely Missing Mono"),
        CosmicFamilySelection::Monospace
    );
}

#[test]
fn cosmic_family_selection_keeps_existing_concrete_family() {
    let font_system = crate::font::metrics::test_catalog::font_system();

    assert_eq!(
        select_cosmic_family(&font_system, "DejaVu Sans Mono"),
        CosmicFamilySelection::Name("DejaVu Sans Mono")
    );
}
