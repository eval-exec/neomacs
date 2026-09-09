# Native tooltip ownership and GNU Emacs compatibility

Research date: 2026-09-09. GNU Emacs source was read from the local checkout
`/home/exec/Projects/github.com/emacs-mirror/emacs` at commit
`a360712c9d272d950d8d8255ef74570f7e90b7d9`. References below pin that revision.
GTK 3.24.49 and the explicitly unpinned Wayland protocol snapshot are additional
primary sources. This is source research, not verification on other platforms.

## Two policy paths, not one renderer overlay

GNU Emacs enables `tooltip-mode` by default. Disabling it intentionally sends
help to the echo area. Its Lisp help path installs `tooltip-hide` on both
`pre-command-hook` and `x-pre-popup-menu-hook`. Initial delay is 0.7 seconds;
subsequent tips within one second of hiding use 0.1 seconds. Automatic hide is
10 seconds. `tooltip-show-help` cancels on nil, ignores equal help rather than
restarting a timer, and hides/rearms for changed help. The NS global-menu case
is explicitly suppressed. These are semantic compatibility rules, not a
reason to duplicate a second delay in the renderer.
[tooltip.el](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/lisp/tooltip.el)

`tooltip-show` preserves existing text faces while appending the tooltip face,
derives frame foreground/background/border colors, and calls `x-show-tip` with
the selected frame, parameters, timeout and offsets. Lisp defaults are +5/+20
pixels, distinct from the low-level +5/-10 defaults. Explicit echo-area mode
and non-graphical displays are intentional alternatives. Its error handler
reports the error, waits one second, then prints the text: that is an error
recovery path, not the normal native-tooltip implementation.
[tooltip.el: tooltip-show](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/lisp/tooltip.el#L242)

Importantly, PGTK menu items attach their help directly to the GTK widget with
`gtk_widget_set_tooltip_text`. Menu help therefore also has a toolkit-managed
path; it is incorrect to assume every GNU menu tooltip traverses the Lisp
delay machinery. GTK toolbar event boxes also permit tooltips on disabled
items. Neomacs should have one explicit policy owner per help source, not
independent Lisp and runtime timers racing to display the same help.
[gtkutil.c: menu-item creation and toolbar event boxes](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/gtkutil.c)

## Primitive contract and native lifetime

`x-show-tip` validates a string and graphical frame, normalizes an empty string
to a space, validates nonnegative integer timeout and integer offsets, and
uses `x-show-tooltip-timeout` when timeout is nil. Placement supports left/top
or right/bottom parameters; otherwise it uses pointer offsets. Maximum text
size is governed by `x-max-tooltip-size`. PGTK retains last owner, string,
parameters and hide timer. Identical visible content/parameters on the same
frame takes a reposition-and-renew-timer path, not destroy/recreate.
[xfns.c: x-show-tip contract](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/xfns.c#L9026),
[pgtkfns.c: x-show-tip](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/pgtkfns.c#L3025)

With system tooltips enabled, PGTK delegates text to the owning frame's GTK
widget; hiding clears that widget's tooltip text. With system tooltips
disabled, Emacs lays out a dedicated tooltip frame and creates a
`GTK_WINDOW_POPUP`. It resizes/moves/shows that window and renders its text,
instead of inserting pixels into the editor frame. `pgtk_hide_tip` cancels
the timer first, handles both system and Emacs tooltip lifetimes even after
the option changes, and returns whether a tip was open. Hidden frames may be
reused, but owner or incompatible appearance changes force replacement.
[gtkutil.c: xg_show_tooltip, xg_hide_tooltip, xg_create_frame_widgets](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/gtkutil.c),
[pgtkfns.c: pgtk_hide_tip and tooltip creation](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/pgtkfns.c)

GNU's custom placement checks monitor geometry, prefers the pointer-offset
position, flips to the opposite side if it fits, and clamps otherwise.
This source uses desktop coordinates; copying those assumptions into a
Wayland backend would be a design error. Preserve placement intent, not its
global-coordinate implementation.
[pgtkfns.c: compute_tip_xy](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/pgtkfns.c#L2845)

## Input and platform differences

GTK owns a current tooltip per display, tracks the originating widget and
context rectangle, and hides on button/key press, scrolling, drag enter,
broken grab, leave or changed context. It has distinct initial/browse timers.
These are explicit state transitions, not arbitrary motion debouncing.
[GTK 3.24.49 gtktooltip.c](https://github.com/GNOME/gtk/blob/3.24.49/gtk/gtktooltip.c)

X11 creates a root-child override-redirect tooltip window with a tooltip
window-type hint. Windows displays its tooltip with `SW_SHOWNOACTIVATE`.
NS's system path uses `EmacsTooltip`, a borderless `NSWindow` containing a
noneditable/nonselectable text field, ordered forward at popup-menu level
with its own hide timer. These are different host mechanisms behind similar
Lisp behavior, not evidence for a portable integer z-level API.
[xfns.c: tooltip window creation](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/xfns.c#L8584),
[w32fns.c: x-show-tip](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/w32fns.c#L7797),
[nsmenu.m: EmacsTooltip](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/nsmenu.m#L1423)

Wayland explicitly permits tooltips as `xdg_popup` surfaces. An explicit grab
is optional and changes input ownership; it requires a triggering event serial.
Parents must already be mapped. Popups stack above earlier related popups,
and destruction must respect topmost-first ordering. Placement/configure
coordinates are relative to the parent surface. Tooltip hosts therefore must
not inherit menu grabbing merely because both use popup surfaces.
[xdg-shell snapshot accessed 2026-09-09](https://github.com/wayland-mirror/wayland-protocols/blob/main/stable/xdg-shell/xdg-shell.xml)

## Recommended Neomacs contract (architectural inference)

1. Keep Lisp API validation, tooltip faces/text, timing and explicit echo-area
   behavior compatible. Implement the GUI primitive rather than removing its
   error handler or pretending the window is textual.
2. Separate help-source policy from presentation. A delayed Lisp request is
   already ready to show; a direct menu/toolbar hover request needs an owned
   deadline. Both become one presentation request, never two competing tips.
3. Give requests a generation and an actual owner surface plus parent-local
   anchor. The editor frame remains the semantic Lisp owner; a native menu
   popup can be the presentation parent. Late timers/replies cannot resurrect
   a tip after leave, click, owner replacement or destruction.
4. Model tooltip interaction separately from menus: passive/nonactivating,
   no explicit grab, no keyboard navigation, pointer-transparent where the
   backend supports it. Dismiss before changing/destroying its popup ancestor.
   A tooltip must never steal the menu's input session.
5. Share native surface creation, target-local scale/layout/rendering,
   configure/present and reverse-order disposal infrastructure. Do not share
   menu selection state or recreate an in-window tooltip overlay fallback.
6. Express offset/edge preference and constraints in the portable request;
   adapters implement compositor-native placement. Report unsupported desktop
   absolute placement honestly instead of inventing global Wayland positions.
7. Verify identical-help stability, stale timer rejection, owner-specific
   cancellation, nested-popup teardown, disabled-item help, tooltip-mode off,
   rich text/color behavior and main-frame render isolation. Linux Wayland
   runtime tests do not establish macOS/Windows/Android/browser support.
