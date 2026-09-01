//! Unix serial-device backend: GNU Emacs `src/sysdep.c:2980-3309`, in Rust.
//!
//! Every `termios` name in the tree is in this file. The parent module hands
//! it narrowed enums and gets back errnos; nothing above it knows that a serial
//! port is a tty.

use std::ffi::{CString, OsStr};
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::OsStrExt;

use super::{
    SerialByteSize, SerialFlowControl, SerialParity, SerialSpeedError, SerialStopBits,
};
use crate::emacs_core::process::{ProcessId, ProcessManager};

#[derive(Debug)]
pub struct Device {
    fd: OwnedFd,
}

#[derive(Debug)]
pub struct Attributes(libc::termios);

/// GNU `serial_open`, src/sysdep.c:2980-2990.
pub fn open(path: &OsStr) -> io::Result<Device> {
    // A port name with an interior NUL cannot be handed to `open(2)`; GNU's
    // `SSDATA` would silently truncate it. Refuse instead, with the errno the
    // truncated name would almost certainly have produced anyway.
    let Ok(c_path) = CString::new(path.as_bytes()) else {
        return Err(io::Error::from_raw_os_error(libc::ENOENT));
    };
    // SAFETY: `c_path` is a valid NUL-terminated string for the call's duration.
    let fd = unsafe {
        libc::open(
            c_path.as_ptr(),
            libc::O_RDWR | libc::O_NOCTTY | libc::O_NONBLOCK,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `open` returned a fresh descriptor this process now owns.
    let fd = unsafe { OwnedFd::from_raw_fd(fd) };
    // GNU asks for exclusive use and ignores the answer (`#ifdef TIOCEXCL`,
    // src/sysdep.c:2985-2987): a port that does not support it is still usable.
    // SAFETY: `TIOCEXCL` takes no pointer argument.
    unsafe {
        libc::ioctl(fd.as_raw_fd(), libc::TIOCEXCL as _, 0 as libc::c_int);
    }
    Ok(Device { fd })
}

impl Device {
    /// GNU's opening of `serial_configure`: `tcgetattr`, then `cfmakeraw`, then
    /// `CLOCAL` and `CREAD` (src/sysdep.c:3164-3172).
    ///
    /// The three are one step because GNU makes them one: there is no point at
    /// which the caller can see a read-but-not-yet-raw attribute set.
    pub fn read_attributes(&self) -> Result<Attributes, i32> {
        let mut attributes = std::mem::MaybeUninit::<libc::termios>::uninit();
        // SAFETY: `tcgetattr` initialises the whole struct on success.
        let mut attributes = unsafe {
            if libc::tcgetattr(self.fd.as_raw_fd(), attributes.as_mut_ptr()) != 0 {
                return Err(last_errno());
            }
            attributes.assume_init()
        };
        // SAFETY: `cfmakeraw` only writes through the provided pointer.
        unsafe { libc::cfmakeraw(&raw mut attributes) };
        attributes.c_cflag |= libc::CLOCAL | libc::CREAD;
        Ok(Attributes(attributes))
    }

    /// GNU `tcsetattr (p->outfd, TCSANOW, &attr)`, src/sysdep.c:3303.
    pub fn write_attributes(&self, attributes: &Attributes) -> Result<(), i32> {
        // SAFETY: `tcsetattr` only reads through the provided pointer.
        let result =
            unsafe { libc::tcsetattr(self.fd.as_raw_fd(), libc::TCSANOW, &raw const attributes.0) };
        if result != 0 {
            return Err(last_errno());
        }
        Ok(())
    }

    pub fn register_readable(
        &self,
        poller: &polling::Poller,
        id: ProcessId,
    ) -> Result<(), String> {
        ProcessManager::register_readable_source(poller, &self.fd, id)
    }

    pub fn modify_interest(
        &self,
        poller: &polling::Poller,
        event: polling::Event,
    ) -> Result<(), String> {
        ProcessManager::modify_poll_source(poller, &self.fd, event)
    }

    pub fn unregister(&self, poller: &polling::Poller) {
        let _ = poller.delete(&self.fd);
    }

    /// `read(2)`, retrying on `EINTR` as std's `Read` implementations do --
    /// GNU reads a serial process through `emacs_read`, whose whole reason for
    /// existing is that retry (src/sysdep.c `emacs_read`).
    pub fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        loop {
            // SAFETY: `read` writes at most `buffer.len()` bytes through the
            // pointer, and the descriptor is owned by `self`.
            let count = unsafe {
                libc::read(
                    self.fd.as_raw_fd(),
                    buffer.as_mut_ptr().cast::<libc::c_void>(),
                    buffer.len(),
                )
            };
            match usize::try_from(count) {
                Ok(count) => return Ok(count),
                Err(_) => {
                    let err = io::Error::last_os_error();
                    if err.kind() != io::ErrorKind::Interrupted {
                        return Err(err);
                    }
                }
            }
        }
    }

    /// `write(2)`, with the same `EINTR` retry (GNU's `emacs_write`).
    pub fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        loop {
            // SAFETY: `write` reads at most `buffer.len()` bytes through the
            // pointer, and the descriptor is owned by `self`.
            let count = unsafe {
                libc::write(
                    self.fd.as_raw_fd(),
                    buffer.as_ptr().cast::<libc::c_void>(),
                    buffer.len(),
                )
            };
            match usize::try_from(count) {
                Ok(count) => return Ok(count),
                Err(_) => {
                    let err = io::Error::last_os_error();
                    if err.kind() != io::ErrorKind::Interrupted {
                        return Err(err);
                    }
                }
            }
        }
    }
}

