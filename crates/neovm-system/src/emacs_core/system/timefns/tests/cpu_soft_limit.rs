use std::io;
use std::marker::PhantomData;
use std::mem::MaybeUninit;
use std::rc::Rc;

/// Bounds one nextest test process, never the Cargo build or test runner.
///
/// These tests run in separate processes under nextest. The resource limit is
/// process-wide, so the guard stays on its creating thread and restores only
/// the old soft limit. The hard limit is never changed.
#[must_use]
#[derive(Debug)]
pub(super) struct CpuSoftLimit {
    previous_soft: libc::rlim_t,
    _thread_bound: PhantomData<Rc<()>>,
}

impl CpuSoftLimit {
    pub(super) fn two_more_seconds() -> io::Result<Self> {
        let mut old = MaybeUninit::<libc::rlimit>::uninit();
        // SAFETY: old is writable for one rlimit; getrlimit initializes it on
        // success. This only queries the current process's CPU resource limit.
        if unsafe { libc::getrlimit(libc::RLIMIT_CPU, old.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: the successful getrlimit above initialized every field.
        let old = unsafe { old.assume_init() };
        let mut usage = MaybeUninit::<libc::rusage>::uninit();
        // SAFETY: usage is writable for one rusage; getrusage initializes it on
        // success. RUSAGE_SELF counts only this isolated test process.
        if unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: the successful getrusage above initialized every field.
        let usage = unsafe { usage.assume_init() };
        let used_micros = i128::from(usage.ru_utime.tv_sec) * 1_000_000
            + i128::from(usage.ru_utime.tv_usec)
            + i128::from(usage.ru_stime.tv_sec) * 1_000_000
            + i128::from(usage.ru_stime.tv_usec);
        // Round the already-used CPU time up, then allow another two seconds.
        // Counting prior startup time avoids accidentally limiting that work.
        let soft = libc::rlim_t::try_from((used_micros + 999_999) / 1_000_000 + 2)
            .map_err(|_| io::Error::other("test CPU usage exceeds resource-limit range"))?;
        let limit = libc::rlimit {
            rlim_cur: soft.min(old.rlim_cur).min(old.rlim_max),
            rlim_max: old.rlim_max,
        };
        // SAFETY: limit is initialized and borrowed for the call. Only the soft
        // limit of this nextest test process changes; the hard limit is retained.
        if unsafe { libc::setrlimit(libc::RLIMIT_CPU, &limit) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            previous_soft: old.rlim_cur,
            _thread_bound: PhantomData,
        })
    }
}

impl Drop for CpuSoftLimit {
    fn drop(&mut self) {
        let mut current = MaybeUninit::<libc::rlimit>::uninit();
        // SAFETY: current is writable for one rlimit; it is read only after
        // success. Drop must not panic if the OS refuses a resource operation.
        if unsafe { libc::getrlimit(libc::RLIMIT_CPU, current.as_mut_ptr()) } != 0 {
            return;
        }
        // SAFETY: the successful getrlimit above initialized every field.
        let mut current = unsafe { current.assume_init() };
        current.rlim_cur = self.previous_soft.min(current.rlim_max);
        // SAFETY: current is initialized; restoring the old soft limit retains
        // the current hard limit. Errors are intentionally ignored during Drop.
        let _ = unsafe { libc::setrlimit(libc::RLIMIT_CPU, &current) };
    }
}

static_assertions::assert_not_impl_any!(CpuSoftLimit: Send, Sync);
