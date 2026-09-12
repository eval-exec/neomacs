# GNU Emacs desktop font discovery and updates

Source inspected: `/home/exec/Projects/github.com/emacs-mirror/emacs`, revision `a360712c9d272d950d8d8255ef74570f7e90b7d9`, on 2026-09-12. This is source analysis for Neomacs issue #360, not a contribution or proposed bug report to GNU Emacs. The GNU checkout was read-only, as required by its [AGENTS.md](/home/exec/Projects/github.com/emacs-mirror/emacs/AGENTS.md:1).

## Findings

GNU Emacs obtains the desktop fixed-width font directly from GSettings or legacy GConf, independently of Fontconfig's `monospace` alias. Its initial default-font path consults that value even when `font-use-system-font` is nil. XSettings supplies a separate application font and rendering/DPI settings, not this fixed-width-font value. These are different settings channels, so there is no single universal “GSettings > XSettings > Xresources” ordering.

### Discovery and precedence

`xsettings_initialize` calls `init_gconf`, then `init_xsettings` on non-PGTK builds, then `init_gsettings`. The later successful reads overwrite the shared cached values. GSettings is compile-time optional; configuration normally disables GConf when GSettings succeeds unless GConf was explicitly requested. See [xsettings.c:1281](/home/exec/Projects/github.com/emacs-mirror/emacs/src/xsettings.c:1281) and [configure.ac:3949](/home/exec/Projects/github.com/emacs-mirror/emacs/configure.ac:3949).

| Value | Legacy GConf source | XSettings source | GSettings source |
| --- | --- | --- | --- |
| Fixed-width text font (`current_mono_font`) | `/desktop/gnome/interface/monospace_font_name` | None in GNU's parser | `org.gnome.desktop.interface` / `monospace-font-name` |
| Application font (`current_font`) | `/desktop/gnome/interface/font_name` | `Gtk/FontName` | `org.gnome.desktop.interface` / `font-name` |
| Rendering and DPI | Separate from the font strings | `Xft/*`, `Gdk/UnscaledDPI`, `Gdk/WindowScalingFactor` | PGTK uses antialiasing, hinting and RGBA options |

Source: [setting names](/home/exec/Projects/github.com/emacs-mirror/emacs/src/xsettings.c:187), [GConf names](/home/exec/Projects/github.com/emacs-mirror/emacs/src/xsettings.c:485), [XSettings application](/home/exec/Projects/github.com/emacs-mirror/emacs/src/xsettings.c:1012), [GSettings initial reads](/home/exec/Projects/github.com/emacs-mirror/emacs/src/xsettings.c:1136).

`init_gsettings` checks whether the GNOME interface schema exists and creates a client for it. It does **not** check that the running desktop is GNOME. Thus GNU can consume this schema on KDE or another desktop when available. Schema existence also does not prove that the desktop explicitly wrote the effective value: this code calls `g_settings_get_value`, not a user-value-only getter. See [xsettings.c:1091](/home/exec/Projects/github.com/emacs-mirror/emacs/src/xsettings.c:1091).

`font-get-system-font` returns the cached fixed-width string or nil; `font-get-system-normal-font` returns the distinct application-font string or nil. See [xsettings.c:1326](/home/exec/Projects/github.com/emacs-mirror/emacs/src/xsettings.c:1326).

For X frames, `x_default_font_parameter` first checks the explicit/default frame font argument. If absent, it tries `xsettings_get_system_font()` regardless of `font-use-system-font`. If opening the system font fails, it proceeds to the font resource and ultimately built-in fallbacks starting with `monospace-10`. Crucially, the final `gui_default_parameter(..., "font", "Font", ...)` reapplies resource lookup: its source comment explicitly says this makes X resources override the system-font suggestion. The earlier comment saying system font should precede resources describes the suggestion stage, not the final result. PGTK has the same sequence. See [xfns.c:4821](/home/exec/Projects/github.com/emacs-mirror/emacs/src/xfns.c:4821), [xfns.c:4878](/home/exec/Projects/github.com/emacs-mirror/emacs/src/xfns.c:4878), and [pgtkfns.c:1075](/home/exec/Projects/github.com/emacs-mirror/emacs/src/pgtkfns.c:1075). Lisp startup/default-face overrides require a separate trace.

### Initial selection versus later changes

`font-use-system-font` defaults to false and controls dynamic fixed-width-font updates. It is not an initial-discovery opt-out. A GSettings `changed` signal on `monospace-font-name` calls `store_monospaced_changed`, which always updates the cache but queues an event only when the first display is still valid and the option is enabled. GConf uses the same helper. See [xsettings.c:111](/home/exec/Projects/github.com/emacs-mirror/emacs/src/xsettings.c:111), [xsettings.c:416](/home/exec/Projects/github.com/emacs-mirror/emacs/src/xsettings.c:416), and [xsettings.c:1395](/home/exec/Projects/github.com/emacs-mirror/emacs/src/xsettings.c:1395).

