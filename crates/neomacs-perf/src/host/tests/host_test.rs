use crate::{HostProvenance, MachinePolicy, cpu_list_contains, validate_machine_policy};

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
    }
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
    };
    let error = validate_machine_policy(&disallowed, &host())
        .expect_err("a CPU outside the process cpuset must reject the run");
    assert!(error.contains("outside the process allowance"));

    let wrong_governor = MachinePolicy {
        cpu: Some(4),
        required_governor: Some("powersave".to_string()),
    };
    let error = validate_machine_policy(&wrong_governor, &host())
        .expect_err("governor drift must reject the run");
    assert!(error.contains("required powersave, found performance"));

    let controlled = MachinePolicy {
        cpu: Some(4),
        required_governor: Some("performance".to_string()),
    };
    validate_machine_policy(&controlled, &host()).expect("controlled host matches the policy");
}
