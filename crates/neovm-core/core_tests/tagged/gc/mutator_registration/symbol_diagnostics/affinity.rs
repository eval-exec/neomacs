//! Test-only affinity. Gate semantics inherit the caller's build-CPU mask.
use super::*;

#[must_use]
pub(super) enum Affinity {
    Inherited,
    #[cfg(target_os = "linux")]
    Restore(libc::cpu_set_t),
}
#[cfg(target_os = "linux")]
fn current() -> Result<libc::cpu_set_t, Error> {
    // SAFETY: cpu_set_t is a plain bitset, initialized by this thread's syscall.
    unsafe {
        let mut set: libc::cpu_set_t = std::mem::zeroed();
        if libc::sched_getaffinity(0, std::mem::size_of_val(&set), &mut set) != 0 {
            return Err(Error::Affinity);
        }
        Ok(set)
    }
}
#[cfg(target_os = "linux")]
fn install(set: &libc::cpu_set_t) -> Result<(), Error> {
    // SAFETY: a valid CPU bitset is applied to the calling thread only.
    if unsafe { libc::sched_setaffinity(0, std::mem::size_of_val(set), set) } != 0 {
        return Err(Error::Affinity);
    }
    Ok(())
}
#[derive(Clone, Copy)]
enum Cpu {
    Controller,
    Worker(Ordinal),
}
fn pin(mode: DiagnosticMode, cpu: Cpu) -> Result<Affinity, Error> {
    match mode {
        DiagnosticMode::Gate => Ok(Affinity::Inherited),
        DiagnosticMode::Measurement => {
            #[cfg(all(target_os = "linux", not(miri)))]
            {
                let previous = current()?;
                let index = match cpu {
                    Cpu::Controller => 5,
                    Cpu::Worker(Ordinal(index)) => {
                        *[4, 6, 8, 10].get(index).ok_or(Error::Affinity)?
                    }
                };
                // SAFETY: index comes from the fixed island plan and is below
                // CPU_SETSIZE; both queried and constructed bitsets are valid.
                let set = unsafe {
                    if !libc::CPU_ISSET(index, &previous) {
                        return Err(Error::Affinity);
                    }
                    let mut set: libc::cpu_set_t = std::mem::zeroed();
                    libc::CPU_SET(index, &mut set);
                    set
                };
                install(&set)?;
                // The guard already owns restoration if verification fails.
                let restore = Affinity::Restore(previous);
                let actual = current()?;
                // SAFETY: querying a valid returned CPU bitset at fixed index.
                if unsafe { libc::CPU_COUNT(&actual) != 1 || !libc::CPU_ISSET(index, &actual) } {
                    return Err(Error::Affinity);
                }
                Ok(restore)
            }
            #[cfg(any(not(target_os = "linux"), miri))]
            {
                let _ = cpu;
                Err(Error::Affinity)
            }
        }
    }
}
pub(super) fn controller(mode: DiagnosticMode) -> Result<Affinity, Error> {
    pin(mode, Cpu::Controller)
}
pub(super) fn worker(mode: DiagnosticMode, ordinal: Ordinal) -> Result<Affinity, Error> {
    pin(mode, Cpu::Worker(ordinal))
}
impl Drop for Affinity {
    fn drop(&mut self) {
        match self {
            Self::Inherited => {}
            #[cfg(target_os = "linux")]
            Self::Restore(previous) => {
                // Keep a restoration failure visible without double-unwinding.
                if let Err(error) = install(previous) {
                    eprintln!("P3SYM_C6_DIAGNOSTIC_AFFINITY_RESTORE {error:?}");
                    assert!(std::thread::panicking(), "affinity restoration failed");
                }
            }
        }
    }
}
