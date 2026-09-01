#![cfg(windows)]

use dwrote::FontCollection;
use neomacs_layout_engine::font_backend::{DirectWriteBackend, FontBackend};
use std::collections::HashSet;

#[test]
fn list_families_includes_directwrite_selected_family_names() {
    let listed = DirectWriteBackend::default()
        .list_families()
        .into_iter()
        .map(|family| family.into_string())
        .collect::<HashSet<_>>();
    let missing = FontCollection::system()
        .families_iter()
        .filter_map(|family| family.family_name().ok())
        .filter(|family| !listed.contains(family))
        .collect::<Vec<_>>();

    assert!(
        missing.is_empty(),
        "DirectWrite family list omitted selected family names: {missing:?}"
    );
}

#[test]
fn listed_families_resolve_through_directwrite() {
    let collection = FontCollection::system();
    let unresolved = DirectWriteBackend::default()
        .list_families()
        .into_iter()
        .filter(|family| {
            collection
                .font_family_by_name(family.as_str())
                .ok()
                .flatten()
                .is_none()
        })
        .map(|family| family.into_string())
        .collect::<Vec<_>>();

    assert!(
        unresolved.is_empty(),
        "DirectWrite family list contained unresolved names: {unresolved:?}"
    );
}

#[test]
fn list_families_includes_accepted_family_alias() {
    const ALIAS: &str = "CaskaydiaCove NF";
    let collection = FontCollection::system();
    if collection
        .font_family_by_name(ALIAS)
        .ok()
        .flatten()
        .is_none()
    {
        return;
    }

    let families = DirectWriteBackend::default().list_families();
    assert!(
        families.iter().any(|family| family.as_str() == ALIAS),
        "DirectWrite family list omitted {ALIAS}"
    );
}
