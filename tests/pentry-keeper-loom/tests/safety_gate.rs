//! MODEL gate for DESIGN §9 E2. Product code is pinned by `seam`, not executed here.
#[path = "admission.rs"]
mod admission;
#[path = "loan_retire.rs"]
mod loan_retire;
#[path = "support/safety_model.rs"]
mod model;
#[path = "serial_vs_gc.rs"]
mod serial_vs_gc;
