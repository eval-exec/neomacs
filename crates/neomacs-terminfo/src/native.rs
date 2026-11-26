//! The complete unsafe surface. ncurses owns/caches the selected terminal;
//! Rust owns only snapshots. See README.md for native contracts and validation.
use std::ffi::{CStr, CString, c_char, c_int, c_long, c_void};
use std::sync::{Mutex, MutexGuard};

use crate::{Database, Error, FlagCapability, Query, StringCapability};

pub(crate) mod padding;
pub(super) use padding::write_padded;

static NATIVE: Mutex<()> = Mutex::new(());

/// Temporarily block `SIGTTOU` on the calling thread and restore the previous
/// signal mask on drop, matching GNU `block_tty_out_signal` /
/// `unblock_tty_out_signal` (`src/sysdep.c:924-939`).
///
/// Used around `tgetent` (GNU `src/term.c:4485-4489`) and ncurses `setupterm`
/// (which `tgetent` wraps), because terminfo initialization may access the
/// controlling terminal while the process is not in the foreground process
/// group. Blocking rather than ignoring preserves the process's inherited
/// `SIGTTOU` disposition for the editor and its subprocesses.
pub(super) struct TtyOutSignalGuard {
    oldset: libc::sigset_t,
    active: bool,
}

impl TtyOutSignalGuard {
    pub(super) fn block() -> Self {
        // SAFETY: `sigset_t` is plain C storage initialized by `sigemptyset` /
        // `sigaddset`, and `pthread_sigmask` updates only the calling thread's
        // mask while writing the previous mask into `oldset`.
        unsafe {
            let mut blocked: libc::sigset_t = std::mem::zeroed();
            let mut oldset: libc::sigset_t = std::mem::zeroed();
            if libc::sigemptyset(&raw mut blocked) == 0
                && libc::sigaddset(&raw mut blocked, libc::SIGTTOU) == 0
                && libc::pthread_sigmask(libc::SIG_BLOCK, &raw const blocked, &raw mut oldset) == 0
            {
                Self {
                    oldset,
                    active: true,
                }
            } else {
                Self {
                    oldset,
                    active: false,
                }
            }
        }
    }
}

impl Drop for TtyOutSignalGuard {
    fn drop(&mut self) {
        if self.active {
            // SAFETY: `oldset` was populated by the matching
            // `pthread_sigmask(SIG_BLOCK, ...)` call in `Self::block`.
            unsafe {
                libc::pthread_sigmask(
                    libc::SIG_SETMASK,
                    &raw const self.oldset,
                    std::ptr::null_mut(),
                );
            }
        }
    }
}

unsafe extern "C" {
    fn tgetent(buffer: *mut c_char, term: *const c_char) -> c_int;
    fn tgetstr(name: *const c_char, area: *mut *mut c_char) -> *mut c_char;
    fn tigetstr(name: *const c_char) -> *mut c_char;
    fn tgetnum(name: *const c_char) -> c_int;
    fn tgetflag(name: *const c_char) -> c_int;
    fn tigetflag(name: *const c_char) -> c_int;
    fn set_curterm(term: *mut c_void) -> *mut c_void;
    fn tparm(format: *const c_char, ...) -> *mut c_char;
}

fn name(value: &str) -> Result<CString, Error> {
    if value.is_empty() {
        return Err(Error::InvalidName);
    }
    CString::new(value).map_err(|_| Error::InvalidName)
}

