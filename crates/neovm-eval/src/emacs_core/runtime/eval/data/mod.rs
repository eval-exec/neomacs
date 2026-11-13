//! Plain data owned by the evaluator layer.
//!
//! Stage-4 W1 (E2): records the display layer reads downward.  They name only
//! value/edit vocabulary, so the evaluator can hold them without naming a
//! display-crate type.

pub mod abbrev;
pub mod command_loop;
pub mod display_spec;
pub mod dynamic_module_registry;
pub mod face;
pub mod face_registry;
pub mod file_coding;
pub mod file_name_expand;
#[doc(hidden)]
pub mod fontset_state;
pub mod frame_alpha;
pub mod layout_input;
pub mod minibuffer;
pub mod minibuffer_entry;
pub mod point_rows;
pub mod posn_object_extent;
pub mod process_spawn;
pub mod process_state;
pub mod process_tls;
pub mod redisplay;
pub mod register;
pub mod terminal_state;
pub mod window_edge_drag;
pub mod window_layout_query;
pub mod window_markers;
pub mod window_state;
pub mod word_wrap;
pub mod xdisp_column_spec;
pub mod xwidget;
