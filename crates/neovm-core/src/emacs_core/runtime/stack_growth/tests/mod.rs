use super::*;

/// GNU's floor is unconditional: `eval.c:2587-2588` raises a requested limit
/// below 100 before it signals, so a handler has room to run.
#[test]
fn every_host_keeps_gnus_floor_under_a_too_small_request() {
    for policy in [
        LispDepthLimit::HostRecovers,
        LispDepthLimit::HostTraps { ceiling: 1600 },
    ] {
        assert_eq!(policy.clamp(0), 100, "{policy:?}");
        assert_eq!(policy.clamp(-9), 100, "{policy:?}");
        assert_eq!(policy.clamp(99), 100, "{policy:?}");
        assert_eq!(policy.clamp(100), 100, "{policy:?}");
    }
}

#[test]
fn a_host_that_reports_an_overrun_lets_lisp_set_any_limit() {
    let policy = LispDepthLimit::HostRecovers;
    assert_eq!(policy.clamp(1600), 1600);
    assert_eq!(policy.clamp(100_000), 100_000);
    assert_eq!(policy.clamp(i64::from(u32::MAX)), u32::MAX as usize);
}

/// The whole point: on a trapping host a Lisp `setq` cannot raise the limit
/// past what the shadow stack survives, because the overrun there is a wasm
/// trap rather than a signalled error.
#[test]
fn a_trapping_host_caps_a_limit_lisp_tries_to_raise() {
    let policy = LispDepthLimit::HostTraps { ceiling: 1600 };
    assert_eq!(
        policy.clamp(800),
        800,
        "below the ceiling Lisp still decides"
    );
    assert_eq!(policy.clamp(1600), 1600);
    assert_eq!(policy.clamp(1601), 1600);
    assert_eq!(policy.clamp(100_000), 1600);
    assert_eq!(policy.clamp(i64::MAX), 1600);
}

/// A ceiling below GNU's floor would make the floor unreachable; assert the
/// two compose in the order that keeps a handler runnable.
#[test]
fn a_ceiling_below_the_floor_still_yields_the_ceiling() {
    let policy = LispDepthLimit::HostTraps { ceiling: 50 };
    assert_eq!(policy.clamp(10), 50);
    assert_eq!(policy.clamp(10_000), 50);
}

#[test]
fn the_compiled_policy_matches_the_target_family() {
    let expected_traps = cfg!(target_family = "wasm");
    assert_eq!(
        matches!(LispDepthLimit::CURRENT, LispDepthLimit::HostTraps { .. }),
        expected_traps,
        "only a host whose overrun is an uncatchable trap may cap Lisp's limit",
    );
}
