# Issue #600: startup hooks, early frame settings, and stipples

Investigated 2026-10-26. The report concerns the macOS 0.0.20 release at
`47fa15cc69ae8f8da1ca3fac25e0beb700e8c118`; source inspection here starts from
NeoMacs `dc1b3ee0343ff6b3a58709e7171b500c3188f820`. GNU Emacs was studied first
at local revision `a360712c9d272d950d8d8255ef74570f7e90b7d9`; its checkout
remained read-only. These are source findings and design inferences, not claims
that all three reported symptoms reproduce on current main or that a macOS
runtime was tested. [Issue #600](https://github.com/eval-exec/neomacs/issues/600).

## Reproduction and resulting change

The author's stipple configuration reproduces a rendering defect on unchanged
main: using a space indicator at column 16 paints 16 vertical stipple lines
across the preceding padding. Both a public layout regression and actual X11
GPU pixel readback fail before the fix. The indicator itself already retains
its bitmap; general stipple support is present.

The plain-line branch of `resolve_indicator_fill` reused the indicator face for
the gap stretch. GNU uses the line-end extension/default face for padding and
merges the indicator face only for its own cell. The fix adds a required
`LineEndFaceResolver::default_padding_face_id` capability and resolves it through
the existing window-scoped face environment. Every resolver must explicitly
implement the new method. Buffer remapping and display realization are
preserved; no hardcoded default face ID or special renderer path is needed.
The existing line-end enums continue to define the ordered glyph effects.

The two startup claims do **not** reproduce on Linux. Five isolated startup
cases cover explicit init directories with and without command-line actions,
ordinary HOME/XDG discovery, and directory precedence. They establish exactly
one early-init load and hook invocation, the author's three frame parameters,
hook ordering, and actual hidden chrome/scroll bars. The baseline X11 hook
witnesses match a live GNU GUI run; final Wayland cases pass too. No startup
production code was changed. Native macOS execution and the author's unspecified
hook body remain unverified, so this change should reference rather than close
the whole issue.

## Validation results

- Failing layout test and failing baseline X11 pixel test recorded before the
  fix; the baseline executable and runtime image were preserved under
  `tmp/issue600/baseline-bin`.
- Eleven focused fill-column-indicator layout tests pass, including default
  face remapping, inverse video, and an extended background.
- Full layout suite: 3,268 pass, one known failure, six skipped. The failure is
  `layout_frame_rust_nerd_font_alias_icon_uses_resolved_monospace_cell_width`
  (icon width 6 versus cell width 7), already listed in the local baseline gate
  from 2026-10-18 and observed before this task. It does not involve an indicator.
- Core display/face/window unit selection: 448 pass.
- Live GNU oracle tests: the exact stipple configuration and frame parameter
  behavior both pass.
- Two TUI comparisons pass, including the exact space/stipple configuration.
- Stipple pixel tests pass on X11 and Wayland, checking one painted column in
  the configured indicator cell and successful process exit.
- Startup GUI cases pass on X11 and Wayland; five X11 cases also compare their
  hook witnesses with GNU. Existing frame-bar GUI coverage passes both backends.
- `cargo fmt --all -- --check` and `git diff --check` pass.

Tests use `cargo nextest`, the existing Nix development environment, and repo
`./tmp/` for temporary files. Logs and red/green artifacts are under
`tmp/issue600/`. A normal development binary with matching regenerated images
was used; no native macOS or release-profile runtime claim is made.

Representative commands (after sourcing the existing development environment
and resetting `TMPDIR="$PWD/tmp"`):

```sh
CARGO_TARGET_DIR="$PWD/target/issue583" cargo nextest run \
  -p neomacs-layout-engine --lib \
  --features redisplay-test-policy,neovm-core/desktop-font-settings

CARGO_TARGET_DIR="$PWD/target/issue583" cargo nextest run \
  -p neovm-core --lib --features desktop-font-settings,redisplay-test-policy \
  -E 'test(emacs_core::display::) or test(face::) or test(window::)'

NEOVM_BINARY_PATH="$PWD/target/issue583/debug/neomacs" \
NEOVM_FORCE_ORACLE_PATH=/home/exec/Projects/github.com/emacs-mirror/emacs/src/emacs \
NEOVM_ORACLE_MODE=verify CARGO_TARGET_DIR="$PWD/target/issue583-oracle" \
  cargo nextest run -p neovm-oracle-tests --test oracle \
  -E 'test(oracle_fill_column_stipple) or test(oracle_frame_bar_parameters)'

NEOMACS_TUI_NEOMACS_BIN="$PWD/target/issue583/debug/neomacs" \
CARGO_TARGET_DIR="$PWD/target/issue583-tui" \
  cargo nextest run -p neomacs-tui-tests --test tui \
  -E 'test(space_fill_column_stipple_preserves_terminal_display) or test(frame_bar_parameters_and_root_geometry)'

NEOMACS_GUI_TEST_BINARY="$PWD/target/issue583/debug/neomacs" \
NEOMACS_GUI_TEST_BACKEND=x11 CARGO_TARGET_DIR="$PWD/target/issue583-ui" \
  cargo nextest run -p neomacs-gui-tests --test gui \
  -E 'test(fill_column_stipple) or test(startup_user_settings) or test(frame_bar_parameters)' \
  --test-threads 1
```

Repeat GUI cases with `NEOMACS_GUI_TEST_BACKEND=wayland`. To compare startup
witnesses with GNU, set `NEOMACS_GUI_STARTUP_ORACLE_BINARY` to the GNU GUI binary
on a backend that binary supports.

## GNU startup contract

The early init file loads before window-system initialization and
`frame-initialize`. The latter combines `initial-frame-alist` before
`default-frame-alist` when creating the first graphical frame. This makes the
initial-frame entries higher priority when both lists specify a parameter.
Later, `frame-notice-user-settings` reconciles changes from the ordinary init
file. Both paths that finish interactive startup run `window-setup-hook`
after that reconciliation; the startup-screen path sets
`inhibit-startup-hooks` to avoid running the hooks again on return.
[GNU startup.el loading and frame initialization](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/lisp/startup.el#L1530-L1611),
[GNU frame.el initial parameter merge](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/lisp/frame.el#L414-L442),
[normal-top-level completion](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/lisp/startup.el#L833-L849),
[startup-screen completion](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/lisp/startup.el#L3080-L3106).

The documented distinction between `emacs-startup-hook` and
`window-setup-hook` is frame parameter reconciliation, rather than first GPU
presentation. The early-init manual also recommends deferring work that needs
GUI facilities to startup hooks. Therefore a NeoMacs fix should preserve the
GNU Lisp startup sequence and establish the required graphical state at its
frame boundary. A separately replayed Rust hook risks duplicate execution or
running against unreconciled parameters.
[Startup Summary](https://www.gnu.org/software/emacs/manual/html_node/elisp/Startup-Summary.html),
[Early Init File](https://www.gnu.org/software/emacs/manual/html_node/emacs/Early-Init-File.html),
[Init File hook documentation](https://www.gnu.org/software/emacs/manual/html_node/elisp/Init-File.html).

An explicitly present nil-valued parameter is different from an absent
parameter: `(vertical-scroll-bars)` is a cons whose value is nil, requesting
disabled scroll bars. Resolution must select the first *present entry*, then
interpret its value. Filtering nil before applying defaults would erase this
request. Similarly, zero menu/tool bar lines are explicit settings. Frame
creation normally fills only unspecified parameters from `default-frame-alist`.
[Creating Frames](https://www.gnu.org/software/emacs/manual/html_node/elisp/Creating-Frames.html),
[GNU frame initialization](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/lisp/frame.el#L424-L434).

## GNU stipple contract

`:stipple` describes a background bitmap, not character ink. It accepts a bitmap
filename or inline `(WIDTH HEIGHT DATA)`. Rows occupy `ceil(WIDTH / 8)` bytes;
row padding is retained. Nil removes the pattern. `bitmap-spec-p` checks
positive dimensions bounded by `INT_MAX` and sufficient data for all rows;
trailing bytes need not be rejected. Invalid stipples passed to
`set-face-attribute` signal an error.
[Face Attributes](https://www.gnu.org/software/emacs/manual/html_node/elisp/Face-Attributes.html),
[GNU bitmap validation](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/xfaces.c#L812-L861),
[GNU attribute validation](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/xfaces.c#L3612-L3623).

GNU's bitmap conversion reads column `x` from bit `x % 8` of byte `x / 8`,
least significant bit first. On Cocoa, glyph backgrounds are filled with the
background color, then tiled with the foreground color through the stipple
mask. Thus an indicator whose character is a space can still show a vertical
pattern: the painted content comes from its face background.
[GNU image conversion](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/image.c#L270-L288),
[GNU Cocoa background painting](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/nsterm.m#L4100-L4135).

GNU produces a fill-column indicator as a character glyph with the
`fill-column-indicator` face merged over the active line-end extension face.
It does not reduce the indicator face to a foreground color. Testing only that
the character or color is correct misses the reporter's space-and-stipple
case. [GNU indicator glyph production](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/xdisp.c#L24562-L24615).

Pattern origin is a separate concern. Xlib defines an infinite repeated bitmap
plane anchored relative to the destination drawable; GNU's normal X11
background fill uses that GC plane. Resetting phase at each clipped glyph
rectangle is not generally equivalent. An origin correction should have its own
oracle evidence and tests, rather than being assumed necessary for this report.
[Xlib fill origin](https://xorg.freedesktop.org/archive/current/doc/libX11/libX11/libX11.html#Setting_the_Fill_Tile_and_Stipple),
[GNU X11 background fill](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/xterm.c#L8572-L8592).

## NeoMacs source observations and design implications

Current main already has inline, built-in, and XBM stipple realization plus GPU
background painting. The broad statement that stipples are unsupported must
therefore be narrowed by reproduction: inspect the exact synthetic indicator
glyph, its face identity, and the realized frame face, then its final pixels.
[Realization](../../crates/neomacs-layout-engine/src/neovm_bridge.rs),
[transported face](../../crates/neomacs-display-protocol/src/face.rs),
[renderer](../../crates/neomacs-renderer-wgpu/src/renderer/glyphs.rs).

The following are design inferences, not a proposed requirement to refactor all
these paths in this issue:

- Preserve the complete resolved face through synthetic glyph production and
  reuse ordinary background painting. A special indicator-only stipple drawing
  path would duplicate inheritance, clipping, opacity, and future face effects.
- Preserve startup phase and frame identity explicitly. A typed one-shot
  transition should establish the opening graphical frame before GNU startup
  consumes its settings; setters should not enqueue an unbounded sequence into
  a bounded native channel while no receiver can drain it.
- Resolve parameter presence separately from value. An enum distinguishing
  absent, explicit nil, and a nonnil value can prevent defaulting code from
  silently converting disabled settings into fallback settings.
- For a later bitmap abstraction, use one fallible constructor with positive
  dimensions, checked row-size multiplication, and immutable owned bytes. Keep
  fields private and make deserialization pass through that constructor;
  otherwise public fields or derived deserialization can bypass validation.
  `NonZeroU32`, `usize::checked_mul`, and Serde's `try_from` support these
  invariants. The current public `StipplePattern { width, height, bits }`
  representation permits zero dimensions and truncated payloads.
  [Rust NonZero](https://doc.rust-lang.org/std/num/struct.NonZero.html),
  [checked arithmetic](https://doc.rust-lang.org/std/primitive.usize.html#method.checked_mul),
  [Serde container conversion](https://serde.rs/container-attrs.html#try_from).

## Verification boundaries

Use separate failing cases for hook execution/order and early-init chrome
settings. Verify both Lisp-visible frame parameters and displayed chrome; one
can be correct while the other is stale. For stipples, retain the reporter's
unibyte bitmap construction and space character, and check face propagation
plus actual colored pixels. Include a non-byte-aligned bitmap width, clipped
domains, and mutation/invalidation coverage when those paths change. Oracle
tests should compare GNU-visible contracts; GUI tests should exercise native
launch rather than pretending batch startup is interactive. TUI coverage should
check startup order and logical glyph behavior without claiming terminal bitmap
rendering. Linux GUI coverage cannot establish native macOS behavior.
