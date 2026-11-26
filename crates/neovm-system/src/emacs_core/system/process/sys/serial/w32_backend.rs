//! Win32 COM serial-device backend: GNU Emacs `src/w32.c:11098-11310`, in
//! Rust.
//!
//! Every `DCB`/`CommTimeouts` name in the tree is in this file or in the
//! cfg-free [`super::dcb_settings`] mirror. The parent module hands it
//! narrowed enums and gets back failing-call names; nothing above it knows
//! that a serial port is a COM handle.
//!
//! GNU's serial_open keeps a CRT fd, a `child_process` slot and a reader
//! thread that blocks in one-char overlapped reads so its `sys_select` can
//! wait on the `char_avail` event (src/w32proc.c:1688-1726). The reader thread
//! exists only because nothing else in GNU can turn a COM handle into a
//! waitable readiness edge; `polling`'s IOCP backend registers waitable
//! HANDLEs directly, so this backend arms the same one-char read-ahead on the
//! main thread and hands the same manual-reset event to the poller. The I/O
//! arms themselves live beside the `Device` in the evaluator
//! (`process_state.rs`), which owns the `OVERLAPPED` blocks.

use neovm_eval::emacs_core::eval::data::process_state::Device;

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{FromRawHandle, OwnedHandle};

use polling::os::iocp::PollerIocpExt;

use super::dcb_settings::{
    SerialDcbSettings, apply_w32_read_defaults, pack_flags, set_w32_byte_size, set_w32_flow_control,
    set_w32_parity, set_w32_speed, set_w32_stop_bits, unpack_flags,
};
use super::{
    SerialByteSize, SerialConfigureStep, SerialDeviceFailure, SerialFlowControl, SerialOpenFailure,
    SerialParity, SerialSpeedError, SerialStopBits,
};
use crate::emacs_core::process::ProcessId;
use windows_sys::Win32::Devices::Communication::{
    DCB, GetCommState, GetCommTimeouts, SetCommState, SetCommTimeouts, COMMTIMEOUTS,
};
use windows_sys::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::Storage::FileSystem::{CreateFileW, FILE_FLAG_OVERLAPPED, OPEN_EXISTING};
use windows_sys::Win32::System::Threading::CreateEventW;

/// A local `DCB` copy: GNU's `DCB dcb` on `serial_configure`'s stack
/// (src/w32.c:11143), with the keyword arithmetic in [`super::dcb_settings`].
#[derive(Debug)]
pub(crate) struct Attributes(pub(crate) SerialDcbSettings);

