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
mod tests {
    use super::*;
    fn viewport(start: usize) -> Vec<ScrollViewport> {
        vec![ScrollViewport {
            window: 1,
            buffer: 1,
            start,
            hscroll: 0,
            vscroll: 0,
        }]
    }
    fn time(nanoseconds: u64) -> PlatformTimestamp {
        PlatformTimestamp {
            clock_id: 1,
            nanoseconds,
        }
    }
    #[test]
    fn output_confirmation_preserves_its_measurement_kind_and_uncertainty() {
        let mut measurements = Measurements::default();
        let token = measurements.receive(7, "precise", time(100));
        let kind = PresentationObservation::FirstPixelOutput { uncertainty_ns: 70 };
        measurements.start(&[token], |_| viewport(1));
        measurements.projected_requested(&[token], 7, 41);
        measurements.projected_observed(7, 41, time(150), kind);
        measurements.seal(7, PresentationId::new(2), &viewport(2));
        let samples = measurements.observed(PresentationId::new(2), time(300), kind);
        assert_eq!(samples[0]["observation"], "native-first-pixel-output");
        assert_eq!(
            samples[0]["projected_observation"],
            "native-first-pixel-output"
        );
        assert_eq!(samples[0]["timestamp_uncertainty_ns"], 70);
        assert_eq!(samples[0]["projected_timestamp_uncertainty_ns"], 70);
        assert_eq!(samples[0]["input_to_present_ns"], 200);
    }

    #[test]
    fn projected_latency_requires_exact_native_submission_and_keeps_authoritative_completion() {
        let mut measurements = Measurements::default();
        let token = measurements.receive(7, "precise", time(100));
        measurements.projected_requested(&[token], 7, 41);
        measurements.projected_confirmed(8, 41, time(110));
        measurements.projected_confirmed(7, 40, time(120));
        measurements.projected_confirmed(
            7,
            41,
            PlatformTimestamp {
                clock_id: 2,
                nanoseconds: 130,
            },
        );
        assert!(measurements.pending[0].projected.is_none());
        measurements.projected_confirmed(7, 41, time(150));
        measurements.projected_confirmed(7, 41, time(160));
        assert!(
            measurements
                .confirmed(PresentationId::new(1), time(170))
                .is_empty()
        );
        measurements.start(&[token], |_| viewport(1));
        measurements.seal(7, PresentationId::new(2), &viewport(2));
        let samples = measurements.confirmed(PresentationId::new(2), time(300));
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0]["input_to_projected_present_ns"], 50);
        assert_eq!(samples[0]["projected_submission"], 41);
        assert_eq!(samples[0]["input_to_present_ns"], 200);
        assert!(measurements.pending.is_empty());
    }

    #[test]
    fn queued_input_and_other_frames_cannot_complete_a_latency_sample() {
        let mut measurements = Measurements::default();
        let token = measurements.receive(7, "wheel", time(100));
        measurements.seal(7, PresentationId::new(1), &viewport(2));
        assert!(
            measurements
                .confirmed(PresentationId::new(1), time(200))
                .is_empty()
        );
        measurements.start(&[token], |_| viewport(1));
        measurements.seal(8, PresentationId::new(2), &viewport(2));
        assert!(
            measurements
                .confirmed(PresentationId::new(2), time(300))
                .is_empty()
        );
        measurements.seal(7, PresentationId::new(3), &viewport(2));
        let samples = measurements.confirmed(PresentationId::new(3), time(400));
        assert_eq!(samples[0]["input_to_present_ns"], 300);
        assert!(
            measurements
                .confirmed(PresentationId::new(3), time(500))
                .is_empty()
        );
    }
    #[test]
    fn superseded_layout_keeps_original_input_time_and_bounds_memory() {
        let mut measurements = Measurements::default();
        for _ in 0..MAX_PENDING + 1 {
            measurements.receive(7, "precise", time(100));
        }
        assert_eq!(measurements.pending.len(), MAX_PENDING);
        assert_eq!(measurements.dropped, 1);
        let token = measurements.pending.back().unwrap().token;
        measurements.start(&[token], |_| viewport(1));
        for id in 1..=20 {
            measurements.seal(7, PresentationId::new(id), &viewport(2));
        }
        assert_eq!(
            measurements.pending.back().unwrap().layouts.len(),
            MAX_PRESENTATIONS
        );
        assert!(
            measurements
                .confirmed(PresentationId::new(1), time(200))
                .is_empty()
        );
        let samples = measurements.confirmed(PresentationId::new(20), time(500));
        assert_eq!(samples[0]["input_to_present_ns"], 400);
        assert_eq!(samples[0]["evicted_inputs"], 1);
    }
    #[test]
    fn command_preflight_cannot_ack_before_the_viewport_changes() {
        let mut measurements = Measurements::default();
        let token = measurements.receive(7, "precise", time(100));
        measurements.start(&[token], |_| viewport(1));
        measurements.seal(7, PresentationId::new(1), &viewport(1));
        assert!(
            measurements
                .confirmed(PresentationId::new(1), time(200))
                .is_empty()
        );
        measurements.seal(7, PresentationId::new(2), &viewport(2));
        assert_eq!(
            measurements
                .confirmed(PresentationId::new(2), time(300))
                .len(),
            1
        );
        let token = measurements.receive(7, "page", time(400));
        measurements.start(&[token], |_| viewport(2));
        measurements.cancel(&[token]);
        measurements.seal(7, PresentationId::new(3), &viewport(3));
        assert!(
            measurements
                .confirmed(PresentationId::new(3), time(500))
                .is_empty()
        );
    }

    #[test]
    fn completed_no_op_cannot_be_credited_to_a_later_command() {
        let mut measurements = Measurements::default();
        let token = measurements.receive(7, "page", time(100));
        measurements.start(&[token], |_| viewport(1));
        measurements.finish(&[token], |_| viewport(1));
        measurements.seal(7, PresentationId::new(2), &viewport(2));
        assert!(
            measurements
                .confirmed(PresentationId::new(2), time(300))
                .is_empty()
        );
    }

    #[test]
    fn completed_scrolls_can_be_coalesced_back_to_the_original_viewport() {
        let mut measurements = Measurements::default();
        let token = measurements.receive(7, "precise", time(100));
        measurements.start(&[token], |_| viewport(1));
        measurements.finish(&[token], |_| viewport(2));
        measurements.seal(7, PresentationId::new(2), &viewport(1));
        assert_eq!(
            measurements
                .confirmed(PresentationId::new(2), time(300))
                .len(),
            1
        );
    }

    #[test]
    fn unrelated_clock_cannot_fabricate_latency() {
        let mut measurements = Measurements::default();
        let token = measurements.receive(7, "page", time(100));
        measurements.start(&[token], |_| viewport(1));
        measurements.seal(7, PresentationId::new(1), &viewport(2));
        let samples = measurements.confirmed(
            PresentationId::new(1),
            PlatformTimestamp {
                clock_id: 2,
                nanoseconds: 200,
            },
        );
        assert!(samples[0]["input_to_present_ns"].is_null());
    }
}
