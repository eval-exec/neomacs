use super::*;

#[test]
fn page_counts_start_at_zero() {
    let pages = GlyphAtlasPages::new(GlyphAtlasConfig::default());
    assert_eq!(pages.page_counts(), (0, 0, 0));
}

#[test]
fn next_page_id_increments() {
    let mut pages = GlyphAtlasPages::new(GlyphAtlasConfig::default());
    let id1 = pages.next_page_id_raw();
    let id2 = pages.next_page_id_raw();
    assert!(id2.get() > id1.get());
}

#[test]
fn clear_resets_page_id_counter() {
    let mut pages = GlyphAtlasPages::new(GlyphAtlasConfig::default());
    let _ = pages.next_page_id_raw();
    let _ = pages.next_page_id_raw();
    pages.clear();
    let id_after = pages.next_page_id_raw();
    assert_eq!(id_after.get(), 1);
}