/// GNU `serial_open`, src/w32.c:11098-11136: `CreateFile (port,
/// GENERIC_READ | GENERIC_WRITE, 0, 0, OPEN_EXISTING, FILE_FLAG_OVERLAPPED,
/// 0)`, then the read/write completion events.
///
/// GNU's CRT-fd step (`_open_osfhandle`) and its child_process slot have no
/// counterpart: the `Device` owns the handle, and the reader thread is
/// replaced by the poller waiting on the read event (see the module docs).
pub(crate) fn open(path: &OsStr) -> Result<Device, SerialOpenFailure> {
    let wide: Vec<u16> = path.encode_wide().chain(std::iter::once(0)).collect();
    // SAFETY: `wide` is NUL-terminated and outlives the call.
    let handle = unsafe {
        CreateFileW(
            wide.as_ptr(),
            GENERIC_READ | GENERIC_WRITE,
            0,
            std::ptr::null(),
            OPEN_EXISTING,
            FILE_FLAG_OVERLAPPED,
            std::ptr::null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(SerialOpenFailure::CouldNotOpen);
    }
    // SAFETY: CreateFileW returned a fresh handle this Device owns.
    let handle = unsafe { OwnedHandle::from_raw_handle(handle) };

    // GNU's `cp->ovl_read.hEvent = CreateEvent (NULL, TRUE, FALSE, NULL)`
    // (manual reset, initially unset) and the same for `ovl_write`
    // (src/w32.c:11127-11133). The Device assembles its read-ahead around
    // the read event; this backend only arms it, at register time.
    let read_event = create_manual_reset_event().ok_or(SerialOpenFailure::CreateReadEvent)?;
    let write_event = create_manual_reset_event().ok_or(SerialOpenFailure::CreateWriteEvent)?;

    Ok(Device::new_serial(handle, read_event, write_event))
}

/// `CreateEventW(NULL, TRUE /*manual reset*/, FALSE, NULL)`, closing the
/// handle again on failure so the pair never leaks a half-built device.
fn create_manual_reset_event() -> Option<OwnedHandle> {
    // SAFETY: no attributes or name are passed.
    let event = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
    if event.is_null() {
        return None;
    }
    // SAFETY: CreateEventW returned a fresh handle this helper owns.
    Some(unsafe { OwnedHandle::from_raw_handle(event) })
}

pub(crate) trait DeviceIo {
    /// GNU's opening of w32 `serial_configure`: zero the `COMMTIMEOUTS`,
    /// `GetCommState`, then the fixed read-side defaults
    /// (src/w32.c:11150-11168). Three device calls, three error texts; the
    /// returned step names the one that failed, and they are one bracket
    /// because there is no point at which the caller can see a partly
    /// configured device.
    fn read_attributes(&self) -> Result<Attributes, SerialDeviceFailure>;
    /// GNU `SetCommState (hnd, &dcb)`, src/w32.c:11281-11282.
    fn write_attributes(&self, attributes: &Attributes) -> Result<(), SerialDeviceFailure>;
}

impl DeviceIo for Device {
    fn read_attributes(&self) -> Result<Attributes, SerialDeviceFailure> {
        let handle = self.serial_handle_raw();
        let mut ct: windows_sys::Win32::Devices::Communication::COMMTIMEOUTS =
            unsafe { std::mem::zeroed() };
        // SAFETY: `ct` is a valid out-param of the right type.
        if unsafe { GetCommTimeouts(handle, &raw mut ct) } == 0 {
            return Err(device_failure(SerialConfigureStep::GetCommTimeouts));
        }
        // "Initialize timeouts for blocking read and blocking write":
        // all five fields zero (src/w32.c:11154-11158).
        ct.ReadIntervalTimeout = 0;
        ct.ReadTotalTimeoutMultiplier = 0;
        ct.ReadTotalTimeoutConstant = 0;
        ct.WriteTotalTimeoutMultiplier = 0;
        ct.WriteTotalTimeoutConstant = 0;
        // SAFETY: `ct` is fully initialized.
        if unsafe { SetCommTimeouts(handle, &raw const ct) } == 0 {
            return Err(device_failure(SerialConfigureStep::SetCommTimeouts));
        }
        let mut dcb: windows_sys::Win32::Devices::Communication::DCB =
            unsafe { std::mem::zeroed() };
        dcb.DCBlength = std::mem::size_of::<
            windows_sys::Win32::Devices::Communication::DCB,
        >() as u32;
        // SAFETY: `dcb` is a valid out-param of the right type.
        if unsafe { GetCommState(handle, &raw mut dcb) } == 0 {
            return Err(device_failure(SerialConfigureStep::GetCommState));
        }
        let mut settings = from_dcb(&dcb);
        apply_w32_read_defaults(&mut settings);
        Ok(Attributes(settings))
    }

    fn write_attributes(&self, attributes: &Attributes) -> Result<(), SerialDeviceFailure> {
        let handle = self.serial_handle_raw();
        let dcb = to_dcb(&attributes.0);
        // SAFETY: `dcb` is fully initialized from the local settings copy.
        if unsafe { SetCommState(handle, &raw const dcb) } == 0 {
            return Err(device_failure(SerialConfigureStep::SetCommState));
        }
        Ok(())
    }
}

impl Attributes {
    /// w32 stores the fixnum raw; no speed table, no failure
    /// (src/w32.c:11170-11176). The signature keeps the termios shape so the
    /// caller's keyword window is shared.
    pub(crate) fn set_speed(&mut self, speed: i64) -> Result<(), SerialSpeedError> {
        set_w32_speed(&mut self.0, speed);
        Ok(())
    }

    pub(crate) fn set_byte_size(&mut self, size: SerialByteSize) {
        set_w32_byte_size(&mut self.0, size);
    }

    pub(crate) fn set_parity(&mut self, parity: SerialParity) {
        set_w32_parity(&mut self.0, parity);
    }

    pub(crate) fn set_stop_bits(&mut self, bits: SerialStopBits) {
        set_w32_stop_bits(&mut self.0, bits);
    }

    pub(crate) fn set_flow_control(&mut self, flow: SerialFlowControl) {
        set_w32_flow_control(&mut self.0, flow);
    }
}

fn device_failure(step: SerialConfigureStep) -> SerialDeviceFailure {
    // GNU's w32 error texts do not print the error code, so the exact value
    // here is diagnostic only; GetLastError is what the failed call left.
    // SAFETY: GetLastError reads thread-local state.
    SerialDeviceFailure {
        step,
        errno: unsafe { windows_sys::Win32::Foundation::GetLastError() } as i32,
    }
}

fn from_dcb(dcb: &windows_sys::Win32::Devices::Communication::DCB) -> SerialDcbSettings {
    let mut settings = SerialDcbSettings {
        dcb_length: dcb.DCBlength,
        baud_rate: dcb.BaudRate,
        xon_lim: dcb.XonLim,
        xoff_lim: dcb.XoffLim,
        byte_size: dcb.ByteSize,
        parity: dcb.Parity,
        stop_bits: dcb.StopBits,
        xon_char: dcb.XonChar,
        xoff_char: dcb.XoffChar,
        error_char: dcb.ErrorChar,
        eof_char: dcb.EofChar,
        evt_char: dcb.EvtChar,
        ..SerialDcbSettings::default()
    };
    unpack_flags(dcb._bitfield, &mut settings);
    settings
}

fn to_dcb(settings: &SerialDcbSettings) -> windows_sys::Win32::Devices::Communication::DCB {
    windows_sys::Win32::Devices::Communication::DCB {
        DCBlength: settings.dcb_length,
        BaudRate: settings.baud_rate,
        _bitfield: pack_flags(settings),
        wReserved: 0,
        XonLim: settings.xon_lim,
        XoffLim: settings.xoff_lim,
        ByteSize: settings.byte_size,
        Parity: settings.parity,
        StopBits: settings.stop_bits,
        XonChar: settings.xon_char,
        XoffChar: settings.xoff_char,
        ErrorChar: settings.error_char,
        EofChar: settings.eof_char,
        EvtChar: settings.evt_char,
        wReserved1: 0,
    }
}
