//! Owned terminal-size queries shared by the reader and redisplay.
//!
//! The owner retains a terminal resource, never its dimensions. Each query
//! samples again; independent reader/redisplay owners have no shared state.

#[cfg(unix)]
use std::fs::File;
use std::io;
#[cfg(unix)]
use std::os::fd::{AsFd, AsRawFd, BorrowedFd};

/// A terminal width measured in character cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct TerminalColumns(u32);

/// A terminal height measured in character cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub struct TerminalRows(u32);

/// Terminal dimensions, including successful native zero-size observations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerminalCellSize {
    columns: TerminalColumns,
    rows: TerminalRows,
}

impl TerminalCellSize {
    pub const fn new(columns: TerminalColumns, rows: TerminalRows) -> Self {
        Self { columns, rows }
    }

    pub fn columns(self) -> TerminalColumns {
        self.columns
    }

    pub fn rows(self) -> TerminalRows {
        self.rows
    }
}

impl From<u32> for TerminalColumns {
    fn from(columns: u32) -> Self {
        Self(columns)
    }
}

impl From<u32> for TerminalRows {
    fn from(rows: u32) -> Self {
        Self(rows)
    }
}

impl From<TerminalColumns> for u32 {
    fn from(columns: TerminalColumns) -> Self {
        columns.0
    }
}

impl From<TerminalRows> for u32 {
    fn from(rows: TerminalRows) -> Self {
        rows.0
    }
}

#[cfg(unix)]
impl From<libc::winsize> for TerminalCellSize {
    fn from(size: libc::winsize) -> Self {
        Self::new(
            TerminalColumns(u32::from(size.ws_col)),
            TerminalRows(u32::from(size.ws_row)),
        )
    }
}

/// A terminal resource used for fresh size observations.
///
/// Unix opens /dev/tty once, independently of stdin. GNU retains its named
/// terminal fd and queries that owner (`term.c:4800`, `dispnew.c:6631`). An
/// initial open failure keeps the complete crossterm strategy available; on
/// other platforms crossterm owns the platform query policy.
#[derive(Debug)]
pub enum TerminalSizeQuery {
    #[cfg(unix)]
    ControllingTerminal(File),
    LegacyFallback,
}

#[cfg(unix)]
impl From<io::Result<File>> for TerminalSizeQuery {
    fn from(terminal: io::Result<File>) -> Self {
        match terminal {
            Ok(file) => Self::ControllingTerminal(file),
            Err(_) => Self::LegacyFallback,
        }
    }
}

impl TerminalSizeQuery {
    pub fn new() -> Self {
        #[cfg(unix)]
        {
            Self::from(File::open("/dev/tty"))
        }
        #[cfg(not(unix))]
        {
            Self::LegacyFallback
        }
    }

    pub fn query(&self) -> io::Result<TerminalCellSize> {
        self.query_with_fallback(legacy_crossterm_terminal_size)
    }

    fn query_with_fallback(
        &self,
        fallback: impl FnOnce() -> io::Result<TerminalCellSize>,
    ) -> io::Result<TerminalCellSize> {
        match self {
            #[cfg(unix)]
            Self::ControllingTerminal(file) => match terminal_cell_size(file.as_fd()) {
                Ok(size) => Ok(size),
                // Preserve crossterm's complete controlling-tty/stdout/tput
                // policy on error, keeping the owned fd for the next query.
                Err(_) => fallback(),
            },
            Self::LegacyFallback => fallback(),
        }
    }
}

// The external callback exposes a tuple. Name and decode that legacy boundary
// here; callers inside the workspace only exchange dimensions with their units.
fn legacy_crossterm_terminal_size() -> io::Result<TerminalCellSize> {
    let (columns, rows) = crossterm::terminal::size()?;
    Ok(TerminalCellSize::new(
        TerminalColumns(u32::from(columns)),
        TerminalRows(u32::from(rows)),
    ))
}

impl Default for TerminalSizeQuery {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(unix)]
fn terminal_cell_size(source: BorrowedFd<'_>) -> io::Result<TerminalCellSize> {
    let mut size = libc::winsize {
        ws_row: 0,
        ws_col: 0,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // SAFETY: source borrows a live fd for this call. TIOCGWINSZ writes only
    // the initialized, correctly sized winsize storage and retains no pointer.
    if unsafe { libc::ioctl(source.as_raw_fd(), libc::TIOCGWINSZ, &mut size) } < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(size.into())
    }
}

static_assertions::assert_eq_size!(TerminalColumns, u32);
static_assertions::assert_eq_size!(TerminalRows, u32);
static_assertions::assert_eq_size!(TerminalCellSize, (u32, u32));
static_assertions::assert_impl_all!(TerminalSizeQuery: Send, std::fmt::Debug);
static_assertions::assert_not_impl_any!(TerminalCellSize: From<(u32, u32)>, From<(u16, u16)>);

#[cfg(all(test, unix))]
#[path = "tests/terminal_size_test.rs"]
mod tests;