impl AsRawFd for Device {
    fn as_raw_fd(&self) -> RawFd {
        self.fd.as_raw_fd()
    }
}

impl Attributes {
    pub fn set_speed(&mut self, speed: i64) -> Result<(), SerialSpeedError> {
        // GNU casts the Lisp fixnum straight into `speed_t`
        // (`convert_speed (XFIXNUM (tem))`, src/sysdep.c:3181), so a negative
        // or oversized speed wraps exactly as it does there.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let requested = speed as libc::speed_t;
        // SAFETY: `cfsetspeed` only writes through the provided pointer.
        let result = unsafe { libc::cfsetspeed(&raw mut self.0, convert_speed(requested)) };
        if result != 0 {
            return Err(SerialSpeedError {
                errno: last_errno(),
            });
        }
        Ok(())
    }

    pub fn set_byte_size(&mut self, size: SerialByteSize) {
        self.0.c_cflag &= !libc::CSIZE;
        self.0.c_cflag |= match size {
            SerialByteSize::Seven => libc::CS7,
            SerialByteSize::Eight => libc::CS8,
        };
    }

    pub fn set_parity(&mut self, parity: SerialParity) {
        self.0.c_cflag &= !(libc::PARENB | libc::PARODD);
        self.0.c_iflag &= !(libc::IGNPAR | libc::INPCK);
        match parity {
            SerialParity::None => {}
            SerialParity::Even => {
                self.0.c_cflag |= libc::PARENB;
                self.0.c_iflag |= libc::IGNPAR | libc::INPCK;
            }
            SerialParity::Odd => {
                self.0.c_cflag |= libc::PARENB | libc::PARODD;
                self.0.c_iflag |= libc::IGNPAR | libc::INPCK;
            }
        }
    }

    pub fn set_stop_bits(&mut self, bits: SerialStopBits) {
        self.0.c_cflag &= !libc::CSTOPB;
        if bits == SerialStopBits::Two {
            self.0.c_cflag |= libc::CSTOPB;
        }
    }

    pub fn set_flow_control(&mut self, flow: SerialFlowControl) {
        self.0.c_cflag &= !libc::CRTSCTS;
        self.0.c_iflag &= !(libc::IXON | libc::IXOFF);
        match flow {
            SerialFlowControl::None => {}
            SerialFlowControl::Hardware => self.0.c_cflag |= libc::CRTSCTS,
            SerialFlowControl::Software => self.0.c_iflag |= libc::IXON | libc::IXOFF,
        }
    }
}

fn last_errno() -> i32 {
    io::Error::last_os_error()
        .raw_os_error()
        .unwrap_or(libc::EIO)
}

/// GNU `convert_speed`, src/sysdep.c:3135-3148 (bug#49524): a numerical speed
/// such as 9600 becomes the `B9600` constant, while a value that is already a
/// `Bnnn` constant is passed through, and an unknown value is passed through
/// too so the platform can accept it (Linux `BOTHER`) or refuse it.
fn convert_speed(speed: libc::speed_t) -> libc::speed_t {
    for (value, internal) in SPEEDS.iter().copied() {
        if speed == internal {
            return speed;
        } else if speed == value {
            return internal;
        }
    }
    speed
}

/// GNU's `speeds[]` table, src/sysdep.c:3024-3132. GNU wraps each row in
/// `#ifdef Bnnn`; the equivalent here is that the rows above `B38400` -- the
/// last one POSIX mandates -- exist only where the platform's libc defines
/// them.
#[cfg(any(target_os = "linux", target_os = "android"))]
static SPEEDS: &[(libc::speed_t, libc::speed_t)] = &[
    (0, libc::B0),
    (50, libc::B50),
    (75, libc::B75),
    (110, libc::B110),
    (134, libc::B134),
    (150, libc::B150),
    (200, libc::B200),
    (300, libc::B300),
    (600, libc::B600),
    (1200, libc::B1200),
    (1800, libc::B1800),
    (2400, libc::B2400),
    (4800, libc::B4800),
    (9600, libc::B9600),
    (19200, libc::B19200),
    (38400, libc::B38400),
    (57600, libc::B57600),
    (115_200, libc::B115200),
    (230_400, libc::B230400),
    (460_800, libc::B460800),
    (500_000, libc::B500000),
    (576_000, libc::B576000),
    (921_600, libc::B921600),
    (1_000_000, libc::B1000000),
    (1_152_000, libc::B1152000),
    (1_500_000, libc::B1500000),
    (2_000_000, libc::B2000000),
    (2_500_000, libc::B2500000),
    (3_000_000, libc::B3000000),
    (3_500_000, libc::B3500000),
    (4_000_000, libc::B4000000),
];

#[cfg(not(any(target_os = "linux", target_os = "android")))]
static SPEEDS: &[(libc::speed_t, libc::speed_t)] = &[
    (0, libc::B0),
    (50, libc::B50),
    (75, libc::B75),
    (110, libc::B110),
    (134, libc::B134),
    (150, libc::B150),
    (200, libc::B200),
    (300, libc::B300),
    (600, libc::B600),
    (1200, libc::B1200),
    (1800, libc::B1800),
    (2400, libc::B2400),
    (4800, libc::B4800),
    (9600, libc::B9600),
    (19200, libc::B19200),
    (38400, libc::B38400),
];
