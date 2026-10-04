use super::*;

#[test]
fn numeric_policy_counts_and_still_restore_nested_error_and_panic_scopes() {
    let prior = forced();
    let prior_counts = counts();
    let prior_still = super::super::STILL_OVERRIDE.with(Cell::get);
    {
        let _outer = Guard::set(false);
        note_legacy_plan();
        let outer_counts = counts();
        let returned_error: Result<(), ()> = (|| {
            let _inner = Guard::set(true);
            note_plan_insertion();
            assert_eq!(forced(), Some(true));
            Err(())
        })();
        assert!(returned_error.is_err());
        assert_eq!(counts(), outer_counts);
        assert_eq!(forced(), Some(false));
        let panic = std::panic::catch_unwind(|| {
            let _inner = Guard::set(true);
            note_legacy_install();
            panic!("numeric scope unwind control");
        });
        assert!(panic.is_err());
        assert_eq!(counts(), outer_counts);
        assert_eq!(super::super::STILL_OVERRIDE.with(Cell::get), Some(false));
    }
    assert_eq!(forced(), prior);
    assert_eq!(counts(), prior_counts);
    assert_eq!(super::super::STILL_OVERRIDE.with(Cell::get), prior_still);
}
