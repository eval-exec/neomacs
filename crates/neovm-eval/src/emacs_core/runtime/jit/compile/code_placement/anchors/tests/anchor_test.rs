use super::*;

#[test]
fn switch64_rounds_anchor_down_while_preserving_required_alignment() {
    for alignment in [1, 2, 4, 8, 16, 32, 64, 128, 4096] {
        for anchor in 0..256 {
            let offset = placement_for_anchor(alignment, Some(anchor)).unwrap();
            offset.validate_alignment(alignment).unwrap();
            let within_line = (offset.bytes() as u64 + u64::from(anchor)) % 64;
            assert!((64 - within_line) % 64 < alignment);
            if alignment == 1 {
                assert_eq!(within_line, 0);
            }
        }
    }
    assert_eq!(placement_for_anchor(8, Some(1)).unwrap().bytes(), 56);
    assert_eq!(placement_for_anchor(8, Some(60)).unwrap().bytes(), 0);
    assert_eq!(placement_for_anchor(8, None).unwrap().bytes(), 0);
    assert!(placement_for_anchor(3, Some(1)).is_err());
}

#[test]
fn switch64_configuration_uses_no_machine_cfg_metadata() {
    let _scope =
        super::super::config_scope_for_test(PlacementConfig::Policy(PlacementPolicy::Switch64));
    assert_eq!(
        super::super::parse_config(Some("switch64"), None).unwrap(),
        PlacementConfig::Policy(PlacementPolicy::Switch64)
    );
    assert!(!super::super::needs_machine_cfg());
    assert_eq!(
        super::super::placement_for(
            PlacementConfig::Policy(PlacementPolicy::Switch64),
            8,
            &[(100, 50)],
        )
        .unwrap()
        .unwrap()
        .bytes(),
        0,
        "unmarked functions must ignore unrelated backward edges",
    );
}
