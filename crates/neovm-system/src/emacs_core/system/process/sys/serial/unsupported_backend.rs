//! Serial-device backend for platforms with no implementation yet.
//!
//! GNU has two (`src/sysdep.c` for termios, `src/w32.c:11098` for `CreateFile`
//! + `DCB`); neomacs has the termios one. Rather than a stub whose methods
//! return errors that can never be observed, [`Device`] is UNINHABITED: the
//! only way to obtain one is [`open`], and [`open`] never returns `Ok`. Every
//! method below is therefore a `match *self {}` that rustc proves unreachable,
//! and adding a w32 backend means giving this type fields -- at which point the
//! compiler asks for real bodies.

use neovm_eval::emacs_core::eval::data::process_state::Device;

use std::ffi::OsStr;
use std::io;

use super::{
    SerialByteSize, SerialFlowControl, SerialParity, SerialSpeedError, SerialStopBits,
};
use crate::emacs_core::process::ProcessId;



#[derive(Debug)]
pub enum Attributes {}

/// GNU `serial_open`. There is no serial device facility on this platform, so
/// the open fails the way `open(2)` fails for an unsupported operation, and
/// `make-serial-process` reports it as `(file-error "Opening serial port" ...)`
/// -- the same shape as any other failed open.
pub fn open(_path: &OsStr) -> io::Result<Device> {
    Err(io::Error::from(io::ErrorKind::Unsupported))
}

pub(crate) trait DeviceIo {
    fn read_attributes(&self) -> Result<Attributes, i32>;
    fn write_attributes(&self, _attributes: &Attributes) -> Result<(), i32>;
    fn register_readable(
        &self,
        _poller: &polling::Poller,
        _id: ProcessId,
    ) -> Result<(), String>;
    fn modify_interest(
        &self,
        _poller: &polling::Poller,
        _event: polling::Event,
    ) -> Result<(), String>;
    fn unregister(&self, _poller: &polling::Poller);
}

impl DeviceIo for Device {
    fn read_attributes(&self) -> Result<Attributes, i32> {
        match *self {}
    }

    fn write_attributes(&self, _attributes: &Attributes) -> Result<(), i32> {
        match *self {}
    }

    fn register_readable(
        &self,
        _poller: &polling::Poller,
        _id: ProcessId,
    ) -> Result<(), String> {
        match *self {}
    }

    fn modify_interest(
        &self,
        _poller: &polling::Poller,
        _event: polling::Event,
    ) -> Result<(), String> {
        match *self {}
    }

    fn unregister(&self, _poller: &polling::Poller) {
        match *self {}
    }
}

impl Attributes {
    pub fn set_speed(&mut self, _speed: i64) -> Result<(), SerialSpeedError> {
        match *self {}
    }

    pub fn set_byte_size(&mut self, _size: SerialByteSize) {
        match *self {}
    }

    pub fn set_parity(&mut self, _parity: SerialParity) {
        match *self {}
    }

    pub fn set_stop_bits(&mut self, _bits: SerialStopBits) {
        match *self {}
    }

    pub fn set_flow_control(&mut self, _flow: SerialFlowControl) {
        match *self {}
    }
}
