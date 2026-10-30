use super::super::{
    FontDefinitionMetadata, FontRepertory, FontsetAddMode, FontsetTarget, StoredFontSpec,
    restore_fontset_registry, snapshot_fontset_registry,
};
use super::*;
use crate::emacs_core::intern::intern;

fn family_entry(family: &str) -> FontSpecEntry {
    FontSpecEntry::Font(StoredFontSpec {
        family: Some(intern(family)),
        registry: None,
        lang: None,
        weight: None,
        slant: None,
        width: None,
        definition: None,
    })
}

fn revision() -> FontSelectionRevision {
    FontSelectionRevision::try_from(1).unwrap()
}

fn family(entry: &FontsetRenderingEntry) -> Option<&str> {
    match entry {
        FontsetRenderingEntry::Font(spec) => spec.family.as_deref(),
        FontsetRenderingEntry::ExplicitNone => None,
    }
}

#[test]
fn shared_capture_preserves_long_specific_and_fallback_chains() {
    let mut data = FontsetData::default();
    for index in 0..80 {
        data.update_target(
            FontsetTarget::Range('好' as u32, '好' as u32),
            family_entry(&format!("specific-{index}")),
            FontsetAddMode::Append,
        );
        data.update_target(
            FontsetTarget::Fallback,
            family_entry(&format!("fallback-{index}")),
            FontsetAddMode::Append,
        );
    }
    let captured = CapturedFontsetChain::from_data(revision(), &data, '好');
    let names: Vec<_> = captured.iter().map(family).collect();
    assert_eq!(names.len(), 160);
    assert_eq!(names[0], Some("specific-0"));
    assert_eq!(names[32], Some("specific-32"));
    assert_eq!(names[79], Some("specific-79"));
    assert_eq!(names[80], Some("fallback-0"));
    assert_eq!(names[159], Some("fallback-79"));
}

#[test]
fn shared_capture_exposes_stop_after_more_than_thirty_two_entries() {
    let mut data = FontsetData::default();
    for index in 0..40 {
        data.update_target(
            FontsetTarget::Fallback,
            family_entry(&format!("candidate-{index}")),
            FontsetAddMode::Append,
        );
    }
    data.update_target(
        FontsetTarget::Fallback,
        FontSpecEntry::ExplicitNone,
        FontsetAddMode::Append,
    );
    data.update_target(
        FontsetTarget::Fallback,
        family_entry("unreachable"),
        FontsetAddMode::Append,
    );
    let captured = CapturedFontsetChain::from_data(revision(), &data, '好');
    let mut attempted = Vec::new();
    let mut terminal = None;
    for entry in captured.iter() {
        match entry {
            FontsetRenderingEntry::Font(spec) => attempted.push(spec.family.as_deref()),
            FontsetRenderingEntry::ExplicitNone => {
                terminal = Some(entry);
                break;
            }
        }
    }
    assert_eq!(attempted.len(), 40);
    assert_eq!(attempted.last(), Some(&Some("candidate-39")));
    assert_eq!(terminal, Some(&FontsetRenderingEntry::ExplicitNone));
    assert!(!attempted.contains(&Some("unreachable")));
}

#[test]
fn shared_capture_clones_only_owner_references_and_shares_range_splits() {
    let mut data = FontsetData::default();
    data.update_target(
        FontsetTarget::Range('a' as u32, 'z' as u32),
        family_entry("range"),
        FontsetAddMode::Overwrite,
    );
    data.update_target(
        FontsetTarget::Fallback,
        family_entry("fallback"),
        FontsetAddMode::Overwrite,
    );
    let first = CapturedFontsetChain::from_data(revision(), &data, 'a');
    let second = CapturedFontsetChain::from_data(revision(), &data, 'a');
    assert!(Arc::ptr_eq(&first.specific.0, &second.specific.0));
    assert!(Arc::ptr_eq(&first.fallback.0, &second.fallback.0));
    assert!(Arc::ptr_eq(&first.specific.0, &data.ranges[0].entries.0));
    assert_eq!(Arc::strong_count(&first.specific.0), 3);
    data.update_target(
        FontsetTarget::Range('m' as u32, 'm' as u32),
        family_entry("middle"),
        FontsetAddMode::Overwrite,
    );
    assert!(Arc::ptr_eq(&data.ranges[0].entries.0, &first.specific.0));
    assert!(Arc::ptr_eq(&data.ranges[2].entries.0, &first.specific.0));
    assert!(!Arc::ptr_eq(&data.ranges[1].entries.0, &first.specific.0));
    assert_eq!(family(first.iter().next().unwrap()), Some("range"));
}

#[test]
fn detached_names_survive_owner_mutation_and_a_thread_without_an_evaluator() {
    let mut data = FontsetData::default();
    data.update_target(
        FontsetTarget::Fallback,
        family_entry("before"),
        FontsetAddMode::Overwrite,
    );
    let captured = CapturedFontsetChain::from_data(revision(), &data, '好');
    data.update_target(
        FontsetTarget::Fallback,
        family_entry("after"),
        FontsetAddMode::Overwrite,
    );
    drop(data);
    let names = std::thread::spawn(move || {
        captured
            .iter()
            .map(|entry| family(entry).map(str::to_owned))
            .collect::<Vec<_>>()
    })
    .join()
    .unwrap();
    assert_eq!(names, vec![Some("before".to_owned())]);
}

