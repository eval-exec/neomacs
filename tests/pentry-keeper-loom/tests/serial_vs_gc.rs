use super::model;

#[test]
fn serial_gc_uses_current_roots_and_blocks_keeper() {
    model::positive(model::Case::SerialVsGc);
}

#[test]
fn negative_keeper_collects_while_ticket_out() {
    model::negative(
        model::Case::SerialVsGc,
        model::Edge::OmitOverlapRefusal,
        "KEEPER_COLLECTS_WHILE_TICKET_OUT",
    );
}
