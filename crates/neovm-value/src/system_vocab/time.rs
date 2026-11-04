use std::time::Duration;

/// A GNU wait request. Finite waits have a normalized, representable timespec;
/// scalar durations contain no mutator-owned state and can cross threads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaitTimeout {
    Poll,
    For(Duration),
    Forever,
}

impl From<f64> for WaitTimeout {
    fn from(seconds: f64) -> Self {
        if !(seconds > 0.0) {
            return Self::Poll;
        }
        // GNU lib/dtotimespec.c saturates at TIME_T_MAX with the final ns.
        if seconds >= i64::MAX as f64 {
            return Self::For(Duration::new(i64::MAX as u64, 999_999_999));
        }
        let whole = seconds.trunc();
        let nanos = ((seconds - whole) * 1_000_000_000.0).ceil() as u32;
        Self::For(Duration::new(whole as u64, nanos))
    }
}

impl From<Option<f64>> for WaitTimeout {
    fn from(seconds: Option<f64>) -> Self {
        seconds.map_or(Self::Forever, Self::from)
    }
}

static_assertions::assert_impl_all!(WaitTimeout: Send, Sync);
