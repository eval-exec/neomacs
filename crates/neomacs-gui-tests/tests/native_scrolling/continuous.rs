//! Sustained device delivery, with no command-completion waits between inputs.
use super::*;
#[path = "latency.rs"]
mod latency;

pub(super) fn run(
    trackpad: &mut wayland::Trackpad,
    artifacts: &Path,
    state_path: &Path,
    latency_path: &Path,
    initial: Value,
) {
    // Longer opt-in runs expose intermittent stalls across coverage turnover.
    let inputs = std::env::var("NEOMACS_GUI_SCROLL_STREAM_INPUTS")
        .map(|value| {
            value
                .parse::<usize>()
                .expect("stream input count must be an integer")
        })
        .unwrap_or(480);
    assert!(
        (480..=12_000).contains(&inputs) && inputs.is_multiple_of(2),
        "stream input count must be even and between 480 and 12000"
    );
    const PIXELS: f64 = 4.0;
    let period = Duration::from_nanos(1_000_000_000 / 120);
    let started = Instant::now();
    let mut sent_ns = Vec::with_capacity(inputs);
    let mut observations = vec![initial.clone()];
    for index in 0..inputs {
        let deadline = started + period * index as u32;
        if let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
            thread::sleep(remaining);
        }
        sent_ns.push(started.elapsed().as_nanos() as u64);
        trackpad.scroll(if index < inputs / 2 { PIXELS } else { -PIXELS });
        // Observe progress without waiting for a timer or a command. Sampling
        // files is outside the editor and does not force its redisplay.
        if let Ok(bytes) = fs::read(state_path)
            && let Ok(state) = serde_json::from_slice::<Value>(&bytes)
            && state["sample"] != observations.last().unwrap()["sample"]
        {
            observations.push(state);
        }
    }
    let injected_ms = started.elapsed().as_secs_f64() * 1000.0;
    let save = |observations: &[Value], drained_ms: Option<f64>| {
        fs::write(
            artifacts.join("continuous-input.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "requested_hz": 120,
                "inputs": inputs,
                "injected_ms": injected_ms,
                "drained_ms": drained_ms,
                "sent_ns": sent_ns,
                "observations": observations,
            }))
            .unwrap(),
        )
        .unwrap();
    };
    // Keep injection/progress evidence even if draining times out.
    save(&observations, None);
    let expected_pixels = initial["processed-pixels"].as_f64().unwrap() + inputs as f64 * PIXELS;
    let drain_deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if let Ok(bytes) = fs::read(state_path)
            && let Ok(state) = serde_json::from_slice::<Value>(&bytes)
            && state["sample"].as_u64() > observations.last().unwrap()["sample"].as_u64()
        {
            observations.push(state);
        }
        if observations.last().unwrap()["processed-pixels"]
            .as_f64()
            .unwrap()
            >= expected_pixels
            || Instant::now() >= drain_deadline
        {
            break;
        }
        thread::sleep(Duration::from_millis(20));
    }
    let completed = observations.last().unwrap();
    let drained = completed["processed-pixels"].as_f64().unwrap() >= expected_pixels;
    save(
        &observations,
        drained.then(|| started.elapsed().as_secs_f64() * 1000.0),
    );

    let deadline = Instant::now() + Duration::from_secs(8);
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
            "expected_receipts": inputs,
            "sway_renderer_request": fs::read_to_string(artifacts.join("sway-renderer-request")).ok(),
            "editor_adapter": fs::read_to_string(artifacts.join("neomacs.log"))
                .ok().and_then(|log| log.lines().find_map(|line|
                    line.split_once("wgpu adapter: ").map(|(_, adapter)| adapter.to_owned()))),
            "samples": samples,
            "report": report,
            "response_budget_ms": {"p95": 50, "maximum": 250, "maximum_gap": 50},
        }))
        .unwrap(),
    )
    .unwrap();
    assert!(
        drained,
        "continuous input did not drain: {completed}; {artifacts:?}"
    );
    assert!(
        observations
            .iter()
            .any(|state| state["start"].as_u64() > initial["start"].as_u64()),
        "continuous input never advanced the visible viewport: {artifacts:?}"
    );
    assert_eq!(
        completed["start"], initial["start"],
        "reversal: {artifacts:?}"
    );
    assert_eq!(
        completed["vscroll"], initial["vscroll"],
        "reversal: {artifacts:?}"
    );

    // Preserve partial evidence before reporting missing or evicted receipts.
    assert_eq!(samples.len(), inputs);
    for sample in &samples {
        assert_eq!(sample["kind"], "precise");
        assert_eq!(
            sample["evicted_inputs"], 0,
            "latency ledger overflow: {artifacts:?}"
        );
        assert!(
            sample["input_to_present_ns"].is_u64(),
            "clock mismatch: {sample}"
        );
    }
    assert!(
        report.meets_response_budget(),
        "continuous scroll exceeded response budget: {report:?}; {artifacts:?}"
    );
}
