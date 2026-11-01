use super::*;

#[test]
fn recorded_fix6_rich_cohorts_obey_their_capacity_contract() {
    // Raw p95/max/gap/median-gap metrics, ms. FIX6 jobs 124828139305 and
    // 124832033189, five cells each. No projected or filtered comparator.
    let fast = [
        (220.691759, 248.691038, 99.801808, 16.960020),
        (370.750850, 526.490716, 165.840737, 19.488403),
        (208.174663, 240.930720, 46.514197, 16.543294),
        (254.519069, 294.902030, 163.888299, 16.795022),
        (172.134548, 226.756836, 68.508221, 17.038518),
    ];
    let reduced = [
        (912.431716, 1114.677304, 103.874391, 21.385282),
        (1055.796299, 1520.183495, 66.594777, 20.705481),
        (1129.390452, 1470.911971, 64.837386, 20.695903),
        (903.569664, 1092.910507, 54.846084, 20.988617),
        (1005.467518, 1176.099389, 70.568371, 21.031489),
    ];
    for (rows, budget) in [
        (fast, ResponseBudget::software_fast()),
        (reduced, ResponseBudget::software_reduced_capacity()),
    ] {
        for (p95, maximum, gap, median) in rows {
            let report = Report {
                p95_response_ms: Some(p95),
                max_response_ms: Some(maximum),
                max_response_gap_ms: Some(gap),
                median_response_gap_ms: Some(median),
                ..Report::from_samples(&[])
            };
            assert_eq!(
                report.assess(&budget),
                Assessment::WithinBudget,
                "{report:?}"
            );
            // Demonstrate that replacing calibration with the old universal
            // limit fails on the actual recorded cases, not a fabricated row.
            assert_eq!(
                report.assess(&ResponseBudget::original()),
                Assessment::ExceededBudget
            );
        }
    }
}

#[test]
fn calibrated_software_contract_still_rejects_latency_cadence_gaps_and_invalid_clocks() {
    let report = || Report {
        p95_response_ms: Some(100.0),
        max_response_ms: Some(200.0),
        max_response_gap_ms: Some(30.0),
        median_response_gap_ms: Some(16.0),
        ..Report::from_samples(&[])
    };
    for budget in [
        ResponseBudget::software_fast(),
        ResponseBudget::software_reduced_capacity(),
    ] {
        assert_eq!(report().assess(&budget), Assessment::WithinBudget);
        let stalled = Report {
            p95_response_ms: Some(5000.0),
            ..report()
        };
        assert_eq!(stalled.assess(&budget), Assessment::ExceededBudget);
        let stalled = Report {
            max_response_ms: Some(5000.0),
            ..report()
        };
        assert_eq!(stalled.assess(&budget), Assessment::ExceededBudget);
        let stalled = Report {
            max_response_gap_ms: Some(201.0),
            ..report()
        };
        assert_eq!(stalled.assess(&budget), Assessment::ExceededBudget);
        let stalled = Report {
            median_response_gap_ms: Some(26.0),
            ..report()
        };
        assert_eq!(stalled.assess(&budget), Assessment::ExceededBudget);
        let invalid = Report {
            invalid_clocks: 1,
            ..report()
        };
        assert_eq!(invalid.assess(&budget), Assessment::ExceededBudget);
        assert_eq!(
            Report::from_samples(&[]).assess(&budget),
            Assessment::ExceededBudget
        );
    }
}
