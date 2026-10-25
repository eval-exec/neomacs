use super::*;
use crate::face::{Color, FaceAttrValue, FontWeight, LFaceAttr, SpecifiedColor};

fn source() -> Face {
    let mut face = Face::default();
    face.set_attribute(
        LFaceAttr::Foreground,
        FaceAttrValue::SpecifiedColor {
            specification: SpecifiedColor::parse("#123456"),
            realized: Color::from_hex("#123456"),
        },
    );
    face
}

#[test]
fn generic_default_slot_projection_tracks_equal_basic_winners_without_rekeying_paint() {
    let attributes = source();
    let mut cache = FrameFaceRealizationCache::default();
    cache.prepare(7, FaceTable::new());
    assert_eq!(cache.lookup_uses_default_slot(&attributes), None);
    cache.insert_basic(BasicFace::Default, attributes.clone(), attributes.clone());
    assert_eq!(cache.lookup_uses_default_slot(&attributes), Some(true));
    cache.insert_basic(BasicFace::Margin, attributes.clone(), attributes.clone());
    assert_eq!(cache.lookup(&attributes), Some(&attributes));
    assert_eq!(cache.lookup_uses_default_slot(&attributes), Some(false));
    assert!(cache.generation_matches(7));
    cache.insert_basic(BasicFace::Default, attributes.clone(), attributes.clone());
    assert_eq!(cache.lookup_uses_default_slot(&attributes), Some(true));
    assert!(cache.generation_matches(7));
}

#[test]
fn realization_cache_compares_complete_attributes_within_color_buckets() {
    let source = source();
    let mut first = source.clone();
    first.foreground = Some(Color::rgb(1, 2, 3));
    let mut second_source = source.clone();
    second_source.weight = Some(FontWeight::BOLD);
    let mut second = second_source.clone();
    second.foreground = Some(Color::rgb(4, 5, 6));
    let mut cache = FrameFaceRealizationCache::default();
    cache.insert(source.clone(), first.clone());
    cache.insert(second_source.clone(), second.clone());
    assert_eq!(cache.lookup(&source), Some(&first));
    assert_eq!(cache.lookup(&second_source), Some(&second));
    let mut third = source;
    third.extend = Some(true); // routing hash intentionally excludes this field
    assert!(cache.lookup(&third).is_none());
}

#[test]
fn realization_cache_generation_and_owners_are_independent() {
    let mut first = FrameFaceRealizationCache::default();
    let second = FrameFaceRealizationCache::default();
    let attributes = source();
    first.prepare(1, FaceTable::new());
    first.insert(attributes.clone(), attributes.clone());
    assert!(first.table(1).is_some());
    assert!(first.table(2).is_none());
    assert!(second.lookup(&attributes).is_none());
    first.prepare(2, FaceTable::new());
    assert!(first.lookup(&attributes).is_none());
    first.invalidate();
    assert!(first.table(2).is_none());
}

#[test]
fn realization_cache_uses_structural_lisp_attribute_equality() {
    let mut first = source();
    first.family = Some(Value::string("separately-allocated-family"));
    first.stipple = Some(Value::list(vec![
        Value::fixnum(8),
        Value::fixnum(1),
        Value::string("bits"),
    ]));
    let mut equal = first.clone();
    equal.family = Some(Value::string("separately-allocated-family"));
    equal.stipple = Some(Value::list(vec![
        Value::fixnum(8),
        Value::fixnum(1),
        Value::string("bits"),
    ]));
    assert_ne!(first.family.unwrap().bits(), equal.family.unwrap().bits());
    assert_ne!(first.stipple.unwrap().bits(), equal.stipple.unwrap().bits());
    let mut cache = FrameFaceRealizationCache::default();
    cache.insert(first.clone(), first.clone());
    assert_eq!(cache.lookup(&equal), Some(&first));
    equal.family = Some(Value::string("different-family"));
    assert!(cache.lookup(&equal).is_none());
}

