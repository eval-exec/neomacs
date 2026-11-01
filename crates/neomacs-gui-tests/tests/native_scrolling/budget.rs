//! Neomacs' recorded software-Vulkan contract, not a GNU comparison.
//! FIX6, four vCPUs / SMT2: 9V74 capacity 2.800–2.861 ms had rich
//! p95/max/gap up to 370.751/526.491/165.841 ms and cadence 19.488 ms;
//! 7763 capacity 3.363–3.395 ms had 1129.390/1520.183/103.874 ms and
//! cadence 21.385 ms. Freeze separate bounded tiers rather than letting
//! a slow scroll run enlarge its own response budget. Hardware/plain keep
//! the original contract. The present-path latency remains a product issue.
use super::{Workload, latency::ResponseBudget};
use serde::Serialize;
use std::{fs, path::Path, process::Command, time::Duration};

#[derive(Debug, Serialize)]
pub(super) struct HostIdentity {
    model: String,
    logical_cpus: usize,
    vector_flags: Vec<String>,
}

impl HostIdentity {
    fn capture() -> Self {
        let info = fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
        let field = |name: &str| {
            info.lines().find_map(|line| {
                let (key, value) = line.split_once(':')?;
                (key.trim() == name).then(|| value.trim().to_owned())
            })
        };
        Self {
            model: field("model name").unwrap_or_else(|| "unavailable".into()),
            logical_cpus: std::thread::available_parallelism().map_or(0, usize::from),
            vector_flags: field("flags")
                .unwrap_or_default()
                .split_whitespace()
                .filter(|flag| flag.starts_with("avx"))
                .map(str::to_owned)
                .collect(),
        }
    }
}

#[derive(Debug, Serialize)]
enum Adapter {
    HardwareOrOther(String),
    SoftwareVulkan(String),
    Missing,
}

