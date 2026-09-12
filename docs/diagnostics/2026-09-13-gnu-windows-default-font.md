# GNU Emacs Windows default font: source findings

Inspected read-only GNU checkout `/home/exec/Projects/github.com/emacs-mirror/emacs`, commit `a360712c9d272d950d8d8255ef74570f7e90b7d9`, on 2026-09-13. GNU's `AGENTS.md` permits search and analysis but prohibits generated contributions; this note is private analysis in Neomacs, not a contribution or report to GNU. No Windows runtime was available; conclusions below are source observations unless marked otherwise.

## Initial editor font

Native Windows does **not** use a desktop monospace preference in the inspected default-selection function. [`w32_default_font_parameter`, w32fns.c:6205](/home/exec/Projects/github.com/emacs-mirror/emacs/src/w32fns.c:6205) resolves an explicit font first, then a `font`/`Font` resource. When the result is not a string, it tries these names in order, stopping at the first font that opens:

1. `Courier New-10`.
2. `-*-Courier-normal-r-*-*-13-*-*-*-c-*-iso8859-1`.
3. `-*-Fixedsys-normal-r-*-*-12-*-*-*-c-*-iso8859-1`.
4. `Fixedsys`.

Failure of all candidates raises `No suitable font was found`. The resulting default goes through `gui_default_parameter`. The Windows frame constructor registers font drivers and calls this function before setting geometry-related defaults ([w32fns.c:6414](/home/exec/Projects/github.com/emacs-mirror/emacs/src/w32fns.c:6414)).

The shared argument helper documents and implements precedence as supplied frame alist, `default-frame-alist`, then GUI resources ([frame.c:6367](/home/exec/Projects/github.com/emacs-mirror/emacs/src/frame.c:6367)). Windows resources search the in-memory resource database, then—unless resources are inhibited—registry resources ([w32reg.c:145](/home/exec/Projects/github.com/emacs-mirror/emacs/src/w32reg.c:145)). Registry lookup checks current user before local machine under `SOFTWARE\\GNU\\Emacs` ([w32reg.c:29](/home/exec/Projects/github.com/emacs-mirror/emacs/src/w32reg.c:29), [w32reg.c:78](/home/exec/Projects/github.com/emacs-mirror/emacs/src/w32reg.c:78)). These are Emacs configuration resources, not an operating-system editor-font preference. This trace does not independently establish the preceding Lisp construction of the initial-frame alist.

## Size and DPI

The `-10` in the first candidate is a point size: the fontconfig-style parser stores that name's numeric size as a floating-point point size ([font.c:1691](/home/exec/Projects/github.com/emacs-mirror/emacs/src/font.c:1691)). The Windows font conversion treats integer font-spec sizes as pixels and floating-point sizes as points, converting through DPI and `72.27` ([w32font.c:2141](/home/exec/Projects/github.com/emacs-mirror/emacs/src/w32font.c:2141)). The XLFD alternatives explicitly specify pixel sizes 13 and 12. Windows display initialization obtains resolution using `GetDeviceCaps(..., LOGPIXELSX/LOGPIXELSY)` ([w32term.c:8056](/home/exec/Projects/github.com/emacs-mirror/emacs/src/w32term.c:8056)). Therefore preserving the selected font's size units matters; `Courier New-10` must not become a fixed 10-pixel font.

## Lisp system-font queries and updates

Repository search found `font-get-system-font` and `font-get-system-normal-font` definitions in `xsettings.c` and `haikufont.c`, and none in the native Windows sources. Their X-settings definitions mean **fixed-width editor font** and **normal application font**, respectively ([xsettings.c:1326](/home/exec/Projects/github.com/emacs-mirror/emacs/src/xsettings.c:1326)). Haiku implements both against distinct default and fixed font families ([haikufont.c:1330](/home/exec/Projects/github.com/emacs-mirror/emacs/src/haikufont.c:1330), [haikufont.c:1369](/home/exec/Projects/github.com/emacs-mirror/emacs/src/haikufont.c:1369)). A case-insensitive repository search found no literal `normalfont` identifier; it should not be treated as a separate Windows API without another reference.

The shared dynamic-setting Lisp checks `fboundp` before calling `font-get-system-font`, and follows `monospace-font-name` changes only when `font-use-system-font` is non-nil ([dynamic-setting.el:40](/home/exec/Projects/github.com/emacs-mirror/emacs/lisp/dynamic-setting.el:40)). Windows forwards `WM_SETTINGCHANGE` to the Lisp thread ([w32fns.c:5655](/home/exec/Projects/github.com/emacs-mirror/emacs/src/w32fns.c:5655)), whose handler updates only mouse-wheel scroll lines ([w32term.c:5240](/home/exec/Projects/github.com/emacs-mirror/emacs/src/w32term.c:5240)). No desktop-font subscription is present in that path. The manual describes system-font following for GNOME and Haiku, with `font-use-system-font` nil by default ([frames.texi:670](/home/exec/Projects/github.com/emacs-mirror/emacs/doc/emacs/frames.texi:670)). This is not a full audit of all Windows DPI-change behavior.

## Implication for Neomacs

Inference from the GNU trace: a Linux GSettings implementation cannot supply native Windows desktop preferences by itself. Matching GNU Windows initial behavior would instead require the Windows fallback and resource/explicit-font precedence above. Defining a Windows system-font preference reader would be an additional design choice, not parity with the native GNU Windows default-selection path inspected here. This note does not audit Neomacs's platform gates or native macOS selection.
