//! Process configuration parsing; no runtime values or mutator dependencies.

use super::*;

#[test]
fn opt_passes_default_and_empty_lists_run_no_passes() {
    for value in [None, Some(""), Some("none"), Some("unknown")] {
        assert_eq!(OptPasses::parse(value), OptPasses::default());
    }
}

#[test]
fn opt_passes_comma_list_and_negative_entries_are_independent() {
    assert_eq!(
        OptPasses::parse(Some(" fold, bool ,reps,gvn,-bool,range,licm,sink,-licm ")),
        OptPasses {
            bool_rep: false,
            licm: false,
            ..OptPasses::ALL
        }
    );
    assert_eq!(
        OptPasses::parse(Some("all,-gvn")),
        OptPasses {
            gvn: false,
            ..OptPasses::ALL
        }
    );
    assert_eq!(OptPasses::parse(Some("all,none")), OptPasses::default());
    assert_eq!(OptPasses::parse(Some("all,-all")), OptPasses::default());
}
