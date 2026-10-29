//! Shared elapsed-time contention bound; production entry adds no caller in E1.
use std::ops::ControlFlow;
use std::time::{Duration, Instant};

pub(crate) const SPIN_WAIT: Duration = Duration::from_secs(60);

/// An absolute contention deadline, never an attempt allowance. Construct it
/// once at the fixture boundary and retain it through capability recovery.
pub(crate) struct Deadline(Instant);

impl Deadline {
    pub(crate) fn contention() -> Self {
        Self(Instant::now() + SPIN_WAIT)
    }

    /// Check the same absolute deadline across caller-visible retry steps.
    /// Expiry returns the caller's typed Busy refusal without spending a
    /// program step or discarding an ownership-returning failure.
    pub(crate) fn check<E>(&self, expired: E) -> Result<(), E> {
        if Instant::now() >= self.0 {
            Err(expired)
        } else {
            Ok(())
        }
    }

    /// Deterministic deadline boundary for fixtures; never sleep for SPIN_WAIT.
    #[cfg(test)]
    pub(crate) fn expired_for_test() -> Self {
        Self(Instant::now())
    }
}

/// Classify the typed error at the operation boundary.
pub(crate) enum Retry {
    Busy,
    Refuse,
}

pub(crate) fn retry<T, E>(
    deadline: Deadline,
    mut operation: impl FnMut() -> Result<T, E>,
    mut classify: impl FnMut(&E) -> Retry,
) -> Result<T, E> {
    retry_owned(
        deadline,
        (),
        |()| operation(),
        |error| match classify(&error) {
            Retry::Busy => ControlFlow::Continue(()),
            Retry::Refuse => ControlFlow::Break(error),
        },
    )
}

/// A failed consuming operation returns its capability in `E`. Recover it
/// only for a retry; refusal or expiry returns that intact failure instead.
pub(crate) fn retry_owned<C, T, E>(
    deadline: Deadline,
    mut capability: C,
    mut operation: impl FnMut(C) -> Result<T, E>,
    mut recover: impl FnMut(E) -> ControlFlow<E, C>,
) -> Result<T, E> {
    loop {
        let failure = match operation(capability) {
            Ok(value) => return Ok(value),
            Err(failure) => failure,
        };
        if deadline.check(()).is_err() {
            return Err(failure);
        }
        match recover(failure) {
            ControlFlow::Break(failure) => return Err(failure),
            ControlFlow::Continue(recovered) => capability = recovered,
        }
        std::thread::yield_now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Eq)]
    enum Error {
        Busy,
        Refused,
    }

    fn classify(error: &Error) -> Retry {
        match error {
            Error::Busy => Retry::Busy,
            Error::Refused => Retry::Refuse,
        }
    }

    #[test]
    fn fixture_retry_outlasts_yield_budget() {
        let ready = Instant::now() + Duration::from_millis(20);
        let mut attempts = 0;
        let result = retry(
            Deadline::contention(),
            || {
                attempts += 1;
                if attempts <= 10_001 || Instant::now() < ready {
                    Err(Error::Busy)
                } else {
                    Ok(())
                }
            },
            classify,
        );
        assert_eq!(result, Ok(()));
        assert!(attempts > 10_001);
        assert!(Instant::now() >= ready);
    }

    #[test]
    fn fixture_retry_refuses_without_another_attempt() {
        let mut attempts = 0;
        let result: Result<(), _> = retry(
            Deadline::contention(),
            || {
                attempts += 1;
                Err(Error::Refused)
            },
            classify,
        );
        assert_eq!(result, Err(Error::Refused));
        assert_eq!(attempts, 1);
    }

    #[derive(Debug)]
    struct Failure {
        capability: Box<u32>,
        reason: Error,
    }

    #[test]
    fn fixture_retry_owned_recovers_busy_capability() {
        let mut attempts = 0;
        let result = retry_owned(
            Deadline::contention(),
            Box::new(11),
            |capability| {
                attempts += 1;
                match attempts {
                    1 => Err(Failure {
                        capability,
                        reason: Error::Busy,
                    }),
                    _ => Ok(capability),
                }
            },
            |failure| match failure.reason {
                Error::Busy => ControlFlow::Continue(failure.capability),
                Error::Refused => ControlFlow::Break(failure),
            },
        );
        assert_eq!(*result.unwrap(), 11);
        assert_eq!(attempts, 2);
    }

    #[test]
    fn fixture_retry_owned_preserves_refused_capability() {
        let result: Result<(), Failure> = retry_owned(
            Deadline::contention(),
            Box::new(17),
            |capability| {
                Err(Failure {
                    capability,
                    reason: Error::Refused,
                })
            },
            |failure| match failure.reason {
                Error::Busy => ControlFlow::Continue(failure.capability),
                Error::Refused => ControlFlow::Break(failure),
            },
        );
        let failure = result.unwrap_err();
        assert_eq!(*failure.capability, 17);
        assert_eq!(failure.reason, Error::Refused);
    }

    #[test]
    fn fixture_retry_owned_expiry_preserves_busy_capability() {
        let result: Result<(), Failure> = retry_owned(
            Deadline::expired_for_test(),
            Box::new(23),
            |capability| {
                Err(Failure {
                    capability,
                    reason: Error::Busy,
                })
            },
            |_| panic!("expiry must return the failure before recovering its capability"),
        );
        let failure = result.unwrap_err();
        assert_eq!(*failure.capability, 23);
        assert_eq!(failure.reason, Error::Busy);
    }
}
