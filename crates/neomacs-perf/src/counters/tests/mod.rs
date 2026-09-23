use crate::{MetricName, MetricUnit, parse_perf_stat_csv};

#[test]
fn perf_stat_csv_becomes_typed_canonical_measurements() {
    let measurements = parse_perf_stat_csv(
        "1234,,cycles:u,100000,100.00\n\
         2345,,instructions:u,100000,100.00\n\
         12,,page-faults,100000,100.00\n\
         7,,branch-misses:u,100000,100.00\n\
         9,,cache-misses:u,100000,100.00\n",
    )
    .expect("parse supported perf stat output");

    assert_eq!(measurements.len(), 5);
    assert_eq!(measurements[0].name, MetricName::CpuCycles);
    assert_eq!(measurements[0].value, 1234.0);
    assert_eq!(measurements[0].unit, MetricUnit::Count);
    assert_eq!(measurements[1].name, MetricName::Instructions);
    assert_eq!(measurements[2].name, MetricName::PageFaults);
    assert_eq!(measurements[3].name, MetricName::BranchMisses);
    assert_eq!(measurements[4].name, MetricName::CacheMisses);
}

#[test]
fn requested_but_unsupported_counters_reject_the_collection() {
    let error = parse_perf_stat_csv("<not supported>,,cycles:u,0,0.00\n")
        .expect_err("missing hardware counters are not valid zeroes");
    assert!(error.contains("cycles"));
    assert!(error.contains("not supported"));
}

#[test]
fn hybrid_cpu_rows_use_the_counted_pmu_and_ignore_its_disabled_sibling() {
    let measurements = parse_perf_stat_csv(
        "<not counted>,,cpu_atom/cycles/u,0,0.00\n\
         19269453,,cpu_core/cycles/u,4295962,100.00\n",
    )
    .expect("one hybrid PMU supplied the requested logical event");
    assert_eq!(measurements.len(), 1);
    assert_eq!(measurements[0].name, MetricName::CpuCycles);
    assert_eq!(measurements[0].value, 19_269_453.0);
}
