use super::*;
use crate::native_scrolling::continuous::latency::BudgetTier;

fn fixture(values: &[&str]) -> String {
    let mut text = "GL_RENDERER: llvmpipe (LLVM 20.1.2, 256 bits)\n".to_owned();
    for value in values {
        text.push_str(&format!("[shading] FrameTime: {value} ms\n"));
    }
    text
}

#[test]
fn fixed_capacity_probe_requires_five_positive_software_frame_costs() {
    let report =
        CapacityReport::parse(&fixture(&["4.514", "2.861", "2.816", "2.808", "2.780"])).unwrap();
    assert_eq!(report.median.0, Duration::from_micros(2816));
    assert!(matches!(
        CapacityReport::parse(&fixture(&["1", "2"])),
        Err(ProbeError::WrongSampleCount(2))
    ));
    for value in ["0", "-1", "NaN", "inf"] {
        assert!(matches!(
            CapacityReport::parse(&fixture(&["1", "2", value, "3", "4"])),
            Err(ProbeError::InvalidFrameCost)
        ));
    }
    assert!(matches!(
        CapacityReport::parse(&fixture(&["1", "2", "3", "4", "5"]).replace("llvmpipe", "hardware")),
        Err(ProbeError::WrongRenderer)
    ));
}

#[test]
fn software_capacity_tiers_are_frozen_and_uncalibrated_hosts_fail() {
    assert!(matches!(
        Environment::software_budget(FrameCost::try_from(2.824).unwrap())
            .unwrap()
            .tier_for_test(),
        BudgetTier::SoftwareVulkanFast
    ));
    assert!(matches!(
        Environment::software_budget(FrameCost::try_from(3.382).unwrap())
            .unwrap()
            .tier_for_test(),
        BudgetTier::SoftwareVulkanReducedCapacity
    ));
    assert!(matches!(
        Environment::software_budget(FrameCost::try_from(3.601).unwrap()),
        Err(CalibrationError::OutsideRecordedCapacity)
    ));
}

fn environment(adapter: Adapter, workload: Workload, capacity: Capacity) -> Environment {
    Environment {
        host: HostIdentity {
            model: "recorded test host".into(),
            logical_cpus: 4,
            vector_flags: vec![],
        },
        adapter,
        workload,
        capacity,
    }
}

#[test]
fn plain_and_hardware_keep_original_limits_and_missing_probes_never_pass() {
    for adapter in [
        Adapter::HardwareOrOther("discrete GPU".into()),
        Adapter::SoftwareVulkan("llvmpipe".into()),
    ] {
        let env = environment(adapter, Workload::Plain, Capacity::NotRequired);
        assert!(matches!(
            env.budget().unwrap().tier_for_test(),
            BudgetTier::Original
        ));
    }
    let env = environment(
        Adapter::HardwareOrOther("discrete GPU".into()),
        Workload::Rich,
        Capacity::NotRequired,
    );
    assert!(matches!(
        env.budget().unwrap().tier_for_test(),
        BudgetTier::Original
    ));
    let env = environment(
        Adapter::SoftwareVulkan("llvmpipe".into()),
        Workload::Rich,
        Capacity::Measured(Err(ProbeError::CommandFailed("missing glmark2".into()))),
    );
    assert!(matches!(env.budget(), Err(CalibrationError::ProbeFailed)));
    assert!(matches!(
        environment(Adapter::Missing, Workload::Rich, Capacity::NotRequired).budget(),
        Err(CalibrationError::MissingAdapter)
    ));
    let json = serde_json::to_string(&env).unwrap();
    assert!(json.contains("recorded test host") && json.contains("missing glmark2"));
}
