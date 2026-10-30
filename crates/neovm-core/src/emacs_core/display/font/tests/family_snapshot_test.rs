use super::*;
use crate::emacs_core::intern::{intern_lisp_string, resolve_sym_lisp_string};

fn row(name: &str, alternatives: &[&str]) -> (SymId, Vec<SymId>) {
    (
        intern(name),
        alternatives.iter().map(|name| intern(name)).collect(),
    )
}

#[test]
fn frozen_families_find_late_alist_rows_and_keep_long_alternative_order() {
    let mut source: AlternativeFontFamilyAlist = (0..80)
        .map(|index| {
            let name = format!("unrelated-{index}");
            let name = intern(&name);
            (name, vec![name])
        })
        .collect();
    let expected: Vec<_> = (0..96).map(|index| format!("target-{index}")).collect();
    source.push((
        intern("Requested"),
        expected.iter().map(|name| intern(name)).collect(),
    ));
    let owner = AlternativeFontFamilyState::from_source(source);
    let captured = owner.capture("requested");
    assert_eq!(captured.iter().collect::<Vec<_>>(), expected);
    assert_eq!(captured.iter().nth(32), Some("target-32"));
    assert_eq!(captured.iter().nth(95), Some("target-95"));
}

#[test]
fn frozen_families_capture_shares_the_owner_array_and_each_name() {
    let owner = AlternativeFontFamilyState::from_source(vec![row(
        "Fixture",
        &["Fixture", "Second", "Third"],
    )]);
    let first = owner.capture("Fixture");
    let second = owner.capture("fixture");
    assert!(Arc::ptr_eq(&owner.native[0].1.0, &first.0));
    assert!(Arc::ptr_eq(&first.0, &second.0));
    assert!(Arc::ptr_eq(&first.0[1], &second.0[1]));
    assert_eq!(Arc::strong_count(&first.0), 3);
}

#[test]
fn frozen_families_survive_owner_replacement_drop_and_detached_thread() {
    let mut owner =
        AlternativeFontFamilyState::from_source(vec![row("Fixture", &["Fixture", "Before"])]);
    let before = owner.capture("Fixture");
    owner = AlternativeFontFamilyState::from_source(vec![row("Fixture", &["Fixture", "After"])]);
    let after = owner.capture("Fixture");
    assert!(!Arc::ptr_eq(&before.0, &after.0));
    drop(owner);
    let retained = std::thread::spawn(move || {
        (
            before.iter().map(str::to_owned).collect::<Vec<_>>(),
            after.iter().map(str::to_owned).collect::<Vec<_>>(),
        )
    })
    .join()
    .unwrap();
    assert_eq!(retained.0, ["Fixture", "Before"]);
    assert_eq!(retained.1, ["Fixture", "After"]);
}

#[test]
fn frozen_families_keep_exact_source_bytes_and_native_unibyte_projection() {
    let alias = LispString::from_unibyte(b"FiXtUrE".to_vec());
    let raw = LispString::from_unibyte(vec![b'A', 0xff, b'B']);
    let source = vec![(
        intern_lisp_string(&alias),
        vec![intern_lisp_string(&alias), intern_lisp_string(&raw)],
    )];
    let owner = AlternativeFontFamilyState::from_source(source);
    assert_eq!(
        resolve_sym_lisp_string(owner.source[0].0).as_bytes(),
        b"FiXtUrE"
    );
    assert_eq!(
        resolve_sym_lisp_string(owner.source[0].1[1]).as_bytes(),
        &[b'A', 0xff, b'B']
    );
    let captured = owner.capture("  fixture  ");
    assert_eq!(
        captured.iter().collect::<Vec<_>>(),
        vec!["FiXtUrE", "A\u{FFFD}B"]
    );
}

#[test]
fn frozen_families_match_exact_name_bytes_with_ascii_case_folding_only() {
    let owner = AlternativeFontFamilyState::from_source(vec![
        row("Älias", &["Älias", "Exact"]),
        row("ASCII", &["ASCII", "Case"]),
    ]);
    assert_eq!(
        owner.capture("Älias").iter().collect::<Vec<_>>(),
        ["Älias", "Exact"]
    );
    assert_eq!(owner.capture("älias").iter().collect::<Vec<_>>(), ["älias"]);
    assert_eq!(
        owner.capture("aScIi").iter().collect::<Vec<_>>(),
        ["ASCII", "Case"]
    );
    assert_eq!(
        owner.capture("  Unknown  ").iter().collect::<Vec<_>>(),
        ["Unknown"]
    );
    assert_eq!(owner.capture("  ").iter().count(), 0);
}

#[test]
fn real_family_setter_publishes_a_new_epoch_and_preserves_old_snapshot() {
    crate::test_utils::init_test_tracing();
    let _eval = crate::emacs_core::Context::new();
    let before = crate::emacs_core::fontset::fontset_generation();
    crate::emacs_core::xfaces::builtin_internal_set_alternative_font_family_alist(vec![
        Value::list(vec![Value::list(vec![
            Value::string("Fixture"),
            Value::string("Before"),
        ])]),
    ])
    .unwrap();
    let captured = capture_alternative_font_families("fixture").unwrap();
    let first_epoch = crate::emacs_core::fontset::fontset_generation();
    assert_ne!(before, first_epoch);
    crate::emacs_core::xfaces::builtin_internal_set_alternative_font_family_alist(vec![
        Value::list(vec![Value::list(vec![
            Value::string("Fixture"),
            Value::string("After"),
        ])]),
    ])
    .unwrap();
    let current = capture_alternative_font_families("fixture").unwrap();
    let second_epoch = crate::emacs_core::fontset::fontset_generation();
    assert_ne!(first_epoch, second_epoch);
    assert!(!Arc::ptr_eq(&captured.0, &current.0));
    assert_eq!(captured.iter().collect::<Vec<_>>(), ["Fixture", "Before"]);
    assert_eq!(current.iter().collect::<Vec<_>>(), ["Fixture", "After"]);
    assert_eq!(
        alternative_font_families("fixture"),
        current.iter().map(str::to_owned).collect::<Vec<_>>()
    );
}
