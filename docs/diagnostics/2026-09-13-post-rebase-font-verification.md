# Post-rebase font verification

Verified production HEAD: `e99ab284d`, Linux, 2026-09-13. No production source
changes were made before these checks. The new live-settings fixture was
prepared separately and is not included in the passing baseline GUI selection.

The user requested `cargo xtask fresh-build --release`, superseding the
handoff's manual build/sealing/pdump instructions. It completed successfully,
including byte compilation and the matching runtime image. Fingerprint:
`6FB9FCC99E843BD77FAE84E369C04F4BFDABCEEAEBD56F98EF17EC75207C14D7`.
`lisp/ldefs-boot.el` retained SHA-256
`094b50fbd69032559bcac655d6368e16b8e69ae70b53989a99576ecd5c325138`.

| Check | Result | Log under `target/diagnostics/issue-360/startup-fonts/` |
| --- | --- | --- |
| Fresh build, sealing, pdump, byte compilation | Passed | `continuation-fresh-build.log` |
| Display runtime | 966 passed, 5 skipped | `continuation-runtime.log` |
| GUI harness contract | 16 passed, 0 skipped | `continuation-harness.log` |
| Core fonts/window commands, bootstrap/platform policy, layout font metrics | 550 passed, 11,963 excluded | `continuation-focused-rebuilt.log` |
| Existing startup-font, startup-failure, resize GUI selection | 11 passed, 0 skipped | `continuation-gui.log` |
| Formatting, explicit Wayland rustfmt, diff whitespace | Passed | `continuation-fmt.log` |

The first focused attempt had 401 passes and 149 failures, all caused by the
same stale `startup.elc` refusal. The completed fresh build removed that stale
bytecode; the identical selection then passed. The initial manual release
build was interrupted when the user directed use of xtask. Neither attempt is
reported as a passing build. Initial test-list/filter command mistakes are
retained in the local command history, not counted as executed tests.

Prior handoff logs and package paths were unavailable in this environment.
Dependencies were prepared below the diagnostic directory, with a private
Fontconfig file adding Ubuntu Mono without installing it on the desktop:

- `svg-lib`: `925ed4a0215c197ba836e7810a93905b34bea777` from its official repository.
- `svg-tag-mode`: `13e888b8bd9a0664d060149a44a751b2113331b6` from its official repository.
- Google Fonts `ufl/ubuntumono/UbuntuMono-Regular.ttf`, SHA-256
  `b35dd9d2131d5d83a9b87fe9ad22c6288fa3d17688d43302c14da29812417d63`.

The Neomacs GUI checks used headless stock Weston, including native virtual
output presentation receipts. The GNU resize control used Xvfb. These are not
physical monitor scanout or macOS/Windows runtime verification. The separate
intentional Weston cached-detach protocol diagnostic was not rerun or fixed.

The related [issue comment](https://github.com/eval-exec/neomacs/issues/360#issuecomment-5645184126)
reports different default frame fonts/cell sizes with matching tag-font
metrics, and matching SVG after setting the same default face. The existing
desktop-font/SVG regression remains the relevant baseline for that report.
No issue comment or other external update was posted.
