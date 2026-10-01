use super::{FamilySides, ModifierSides, TrackedModifier, TrackedSide};
use neomacs_display_protocol::ModifierSideState;

#[test]
fn aggregate_up_reports_no_family() {
    let sides = ModifierSides::default();
    let raw = sides.to_raw(false, false, false, false);
    assert_eq!(raw.ctrl, None);
    assert_eq!(raw.command, None);
    assert_eq!(raw.option, None);
}

#[test]
fn unobserved_sides_fall_back_to_gnus_left_rule() {
    let sides = ModifierSides::default();
    let raw = sides.to_raw(false, false, true, false);
    assert_eq!(raw.command, Some(ModifierSideState::Unknown));
}

#[test]
fn observed_left_side_maps_to_left_only() {
    let mut sides = ModifierSides::default();
    sides.observe_key(TrackedModifier::Command, TrackedSide::Left, true);
    let raw = sides.to_raw(false, false, true, false);
    assert_eq!(raw.command, Some(ModifierSideState::LeftOnly));
}

#[test]
fn observed_right_side_maps_to_right_only() {
    let mut sides = ModifierSides::default();
    sides.observe_key(TrackedModifier::Option, TrackedSide::Right, true);
    let raw = sides.to_raw(false, false, false, true);
    assert_eq!(raw.option, Some(ModifierSideState::RightOnly));
}

#[test]
fn both_sides_observed_down_map_to_both() {
    let mut sides = ModifierSides::default();
    sides.observe_key(TrackedModifier::Option, TrackedSide::Left, true);
    sides.observe_key(TrackedModifier::Option, TrackedSide::Right, true);
    let raw = sides.to_raw(false, false, false, true);
    assert_eq!(raw.option, Some(ModifierSideState::Both));
}

#[test]
fn released_side_stops_claiming_its_side() {
    let mut sides = ModifierSides::default();
    sides.observe_key(TrackedModifier::Command, TrackedSide::Left, true);
    sides.observe_key(TrackedModifier::Command, TrackedSide::Left, false);
    sides.observe_key(TrackedModifier::Command, TrackedSide::Right, true);
    let raw = sides.to_raw(false, false, true, false);
    assert_eq!(raw.command, Some(ModifierSideState::RightOnly));
}

#[test]
fn reset_forgets_every_observation() {
    let mut sides = ModifierSides::default();
    sides.observe_key(TrackedModifier::Command, TrackedSide::Left, true);
    sides.observe_key(TrackedModifier::Option, TrackedSide::Right, true);
    sides.reset();
    let raw = sides.to_raw(false, false, true, true);
    assert_eq!(raw.command, Some(ModifierSideState::Unknown));
    assert_eq!(raw.option, Some(ModifierSideState::Unknown));
}

#[test]
fn families_track_independently() {
    let mut sides = ModifierSides::default();
    sides.observe_key(TrackedModifier::Control, TrackedSide::Left, true);
    let raw = sides.to_raw(true, true, true, true);
    assert_eq!(raw.ctrl, Some(ModifierSideState::LeftOnly));
    assert_eq!(raw.command, Some(ModifierSideState::Unknown));
    assert_eq!(raw.option, Some(ModifierSideState::Unknown));
}

#[test]
fn family_sides_default_is_unobserved() {
    let family = FamilySides::default();
    assert_eq!(family.sides(true), Some(ModifierSideState::Unknown));
    assert_eq!(family.sides(false), None);
}
