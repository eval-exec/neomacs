use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::profile_gate::ProfileGate;
use crate::{ArtifactFile, ArtifactKind, CaptureRoute, Measurement, MetricName, MetricUnit};

const STANDARD_EVENTS: &[&str] = &[
    "cycles:u",
    "instructions:u",
    "page-faults",
    "branch-misses:u",
    "cache-misses:u",
    "L1-dcache-load-misses:u",
    "dTLB-load-misses:u",
];

/// Portion of a scenario observed by Linux hardware counters.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CounterScope {
    EditLoop,
    WholeProcess,
}

/// Parse `perf stat --field-separator , --no-big-num` output into typed metrics.
pub fn parse_perf_stat_csv(contents: &str) -> Result<Vec<Measurement>, String> {
    let mut values = BTreeMap::<MetricName, f64>::new();
    let mut unavailable = BTreeMap::<MetricName, String>::new();
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields = line.split(',').map(str::trim).collect::<Vec<_>>();
        if fields.len() < 3 {
            return Err(format!("malformed perf stat row {line:?}"));
        }
        let Some(name) = counter_metric(fields[2]) else {
            continue;
        };
        let raw_value = fields[0];
        if raw_value.starts_with('<') {
            unavailable
                .entry(name)
                .or_insert_with(|| format!("perf counter {} was {}", fields[2], raw_value));
            continue;
        }
        let value = raw_value.parse::<f64>().map_err(|error| {
            format!(
                "perf counter {} had invalid value {raw_value:?}: {error}",
                fields[2]
            )
        })?;
        if !value.is_finite() || value < 0.0 {
            return Err(format!(
                "perf counter {} had invalid value {raw_value:?}",
                fields[2]
            ));
        }
        *values.entry(name).or_default() += value;
    }
    if values.is_empty() {
        if let Some((_, message)) = unavailable.into_iter().next() {
            return Err(message);
        }
        return Err("perf stat output contained no supported counters".to_string());
    }
    Ok(values
        .into_iter()
        .map(|(name, value)| Measurement {
            name,
            value,
            unit: MetricUnit::Count,
        })
        .collect())
}

fn counter_metric(event: &str) -> Option<MetricName> {
    let event = event
        .strip_prefix("cpu_core/")
        .or_else(|| event.strip_prefix("cpu_atom/"))
        .unwrap_or(event);
    let event = event.strip_suffix("/u").unwrap_or(event);
    let event = event.strip_suffix(":u").unwrap_or(event);
    match event {
        "cycles" => Some(MetricName::CpuCycles),
        "instructions" => Some(MetricName::Instructions),
        "page-faults" => Some(MetricName::PageFaults),
        "branch-misses" => Some(MetricName::BranchMisses),
        "cache-misses" => Some(MetricName::CacheMisses),
        "L1-dcache-load-misses" => Some(MetricName::L1DataCacheLoadMisses),
        "dTLB-load-misses" => Some(MetricName::DataTlbLoadMisses),
        _ => None,
    }
}

pub(crate) struct PerfStatCapture {
    scope: CounterScope,
    output: PathBuf,
    timeout: Duration,
    gate: Option<ProfileGate>,
}

impl PerfStatCapture {
    pub(crate) fn new(directory: &Path, scope: CounterScope, timeout: Duration) -> Self {
        Self {
            scope,
            output: directory.join("hardware-counters.csv"),
            timeout,
            gate: None,
        }
    }

    pub(crate) fn wrap(
        &mut self,
        mut command: Command,
        route: CaptureRoute,
    ) -> Result<Command, String> {
        if !cfg!(target_os = "linux") {
            return Err("hardware counter collection requires Linux perf".to_string());
        }
        if self.scope == CounterScope::EditLoop {
            self.gate = Some(ProfileGate::start(
                self.output
                    .parent()
                    .expect("counter output has a parent directory"),
                self.timeout,
            )?);
        }
        if let CaptureRoute::Adapter(prefix) = route {
            command.env(format!("{prefix}_PERF_STAT"), &self.output);
            command.env(format!("{prefix}_PERF_EVENTS"), STANDARD_EVENTS.join(","));
            self.configure_gate_environment(&mut command, Some(prefix));
            return Ok(command);
        }

        let mut captured = Command::new("perf");
        captured.args(self.stat_arguments());
        captured.arg(command.get_program());
        captured.args(command.get_args());
        if let Some(directory) = command.get_current_dir() {
            captured.current_dir(directory);
        }
        captured.env_clear();
        for (name, value) in command.get_envs() {
            match value {
                Some(value) => {
                    captured.env(name, value);
                }
                None => {
                    captured.env_remove(name);
                }
            }
        }
        self.configure_gate_environment(&mut captured, None);
        Ok(captured)
    }

    fn stat_arguments(&self) -> Vec<OsString> {
        let mut arguments = vec![
            OsString::from("stat"),
            OsString::from("--no-big-num"),
            OsString::from("--field-separator"),
            OsString::from(","),
            OsString::from("--output"),
            self.output.as_os_str().to_os_string(),
            OsString::from("--event"),
            OsString::from(STANDARD_EVENTS.join(",")),
        ];
        if let Some(gate) = &self.gate {
            let paths = gate.control_paths();
            arguments.push(OsString::from("--delay=-1"));
            arguments.push(OsString::from(format!(
                "--control=fifo:{},{}",
                paths.command.display(),
                paths.acknowledgement.display()
            )));
        }
        arguments.push(OsString::from("--"));
        arguments
    }

    fn configure_gate_environment(&self, command: &mut Command, adapter_prefix: Option<&str>) {
        let Some(gate) = &self.gate else {
            return;
        };
        command.env("NEOMACS_PERF_GATE_PORT", gate.endpoint().port().to_string());
        if let Some(prefix) = adapter_prefix {
            let paths = gate.control_paths();
            command.env(
                format!("{prefix}_PERF_CONTROL"),
                format!(
                    "fifo:{},{}",
                    paths.command.display(),
                    paths.acknowledgement.display()
                ),
            );
        }
    }

    pub(crate) fn finish_gate(&mut self) -> Result<(), String> {
        match &mut self.gate {
            Some(gate) => gate.finish(),
            None => Ok(()),
        }
    }

    pub(crate) fn cancel_gate(&mut self) {
        self.gate.take();
    }

    pub(crate) fn collect(&self) -> Result<(Vec<Measurement>, ArtifactFile), String> {
        let contents = fs::read_to_string(&self.output).map_err(|error| {
            format!(
                "failed to read hardware counter output {}: {error}",
                self.output.display()
            )
        })?;
        let measurements = parse_perf_stat_csv(&contents)?;
        for expected in STANDARD_EVENTS
            .iter()
            .filter_map(|event| counter_metric(event))
        {
            if !measurements
                .iter()
                .any(|measurement| measurement.name == expected)
            {
                return Err(format!(
                    "perf stat output omitted requested counter {expected:?}"
                ));
            }
        }
        Ok((
            measurements,
            ArtifactFile {
                kind: ArtifactKind::HardwareCounters,
                path: PathBuf::from("hardware-counters.csv"),
            },
        ))
    }
}

#[cfg(test)]
mod tests;
