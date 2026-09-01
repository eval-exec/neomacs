//! Serial-port devices (per-facility platform module).
//!
//! This is a PAL facility because GNU Emacs makes it one. `serial_open` and
//! `serial_configure` are declared ONCE, in `src/systty.h:90-91`, and
//! implemented TWICE: termios in `src/sysdep.c:2980` / `:3151`, and Win32
//! `CreateFile` + `DCB` in `src/w32.c:11098` / `:11138`. A serial port is a
//! `termios` device on Unix and a COM handle elsewhere, so the two calls GNU
//! puts behind that header are exactly the two this module exports. Nothing
//! above it names `termios`, `tcgetattr` or a `Bnnn` constant.
//!
//! One thing is deliberately NOT here that is in GNU's `serial_configure`: the
//! keyword validation. GNU validates `:bytesize` / `:parity` / `:stopbits` /
//! `:flowcontrol` inside each platform implementation, so both copies carry the
//! same four `error` messages and the same 8/N/1 defaults, and w32 has its own
//! transcription of them (src/w32.c:11138-11290). Here the Lisp half is lifted
//! into the caller and reaches the device as the four narrowed enums below --
//! but see [`SerialPort::configure`], which keeps GNU's *ordering* even though
//! it moved GNU's *code*.

use std::io;

/// Bits per byte. GNU accepts only 7 or 8 (`serial_configure`,
/// src/sysdep.c:3193-3195); this arrives already narrowed, so the device layer
/// has no domain error left to raise.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SerialByteSize {
    Seven,
    Eight,
}

/// GNU's `:parity` domain: nil, `even' or `odd' (src/sysdep.c:3211-3213).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SerialParity {
    None,
    Even,
    Odd,
}

/// GNU's `:stopbits` domain: 1 or 2 (src/sysdep.c:3247-3249).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SerialStopBits {
    One,
    Two,
}

/// GNU's `:flowcontrol` domain: nil, `hw' (RTS/CTS) or `sw' (XON/XOFF)
/// (src/sysdep.c:3266-3268).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SerialFlowControl {
    None,
    Hardware,
    Software,
}

/// The two device calls that bracket GNU's `serial_configure`, named so the
/// caller can produce GNU's Lisp-visible message for whichever one failed.
///
/// They are a pair on purpose: everything between them happens in a LOCAL copy
/// of the attributes, which is why GNU can validate `:bytesize` after having
/// already applied the speed and still leave the device untouched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SerialConfigureStep {
    /// GNU `tcgetattr` -> `report_file_error ("Failed tcgetattr", Qnil)`
    /// (src/sysdep.c:3164-3166).
    ReadAttributes,
    /// GNU `tcsetattr` -> `report_file_error ("Failed tcsetattr", Qnil)`
    /// (src/sysdep.c:3303-3305).
    WriteAttributes,
}

/// A device call failed, or the caller's own settings step did.
///
/// `E` is the caller's error type, so the Lisp half keeps its own errors and
/// the device half keeps errnos; neither can be mistaken for the other.
#[derive(Debug)]
pub enum SerialConfigureFailure<E> {
    Device {
        step: SerialConfigureStep,
        errno: i32,
    },
    Settings(E),
}

/// A `cfsetspeed` failure. Separate from [`SerialConfigureFailure`] because GNU
/// is the only step whose error names the value that caused it:
/// `report_file_error ("Failed cfsetspeed", tem)`, src/sysdep.c:3181-3183.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SerialSpeedError {
    pub errno: i32,
}

std::cfg_select! {
    unix => {
        mod termios_backend;
        use termios_backend as backend;
    }
    _ => {
        mod unsupported_backend;
        use unsupported_backend as backend;
    }
}

/// One open serial device.
///
/// There is no constructor but [`SerialPort::open`], no `Default` and no
/// `Clone`: a value of this type is proof that GNU's `serial_open`
/// (src/process.c:3212) has run and succeeded. `make-serial-process` cannot
/// build a process record without one, which is the whole of DIVERGENCES.md
/// entry 147.
#[derive(Debug)]
pub struct SerialPort(backend::Device);

/// A local, modifiable copy of a device's line settings.
///
/// GNU's `struct termios attr` on `serial_configure`'s stack. Handing it out
/// rather than a settings struct is what lets the caller's validation run
/// between the read and the write, which is where GNU's runs.
#[derive(Debug)]
pub struct SerialAttributes(backend::Attributes);

