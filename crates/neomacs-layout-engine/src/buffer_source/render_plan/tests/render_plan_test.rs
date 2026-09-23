use super::*;
use crate::display_row::metrics::DisplayRowFallbackMetrics;
use neovm_core::buffer::{Buffer, BufferId};
use neovm_core::emacs_core::Value;
use neovm_core::face::FaceTable;

#[test]
fn default_face_plan_uses_buffer_default_face_remap() {
    let _runtime = neovm_core::emacs_core::Context::new();
    let table = FaceTable::new();
    let resolver = FaceResolver::new(&table, 0x000000, 0xFFFFFF, 14.0, None);
    let mut buffer = Buffer::new_standalone(BufferId(42), Value::string("*default-remap*"));
    buffer.set_buffer_local(
        "face-remapping-alist",
        Value::list(vec![Value::list(vec![
            Value::symbol("default"),
            Value::list(vec![
                Value::keyword("background"),
                Value::string("#000000"),
                Value::keyword("foreground"),
                Value::string("#ffffff"),
            ]),
            Value::symbol("default"),
        ])]),
    );

    let plan = BufferSourceDefaultFacePlan::new(
        &resolver,
        &buffer,
        &mut None,
        DisplayRowMeasurementMode::LogicalCells,
        DisplayRowFallbackMetrics::from_default_face_extents(8.0, 16.0, 12.0),
    );

    assert_eq!(plan.face().bg, 0x000000);
    assert_eq!(plan.face().fg, 0xFFFFFF);
}
