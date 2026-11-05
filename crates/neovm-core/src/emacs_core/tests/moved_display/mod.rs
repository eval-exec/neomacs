//! Re-homed display library tests that need system/core or core boot.
//! Each file moves WHOLE tests plus their private helpers; counts reconcile
//! display -N == core +N. See DISPLAY-TESTS-oracleb.tsv.

#[cfg(test)]
#[path = "font_face_boot_test.rs"]
mod font_face_boot_test;

#[cfg(test)]
#[path = "tsb_boundary_numbers.rs"]
mod tsb_boundary_numbers;

#[cfg(test)]
#[path = "window_system_preload_test.rs"]
mod window_system_preload_test;

#[cfg(test)]
#[path = "text_measurement_invisible_test.rs"]
mod text_measurement_invisible_test;

#[cfg(test)]
#[path = "tsb_face_font_validation.rs"]
mod tsb_face_font_validation;

#[cfg(test)]
#[path = "tsb_face_height.rs"]
mod tsb_face_height;

#[cfg(test)]
#[path = "dispnew_test.rs"]
mod dispnew_test;

#[cfg(test)]
#[path = "idle_redraw_test.rs"]
mod idle_redraw_test;

#[cfg(test)]
#[path = "cycling_test.rs"]
mod cycling_test;

#[cfg(test)]
#[path = "selection_test.rs"]
mod selection_test;

#[cfg(test)]
#[path = "xwidget_runtime_test.rs"]
mod xwidget_runtime_test;

#[cfg(test)]
#[path = "core_hook_restore_test.rs"]
mod core_hook_restore_test;

#[cfg(test)]
#[path = "window_cmds_runtime_eval_test.rs"]
mod window_cmds_runtime_eval_test;

#[cfg(test)]
#[path = "xdisp_runtime_eval_test.rs"]
mod xdisp_runtime_eval_test;

#[cfg(test)]
#[path = "image_lisp_playback_test.rs"]
mod image_lisp_playback_test;
