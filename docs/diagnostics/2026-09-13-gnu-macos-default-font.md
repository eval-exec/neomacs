# GNU Emacs macOS initial GUI font

Inspected read-only GNU checkout revision `a360712c9d272d950d8d8255ef74570f7e90b7d9`, 2026-09-13. This is Neomacs research, not an upstream contribution. No macOS runtime verification was possible on this Linux machine.

## Native selection, not Linux settings

GNU's Cocoa frame creation calls `[NSFont userFixedPitchFontOfSize: -1.0]`, obtains its display name, removes a trailing ` Regular`, and uses the result as the default `font` parameter. It seeds `fontsize` with zero, explicitly **not** the AppKit object's point size. GNUstep instead supplies `fixed`. Source: [nsfns.m:1322](/home/exec/Projects/github.com/emacs-mirror/emacs/src/nsfns.m:1322). Tooltip-frame creation repeats this policy at [nsfns.m:3008](/home/exec/Projects/github.com/emacs-mirror/emacs/src/nsfns.m:3008).

The AppKit API returns the user's default fixed-pitch document font; zero or a negative size requests the default size. This is not the same API as the monospaced system UI font. Source: [Apple NSFont documentation](https://developer.apple.com/documentation/AppKit/NSFont). Do not hard-code Monaco, Menlo or SF Mono as a substitute for this lookup.

The supplied font is only a fallback: `gui_default_parameter` first checks frame arguments, `default-frame-alist`, and platform resource lookup. Sources: [frame.c:6381](/home/exec/Projects/github.com/emacs-mirror/emacs/src/frame.c:6381), [frame.c:6514](/home/exec/Projects/github.com/emacs-mirror/emacs/src/frame.c:6514).

## Realization and units

Cocoa registers the `macfont` driver, not GNUstep's `nsfont` driver: [nsfns.m:1312](/home/exec/Projects/github.com/emacs-mirror/emacs/src/nsfns.m:1312). `macfont_open` calls `CTFontCreateWithName`, then reads the realized point size back into the font object: [macfont.m:2664](/home/exec/Projects/github.com/emacs-mirror/emacs/src/macfont.m:2664). Apple documents a default of 12 points **when this Core Text API receives size zero**; this conditional fact should not be confused with copying the size of the earlier AppKit font object. Source: [CTFontCreateWithName](https://developer.apple.com/documentation/coretext/ctfontcreatewithname(_:_:_:)).

GNU NS fixes logical resolution to 72.27, so its printer's points equal logical display units: [nsterm.m:5725](/home/exec/Projects/github.com/emacs-mirror/emacs/src/nsterm.m:5725). Backing scale is obtained separately from the window: [nsterm.m:827](/home/exec/Projects/github.com/emacs-mirror/emacs/src/nsterm.m:827). Retina scale must not be multiplied into the semantic point-size request a second time.

## Implication for Neomacs

The current change implements Linux-only preference discovery in `neomacs-display-runtime/src/desktop_fonts/mod.rs`; other targets return an empty snapshot. Shared parsing and realization improvements are cross-platform, but they do not constitute macOS native default selection.

A macOS adapter should obtain the native fixed-pitch name on the appropriate native thread and return an owned typed request, with backend-default size distinct from an explicit positive point size. The shared selection policy should distinguish an actual desktop preference from a platform fallback and from an explicit user override. GNU's `font-get-system-font` functions are defined in `xsettings.c` and `haikufont.c`, not its Cocoa font backend; do not equate Cocoa's native initial-font lookup with those Linux Lisp queries.

This note records source behavior and a proposed Neomacs design direction only. It does not claim native macOS tests passed or implement a macOS adapter.
