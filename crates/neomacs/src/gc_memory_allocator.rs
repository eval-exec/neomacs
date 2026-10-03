//! Jemalloc observations for the optional GC memory telemetry build.
//!
//! The parent module is compiled only with `gc-memory-telemetry`. Ordinary
//! editor builds have no allocator observation calls or module dependencies.

use neovm_core::tagged::gc::memory_telemetry::AllocatorSnapshot;

#[cfg(all(
    not(test),
    target_os = "linux",
    any(feature = "platform-allocator", feature = "jemalloc"),
))]
pub(crate) fn sample() -> AllocatorSnapshot {
    jemalloc::sample()
}

// Neomacs' own test targets deliberately use the system allocator. Do not
// report statistics from a linked but inactive jemalloc in those targets.
#[cfg(not(all(
    not(test),
    target_os = "linux",
    any(feature = "platform-allocator", feature = "jemalloc"),
)))]
pub(crate) fn sample() -> AllocatorSnapshot {
    AllocatorSnapshot::default()
}

#[cfg(all(
    not(test),
    target_os = "linux",
    any(feature = "platform-allocator", feature = "jemalloc"),
))]
mod jemalloc {
    use super::AllocatorSnapshot;
    use std::ffi::CStr;
    use std::mem::size_of;
    use std::ptr;

    pub(super) fn sample() -> AllocatorSnapshot {
        // `available` describes the selected allocator's observation API;
        // `stats_enabled` separately describes full statistics support.
        let mut snapshot = AllocatorSnapshot {
            available: true,
            ..AllocatorSnapshot::default()
        };
        snapshot.stats_enabled = match read_stats_enabled() {
            Ok(enabled) => enabled,
            Err(error) => {
                snapshot.error = error;
                return snapshot;
            }
        };
        if !snapshot.stats_enabled {
            return snapshot;
        }

        // Refresh once for this group. An epoch failure leaves every byte
        // field absent rather than publishing cached statistics as current.
        if let Err(error) = refresh_epoch() {
            snapshot.error = error;
            return snapshot;
        }
        snapshot.allocated_bytes = read_size(c"stats.allocated", &mut snapshot.error);
        snapshot.active_bytes = read_size(c"stats.active", &mut snapshot.error);
        snapshot.resident_bytes = read_size(c"stats.resident", &mut snapshot.error);
        snapshot.mapped_bytes = read_size(c"stats.mapped", &mut snapshot.error);
        snapshot.retained_bytes = read_size(c"stats.retained", &mut snapshot.error);
        snapshot
    }

    fn read_stats_enabled() -> Result<bool, i32> {
        let mut enabled = false;
        let mut len = size_of::<bool>();
        // SAFETY: config.stats returns a C bool, with output storage of its
        // exact size; this read has no input value or allocator side effects.
        let error = unsafe {
            tikv_jemalloc_sys::mallctl(
                c"config.stats".as_ptr(),
                (&mut enabled as *mut bool).cast(),
                &mut len,
                ptr::null_mut(),
                0,
            )
        };
        check_read(error, len, size_of::<bool>())?;
        Ok(enabled)
    }

    fn refresh_epoch() -> Result<(), i32> {
        let mut epoch = 0u64;
        let mut len = size_of::<u64>();
        let mut refresh = 1u64;
        // SAFETY: epoch uses uint64_t for both parameters. Supplying an
        // input refreshes the cached statistics; it does not purge arenas or
        // flush tcaches. The independent input/output storage remains live.
        let error = unsafe {
            tikv_jemalloc_sys::mallctl(
                c"epoch".as_ptr(),
                (&mut epoch as *mut u64).cast(),
                &mut len,
                (&mut refresh as *mut u64).cast(),
                size_of::<u64>(),
            )
        };
        check_read(error, len, size_of::<u64>())
    }

    fn read_size(name: &CStr, first_error: &mut i32) -> Option<usize> {
        let mut value = 0usize;
        let mut len = size_of::<usize>();
        // SAFETY: all callers supply a static stats.* size_t entry. Output
        // storage has exactly that size, and no input value is provided.
        let error = unsafe {
            tikv_jemalloc_sys::mallctl(
                name.as_ptr(),
                (&mut value as *mut usize).cast(),
                &mut len,
                ptr::null_mut(),
                0,
            )
        };
        match check_read(error, len, size_of::<usize>()) {
            Ok(()) => Some(value),
            Err(error) => {
                if *first_error == 0 {
                    *first_error = error;
                }
                None
            }
        }
    }

    fn check_read(error: i32, actual_len: usize, expected_len: usize) -> Result<(), i32> {
        if error != 0 {
            Err(error)
        } else if actual_len != expected_len {
            Err(libc::EINVAL)
        } else {
            Ok(())
        }
    }
}
