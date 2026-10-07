use super::*;

#[test]
fn worker_query_budget_reserves_distinct_requests_before_native_lookup() {
    let mut resolver = FontResolver::platform_default();
    let size = FontSelectionSize::new(
        14.0,
        neomacs_display_protocol::DeviceScale::new(1.0).unwrap(),
    );
    let policy = Arc::new(
        FrozenCharacterPolicies::capture(&[("monospace", '中', 400, false, size)], 4096).unwrap(),
    );
    let other = Arc::new(
        FrozenCharacterPolicies::capture(&[("monospace", '文', 400, false, size)], 4096).unwrap(),
    );
    assert!(resolver.worker_policy_fits_cache(&policy, 1));
    resolver.install_worker_policy(policy.clone());
    assert!(resolver.worker_policy_fits_cache(&policy, 1));
    assert!(!resolver.worker_policy_fits_cache(&other, 1));
    assert!(resolver.worker_policy_fits_cache(&other, 2));
    resolver.install_worker_policy(other);
    assert!(!resolver.worker_policy_fits_cache(&policy, 1));
    assert!(resolver.worker_policy_fits_cache(&policy, 2));
    resolver.clear_caches();
    assert!(resolver.worker_policy_fits_cache(&policy, 1));
}
