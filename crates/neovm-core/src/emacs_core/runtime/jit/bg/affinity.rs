//! Validated CPU indices for the worker-affinity measurement knob.
//!
//! Mask representability is distinct from online/allowed CPU availability:
//! the kernel can refuse an in-range mask without a userspace indexing panic.
//! This sibling of `worker` hides the constructor from the unsafe consumer.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WorkerCpu(usize);

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum WorkerCpuError {
    #[cfg(target_os = "linux")]
    #[error("worker CPU {cpu} does not fit the affinity mask (limit {limit})")]
    OutOfMask { cpu: usize, limit: usize },
    #[cfg(not(target_os = "linux"))]
    #[error("worker affinity is unsupported on this platform")]
    UnsupportedPlatform,
}

impl WorkerCpu {
    pub(super) const fn index(self) -> usize {
        self.0
    }
}

impl TryFrom<usize> for WorkerCpu {
    type Error = WorkerCpuError;

    fn try_from(cpu: usize) -> Result<Self, Self::Error> {
        #[cfg(target_os = "linux")]
        {
            let limit = libc::CPU_SETSIZE as usize;
            if cpu >= limit {
                return Err(WorkerCpuError::OutOfMask { cpu, limit });
            }
            Ok(Self(cpu))
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = cpu;
            Err(WorkerCpuError::UnsupportedPlatform)
        }
    }
}

/// Preserve ignored nonnumeric tokens, but reject the complete mask if
/// any parsed CPU cannot fit; do not silently apply an unintended subset.
pub(super) fn parse(value: &str) -> Result<Option<Vec<WorkerCpu>>, WorkerCpuError> {
    let cpus: Vec<WorkerCpu> = value
        .split(',')
        .filter_map(|token| token.trim().parse::<usize>().ok())
        .map(WorkerCpu::try_from)
        .collect::<Result<_, _>>()?;
    Ok((!cpus.is_empty()).then_some(cpus))
}

#[cfg(test)]
#[path = "tests/affinity_test.rs"]
mod tests;
