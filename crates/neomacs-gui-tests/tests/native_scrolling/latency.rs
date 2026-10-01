//! Native receipt analysis. These gaps describe confirmed responses to inputs,
//! not every GPU presentation or physical display scanout.
use serde::Serialize;
use serde_json::Value;

// Acceptance budgets for the sustained 120 Hz scenario. Keep these fixed
// across plain and rich content rather than relaxing them to fit a slow run.
const P95_RESPONSE_MS: f64 = 50.0;
const MAX_RESPONSE_MS: f64 = 250.0;
const MAX_RESPONSE_GAP_MS: f64 = 50.0;

#[derive(Debug, Serialize)]
pub(super) struct Report {
    measurement: &'static str,
    pub received_receipts: usize,
    invalid_clocks: usize,
    presented_ns: Vec<u64>,
    gaps_ms: Vec<f64>,
    first_visible_latency_ms: Vec<f64>,
    p95_response_ms: Option<f64>,
    max_response_ms: Option<f64>,
    max_response_gap_ms: Option<f64>,
}

impl Report {
    pub(super) fn from_samples(samples: &[Value]) -> Self {
        let mut presented_ns = Vec::new();
        let mut latencies = Vec::new();
        let mut invalid_clocks = 0;
        let mut last_first_response = 0;
        for sample in samples {
            let first = match (
                sample["projected_presented_ns"].as_u64(),
                sample["presented_ns"].as_u64(),
            ) {
                (Some(projected), Some(authoritative)) => Some(projected.min(authoritative)),
                (projected, authoritative) => projected.or(authoritative),
            };
            let elapsed = first
                .zip(sample["received_ns"].as_u64())
                .and_then(|(presented, received)| presented.checked_sub(received));
            if let (Some(presented), Some(elapsed)) = (first, elapsed) {
                last_first_response = last_first_response.max(presented);
                // A later authoritative response can bridge the cadence
                // while another input is still awaiting its first response.
                // Keep per-input latency based on the first response only.
                presented_ns.extend(
                    [
                        sample["projected_presented_ns"].as_u64(),
                        sample["presented_ns"].as_u64(),
                    ]
                    .into_iter()
                    .flatten(),
                );
                latencies.push(elapsed as f64 / 1e6);
            } else {
                invalid_clocks += 1;
            }
        }
        // Once every input has a first response, delayed acknowledgments
        // cannot create a new gap in this input-response observation window.
        presented_ns.retain(|timestamp| *timestamp <= last_first_response);
        presented_ns.sort_unstable();
        presented_ns.dedup();
        let gaps_ms: Vec<_> = presented_ns
            .windows(2)
            .map(|pair| (pair[1] - pair[0]) as f64 / 1e6)
            .collect();
        latencies.sort_by(f64::total_cmp);
        let p95_response_ms = latencies
            .get((latencies.len() * 95).div_ceil(100).saturating_sub(1))
            .copied();
        Self {
            measurement: "first confirmed response latency; all confirmed response presentations through the final first response",
            received_receipts: samples.len(),
            invalid_clocks,
            max_response_ms: latencies.last().copied(),
            max_response_gap_ms: gaps_ms.iter().copied().max_by(f64::total_cmp),
            p95_response_ms,
            presented_ns,
            gaps_ms,
            first_visible_latency_ms: latencies,
        }
    }

    pub(super) fn meets_response_budget(&self) -> bool {
        self.invalid_clocks == 0
            && self
                .p95_response_ms
                .is_some_and(|value| value <= P95_RESPONSE_MS)
            && self
                .max_response_ms
                .is_some_and(|value| value <= MAX_RESPONSE_MS)
            && self
                .max_response_gap_ms
                .is_some_and(|value| value <= MAX_RESPONSE_GAP_MS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn native_latency_report_uses_first_visible_response_and_distinct_presentations() {
        let samples = [
            json!({"received_ns": 0, "projected_presented_ns": 8_000_000, "presented_ns": 500_000_000}),
            json!({"received_ns": 4_000_000, "projected_presented_ns": 8_000_000, "presented_ns": 500_000_000}),
            json!({"received_ns": 8_000_000, "presented_ns": 16_000_000}),
        ];
        let report = Report::from_samples(&samples);
        assert_eq!(report.presented_ns, [8_000_000, 16_000_000]);
        assert_eq!(report.gaps_ms, [8.0]);
        assert_eq!(report.first_visible_latency_ms, [4.0, 8.0, 8.0]);
        assert!(report.meets_response_budget());
    }

    #[test]
    fn native_latency_gap_includes_confirmed_catchup_while_inputs_await_response() {
        let report = Report::from_samples(&[
            json!({"received_ns": 0, "projected_presented_ns": 10_000_000, "presented_ns": 60_000_000}),
            json!({"received_ns": 30_000_000, "projected_presented_ns": 40_000_000, "presented_ns": 80_000_000}),
            json!({"received_ns": 35_000_000, "presented_ns": 100_000_000}),
            // Confirmation after every first response is outside the measured
            // response interval, as in the existing late-acknowledgment test.
            json!({"received_ns": 30_000_000, "projected_presented_ns": 40_000_000, "presented_ns": 500_000_000}),
        ]);
        assert_eq!(report.first_visible_latency_ms, [10.0, 10.0, 10.0, 65.0]);
        assert_eq!(
            report.presented_ns,
            [10_000_000, 40_000_000, 60_000_000, 80_000_000, 100_000_000]
        );
        assert_eq!(report.max_response_gap_ms, Some(30.0));
        // Correcting cadence does not erase the slow input's latency.
        assert_eq!(report.max_response_ms, Some(65.0));
        assert!(!report.meets_response_budget());
    }

    #[test]
    fn native_latency_budget_rejects_stalls_even_when_every_input_completes() {
        let samples: Vec<_> = (0..480u64)
            .map(|index| {
                json!({
                    "received_ns": index * 8_333_333,
                    "presented_ns": index * 8_333_333 + 5_000_000_000,
                })
            })
            .collect();
        let report = Report::from_samples(&samples);
        assert_eq!(report.received_receipts, 480);
        assert_eq!(report.p95_response_ms, Some(5000.0));
        assert!(!report.meets_response_budget());
    }

    #[test]
    fn native_latency_budget_rejects_missing_clocks_and_long_response_gaps() {
        assert!(!Report::from_samples(&[]).meets_response_budget());
        assert!(
            !Report::from_samples(&[json!({"received_ns": 2, "presented_ns": 1})])
                .meets_response_budget()
        );
        assert!(!Report::from_samples(&[json!({"presented_ns": 1})]).meets_response_budget());
        let report = Report::from_samples(&[
            json!({"received_ns": 0, "presented_ns": 1}),
            json!({"received_ns": 100_000_000, "presented_ns": 100_000_001}),
        ]);
        assert_eq!(report.max_response_ms, Some(0.000001));
        assert!(!report.meets_response_budget());
    }
}