#[test]
fn realization_cache_retags_only_the_immediately_previous_valid_generation() {
    let mut cache = FrameFaceRealizationCache::default();
    let table = FaceTable::new();
    let attributes = source();
    cache.prepare(4, table.clone());
    cache.insert(attributes.clone(), attributes.clone());
    cache.preserve_across_change(4, 5);
    assert!(cache.table(5).unwrap().shares_definitions_with(&table));
    assert_eq!(cache.lookup(&attributes), Some(&attributes));

    // Another frame-local change cannot resurrect a source already stale due
    // to an earlier global change, or one explicitly invalidated by a callback.
    cache.preserve_across_change(6, 7);
    assert!(cache.table(7).is_none());
    assert!(cache.table(5).is_some());
    cache.invalidate();
    cache.preserve_across_change(5, 6);
    assert!(cache.table(6).is_none());
}

#[test]
fn realization_source_identity_detects_copy_on_write_definition_replacement() {
    let original = FaceTable::new();
    let mut replaced = original.clone();
    assert!(original.shares_definitions_with(&replaced));
    replaced.define("callback-changed-face", source());
    assert!(!original.shares_definitions_with(&replaced));
    assert!(!original.shares_definitions_with(&FaceTable::new()));
}

#[test]
fn forced_basic_roles_keep_distinct_results_for_identical_attributes() {
    let attributes = source();
    let mut default = attributes.clone();
    default.foreground = Some(Color::rgb(1, 2, 3));
    let role_only_root = Value::string("forced-default-role-root");
    default.doc = Some(role_only_root);
    let mut margin = attributes.clone();
    margin.foreground = Some(Color::rgb(4, 5, 6));
    let mut cache = FrameFaceRealizationCache::default();
    cache.prepare(1, FaceTable::new());
    cache.insert_basic(BasicFace::Default, attributes.clone(), default.clone());
    cache.insert_basic(BasicFace::Margin, attributes.clone(), margin.clone());
    assert_eq!(
        cache.basic(BasicFace::Default),
        Some(&(attributes.clone(), default))
    );
    assert_eq!(
        cache.basic(BasicFace::Margin),
        Some(&(attributes.clone(), margin.clone()))
    );
    assert_eq!(cache.lookup(&attributes), Some(&margin));
    let mut roots = Vec::new();
    cache.trace_roots(&mut roots);
    assert!(
        roots
            .iter()
            .any(|root| root.bits() == role_only_root.bits())
    );
    cache.preserve_across_change(1, 2);
    assert!(cache.basic(BasicFace::Default).is_some());
    cache.prepare(3, FaceTable::new());
    assert!(cache.basic(BasicFace::Default).is_none());
    assert!(cache.basic(BasicFace::Margin).is_none());
}

#[test]
fn definitions_only_refresh_retains_roles_and_old_realizations_while_new_attributes_miss() {
    let old_source = source();
    let mut new_source = old_source.clone();
    new_source.set_attribute(
        LFaceAttr::Foreground,
        FaceAttrValue::SpecifiedColor {
            specification: SpecifiedColor::parse("#654321"),
            realized: Color::from_hex("#654321"),
        },
    );
    let mut table = FaceTable::new();
    table.define("no-purge-face", old_source.clone());
    let mut cache = FrameFaceRealizationCache::default();
    cache.prepare(1, table.clone());
    cache.insert_basic(BasicFace::Default, old_source.clone(), old_source.clone());
    cache.insert(old_source.clone(), old_source.clone());
    cache.definitions_changed(1, 2, Some("no-purge-face"));
    assert!(cache.generation_matches(2));
    assert!(cache.table(2).is_none());
    assert_eq!(cache.lookup(&old_source), Some(&old_source));
    assert!(cache.basic(BasicFace::Default).is_some());
    table.define("no-purge-face", new_source.clone());
    cache.replace_definitions(table.clone());
    assert!(cache.table(2).unwrap().shares_definitions_with(&table));
    assert_eq!(cache.lookup(&old_source), Some(&old_source));
    assert!(cache.lookup(&new_source).is_none());
    assert!(cache.basic(BasicFace::Default).is_some());
    cache.prepare(3, table);
    assert!(cache.lookup(&old_source).is_none());
    assert!(cache.basic(BasicFace::Default).is_none());
}

