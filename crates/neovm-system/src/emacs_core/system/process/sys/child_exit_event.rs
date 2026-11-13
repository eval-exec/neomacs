//! One shared native exit set for deleted children, independent of Lisp.
//!
//! Registrations and removals are serialized by the ownership registry mutex.
//! Its one collector can block concurrently with registration. Completion tokens
//! name registry entries, never PIDs that could be reused after a reap.

use std::io;
#[cfg(any(target_os = "linux", target_os = "macos"))]
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

#[derive(Debug)]
pub enum ChildExitNotification {
    Exited(usize),
    // Also constructed by portable tests of Darwin's pre-zombie ordering.
    #[cfg_attr(not(any(target_os = "macos", test)), allow(dead_code))]
    ChildSignal,
    Wake,
}

#[derive(Debug)]
pub struct ChildExitEvent {
    #[cfg(target_os = "linux")]
    queue: OwnedFd,
    #[cfg(target_os = "linux")]
    wakeup: OwnedFd,
    #[cfg(target_os = "linux")]
    children: std::sync::Mutex<std::collections::HashMap<usize, OwnedFd>>,
    #[cfg(target_os = "macos")]
    queue: OwnedFd,
}

impl ChildExitEvent {
    #[cfg(target_os = "linux")]
    pub fn open() -> io::Result<Self> {
        // SAFETY: epoll_create1 takes no pointers and returns a fresh fd.
        let queue = unsafe { libc::epoll_create1(libc::EPOLL_CLOEXEC) };
        if queue < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: epoll_create1 returned a fresh descriptor owned here.
        let queue = unsafe { OwnedFd::from_raw_fd(queue) };
        // SAFETY: eventfd takes scalar flags and returns a fresh descriptor.
        let wakeup = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC | libc::EFD_NONBLOCK) };
        if wakeup < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: eventfd returned a fresh descriptor owned here.
        let wakeup = unsafe { OwnedFd::from_raw_fd(wakeup) };
        let event = Self {
            queue,
            wakeup,
            children: std::sync::Mutex::new(std::collections::HashMap::new()),
        };
        event.add_linux(event.wakeup.as_raw_fd(), 0)?;
        Ok(event)
    }

    #[cfg(target_os = "linux")]
    fn add_linux(&self, fd: libc::c_int, token: usize) -> io::Result<()> {
        let mut change = libc::epoll_event {
            events: libc::EPOLLIN as u32,
            u64: token as u64,
        };
        loop {
            // SAFETY: both descriptors are owned and change is initialized.
            let result = unsafe {
                libc::epoll_ctl(self.queue.as_raw_fd(), libc::EPOLL_CTL_ADD, fd, &mut change)
            };
            if result == 0 {
                return Ok(());
            }
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error);
            }
        }
    }

    #[cfg(target_os = "macos")]
    pub fn open() -> io::Result<Self> {
        // SAFETY: kqueue takes no pointers and returns a fresh descriptor.
        let queue = unsafe { libc::kqueue() };
        if queue < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: kqueue returned a fresh descriptor owned here.
        let event = Self {
            queue: unsafe { OwnedFd::from_raw_fd(queue) },
        };
        // SAFETY: queue is live; F_SETFD takes an integer flag argument.
        if unsafe { libc::fcntl(event.queue.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) } < 0 {
            return Err(io::Error::last_os_error());
        }
        // XNU posts NOTE_EXIT before SZOMB and SIGCHLD after SZOMB. Register
        // SIGCHLD before any process knote, without changing its disposition.
        event.change_darwin(
            libc::SIGCHLD as _,
            libc::EVFILT_SIGNAL,
            libc::EV_ADD | libc::EV_CLEAR,
            0,
            0,
        )?;
        event.change_darwin(0, libc::EVFILT_USER, libc::EV_ADD | libc::EV_CLEAR, 0, 0)?;
        Ok(event)
    }

    #[cfg(target_os = "macos")]
    fn change_darwin(
        &self,
        ident: libc::uintptr_t,
        filter: i16,
        flags: u16,
        fflags: u32,
        token: usize,
    ) -> io::Result<()> {
        let change = libc::kevent {
            ident,
            filter,
            flags,
            fflags,
            data: 0,
            udata: token as *mut libc::c_void,
        };
        loop {
            // SAFETY: one initialized change; no output events requested.
            let result = unsafe {
                libc::kevent(
                    self.queue.as_raw_fd(),
                    &change,
                    1,
                    std::ptr::null_mut(),
                    0,
                    std::ptr::null(),
                )
            };
            if result >= 0 {
                return Ok(());
            }
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error);
            }
        }
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    pub fn open() -> io::Result<Self> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "native deleted-child exit source unavailable",
        ))
    }

    /// The caller owns an unreaped child and holds the ownership registry lock.
    #[cfg(target_os = "linux")]
    pub fn register(&self, pid: u32, token: usize) -> io::Result<ChildExitNotification> {
        // SAFETY: pid names the caller's exclusively owned, unreaped child.
        let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid as libc::pid_t, 0) };
        if fd < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: pidfd_open returned a fresh CLOEXEC descriptor owned here.
        let fd = unsafe { OwnedFd::from_raw_fd(fd as libc::c_int) };
        self.add_linux(fd.as_raw_fd(), token)?;
        self.children
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(token, fd);
        self.wake()?;
        Ok(ChildExitNotification::Wake)
    }

    #[cfg(target_os = "macos")]
    pub fn register(&self, pid: u32, token: usize) -> io::Result<ChildExitNotification> {
        let notification = match self.change_darwin(
            pid as _,
            libc::EVFILT_PROC,
            libc::EV_ADD | libc::EV_ONESHOT,
            libc::NOTE_EXIT,
            token,
        ) {
            Ok(()) => ChildExitNotification::Wake,
            // NOTE_EXIT has already passed. An initial WNOHANG probe,
            // followed by the already-registered SIGCHLD source, covers it.
            Err(error) if error.raw_os_error() == Some(libc::ESRCH) => {
                ChildExitNotification::Exited(token)
            }
            Err(error) => return Err(error),
        };
        self.wake()?;
        Ok(notification)
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    pub fn register(&self, _pid: u32, _token: usize) -> io::Result<ChildExitNotification> {
        unreachable!()
    }

    /// Remove the source before releasing a completed entry's PID/token.
    pub fn remove(&self, token: usize, pid: u32) {
        #[cfg(target_os = "linux")]
        {
            let _ = pid;
            // Closing the last pidfd removes its epoll interest as well.
            self.children
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .remove(&token);
        }
        #[cfg(target_os = "macos")]
        {
            // A delivered oneshot is already gone; ENOENT/ESRCH are harmless.
            let _ = self.change_darwin(pid as _, libc::EVFILT_PROC, libc::EV_DELETE, 0, token);
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        let _ = (token, pid);
    }

    pub fn wake(&self) -> io::Result<()> {
        #[cfg(target_os = "linux")]
        loop {
            let value = 1u64;
            // SAFETY: eventfd is live and value supplies its required 8 bytes.
            let result = unsafe {
                libc::write(
                    self.wakeup.as_raw_fd(),
                    (&value as *const u64).cast(),
                    std::mem::size_of::<u64>(),
                )
            };
            if result >= 0 {
                return Ok(());
            }
            let error = io::Error::last_os_error();
            match error.kind() {
                io::ErrorKind::Interrupted => continue,
                io::ErrorKind::WouldBlock => return Ok(()),
                _ => return Err(error),
            }
        }
        #[cfg(target_os = "macos")]
        {
            self.change_darwin(0, libc::EVFILT_USER, 0, libc::NOTE_TRIGGER, 0)
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            unreachable!()
        }
    }

    /// Block on the shared kernel set. No timeout or child-status polling.
    pub fn wait(&self) -> io::Result<Vec<ChildExitNotification>> {
        #[cfg(target_os = "linux")]
        loop {
            let mut events = [libc::epoll_event { events: 0, u64: 0 }; 64];
            // SAFETY: events is writable storage; queue stays owned throughout.
            let count = unsafe {
                libc::epoll_wait(
                    self.queue.as_raw_fd(),
                    events.as_mut_ptr(),
                    events.len() as _,
                    -1,
                )
            };
            if count >= 0 {
                let mut notifications = Vec::with_capacity(count as usize);
                for event in &events[..count as usize] {
                    if event.u64 == 0 {
                        let mut value = 0u64;
                        loop {
                            // SAFETY: eventfd is live and value is writable for 8 bytes.
                            let read = unsafe {
                                libc::read(
                                    self.wakeup.as_raw_fd(),
                                    (&mut value as *mut u64).cast(),
                                    std::mem::size_of::<u64>(),
                                )
                            };
                            if read >= 0 {
                                break;
                            }
                            let error = io::Error::last_os_error();
                            if error.kind() == io::ErrorKind::Interrupted {
                                continue;
                            }
                            if error.kind() != io::ErrorKind::WouldBlock {
                                return Err(error);
                            }
                            break;
                        }
                        notifications.push(ChildExitNotification::Wake);
                    } else {
                        notifications.push(ChildExitNotification::Exited(event.u64 as usize));
                    }
                }
                return Ok(notifications);
            }
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error);
            }
        }
        #[cfg(target_os = "macos")]
        loop {
            let mut events = std::mem::MaybeUninit::<[libc::kevent; 64]>::uninit();
            // SAFETY: events has room for 64 kevents and queue remains owned.
            let count = unsafe {
                libc::kevent(
                    self.queue.as_raw_fd(),
                    std::ptr::null(),
                    0,
                    events.as_mut_ptr().cast(),
                    64,
                    std::ptr::null(),
                )
            };
            if count >= 0 {
                // SAFETY: kevent initialized exactly count entries in this buffer.
                let events = unsafe {
                    std::slice::from_raw_parts(
                        events.as_ptr().cast::<libc::kevent>(),
                        count as usize,
                    )
                };
                let mut notifications = Vec::with_capacity(count as usize);
                for event in events {
                    if event.flags & libc::EV_ERROR != 0 {
                        return Err(io::Error::from_raw_os_error(event.data as _));
                    }
                    notifications.push(match event.filter {
                        libc::EVFILT_PROC => ChildExitNotification::Exited(event.udata as usize),
                        libc::EVFILT_SIGNAL => ChildExitNotification::ChildSignal,
                        libc::EVFILT_USER => ChildExitNotification::Wake,
                        _ => {
                            return Err(io::Error::other("unexpected deleted-child kqueue filter"));
                        }
                    });
                }
                return Ok(notifications);
            }
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error);
            }
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            unreachable!()
        }
    }
}
