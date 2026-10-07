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
