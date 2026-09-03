//! Compile-target-selected monotonic time.
//!
//! Native targets use [`std::time::Instant`] directly. Browser WebAssembly has
//! no ambient clock API, so each Wasm instance installs its embedding
//! environment's `performance.now()` adapter at its composition root. The
//! evaluator never carries or branches on a runtime host identity.

std::cfg_select! {
    target_family = "wasm" => {
        mod wasm {
            use std::cell::Cell;
            use std::fmt::{Display, Formatter};
            use std::ops::{Add, AddAssign, Sub, SubAssign};
            use std::time::Duration;

            type MonotonicClock = fn() -> f64;

            thread_local! {
                static MONOTONIC_CLOCK: Cell<Option<MonotonicClock>> = const { Cell::new(None) };
            }

            /// A monotonic clock was already installed in this Wasm instance.
            #[derive(Clone, Copy, Debug, Eq, PartialEq)]
            pub struct MonotonicClockAlreadyInstalled;

            impl Display for MonotonicClockAlreadyInstalled {
                fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
                    formatter.write_str("the browser monotonic clock is already installed")
                }
            }

            impl std::error::Error for MonotonicClockAlreadyInstalled {}

            /// Install the embedding environment's monotonic millisecond clock.
            ///
            /// This is called once by each Wasm composition root before any
            /// evaluator or presentation object is constructed.
            pub fn install_monotonic_clock(
                clock: MonotonicClock,
            ) -> Result<(), MonotonicClockAlreadyInstalled> {
                MONOTONIC_CLOCK.with(|slot| {
                    if slot.get().is_some() {
                        Err(MonotonicClockAlreadyInstalled)
                    } else {
                        slot.set(Some(clock));
                        Ok(())
                    }
                })
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

        pub use wasm::{Instant, MonotonicClockAlreadyInstalled, install_monotonic_clock};
    }
    _ => {
        pub use std::time::Instant;
    }
}
