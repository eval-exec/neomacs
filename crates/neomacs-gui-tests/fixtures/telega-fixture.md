# Account-free Telega frontend fixture

Deterministic, offline integration coverage for the real Telega Lisp frontend
running in Neomacs on a harness-owned private display. It implements the Telega
subprocess boundary, not TDLib or MTProto.
Passing these tests does not establish the cause of the reported missing
avatars in a personal session.

## Seam and architecture

| Piece | Where | What is real |
| --- | --- | --- |
| Wire protocol | `src/telega_fixture/protocol.rs` | Telega's byte-length framed plist protocol: `send <N>\n<utf8 plist>\n` in, `event <N>\n<utf8 plist>\n` out, `N` = UTF-8 **bytes**, `:@extra` reply correlation preserved |
| Mock server | `src/telega_fixture/mock.rs` + `src/bin/neomacs-telega-fixture-mock.rs` | launched by Telega through its real `telega-server-command`; answers `setOption`/`setTdlibParameters`/`setNetworkType`/`setScopeNotificationSettings`/`getOption`/`loadChats`/`downloadFile`/`getBlockedMessageSenders`/`getSavedMessagesTags`/`getBasicGroup` (correlated `ok`/typed replies whenever `:@extra` is present), answers the `-h` version probe, rejects unmodeled requests with `NEOMACS_FIXTURE_UNSUPPORTED`, and rejects unknown option/file/group/control ids with `NEOMACS_FIXTURE_UNKNOWN_*` plus a `violation` log record |
| Scenario | `src/telega_fixture/scenario.rs` | 112 synthetic `chatTypeBasicGroup` chats, lossless generated PNG avatars, same-width positive int64 chat orders so Telega's string sort matches "Synthetic Group 01" first |
| Frontend scenario | `fixtures/telega-fixture-gui.el` | real pinned Telega loaded from the provisioned tree; real process filter, callback dispatch, root buffer, SVG avatar construction, redisplay and PageUp/PageDown scrolling |
| GUI tests | `tests/telega_fixture_gui.rs` | private Xvfb display, fresh HOME/XDG/TMPDIR, owned runtime dir, private process group |

The tests assert on the rendered frame snapshot (per-row glyphs and image
glyphs) **and** on private-display screenshots of the same checkpoint: every
fully visible synthetic row must start with an avatar image glyph and carry
enough pixels of the expected avatar color in its leading band.  Clipped
partial rows are excluded; window-start must advance across PageDown and
return to 1 across PageUp.

## Scope

In scope: Telega's subprocess protocol handling, request/response correlation,
update dispatch, chat-list rendering, avatar media construction/embedding,
redisplay of avatars, and paging in Neomacs.

Out of scope: native TDLib, MTProto, authentication, download semantics, and
anything about a real Telegram account. The mock does not contact Telegram
or another service. Package provisioning may fetch pinned sources before
the scenario starts. The private X11 display uses authenticated local TCP.

## Isolation

* Fresh `HOME`, `XDG_CONFIG_HOME/CACHE/DATA/STATE`, `TMPDIR` under
  `target/neomacs-gui-tests/tf-<name>-<pid>/`; Telega's directory, database,
  cache, and temp paths are set **before** Telega is loaded.
* Owned `XDG_RUNTIME_DIR` with mode 0700, published as
  `/proc/<test-pid>/fd/<fd>` (the existing neomacs-infra pattern) so long
  checkout paths cannot overflow `sockaddr_un`; the test canonicalizes the
  published path and asserts it is the owned directory.
* Private Xvfb display from `neomacs_infra::display::start_xvfb`; the tests
  assert `DISPLAY`/runtime from `/proc/<pid>/environ`.  `WAYLAND_SOCKET` is
  removed (absent, not empty), `DBUS_SESSION_BUS_ADDRESS`,
  `EMACSLOADPATH` and related loading overrides are removed.
  Xvfb uses a fixture-owned Xauthority cookie rather than the user's display
  credentials.
