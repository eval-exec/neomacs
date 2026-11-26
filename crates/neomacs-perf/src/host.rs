use std::fmt;
use std::fs;

use serde::{Deserialize, Serialize};

use crate::{PerfError, ScenarioId};

/// 1-minute `/proc/loadavg` ceiling above which cohort-gated scenarios refuse
/// to run locally unless `--allow-local` is passed explicitly.
pub const LOCAL_HIGH_LOAD_THRESHOLD: f64 = 2.0;

/// Provenance classification stamped on benchmark artifacts so local override
/// runs on loaded hosts can never be mistaken for calibrated treeroot runs.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub enum MeasurementProvenance {
    #[serde(rename = "calibrated")]
    Calibrated,
    #[serde(rename = "local-UNTRUSTED")]
    LocalUntrusted,
}

impl fmt::Display for MeasurementProvenance {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Calibrated => "calibrated",
            Self::LocalUntrusted => "local-UNTRUSTED",
        })
    }
}

/// Stable execution constraints requested by a benchmark invocation.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MachinePolicy {
    pub cpu: Option<u32>,
    pub required_governor: Option<String>,
    /// Explicitly permit running a cohort-gated scenario on a loaded local host
    /// (`loadavg_1m >= LOCAL_HIGH_LOAD_THRESHOLD`), stamping all emitted
    /// artifacts with `provenance = local-UNTRUSTED`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub allow_local: bool,
}

/// Host details that materially affect repeatability without identifying the host.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct HostProvenance {
    pub operating_system: String,
    pub architecture: String,
    pub kernel_release: Option<String>,
    pub cpu_model: Option<String>,
    pub logical_cpu_count: usize,
    pub allowed_cpus: Option<String>,
    pub selected_cpu: Option<u32>,
    pub scaling_governor: Option<String>,
    pub perf_event_paranoid: Option<i32>,
    pub continuous_integration: bool,
    /// Linux `/proc/loadavg` `1m 5m 15m` triplet sampled at run start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loadavg: Option<String>,
    /// Provenance classification (`calibrated` vs `local-UNTRUSTED`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<MeasurementProvenance>,
}

pub(crate) fn collect_host_provenance(policy: &MachinePolicy) -> HostProvenance {
    collect_host_provenance_with_loadavg(policy, read_proc_loadavg())
}

pub(crate) fn collect_host_provenance_with_loadavg(
    policy: &MachinePolicy,
    loadavg: Option<String>,
) -> HostProvenance {
    let provenance = classify_provenance(policy, loadavg.as_deref());
    HostProvenance {
        operating_system: std::env::consts::OS.to_string(),
        architecture: std::env::consts::ARCH.to_string(),
        kernel_release: command_line("uname", &["-r"]),
        cpu_model: proc_value("/proc/cpuinfo", "model name"),
        logical_cpu_count: std::thread::available_parallelism()
            .map(usize::from)
            .unwrap_or(1),
        allowed_cpus: proc_value("/proc/self/status", "Cpus_allowed_list"),
        selected_cpu: policy.cpu,
        scaling_governor: policy.cpu.and_then(cpu_governor),
        perf_event_paranoid: read_trimmed("/proc/sys/kernel/perf_event_paranoid")
            .and_then(|value| value.parse().ok()),
        continuous_integration: std::env::var_os("CI").is_some(),
        loadavg,
        provenance,
    }
}

pub(crate) fn classify_provenance(
    policy: &MachinePolicy,
    loadavg: Option<&str>,
) -> Option<MeasurementProvenance> {
    if policy.allow_local {
        return Some(MeasurementProvenance::LocalUntrusted);
    }
    let load_1m = loadavg.and_then(parse_loadavg_1m)?;
    if load_1m >= LOCAL_HIGH_LOAD_THRESHOLD {
        Some(MeasurementProvenance::LocalUntrusted)
    } else {
        Some(MeasurementProvenance::Calibrated)
    }
}

pub(crate) fn parse_loadavg_1m(loadavg: &str) -> Option<f64> {
    let first = loadavg.split_whitespace().next()?;
    let value = first.parse::<f64>().ok()?;
    (value.is_finite() && value >= 0.0).then_some(value)
}

/// Enforce the cohort high-load refusal gate before creating artifacts or
/// launching any editor subprocess.
pub(crate) fn validate_cohort_load_policy(
    scenario: ScenarioId,
    policy: &MachinePolicy,
    host: &HostProvenance,
) -> Result<(), PerfError> {
    if !scenario.enforces_cohort_load_gate() || policy.allow_local {
        return Ok(());
    }
    if let Some(raw) = host.loadavg.as_deref()
        && let Some(loadavg_1m) = parse_loadavg_1m(raw)
        && loadavg_1m >= LOCAL_HIGH_LOAD_THRESHOLD
    {
        return Err(PerfError::HighLoadLocalRefused {
            scenario,
            loadavg_1m,
            threshold: LOCAL_HIGH_LOAD_THRESHOLD,
            loadavg: raw.to_owned(),
        });
    }
    Ok(())
}

fn read_proc_loadavg() -> Option<String> {
    let contents = fs::read_to_string("/proc/loadavg").ok()?;
    let mut fields = contents.split_whitespace();
    let one = fields.next()?;
    let five = fields.next()?;
    let fifteen = fields.next()?;
    parse_loadavg_1m(one)?;
    Some(format!("{one} {five} {fifteen}"))
}

pub(crate) fn validate_machine_policy(
    policy: &MachinePolicy,
    host: &HostProvenance,
) -> Result<(), String> {
    if policy.cpu.is_some() && std::env::consts::OS != "linux" {
        return Err("CPU affinity is currently supported only on Linux".to_string());
    }
    if let (Some(cpu), Some(allowed)) = (policy.cpu, host.allowed_cpus.as_deref())
        && !cpu_list_contains(allowed, cpu)
    {
        return Err(format!(
            "selected CPU {cpu} is outside the process allowance {allowed}"
        ));
    }
    if let Some(required) = &policy.required_governor {
        let actual = host.scaling_governor.as_deref().ok_or_else(|| {
            "the requested CPU scaling governor could not be read for the selected CPU".to_string()
        })?;
        if actual != required {
            return Err(format!(
                "CPU scaling governor mismatch: required {required}, found {actual}"
            ));
        }
    }
    Ok(())
}

pub(crate) fn cpu_list_contains(list: &str, cpu: u32) -> bool {
    list.split(',').any(|part| {
        let part = part.trim();
        if let Some((start, end)) = part.split_once('-') {
            return start
                .parse::<u32>()
                .ok()
                .zip(end.parse::<u32>().ok())
                .is_some_and(|(start, end)| (start..=end).contains(&cpu));
        }
        part.parse::<u32>().is_ok_and(|allowed| allowed == cpu)
    })
}

fn cpu_governor(cpu: u32) -> Option<String> {
    read_trimmed(&format!(
        "/sys/devices/system/cpu/cpu{cpu}/cpufreq/scaling_governor"
    ))
}

fn proc_value(path: &str, key: &str) -> Option<String> {
    let contents = fs::read_to_string(path).ok()?;
    contents.lines().find_map(|line| {
        let (candidate, value) = line.split_once(':')?;
        (candidate.trim() == key).then(|| value.trim().to_string())
    })
}

fn read_trimmed(path: &str) -> Option<String> {
    let value = fs::read_to_string(path).ok()?;
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn command_line(program: &str, arguments: &[&str]) -> Option<String> {
    let output = std::process::Command::new(program)
        .args(arguments)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?;
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

#[cfg(test)]
#[path = "host/tests/host_test.rs"]
mod tests;
