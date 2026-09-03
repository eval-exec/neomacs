//! Compile-target-selected clocks.
//!
//! Native targets use [`std::time::Instant`] directly. Browser WebAssembly has
//! no ambient clock API, so each Wasm instance installs its embedding
//! environment's monotonic and wall-clock adapters at its composition root.
//! The evaluator never carries or branches on a runtime host identity.

use std::fmt::{Display, Formatter};
use std::time::Duration;

/// The native wall clock reported a time before the Unix epoch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WallClockBeforeUnixEpoch(Duration);

impl WallClockBeforeUnixEpoch {
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

            type MonotonicClock = fn() -> f64;
            type WallClock = fn() -> f64;

            thread_local! {
                static MONOTONIC_CLOCK: Cell<Option<MonotonicClock>> = const { Cell::new(None) };
                static WALL_CLOCK: Cell<Option<WallClock>> = const { Cell::new(None) };
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

            /// Install the embedding environment's browser clocks.
            ///
            /// This is called once by each Wasm composition root before any
            /// evaluator or presentation object is constructed.
            pub fn install_browser_clocks(
                monotonic_clock: MonotonicClock,
                wall_clock: WallClock,
            ) -> Result<(), BrowserClocksAlreadyInstalled> {
                if MONOTONIC_CLOCK.with(Cell::get).is_some()
                    || WALL_CLOCK.with(Cell::get).is_some()
                {
                    return Err(BrowserClocksAlreadyInstalled);
                }
                MONOTONIC_CLOCK.with(|slot| slot.set(Some(monotonic_clock)));
                WALL_CLOCK.with(|slot| slot.set(Some(wall_clock)));
                Ok(())
            }

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
                #[must_use]
                pub fn now() -> Self {
                    Self(monotonic_now())
                }

                #[must_use]
                pub fn duration_since(&self, earlier: Self) -> Duration {
                    self.0 - earlier.0
                }

                #[must_use]
                pub fn checked_duration_since(&self, earlier: Self) -> Option<Duration> {
                    self.0.checked_sub(earlier.0)
                }

                #[must_use]
                pub fn saturating_duration_since(&self, earlier: Self) -> Duration {
                    self.0.saturating_sub(earlier.0)
                }

                #[must_use]
                pub fn elapsed(&self) -> Duration {
                    Self::now().duration_since(*self)
                }

                #[must_use]
                pub fn checked_add(&self, duration: Duration) -> Option<Self> {
                    self.0.checked_add(duration).map(Self)
                }

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
            BrowserClocksAlreadyInstalled, Instant, install_browser_clocks,
            wall_time_since_unix_epoch,
        };
    }
    _ => {
        pub use std::time::Instant;

        pub fn wall_time_since_unix_epoch() -> Result<Duration, WallClockBeforeUnixEpoch> {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|error| WallClockBeforeUnixEpoch(error.duration()))
        }
    }
}
