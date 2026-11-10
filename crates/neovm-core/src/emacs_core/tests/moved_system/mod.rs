//! Booted-`Context` tests re-homed from `neovm-system`
//! (`TEST-SUPPORT-DESIGN.md` section 1a, lead ruling 01:04Z option A).
//!
//! Bodies, test names and assertions are byte-identical to the origin
//! files; only `use` paths and module-path strings were rewritten.

#[path = "emacs_core/system/callproc/tests/raw_bytes_test.rs"]
mod callproc_tests_raw_bytes_test_rs;

#[path = "emacs_core/system/callproc/tests/read_coding_test.rs"]
mod callproc_tests_read_coding_test_rs;

#[path = "emacs_core/system/callproc/tests/working_dir_infile_test.rs"]
mod callproc_tests_working_dir_infile_test_rs;

#[cfg(neomacs_have_dbus)]
#[path = "emacs_core/system/dbusbind/tests/call_test.rs"]
mod dbusbind_tests_call_test_rs;

#[cfg(neomacs_have_dbus)]
#[path = "emacs_core/system/dbusbind/tests/connection_test.rs"]
mod dbusbind_tests_connection_test_rs;

#[cfg(neomacs_have_dbus)]
#[path = "emacs_core/system/dbusbind/tests/signal_test.rs"]
mod dbusbind_tests_signal_test_rs;

#[cfg(neomacs_have_dbus)]
#[path = "emacs_core/system/dbusbind/tests/surface_test.rs"]
mod dbusbind_tests_surface_test_rs;

#[path = "emacs_core/system/dynamic_module/tests/dynamic_module_test.rs"]
mod dynamic_module_tests_dynamic_module_test_rs;

#[path = "emacs_core/system/dynamic_module/tests/gc_tls_ownership_test.rs"]
mod dynamic_module_tests_gc_tls_ownership_test_rs;

#[cfg(target_os = "linux")]
#[path = "emacs_core/system/dynamic_module/tests/library_lifetime.rs"]
mod dynamic_module_tests_library_lifetime_rs;

#[path = "emacs_core/system/fileio/tests/fileio_test.rs"]
mod fileio_tests_fileio_test_rs;

#[path = "emacs_core/system/fileio/tests/fix8_test.rs"]
mod fileio_tests_fix8_test_rs;

#[path = "emacs_core/system/filelock/tests/filelock_test.rs"]
mod filelock_tests_filelock_test_rs;

#[path = "emacs_core/system/host_input/tests/host_input_test.rs"]
mod host_input_tests_host_input_test_rs;

#[path = "emacs_core/system/os_signal/tests/os_signal_test.rs"]
mod os_signal_tests_os_signal_test_rs;

#[path = "emacs_core/system/platform/c_features/tests/surface_test.rs"]
mod platform_c_features_tests_surface_test_rs;

#[path = "emacs_core/system/platform/w32/tests/w32_test.rs"]
mod platform_w32_tests_w32_test_rs;

#[path = "emacs_core/system/post_image_init/tests/post_image_init_test.rs"]
mod post_image_init_tests_post_image_init_test_rs;

#[path = "emacs_core/system/process/tests/darwin_attributes_test.rs"]
mod process_tests_darwin_attributes_test_rs;

#[path = "emacs_core/system/process/tests/deferred_reap_fixture_test.rs"]
mod process_tests_deferred_reap_fixture_test_rs;

#[path = "emacs_core/system/process/tests/gnu_local_service_oracle.rs"]
mod process_tests_gnu_local_service_oracle_rs;

#[path = "emacs_core/system/process/tests/gnu_pty_exit_oracle.rs"]
mod process_tests_gnu_pty_exit_oracle_rs;

#[cfg(unix)]
#[path = "emacs_core/system/process/tests/native_deferred_reap_fixture_test.rs"]
mod process_tests_native_deferred_reap_fixture_test_rs;

#[cfg(unix)]
#[path = "emacs_core/system/process/tests/output_fairness.rs"]
mod process_tests_output_fairness_rs;

#[cfg(unix)]
#[path = "emacs_core/system/process/tests/pipe_exit.rs"]
mod process_tests_pipe_exit_rs;

#[path = "emacs_core/system/process/tests/process_test.rs"]
mod process_tests_process_test_rs;

#[cfg(unix)]
#[path = "emacs_core/system/process/tests/pty_exit.rs"]
mod process_tests_pty_exit_rs;

#[path = "emacs_core/system/process/tests/raw_bytes_test.rs"]
mod process_tests_raw_bytes_test_rs;

#[path = "emacs_core/system/process/tests/signal_number.rs"]
mod process_tests_signal_number_rs;

#[path = "emacs_core/system/process/tests/timeout.rs"]
mod process_tests_timeout_rs;

#[path = "emacs_core/system/process/tests/green_select_rehome.rs"]
mod process_tests_green_select_rehome_rs;

#[cfg(not(feature = "sqlite"))]
#[path = "emacs_core/system/sqlite/tests/disabled_test.rs"]
mod sqlite_tests_disabled_test_rs;

#[cfg(feature = "sqlite")]
#[path = "emacs_core/system/sqlite/tests/enabled_test.rs"]
mod sqlite_tests_enabled_test_rs;

#[path = "emacs_core/system/timefns/tests/timefns_test.rs"]
mod timefns_tests_timefns_test_rs;

#[path = "emacs_core/system/timer/tests/timer_test.rs"]
mod timer_tests_timer_test_rs;

#[path = "emacs_core/system/tls/tests/runtime_test.rs"]
mod tls_tests_runtime_test_rs;

#[path = "emacs_core/system/wait/tests/wait_test.rs"]
mod wait_tests_wait_test_rs;

#[cfg(neomacs_have_dbus)]
#[path = "emacs_core/system/dbusbind/tests/private_bus.rs"]
pub(crate) mod private_bus;
