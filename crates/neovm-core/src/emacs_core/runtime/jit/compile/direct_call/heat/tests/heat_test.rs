use super::DirectSelfHeat;

#[test]
fn modes_parse_explicitly_and_other_settings_preserve_the_original_policy() {
    assert_eq!(DirectSelfHeat::parse(Some("seen")), DirectSelfHeat::Seen);
    assert_eq!(DirectSelfHeat::parse(Some("hot")), DirectSelfHeat::Hot);
    assert_eq!(DirectSelfHeat::parse(Some(" SEEN ")), DirectSelfHeat::Seen);
    assert_eq!(DirectSelfHeat::parse(Some("HOT")), DirectSelfHeat::Hot);
    for value in [
        None,
        Some(""),
        Some("off"),
        Some("0"),
        Some("on"),
        Some("1000"),
    ] {
        assert_eq!(DirectSelfHeat::parse(value), DirectSelfHeat::Off);
    }
}

#[test]
fn off_admits_cold_and_hot_sources_without_a_threshold_requirement() {
    for heat in [0, 1, 999, 1000, u32::MAX] {
        assert!(DirectSelfHeat::Off.allows(heat, u32::MAX));
    }
}

#[test]
fn seen_declines_only_zero_heat_even_when_the_normal_threshold_is_higher() {
    assert!(!DirectSelfHeat::Seen.allows(0, 1000));
    for heat in [1, 999, 1000, u32::MAX] {
        assert!(DirectSelfHeat::Seen.allows(heat, 1000));
    }
    assert!(!DirectSelfHeat::Seen.allows(0, 0));
    assert!(DirectSelfHeat::Seen.allows(1, u32::MAX));
}

#[test]
fn hot_follows_the_normal_dispatch_threshold_and_its_overrides() {
    for heat in [0, 1, 999] {
        assert!(!DirectSelfHeat::Hot.allows(heat, 1000));
    }
    for heat in [1000, u32::MAX] {
        assert!(DirectSelfHeat::Hot.allows(heat, 1000));
    }
    assert!(!DirectSelfHeat::Hot.allows(0, 1));
    assert!(DirectSelfHeat::Hot.allows(1, 1));
    assert!(DirectSelfHeat::Hot.allows(0, 0));
}
