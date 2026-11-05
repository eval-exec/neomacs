mod frame_or_buffer_changed;

#[doc(hidden)]
pub use frame_or_buffer_changed::builtin_frame_or_buffer_changed_p;

pub mod pure;

#[cfg(test)]
#[path = "tests/dispnew_test.rs"]
mod tests;
