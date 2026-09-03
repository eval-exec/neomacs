//! Compile-target-selected clocks.
//!
//! Native targets use [`std::time::Instant`] directly. Browser WebAssembly has
//! no ambient clock API, so each Wasm instance installs its embedding
//! environment's monotonic and wall-clock adapters at its composition root.
//! Callers never carry or branch on a runtime host identity.

use std::fmt::{Display, Formatter};
use std::time::Duration;

/// The native wall clock reported a time before the Unix epoch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WallClockBeforeUnixEpoch(Duration);

impl WallClockBeforeUnixEpoch {
    /// Return the magnitude by which the clock preceded the Unix epoch.
    #[must_use]
    pub const fn duration(self) -> Duration {
        self.0
    }
}

impl Display for WallClockBeforeUnixEpoch {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("the wall clock is earlier than the Unix epoch")
    }
}

impl std::error::Error for WallClockBeforeUnixEpoch {}

std::cfg_select! {
    target_family = "wasm" => {
        mod wasm {
            use super::WallClockBeforeUnixEpoch;
            use std::cell::Cell;
            use std::ops::{Add, AddAssign, Sub, SubAssign};
            use std::time::Duration;

            /// Function supplied by the browser for a monotonic millisecond clock.
            pub type MonotonicClock = fn() -> f64;
            /// Function supplied by the browser for Unix time in milliseconds.
            pub type WallClock = fn() -> f64;

            thread_local! {
                static MONOTONIC_CLOCK: Cell<Option<MonotonicClock>> = const { Cell::new(None) };
                static WALL_CLOCK: Cell<Option<WallClock>> = const { Cell::new(None) };
            }

            /// Complete clock capability supplied by a browser composition root.
            #[derive(Clone, Copy)]
            pub struct BrowserClocks {
                monotonic: MonotonicClock,
                wall: WallClock,
            }

            impl BrowserClocks {
                /// Bundle the browser's monotonic and wall-clock functions.
                #[must_use]
                pub const fn new(monotonic: MonotonicClock, wall: WallClock) -> Self {
                    Self { monotonic, wall }
                }

                /// Install both clocks atomically for the current Wasm instance.
                pub fn install(self) -> Result<(), BrowserClocksAlreadyInstalled> {
                    if MONOTONIC_CLOCK.with(Cell::get).is_some()
                        || WALL_CLOCK.with(Cell::get).is_some()
                    {
                        return Err(BrowserClocksAlreadyInstalled);
                    }
                    MONOTONIC_CLOCK.with(|slot| slot.set(Some(self.monotonic)));
                    WALL_CLOCK.with(|slot| slot.set(Some(self.wall)));
                    Ok(())
                }
            }

            /// Browser clocks were already installed in this Wasm instance.
            #[derive(Clone, Copy, Debug, Eq, PartialEq)]
            pub struct BrowserClocksAlreadyInstalled;

            impl std::fmt::Display for BrowserClocksAlreadyInstalled {
                fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    formatter.write_str("the browser clocks are already installed")
                }
            }

            impl std::error::Error for BrowserClocksAlreadyInstalled {}

            fn monotonic_now() -> Duration {
                let milliseconds = MONOTONIC_CLOCK.with(|slot| {
                    slot.get()
                        .expect("browser monotonic clock must be installed before editor startup")()
                });
                assert!(
                    milliseconds.is_finite() && milliseconds >= 0.0,
                    "browser monotonic clock returned invalid milliseconds: {milliseconds}"
                );
                Duration::from_secs_f64(milliseconds / 1_000.0)
            }

            /// Return the browser wall-clock duration since the Unix epoch.
            pub fn wall_time_since_unix_epoch() -> Result<Duration, WallClockBeforeUnixEpoch> {
                let milliseconds = WALL_CLOCK.with(|slot| {
                    slot.get()
                        .expect("browser wall clock must be installed before editor startup")()
                });
                assert!(
                    milliseconds.is_finite() && milliseconds >= 0.0,
                    "browser wall clock returned invalid Unix milliseconds: {milliseconds}"
                );
                Ok(Duration::from_secs_f64(milliseconds / 1_000.0))
            }

            /// Monotonic time point backed by the browser host's clock.
            #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
            pub struct Instant(Duration);

            impl Instant {
                /// Return the current monotonic time point.
                #[must_use]
                pub fn now() -> Self {
                    Self(monotonic_now())
                }

                /// Return the elapsed duration since `earlier`, panicking on reversal.
                #[must_use]
                pub fn duration_since(&self, earlier: Self) -> Duration {
                    self.0 - earlier.0
                }

                /// Return the elapsed duration, or `None` if `earlier` is later.
                #[must_use]
                pub fn checked_duration_since(&self, earlier: Self) -> Option<Duration> {
                    self.0.checked_sub(earlier.0)
                }

                /// Return the elapsed duration, saturating at zero on reversal.
                #[must_use]
                pub fn saturating_duration_since(&self, earlier: Self) -> Duration {
                    self.0.saturating_sub(earlier.0)
                }

                /// Return the duration elapsed since this time point.
                #[must_use]
                pub fn elapsed(&self) -> Duration {
                    Self::now().duration_since(*self)
                }

                /// Add a duration, returning `None` on overflow.
                #[must_use]
                pub fn checked_add(&self, duration: Duration) -> Option<Self> {
                    self.0.checked_add(duration).map(Self)
                }

                /// Subtract a duration, returning `None` on underflow.
                #[must_use]
                pub fn checked_sub(&self, duration: Duration) -> Option<Self> {
                    self.0.checked_sub(duration).map(Self)
                }
            }

            impl Add<Duration> for Instant {
                type Output = Self;

                fn add(self, duration: Duration) -> Self::Output {
                    Self(self.0 + duration)
                }
            }

            impl AddAssign<Duration> for Instant {
                fn add_assign(&mut self, duration: Duration) {
                    self.0 += duration;
                }
            }

            impl Sub<Duration> for Instant {
                type Output = Self;

                fn sub(self, duration: Duration) -> Self::Output {
                    Self(self.0 - duration)
                }
            }

            impl SubAssign<Duration> for Instant {
                fn sub_assign(&mut self, duration: Duration) {
                    self.0 -= duration;
                }
            }

            impl Sub for Instant {
                type Output = Duration;

                fn sub(self, earlier: Self) -> Self::Output {
                    self.duration_since(earlier)
                }
            }
        }

        pub use wasm::{
            BrowserClocks, BrowserClocksAlreadyInstalled, Instant,
            wall_time_since_unix_epoch,
        };
    }
    _ => {
        pub use std::time::Instant;

        /// Return the native wall-clock duration since the Unix epoch.
        pub fn wall_time_since_unix_epoch() -> Result<Duration, WallClockBeforeUnixEpoch> {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|error| WallClockBeforeUnixEpoch(error.duration()))
        }
    }
}
