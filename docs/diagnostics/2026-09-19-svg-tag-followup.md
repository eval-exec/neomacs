# Issue #360 follow-up verification

The September 12 reporter follow-up has nil face remapping in both editors,
but different initial fonts: GNU Ubuntu Mono 35px (18x36 cells), Neomacs
DejaVu Sans Mono 25px (15x30). Explicitly matching the default face makes
the generated SVG identical. The earlier remapping fix alone did not explain
this case.

GNU `src/xfns.c:x_default_font_parameter` consults the desktop monospace
font before its fallback, independently of `font-use-system-font`.
`src/xsettings.c:init_gsettings` reads `monospace-font-name` separately from
the application font. Both source paths were read again for this verification.

This checkout already contains the subsequent fix:

- `ad947f0b5b`: Linux desktop-font discovery before startup.
- `ee47cbf313`: shared platform startup-font/geometry ownership.
- `eab4a24fe4`: open the configured font before allocating the native window.

The existing `SystemFontRole`, `FrameFontSize` and `BootstrapFont` types keep
desktop roles, sizing units, and GUI/TTY bootstrap separate. One opened GUI
font supplies both initial geometry and default-face realization. No new
production change is justified by the original report on this branch.

## Fresh checks

The initial run lacked the tests' required Ubuntu Mono installation: the
desktop tests failed while the SVG remapping test passed. Ubuntu Mono and
the reporter's attached IBM Plex Mono files were then supplied through a
private Fontconfig configuration under `tmp/neomacs-360/`; no desktop font
settings were changed.

All three existing GUI regressions passed on the freshly built executable:

- Desktop monospace startup/window/SVG metrics.
- The same startup contract on a 4K HiDPI output.
- Relative face remapping changes box metrics but not the separate SVG font.

A fresh GNU/Neomacs X11 comparison uses the same isolated GSettings schema,
Fontconfig file, packages, 3840x2160 virtual display, and IBM Plex Mono tag
font. With no face remapping:

| Controlled setting | GNU and Neomacs cells | SVG box in both |
| --- | --- | --- |
| Ubuntu Mono 13, 96 DPI | 9x18 | 36.63x12.78 |
| Ubuntu Mono 13, explicit Xft.dpi=192 | 18x36 | 73.26x25.56 |
| Explicit DejaVu Sans Mono height 120, 96 DPI | 10x19 | 40.7x13.49 |
| Explicit DejaVu Sans Mono height 120, Xft.dpi=192 | 19x38 | 77.33x26.98 |

The complete SVG source matches in each controlled comparison, not just its
outer dimensions. Floating-point serialization has the same long decimals
in both editors. The 192-DPI initial box matches the reporter's GNU result.

Without explicit Xft.dpi, a 192-DPI Xvfb probe selected 34px in GNU and 35px
in Neomacs; specifying the shared resource removed this difference. This
verification does not claim identical fallback DPI discovery, nor establish
the original KDE session's settings provenance. It also does not establish
pixel-identical SVG rasterization or resolve the separate font-weight report.

Logs/probes are under `tmp/neomacs-360/`: `current.log`,
`comparison-96.log`, and `verified-192.json`. The original startup test's
red-to-green evidence is recorded in `2026-09-12-gnu-desktop-font-settings.md`.
This follow-up is verification of an existing fix, not a newly failing test
or a new implementation claimed to fix it again.
