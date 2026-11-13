use super::*;

fn pin_table_with_named_face() -> FaceTable {
    let mut table = FaceTable::new();
    let mut named = NeoFace::new("pin-refresh-named");
    named.weight = Some(neovm_core::face::FontWeight::BOLD);
    table.define("pin-refresh-named", named);
    table
}

#[test]
fn composed_pin_adopts_the_cache_across_a_definitions_refresh() {
    let _context = neovm_core::emacs_core::eval_hook_installers::new_context();
    let table = pin_table_with_named_face();

    let resolver = FaceResolver::new(&table, 0x00FFFFFF, 0x00000000, 14.0, None);
    // A named fold and an anonymous composition, so both cache halves are
    // populated before the pin is taken.
    resolver.resolve_named_face_overlay_spec(
        "pin-refresh-named",
        0,
        FaceReferenceDiagnostics::Suppress,
    );
    let mut base = resolver.default_face().clone();
    base.source_attributes = Some(std::sync::Arc::new(
        RootedFace::new(NeoFace::new("pin-refresh-base")).expect("installed mutator heap"),
    ));
    let mut overlay = NeoFace::new("pin-refresh-anonymous");
    overlay.slant = Some(neovm_core::face::FontSlant::Italic);
    let _ = resolver.apply_specified_face_over(&base, &overlay, None);
    {
        let cache = resolver.composed_sources.borrow();
        assert!(!cache.named_specs.is_empty(), "named fold recorded");
        assert!(!cache.compositions.is_empty(), "composition recorded");
    }

    // A definitions refresh re-derives a face: copy-on-write produces a new
    // snapshot, exactly `Rc::make_mut` reallocating under this pin.
    let pin = resolver.composed_source_pin();
    let mut refreshed = table.clone();
    let mut changed = NeoFace::new("pin-refresh-named");
    changed.weight = Some(neovm_core::face::FontWeight::Normal);
    refreshed.define("pin-refresh-named", changed);
    assert!(!refreshed.same_snapshot(&table));

    let mut next = FaceResolver::new(&refreshed, 0x00FFFFFF, 0x00000000, 14.0, None);
    pin.apply_to(&mut next);
    {
        let cache = next.composed_sources.borrow();
        assert!(
            cache.named_specs.is_empty(),
            "folds against the old table must re-fold"
        );
        assert!(
            !cache.compositions.is_empty(),
            "input-keyed compositions survive the refresh"
        );
        assert!(
            !cache.entries.is_empty(),
            "interned merged sources survive the refresh"
        );
    }

    // The composition is still reusable: the same anonymous overlay over the
    // same base allocation hits the carried entry rather than re-merging or
    // growing the cache.
    let compositions_before = {
        let cache = next.composed_sources.borrow();
        cache.compositions.values().map(Vec::len).sum::<usize>()
    };
    let again = next.apply_specified_face_over(&base, &overlay, None);
    assert!(again.source_attributes.is_some(), "composition rooted");
    let compositions_after = {
        let cache = next.composed_sources.borrow();
        cache.compositions.values().map(Vec::len).sum::<usize>()
    };
    assert_eq!(compositions_after, compositions_before);
}
