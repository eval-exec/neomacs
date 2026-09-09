//! The complete unsafe surface. ncurses owns/caches the selected terminal;
//! Rust owns only snapshots. See README.md for native contracts and validation.
use std::ffi::{CStr, CString, c_char, c_int, c_long, c_void};
use std::sync::Mutex;

use crate::{Database, Error, FlagCapability, Query, StringCapability};

static NATIVE: Mutex<()> = Mutex::new(());

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
    let status = unsafe {
        set_curterm(std::ptr::null_mut());
        tgetent(std::ptr::null_mut(), term.as_ptr())
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
pub(super) fn expand(format: &CStr, parameters: [i32; 9]) -> Result<Vec<u8>, Error> {
    let _guard = NATIVE.lock().map_err(|_| Error::NativeStatePoisoned)?;
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
