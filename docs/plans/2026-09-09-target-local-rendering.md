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

Before expanding host use, replace mutable indexed surface access with focused
resize/acquire/present operations, so callers cannot bypass parenting and
configuration invariants. `RenderTarget` currently requires a full-size,
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
