# GNU XWayland / native Wayland font evidence

This diagnostic compares the attested GTK3/X11 GNU oracle under XWayland
with Neo's native Wayland backend in one private headless Sway session.
It changes no product policy. It stops and records the first difference for
a reference decision; it does not bless either backend's output.

The test is explicitly ignored in the ordinary suite because it requires a
real GNOME settings daemon and complete GNOME schemas, and a difference is
an unresolved reference decision. An ignored row is not a parity pass.

Set these executable/schema paths before running the row:

- `NEOMACS_GUI_SWAY`: the direct, unwrapped Sway ELF executable. A shell
  wrapper can leave a compositor running after its owned wrapper exits.
- `NEOMACS_GUI_GRIM`: grim executable.
- `NEOMACS_GUI_GSD_XSETTINGS`: real `gsd-xsettings` executable.
- `NEOMACS_GUI_GSD_SCHEMAS`: colon-separated complete compiled schema
  directories for gsettings-desktop-schemas and gnome-settings-daemon.
  The full enum definitions are required; a minimal string-only schema
  does not exercise gsd's `g_settings_get_enum` path.
- `NEOMACS_GUI_TEST_BINARY`: matching fresh release Neo executable.
- `NEOMACS_RUNTIME_ROOT`: that executable's generated runtime checkout.
- `NEOVM_ORACLE_MODE=verify` and `NEOVM_FORCE_ORACLE_PATH`: the canonical
  `tmp/gnu-emacs-oracle/bin/emacs` executable. The row also invokes
  `scripts/ci/check-gnu-oracle.sh` and saves its profile/features output.

On a software Vulkan host, also select the installed lavapipe ICD through
`VK_DRIVER_FILES` and set `LP_NUM_THREADS=2`. Run inside the repository's
normal development environment:

```sh
cargo nextest run --locked -p neomacs-gui-tests --test gui -j 1 \
  --run-ignored only \
  -E 'test(=font_xwayland_parity::attested_gnu_xwayland_and_native_wayland_raster_preferences)'
```

The helper owns a private D-Bus session, keyfile GSettings backend and real
`gsd-xsettings`. It refuses an existing XSETTINGS owner. It proves the
XWAYLAND extension, XSETTINGS owner continuity, changed serials, raw
GSettings values, and matching X resources before launching editors.
The preflight changes rgba/full/bgr and restores grayscale/slight/rgb.
Neither host desktop settings nor host D-Bus services are changed.

The fixture preserves GNU/Neo's ordinary dynamic-setting handler. The row
first compares startup grayscale/slight/rgb, then changes to none/none/rgb
and requires completed real font-render events in both arms. If those
comparisons match, it checks fresh pairs for none/grayscale/rgba crossed
with none/slight/full, plus live/fresh consistency. Any first difference
stops subsequent phases. Unrun phases must be reported as unrun.

Artifacts under `tmp/neomacs-gui-tests/font-xwayland-*` include the oracle
check, daemon and process logs, raw XSETTINGS/Xresources, full font-info
vectors, frame/character metrics, completed event traces, compositor PNGs,
the comparison matrix, and owned-child cleanup receipts. Metrics compare
font-info numeric fields 2 through 11, frame character width/height, and
each displayed character's advance. Full font names/capabilities remain
in the raw evidence; both resolved regular/bold files must match the
same pinned IBM Plex Sans files. Ink is the sum of `765-R-G-B` over each
row's actual compositor region, after two stable observations.

The source contract is GNU 31.1 at the pinned mirror commit:

- `src/ftcrfont.c:175-187`: PGTK passes GSettings options to the metrics'
  scaled font; GTK3/X11 starts those options at Cairo defaults.
- `src/xsettings.c:865-1003`: XSETTINGS changes update the exposed Xft
  settings and enqueue font-render events; the Cairo build has a distinct
  path from Xft's defaults.
- `lisp/dynamic-setting.el:62-88`: ordinary font-render handling reconsiders
  font settings. The fixture observes this handler rather than replacing it.
- `src/font.c:5594-5672`: the full 14-field font-info vector contract.

The bridge follows the [XSETTINGS owner/property/serial protocol](https://specifications.freedesktop.org/xsettings/latest/).
Cairo documents [font-option defaults and merging](https://www.cairographics.org/manual/cairo-cairo-font-options-t.html);
those backend boundaries are why a paired observation is needed. Native
GNU PGTK remains **UNATTESTED**: the canonical Linux oracle recipe builds
GTK3 with `--without-pgtk`. This test supplies no PGTK behavioral claim.