The event becomes `(config-changed-event monospace-font-name DISPLAY)`. The Lisp handler checks the option again, clears the font cache, and calls `set-frame-font` for frames on the display, with semantics covering future frames too. `font-render` events instead recreate the current fonts; they do not switch to the cached system family. A `font-name` event is emitted for application-font changes, but this revision's Lisp handler has no corresponding branch. See [keyboard.c:7293](/home/exec/Projects/github.com/emacs-mirror/emacs/src/keyboard.c:7293) and [dynamic-setting.el:40](/home/exec/Projects/github.com/emacs-mirror/emacs/lisp/dynamic-setting.el:40).

On X11, Emacs watches the XSettings selection owner's property and destruction events, plus root-window manager announcements. A relevant event rereads and applies settings. See [get_prop_window](/home/exec/Projects/github.com/emacs-mirror/emacs/src/xsettings.c:548) and [xft_settings_event](/home/exec/Projects/github.com/emacs-mirror/emacs/src/xsettings.c:1046).

### Font-name parsing, points, pixels and DPI

The desktop string goes through `font_open_by_name` and Emacs's own font-spec/name parser. `font_parse_fcname` accepts Fontconfig and GTK-style names; for `Ubuntu Mono 13`, the trailing integer is stored as floating-point point size. It is not necessary to invoke Pango to parse this initial desktop string. The GTK-style branch scans backward over digits only, requiring a preceding space, so a raw trailing `13.5` is not recognized as a size by this revision. The Fontconfig-style `Ubuntu Mono-13.5` branch explicitly accepts a decimal point. The GTK font chooser separately uses Pango descriptions and converts Pango units to a floating-point `:size`. See [font.c:3542](/home/exec/Projects/github.com/emacs-mirror/emacs/src/font.c:3542), [font.c:1608](/home/exec/Projects/github.com/emacs-mirror/emacs/src/font.c:1608), [font.c:1642](/home/exec/Projects/github.com/emacs-mirror/emacs/src/font.c:1642), [font.c:1796](/home/exec/Projects/github.com/emacs-mirror/emacs/src/font.c:1796), and [gtkutil.c:2961](/home/exec/Projects/github.com/emacs-mirror/emacs/src/gtkutil.c:2961).

`font_pixel_size` treats an integer size as pixels and a floating-point size as points, using explicit font DPI if present or `FRAME_RES` otherwise. On Linux, the conversion is rounded `points * DPI / 72.27`; face heights are tenths of a point and are divided by 10 first. Therefore a reported 35-pixel font does not itself imply a desktop setting of 35 points. For example, 13 points at 192 DPI rounds to 35 pixels; this is arithmetic illustrating a possible mechanism, not evidence of the reporter's actual DPI. See [font.c:339](/home/exec/Projects/github.com/emacs-mirror/emacs/src/font.c:339), [font.h:561](/home/exec/Projects/github.com/emacs-mirror/emacs/src/font.h:561), [frame.h:990](/home/exec/Projects/github.com/emacs-mirror/emacs/src/frame.h:990), and [font.c:3256](/home/exec/Projects/github.com/emacs-mirror/emacs/src/font.c:3256).

On X11, initial resolution prefers `Xft.dpi` from X resources, otherwise derives it from X display pixel/mm dimensions (100 DPI fallback for invalid millimeter dimensions). XSettings can subsequently change it: `Xft/DPI` is divided by 1024; positive GDK unscaled-DPI and window-scale values together override that result with their product divided by 1024. `apply_xft_settings` applies DPI only if positive and more than 2 DPI different, updates the display resolution, and emits `font-render`. See [xterm.c:31470](/home/exec/Projects/github.com/emacs-mirror/emacs/src/xterm.c:31470), [xsettings.c:780](/home/exec/Projects/github.com/emacs-mirror/emacs/src/xsettings.c:780), [xsettings.c:806](/home/exec/Projects/github.com/emacs-mirror/emacs/src/xsettings.c:806), and [xsettings.c:954](/home/exec/Projects/github.com/emacs-mirror/emacs/src/xsettings.c:954).

PGTK instead initializes resolution with `gdk_screen_get_resolution`, falling back to 96 when negative, and skips Emacs's XSettings reader. It has separate GSettings-backed Cairo antialiasing/hinting/RGBA handling. The “Only use the gsettings font entries ... on PGTK” comment at line 1155 applies to these rendering options below it; the fixed-width font read above it also runs on eligible X11 builds. See [pgtkterm.c:7229](/home/exec/Projects/github.com/emacs-mirror/emacs/src/pgtkterm.c:7229), [xsettings.c:1155](/home/exec/Projects/github.com/emacs-mirror/emacs/src/xsettings.c:1155), and [xsettings.c:446](/home/exec/Projects/github.com/emacs-mirror/emacs/src/xsettings.c:446).

### KDE and issue #360: limits of attribution

GNU's bundled KDE troubleshooting note describes KDE 2/3 applying fonts/colors to non-KDE applications, including application-default resources and an old GTK theme engine. It is historical evidence that desktop integration can affect more than one channel, not documentation of current Plasma behavior. See [etc/PROBLEMS:1712](/home/exec/Projects/github.com/emacs-mirror/emacs/etc/PROBLEMS:1712).

