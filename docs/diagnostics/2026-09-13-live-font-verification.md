# Linux live desktop-font verification

Neomacs now observes Linux desktop font preferences and routes changes through
GNU's existing Lisp policy. Queries remain current while adoption is disabled.
Opted-in changes update current frames and future defaults through opened-font
realization, including native geometry changes and inhibited-resize accounting.

GNU reference: the read-only checkout at
`/home/exec/Projects/github.com/emacs-mirror/emacs`, revision
`a360712c9d272d950d8d8255ef74570f7e90b7d9`.
The oracle binary reports GNU Emacs 31.1 with that same repository revision
and GTK3/X11/GSETTINGS support (`live-gnu-runtime-version.log`). Its SHA-256 is
`1bbc23be1ed3ed354779b184f872f0ff7879aac1e121b475ee38c319275e9791`.
See the [source research](2026-09-13-live-desktop-font-settings.md) and
[implementation plan](../plans/2026-09-13-platform-startup-fonts.md).

## Behavior coverage

The GUI fixtures use actual external `gsettings` writes with private keyfile
backends, schemas and config directories. They never modify the user's desktop
settings. GNU runs on Xvfb at 96 DPI; Neomacs runs on stock headless Weston.

| Behavior | Observation |
| --- | --- |
| Opt-in | Query, default face and opened-font metrics change together |
| Opt-out | Queries refresh; font-object identity and frame geometry remain unchanged |
| Application preference | Its query changes without replacing the document font |
| Enabling opt-in | Does not replay a preference skipped while disabled |
| Explicit current fonts | Both existing frames adopt the enabled update |
| Future frames | Ordinary frames inherit the new default; an explicit new-frame font takes precedence |
| Duplicate preference | A later application event provides a barrier; font identity and geometry stay unchanged |
| Repeated preference | A second distinct font returns to the original metrics and text grid |
| Native geometry | 80×24 text cells are retained, with a later matching Presented receipt |
| Child frame | A 40×10 grid follows the changed font through the existing child resize path |
| Resize inhibition | Pixel allocation stays fixed while columns are recomputed |
| Fullscreen | Confirmed fullscreen dimensions remain fixed during adoption |
| Pending explicit resize | A local font-geometry refresh preserves the request for later delivery |
| Shutdown | Normal GUI exits and both existing early-startup failure paths terminate without hanging |

The fullscreen control is native Neomacs Wayland coverage plus GNU source
comparison. Xvfb has no window manager for a GNU native fullscreen oracle.
The pending-request regression uses the previously approved public Lisp/host
seam in `window_cmds/tests/frame_resize_test.rs`.

## Final verification checkpoint

Verified production commit: `2807b139d`. The build and checks ran with HEAD
`c850e7c82` plus the exact working-tree changes subsequently committed as
`2807b139d`; no production code changed after those checks. This document is
a subsequent documentation-only checkpoint.

The user-requested `RUST_LOG=warn cargo xtask fresh-build --release` completed
successfully, including autoload generation, byte compilation and a matching
dump. Executable/dump fingerprint:
`2D4F66AABDC881F2677C457E9FD2F31460992620D973D09DFAEF610141E737C8`.
The preserved `lisp/ldefs-boot.el` SHA-256 is
`094b50fbd69032559bcac655d6368e16b8e69ae70b53989a99576ecd5c325138`.

All logs below are under `target/diagnostics/issue-360/startup-fonts/`.

| Check | Result | Log |
| --- | --- | --- |
| Combined runtime/core/layout selection | 1,558 passed; 11,927 skipped/excluded by selection | `live-final-focused.log` |
| Bare-core capability and coupled-variable checks | 19 passed; 9,887 excluded by selection | `live-final-bare-core.log` |
| Live font GUI/GNU controls | 15 passed, 0 skipped | `live-final-gui.log` |
| Established GUI/GNU baseline | 11 passed, 0 skipped | `live-final-gui.log` |
| GUI harness contracts | 16 passed, 0 skipped | `live-final-gui.log` |
| Full release/image build | Passed | `live-inhibited-final-build.log` |
| Formatting, explicit Linux/Wayland rustfmt, diff whitespace | Passed | `live-final-fmt.log` |
| Pending explicit request, before fix | Failed at missing host request | `live-inhibited-pending-red.log` |
| Pending explicit request, after fix | Passed | `live-inhibited-pending-green.log` |

