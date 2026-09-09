//! Synchronous ncurses padding bridge. All native state uses the same lock as
//! lookup/tparm. A temporary setupterm context never enters tgetent's cache.
use super::*;
use std::cell::Cell;
use std::io::{self, Write};

unsafe extern "C" {
    fn setupterm(term: *const c_char, fd: c_int, error: *mut c_int) -> c_int;
    fn del_curterm(term: *mut c_void) -> c_int;
    fn baudrate() -> c_int;
    #[cfg(target_os = "macos")]
    static mut ospeed: std::ffi::c_short;
    fn tputs(sequence: *const c_char, lines: c_int, emit: extern "C" fn(c_int) -> c_int) -> c_int;
}

// This pointer is installed only during synchronous tputs. Its pointee stays
// on the calling thread's stack and the scope guard clears it before returning.
thread_local! { static SINK: Cell<*mut c_void> = const { Cell::new(std::ptr::null_mut()) }; }
struct Sink<'a> {
    writer: &'a mut dyn Write,
    error: Option<io::Error>,
    panic: Option<Box<dyn std::any::Any + Send>>,
}
extern "C" fn emit(byte: c_int) -> c_int {
    // Never allow a Rust panic to unwind across C. Re-raise it after restoring
    // native state and clearing the callback pointer.
    SINK.with(|slot| {
        let ptr = slot.get();
        if ptr.is_null() {
            return -1;
        }
        // SAFETY: write_padded installed a unique, live Sink on this thread.
        // ncurses calls back synchronously; neither it nor the writer can
        // access the Sink except through this callback.
        let sink = unsafe { &mut *ptr.cast::<Sink<'_>>() };
        if sink.error.is_some() || sink.panic.is_some() {
            return -1;
        }
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            sink.writer.write_all(&[byte as u8])?;
            // tputs may sleep between callbacks (npc). Flush to the actual
            // writer now, so delay precedes subsequent bytes on the device.
            sink.writer.flush()
        })) {
            Ok(Ok(())) => byte,
            Ok(Err(error)) => {
                sink.error = Some(error);
                -1
            }
            Err(panic) => {
                sink.panic = Some(panic);
                -1
            }
        }
    })
}

struct Context {
    previous: *mut c_void,
}
impl Drop for Context {
    fn drop(&mut self) {
        SINK.with(|slot| slot.set(std::ptr::null_mut()));
        // SAFETY: NATIVE is still locked. Detaching before setupterm forced a
        // fresh allocation, independent of the previous/tgetent-cached term.
        // Restore that previous context before freeing only our allocation.
        unsafe {
            let current = set_curterm(self.previous);
            if !current.is_null() {
                del_curterm(current);
            }
        }
    }
}

fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "padding exceeds native arithmetic limits",
    )
}

// Bound native C arithmetic without implementing its padding semantics. tputs
// accumulates tenths of milliseconds, multiplies by affcnt, then delay_output
// multiplies whole milliseconds by baud. Reject overflow before entering C.
fn validate(sequence: &[u8], lines: i32, baud: i32) -> io::Result<()> {
    static MARKER: std::sync::LazyLock<regex::bytes::Regex> = std::sync::LazyLock::new(|| {
        regex::bytes::Regex::new(r"\A\$<([0-9]{0,6})(?:\.([0-9]{0,6}))?([*/]{0,2})>").unwrap()
    });
    // Some ncurses builds accept legacy BSD leading delays. Keep this bridge
    // restricted to terminfo notation rather than feeding that unbounded path.
    if sequence.first().is_some_and(u8::is_ascii_digit) {
        return Err(invalid());
    }
    let mut remaining = sequence;
    while let Some(start) = remaining.windows(2).position(|bytes| bytes == b"$<") {
        remaining = &remaining[start..];
        let Some(cap) = MARKER.captures(remaining) else {
            return Err(invalid());
        };
        let whole = std::str::from_utf8(&cap[1])
            .unwrap()
            .parse::<i32>()
            .unwrap_or(0);
        let fraction = cap
            .get(2)
            .and_then(|part| part.as_bytes().first())
            .map_or(0, |byte| i32::from(byte - b'0'));
        let mut tenths = whole
            .checked_mul(10)
            .and_then(|n| n.checked_add(fraction))
            .ok_or_else(invalid)?;
        for flag in &cap[3] {
            if *flag == b'*' {
                tenths = tenths.checked_mul(lines).ok_or_else(invalid)?;
            }
        }
        (tenths / 10).checked_mul(baud.max(0)).ok_or_else(invalid)?;
        remaining = &remaining[cap.get(0).unwrap().end()..];
    }
    Ok(())
}

