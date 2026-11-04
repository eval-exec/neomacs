//! Plain data owned by the evaluator layer.
//!
//! Stage-4 W1 (E2): records the display layer reads downward.  They name only
//! value/edit vocabulary, so the evaluator can hold them without naming a
//! display-crate type.

pub mod dynamic_module_registry;
pub mod face;
pub mod process_spawn;
pub mod process_tls;
pub mod redisplay;
pub mod window_edge_drag;
pub mod window_state;
pub mod xwidget;
