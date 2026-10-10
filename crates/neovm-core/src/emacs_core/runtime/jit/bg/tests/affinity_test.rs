//! Pure mask-boundary tests: no process knobs, threads or affinity syscalls.

use super::*;

#[cfg(target_os = "linux")]
#[test]
fn jit_bg_affinity_parser_rejects_out_of_mask_and_mixed_masks() {
    let limit = libc::CPU_SETSIZE as usize;
    assert_eq!(WorkerCpu::try_from(0).map(WorkerCpu::index), Ok(0));
    assert_eq!(
        WorkerCpu::try_from(limit - 1).map(WorkerCpu::index),
        Ok(limit - 1)
    );
    for cpu in [limit, usize::MAX] {
        assert_eq!(
            WorkerCpu::try_from(cpu),
            Err(WorkerCpuError::OutOfMask { cpu, limit })
        );
        assert_eq!(
            parse(&cpu.to_string()),
            Err(WorkerCpuError::OutOfMask { cpu, limit })
        );
        assert_eq!(
            parse(&format!("0,{cpu}")),
            Err(WorkerCpuError::OutOfMask { cpu, limit })
        );
    }
    let indices = |value: &str| {
        parse(value)
            .map(|mask| mask.map(|cpus| cpus.into_iter().map(WorkerCpu::index).collect::<Vec<_>>()))
    };
    assert_eq!(indices("0,4,4,invalid,-1"), Ok(Some(vec![0, 4, 4])));
    assert_eq!(indices(&(limit - 1).to_string()), Ok(Some(vec![limit - 1])));
    assert_eq!(parse(" ,invalid,-1"), Ok(None));
}

#[cfg(not(target_os = "linux"))]
#[test]
fn jit_bg_affinity_parser_has_no_representable_cpu_on_unsupported_platforms() {
    assert_eq!(
        WorkerCpu::try_from(0),
        Err(WorkerCpuError::UnsupportedPlatform)
    );
    assert_eq!(parse("0"), Err(WorkerCpuError::UnsupportedPlatform));
    assert_eq!(parse("invalid"), Ok(None));
}
