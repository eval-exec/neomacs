//! Terminal capabilities without native handles or borrowed native buffers.
//!
//! Load a finite set of queries into an owned snapshot. All ncurses operations
//! are serialized, including copying their results. This crate must be the only
//! owner of ncurses access in the process; independent FFI callers cannot share
//! its lock. No terminal I/O, renderer policy, or environment mutation occurs.
#![deny(unsafe_code)]

use std::collections::BTreeMap;
use std::fmt;

mod numeric;
pub use numeric::expand_numeric;

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[allow(unsafe_code)]
mod native;

/// String names occupy distinct namespaces: `us` is termcap, `smul` terminfo.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StringCapability<'a> {
    Termcap(&'a str),
    Terminfo(&'a str),
}

/// Boolean names also have distinct namespaces (`RGB` is terminfo).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FlagCapability<'a> {
    Termcap(&'a str),
    Terminfo(&'a str),
}

/// The values a caller wants to retain from a terminal entry.
#[derive(Clone, Copy, Debug)]
pub enum Query<'a> {
    String(StringCapability<'a>),
    TermcapNumber(&'a str),
    Flag(FlagCapability<'a>),
}

/// Loading/expansion failures are distinct from missing individual capabilities.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    UnsupportedPlatform,
    InvalidName,
    TerminalNotFound,
    DatabaseUnavailable,
    InvalidNumericFormat,
    NativeStatePoisoned,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::UnsupportedPlatform => "terminfo is unavailable on this platform",
            Self::InvalidName => "terminal or capability name is empty or contains NUL",
            Self::TerminalNotFound => "terminal entry was not found",
            Self::DatabaseUnavailable => "terminfo database could not be opened",
            Self::InvalidNumericFormat => "unsupported or malformed numeric terminfo format",
            Self::NativeStatePoisoned => "native terminfo state was interrupted by a panic",
        })
    }
}

impl std::error::Error for Error {}

/// A Rust-owned snapshot. Reading it never touches ncurses or another entry.
/// Queries not requested at load time return the same absence as missing values.
#[derive(Clone, Debug, Default)]
pub struct Database {
    termcap_strings: BTreeMap<String, Vec<u8>>,
    terminfo_strings: BTreeMap<String, Vec<u8>>,
    numbers: BTreeMap<String, i32>,
    termcap_flags: BTreeMap<String, bool>,
    terminfo_flags: BTreeMap<String, bool>,
}

impl Database {
    pub fn load(term: &str, queries: &[Query<'_>]) -> Result<Self, Error> {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            native::load(term, queries)
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            let _ = (term, queries);
            Err(Error::UnsupportedPlatform)
        }
    }

    pub fn string(&self, capability: StringCapability<'_>) -> Option<&[u8]> {
        let (map, name) = match capability {
            StringCapability::Termcap(name) => (&self.termcap_strings, name),
            StringCapability::Terminfo(name) => (&self.terminfo_strings, name),
        };
        map.get(name).map(Vec::as_slice)
    }

    pub fn termcap_number(&self, name: &str) -> Option<i32> {
        self.numbers.get(name).copied()
    }

    pub fn flag(&self, capability: FlagCapability<'_>) -> bool {
        let (map, name) = match capability {
            FlagCapability::Termcap(name) => (&self.termcap_flags, name),
            FlagCapability::Terminfo(name) => (&self.terminfo_flags, name),
        };
        map.get(name).copied().unwrap_or(false)
    }
}
