//! cfg(test)-only core wrapper for the historical `emacs_core::eval` module
//! path: the TA2-fence test re-homed out of eval's test tree (S-2).

#[cfg(test)]
#[path = "../../core_tests/emacs_core/runtime/eval/tests/minimal_vm_buffer_storage_test.rs"]
mod minimal_vm_buffer_storage_test;
