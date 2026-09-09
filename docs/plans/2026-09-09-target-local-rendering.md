# Target-local rendering and native presentation ownership

## Goal

Drawing a popup must not change an unchanged editor frame's pixels. Native
surface parenting and stacking belong to presentation, menu navigation to
menus, and projection/clipping/attachments to an explicit drawing target.
The long-term renderer has shared GPU resources, per-scene rendering state,
explicit targets, and a scoped drawing context. It has no mutable current
window or save/change/restore contract.

## Implemented migration slice

- `renderer/target.rs`: a borrowed destination paired with the existing
  validated `DrawableSurface` geometry. No duplicate coordinate system.
- `renderer/draw/`: scoped menu/blit context and immutable parameter bindings.
  Buffers have no `COPY_DST` usage. A bounded cache reuses identical snapshots;
  eviction drops only cache ownership, never rewrites in-flight storage.
- `renderer/paint/`: menu painting and retained-image blitting. Their implicit
  target entry points are removed; production callers use `begin_draw`.
- Frame glyph and cursor passes bind a projection snapshot for the resolved
  `PresentMapping`; their layer painters cannot overwrite that projection.
  Standalone overlays and child-frame draws use immutable snapshots too.
- `presentation/host.rs`: owns a popup chain, chooses each child's parent,
  rejects unmapped parents, and closes children before parents. No numeric
  native z-index. The compositor remains authoritative for popup placement.
- `presentation/platform/winit.rs`: native creation and platform constraints.
- `presentation/surface.rs`: native drawable ownership, without menu items or
  glyph atlases.
- `menus/controller.rs`: menu state plus atlas/scrolling state, using the host
  for native lifetime and the draw context for painting.

The offscreen regression draws an editor scene, paints differently scaled
popups with text and a separate glyph atlas through parameter-cache eviction,
then blits the retained editor image and performs a fresh editor glyph redraw.
Both must be identical without any renderer resize/restore. The original
reproducer failed with 866 differing pixels before this migration.

## Remaining migration (not claimed complete)

The legacy renderer still owns frame-sized CPU/effect state and stencil targets;
frame runtime paths still select/restore that legacy state. Transition helpers
and several existing overlay interfaces still obtain their dimensions through
that frame context. Move those onto explicit scene/target ownership before
removing the remaining renderer dimensions and save/restore paths.

Top-level editor windows remain owned by the existing frame-window manager.
The new popup host is not yet the single host for all native surfaces. Extract
native frame ownership without moving evaluator/frame interaction logic into
presentation. Do not create empty platform adapters or duplicate scene caches
just to match a directory tree.

The follow-up now replaces mutable indexed popup access with focused
geometry/resize/draw operations. Acquisition, retry and presentation happen
inside the host borrow; menu code cannot replace parent surfaces or retain an
acquired image. `RenderTarget` currently requires a full-size,
default-format, single-sample view; it does not validate arbitrary view
descriptors. Raw frame layer passes inherit group 0 from their caller.

Windows/macOS remain unverified; upstream X11 popup support and Android/browser
presentation adapters remain unfinished. This change adds no in-window fallback
and changes no default Cargo features.

## Verification

Use `cargo nextest`, never `cargo test`.

```sh
cargo nextest run -p neomacs-display-protocol -p neomacs-display-runtime \
  -p neomacs-renderer-wgpu --no-default-features --no-fail-fast
cargo check -p neomacs --features video,webview --all-targets
WAYLAND_DEBUG=1 cargo nextest run -p neomacs-display-runtime \
  --no-default-features --lib -E 'test(linux_wayland_native_menu_smoke)' \
  --run-ignored only --no-capture
```

2026-09-09: 2,346 affected tests passed (two skipped), the all-targets check
passed, and the explicitly enabled Linux Wayland native menu test passed.
Standards review found no concrete correctness blocker; its scroll-state data
clump and implicit binding documentation findings were addressed. Host access
encapsulation remains a follow-up above. Spec review found the popup-text
coverage gap, now covered by the separate-atlas regression, and confirmed the
larger migration is partial rather than complete.

Release verification also passed: `cargo xtask fresh-build --profile release
--no-byte-compile` rebuilt the executable and matching pdump; batch startup
printed `target-local-pdump-ok` and exited 0. A timed release GUI launch opened
a keymap-backed `x-popup-menu`, created/configured/destroyed an `xdg_popup`, and
exited cleanly. This programmatic check does not establish interactive hover,
input-grab, or focus behavior. Logs:

- `/tmp/neomacs-target-local-release.log`
- `/tmp/neomacs-target-local-release-startup.log`
- `/tmp/neomacs-target-local-release-gui.log`

## Heading interaction follow-up

The user confirmed the stretching fix, then reported jitter while moving over
Help and Interactively. Their trace contained 177 menu-open receipts (33 Help,
144 Interactively), with native popup creation often 20–30 ms apart.

The old path stored the requested heading in frame chrome. After a hover switch
cancelled the previous popup, its asynchronous HidePopupMenu could erase that
new heading before the new ShowPopupMenu arrived. The next motion therefore
cancelled/reopened the same menu. A runtime regression reproduced that erased
heading; it now preserves the pending intent and repeated hover emits nothing.

- `menus/menu_bar.rs` owns heading intent and pending/shown request identity.
  Chrome projects that state; pointer and root keyboard switching share it.
- `MenuBarRequestId` travels through keyboard input, the evaluator's pending
  native anchor, and popup replies. A stale A response after A→B→A is rejected.
  MenuToken remains the identity of the evaluator popup transaction/revision.
- A separate real evaluator regression reproduced a newer heading click being
  swallowed by the old modal popup loop when cancellation arrived later. That
  loop now requeues the heading sequence for ordinary dispatch, preserving its
  request identity and native anchor.
- Cancellation keys work before contents arrive. A closing press retains its
  release ownership; drag-to-select clears it without suppressing activation.
- Native popup painting clears its own target. The presentation host retains
  native lifetime, parent readiness and acquisition/presentation ownership.

The primary-source comparison is in
[`menu-interaction-ownership`](../research/2026-09-09-menu-interaction-ownership.md).
Its GTK/Qt same-selection guards and Chromium active/pending controller informed
this work. No debounce or pointer-navigation timeout was added; diagonal
submenu intent/geometry policy is separate future work. This does not complete
the broader per-scene renderer/native-frame ownership migration described above.

Follow-up verification: 2,352 display/protocol/runtime/renderer tests passed
(two skipped), 26 focused evaluator/bridge tests passed, and the explicitly
enabled Wayland test passed with exactly two native popup identities despite
repeated heading hover. The `video,webview` all-targets check passed. Standards
review's pending Ctrl-G and drag-release findings were fixed; spec review's
queued evaluator switch, pending cancellation, release capture, and keyboard
ownership findings were addressed. Both rechecks reported no remaining blockers.

Logs: `/tmp/neomacs-menu-owner-final-suite.log`,
`/tmp/neomacs-menu-owner-final-protocol.log`,
`/tmp/neomacs-menu-owner-final-wayland.log`, and
`/tmp/neomacs-menu-owner-check.log`. These targeted results do not supersede the
previously recorded unrelated whole-core test failures.
