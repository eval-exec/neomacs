/// GNU's `command-line-max-length` initializer (`src/callproc.c:2246-2252`).
///
/// `sysconf (_SC_ARG_MAX) / 4` -- "divide it by 4 as a crude way to go
/// bytes->characters" -- with GNU's own 4096 fallback for platforms that do not
/// publish `_SC_ARG_MAX`.  `sysconf` answers -1 without setting `errno` when a
/// limit is indeterminate, which is the same "no answer" case, so it takes the
/// fallback too.
pub fn command_line_max_length() -> i64 {
    // Miri does not implement `sysconf(_SC_ARG_MAX)` and aborts the run the
    // moment a Context is built. The value is a startup constant with no
    // bearing on heap or aliasing behaviour, so interpreted runs take GNU's
    // own 4096 fallback instead (the arm `sysconf` failure takes anyway).
    #[cfg(all(unix, not(miri)))]
    {
        // SAFETY: `sysconf` reads a static system limit and touches no memory
        // the caller owns.
        let arg_max = unsafe { libc::sysconf(libc::_SC_ARG_MAX) };
        if arg_max > 0 {
            return (arg_max as i64) / 4;
        }
    }
    4096
}