pub(super) fn load(term: &str, queries: &[Query<'_>]) -> Result<Database, Error> {
    let term = name(term)?;
    let names = queries
        .iter()
        .map(|query| {
            name(match query {
                Query::String(StringCapability::Termcap(n) | StringCapability::Terminfo(n))
                | Query::TermcapNumber(n)
                | Query::Flag(FlagCapability::Termcap(n) | FlagCapability::Terminfo(n)) => n,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let _guard = NATIVE.lock().map_err(|_| Error::NativeStatePoisoned)?;
    // SAFETY: this crate exclusively owns ncurses access under NATIVE. Detach
    // the previous current terminal so tgetent does not reuse it. Its cache
    // still owns that allocation and reclaims it on the next tgetent, including
    // failure. Reusing the null buffer selects the same bounded cache slot.
    // Do not del_curterm: that would leave tgetent's cache with a dangling owner.
    // ncurses ignores the buffer (this is not a generic BSD termcap binding).
    // Block SIGTTOU across tgetent matching GNU `init_tty` (`src/term.c:4485-4489`).
    let status = {
        let _tty_out_guard = TtyOutSignalGuard::block();
        unsafe {
            set_curterm(std::ptr::null_mut());
            tgetent(std::ptr::null_mut(), term.as_ptr())
        }
    };
    match status {
        1 => {}
        0 => return Err(Error::TerminalNotFound),
        _ => return Err(Error::DatabaseUnavailable),
    }
    let mut result = Database::default();
    for (query, name) in queries.iter().zip(&names) {
        match query {
            Query::String(capability) => {
                let (map, key, raw) = match capability {
                    StringCapability::Termcap(key) => {
                        // SAFETY: NUL-terminated name, active terminal, and
                        // exclusive access. NULL area prevents unbounded writes
                        // to a caller buffer; ncurses retains the returned data.
                        let raw = unsafe { tgetstr(name.as_ptr(), std::ptr::null_mut()) };
                        (&mut result.termcap_strings, key, raw)
                    }
                    StringCapability::Terminfo(key) => {
                        // SAFETY: same name/state invariants as above.
                        let raw = unsafe { tigetstr(name.as_ptr()) };
                        (&mut result.terminfo_strings, key, raw)
                    }
                };
                if raw.is_null() || raw as isize == -1 {
                    continue;
                }
                // SAFETY: successful string lookup returns a NUL-terminated
                // allocation, valid until another native operation changes it.
                // The shared lock stays held through copying into owned bytes.
                let value = unsafe { CStr::from_ptr(raw) }.to_bytes().to_vec();
                map.insert((*key).to_owned(), value);
            }
            Query::TermcapNumber(key) => {
                // SAFETY: valid name and active terminal under NATIVE.
                let value = unsafe { tgetnum(name.as_ptr()) };
                if value >= 0 {
                    result.numbers.insert((*key).to_owned(), value);
                }
            }
            Query::Flag(capability) => {
                let (map, key, value) = match capability {
                    FlagCapability::Termcap(key) => {
                        // SAFETY: valid name and active terminal under NATIVE.
                        (&mut result.termcap_flags, key, unsafe {
                            tgetflag(name.as_ptr())
                        })
                    }
                    FlagCapability::Terminfo(key) => {
                        // SAFETY: same invariants; -1 means invalid name, not true.
                        (&mut result.terminfo_flags, key, unsafe {
                            tigetflag(name.as_ptr())
                        })
                    }
                };
                map.insert((*key).to_owned(), value > 0);
            }
        }
    }
    Ok(result)
}

// Only numeric.rs calls this function, after checking every directive (including
// branches that are not taken). It is not part of the crate's public interface.
pub(super) fn expand(
    program: &crate::numeric::NumericProgram,
    parameters: [i32; 9],
) -> Result<Vec<u8>, Error> {
    let guard = NATIVE.lock().map_err(|_| Error::NativeStatePoisoned)?;
    if let Some(division) = program.division() {
        let mut variables = [0; 52];
        if let Some(probe) = &division.variable_probe {
            // Reading variables does not assign them. On native versions where
            // lowercase variables reset per call, this observes their initial
            // zero; on older versions it observes their preserved values.
            let output = call_numeric(&guard, probe, [0; 9])?;
            let mut values = output.split(|byte| *byte == b';');
            for name in &division.variables {
                variables[crate::numeric::division::variable(*name)] = values
                    .next()
                    .and_then(|value| std::str::from_utf8(value).ok())
                    .and_then(|value| value.parse().ok())
                    .ok_or(Error::InvalidNumericFormat)?;
            }
        }
        division.validate(program.as_bytes(), program.tokens(), parameters, variables)?;
    }
    call_numeric(&guard, program.format(), parameters)
}

// Caller holds the process-wide lock and supplies a completely numeric format:
// either validated user input or the fixed variable-reading probe above.
fn call_numeric(
    _guard: &MutexGuard<'_, ()>,
    format: &CStr,
    parameters: [i32; 9],
) -> Result<Vec<u8>, Error> {
    let p = parameters.map(c_long::from);
    // SAFETY: the caller's complete numeric grammar excludes all conversions
    // and operations that consume strings. Thus ncurses' parameter analysis
    // cannot fetch a pointer from these nine C long slots. The format is live
    // and NUL-terminated, and NATIVE protects state until the result is copied.
    let raw = unsafe {
        tparm(
            format.as_ptr(),
            p[0],
            p[1],
            p[2],
            p[3],
            p[4],
            p[5],
            p[6],
            p[7],
            p[8],
        )
    };
    if raw.is_null() {
        return Err(Error::InvalidNumericFormat);
    }
    // SAFETY: non-null tparm output is a NUL-terminated native buffer; no other
    // native operation can reuse/free it while NATIVE is held.
    Ok(unsafe { CStr::from_ptr(raw) }.to_bytes().to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sigttou_is_blocked() -> bool {
        unsafe {
            let mut current: libc::sigset_t = std::mem::zeroed();
            assert_eq!(
                libc::pthread_sigmask(libc::SIG_SETMASK, std::ptr::null(), &raw mut current),
                0
            );
            libc::sigismember(&raw const current, libc::SIGTTOU) == 1
        }
    }

    fn sigttou_handler() -> libc::sighandler_t {
        unsafe {
            let mut action: libc::sigaction = std::mem::zeroed();
            assert_eq!(
                libc::sigaction(libc::SIGTTOU, std::ptr::null(), &raw mut action),
                0
            );
            action.sa_sigaction
        }
    }

    #[test]
    fn tty_out_signal_guard_blocks_and_restores_without_changing_disposition() {
        let handler_before = sigttou_handler();
        // Ensure SIGTTOU starts unblocked on this thread.
        unsafe {
            let mut set: libc::sigset_t = std::mem::zeroed();
            assert_eq!(libc::sigemptyset(&raw mut set), 0);
            assert_eq!(libc::sigaddset(&raw mut set, libc::SIGTTOU), 0);
            assert_eq!(
                libc::pthread_sigmask(libc::SIG_UNBLOCK, &raw const set, std::ptr::null_mut()),
                0
            );
        }
        assert!(!sigttou_is_blocked());

        {
            let _guard = TtyOutSignalGuard::block();
            assert!(
                sigttou_is_blocked(),
                "SIGTTOU must be blocked while TtyOutSignalGuard is live"
            );
            assert_eq!(
                sigttou_handler(),
                handler_before,
                "TtyOutSignalGuard must not alter SIGTTOU disposition"
            );
        }

        assert!(
            !sigttou_is_blocked(),
            "SIGTTOU must be restored to unblocked when TtyOutSignalGuard drops"
        );
        assert_eq!(sigttou_handler(), handler_before);
    }

    #[test]
    fn tty_out_signal_guard_preserves_preexisting_sigttou_block() {
        let _outer = TtyOutSignalGuard::block();
        assert!(sigttou_is_blocked());
        {
            let _inner = TtyOutSignalGuard::block();
            assert!(sigttou_is_blocked());
        }
        assert!(
            sigttou_is_blocked(),
            "dropping inner guard must restore oldset with SIGTTOU still blocked"
        );
    }

    #[test]
    fn tty_out_signal_guard_restores_mask_on_panic() {
        unsafe {
            let mut set: libc::sigset_t = std::mem::zeroed();
            assert_eq!(libc::sigemptyset(&raw mut set), 0);
            assert_eq!(libc::sigaddset(&raw mut set, libc::SIGTTOU), 0);
            assert_eq!(
                libc::pthread_sigmask(libc::SIG_UNBLOCK, &raw const set, std::ptr::null_mut()),
                0
            );
        }
        let result = std::panic::catch_unwind(|| {
            let _guard = TtyOutSignalGuard::block();
            assert!(sigttou_is_blocked());
            panic!("intentional unwind inside SIGTTOU block");
        });
        assert!(result.is_err());
        assert!(
            !sigttou_is_blocked(),
            "SIGTTOU must be unblocked after unwinding through TtyOutSignalGuard"
        );
    }

    #[test]
    fn database_load_restores_sigttou_mask_and_disposition() {
        let handler_before = sigttou_handler();
        unsafe {
            let mut set: libc::sigset_t = std::mem::zeroed();
            assert_eq!(libc::sigemptyset(&raw mut set), 0);
            assert_eq!(libc::sigaddset(&raw mut set, libc::SIGTTOU), 0);
            assert_eq!(
                libc::pthread_sigmask(libc::SIG_UNBLOCK, &raw const set, std::ptr::null_mut()),
                0
            );
        }
        let _ = Database::load("xterm", &[Query::String(StringCapability::Termcap("md"))]);
        assert!(!sigttou_is_blocked());
        assert_eq!(sigttou_handler(), handler_before);
    }
}
