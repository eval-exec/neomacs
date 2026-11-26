use super::model;

#[test]
fn retire_ends_views_then_returns_ticket() {
    model::positive(model::Case::LoanRetire);
}

#[test]
fn negative_reopen_with_retained_loan() {
    model::negative(
        model::Case::LoanRetire,
        model::Edge::OmitRetainedCheck,
        "REOPEN_WITH_RETAINED_LOAN",
    );
}
