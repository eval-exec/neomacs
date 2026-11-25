//! Injected time and the one agreed liveness timeout (DESIGN §6.15, §6.19).
//!
//! Time is a parameter (`docs/design/display-crate-layout.md` rule 1): the
//! state machines never read a clock, they are told `now`. A [`Clock`] is the
//! only source of `now`; [`MonotonicClock`] is the production one and the
//! `testing` feature provides a fake.

use std::time::Duration;

/// A point on one process's monotonic session clock, measured from an
/// arbitrary origin. Times from two processes are never compared.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SessionTime(Duration);

impl SessionTime {
    pub const ZERO: Self = Self(Duration::ZERO);

    #[must_use]
    pub const fn from_duration(since_origin: Duration) -> Self {
        Self(since_origin)
    }

    #[must_use]
    pub const fn from_millis(millis: u64) -> Self {
        Self(Duration::from_millis(millis))
    }

    #[must_use]
    pub const fn since_origin(self) -> Duration {
        self.0
    }

    /// Time from `earlier` to `self`, zero when `earlier` is later.
    #[must_use]
    pub fn saturating_since(self, earlier: Self) -> Duration {
        self.0.saturating_sub(earlier.0)
    }

    #[must_use]
    pub fn saturating_add(self, duration: Duration) -> Self {
        Self(self.0.saturating_add(duration))
    }
}

/// The source of `now` for a pump.
pub trait Clock {
    fn now(&self) -> SessionTime;
}

/// The process's monotonic clock. This is the one place the session crate
/// reads real time.
#[derive(Clone, Copy, Debug)]
pub struct MonotonicClock {
    origin: std::time::Instant,
}

impl MonotonicClock {
    #[must_use]
    pub fn start() -> Self {
        Self {
            origin: std::time::Instant::now(),
        }
    }
}

impl Clock for MonotonicClock {
    fn now(&self) -> SessionTime {
        SessionTime(self.origin.elapsed())
    }
}

/// Defaults from DESIGN §6.18-§6.19.
pub const DEFAULT_KEEPALIVE_TIMEOUT: Duration = Duration::from_secs(10);
/// Each side pings after this much send silence.
pub const PING_AFTER_SEND_SILENCE: Duration = Duration::from_secs(1);
/// A handshake or resume that has not completed by then is abandoned.
pub const HANDSHAKE_DEADLINE: Duration = Duration::from_secs(10);

/// What a [`Keepalive`] asks its link to do at `tick`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeepaliveAction {
    Nothing,
    SendPing { nonce: u64 },
    Expired,
}

/// Liveness of one link: one timeout for both sides, announced in Welcome.
///
/// Any received record proves the peer alive; each side pings after one
/// second of its own send silence, and I/O threads answer pings, so a busy
/// evaluator never looks dead.
#[derive(Clone, Debug)]
pub struct Keepalive {
    timeout: Duration,
    last_received: SessionTime,
    last_sent: SessionTime,
    outstanding_ping: Option<(u64, SessionTime)>,
    next_nonce: u64,
    rtt: Option<Duration>,
}

impl Keepalive {
    #[must_use]
    pub fn new(timeout: Duration, now: SessionTime) -> Self {
        Self {
            timeout,
            last_received: now,
            last_sent: now,
            outstanding_ping: None,
            next_nonce: 1,
            rtt: None,
        }
    }

    #[must_use]
    pub const fn timeout(&self) -> Duration {
        self.timeout
    }

    pub fn set_timeout(&mut self, timeout: Duration) {
        self.timeout = timeout;
    }

    pub fn record_received(&mut self, now: SessionTime) {
        self.last_received = now;
    }

    pub fn record_sent(&mut self, now: SessionTime) {
        self.last_sent = now;
    }

    /// A pong answering `nonce` arrived: the round trip is measured on this
    /// side's own clock only.
    pub fn record_pong(&mut self, nonce: u64, now: SessionTime) {
        if let Some((outstanding, sent_at)) = self.outstanding_ping
            && outstanding == nonce
        {
            self.rtt = Some(now.saturating_since(sent_at));
            self.outstanding_ping = None;
        }
    }

    /// The latest measured round trip, if any.
    #[must_use]
    pub const fn rtt(&self) -> Option<Duration> {
        self.rtt
    }

    #[must_use]
    pub fn poll(&mut self, now: SessionTime) -> KeepaliveAction {
        if now.saturating_since(self.last_received) >= self.timeout {
            return KeepaliveAction::Expired;
        }
        if self.outstanding_ping.is_none()
            && now.saturating_since(self.last_sent) >= PING_AFTER_SEND_SILENCE
        {
            let nonce = self.next_nonce;
            self.next_nonce = self.next_nonce.wrapping_add(1);
            self.outstanding_ping = Some((nonce, now));
            return KeepaliveAction::SendPing { nonce };
        }
        if let Some((_, sent_at)) = self.outstanding_ping
            && now.saturating_since(sent_at) >= self.timeout
        {
            // The pong never came; the receive check above decides liveness,
            // so only forget the probe and allow another.
            self.outstanding_ping = None;
        }
        KeepaliveAction::Nothing
    }
}
