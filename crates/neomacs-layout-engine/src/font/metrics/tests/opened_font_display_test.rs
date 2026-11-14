use super::*;
use neomacs_display_protocol::GraphicalBackend;

fn connection() -> GraphicalDisplayIdentity {
    GraphicalDisplayIdentity::named(GraphicalBackend::X11, ":fixture").unwrap()
}

fn service() -> FontMetricsService {
    test_catalog::metrics_service()
}

fn open_height(service: &mut FontMetricsService, size: f32) -> u32 {
    let font = service
        .select_font_for_char('M', "monospace", 400, false, size)
        .expect("the immutable catalog has a monospace primary font");
    u32::try_from(font.metrics.ascent + font.metrics.descent).unwrap()
}

#[test]
fn display_minimum_is_shared_by_services_and_survives_cache_invalidation() {
    let display = connection();
    let clone = display.clone();
    let mut presentation = service();
    let mut query = service();
    presentation.set_display_identity(display.clone());
    query.set_display_identity(clone.clone());

    let large = open_height(&mut presentation, 32.0);
    assert_eq!(display.smallest_opened_font_height(), large);
    assert_eq!(clone.smallest_opened_font_height(), large);
    let small = open_height(&mut query, 8.0);
    assert!(small < large);
    assert_eq!(display.smallest_opened_font_height(), small);

    presentation.clear_caches();
    assert_eq!(open_height(&mut presentation, 32.0), large);
    assert_eq!(display.smallest_opened_font_height(), small);
    // A catalog transition discards canonical metric records as well as
    // request caches. Neither operation closes the display's retained font.
    presentation.font_instances.invalidate_catalog_records();
    presentation.clear_caches();
    assert_eq!(open_height(&mut presentation, 32.0), large);
    assert_eq!(display.smallest_opened_font_height(), small);
}

#[test]
fn rebinding_equal_display_labels_replaces_the_connection_owner() {
    let first = connection();
    let second = connection();
    assert_eq!(first, second);
    let mut metrics = service();
    metrics.set_display_identity(first.clone());
    let small = open_height(&mut metrics, 8.0);
    assert_eq!(first.smallest_opened_font_height(), small);
    assert_eq!(second.smallest_opened_font_height(), 1);

    metrics.set_display_identity(second.clone());
    let large = open_height(&mut metrics, 32.0);
    assert!(large > small);
    assert_eq!(second.smallest_opened_font_height(), large);
    assert_eq!(first.smallest_opened_font_height(), small);
}

#[test]
fn entity_opening_records_its_exact_integer_metrics_without_a_layout_walk() {
    let display = connection();
    let mut metrics = service();
    metrics.set_display_identity(display.clone());
    let query = crate::font::resolver::FontEntityQuery::new(
        crate::font_backend::FontFamilyName::new("Noto Sans Mono"),
    );
    let large = metrics.open_font_entity(&query, 32).unwrap();
    let large_height = u32::try_from(large.metrics.ascent + large.metrics.descent).unwrap();
    assert_eq!(display.smallest_opened_font_height(), large_height);
    let small = metrics.open_font_entity(&query, 8).unwrap();
    let small_height = u32::try_from(small.metrics.ascent + small.metrics.descent).unwrap();
    assert!(small_height < large_height);
    assert_eq!(display.smallest_opened_font_height(), small_height);
}

#[test]
fn failed_or_unobserved_vertical_metrics_do_not_lower_the_display_minimum() {
    let display = connection();
    let mut metrics = service();
    assert!(metrics.display_identity.is_none());
    metrics.set_display_identity(display.clone());
    let opened_height = open_height(&mut metrics, 32.0);
    metrics.observe_opened_font_vertical_metrics(None);
    for (ascent, descent) in [
        (0.0, 0.0),
        (f32::NAN, 1.0),
        (1.0, f32::INFINITY),
        (-0.25, 1.0),
        (f32::MAX, 1.0),
    ] {
        metrics.observe_opened_font_vertical_metrics(Some(FontVerticalMetrics {
            ascent,
            descent,
            // A projected/fallback line height cannot repair absent metrics.
            line_height: 1.0,
        }));
    }
    let missing = crate::font::resolver::FontEntityQuery::new(None)
        .with_postscript_name("NoSuchFixtureFont-OpenedDisplayMetrics");
    assert!(metrics.open_font_entity(&missing, 8).is_none());
    assert_eq!(display.smallest_opened_font_height(), opened_height);
}
