use super::super::*;
use super::selected_font_capability_test::{
    list_catalog_font, listed_catalog_font, native_catalog_font,
};
use neomacs_layout_engine::font::catalog::{FontCatalog, FontCatalogChange};

struct ReplacedFontFile(std::path::PathBuf);

impl ReplacedFontFile {
    fn new() -> Self {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "neomacs-catalog-replacement-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn replace_and_observe(
        &self,
        source: &std::path::Path,
    ) -> neomacs_layout_engine::font_backend::PlatformFontMatch {
        use neomacs_display_protocol::font::{FontMemoryAsset, FontOutlineAsset};
        let path = self.0.join("font.ttf");
        std::fs::copy(source, &path).unwrap();
        let key = format!("native-observation:{}", path.display());
        let mut font = native_catalog_font(&key);
        // A native-memory observation fixture captures real bytes after a
        // stable-path replacement. This is not a native CoreText execution.
        font.asset = FontOutlineAsset::Memory(
            FontMemoryAsset::new(key, Arc::new(std::fs::read(&path).unwrap()), 0).unwrap(),
        );
        font
    }
}

impl Drop for ReplacedFontFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn replacement_pair() -> (
    ReplacedFontFile,
    neomacs_layout_engine::font_backend::PlatformFontMatch,
    neomacs_layout_engine::font_backend::PlatformFontMatch,
) {
    let fixture = ReplacedFontFile::new();
    let mut old = fixture.replace_and_observe(neomacs_test_fonts::hack_regular_3_003());
    old.metadata.family = "Hack".to_owned();
    let mut new = fixture.replace_and_observe(neomacs_test_fonts::dejavu_2_37().sans_regular());
    new.metadata.family = "DejaVu Sans".to_owned();
    assert_eq!(
        old.identity, new.identity,
        "native catalog identity remains stable"
    );
    let mut metrics = FontMetricsService::new();
    assert_eq!(
        metrics.font_has_char_in_match(&old, '\u{0180}'),
        Some(false),
        "pinned Hack control"
    );
    assert_eq!(
        metrics.font_has_char_in_match(&new, '\u{0180}'),
        Some(true),
        "pinned DejaVu control"
    );
    (fixture, old, new)
}

#[test]
fn catalog_generation_replacement_discovers_new_coverage_and_preserves_old_handle() {
    let (_fixture, old, new) = replacement_pair();
    let mut catalog = FontEntityCatalog::default();
    let mut lifecycle = FontCatalog::default();
    let old_handle = catalog.intern(old.clone()).unwrap();
    catalog.observe_catalog_update(lifecycle.observe(FontCatalogChange::Changed));
    let new_handle = list_catalog_font(&mut catalog, listed_catalog_font(&new))
        .coverage_handle
        .unwrap();
    assert_ne!(
        new_handle, old_handle,
        "new discoveries must not reuse the previous generation"
    );
    assert_eq!(
        catalog.discovered(new_handle).unwrap().metadata.family,
        "DejaVu Sans"
    );
    assert_eq!(catalog.get(old_handle), Some(&old));
    assert!(catalog.discovered(new_handle).is_some());
    assert!(catalog.promote(new_handle, new.clone()));
    let mut metrics = FontMetricsService::new();
    assert_eq!(
        metrics.font_has_char_in_match(catalog.get(old_handle).unwrap(), '\u{0180}'),
        Some(false)
    );
    assert_eq!(
        metrics.font_has_char_in_match(catalog.get(new_handle).unwrap(), '\u{0180}'),
        Some(true)
    );
    assert_eq!(catalog.intern(new), Some(new_handle));
}

#[test]
fn catalog_generation_old_promotion_before_rediscovery_cannot_republish_stale_identity() {
    let (_fixture, old, new) = replacement_pair();
    let mut catalog = FontEntityCatalog::default();
    let mut lifecycle = FontCatalog::default();
    let old_handle = list_catalog_font(&mut catalog, listed_catalog_font(&old))
        .coverage_handle
        .unwrap();
    catalog.observe_catalog_update(lifecycle.observe(FontCatalogChange::Changed));
    assert!(
        catalog.promote(old_handle, old.clone()),
        "an old Lisp handle remains usable"
    );
    let new_handle = list_catalog_font(&mut catalog, listed_catalog_font(&new))
        .coverage_handle
        .unwrap();
    assert_ne!(
        new_handle, old_handle,
        "old promotion cannot repopulate current canonical lookup"
    );
    assert_eq!(catalog.get(old_handle), Some(&old));
    assert!(catalog.promote(new_handle, new.clone()));
    assert_eq!(catalog.intern(new), Some(new_handle));
}

#[test]
fn catalog_generation_old_promotion_cannot_override_a_current_handle() {
    let (_fixture, old, new) = replacement_pair();
    let mut catalog = FontEntityCatalog::default();
    let mut lifecycle = FontCatalog::default();
    let old_handle = list_catalog_font(&mut catalog, listed_catalog_font(&old))
        .coverage_handle
        .unwrap();
    catalog.observe_catalog_update(lifecycle.observe(FontCatalogChange::Changed));
    let new_handle = list_catalog_font(&mut catalog, listed_catalog_font(&new))
        .coverage_handle
        .unwrap();
    assert!(catalog.promote(old_handle, old.clone()));
    assert_eq!(
        catalog.intern(new.clone()),
        Some(new_handle),
        "archived promotion must not hijack current intern"
    );
    assert_ne!(new_handle, old_handle);
    assert_eq!(catalog.get(old_handle), Some(&old));
    assert_eq!(catalog.get(new_handle), Some(&new));
}

#[test]
fn catalog_generation_unchanged_update_preserves_current_lookup_reuse() {
    let mut catalog = FontEntityCatalog::default();
    let mut lifecycle = FontCatalog::default();
    let font = native_catalog_font("native:unchanged-control");
    let handle = catalog.intern(font.clone()).unwrap();
    catalog.observe_catalog_update(lifecycle.observe(FontCatalogChange::Unchanged));
    assert_eq!(
        catalog.intern_discovered(listed_catalog_font(&font).candidate),
        Some(handle)
    );
    assert_eq!(catalog.intern(font), Some(handle));
    assert_eq!(catalog.entries.len(), 1);
}
