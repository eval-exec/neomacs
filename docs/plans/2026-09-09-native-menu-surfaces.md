# Native menu surfaces

## Ownership

The evaluator owns command/keymap interpretation and the modal menu transaction.
`PopupMenuRequest` publishes an immutable snapshot identified by `MenuToken`
(session ID plus revision). The GUI returns a snapshot-scoped item identity or
cancellation. Lisp values and command execution never move to the renderer.

The source layout is:

```text
neomacs-display-protocol/src/menu.rs       items, tokens, results, panel paint data
neomacs-display-protocol/src/popup_placement.rs
                                         parent-relative placement intent
neomacs-display-runtime/src/menus/
    mod.rs                               runtime-facing exports
    session.rs                           hierarchy and transaction lifetime
    interaction.rs                       keyboard navigation
    layout.rs                            panel measurement
    presentation.rs                      native panel chain, scrolling, events
    platform/desktop.rs                  winit windows and wgpu drawing targets
    session_test.rs                      platform-independent behavior
    native_test.rs                       opt-in Wayland compositor smoke test
neomacs-renderer-wgpu/src/renderer/menu.rs one panel's painting
```

`render_thread` routes events and commands. Frame chrome handles menu-bar heading
switches before handing other menu input to `menus`. Toolbars remain frame
chrome; this is not a general widget framework. Tooltips remain unchanged.

## Invariants

- There is no in-window submenu painter or input fallback.
- Native windows and GPU surfaces live on the GUI event-loop thread.
- Each submenu retains its native parent. Teardown pops children before parents;
  each GPU surface is dropped before its window.
- A child popup is created only after its parent has presented a buffer.
- Input coordinates are local to the actual popup, independent of compositor
  flips/slides. Compositor-configured size controls the scrolling viewport.
- Placement constraints target the compositor's usable area, not the editor
  viewport. Old frame-viewport padding is not interpreted as screen-edge padding.
- Results retain the token at the moment of completion, even when another menu
  opens before the result is drained. Cancellation is reliable input.
- Stale show/hide messages are rejected by the runtime. Stale result revisions
  are rejected by the evaluator. A newer revision can supersede an older
  revision's cancellation without leaving the evaluator waiting indefinitely.
- Item identities refer to the full snapshot, not a panel's local row. Reordering
  a snapshot requires a new revision; identities are not semantic command IDs
  that survive arbitrary reordering.
- Popup atlases bind the owning frame's captured font tables before painting.

The historical unqualified keyboard menu-selection form remains accepted for
TTY and synthetic-input compatibility. All native GUI results carry tokens.

## Windowing dependency and platform status

The workspace pins winit commit
`a98b2b217c901f8776a2bdb4ebc35ea7611998ce` (0.31.0-beta.3). This supplies native
popup window roles and positioners that the previous 0.30 dependency lacked.
The migration also updates pointer events, IME-compatible calls, drag-and-drop,
monitor queries, and event-loop/window ownership.

Linux Wayland is the only runtime-verified platform. The smoke test created a
344x58 root popup and a 201x638 submenu attached to a 200x100 parent; the protocol
trace confirmed separate `xdg_popup` objects and configured extents.

Windows and macOS use the shared desktop entry point but are unverified. The
pinned upstream X11 implementation rejects popup window creation. Android and
WASM do not yet have presentation adapters. These are explicit unsupported
paths, not completed cross-platform delivery. They must gain real platform
implementations before this feature can be called available on every target.
Browser presentation must use a browser-native presentation mechanism subject
to browser constraints; it must not silently substitute a desktop frame overlay.

Further work includes platform rollout and richer menu text shaping, mnemonics,
and accessibility. Current panel measurement retains the existing fixed-cell
menu metrics; this change does not claim full toolkit text/layout parity.

## Verification

```sh
cargo nextest run -p neomacs-display-runtime -p neomacs-display-protocol \
  -p neomacs-renderer-wgpu --no-default-features
cargo nextest run -p neovm-core -p neomacs --lib --no-fail-fast
cargo check -p neomacs --features video,webview
WAYLAND_DEBUG=1 cargo nextest run -p neomacs-display-runtime --no-default-features \
  --lib -E 'test(linux_wayland_native_menu_smoke)' --run-ignored only --no-capture
```

The last command opens temporary windows for five seconds and requires a live
Wayland compositor and a Vulkan-capable GPU. Other tests do not establish native
behavior on operating systems that were not available for testing.

The compositor smoke test verifies separate popup surfaces and rendering, not
an end-to-end user-input grab: it opens menus programmatically without a pointer
or keyboard serial. Interactive dismissal and focus behavior still need manual
Wayland QA.

The display protocol/runtime/renderer suites passed under `cargo nextest`
(2,345 tests including integration tests), as did the native Wayland smoke test,
the evaluator's stale-menu-result test, and the
`video,webview` all-targets compile check. The broader Neomacs/core library run
under nextest completed with 10,022 passes, 21 failures, one timeout, and 54
skipped tests. Failures included startup assertions, video handles, missing
bytecode (consistent with keeping stale bytecode out without byte-compiling),
process behavior, and pdump-derived syntax. The timeout was
`all_eighteen_are_ordinary_calls_that_read_the_cell_like_gnu`. These broader
failures remain unresolved; this is not a clean whole-workspace test run.

`cargo xtask fresh-build --profile release --no-byte-compile` completed
successfully. The rebuilt executable loaded its final pdump in batch mode and
exited successfully. A timed release GUI probe using a keymap-backed
`x-popup-menu` created an `xdg_popup` through the actual evaluator/runtime path
and exited cleanly. This programmatic probe still does not verify input grabs.
