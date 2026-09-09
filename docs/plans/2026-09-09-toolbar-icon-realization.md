# Toolbar icon realization

Baseline: `e122cb442` (verified popup lifetime work). User approved all toolbar
appearance, decoding, and cache-invalidation regression seams, and requested
Rust types/enums/strum and compile-time checks with out-of-line tests.

## Requirements

- Normal and compact toolbars consume the published `tool-bar` face foreground
  and background. The compact menu segment retains the `menu` face.
- SVG `currentColor` and monochrome XBM set bits use the toolbar foreground.
  Explicit artwork colors are preserved; PNG/XPM retain encoded colors/alpha.
- Chrome icons preserve transparent backgrounds so bar and interaction paint
  can show through. Existing editor-image face-background semantics stay intact.
- One typed realization key covers source, logical icon size, face colors,
  background policy, and the owning native window's device scale. Loading and
  drawing derive this same key from published content.
- Reuse unchanged resources; retain variants needed by another live window;
  retire obsolete theme/font-size/scale/source variants via the renderer image
  lifetime fence. Recreate resources following GPU loss.
- Decode completion must redraw chrome even without a new evaluator frame.
- Keep file sources as paths, not string-encoded cache/state keys. Exhaustive
  enum handling and validated scale/nonzero size carry the relevant invariants.
- No change to default video/webview features, popup backends, or Lisp autoloads.

## Structure

- `neomacs-display-protocol/src/toolbar_icon.rs`: immutable style/realization
  identity shared by loading and drawing, derived from frame chrome content.
- `neomacs-display-runtime/src/render_thread/toolbar.rs`: reconcile accepted
  top-level/child presentations with toolbar-owned renderer image residency.
- `neomacs-renderer-wgpu/src/renderer/media.rs`: explicit-scale icon realization.
- `neomacs-renderer-wgpu/src/renderer/ui_overlays.rs`: consume complete published
  content, resolve exact texture keys, composite bar faces and interaction state.
- `ImageBackgroundPolicy`: explicit editor-versus-chrome decoding policy; default
  remains `FaceColor`. Enum iteration covers every policy in regression tests.

## Verification

Use `cargo nextest`, never manually invoke `cargo test`. Cover layout-published
appearance, SVG/XBM/PNG decoded pixels, reuse/retirement, exact texture lookup,
and offscreen GPU pixels for compact background and fractional-scale icons.
Linux/Wayland is the only available native platform; other OS backends are not
claimed verified. Full release binary/pdump regeneration is a separate step.

Non-goals: new image-source formats, full Lisp image-spec parsing parity, native
OS toolbar widgets, or invalidation of files edited in place without a new
published source/appearance. Font identity alone does not invalidate an icon
when its effective raster inputs are unchanged.

## Results

- `cargo check -p neomacs`: passed (existing workspace warnings).
- Full nextest run for display-protocol, layout-engine, display-runtime, and
  renderer-wgpu: **4,719 passed, 8 skipped**. This includes offscreen GPU checks.
  Log: `/tmp/neomacs-toolbar-full-nextest.log`.
- Red-to-green pixel regressions observed the previous opaque SVG wrapper and
  compact toolbar's ignored background, then passed with the fixes.
- Additional coverage: XBM/PNG/XPM pixels, fractional-scale loading independent
  of the renderer's ambient scale, normal/compact icon pixel parity, device-loss
  reload, and completion-triggered redraw without an evaluator publication.
- No release binary/pdump rebuild or cross-platform native verification in this
  change. Existing `lisp/ldefs-boot.el` changes are preserved and excluded.

## Standards review

No hard violations or correctness defect found. Typed/private realization keys,
validated scale/nonzero size, exhaustive background policy, and out-of-line tests
follow the requested standards. The review's naming suggestion was applied:
`forget_after_device_loss()` explicitly limits the cache-reset operation. Small
toolbar-set reconciliation remains linear; measure before adding bookkeeping.

## Spec review

No implementation blocker or scope creep found. The requested completion-to-
redraw regression was added and re-reviewed: a clean accepted frame becomes
dirty on toolbar completion, while an unrelated completion leaves it clean.

Final review status: Standards — 0 unresolved actionable findings; Spec — 0
unresolved findings.
