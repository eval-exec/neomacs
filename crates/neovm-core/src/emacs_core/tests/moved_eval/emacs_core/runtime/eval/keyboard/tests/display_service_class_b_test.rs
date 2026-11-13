#[test]
fn gui_service_keeps_deadline_and_pending_input_order() {
    neovm_eval::test_support::authority::run_gui_service_keeps_deadline_and_pending_input_order_for_test(
        crate::test_utils::new_context,
    );
}
