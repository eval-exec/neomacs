use super::model;

#[test]
fn admission_ticket_only_after_all_stopped() {
    model::positive(model::Case::Admission);
}

#[test]
fn negative_admission_while_running() {
    model::negative(
        model::Case::Admission,
        model::Edge::OmitAllStopped,
        "ADMISSION_WHILE_RUNNING",
    );
}
