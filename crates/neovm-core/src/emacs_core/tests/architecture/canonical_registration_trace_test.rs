//! T6 literal equality (1/3): the canonical PRODUCTION registration trace.
//!
//! Peer T6 structure ruling, option (a) (LEAD 04:5xZ): three literal
//! equalities against the one shared, drift-checked builder
//! [`neovm_eval::test_utils::canonical_host_active_trace_for_test`]:
//! (1) core production (this test), (2) eval's `cfg(test)` unit runtime
//! (eval's own pin — the unit runtime cannot be constructed from another
//! crate, and a test-support hook into it is explicitly refused), and
//! (3) display's `cfg(test)` unit runtime (display's own test). Equality
//! is transitive through the builder; all three run in the same P2
//! invocation, alongside the HOST_CFG agreement test.
//!
//! The trace records the single registration choke point
//! (`Context::register_subr`) plus the `EventSymbolProperties` marker at
//! its canonical step, so this comparison pins names, min/max arity and
//! order against the drift-checked plan.

#[test]
fn canonical_production_registration_trace_matches_the_plan() {
    // The T6 structure ruling also requires each test to assert its
    // crate's host-cfg view equals the shared classification. Core emits
    // both host cfgs itself through the shared build-script probe.
    use neovm_eval::host_cfg::HostCfgClass;
    assert_eq!(
        cfg!(neomacs_have_dbus),
        HostCfgClass::Dbus.is_active(),
        "core's neomacs_have_dbus view diverges from HostCfgClass"
    );
    assert_eq!(
        cfg!(neomacs_have_lcms2),
        HostCfgClass::Lcms2.is_active(),
        "core's neomacs_have_lcms2 view diverges from HostCfgClass"
    );
    let expected = neovm_eval::test_utils::canonical_host_active_trace_for_test();
    neovm_eval::emacs_core::subr::reset_registered_subr_trace_for_test();
    let context = crate::test_utils::new_context();
    let trace = neovm_eval::emacs_core::subr::take_registered_subr_trace_for_test();
    drop(context);
    assert_eq!(
        trace, expected,
        "the canonical production registration trace (name/min/max/order plus the \
         event-properties marker) diverges from the shared host-active plan sequence"
    );
}

/// W-REG union gate: the installed production trace is the union of the
/// per-crate stage tables in core's one registration chain. Compare it with a
/// CHECKED-IN FROZEN pre-move fixture, position by position (set AND order);
/// the mutable generated plan must never be the oracle here, or the test
/// would move with the code it is meant to pin.
#[test]
fn canonical_production_registration_trace_matches_the_frozen_pre_move_plan() {
    use neovm_eval::emacs_core::subr::SubrRegistration;
    use neovm_eval::host_cfg::HostCfgClass;

    const NAMES: &str = include_str!("fixtures/wreg/W-REG-PRE-MOVE-NAMES.tsv");
    const PLAN: &str = include_str!("fixtures/wreg/W-REG-PRE-MOVE-PLAN.tsv");

    struct Name {
        name: &'static str,
        min: u16,
        max: Option<u16>,
        cfg: &'static str,
    }
    struct Step {
        kind: &'static str,
        id: &'static str,
        first: usize,
        count: usize,
        marker: &'static str,
    }

    let names: Vec<Name> = NAMES
        .lines()
        .skip(1)
        .filter(|line| !line.is_empty())
        .map(|line| {
            let fields: Vec<&str> = line.split('\t').collect();
            assert_eq!(fields.len(), 5, "malformed frozen name row: {line}");
            Name {
                name: fields[0],
                min: fields[1].parse().expect("frozen name min"),
                max: if fields[2] == "-" {
                    None
                } else {
                    Some(fields[2].parse().expect("frozen name max"))
                },
                cfg: fields[4],
            }
        })
        .collect();
    let steps: Vec<Step> = PLAN
        .lines()
        .skip(1)
        .filter(|line| !line.is_empty())
        .map(|line| {
            let fields: Vec<&str> = line.split('\t').collect();
            assert_eq!(fields.len(), 7, "malformed frozen plan row: {line}");
            Step {
                kind: fields[1],
                id: fields[2],
                first: fields[4].parse().expect("frozen step first"),
                count: fields[5].parse().expect("frozen step count"),
                marker: fields[6],
            }
        })
        .collect();

    let mut expected = Vec::new();
    for step in &steps {
        if step.kind == "eval_call" && step.id == "eval.event_properties" {
            assert_eq!(step.count, 0, "event-properties must register no subr");
            assert_eq!(step.marker, "init_event_symbol_properties");
            expected.push(SubrRegistration::EventSymbolProperties);
            continue;
        }
        for row in &names[step.first - 1..step.first - 1 + step.count] {
            let class = HostCfgClass::from_label(row.cfg)
                .unwrap_or_else(|| panic!("unknown frozen cfg label {:?}", row.cfg));
            if class.is_active() {
                expected.push(SubrRegistration::Subr {
                    name: row.name,
                    min: row.min,
                    max: row.max,
                });
            }
        }
    }

    neovm_eval::emacs_core::subr::reset_registered_subr_trace_for_test();
    let context = crate::test_utils::new_context();
    let trace = neovm_eval::emacs_core::subr::take_registered_subr_trace_for_test();
    drop(context);
    assert_eq!(
        trace, expected,
        "the installed registration union (per-crate stage tables in core's chain) \
         diverges from the frozen pre-move plan in set or order"
    );
}
