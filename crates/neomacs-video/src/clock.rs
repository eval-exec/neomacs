use std::time::{Duration, Instant};

use crate::{MediaTime, PlaybackRate, VideoSessionState};

/// Session-local mapping between decoder PTS and the application's monotonic
/// clock. Decoder timestamps are never compared directly with process uptime.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PlaybackClock {
    anchor_wall: Instant,
    anchor_media: MediaTime,
    rate: PlaybackRate,
    running: bool,
}

impl PlaybackClock {
    pub(crate) fn new(now: Instant, initial_state: VideoSessionState) -> Self {
        Self {
            anchor_wall: now,
            anchor_media: MediaTime::ZERO,
            rate: PlaybackRate::NORMAL,
            running: matches!(initial_state, VideoSessionState::Playing),
        }
    }

    pub(crate) fn media_time(self, now: Instant) -> MediaTime {
        if !self.running {
            return self.anchor_media;
        }
        let elapsed = now.saturating_duration_since(self.anchor_wall);
        let scaled = elapsed.as_secs_f64() * self.rate.get();
        let nanos = Duration::from_secs_f64(scaled)
            .as_nanos()
            .min(u128::from(u64::MAX)) as u64;
        self.anchor_media
            .saturating_add(MediaTime::from_nanos(nanos))
    }

    pub(crate) fn deadline_for(self, pts: MediaTime, now: Instant) -> Option<Instant> {
        if !self.running {
            return None;
        }
        let media_now = self.media_time(now);
        let remaining = pts.as_nanos().checked_sub(media_now.as_nanos())?;
        let wall_seconds = (remaining as f64 / 1_000_000_000.0) / self.rate.get();
        now.checked_add(Duration::from_secs_f64(wall_seconds))
    }

    pub(crate) fn set_running(&mut self, running: bool, now: Instant) {
        self.anchor_media = self.media_time(now);
        self.anchor_wall = now;
        self.running = running;
    }

    pub(crate) fn seek(&mut self, position: MediaTime, now: Instant) {
        self.anchor_media = position;
        self.anchor_wall = now;
    }

    pub(crate) fn set_rate(&mut self, rate: PlaybackRate, now: Instant) {
        self.anchor_media = self.media_time(now);
        self.anchor_wall = now;
        self.rate = rate;
    }
}
