//! Query owned PTYs without replacing stdin, acquiring a controlling terminal,
//! installing signal handlers, or sending process-global signals.

use super::*;
use std::cell::Cell;
use std::os::fd::{FromRawFd, OwnedFd};

#[derive(Debug)]
struct SizePty {
    _master: OwnedFd,
    slave: File,
}

impl SizePty {
    fn new(size: TerminalCellSize) -> io::Result<Self> {
        let mut master = -1;
        let mut slave = -1;
        let mut dimensions = libc::winsize {
            ws_row: u16::try_from(u32::from(size.rows())).map_err(io::Error::other)?,
            ws_col: u16::try_from(u32::from(size.columns())).map_err(io::Error::other)?,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        // SAFETY: writable fd output slots and initialized winsize; the null
        // name/termios arguments request no caller buffers or terminal modes.
        if unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut dimensions,
            )
        } < 0
        {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: successful openpty returned two distinct owned descriptors.
        let master = unsafe { OwnedFd::from_raw_fd(master) };
        // SAFETY: this is the other descriptor transferred by openpty.
        let slave = unsafe { OwnedFd::from_raw_fd(slave) };
        set_close_on_exec(master.as_fd())?;
        set_close_on_exec(slave.as_fd())?;
        Ok(Self {
            _master: master,
            slave: File::from(slave),
        })
    }

    fn resize(&self, size: TerminalCellSize) -> io::Result<()> {
        let dimensions = libc::winsize {
            ws_row: u16::try_from(u32::from(size.rows())).map_err(io::Error::other)?,
            ws_col: u16::try_from(u32::from(size.columns())).map_err(io::Error::other)?,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        // SAFETY: this fixture owns the live PTY and initialized winsize.
        // No session/foreground process group is assigned to this terminal.
        if unsafe { libc::ioctl(self.slave.as_raw_fd(), libc::TIOCSWINSZ, &dimensions) } < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    fn query_owner(&self) -> io::Result<TerminalSizeQuery> {
        Ok(TerminalSizeQuery::from(Ok(self.slave.try_clone()?)))
    }
}

fn set_close_on_exec(source: BorrowedFd<'_>) -> io::Result<()> {
    // SAFETY: source borrows the fixture's live descriptor.
    let flags = unsafe { libc::fcntl(source.as_raw_fd(), libc::F_GETFD) };
    if flags < 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: preserve every existing descriptor flag on that same live fd.
    if unsafe { libc::fcntl(source.as_raw_fd(), libc::F_SETFD, flags | libc::FD_CLOEXEC) } < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[test]
fn size_query_uses_its_terminal_owner_instead_of_the_input_terminal() -> io::Result<()> {
    let input_size = TerminalCellSize {
        columns: TerminalColumns(81),
        rows: TerminalRows(25),
    };
    let terminal_size = TerminalCellSize {
        columns: TerminalColumns(133),
        rows: TerminalRows(47),
    };
    let input = SizePty::new(input_size)?;
    let terminal = SizePty::new(terminal_size)?;
    let query = terminal.query_owner()?;
    let observed = query.query()?;
    assert_eq!(observed, terminal_size);
    assert_ne!(observed, terminal_cell_size(input.slave.as_fd())?);
    Ok(())
}

#[test]
fn size_query_refreshes_dimensions_on_the_same_owned_descriptor() -> io::Result<()> {
    let first = TerminalCellSize {
        columns: TerminalColumns(93),
        rows: TerminalRows(29),
    };
    let second = TerminalCellSize {
        columns: TerminalColumns(117),
        rows: TerminalRows(41),
    };
    let terminal = SizePty::new(first)?;
    let query = terminal.query_owner()?;
    assert_eq!(query.query()?, first);
    terminal.resize(second)?;
    assert_eq!(query.query()?, second);
    Ok(())
}

#[test]
fn size_query_ioctl_failure_calls_fallback_once_and_keeps_the_fd_live() -> io::Result<()> {
    let query = TerminalSizeQuery::from(Ok(tempfile::tempfile()?));
    let expected = TerminalCellSize {
        columns: TerminalColumns(101),
        rows: TerminalRows(33),
    };
    let fallbacks = Cell::new(0);
    assert_eq!(
        query.query_with_fallback(|| {
            fallbacks.set(fallbacks.get() + 1);
            Ok(expected)
        })?,
        expected
    );
    assert_eq!(fallbacks.get(), 1);
    match &query {
        TerminalSizeQuery::ControllingTerminal(file) => {
            file.metadata()?;
        }
        TerminalSizeQuery::LegacyFallback => {
            return Err(io::Error::other(
                "failed ioctl replaced the owned query source",
            ));
        }
    }
    Ok(())
}

#[test]
fn size_query_fallback_error_does_not_poison_later_queries() -> io::Result<()> {
    let query = TerminalSizeQuery::from(Ok(tempfile::tempfile()?));
    let first = query.query_with_fallback(|| Err(io::ErrorKind::Interrupted.into()));
    assert_eq!(
        first.err().map(|error| error.kind()),
        Some(io::ErrorKind::Interrupted)
    );
    let recovered = TerminalCellSize {
        columns: TerminalColumns(119),
        rows: TerminalRows(39),
    };
    assert_eq!(query.query_with_fallback(|| Ok(recovered))?, recovered);
    Ok(())
}

#[test]
fn size_query_initial_open_failure_keeps_the_full_legacy_query_retriable() -> io::Result<()> {
    let query = TerminalSizeQuery::from(Err(io::ErrorKind::NotFound.into()));
    let first = query.query_with_fallback(|| Err(io::ErrorKind::NotConnected.into()));
    assert_eq!(
        first.err().map(|error| error.kind()),
        Some(io::ErrorKind::NotConnected)
    );
    let recovered = TerminalCellSize {
        columns: TerminalColumns(83),
        rows: TerminalRows(27),
    };
    assert_eq!(query.query_with_fallback(|| Ok(recovered))?, recovered);
    Ok(())
}

#[test]
fn size_query_zero_dimensions_are_successful_not_a_fallback_trigger() -> io::Result<()> {
    let zero = TerminalCellSize {
        columns: TerminalColumns(0),
        rows: TerminalRows(0),
    };
    let terminal = SizePty::new(zero)?;
    let query = terminal.query_owner()?;
    let observed = query.query()?;
    assert_eq!(observed, zero);
    Ok(())
}