* Package mounting is explicit: the harness passes the provisioned package
  directories and the pinned `telega.el` path; the scenario adds exactly those
  to `load-path` and loads that file.  `package-user-dir` points at a
  fixture-owned empty directory and no archive/discovery path is consulted.
* **This is configuration isolation, not a sandbox.**  The fixture does not
  create a network namespace, seccomp filter, or mount namespace. Filesystem
  access and external network access are not denied by the operating system.
  Other host environment variables, tools, fonts and graphics libraries can
  still affect a run. The scenario asserts the paths it configured.
* Cleanup: the editor is spawned with `process_group(0)`.  Teardown writes a
  graceful quit request, waits, then signals exactly `kill(-pgid, ...)` for
  the recorded group (editor + mock it spawned) and never any process by
  name.  The private Xvfb is owned and killed by `DisplaySession`.

## Commands

```sh
# Provision the pinned Telega (+ visual-fill-column, transient) once; network
# access happens here only.
cargo run -p neomacs-infra --bin infra -- packages preflight telega@20261002.1709

# Protocol/scenario/mock unit tests.
cargo nextest run -p neomacs-gui-tests --lib telega

# Private GUI integration (needs target/release/neomacs).
cargo build --release -p neomacs --bin neomacs
cargo nextest run -p neomacs-gui-tests --test telega_fixture_gui

# Focused slices.
cargo nextest run -p neomacs-gui-tests --test telega_fixture_gui mock_answers_the_telega_server_version_probe
cargo nextest run -p neomacs-gui-tests --test telega_fixture_gui telega_frontend_renders_synthetic_chats_and_paged_avatars_on_a_private_display
```

Artifacts land in `target/neomacs-gui-tests/tf-<name>-<pid>/`:
`page-top.png`, `page-down-N.png`, `page-up-N.png` (private-display
screenshots), `frame-snapshot.json`, `checkpoint.json`, `mock-log.jsonl`,
`isolation.json`, `neomacs.stderr.log`.  A trial run of the paging slice
(`tf-render-paging-*`) produced screenshots with magenta avatar circles at
the leading edge of every visible synthetic row.

### GUI test inventory

| Test | Covers |
| --- | --- |
| `mock_answers_the_telega_server_version_probe` | `telega-server -h` version contract |
| `telega_frontend_renders_synthetic_chats_and_paged_avatars_on_a_private_display` | 112 rendered chats, per-row avatar pixels at the line beginning, ≥3 viewports, PageDown advance + bottom, PageUp return to window-start 1, Group 01 first, isolation + zero fixture drift |
| `telega_frontend_ingests_a_delayed_avatar_update_file` | no photo before delivery, explicit `downloadFile` request, `updateFile` ingestion into Telega's file table and cached avatar spec |
| `telega_frontend_repaints_a_delayed_avatar_after_update_file` | `updateFile` must replace initials with photo pixels without a test-side refresh |
| `telega_frontend_replaces_a_chat_photo_without_restarting` | `updateChatPhoto` swaps exactly the first row's photo to the replacement color while other rows keep theirs |
| `fixture_tears_down_its_owned_process_group_and_display` | graceful quit, recorded process group gone (`kill(-pgid, 0)` → ESRCH), authenticated display probe succeeds before teardown and fails afterward, mock recorded `eof`/`stopped` |
| `fixture_cleans_up_after_a_panicking_scenario` | unwinding teardown after a simulated failure still releases the recorded group and the private display |

## Separate retained-layout regression

The reduced editor test is independent of the Telega fixture:

```sh
cargo nextest run -p neomacs-layout-engine --lib \
  forced_window_update_rebuilds_a_mutated_buffer_image_spec
```

It mutates a buffer image spec in place, calls `(image-flush SPEC t)` and
`(force-window-update)`, and compares retained geometry with a fresh full
layout and synchronous queries. Before the production change, it failed
because the retained geometry stayed stale. It passes with the typed
`BodyRedisplayRevision` in the retained key and layout freshness tokens.
Presentation-only redisplay requests remain separate, preserving body reuse.