pub(crate) fn write_padded(
    term: &str,
    fd: c_int,
    output: &mut dyn Write,
    sequence: &[u8],
    lines: usize,
    device_speed: Option<u32>,
) -> io::Result<()> {
    let term = name(term).map_err(io::Error::other)?;
    let sequence = CString::new(sequence).map_err(io::Error::other)?;
    let lines = i32::try_from(lines).map_err(|_| invalid())?;
    if SINK.with(|slot| !slot.get().is_null()) {
        return Err(io::Error::other("recursive native padding output"));
    }
    output.flush()?;
    let guard = NATIVE
        .lock()
        .map_err(|_| io::Error::other(Error::NativeStatePoisoned))?;
    // SAFETY: exclusive native lock, terminated name, live writable error
    // slot. setupterm only inspects fd; the public Padding owns its descriptor.
    let context = Context {
        previous: unsafe { set_curterm(std::ptr::null_mut()) },
    };
    let mut error = 0;
    let status = unsafe { setupterm(term.as_ptr(), fd, &mut error) };
    if status != 0 {
        return Err(io::Error::other("could not initialize padding terminal"));
    }
    // SAFETY: setupterm established a current terminal under the lock.
    let baud = unsafe { baudrate() };
    #[cfg(target_os = "macos")]
    let baud = correct_apple_speed(baud, device_speed)?;
    #[cfg(not(target_os = "macos"))]
    let _ = device_speed;
    validate(sequence.as_bytes(), lines, baud)?;
    let mut sink = Sink {
        writer: output,
        error: None,
        panic: None,
    };
    SINK.with(|slot| slot.set((&mut sink as *mut Sink<'_>).cast()));
    // SAFETY: live terminated sequence, bounded arithmetic, initialized current
    // terminal. emit cannot unwind into C or retain references beyond this call.
    let status = unsafe { tputs(sequence.as_ptr(), lines, emit) };
    drop(context);
    drop(guard);
    if let Some(panic) = sink.panic {
        std::panic::resume_unwind(panic);
    }
    if let Some(error) = sink.error {
        return Err(error);
    }
    if status != 0 {
        return Err(io::Error::other("native tputs failed"));
    }
    Ok(())
}

/// Apple's ncurses 6.0 def_prog_mode fails without a SCREEN, leaving baudrate
/// zero after setupterm. Supply the public termcap ospeed value from the device.
/// These are Apple's sys/ttydev.h USE_OLD_TTY codes, also used by ncurses'
/// NCURSES_OSPEED_COMPAT configuration (lib_baudrate.c). Do not use private
/// _nc_ospeed or inspect ncurses' opaque TERMINAL layout.
#[cfg(target_os = "macos")]
fn correct_apple_speed(native: i32, device: Option<u32>) -> io::Result<i32> {
    if native != 0 {
        return Ok(native);
    }
    let Some(speed) = device.filter(|speed| *speed != 0) else {
        return Ok(native);
    };
    const SPEEDS: [u32; 18] = [
        0, 50, 75, 110, 134, 150, 200, 300, 600, 1200, 1800, 2400, 4800, 9600, 19200, 38400, 57600,
        115200,
    ];
    let code = SPEEDS
        .iter()
        .position(|value| *value == speed)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::Unsupported,
                "baud rate is not representable by Apple's legacy termcap ABI",
            )
        })?;
    // SAFETY: the native mutex is held and setupterm selected our temporary
    // terminal. ospeed is the documented short termcap global on macOS. The
    // context guard restores the previous terminal and its speed on every exit.
    unsafe {
        ospeed = code as std::ffi::c_short;
    }
    Ok(speed as i32)
}
