use crate::{
    HostProvenance, LOCAL_HIGH_LOAD_THRESHOLD, MachinePolicy, MeasurementProvenance, PerfError,
    ScenarioId, collect_host_provenance_with_loadavg, cpu_list_contains, parse_loadavg_1m,
    validate_cohort_load_policy, validate_machine_policy,
};

fn host() -> HostProvenance {
    HostProvenance {
        operating_system: "linux".to_string(),
        architecture: "x86_64".to_string(),
        kernel_release: Some("test-kernel".to_string()),
        cpu_model: Some("test-cpu".to_string()),
        logical_cpu_count: 8,
        allowed_cpus: Some("0-2,4,6-7".to_string()),
        selected_cpu: Some(4),
        scaling_governor: Some("performance".to_string()),
        perf_event_paranoid: Some(1),
        continuous_integration: false,
        loadavg: Some("0.42 0.50 0.60".to_string()),
        provenance: Some(MeasurementProvenance::Calibrated),
    }
}

#[test]
fn host_provenance_captures_linux_loadavg_and_deserializes_legacy_artifacts() {
    let collected = crate::host::collect_host_provenance(&MachinePolicy::default());
    if cfg!(target_os = "linux") {
        let loadavg = collected
            .loadavg
            .as_deref()
            .expect("Linux host provenance must record /proc/loadavg");
        assert_eq!(
            loadavg.split_whitespace().count(),
            3,
            "unexpected /proc/loadavg triplet format: {loadavg:?}"
        );
    }

    let legacy_json = serde_json::json!({
        "operating_system": "linux",
        "architecture": "x86_64",
        "kernel_release": "6.0-test",
        "cpu_model": "test-cpu",
        "logical_cpu_count": 8,
        "allowed_cpus": "0-7",
        "selected_cpu": 0,
        "scaling_governor": "performance",
        "perf_event_paranoid": 1,
        "continuous_integration": false
    });
    let decoded: HostProvenance =
        serde_json::from_value(legacy_json).expect("legacy HostProvenance without loadavg");
    assert_eq!(decoded.loadavg, None);
}

#[test]
fn linux_cpu_list_parser_handles_singletons_and_ranges() {
    for cpu in [0, 1, 2, 4, 6, 7] {
        assert!(cpu_list_contains("0-2,4,6-7", cpu));
    }
    for cpu in [3, 5, 8] {
        assert!(!cpu_list_contains("0-2,4,6-7", cpu));
    }
}

#[test]
fn machine_policy_rejects_disallowed_cpus_and_governor_drift() {
    let disallowed = MachinePolicy {
        cpu: Some(5),
        required_governor: Some("performance".to_string()),
        allow_local: false,
    };
    let error = validate_machine_policy(&disallowed, &host())
        .expect_err("a CPU outside the process cpuset must reject the run");
    assert!(error.contains("outside the process allowance"));

    let wrong_governor = MachinePolicy {
        cpu: Some(4),
        required_governor: Some("powersave".to_string()),
        allow_local: false,
    };
    let error = validate_machine_policy(&wrong_governor, &host())
        .expect_err("governor drift must reject the run");
    assert!(error.contains("required powersave, found performance"));

    let controlled = MachinePolicy {
        cpu: Some(4),
        required_governor: Some("performance".to_string()),
        allow_local: false,
    };
    validate_machine_policy(&controlled, &host()).expect("controlled host matches the policy");
}

#[test]
fn cohort_load_gate_refuses_high_load_and_stamps_local_untrusted_on_override() {
    assert_eq!(parse_loadavg_1m("0.51 1.56 2.37"), Some(0.51));
    assert_eq!(parse_loadavg_1m("47.81 41.10 37.12"), Some(47.81));
    assert_eq!(parse_loadavg_1m(""), None);
    assert_eq!(parse_loadavg_1m("-1.0 0.0 0.0"), None);
    assert_eq!(parse_loadavg_1m("NaN 0.0 0.0"), None);

    // Idle host without --allow-local is stamped calibrated and permitted.
    let idle_policy = MachinePolicy::default();
    let idle_host =
        collect_host_provenance_with_loadavg(&idle_policy, Some("0.68 0.90 1.12".to_string()));
    assert_eq!(idle_host.loadavg.as_deref(), Some("0.68 0.90 1.12"));
    assert_eq!(
        idle_host.provenance,
        Some(MeasurementProvenance::Calibrated)
    );
    validate_cohort_load_policy(ScenarioId::ScrollingLongLines, &idle_policy, &idle_host)
        .expect("idle host below threshold passes cohort gate");

    // Loaded host without --allow-local MUST refuse cohort-gated scenarios.
    let loaded_host =
        collect_host_provenance_with_loadavg(&idle_policy, Some("14.25 12.10 10.05".to_string()));
    assert_eq!(
        loaded_host.provenance,
        Some(MeasurementProvenance::LocalUntrusted)
    );
    let err =
        validate_cohort_load_policy(ScenarioId::ScrollingLongLines, &idle_policy, &loaded_host)
            .expect_err("high-load local run without --allow-local must fail closed");
    match err {
        PerfError::HighLoadLocalRefused {
            scenario,
            loadavg_1m,
            threshold,
            loadavg,
        } => {
            assert_eq!(scenario, ScenarioId::ScrollingLongLines);
            assert!((loadavg_1m - 14.25).abs() < f64::EPSILON);
            assert!((threshold - LOCAL_HIGH_LOAD_THRESHOLD).abs() < f64::EPSILON);
            assert_eq!(loadavg, "14.25 12.10 10.05");
        }
        other => panic!("expected HighLoadLocalRefused, got {other:?}"),
    }

    // Non-cohort scenarios are not blocked by the cohort load refusal gate.
    validate_cohort_load_policy(ScenarioId::Scrolling, &idle_policy, &loaded_host)
        .expect("non-cohort scenarios remain unblocked");

    // Explicit --allow-local permits execution and ALWAYS stamps local-UNTRUSTED,
    // even if the host happens to be quiet when --allow-local was passed.
    let override_policy = MachinePolicy {
        allow_local: true,
        ..MachinePolicy::default()
    };
    let override_loaded = collect_host_provenance_with_loadavg(
        &override_policy,
        Some("14.25 12.10 10.05".to_string()),
    );
    assert_eq!(
        override_loaded.provenance,
        Some(MeasurementProvenance::LocalUntrusted)
    );
    assert_eq!(
        override_loaded.provenance.unwrap().to_string(),
        "local-UNTRUSTED"
    );
    validate_cohort_load_policy(
        ScenarioId::ScrollingLongLines,
        &override_policy,
        &override_loaded,
    )
    .expect("--allow-local overrides the refusal while stamping local-UNTRUSTED");

    let override_idle =
        collect_host_provenance_with_loadavg(&override_policy, Some("0.20 0.25 0.30".to_string()));
    assert_eq!(
        override_idle.provenance,
        Some(MeasurementProvenance::LocalUntrusted)
    );
}
