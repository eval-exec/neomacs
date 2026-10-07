use super::{
    FontCatalog, FontCatalogChange, FontCatalogChangeCounter, FontCatalogPollAction,
    FontCatalogUpdate, RateLimitedFontCatalogPoller,
};

#[test]
fn native_change_counter_coalesces_bursts_for_every_observer() {
    let counter = FontCatalogChangeCounter::default();
    let mut layout = counter.cursor();
    let mut display_host = counter.cursor();
    counter.mark_changed();
    counter.mark_changed();

    assert_eq!(layout.poll(&counter), FontCatalogChange::Changed);
    assert_eq!(layout.poll(&counter), FontCatalogChange::Unchanged);
    assert_eq!(display_host.poll(&counter), FontCatalogChange::Changed);
    assert_eq!(display_host.poll(&counter), FontCatalogChange::Unchanged);
}

#[test]
fn rate_limited_poller_prioritizes_published_edges_and_coalesces_probes() {
    let counter = FontCatalogChangeCounter::default();
    let mut poller =
        RateLimitedFontCatalogPoller::new(&counter, std::time::Duration::from_secs(60));

    assert_eq!(
        poller.begin(&counter),
        FontCatalogPollAction::ProbeNativeCatalog
    );
    assert_eq!(poller.begin(&counter), FontCatalogPollAction::Wait);

    counter.mark_changed();
    assert_eq!(
        poller.begin(&counter),
        FontCatalogPollAction::PublishedChange
    );
    assert_eq!(poller.begin(&counter), FontCatalogPollAction::Wait);
}

#[test]
fn catalog_advances_exactly_once_for_each_consumed_change() {
    let mut catalog = FontCatalog::default();
    let initial = catalog.generation();

    assert_eq!(
        catalog.observe(FontCatalogChange::Unchanged),
        FontCatalogUpdate::Unchanged(initial)
    );
    assert_eq!(
        catalog.observe(FontCatalogChange::Changed),
        FontCatalogUpdate::Advanced {
            previous: initial,
            current: initial.next(),
        }
    );
    assert_eq!(catalog.generation(), initial.next());
}
