use std::fs;

use serde::{Deserialize, Serialize};

/// Stable execution constraints requested by a benchmark invocation.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MachinePolicy {
    pub cpu: Option<u32>,
    pub required_governor: Option<String>,
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
}

pub(crate) fn collect_host_provenance(policy: &MachinePolicy) -> HostProvenance {
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
    }
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
mod tests;
