//! Child PTY terminal setup (per-facility platform module, Unix only).
//!
//! `configure_child_pty_tty` puts a freshly-allocated child PTY into the mode
//! GNU Emacs uses for subprocesses (`child_setup_tty`, sysdep.c): output
//! post-processing on, NL->CR-NL off, echo off, signals on, canonical mode with
//! erase/kill disabled and EOF = C-d. The case-mapping (`IUCLC`/`OLCUC`) and
//! tab-expansion (`TAB3`) flags exist only on Linux/Android, gated inside
//! exactly as GNU's `#ifdef IUCLC` / `#ifdef OLCUC`. This is a Unix-only
//! facility (Windows has no termios/PTY here), so the whole module is
//! `#[cfg(unix)]`.

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::io::RawFd;

fn close_fd(fd: RawFd) {
    unsafe {
        libc::close(fd);
    }
}

fn set_cc(settings: &mut libc::termios, index: usize, value: u8) {
    if index < settings.c_cc.len() {
        settings.c_cc[index] = value;
    }
}

pub fn configure_child_pty_tty(tty_name: &OsStr) -> Result<(), String> {
    let path = std::ffi::CString::new(tty_name.as_bytes())
        .map_err(|_| "PTY tty name contains an interior NUL".to_string())?;
    let fd = unsafe { libc::open(path.as_ptr(), libc::O_RDWR | libc::O_NOCTTY) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }

    let mut settings = unsafe {
        let mut settings = std::mem::MaybeUninit::<libc::termios>::uninit();
        if libc::tcgetattr(fd, settings.as_mut_ptr()) != 0 {
            let err = std::io::Error::last_os_error().to_string();
            close_fd(fd);
            return Err(err);
        }
        settings.assume_init()
    };

    settings.c_oflag |= libc::OPOST;
    settings.c_oflag &= !libc::ONLCR;
    settings.c_lflag &= !libc::ECHO;
    settings.c_lflag |= libc::ISIG;
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        settings.c_iflag &= !libc::IUCLC;
        settings.c_oflag &= !libc::OLCUC;
    }
    settings.c_iflag &= !libc::ISTRIP;
    #[cfg(any(target_os = "linux", target_os = "android"))]
    {
        settings.c_oflag &= !libc::TAB3;
    }
    settings.c_cflag = (settings.c_cflag & !libc::CSIZE) | libc::CS8;
    set_cc(&mut settings, libc::VERASE, 0);
    set_cc(&mut settings, libc::VKILL, 0);
    settings.c_lflag |= libc::ICANON;
    set_cc(&mut settings, libc::VEOF, 4);

    let result = unsafe { libc::tcsetattr(fd, libc::TCSANOW, &settings) };
    let err = if result != 0 {
        Some(std::io::Error::last_os_error().to_string())
    } else {
        None
    };
    close_fd(fd);
    err.map_or(Ok(()), Err)
}

/// Make the PTY slave at `tty` the child's controlling terminal, on fds 0/1.
///
/// Runs inside a `Command` `pre_exec` closure -- i.e. in the forked child
/// between fork and exec -- so it uses only async-signal-safe syscalls and never
/// allocates. `setsid` is assumed already done (the child is a session leader
/// with no controlling tty), matching GNU's `child_setup`: open the slave, make
/// it the controlling terminal (`TIOCSCTTY`), and `dup2` it onto stdin/stdout,
/// leaving stderr (fd 2) on whatever the parent set up (GNU's
/// forkin/forkout = pty_tty, forkerr = stderr-pipe arrangement).
///
/// # Safety
/// Must only be called from a `pre_exec` context (post-fork, pre-exec), where
/// running async-signal-safe syscalls on the child's fds is sound.
pub unsafe fn establish_pty_controlling_terminal(tty: &std::ffi::CStr) -> std::io::Result<()> {
    let slave = unsafe { libc::open(tty.as_ptr(), libc::O_RDWR | libc::O_NOCTTY) };
    if slave < 0 {
        return Err(std::io::Error::last_os_error());
    }
    #[allow(clippy::cast_lossless)]
    if unsafe { libc::ioctl(slave, libc::TIOCSCTTY as _, 0) } == -1 {
        let err = std::io::Error::last_os_error();
        unsafe { libc::close(slave) };
        return Err(err);
    }
    if unsafe { libc::dup2(slave, libc::STDIN_FILENO) } == -1
        || unsafe { libc::dup2(slave, libc::STDOUT_FILENO) } == -1
    {
        let err = std::io::Error::last_os_error();
        unsafe { libc::close(slave) };
        return Err(err);
    }
    if slave > libc::STDERR_FILENO {
        unsafe { libc::close(slave) };
    }
    Ok(())
}

/// The foreground process-group id of the terminal open on `fd` (`TIOCGPGRP`),
/// or `None` if `fd` is not a tty or has no foreground group. This is GNU's
/// `emacs_get_tty_pgrp` probe, used by `process-running-child-p`.
pub fn fd_foreground_pgrp(fd: RawFd) -> Option<i32> {
    let mut gid: libc::pid_t = -1;
    // SAFETY: `TIOCGPGRP` writes the pgrp through the provided `&mut gid`.
    let ok = unsafe { libc::ioctl(fd, libc::TIOCGPGRP as _, &mut gid) } != -1;
    (ok && gid != -1).then_some(gid)
}

/// Like [`fd_foreground_pgrp`] but for a tty given by path: open it read-only,
/// probe, and close. Used when a process has no live PTY master fd but does
/// know its controlling tty's name.
pub fn tty_path_foreground_pgrp(path: &OsStr) -> Option<i32> {
    let c_path = std::ffi::CString::new(path.as_bytes()).ok()?;
    // SAFETY: `c_path` is a valid C string; the tty is opened read-only.
    let fd = unsafe { libc::open(c_path.as_ptr(), libc::O_RDONLY, 0) };
    if fd == -1 {
        return None;
    }
    let gid = fd_foreground_pgrp(fd);
    close_fd(fd);
    gid
}
