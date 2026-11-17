# SYSTEM — per-item class table for the wave-2 TEST-SUPPORT-VIS rows (B15-1)

Companion to `TEST-SUPPORT-VIS-SYSTEM.tsv`.  Reviews the wave-2 A' rows against
the three-way rule (lead #6537 / PEER 5812):

- **rule 1** — production code reaches the item; evidence is the production
  caller (`file:line`).
- **rule 2** — no production caller.  The item is a test-support seam that
  drives a production function; evidence names that production function.
- **rule 3** — no production caller and the item duplicates a production
  implementation, so tests on it are false coverage.  Every class-3 item is
  deleted here and its tests are retargeted to the production path.

Paths are relative to `crates/neovm-system/src/emacs_core/system/` unless
stated.  Line numbers are as of the B15-1 commit.

## fileio adapters (rule 2: thin test-support drivers of the production cores)

These were &str copies in the wave-2 triage ("A' over a pub(crate) production
item").  They were neither: production does not call them.  Each body is now a
one-line conversion around the production core the builtin calls; the copied
logic is gone.

| item | kind | class | evidence |
|---|---|---|---|
| `fileio::delete_file` | fn | 2 | drives `unlink_file_path` (fileio/mod.rs:876), the unlink core `builtin_delete_file_internal` (fileio/mod.rs:3865) reaches via `delete_file_compat_path` |
| `fileio::directory_name_p` | fn | 2 | drives `builtin_directory_name_p` (fileio/mod.rs:2184), the production subr |
| `fileio::file_directory_p` | fn | 2 | drives `file_directory_path` (fileio/mod.rs:2645), called by `builtin_file_directory_p` (fileio/mod.rs:3352) |
| `fileio::file_name_as_directory` | fn | 2 | drives `lisp_file_name_as_directory` (fileio/mod.rs:1245), called by `builtin_file_name_as_directory` (fileio/mod.rs:2121) |
| `fileio::file_name_concat` | fn | 2 | drives `file_name_concat_lisp` (fileio/mod.rs:1144), called by `builtin_file_name_concat` (fileio/mod.rs:2143) |
| `fileio::file_name_directory` | fn | 2 | drives `lisp_file_name_directory` (fileio/mod.rs:1165), called by `builtin_file_name_directory` (fileio/mod.rs:2096) |
| `fileio::file_name_nondirectory` | fn | 2 | drives `lisp_file_name_nondirectory` (fileio/mod.rs:1226), called by `builtin_file_name_nondirectory` (fileio/mod.rs:2110) |
| `fileio::file_readable_p` | fn | 2 | drives `file_readable_path` (fileio/mod.rs:2531), called by `builtin_file_readable_p` (fileio/mod.rs:2989) |
| `fileio::file_regular_p` | fn | 2 | drives `file_regular_path` (fileio/mod.rs:2649), called by `builtin_file_regular_p` (fileio/mod.rs:3370) |
| `fileio::file_symlink_p` | fn | 2 | drives `file_symlink_target_lisp` (fileio/mod.rs:421), called by `builtin_file_symlink_p` (fileio/mod.rs:3392) |
| `fileio::rename_file` | fn | 2 | drives `rename_path_with_cross_device_fallback` (fileio/mod.rs:904), used by `builtin_rename_file` (fileio/mod.rs:4011) |
| `fileio::write_string_to_file` | fn | 2 | drives `write_bytes_to_file_with_mode` (fileio/mod.rs:585), the open/write core `write-region` calls (fileio/mod.rs:7178); only the coding step is skipped |

## Deleted class-3 items

| item | kind | class | evidence / retarget |
|---|---|---|---|
| `fileio::read_file_contents` | fn | 3 | was a raw `fs::read_to_string` model with no production caller (production reads through `insert-file-contents`).  Deleted; fixtures use `std::fs::read_to_string`, and `test_read_write_file` now writes with `write-region` and reads with `insert-file-contents`. |
| `fileio::make_directory` | fn | 3 | duplicated `create_dir*` without `builtin_make_directory_internal`'s umask policy and had no production caller.  Deleted; its only caller now creates the fixture with `std::fs::create_dir_all` and asserts the production `file-directory-p` predicate. |
| `ProcessOutputDestination` | struct | 1 | not a class-3 item: it is the parameter type of 8 public extension-trait methods (trait declaration and impl each warn), so B13-1 widened it back to `pub` and removed its ledger row.  Recorded here because it was one of the wave-2 rows. |

## rule-1 items (production callers)

| item | kind | class | evidence |
|---|---|---|---|
| `dynamic_module::emacs_env_private` | struct | 1 | production env layout: `private_members: *mut emacs_env_private` (dynamic_module/mod.rs:77), dereferenced by the FFI entry points at :453-563 |
| `dynamic_module::emacs_value_storage` | struct | 1 | production value storage: field of `emacs_env_private` (:302), `impl emacs_value_storage` (:381), `allocate_emacs_value` (:395) |
| `dynamic_module::emacs_funcall_exit` | enum | 1 | production FFI callback result type (:87, :94) and `pending_non_local_exit` field (:299) |
| `process::sys::child_wait::ChildWait` | enum | 1 | return type of `poll_child_status` (process/sys/child_wait.rs:35), the production reaper |
| `process::sys::child_wait::decode_wait_status` | fn | 1 | called by `poll_child_status` at process/sys/child_wait.rs:46 |
| `process::sys::fd::set_fd_nonblocking` | fn | 1 | process/mod.rs:182; process/types.rs:4421, :4561, :5341 |
| `process::sys::net::af_inet6` | fn | 1 | process/bootstrap_vars.rs:652 (`Self::Ipv6 => sys::net::af_inet6()`) |
| `process::sys::process_status::process_is_alive` | fn | 1 | filelock/mod.rs:317 |
| `system::sqlite::enabled::available_p` | fn | 1 | registered as the production subr at sqlite/subrs.rs:106 |
| `system::sqlite::enabled::is_sqlite_object` | fn | 1 | registered as the production subr at sqlite/subrs.rs:101 |

## rule-2 self-subject items (measurement table, no production reader)

`cus_start_platform_vars` is the executable form of a GNU measurement: it is
built only under `cfg(test)`/`test-support`, nothing in production reads it, and
its unit test (`platform/cus_start_platform_vars/tests/cus_start_platform_vars_test.rs`)
is the drift guard over the measured rows.  Production declarations live at the
`syms_of_*` counterparts named in each row's `gnu`/site columns and are reached
by `var_docs::SnarfedVariable` (GNU `src/doc.c:606-613`).

| item | kind | class | evidence |
|---|---|---|---|
| `system::platform::cus_start_platform_vars::CUS_START_PLATFORM_VARIABLES` | static | 2 | self-subject; table at platform/cus_start_platform_vars/mod.rs:173, drift guard at :302 |
| `system::platform::cus_start_platform_vars::CusStartPlatformVariable` | struct | 2 | self-subject; row type of the same table |
| `system::platform::cus_start_platform_vars::GnuBinding` | enum | 2 | self-subject; measured GNU binding answer of the same table |

## use re-exports (class follows the item they re-export)

| item | kind | class | evidence |
|---|---|---|---|
| `process::sys::ChildWait` | use | 1 | re-export in process/sys/mod.rs of the rule-1 type; caller child_wait.rs:35 |
| `process::sys::decode_wait_status` | use | 1 | re-export at process/sys/mod.rs:111 of the rule-1 fn; caller child_wait.rs:46 |
| `process::sys::set_fd_nonblocking` | use | 1 | re-export of the rule-1 fn; callers process/mod.rs:182 and process/types.rs:4421, :4561, :5341 |
| `process::sys::process_is_alive` | use | 1 | re-export of the rule-1 fn; caller filelock/mod.rs:317 |
| `system::sqlite::available_p` | use | 1 | re-export at sqlite/mod.rs:1079 of `enabled::available_p`; production subr sqlite/subrs.rs:106 |
| `system::sqlite::is_sqlite_object` | use | 1 | re-export at sqlite/mod.rs:1079 of `enabled::is_sqlite_object`; production subr sqlite/subrs.rs:101 |

## Ledger effect

- Rows: 386 -> 384 (the two deleted class-3 helpers).  The ceiling stays 386;
  it is never taken below the row count.
- `test_read_write_file` now exercises the production `write-region` /
  `insert-file-contents` pair instead of the deleted read/write helpers.
