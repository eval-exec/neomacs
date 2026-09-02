//! Target-specific host inventory.
//!
//! Callers use this module's interface without knowing whether the runtime can
//! inspect an operating system. Target selection happens once at this seam:
//! native hosts query the OS, while WebAssembly reports unavailable facts as
//! `None`. Lisp-facing callers remain responsible for applying GNU-compatible
//! fallback values where GNU's interface requires one.

/// A host boot timestamp with its unit represented in the type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BootTime {
    unix_seconds: i64,
}

impl BootTime {
    fn from_unix_seconds(unix_seconds: i64) -> Option<Self> {
        (unix_seconds > 0).then_some(Self { unix_seconds })
    }

    pub(crate) fn unix_seconds(self) -> i64 {
        self.unix_seconds
    }
}

/// The one-, five-, and fifteen-minute load averages.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LoadAverage {
    one_minute: f64,
    five_minutes: f64,
    fifteen_minutes: f64,
}

impl LoadAverage {
    pub(crate) const ZERO: Self = Self::new(0.0, 0.0, 0.0);

    const fn new(one_minute: f64, five_minutes: f64, fifteen_minutes: f64) -> Self {
        Self {
            one_minute,
            five_minutes,
            fifteen_minutes,
        }
    }

    pub(crate) const fn one_minute(self) -> f64 {
        self.one_minute
    }

    pub(crate) const fn five_minutes(self) -> f64 {
        self.five_minutes
    }

    pub(crate) const fn fifteen_minutes(self) -> f64 {
        self.fifteen_minutes
    }
}

std::cfg_select! {
    target_family = "wasm" => {
        mod wasm;
        use self::wasm as backend;
    }
    _ => {
        mod native;
        use self::native as backend;
    }
}

pub(crate) use backend::{
    boot_time, configured_processor_count, load_average, operating_system_release, system_name,
};

#[cfg(test)]
mod tests;
