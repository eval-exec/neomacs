//! Sustained device delivery, with no command-completion waits between inputs.
use super::*;
use std::time::Instant;
#[path = "budget.rs"]
mod budget;
#[path = "latency.rs"]
mod latency;

#[derive(Clone, Copy, Debug, serde::Serialize)]
pub(super) enum Workload {
    Plain,
    Rich,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub(super) enum Mode {
    /// Gate row: verify sustained-stream state transitions (drain, advance,
    /// exact reversal, complete non-evicted receipts) without asserting
    /// host-dependent software-renderer pacing budgets.
    StateOnly,
    /// Calibrated perf/pacing row: run the capacity probe and enforce
    /// `ResponseBudget` latency/cadence limits on a quiet, calibrated host.
    CalibratedPacing,
}

pub(super) fn run(
    trackpad: &mut wayland::Trackpad,
    artifacts: &Path,
    state_path: &Path,
    latency_path: &Path,
    initial: Value,
    workload: Workload,
    mode: Mode,
) {
    let (environment, selected_budget) = match mode {
        Mode::StateOnly => (None, None),
        Mode::CalibratedPacing => {
            let environment = budget::Environment::capture(artifacts, workload);
            let selected_budget = environment.budget();
            fs::write(
                artifacts.join("continuous-environment.json"),
                serde_json::to_vec_pretty(
                    &serde_json::json!({"environment": environment, "budget": selected_budget}),
                )
                .unwrap(),
            )
            .unwrap();
            let selected_budget = selected_budget.unwrap_or_else(|error| {
                panic!("uncalibrated scroll environment: {error:?}; {environment:?}; {artifacts:?}")
            });
            (Some(environment), Some(selected_budget))
        }
    };
    // Calibrated pacing runs 480 inputs (overrideable via env to expose
    // intermittent stalls across coverage turnover). State-only gate runs keep
    // total in-flight inputs below the latency ledger's MAX_PENDING (256) so a
    // busy machine slows draining instead of overflowing the diagnostic ledger.
    let inputs = match mode {
        Mode::StateOnly => 240,
        Mode::CalibratedPacing => {
            let count = std::env::var("NEOMACS_GUI_SCROLL_STREAM_INPUTS")
                .map(|value| {
                    value
                        .parse::<usize>()
                        .expect("stream input count must be an integer")
                })
                .unwrap_or(480);
            assert!(
                (480..=12_000).contains(&count) && count.is_multiple_of(2),
                "stream input count must be even and between 480 and 12000"
            );
            count
        }
    };
    const PIXELS: f64 = 4.0;
    let period = Duration::from_nanos(1_000_000_000 / 120);
    let started = Instant::now();
    let mut sent_ns = Vec::with_capacity(inputs);
    // How late each paced send left relative to its 120 Hz slot: the test
    // process's own view of CPU contention during the measured window.
    let mut send_lateness_ns = Vec::with_capacity(inputs);
    let load_before = HostLoad::sample();
    let mut observations = vec![initial.clone()];
    let sample_state = |observations: &mut Vec<Value>| {
        if let Ok(bytes) = fs::read(state_path)
            && let Ok(state) = serde_json::from_slice::<Value>(&bytes)
            && state["sample"].as_u64() > observations.last().unwrap()["sample"].as_u64()
        {
            observations.push(state);
        }
    };
    let wait_for_pixels = |observations: &mut Vec<Value>, target_pixels: f64, timeout: Duration| {
        let deadline = Instant::now() + timeout;
        loop {
            sample_state(observations);
            if observations.last().unwrap()["processed-pixels"]
                .as_f64()
                .unwrap()
                >= target_pixels
                || Instant::now() >= deadline
            {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
    };
    let initial_pixels = initial["processed-pixels"].as_f64().unwrap();
    for index in 0..inputs {
        if mode == Mode::StateOnly && index == inputs / 2 {
            // Deterministically observe the forward viewport advance before
            // reversing, rather than hoping the 50 ms timer fires mid-burst
            // under heavy CPU load.
            wait_for_pixels(
                &mut observations,
                initial_pixels + (inputs / 2) as f64 * PIXELS,
                Duration::from_secs(20),
            );
        }
        let deadline = started + period * index as u32;
        if mode == Mode::CalibratedPacing
            && let Some(remaining) = deadline.checked_duration_since(Instant::now())
        {
            thread::sleep(remaining);
        }
        send_lateness_ns.push(
            Instant::now()
                .saturating_duration_since(deadline)
                .as_nanos() as u64,
        );
        sent_ns.push(started.elapsed().as_nanos() as u64);
        trackpad.scroll(if index < inputs / 2 { PIXELS } else { -PIXELS });
        // Observe progress without waiting for a timer or a command. Sampling
        // files is outside the editor and does not force its redisplay.
        sample_state(&mut observations);
    }
    let injected_ms = started.elapsed().as_secs_f64() * 1000.0;
    let contention = Contention::measure(&send_lateness_ns, load_before, HostLoad::sample());
    let save = |observations: &[Value], drained_ms: Option<f64>| {
        fs::write(
            artifacts.join("continuous-input.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "requested_hz": (mode == Mode::CalibratedPacing).then_some(120),
                "inputs": inputs,
                "injected_ms": injected_ms,
                "drained_ms": drained_ms,
                "sent_ns": sent_ns,
                "send_lateness_ns": send_lateness_ns,
                "contention": contention,
                "observations": observations,
            }))
            .unwrap(),
        )
        .unwrap();
    };
    // Keep injection/progress evidence even if draining times out.
    save(&observations, None);
    let expected_pixels = initial_pixels + inputs as f64 * PIXELS;
    wait_for_pixels(&mut observations, expected_pixels, Duration::from_secs(20));
    let completed = observations.last().unwrap();
    let drained = completed["processed-pixels"].as_f64().unwrap() >= expected_pixels;
    save(
        &observations,
        drained.then(|| started.elapsed().as_secs_f64() * 1000.0),
    );

    let deadline = Instant::now() + Duration::from_secs(20);
    let samples: Vec<Value> = loop {
        let samples: Vec<Value> = fs::read_to_string(latency_path)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();
        if samples.len() >= inputs || Instant::now() >= deadline {
            break samples;
        }
        thread::sleep(Duration::from_millis(20));
    };
    let report = latency::Report::from_samples(&samples);
    fs::write(
        artifacts.join("continuous-presentations.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "mode": mode,
            "expected_receipts": inputs,
            "sway_renderer_request": fs::read_to_string(artifacts.join("sway-renderer-request")).ok(),
            "editor_adapter": fs::read_to_string(artifacts.join("neomacs.log"))
                .ok().and_then(|log| log.lines().find_map(|line|
                    line.split_once("wgpu adapter: ").map(|(_, adapter)| adapter.to_owned()))),
            "samples": samples,
            "report": report,
            "response_budget": selected_budget,
            "environment": environment,
        }))
        .unwrap(),
    )
    .unwrap();
    assert!(
        drained,
        "continuous input did not drain: {completed}; {environment:?}; {artifacts:?}"
    );
    assert!(
        observations
            .iter()
            .any(|state| state["start"].as_u64() > initial["start"].as_u64()),
        "continuous input never advanced the visible viewport: {environment:?}; {artifacts:?}"
    );
    assert_eq!(
        completed["start"], initial["start"],
        "reversal: {environment:?}; {artifacts:?}"
    );
    assert_eq!(
        completed["vscroll"], initial["vscroll"],
        "reversal: {environment:?}; {artifacts:?}"
    );

    // Preserve partial evidence before reporting missing or evicted receipts.
    assert_eq!(
        samples.len(),
        inputs,
        "missing receipts: {environment:?}; {artifacts:?}"
    );
    for sample in &samples {
        assert_eq!(sample["kind"], "precise", "{environment:?}");
        assert_eq!(
            sample["evicted_inputs"], 0,
            "latency ledger overflow: {environment:?}; {artifacts:?}"
        );
        assert!(
            sample["input_to_present_ns"].is_u64(),
            "clock mismatch: {sample}; {environment:?}"
        );
    }
    if let Some(selected_budget) = &selected_budget
        && report.assess(selected_budget) != latency::Assessment::WithinBudget
    {
        // The budget is unchanged. A miss on a contended host is not evidence
        // about the editor, so it fails with a rerun diagnosis instead of
        // reading as a pacing regression.
        assert!(
            !contention.contended(),
            "host contended, rerun: {contention:?}; continuous scroll exceeded response budget: {report:?}; budget={selected_budget:?}; {environment:?}; {artifacts:?}"
        );
        panic!(
            "continuous scroll exceeded response budget on an uncontended host: {report:?}; budget={selected_budget:?}; {contention:?}; {environment:?}; {artifacts:?}"
        );
    }
}

