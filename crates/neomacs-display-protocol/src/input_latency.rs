//! Opt-in causal input-to-presentation measurements.
//!
//! A received input is not an acknowledgement. Its command must have started,
//! and a sealed layout must show a changed viewport, before native confirmation.
//! Storage is bounded, and a coalesced/discarded layout may be superseded by a
//! later one without losing the input's original receive time.

use crate::PresentationId;
use std::{
    cell::RefCell,
    collections::VecDeque,
    io::Write,
    path::PathBuf,
    sync::{Mutex, OnceLock},
};

const MAX_PENDING: usize = 256;
const MAX_PRESENTATIONS: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InputToken(u64);

/// Timestamp in a platform clock domain, never a scheduler prediction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlatformTimestamp {
    pub clock_id: u32,
    pub nanoseconds: u64,
}

/// What the platform actually timed; neither variant claims photon visibility.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PresentationObservation {
    Compositor,
    FirstPixelOutput { uncertainty_ns: u64 },
}
impl PresentationObservation {
    fn label(self) -> &'static str {
        match self {
            Self::Compositor => "compositor-confirmed",
            Self::FirstPixelOutput { .. } => "native-first-pixel-output",
        }
    }
    fn uncertainty(self) -> Option<u64> {
        match self {
            Self::Compositor => None,
            Self::FirstPixelOutput { uncertainty_ns } => Some(uncertainty_ns),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScrollViewport {
    pub window: u64,
    pub buffer: u64,
    pub start: usize,
    pub hscroll: usize,
    pub vscroll: i32,
}

#[derive(Debug)]
struct Pending {
    token: InputToken,
    frame: u64,
    kind: &'static str,
    received: PlatformTimestamp,
    baseline: Option<Vec<ScrollViewport>>,
    completed: bool,
    layouts: VecDeque<PresentationId>,
    projected_submissions: VecDeque<u64>,
    projected: Option<(u64, PlatformTimestamp, PresentationObservation)>,
}

#[derive(Default)]
struct Measurements {
    next: u64,
    pending: VecDeque<Pending>,
    dropped: u64,
}

impl Measurements {
    fn receive(
        &mut self,
        frame: u64,
        kind: &'static str,
        received: PlatformTimestamp,
    ) -> InputToken {
        self.next += 1;
        let token = InputToken(self.next);
        if self.pending.len() == MAX_PENDING {
            self.pending.pop_front();
            self.dropped += 1;
        }
        self.pending.push_back(Pending {
            token,
            frame,
            kind,
            received,
            baseline: None,
            completed: false,
            layouts: VecDeque::new(),
            projected_submissions: VecDeque::new(),
            projected: None,
        });
        token
    }

    fn start(
        &mut self,
        tokens: &[InputToken],
        mut viewport: impl FnMut(u64) -> Vec<ScrollViewport>,
    ) {
        for item in &mut self.pending {
            if tokens.contains(&item.token) {
                item.baseline = Some(viewport(item.frame));
            }
        }
    }

    fn finish(
        &mut self,
        tokens: &[InputToken],
        mut viewport: impl FnMut(u64) -> Vec<ScrollViewport>,
    ) {
        self.pending.retain_mut(|item| {
            if !tokens.contains(&item.token) {
                return true;
            }
            item.completed = true;
            // A no-op must not be attributed to some unrelated future scroll.
            !item.layouts.is_empty()
                || item.baseline.as_ref().is_some_and(|baseline| {
                    !baseline.is_empty() && *baseline != viewport(item.frame)
                })
        });
    }

    fn cancel(&mut self, tokens: &[InputToken]) {
        self.pending.retain(|item| !tokens.contains(&item.token));
    }

    fn seal(&mut self, frame: u64, layout: PresentationId, viewport: &[ScrollViewport]) {
        for item in &mut self.pending {
            if item.frame == frame
                && item.baseline.as_ref().is_some_and(|baseline| {
                    !baseline.is_empty() && (item.completed || baseline != viewport)
                })
            {
                if item.layouts.len() == MAX_PRESENTATIONS {
                    item.layouts.pop_front();
                }
                item.layouts.push_back(layout);
            }
        }
    }

    fn projected_requested(&mut self, tokens: &[InputToken], frame: u64, serial: u64) {
        for item in &mut self.pending {
            if item.frame == frame && item.projected.is_none() && tokens.contains(&item.token) {
                if item.projected_submissions.len() == MAX_PRESENTATIONS {
                    item.projected_submissions.pop_front();
                }
                item.projected_submissions.push_back(serial);
            }
        }
    }

    #[cfg(test)]
    fn projected_confirmed(&mut self, frame: u64, serial: u64, time: PlatformTimestamp) {
        self.projected_observed(frame, serial, time, PresentationObservation::Compositor);
    }

    fn projected_observed(
        &mut self,
        frame: u64,
        serial: u64,
        time: PlatformTimestamp,
        observation: PresentationObservation,
    ) {
        for item in &mut self.pending {
            if item.frame == frame
                && item.projected_submissions.contains(&serial)
                && item.received.clock_id == time.clock_id
                && time.nanoseconds >= item.received.nanoseconds
                && item
                    .projected
                    .is_none_or(|(_, previous, _)| time.nanoseconds < previous.nanoseconds)
            {
                item.projected = Some((serial, time, observation));
            }
        }
    }

    #[cfg(test)]
    fn confirmed(
        &mut self,
        layout: PresentationId,
        time: PlatformTimestamp,
    ) -> Vec<serde_json::Value> {
        self.observed(layout, time, PresentationObservation::Compositor)
    }

    fn observed(
        &mut self,
        layout: PresentationId,
        time: PlatformTimestamp,
        observation: PresentationObservation,
    ) -> Vec<serde_json::Value> {
        let mut samples = Vec::new();
        self.pending.retain(|item| {
            if !item.layouts.contains(&layout) {
                return true;
            }
            // Clock disagreement is unavailable evidence, never zero latency.
            let latency = (time.clock_id == item.received.clock_id)
                .then(|| time.nanoseconds.checked_sub(item.received.nanoseconds))
                .flatten();
            samples.push(serde_json::json!({
                "input": item.token.0, "frame": item.frame, "kind": item.kind,
                "presentation": layout.get(), "clock_id": time.clock_id,
                "observation": observation.label(), "timestamp_uncertainty_ns": observation.uncertainty(),
                "projected_observation": item.projected.map(|(_, _, kind)| kind.label()),
                "projected_timestamp_uncertainty_ns": item.projected.and_then(|(_, _, kind)| kind.uncertainty()),
                "received_ns": item.received.nanoseconds, "presented_ns": time.nanoseconds,
                "input_to_present_ns": latency, "evicted_inputs": self.dropped,
                "projected_submission": item.projected.map(|(serial, _, _)| serial),
                "projected_presented_ns": item.projected.map(|(_, time, _)| time.nanoseconds),
                "input_to_projected_present_ns": item.projected.map(|(_, time, _)| time.nanoseconds - item.received.nanoseconds),
            }));
            false
        });
        samples
    }
}

struct Recorder {
    path: PathBuf,
    measurements: Mutex<Measurements>,
}
static RECORDER: OnceLock<Option<Recorder>> = OnceLock::new();
fn recorder() -> Option<&'static Recorder> {
    RECORDER
        .get_or_init(|| {
            std::env::var_os("NEOMACS_INPUT_LATENCY_FILE")
                .filter(|p| !p.is_empty())
                .map(|path| Recorder {
                    path: path.into(),
                    measurements: Mutex::new(Measurements::default()),
                })
        })
        .as_ref()
}

pub fn enabled() -> bool {
    recorder().is_some()
}

pub fn received(frame: u64, kind: &'static str, time: PlatformTimestamp) -> Option<InputToken> {
    Some(
        recorder()?
            .measurements
            .lock()
            .unwrap()
            .receive(frame, kind, time),
    )
}

thread_local! { static CONSUMED: RefCell<Vec<InputToken>> = const { RefCell::new(Vec::new()) }; }

/// Called only when the ordered input reader removes the actual command event.
pub fn consumed(token: InputToken) {
    CONSUMED.with_borrow_mut(|tokens| {
        if tokens.len() == MAX_PENDING {
            tokens.remove(0);
        }
        tokens.push(token);
    });
}

/// Captures inputs for one executing command. In-command redisplay can be the
/// first response; it qualifies only after the viewport actually changes.
/// Dropping an unfinished command cancels inputs that have not been presented.
pub struct CommandInputs {
    tokens: Vec<InputToken>,
    completed: bool,
}
impl CommandInputs {
    pub fn begin() -> Self {
        Self {
            tokens: CONSUMED.with_borrow_mut(std::mem::take),
            completed: false,
        }
    }
    pub fn start(&self, viewport: impl FnMut(u64) -> Vec<ScrollViewport>) {
        if let Some(recorder) = recorder() {
            recorder
                .measurements
                .lock()
                .unwrap()
                .start(&self.tokens, viewport);
        }
    }
    pub fn complete(mut self, viewport: impl FnMut(u64) -> Vec<ScrollViewport>) {
        if let Some(recorder) = recorder() {
            recorder
                .measurements
                .lock()
                .unwrap()
                .finish(&self.tokens, viewport);
        }
        self.completed = true;
    }
}

impl Drop for CommandInputs {
    fn drop(&mut self) {
        if !self.completed {
            if let Some(recorder) = recorder() {
                recorder.measurements.lock().unwrap().cancel(&self.tokens);
            }
        }
        CONSUMED.with_borrow_mut(Vec::clear);
    }
}

pub fn sealed(frame: u64, layout: PresentationId, viewport: impl FnOnce() -> Vec<ScrollViewport>) {
    if let Some(recorder) = recorder() {
        recorder
            .measurements
            .lock()
            .unwrap()
            .seal(frame, layout, &viewport());
    }
}

/// Associate diagnostic inputs with the exact native submission that paints
/// their projection. Layout IDs alone cannot distinguish repeated submissions.
pub fn projected_requested(tokens: &[InputToken], frame: u64, serial: u64) {
    if tokens.is_empty() {
        return;
    }
    if let Some(recorder) = recorder() {
        recorder
            .measurements
            .lock()
            .unwrap()
            .projected_requested(tokens, frame, serial);
    }
}

pub fn projected_confirmed(frame: u64, serial: u64, time: PlatformTimestamp) {
    projected_observed(frame, serial, time, PresentationObservation::Compositor);
}

pub fn projected_observed(
    frame: u64,
    serial: u64,
    time: PlatformTimestamp,
    observation: PresentationObservation,
) {
    if let Some(recorder) = recorder() {
        recorder
            .measurements
            .lock()
            .unwrap()
            .projected_observed(frame, serial, time, observation);
    }
}

pub fn confirmed(layout: PresentationId, time: PlatformTimestamp) {
    observed(layout, time, PresentationObservation::Compositor);
}

pub fn observed(
    layout: PresentationId,
    time: PlatformTimestamp,
    observation: PresentationObservation,
) {
    let Some(recorder) = recorder() else { return };
    let samples = recorder
        .measurements
        .lock()
        .unwrap()
        .observed(layout, time, observation);
    if samples.is_empty() {
        return;
    }
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&recorder.path)
    {
        Ok(mut file) => {
            for sample in samples {
                let _ = writeln!(file, "{sample}");
            }
        }
        Err(error) => tracing::warn!(%error, "cannot write input latency samples"),
    }
}

#[cfg(test)]
#[path = "tests/input_latency_test.rs"]
mod tests;