#[test]
fn realization_epoch_survives_definition_refresh_but_never_full_retirement() {
    let mut cache = FrameFaceRealizationCache::default();
    assert!(cache.realization_epoch().is_none());
    cache.prepare(1, FaceTable::new());
    let original = cache.realization_epoch().unwrap();
    cache.preserve_across_change(1, 2);
    assert!(cache.retains_epoch(&original));
    cache.definitions_changed(2, 3, Some("epoch-face"));
    assert!(cache.retains_epoch(&original));
    cache.replace_definitions(FaceTable::new());
    assert!(cache.retains_epoch(&original));
    cache.prepare(4, FaceTable::new());
    assert!(!cache.retains_epoch(&original));
    let replacement = cache.realization_epoch().unwrap();
    cache.invalidate();
    assert!(!cache.retains_epoch(&replacement));
    assert!(cache.realization_epoch().is_none());
}

#[test]
fn default_retirement_restores_the_last_equal_role_and_preserves_unrelated_entries() {
    let source = source();
    let mut default = source.clone();
    default.foreground = Some(Color::rgb(1, 2, 3));
    let mut margin = source.clone();
    margin.foreground = Some(Color::rgb(4, 5, 6));
    let mut unrelated = source.clone();
    unrelated.weight = Some(FontWeight::BOLD);
    let mut cache = FrameFaceRealizationCache::default();
    cache.prepare(1, FaceTable::new());
    cache.insert_basic(BasicFace::Default, source.clone(), default.clone());
    cache.insert_basic(BasicFace::Margin, source.clone(), margin.clone());
    cache.insert(unrelated.clone(), unrelated.clone());
    cache.retain_default_lface(Value::vector(vec![Value::string("cached-source")]));
    cache.retire_default();
    assert!(cache.basic(BasicFace::Default).is_none());
    assert!(cache.default_lface().is_none());
    assert_eq!(cache.lookup(&source), Some(&margin));
    assert_eq!(cache.lookup(&unrelated), Some(&unrelated));

    // A replacement default becomes the latest equal lookup, then retirement
    // reveals the older independent basic role again.
    cache.insert_basic(BasicFace::Default, source.clone(), default.clone());
    assert_eq!(cache.lookup(&source), Some(&default));
    cache.retire_default();
    assert_eq!(cache.lookup(&source), Some(&margin));

    let mut isolated = FrameFaceRealizationCache::default();
    isolated.insert_basic(BasicFace::Default, source.clone(), default);
    isolated.insert(unrelated.clone(), unrelated.clone());
    isolated.retire_default();
    assert!(isolated.lookup(&source).is_none());
    assert_eq!(isolated.lookup(&unrelated), Some(&unrelated));
}

#[test]
fn forced_default_retirement_restores_ordinary_predecessor_and_traces_it() {
    let mut ordinary_source = source();
    ordinary_source.family = Some(Value::string("ordinary-predecessor-source"));
    let mut ordinary_paint = ordinary_source.clone();
    ordinary_paint.foreground = Some(Color::rgb(1, 2, 3));
    ordinary_paint.stipple = Some(Value::string("ordinary-predecessor-paint"));
    let mut forced_paint = ordinary_source.clone();
    forced_paint.foreground = Some(Color::rgb(4, 5, 6));
    let mut cache = FrameFaceRealizationCache::default();
    cache.prepare(1, FaceTable::new());
    cache.insert(ordinary_source.clone(), ordinary_paint.clone());
    cache.insert_basic(
        BasicFace::Default,
        ordinary_source.clone(),
        forced_paint.clone(),
    );
    assert_eq!(cache.lookup(&ordinary_source), Some(&forced_paint));
    let mut roots = Vec::new();
    cache.trace_roots(&mut roots);
    assert!(
        roots
            .iter()
            .any(|value| value.bits() == ordinary_source.family.unwrap().bits())
    );
    assert!(
        roots
            .iter()
            .any(|value| value.bits() == ordinary_paint.stipple.unwrap().bits())
    );
    cache.retire_default();
    let mut changed = ordinary_source.clone();
    changed.weight = Some(FontWeight::BOLD);
    cache.insert_basic(BasicFace::Default, changed.clone(), changed);
    assert_eq!(cache.lookup(&ordinary_source), Some(&ordinary_paint));

    // Even an identical paint written by a later role has distinct ownership.
    cache.insert_basic(
        BasicFace::Default,
        ordinary_source.clone(),
        forced_paint.clone(),
    );
    cache.insert_basic(
        BasicFace::Margin,
        ordinary_source.clone(),
        forced_paint.clone(),
    );
    cache.retire_default();
    assert_eq!(cache.lookup(&ordinary_source), Some(&forced_paint));
}