GNU's `src/window.c`, `Fforce_window_update` and
`window_loop(REDISPLAY_BUFFER_WINDOWS)`, define the all-window, window and
buffer scopes. The buffer walk excludes even active minibuffers, while an
explicit live minibuffer-window target is supported. A successful force also
requests mode-line updates.
The return-value contract is checked against live GNU Emacs by the cx409
oracle tests. The terminal test uses a mutable space display spec because
TTY frames cannot render avatars.

The revised Telega fixture's seven GUI tests also passed on the unchanged
release binary, before this production change. An earlier fixture revision
failed delayed repaint, but changes to initialization, protocol responses
and readiness preceded its passing run. That result is not evidence that
this layout change fixes the original Telega report. The reduced layout
regression supplies the failing test for the production change.

## Research and coverage boundary

Telega exposes its subprocess through
[`telega-server-command`](https://github.com/zevlg/telega.el/blob/a6abce419828fc63c7698c7d4951614e8d12d2ff/telega-customize.el).
The fixture models the framing and callback boundary in
[`telega-server.el`](https://github.com/zevlg/telega.el/blob/a6abce419828fc63c7698c7d4951614e8d12d2ff/telega-server.el).
This keeps the frontend real while replacing the account-dependent service.

[Telegram's test accounts](https://core.telegram.org/api/auth#test-accounts)
use remote test data centers. They are useful for service integration, but
do not provide a deterministic offline GUI fixture. A full local MTProto
service would require a much larger authentication/dialog/photo contract
and stock TDLib compatibility. The current seam intentionally tests neither.

## Validation

Run protocol/scenario tests, package-pin tests and the GUI command above.
For the shared redisplay change also run:

```sh
cargo nextest run -p neovm-core --lib force_window_update
NEOVM_ORACLE_MODE=verify NEOVM_FORCE_ORACLE_PATH=/path/to/gnu/emacs \
  cargo nextest run -p neovm-oracle-tests div_cx409_force_window_update
cargo nextest run -p neomacs-tui-tests --test tui \
  force_window_update_repaints_mutated_display_spec_like_gnu
```

GUI and TUI integration must use a binary rebuilt from the working tree.
`NEOMACS_GUI_TEST_BINARY`, `NEOMACS_TUI_NEOMACS_BIN`, and the oracle's
`NEOVM_BINARY_PATH` can select it.
Package provisioning is cached separately from each fresh scenario.

The final post-rebase run used a freshly rebuilt `target/debug/neomacs` for final
integration checks. Results: 23 fixture unit tests, one package-pin test,
15 core redisplay tests, 117 layout validity/incremental/freshness tests,
two live GNU oracle tests, two paired terminal tests (forced display-spec
repaint and the new main-branch terminal-output test), and all seven private
GUI tests passed. Formatting passed. Clippy found no warnings in the new
fixture; strict Clippy was blocked by eight findings in unchanged
infrastructure files.

A repeatability check ran the 23 fixture unit tests, seven private GUI tests,
two live GNU oracle tests, and one paired terminal test three times each
with `--stress-count 3 --retries 0`: all iterations passed. This establishes
repeatability in the current environment, not portability across every host
or freedom from all timing-related failures. Each GUI repetition starts a
new editor, mock, display, and scenario directory. After review strengthened
the teardown probes to preserve and authenticate with the private cookie,
both normal and panic cleanup tests passed three further repetitions without
retries. The final 15 core tests also passed three repetitions without retries.

## Boundaries

The fixture never reads `~/.config/emacs`, `~/.emacs.d`, `~/.telega`, an
account, database, cache, profile photo, captured conversation, user Emacs
server, user display, or user desktop focus.  All chats, titles, and avatar
pixels are generated in-tree.  The archived probes under
`tmp/telega-avatar-probes/` are not part of the suite and are not restored by
this work.
