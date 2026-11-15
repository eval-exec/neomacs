//! Serial-port devices (per-facility platform module).
//!
//! This is a PAL facility because GNU Emacs makes it one. `serial_open` and
//! `serial_configure` are declared ONCE, in `src/systty.h:90-91`, and
//! implemented TWICE: termios in `src/sysdep.c:2980` / `:3151`, and Win32
//! `CreateFile` + `DCB` in `src/w32.c:11098` / `:11138`. A serial port is a
//! `termios` device on Unix and a COM handle on Windows, so the two calls GNU
//! puts behind that header are exactly the two this module exports. Nothing
//! above it names `termios`, `tcgetattr`, `DCB` or `SetCommState`.
//!
//! One thing is deliberately NOT here that is in GNU's `serial_configure`: the
//! keyword validation. GNU validates `:bytesize` / `:parity` / `:stopbits` /
//! `:flowcontrol` inside each platform implementation, so both copies carry the
//! same four `error` messages and the same 8/N/1 defaults, and w32 has its own
//! transcription of them (src/w32.c:11138-11290). Here the Lisp half is lifted
//! into the caller and reaches the device as the four narrowed enums below --
//! but see [`SerialPort::configure`], which keeps GNU's *ordering* even though
//! it moved GNU's *code*.
//!
//! The two platform halves do NOT agree on error shapes, and those shapes are
//! observable, so they live here at the seam rather than in the caller:
//! sysdep.c's failures go through `report_file_error` (the `file-error` family
//! with errnos) while w32.c's are plain `error (...)` with fixed texts. The
//! [`SerialOpenFailure::into_lisp_error`] and
//! [`SerialDeviceFailure::into_lisp_error`] constructors below are the only
//! place that difference is visible.

pub use neovm_eval::emacs_core::eval::data::process_state::SerialPort;

use backend::DeviceIo;
use neovm_value::error::Flow;
#[cfg(windows)]
use neovm_value::error::signal;
use neovm_value::value::Value;
// The `io`/`signal_file_errno` halves belong to the non-w32 error shapes: the
// w32 arms raise plain `error` texts instead.
#[cfg(not(windows))]
use std::io;

#[cfg(not(windows))]
use crate::emacs_core::process::signal_file_errno;

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

/// The device calls that bracket GNU's `serial_configure`, named so the caller
/// can produce GNU's Lisp-visible message for whichever one failed.
///
/// Unix brackets its keyword arms with two calls and w32 with four, but the
/// bracket SHAPE is the same on both sides: everything before the keywords
/// happens in one read bracket (unix `tcgetattr` + `cfmakeraw` + `CLOCAL` +
/// `CREAD`; w32 `GetCommTimeouts` + `SetCommTimeouts` + `GetCommState` +
/// defaults), everything after them in one write bracket (unix `tcsetattr`,
/// w32 `SetCommState`). Everything between happens in a LOCAL copy of the
/// attributes, which is why GNU can validate `:bytesize` after having already
/// applied the speed and still leave the device untouched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SerialConfigureStep {
    /// GNU `tcgetattr` -> `report_file_error ("Failed tcgetattr", Qnil)`
    /// (src/sysdep.c:3164-3166).
    #[cfg(unix)]
    ReadAttributes,
    /// GNU `tcsetattr` -> `report_file_error ("Failed tcsetattr", Qnil)`
    /// (src/sysdep.c:3303-3305).
    #[cfg(unix)]
    WriteAttributes,
    /// w32 `error ("GetCommTimeouts() failed")` (src/w32.c:11152-11153).
    #[cfg(windows)]
    GetCommTimeouts,
    /// w32 `error ("SetCommTimeouts() failed")` (src/w32.c:11159-11160).
    #[cfg(windows)]
    SetCommTimeouts,
    /// w32 `error ("GetCommState() failed")` (src/w32.c:11165-11166).
    #[cfg(windows)]
    GetCommState,
    /// w32 `error ("SetCommState() failed")` (src/w32.c:11281-11282).
    #[cfg(windows)]
    SetCommState,
}

/// Why a serial open failed. The variants are the error texts GNU's two
/// `serial_open`s can produce; each platform's backend returns the ones its
/// source can reach.
#[derive(Debug)]
pub(crate) enum SerialOpenFailure {
    /// The `open(2)` itself failed; the caller classifies by errno, exactly
    /// as `report_file_error` does (src/sysdep.c:2984).
    #[cfg(not(windows))]
    Open(io::Error),
    /// w32 `CreateFile` refused the port: `error ("Could not open %s", port)`
    /// (src/w32.c:11102-11103). The `GetLastError` code is not part of GNU's
    /// message, so it is not carried.
    #[cfg(windows)]
    CouldNotOpen,
    /// w32 `error ("Could not create read event")` (src/w32.c:11128-11129).
    #[cfg(windows)]
    CreateReadEvent,
    /// w32 `error ("Could not create write event")` (src/w32.c:11132-11133).
    #[cfg(windows)]
    CreateWriteEvent,
}