/// One `/proc/loadavg` 1-minute sample against the online CPU count.
#[derive(Clone, Copy, Debug, serde::Serialize)]
struct HostLoad {
    load_1m: Option<f64>,
    cpus: usize,
}

impl HostLoad {
    fn sample() -> Self {
        let load_1m = fs::read_to_string("/proc/loadavg")
            .ok()
            .and_then(|text| text.split_whitespace().next()?.parse().ok());
        let cpus = thread::available_parallelism().map_or(1, usize::from);
        Self { load_1m, cpus }
    }

    /// More runnable work than half the CPUs: a software-Vulkan presenter
    /// no longer gets a quiet core.
    fn busy(self) -> bool {
        self.load_1m
            .is_some_and(|load| load > self.cpus as f64 / 2.0)
    }
}

/// Host contention over the measured window, from the test's own paced
/// sends (p95 lateness against the 120 Hz slot) and the load average at
/// both ends.
#[derive(Debug, serde::Serialize)]
struct Contention {
    send_lateness_p95_us: Option<u64>,
    send_lateness_max_us: Option<u64>,
    before: HostLoad,
    after: HostLoad,
}

impl Contention {
    /// Late sends above this share of the 8.33 ms period mean the test
    /// process itself was descheduled while it paced input.
    const LATE_SEND_P95_US: u64 = 1_000;