impl SerialPort {
    /// GNU `serial_open`, src/sysdep.c:2980-2990: `open (O_RDWR | O_NOCTTY |
    /// O_NONBLOCK)` then a best-effort `TIOCEXCL`.
    ///
    /// The error is returned raw so the caller can run it through the same
    /// errno classification as `report_file_error` -- `ENOENT` is
    /// `file-missing`, `EACCES` is `permission-denied`, anything else is
    /// `file-error`, all with "Opening serial port" and the port name.
    pub fn open(path: &std::ffi::OsStr) -> io::Result<Self> {
        backend::open(path).map(Self)
    }

    /// GNU `serial_configure`, src/sysdep.c:3151-3309, with the keyword
    /// validation supplied by the caller.
    ///
    /// The read (`tcgetattr` + `cfmakeraw` + `CLOCAL` + `CREAD`) always
    /// happens first, `settings` always runs on the local copy, and the write
    /// (`tcsetattr (TCSANOW)`) always happens last and only if `settings`
    /// succeeded. GNU gets that ordering from statement order inside one
    /// function; this gets it from the type, and the ordering is observable:
    ///
    /// * a device that is not a tty reports `Failed tcgetattr` even when the
    ///   keywords are also invalid -- measured, `:port "/dev/null" :speed 9600
    ///   :bytesize 5` is `(file-error "Failed tcgetattr" ...)` under GNU
    ///   31.0.90, not the `:bytesize` message;
    /// * an invalid keyword leaves the device UNCHANGED, because nothing has
    ///   been written back yet.
    ///
    /// Neither can be got wrong by a caller: there is no way to reach the
    /// attributes without the read having happened, and no way to reach the
    /// write except by returning `Ok` from `settings`.
    pub fn configure<E>(
        &self,
        settings: impl FnOnce(&mut SerialAttributes) -> Result<(), E>,
    ) -> Result<(), SerialConfigureFailure<E>> {
        let mut attributes = SerialAttributes(self.0.read_attributes().map_err(|errno| {
            SerialConfigureFailure::Device {
                step: SerialConfigureStep::ReadAttributes,
                errno,
            }
        })?);
        settings(&mut attributes).map_err(SerialConfigureFailure::Settings)?;
        self.0
            .write_attributes(&attributes.0)
            .map_err(|errno| SerialConfigureFailure::Device {
                step: SerialConfigureStep::WriteAttributes,
                errno,
            })
    }

    /// Register this device's readable edge with the wait `poller`, keyed by
    /// process id, using the same level-triggered policy as every other
    /// process-output source.
    pub fn register_readable(
        &self,
        poller: &polling::Poller,
        id: crate::emacs_core::process::ProcessId,
    ) -> Result<(), String> {
        self.0.register_readable(poller, id)
    }

    /// Change this device's poll interest (readable, writable, or both).
    pub fn modify_interest(
        &self,
        poller: &polling::Poller,
        event: polling::Event,
    ) -> Result<(), String> {
        self.0.modify_interest(poller, event)
    }

    /// Remove this device from `poller`.
    pub fn unregister(&self, poller: &polling::Poller) {
        self.0.unregister(poller);
    }
}

impl SerialAttributes {
    /// GNU `cfsetspeed (&attr, convert_speed (XFIXNUM (tem)))`,
    /// src/sysdep.c:3181.
    pub fn set_speed(&mut self, speed: i64) -> Result<(), SerialSpeedError> {
        self.0.set_speed(speed)
    }

    /// GNU's `CSIZE`/`CS7`/`CS8` arm, src/sysdep.c:3197-3199.
    pub fn set_byte_size(&mut self, size: SerialByteSize) {
        self.0.set_byte_size(size);
    }

    /// GNU's `PARENB`/`PARODD`/`IGNPAR`/`INPCK` arm, src/sysdep.c:3214-3232.
    pub fn set_parity(&mut self, parity: SerialParity) {
        self.0.set_parity(parity);
    }

    /// GNU's `CSTOPB` arm, src/sysdep.c:3251-3253.
    pub fn set_stop_bits(&mut self, bits: SerialStopBits) {
        self.0.set_stop_bits(bits);
    }

    /// GNU's `CRTSCTS`/`IXON`/`IXOFF` arm, src/sysdep.c:3269-3299.
    pub fn set_flow_control(&mut self, flow: SerialFlowControl) {
        self.0.set_flow_control(flow);
    }
}

impl io::Read for SerialPort {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.0.read(buffer)
    }
}

impl io::Write for SerialPort {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.0.write(buffer)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