impl SerialOpenFailure {
    /// The Lisp error a failed serial open reports. sysdep.c folds it into
    /// the shared `file-error` classification with the port as the file name
    /// (`report_file_error ("Opening serial port", port)`); w32.c raises plain
    /// `error` texts, one of which interpolates the port.
    ///
    /// `port_name` is the port as text for w32's `%s`; the `file-error` arm
    /// carries the port as Lisp data, which is what its callers already hold.
    pub(crate) fn into_lisp_error(self, port: Value, port_name: &str) -> Flow {
        #[cfg(not(windows))]
        {
            let _ = port_name;
            match self {
                Self::Open(err) => signal_file_errno(
                    "Opening serial port",
                    port,
                    err.raw_os_error().unwrap_or(libc::EIO),
                ),
            }
        }
        #[cfg(windows)]
        {
            let _ = port;
            let message = match self {
                Self::CouldNotOpen => format!("Could not open {port_name}"),
                Self::CreateReadEvent => "Could not create read event".to_owned(),
                Self::CreateWriteEvent => "Could not create write event".to_owned(),
            };
            signal("error", vec![Value::string(&message)])
        }
    }
}

/// One of the bracketing device calls failed: which one, and the errno /
/// `GetLastError` code it failed with (the w32 error texts do not print the
/// code, but the errno half of the unix messages does, so both are carried).
#[derive(Clone, Copy, Debug)]
pub(crate) struct SerialDeviceFailure {
    pub(crate) step: SerialConfigureStep,
    pub(crate) errno: i32,
}

impl SerialDeviceFailure {
    /// The Lisp error a failed device call reports. Unix goes through
    /// `report_file_error` with `Qnil` as the file name; w32 raises plain
    /// `error` with the failing call's name.
    pub(crate) fn into_lisp_error(self) -> Flow {
        #[cfg(unix)]
        {
            let message = match self.step {
                SerialConfigureStep::ReadAttributes => "Failed tcgetattr",
                SerialConfigureStep::WriteAttributes => "Failed tcsetattr",
            };
            signal_file_errno(message, Value::NIL, self.errno)
        }
        #[cfg(windows)]
        {
            let message = match self.step {
                SerialConfigureStep::GetCommTimeouts => "GetCommTimeouts() failed",
                SerialConfigureStep::SetCommTimeouts => "SetCommTimeouts() failed",
                SerialConfigureStep::GetCommState => "GetCommState() failed",
                SerialConfigureStep::SetCommState => "SetCommState() failed",
            };
            signal("error", vec![Value::string(message)])
        }
        // Every step variant belongs to a backend, and the no-backend arm
        // leaves the enum empty: a failure here is unrepresentable.
        #[cfg(not(any(unix, windows)))]
        {
            match self.step {}
        }
    }
}

/// A `cfsetspeed` failure. Separate from [`SerialDeviceFailure`] because on
/// Unix this is the only step whose error names the value that caused it:
/// `report_file_error ("Failed cfsetspeed", tem)`, src/sysdep.c:3181-3183.
/// Windows has no counterpart: `dcb.BaudRate = XFIXNUM (tem)` stores the raw
/// number (src/w32.c:11170-11176), so the w32 attributes never fail here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SerialSpeedError {
    pub errno: i32,
}

std::cfg_select! {
    unix => {
        mod termios_backend;
        use termios_backend as backend;
    }
    windows => {
        mod w32_backend;
        use w32_backend as backend;
    }
    _ => {
        mod unsupported_backend;
        use unsupported_backend as backend;
    }
}

/// GNU's Win32 `DCB` keyword arithmetic, kept cfg-free so its tests run on any
/// host (the same way the termios `SPEEDS` table is host data).
#[cfg(any(windows, test))]
pub(crate) mod dcb_settings;

/// A local, modifiable copy of a device's line settings.
///
/// GNU's `struct termios attr` on `serial_configure`'s stack (or its `DCB dcb`,
/// on w32). Handing it out rather than a settings struct is what lets the
/// caller's validation run between the read and the write, which is where
/// GNU's runs.
#[derive(Debug)]
pub struct SerialAttributes(backend::Attributes);

