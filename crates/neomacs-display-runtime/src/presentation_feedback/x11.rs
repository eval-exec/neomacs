//! Exact Vulkan present-ID receipts for an X11 window. Polling never requests
//! redraws, waits for the device, or guesses timestamps from CPU completion.
use neomacs_display_protocol::{
    PresentationId,
    frame_time::{EventTime, observe_platform_now},
    input_latency::{self, InputToken, PlatformTimestamp, PresentationObservation},
};
use neomacs_renderer_wgpu::native_presentation::SwapchainTiming;
use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    time::Duration,
};

struct Pending {
    serial: u64,
    layout: PresentationId,
    size: (u32, u32),
    scale: f64,
    requested: EventTime,
}
struct Session {
    generation: u64,
    timing: Option<SwapchainTiming>,
    pending: VecDeque<Pending>,
}

pub(super) struct Observer {
    path: Option<PathBuf>,
    sessions: HashMap<u64, Session>,
    next_serial: u64,
    latest_written: u64,
}
impl Observer {
    pub(super) fn new() -> Self {
        Self {
            path: std::env::var_os("NEOMACS_GUI_PRESENTATION_RECEIPT")
                .map(PathBuf::from)
                .or_else(|| {
                    std::env::var_os("NEOMACS_INPUT_LATENCY_FILE")
                        .map(|p| PathBuf::from(p).with_extension("receipt"))
                }),
            sessions: HashMap::new(),
            next_serial: 0,
            latest_written: 0,
        }
    }

    pub(super) fn before_present(
        &mut self,
        device: &wgpu::Device,
        surface: &wgpu::Surface<'_>,
        generation: u64,
        frame: u64,
        layout: PresentationId,
        size: (u32, u32),
        scale: f64,
        projected: &[InputToken],
    ) {
        if self.path.is_none() || (!self.sessions.contains_key(&frame) && self.sessions.len() >= 16)
        {
            return;
        }
        // SAFETY: render-thread ownership keeps this surface configured on the
        // supplied renderer device; generations change on every configure.
        let session = self.sessions.entry(frame).or_insert_with(|| Session {
            generation,
            timing: unsafe { SwapchainTiming::new(device, surface, generation) },
            pending: VecDeque::new(),
        });
        if session.generation != generation
            || session
                .timing
                .as_ref()
                .is_some_and(|timing| !timing.matches(surface, generation))
        {
            *session = Session {
                generation,
                timing: unsafe { SwapchainTiming::new(device, surface, generation) },
                pending: VecDeque::new(),
            };
        }
        let Some(timing) = &mut session.timing else {
            return;
        };
        let Some(serial) = self.next_serial.checked_add(1) else {
            return;
        };
        if session.pending.len() >= 256 || !unsafe { timing.request(surface, generation, serial) } {
            return;
        }
        self.next_serial = serial;
        session.pending.push_back(Pending {
            serial,
            layout,
            size,
            scale,
            requested: observe_platform_now(),
        });
        input_latency::projected_requested(projected, frame, serial);
    }

    pub(super) fn deadline(&mut self, now: EventTime) -> Option<EventTime> {
        for session in self.sessions.values_mut() {
            session
                .pending
                .retain(|pending| now.saturating_since(pending.requested) < Duration::from_secs(1));
        }
        self.sessions
            .values()
            .any(|session| !session.pending.is_empty())
            .then(|| now.plus(Duration::from_millis(4)))
    }

    pub(super) fn dispatch<'a>(
        &mut self,
        device: &wgpu::Device,
        mut surface: impl FnMut(u64) -> Option<(&'a wgpu::Surface<'static>, u64)>,
    ) {
        let Some(path) = &self.path else {
            return;
        };
        // SAFETY: the caller borrows each current surface from the renderer
        // owning device; reconfiguration cannot run during this dispatch.
        self.sessions.retain(|&frame, session| {
            let Some((surface, generation)) = surface(frame) else { return false; };
            let Some(timing) = &mut session.timing else { return true; };
            let Some(observations) = (unsafe { timing.poll(device, surface, generation) }) else {
                session.timing = None;
                session.pending.clear();
                return true;
            };
            for observation in observations {
                let Some(index) = session.pending.iter().position(|pending| pending.serial == observation.present_id) else { continue; };
                let pending = session.pending.remove(index).unwrap();
                let time = PlatformTimestamp { clock_id: libc::CLOCK_MONOTONIC as u32, nanoseconds: observation.monotonic_ns };
                let kind = PresentationObservation::FirstPixelOutput { uncertainty_ns: observation.uncertainty_ns };
                input_latency::projected_observed(frame, pending.serial, time, kind);
                input_latency::observed(pending.layout, time, kind);
                if pending.serial <= self.latest_written { continue; }
                let receipt = format!("(:submission {} :frame {} :presentation {} :width {} :height {} :scale {} :outcome presented :observation native-first-pixel-output :uncertainty-ns {} :clock-id {} :seconds {} :nanoseconds {})\n",
                    pending.serial, frame, pending.layout.get(), pending.size.0, pending.size.1, pending.scale,
                    observation.uncertainty_ns, time.clock_id, time.nanoseconds / 1_000_000_000, time.nanoseconds % 1_000_000_000);
                let temporary = path.with_extension("pending");
                match std::fs::write(&temporary, receipt).and_then(|()| std::fs::rename(&temporary, path)) {
                    Ok(()) => self.latest_written = pending.serial,
                    Err(error) => tracing::warn!(%error, "cannot write X11 presentation receipt"),
                }
            }
            true
        });
    }
    pub(super) fn shutdown(&mut self) {
        self.sessions.clear();
    }
}
