use super::*;

#[test]
fn cosmic_family_selection_uses_generic_monospace_for_missing_mono_like_family() {
    let font_system = FontSystem::new();

    assert_eq!(
        select_cosmic_family(&font_system, "Definitely Missing Mono"),
        CosmicFamilySelection::Monospace
    );
}

#[test]
fn cosmic_family_selection_keeps_existing_concrete_family() {
    let font_system = FontSystem::new();

    assert_eq!(
        select_cosmic_family(&font_system, "DejaVu Sans Mono"),
        CosmicFamilySelection::Name("DejaVu Sans Mono")
    );
}

#[test]
fn nearest_lower_then_upper_for_static_weights() {
    let ws = [400u16, 600, 800];
    assert_eq!(pick_nearest_css_weight(&ws, 700), 600);
    assert_eq!(pick_nearest_css_weight(&ws, 850), 800);
    assert_eq!(pick_nearest_css_weight(&ws, 300), 400);
}

#[test]
fn nearest_weight_prefers_closest_match() {
    let ws = [400u16, 700];
    assert_eq!(pick_nearest_css_weight(&ws, 600), 700);
    assert_eq!(pick_nearest_css_weight(&ws, 650), 700);
    assert_eq!(pick_nearest_css_weight(&ws, 550), 400);
}

#[test]
fn variable_font_without_named_instances_clamps_within_range() {
    let info = FamilyWeightInfo {
        discrete_weights: vec![400],
        variable_weight_range: Some((100, 900)),
        named_instance_weights: vec![],
    };
    assert_eq!(resolve_requested_weight(&info, 700), 700);
}

#[test]
fn variable_font_clamps_only_to_axis_bounds_without_named_instances() {
    let info = FamilyWeightInfo {
        discrete_weights: vec![400],
        variable_weight_range: Some((200, 750)),
        named_instance_weights: vec![],
    };
    assert_eq!(resolve_requested_weight(&info, 150), 200);
    assert_eq!(resolve_requested_weight(&info, 900), 750);
}

#[test]
fn variable_font_snaps_request_to_nearest_named_instance() {
    // Noto Sans: named instances thin..black (100..900), no 350/950.
    // GNU/fontconfig open the nearest instance (verified vs GNU 31.0.50).
    let info = FamilyWeightInfo {
        discrete_weights: vec![400],
        variable_weight_range: Some((100, 900)),
        named_instance_weights: vec![100, 200, 300, 400, 500, 600, 700, 800, 900],
    };
    // semi-light 350 -> light 300 (tie 300/400 broken toward lower).
    assert_eq!(resolve_requested_weight(&info, 350), 300);
    // ultra-heavy 950 -> black 900 (nothing higher available).
    assert_eq!(resolve_requested_weight(&info, 950), 900);
    // exact instances pass through.
    assert_eq!(resolve_requested_weight(&info, 700), 700);
    assert_eq!(resolve_requested_weight(&info, 100), 100);
}
