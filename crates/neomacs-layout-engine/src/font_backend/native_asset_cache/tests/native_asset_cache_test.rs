use super::NativeFontAssetCache;
use neomacs_display_protocol::font::{FontBackendKind, ResolvedFontIdentity};
use std::num::NonZeroUsize;
use std::sync::Arc;

fn identity(stable_key: &str, face_index: u32) -> ResolvedFontIdentity {
    ResolvedFontIdentity::from_memory(
        FontBackendKind::CoreText,
        stable_key.to_owned(),
        face_index,
        None,
    )
}

#[test]
fn file_identity_cannot_enter_the_native_asset_cache() {
    let cache = NativeFontAssetCache::default();
    let file = ResolvedFontIdentity::from_file("/fixture/font.ttf", 0, None);
    let mut materialized = false;

    let asset = cache.get_or_materialize(&file, || {
        materialized = true;
        Some(vec![1])
    });

    assert!(asset.is_none());
    assert!(!materialized);
}

#[test]
fn live_native_bytes_are_reused_without_another_platform_copy() {
    let cache = NativeFontAssetCache::new(NonZeroUsize::new(2).expect("non-zero capacity"));
    let mut loads = 0;
    let identity = identity("native:fixture", 0);
    let first = cache
        .get_or_materialize(&identity, || {
            loads += 1;
            Some(vec![1, 2, 3])
        })
        .expect("first materialization");
    let second = cache
        .get_or_materialize(&identity, || {
            loads += 1;
            Some(vec![4, 5, 6])
        })
        .expect("cached materialization");

    assert_eq!(loads, 1);
    assert!(Arc::ptr_eq(&first.shared_bytes(), &second.shared_bytes()));
}

#[test]
fn cache_is_bounded_and_does_not_keep_evicted_font_bytes_alive() {
    let cache = NativeFontAssetCache::new(NonZeroUsize::new(1).expect("non-zero capacity"));
    let first = cache
        .get_or_materialize(&identity("native:first", 0), || Some(vec![1]))
        .expect("first materialization");
    let first_bytes = first.shared_bytes();
    let first_weak = Arc::downgrade(&first_bytes);

    let _second = cache
        .get_or_materialize(&identity("native:second", 0), || Some(vec![2]))
        .expect("second materialization evicts the first cache entry");
    drop(first_bytes);
    drop(first);
    assert!(first_weak.upgrade().is_none());

    let mut reloaded = false;
    cache
        .get_or_materialize(&identity("native:first", 0), || {
            reloaded = true;
            Some(vec![3])
        })
        .expect("evicted asset is materialized again");
    assert!(reloaded);
}

#[test]
fn clearing_starts_a_new_generation_while_old_users_keep_their_bytes() {
    let cache = NativeFontAssetCache::new(NonZeroUsize::new(2).expect("non-zero capacity"));
    let identity = identity("native:fixture", 0);
    let old = cache
        .get_or_materialize(&identity, || Some(vec![1]))
        .expect("old generation materialization");

    cache.clear();
    let mut reloaded = false;
    let new = cache
        .get_or_materialize(&identity, || {
            reloaded = true;
            Some(vec![2])
        })
        .expect("new generation materialization");

    assert!(reloaded);
    assert_eq!(old.bytes(), &[1]);
    assert_eq!(new.bytes(), &[2]);
    assert!(!Arc::ptr_eq(&old.shared_bytes(), &new.shared_bytes()));
}