The earlier post-rebase checkpoint is recorded separately in
[post-rebase verification](2026-09-13-post-rebase-font-verification.md).

The combined selection includes all display-runtime library tests plus the
bootstrap, font, frontend-event, window-command, capability and font-metrics
filters below. The skip counts include filtered-out tests; they are not a full
workspace or full core-suite pass.

```sh
cargo nextest run -p neomacs -p neovm-core -p neomacs-layout-engine -p neomacs-display-runtime --lib --test-threads 8 -E 'package(=neomacs-display-runtime)|(package(=neomacs)&(test(bootstrap)|test(platform_fonts)|test(input_bridge)))|(package(=neovm-core)&(test(font::tests)|test(frontend_events)|test(window_cmds)|test(c_features)|test(provide_coupled_vars)))|(package(=neomacs-layout-engine)&test(font::metrics))' --no-fail-fast
cargo nextest run -p neovm-core --lib --test-threads 4 -E 'test(c_features)|test(provide_coupled_vars)' --no-fail-fast

FONTCONFIG_FILE="$PWD/target/diagnostics/issue-360/startup-fonts/deps/fonts.conf" \
NEOMACS_GUI_SVG_LIB_DIR="$PWD/target/diagnostics/issue-360/startup-fonts/deps/svg-lib" \
NEOMACS_GUI_SVG_TAG_MODE_DIR="$PWD/target/diagnostics/issue-360/startup-fonts/deps/svg-tag-mode" \
cargo nextest run -p neomacs-gui-tests --test desktop_font_updates --test desktop_font_startup --test startup_failure --test frame_resize_oracle --test harness_contract --run-ignored all --test-threads 2 --no-fail-fast
```

The isolated font/SVG dependency paths and versions are recorded in the
post-rebase verification document. No dependency was installed into the user's
desktop configuration.

## Local commits and review

| Commit | Work |
| --- | --- |
| `201b74218` | Completed post-rebase font/GUI checkpoint |
| `4a325067c` | Owned Linux subscription, typed input delivery and GNU Lisp policy |
| `10d71f645` | Opt-out and application-role controls |
| `03b0c8885` | Explicit/current/future-frame precedence |
| `34745a918` | Implied font resize and presented text-grid coverage |
| `c850e7c82` | Duplicate and repeated preference controls |
| `2807b139d` | Inhibited geometry, preserved pending requests, child/fullscreen controls |

Standards and spec reviews found no remaining blockers in these changes.
Review tightened receiver ownership, removed the child-frame exclusion and
identified the pending-request loss; each correction is included. The final
pending-request regression passed after the ownership fix.

## Limits and retained failures

- Native execution is Linux Wayland only for Neomacs. macOS/Windows live
  subscriptions remain explicitly unsupported; their existing startup policies
  remain separate from the Linux adapter.
- Native Presented feedback confirms Weston's virtual output, not physical
  monitor scanout. No compositor delays, forced commits or patched Weston are
  involved.
- Two concurrent baseline GUI sessions lost their Weston connections once.
  Both passed separately; raw artifacts remain in `geometry-baseline-peer-reset/`.
  The cause is unconfirmed. This is separate from the retained Weston cached
  subsurface-detach protocol diagnostic.
- The text-grid oracle measures `frame-text-lines` and exact text pixels.
  Existing grown-minibuffer `frame-height` accounting and child startup width
  with chrome have separate discrepancies; these fixtures isolate font updates
  from those setup differences.
- Full GNU split-window minimum-size overrides and overlapping stale native
  acknowledgements are not established by this sequential font-update coverage.
- Source ownership and shutdown review confirm that the observer joins its
  worker and destroys Settings/callbacks there. GLib's default backend remains
  process-global; no claim is made that every GLib monitor is freed.

No changes have been pushed and no GitHub issue comment has been posted.
