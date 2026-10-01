//! Scenario implementations: each module owns one workload family's
//! preparation, result schema, invariants, and measurements.

pub(crate) mod bounded_search;
pub(crate) mod builtin_call;
pub(crate) mod bytecode;
pub(crate) mod editor_workload;
pub(crate) mod elisp_benchmarks;
pub(crate) mod mx_tab;
pub(crate) mod org_journal_open;
pub(crate) mod rust_lsp;
pub(crate) mod scrolling;
pub(crate) mod search_shape;
pub(crate) mod sustained_native_video;
pub(crate) mod vm_loop;

/// Omitted fields are legacy; a present field must be an unsigned integer.
/// In particular, six explicit nulls must not look like a legacy record.
fn deserialize_gc_boundary<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    <u64 as serde::Deserialize>::deserialize(deserializer).map(Some)
}

/// Legacy results omit every boundary. New results report complete, monotonic
/// count/time triples; a malformed delta cannot masquerade as no collection.
fn validate_gc_window(fields: [Option<u64>; 6]) -> Result<(), String> {
    if fields.iter().all(Option::is_none) {
        return Ok(());
    }
    let [
        Some(count_start),
        Some(count_end),
        Some(count_delta),
        Some(time_start),
        Some(time_end),
        Some(time_delta),
    ] = fields
    else {
        return Err("incomplete edit-loop GC boundaries".to_string());
    };
    if count_end.checked_sub(count_start) != Some(count_delta)
        || time_end.checked_sub(time_start) != Some(time_delta)
    {
        return Err("inconsistent edit-loop GC boundaries".to_string());
    }
    Ok(())
}

/// Preserve zero deltas and omit the whole optional series for legacy results.
fn append_gc_window_measurements(
    measurements: &mut Vec<crate::Measurement>,
    fields: [Option<u64>; 6],
) {
    use crate::MetricName;
    let names = [
        MetricName::GcsDoneStart,
        MetricName::GcsDoneEnd,
        MetricName::GcsDoneDelta,
        MetricName::GcElapsedStart,
        MetricName::GcElapsedEnd,
        MetricName::GcElapsedDelta,
    ];
    for (name, value) in names.into_iter().zip(fields) {
        if let Some(value) = value {
            measurements.push(crate::Measurement {
                name,
                value: value as f64,
                unit: name.canonical_unit(),
            });
        }
    }
}

#[cfg(test)]
#[path = "tests/gc_boundary_test.rs"]
mod gc_window_tests;
