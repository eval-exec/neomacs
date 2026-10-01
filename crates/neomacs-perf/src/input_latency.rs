//! Reports native confirmed input latency separately from redisplay throughput.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Deserialize)]
struct Sample {
    input: u64,
    kind: String,
    input_to_present_ns: Option<u64>,
    #[serde(default)]
    input_to_projected_present_ns: Option<u64>,
    evicted_inputs: u64,
}

#[derive(Serialize)]
struct Summary {
    samples: usize,
    p50_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
    max_ms: f64,
    over_budget: usize,
}

pub(crate) fn report(text: &str, budget_us: u64) -> Result<serde_json::Value, String> {
    let mut groups: BTreeMap<String, Vec<u64>> = BTreeMap::new();
    let mut projected_groups: BTreeMap<String, Vec<u64>> = BTreeMap::new();
    let mut first_response_groups: BTreeMap<String, Vec<u64>> = BTreeMap::new();
    let mut identities = BTreeSet::new();
    for (index, line) in text.lines().enumerate() {
        let sample: Sample =
            serde_json::from_str(line).map_err(|error| format!("line {}: {error}", index + 1))?;
        if !identities.insert(sample.input) {
            return Err(format!(
                "duplicate input {}: use one editor session per report",
                sample.input
            ));
        }
        if sample.evicted_inputs != 0 {
            return Err("input tracking overflowed; latency distribution is incomplete".into());
        }
        let latency = sample
            .input_to_present_ns
            .ok_or("input and presentation timestamps are not comparable")?;
        if let Some(projected) = sample.input_to_projected_present_ns {
            projected_groups
                .entry(sample.kind.clone())
                .or_default()
                .push(projected);
        }
        first_response_groups
            .entry(sample.kind.clone())
            .or_default()
            .push(
                sample
                    .input_to_projected_present_ns
                    .map_or(latency, |projected| projected.min(latency)),
            );
        groups.entry(sample.kind).or_default().push(latency);
    }
    if groups.is_empty() {
        return Err("no compositor-confirmed input samples".into());
    }
    let summarize = |groups: BTreeMap<String, Vec<u64>>| -> BTreeMap<String, Summary> {
        groups
            .into_iter()
            .map(|(kind, mut values)| {
                values.sort_unstable();
                let quantile = |percent: usize| {
                    values[(values.len() * percent).div_ceil(100).saturating_sub(1)] as f64
                        / 1_000_000.0
                };
                let summary = Summary {
                    samples: values.len(),
                    p50_ms: quantile(50),
                    p95_ms: quantile(95),
                    p99_ms: quantile(99),
                    max_ms: *values.last().unwrap() as f64 / 1_000_000.0,
                    over_budget: values
                        .iter()
                        .filter(|&&value| value > budget_us.saturating_mul(1_000))
                        .count(),
                };
                (kind, summary)
            })
            .collect()
    };
    let summaries = summarize(groups);
    let projected = summarize(projected_groups);
    let first_response = summarize(first_response_groups);
    Ok(
        serde_json::json!({ "measurement": "native-enqueue-to-compositor-presentation", "scope": "inputs with a confirmed viewport change", "budget_us": budget_us, "by_input_kind": summaries, "first_response_by_input_kind": first_response, "first_response_scope": "earliest confirmed projected or authoritative presentation for every input, including fallbacks", "projected_by_input_kind": projected, "projected_scope": "subset with exact native submission confirmation; authoritative samples include fallbacks" }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn report_separates_projected_subset_from_authoritative_latency() {
        let text = [
            r#"{"input":1,"kind":"precise","input_to_present_ns":100000000,"input_to_projected_present_ns":5000000,"evicted_inputs":0}"#,
            r#"{"input":2,"kind":"precise","input_to_present_ns":200000000,"evicted_inputs":0}"#,
        ].join("\n");
        let report = report(&text, 16667).unwrap();
        assert_eq!(report["by_input_kind"]["precise"]["samples"], 2);
        assert_eq!(report["projected_by_input_kind"]["precise"]["samples"], 1);
        assert_eq!(report["projected_by_input_kind"]["precise"]["p50_ms"], 5.0);
        assert_eq!(
            report["projected_by_input_kind"]["precise"]["over_budget"],
            0
        );
    }

    #[test]
    fn first_response_includes_fallbacks_and_uses_the_earliest_confirmation() {
        let text = [
            r#"{"input":1,"kind":"precise","input_to_present_ns":100000000,"input_to_projected_present_ns":5000000,"evicted_inputs":0}"#,
            r#"{"input":2,"kind":"precise","input_to_present_ns":200000000,"evicted_inputs":0}"#,
            r#"{"input":3,"kind":"precise","input_to_present_ns":4000000,"input_to_projected_present_ns":6000000,"evicted_inputs":0}"#,
        ].join("\n");
        let report = report(&text, 16667).unwrap();
        let first = &report["first_response_by_input_kind"]["precise"];
        assert_eq!(first["samples"], 3);
        assert_eq!(first["p50_ms"], 5.0);
        assert_eq!(first["p95_ms"], 200.0);
        assert_eq!(first["over_budget"], 1);
    }

    #[test]
    fn report_keeps_input_kinds_separate_and_counts_budget_misses() {
        let text = [
            r#"{"input":1,"kind":"wheel","input_to_present_ns":1000000,"evicted_inputs":0}"#,
            r#"{"input":2,"kind":"wheel","input_to_present_ns":30000000,"evicted_inputs":0}"#,
            r#"{"input":3,"kind":"page","input_to_present_ns":5000000,"evicted_inputs":0}"#,
        ]
        .join("\n");
        let result = report(&text, 16667).unwrap();
        assert_eq!(result["by_input_kind"]["wheel"]["samples"], 2);
        assert_eq!(result["by_input_kind"]["wheel"]["p95_ms"], 30.0);
        assert_eq!(result["by_input_kind"]["wheel"]["over_budget"], 1);
        assert_eq!(result["by_input_kind"]["page"]["over_budget"], 0);
    }
    #[test]
    fn report_rejects_unavailable_or_biased_measurements() {
        for input in [
            "",
            "broken",
            r#"{"input":1,"kind":"page","input_to_present_ns":null,"evicted_inputs":0}"#,
            r#"{"input":1,"kind":"page","input_to_present_ns":100,"evicted_inputs":1}"#,
        ] {
            assert!(report(input, 16667).is_err());
        }
        let sample = r#"{"input":1,"kind":"page","input_to_present_ns":100,"evicted_inputs":0}"#;
        assert!(report(&format!("{sample}\n{sample}"), 16667).is_err());
    }
}