The parent investigation reports an isolated experiment where GNU selected GSettings `Ubuntu Mono 13` and Neomacs retained its Fontconfig monospace default. That supports the missing desktop-font-discovery path as a mechanism. It does not establish which channel supplied the original KDE reporter's Ubuntu Mono font or DPI. The reporter's GNU backend (X11 versus PGTK) remains unknown in this investigation. Establishing provenance needs the reporter's effective `font-get-system-font`, frame font, X resources, build/backend, and resolution values.

Likewise, the reported agreement after explicitly selecting IBM Plex Sans at height 120 is consistent with the default-font inputs explaining the SVG difference. It does not by itself establish the original desktop value's provenance. Nil face remapping removes one override mechanism; it does not remove the initial desktop font or later default-face settings.

## Neomacs implementation and verification

The initial-font fix separates three responsibilities:

- `neomacs-display-runtime/src/desktop_fonts`: a Linux GIO adapter discovers effective preferences, without initializing GTK. Missing schemas/keys, non-string values and empty names produce absent preferences. Other platforms retain their existing fallback.
- `neovm-core/.../display_host/system_fonts.rs`: an owned `SystemFonts` snapshot distinguishes `SystemFontRole::{Monospace, Application}` and validates native names. Native settings handles do not enter the evaluator. Lisp font queries read this snapshot through `DisplayHost`.
- `neomacs/src/startup_font.rs`: the existing shared GNU-compatible name parser and typed `FrameFontSize`/`FontSizing` select one opened startup font. Its metrics seed the bootstrap frame, and the same opened font seeds the default face. Explicit Lisp configuration can subsequently replace it. The former second, incomplete frame-font-name parser was removed.

This implements **startup discovery**, not all of GNU's live desktop integration. The snapshot is not refreshed after startup; GSettings change events, legacy GConf, XSettings application-font discovery, and other platforms' discovery adapters remain follow-ups. Initial native window pixel dimensions are still requested before evaluator-side font selection; this patch does not claim GNU's exact initial character-grid/window-size parity. A future live-update adapter should publish typed preference changes to the evaluator, keep discovery independent of `font-use-system-font`, and let Lisp's opt-in control frame updates. It must not reapply the system font during redisplay.

### Red → green evidence (2026-09-12)

The tracked GUI test `neomacs-gui-tests/tests/desktop_font_startup.rs` compiles an isolated schema from `fixtures/desktop-fonts.gschema.xml`, uses the memory settings backend and launches a real release binary under headless Weston. Its Lisp fixture exercises the public font queries, the realized default face and `svg-tag-make`. The original binary failed with `expected Ubuntu Mono 13, got nil`; the rebuilt binary passes.

The separate two-editor diagnostic under `target/diagnostics/issue-360/startup-fonts` uses GNU X11 at 96 DPI and Neomacs Wayland at logical 96 DPI. It compares the entire SVG source and window font metrics, with no face remapping:

| Controlled input | Before fix | After fix |
| --- | --- | --- |
| Desktop `Ubuntu Mono 13` | GNU 9×18 / SVG 36.63×12.78; Neo 8×18 / SVG 32.56×12.78 | Both 9×18 / SVG 36.63×12.78, identical SVG source |
| Same desktop, explicit IBM Plex Sans height 120 | Both 8×22 / identical SVG source | Still identical |
| No desktop schema, Fontconfig monospace fallback | Both 8×18 / identical SVG source | Still identical |

Both editors report default face height 128 for the realized 17px Ubuntu Mono font. Independently, GNU `emacs -Q -fn monospace-10` under Xvfb at 100 DPI reports `(face-height pixel-size) = (101 14)`, confirming that startup face height must describe the rounded opened font rather than retain the nominal request of 100.

`cargo nextest` verification:

- Core font/remapping selection: **74 passed** (includes the new public Lisp tests for distinct roles, no-display nil, and GTK-style family/style/point-size parsing).
- Native desktop startup/SVG GUI regression: **1 passed**, after its captured failing run.
- Bootstrap default-face realization regression: **1 passed**, after updating its nominal-height expectation using the GNU oracle above.
- The broader Neomacs library run initially had 284 passes and six failures. The related face-height assertion is now fixed and passes; five failures remain outside this focused verification: three video tests reject an unsupported host with default features, one startup-error test fails its Messages assertion, and one GNU GUI startup test reports unsupported window system `neo`. These have not been established as baseline failures and are not claimed fixed here.

`cargo check -p neomacs` and `cargo build -p neomacs --release` succeeded. The rebuilt executable was sealed and its matching pdump regenerated through `loadup --temacs=pdump`, without byte compilation or autoload regeneration. `lisp/ldefs-boot.el` remained unchanged (SHA-256 `094b50fbd69032559bcac655d6368e16b8e69ae70b53989a99576ecd5c325138`). No GitHub comment was posted.