#[test]
fn shared_capture_does_not_clone_informational_repertory() {
    let FontSpecEntry::Font(mut original) = family_entry("fixture") else {
        unreachable!()
    };
    original.definition = Some(FontDefinitionMetadata {
        encoding: intern("ascii"),
        repertory: Some(FontRepertory::CharTableRanges(vec![(0, 100); 1024])),
    });
    let mut data = FontsetData::default();
    data.update_target(
        FontsetTarget::Fallback,
        FontSpecEntry::Font(original),
        FontsetAddMode::Overwrite,
    );
    let captured = CapturedFontsetChain::from_data(revision(), &data, '好');
    let owner = data.fallback.as_ref().unwrap();
    assert!(Arc::ptr_eq(&captured.fallback.0, &owner.0));
    let FontSpecEntry::Font(retained) = owner[0].original() else {
        unreachable!()
    };
    assert!(retained.definition.is_some());
    let FontsetRenderingEntry::Font(rendering) = captured.iter().next().unwrap() else {
        unreachable!()
    };
    assert_eq!(rendering.family.as_deref(), Some("fixture"));
    assert_eq!(rendering.registry, None);
    assert_eq!(rendering.lang, None);
}

#[test]
fn public_capture_binds_owner_generation_and_restore_keeps_detached_data_alive() {
    super::super::reset_fontset_registry();
    {
        let mut slot = registry().write().unwrap();
        slot.update_target(
            &fontset_name_spelling(DEFAULT_FONTSET_NAME),
            FontsetTarget::Fallback,
            family_entry("before"),
            FontsetAddMode::Overwrite,
        );
    }
    let before = capture_default_fontset_chain('好').unwrap();
    let saved = snapshot_fontset_registry();
    {
        let mut slot = registry().write().unwrap();
        slot.update_target(
            &fontset_name_spelling(DEFAULT_FONTSET_NAME),
            FontsetTarget::Fallback,
            family_entry("after"),
            FontsetAddMode::Overwrite,
        );
    }
    let after = capture_default_fontset_chain('好').unwrap();
    assert_ne!(before.revision(), after.revision());
    assert_eq!(family(before.iter().next().unwrap()), Some("before"));
    assert_eq!(family(after.iter().next().unwrap()), Some("after"));
    restore_fontset_registry(saved);
    let restored = capture_default_fontset_chain('好').unwrap();
    assert_eq!(restored.revision(), before.revision());
    assert_eq!(family(restored.iter().next().unwrap()), Some("before"));
    assert_eq!(family(after.iter().next().unwrap()), Some("after"));
}

#[test]
fn revision_rejects_uninitialized_authority() {
    assert_eq!(
        FontSelectionRevision::try_from(0),
        Err(FontsetCaptureError::InvalidRevision)
    );
    static_assertions::assert_eq_size!(FontSelectionRevision, u64);
}

#[test]
fn native_font_names_borrow_utf8_and_convert_real_unibyte_names() {
    use crate::emacs_core::intern::intern_lisp_string;
    use crate::heap_types::LispString;

    assert!(matches!(
        native_font_name(intern("Noto Sans CJK SC")),
        Cow::Borrowed("Noto Sans CJK SC")
    ));
    let raw_name = LispString::from_unibyte(vec![b'A', 0xff, b'B']);
    let raw_symbol = intern_lisp_string(&raw_name);
    assert_eq!(
        resolve_sym_lisp_string(raw_symbol).as_bytes(),
        &[b'A', 0xff, b'B']
    );
    assert!(matches!(native_font_name(raw_symbol), Cow::Owned(ref name) if name == "A\u{FFFD}B"));
}

#[test]
fn raw_unibyte_setter_and_snapshot_restore_keep_exact_informational_names() {
    use crate::emacs_core::intern::resolve_sym_lisp_string;
    use crate::emacs_core::value::Value;
    use crate::heap_types::LispString;

    crate::test_utils::init_test_tracing();
    super::super::reset_fontset_registry();
    let raw_bytes = vec![b'A', 0xff, b'B'];
    let raw_value = Value::heap_string(LispString::from_unibyte(raw_bytes.clone()));
    super::super::set_fontset_font(
        &Value::T,
        &Value::fixnum('好' as i64),
        &raw_value,
        None,
        None,
        None,
        Some(&Value::NIL),
    )
    .expect("raw-unibyte font names remain valid fontset inputs");
    let captured = capture_default_fontset_chain('好').unwrap();
    assert_eq!(family(captured.iter().next().unwrap()), Some("A\u{FFFD}B"));
    let saved = snapshot_fontset_registry();
    let original =
        super::super::matching_entries_for_fontset(super::super::DEFAULT_FONTSET_NAME, '好');
    let FontSpecEntry::Font(spec) = &original[0] else {
        unreachable!()
    };
    let symbol = spec.family.unwrap();
    assert_eq!(resolve_sym_lisp_string(symbol).as_bytes(), raw_bytes);
    super::super::reset_fontset_registry();
    restore_fontset_registry(saved);
    let restored = capture_default_fontset_chain('好').unwrap();
    assert_eq!(restored, captured);
    let original =
        super::super::matching_entries_for_fontset(super::super::DEFAULT_FONTSET_NAME, '好');
    let FontSpecEntry::Font(spec) = &original[0] else {
        unreachable!()
    };
    assert_eq!(
        resolve_sym_lisp_string(spec.family.unwrap()).as_bytes(),
        raw_bytes
    );
    assert_eq!(family(restored.iter().next().unwrap()), Some("A\u{FFFD}B"));
}