impl Adapter {
    fn capture(log: &str) -> Self {
        match log
            .lines()
            .find_map(|line| line.split_once("wgpu adapter: ").map(|(_, name)| name))
        {
            Some(name)
                if name.contains("llvmpipe") && name.contains("type=Cpu, backend=Vulkan") =>
            {
                Self::SoftwareVulkan(name.to_owned())
            }
            Some(name) => Self::HardwareOrOther(name.to_owned()),
            None => Self::Missing,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
struct FrameCost(Duration);

impl TryFrom<f64> for FrameCost {
    type Error = ProbeError;

    fn try_from(milliseconds: f64) -> Result<Self, Self::Error> {
        Duration::try_from_secs_f64(milliseconds / 1000.0)
            .ok()
            .filter(|cost| !cost.is_zero())
            .map(Self)
            .ok_or(ProbeError::InvalidFrameCost)
    }
}

#[derive(Debug, Serialize)]
pub(super) enum ProbeError {
    Io(String),
    CommandFailed(String),
    WrongRenderer,
    WrongSampleCount(usize),
    InvalidFrameCost,
}

#[derive(Debug, Serialize)]
struct CapacityReport {
    renderer: String,
    frames: [FrameCost; 5],
    median: FrameCost,
}

impl CapacityReport {
    fn parse(text: &str) -> Result<Self, ProbeError> {
        let renderer = text
            .lines()
            .find_map(|line| line.split_once("GL_RENDERER:").map(|(_, r)| r.trim()));
        let renderer = renderer
            .filter(|r| r.to_ascii_lowercase().contains("llvmpipe"))
            .ok_or(ProbeError::WrongRenderer)?;
        let mut frames = Vec::new();
        for line in text.lines() {
            if let Some((_, suffix)) = line.split_once("FrameTime:") {
                let value = suffix
                    .split_whitespace()
                    .next()
                    .ok_or(ProbeError::InvalidFrameCost)?;
                let value = value
                    .parse::<f64>()
                    .map_err(|_| ProbeError::InvalidFrameCost)?;
                frames.push(FrameCost::try_from(value)?);
            }
        }
        let frames: [FrameCost; 5] = frames
            .try_into()
            .map_err(|v: Vec<_>| ProbeError::WrongSampleCount(v.len()))?;
        let mut sorted = frames;
        sorted.sort_by_key(|cost| cost.0);
        Ok(Self {
            renderer: renderer.to_owned(),
            frames,
            median: sorted[2],
        })
    }

    fn capture(artifacts: &Path) -> Result<Self, ProbeError> {
        // Same fixed GL proxy as FIX6; no renderer measurement runs during
        // the input stream. timeout bounds both tool and X server failures.
        let mut command = Command::new("timeout");
        command.args([
            "--kill-after=2s",
            "25s",
            "xvfb-run",
            "--auto-servernum",
            "--server-args=-screen 0 1280x800x24 -nolisten tcp",
            "glmark2",
            "--off-screen",
            "--size",
            "1280x800",
            "--frame-end",
            "finish",
            "--reuse-context",
        ]);
        for _ in 0..5 {
            command.args(["--benchmark", "shading:shading=phong:duration=1.0"]);
        }
        let output = command
            .env("LIBGL_ALWAYS_SOFTWARE", "1")
            .env("GALLIUM_DRIVER", "llvmpipe")
            .env("LP_NUM_THREADS", "4")
            .env("vblank_mode", "0")
            .output()
            .map_err(|e| ProbeError::Io(e.to_string()))?;
        fs::write(artifacts.join("capacity.stdout"), &output.stdout)
            .map_err(|e| ProbeError::Io(e.to_string()))?;
        fs::write(artifacts.join("capacity.stderr"), &output.stderr)
            .map_err(|e| ProbeError::Io(e.to_string()))?;
        if !output.status.success() {
            return Err(ProbeError::CommandFailed(output.status.to_string()));
        }
        Self::parse(&String::from_utf8_lossy(&output.stdout))
    }
}

#[derive(Debug, Serialize)]
enum Capacity {
    NotRequired,
    Measured(Result<CapacityReport, ProbeError>),
}

#[derive(Debug, Serialize)]
pub(super) struct Environment {
    host: HostIdentity,
    workload: Workload,
    adapter: Adapter,
    capacity: Capacity,
}

#[derive(Debug, Serialize)]
pub(super) enum CalibrationError {
    MissingAdapter,
    ProbeFailed,
    OutsideRecordedCapacity,
}

impl Environment {
    pub(super) fn capture(artifacts: &Path, workload: Workload) -> Self {
        let adapter = Adapter::capture(
            &fs::read_to_string(artifacts.join("neomacs.log")).unwrap_or_default(),
        );
        let capacity = match (&adapter, workload) {
            (Adapter::SoftwareVulkan(_), Workload::Rich) => {
                Capacity::Measured(CapacityReport::capture(artifacts))
            }
            (Adapter::SoftwareVulkan(_), Workload::Plain)
            | (Adapter::HardwareOrOther(_), Workload::Plain | Workload::Rich)
            | (Adapter::Missing, Workload::Plain | Workload::Rich) => Capacity::NotRequired,
        };
        Self {
            host: HostIdentity::capture(),
            workload,
            adapter,
            capacity,
        }
    }

    pub(super) fn budget(&self) -> Result<ResponseBudget, CalibrationError> {
        match (&self.adapter, self.workload) {
            (Adapter::Missing, Workload::Plain | Workload::Rich) => {
                Err(CalibrationError::MissingAdapter)
            }
            (Adapter::HardwareOrOther(_), Workload::Plain | Workload::Rich)
            | (Adapter::SoftwareVulkan(_), Workload::Plain) => Ok(ResponseBudget::original()),
            (Adapter::SoftwareVulkan(_), Workload::Rich) => match &self.capacity {
                Capacity::Measured(Ok(probe)) => Self::software_budget(probe.median),
                Capacity::Measured(Err(_)) | Capacity::NotRequired => {
                    Err(CalibrationError::ProbeFailed)
                }
            },
        }
    }

    fn software_budget(cost: FrameCost) -> Result<ResponseBudget, CalibrationError> {
        if cost.0 <= Duration::from_millis(3) {
            Ok(ResponseBudget::software_fast())
        } else if cost.0 <= Duration::from_micros(3600) {
            Ok(ResponseBudget::software_reduced_capacity())
        } else {
            Err(CalibrationError::OutsideRecordedCapacity)
        }
    }
}

#[cfg(test)]
#[path = "budget_tests.rs"]
mod tests;