    fn measure(lateness_ns: &[u64], before: HostLoad, after: HostLoad) -> Self {
        let mut sorted: Vec<u64> = lateness_ns.iter().map(|ns| ns / 1_000).collect();
        sorted.sort_unstable();
        let p95 =
            (!sorted.is_empty()).then(|| sorted[(sorted.len() * 95 / 100).min(sorted.len() - 1)]);
        Self {
            send_lateness_p95_us: p95,
            send_lateness_max_us: sorted.last().copied(),
            before,
            after,
        }
    }

    fn contended(&self) -> bool {
        self.send_lateness_p95_us
            .is_some_and(|p95| p95 > Self::LATE_SEND_P95_US)
            || self.before.busy()
            || self.after.busy()
    }
}

#[cfg(test)]
mod contention_tests {
    use super::{Contention, HostLoad};

    const QUIET: HostLoad = HostLoad {
        load_1m: Some(0.5),
        cpus: 12,
    };

    #[test]
    fn punctual_sends_on_a_quiet_host_are_not_contention() {
        let lateness = vec![200_000; 480];
        assert!(!Contention::measure(&lateness, QUIET, QUIET).contended());
    }

    #[test]
    fn late_sends_or_a_busy_host_mean_contention() {
        // 5% of sends 2 ms late puts the p95 over the 1 ms bound.
        let mut lateness = vec![100_000; 456];
        lateness.extend(vec![2_000_000; 24]);
        assert!(Contention::measure(&lateness, QUIET, QUIET).contended());

        let busy = HostLoad {
            load_1m: Some(7.0),
            cpus: 12,
        };
        assert!(Contention::measure(&[0; 480], busy, QUIET).contended());
        assert!(Contention::measure(&[0; 480], QUIET, busy).contended());
    }

    #[test]
    fn an_unreadable_load_average_does_not_by_itself_mean_contention() {
        let unknown = HostLoad {
            load_1m: None,
            cpus: 12,
        };
        assert!(!Contention::measure(&[0; 480], unknown, unknown).contended());
    }
}