pub(crate) trait SerialPortIo {
    /// GNU `serial_open`, src/sysdep.c:2980-2990 (termios) / src/w32.c:11098
    /// (COM handle).
    ///
    /// The failure names which of GNU's error texts applies, so the caller can
    /// raise the platform's exact shape through
    /// [`SerialOpenFailure::into_lisp_error`].
    fn open(path: &std::ffi::OsStr) -> Result<SerialPort, SerialOpenFailure>;
    /// GNU `serial_configure`, src/sysdep.c:3151-3309 (termios) /
    /// src/w32.c:11138-11310 (DCB), with the keyword validation supplied by
    /// the caller.
    ///
    /// The read bracket always happens first, `settings` always runs on the
    /// local copy, and the write bracket always happens last and only if
    /// `settings` succeeded. GNU gets that ordering from statement order
    /// inside one function; this gets it from the type, and the ordering is
    /// observable:
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
    fn configure<E>(
        &self,
        settings: impl FnOnce(&mut SerialAttributes) -> Result<(), E>,
    ) -> Result<(), SerialConfigureFailure<E>>;
    /// Register this device's readable edge with the wait `poller`, keyed by
    /// process id, using the same level-triggered policy as every other
    /// process-output source.
    fn register_readable(
        &self,
        poller: &polling::Poller,
        id: crate::emacs_core::process::ProcessId,
    ) -> Result<(), String>;
    /// Change this device's poll interest (readable, writable, or both).
    fn modify_interest(
        &self,
        poller: &polling::Poller,
        event: polling::Event,
    ) -> Result<(), String>;
    /// Remove this device from `poller`.
    fn unregister(&self, poller: &polling::Poller);
}

impl SerialPortIo for SerialPort {
    fn open(path: &std::ffi::OsStr) -> Result<Self, SerialOpenFailure> {
        backend::open(path).map(Self)
    }

    fn configure<E>(
        &self,
        settings: impl FnOnce(&mut SerialAttributes) -> Result<(), E>,
    ) -> Result<(), SerialConfigureFailure<E>> {
        let mut attributes = SerialAttributes(
            self.0
                .read_attributes()
                .map_err(SerialConfigureFailure::Device)?,
        );
        settings(&mut attributes).map_err(SerialConfigureFailure::Settings)?;
        self.0
            .write_attributes(&attributes.0)
            .map_err(SerialConfigureFailure::Device)
    }

    fn register_readable(
        &self,
        poller: &polling::Poller,
        id: crate::emacs_core::process::ProcessId,
    ) -> Result<(), String> {
        self.0.register_readable(poller, id)
    }

    fn modify_interest(
        &self,
        poller: &polling::Poller,
        event: polling::Event,
    ) -> Result<(), String> {
        self.0.modify_interest(poller, event)
    }

    fn unregister(&self, poller: &polling::Poller) {
        self.0.unregister(poller);
    }
}

/// A device call failed, or the caller's own settings step did.
///
/// `E` is the caller's error type, so the Lisp half keeps its own errors and
/// the device half keeps its failing call; neither can be mistaken for the
/// other.
#[derive(Debug)]
pub enum SerialConfigureFailure<E> {
    Device(SerialDeviceFailure),
    Settings(E),
}

impl SerialAttributes {
    /// GNU `cfsetspeed (&attr, convert_speed (XFIXNUM (tem)))`,
    /// src/sysdep.c:3181. The w32 arm stores the raw number and cannot fail
    /// (src/w32.c:11170-11176).
    pub fn set_speed(&mut self, speed: i64) -> Result<(), SerialSpeedError> {
        self.0.set_speed(speed)
    }

    /// GNU's `CSIZE`/`CS7`/`CS8` arm, src/sysdep.c:3197-3199 (w32:
    /// `dcb.ByteSize`, src/w32.c:11183-11192).
    pub fn set_byte_size(&mut self, size: SerialByteSize) {
        self.0.set_byte_size(size);
    }

    /// GNU's `PARENB`/`PARODD`/`IGNPAR`/`INPCK` arm, src/sysdep.c:3214-3232
    /// (w32: `fParity`/`Parity`/`fErrorChar`, src/w32.c:11206-11238).
    pub fn set_parity(&mut self, parity: SerialParity) {
        self.0.set_parity(parity);
    }

    /// GNU's `CSTOPB` arm, src/sysdep.c:3251-3253 (w32: `StopBits`,
    /// src/w32.c:11240-11249).
    pub fn set_stop_bits(&mut self, bits: SerialStopBits) {
        self.0.set_stop_bits(bits);
    }

    /// GNU's `CRTSCTS`/`IXON`/`IXOFF` arm, src/sysdep.c:3269-3299 (w32: the
    /// handshake block, src/w32.c:11251-11276).
    pub fn set_flow_control(&mut self, flow: SerialFlowControl) {
        self.0.set_flow_control(flow);
    }
}
