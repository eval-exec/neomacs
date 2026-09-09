# Menu interaction ownership and popup jitter

Research date: 2026-09-09. Primary sources only; source versions are pinned below
except the explicitly marked Wayland protocol snapshot.

## Finding and scope

Mature menu implementations distinguish selection, the pending selection, and
popup lifetime. Repeated pointer motion does not mean repeated popup creation.
They also use timed submenu-navigation policies, but those solve accidental
switching while crossing neighboring rows, not duplicate opening of the same
menu. The evidence below supports repairing state ownership before adding any
pointer debounce.

The Neomacs investigation context reports 177 menu-open events while moving over
Help and Interactively. That is a local diagnostic observation supplied to this
research, not a conclusion independently established by external sources. This
note does not prove Neomacs's exact triggering transition or a compositor bug.

## GTK 3 and GNU Emacs GTK

GTK 3.24.49's `GtkMenuShell` is the shared interaction foundation for `GtkMenu`
and `GtkMenuBar`. Its terminology separates active menus from selected items
and describes a hierarchical current item and keyboard grab. Crucially,
`gtk_menu_shell_select_item` skips the selection callback when the shell is
already active with that same item. This is an explicit same-selection guard,
not a timeout. [GTK 3.24.49, gtkmenushell.c](https://github.com/GNOME/gtk/blob/3.24.49/gtk/gtkmenushell.c)

GTK separately maintains a submenu navigation triangle. Motion within that
region can leave the existing submenu open while the pointer travels toward
it; leaving the region ends that protection. `gtk_menu_leave_notify` also
filters GTK grab/ungrab/state-change crossing events rather than treating
every leave as an ordinary user exit. These are event-semantics and navigation
policies, not reasons to recreate a popup for every move.
[GTK 3.24.49, gtkmenu.c: gtk_menu_navigating_submenu, gtk_menu_leave_notify](https://github.com/GNOME/gtk/blob/3.24.49/gtk/gtkmenu.c)

GNU Emacs 30.2's GTK backend constructs GTK menus and attaches submenu widgets
with `gtk_menu_item_set_submenu`, appending items to a GTK menu shell. Thus the
comparison relevant to GNU Emacs's GTK menus is GTK's menu machinery, not an
editor-text rendering technique.
[Emacs 30.2, gtkutil.c: create_menus](https://github.com/emacs-mirror/emacs/blob/emacs-30.2/src/gtkutil.c)

## Qt Widgets

Qt 6.8.3's `QMenuBarPrivate::setCurrentAction` immediately returns when both the
current action and popup state are unchanged, before hiding the old menu or
opening another. Its `focusOutEvent` clears selection only when popup state is
false. This is a close match for the distinction Neomacs needs: losing menubar
focus while its popup is active is not equivalent to ending menu interaction.
The same file also contains a platform-menu bridge; these observations concern
the inspected Widgets interaction path, not a claim that every platform uses
identical native event handling.
[Qt 6.8.3, qmenubar.cpp: setCurrentAction, focusOutEvent](https://github.com/qt/qtbase/blob/v6.8.3/src/widgets/widgets/qmenubar.cpp)

Within `QMenu`, delayed popup handling checks that the requested submenu is not
already visible. `internalDelayedPopup` hides the previous active submenu only
if its action differs, and returns if the current action's submenu is visible.
Opening is therefore guarded independently of selection repainting.
[Qt 6.8.3, qmenu.cpp: popupAction, internalDelayedPopup](https://github.com/qt/qtbase/blob/v6.8.3/src/widgets/widgets/qmenu.cpp)

Qt's separate `QMenuSloppyState` tracks pointer direction, submenu geometry,
the origin/reset action, and timers. Its policy comes from style hints,
including close timeout and directional behavior. This is deliberately richer
than delaying all pointer events by an arbitrary constant. Timer ownership and
navigation intent have names and state of their own.
[Qt 6.8.3, qmenu_p.h: QMenuSloppyState](https://github.com/qt/qtbase/blob/v6.8.3/src/widgets/widgets/qmenu_p.h)

## Chromium Views

Inspected Chromium commit `2ea935d201ac9d5ae13b62728c37add6ac206cf2`, resolved
from the upstream repository during this research.

`MenuController` owns `state_` and `pending_state_`. `SetSelection` compares
selection paths and resets/restarts the show timer only when the pending item
or its submenu-open state changes. `CommitPendingSelection` closes the
divergent part of the old path, then opens the requested path. `OpenMenu`
returns immediately for an already-showing submenu. `OpenMenuImpl` supplies
`MenuHost::InitParams`, including owner, bounds, capture, and anchor/context;
the show and reposition paths are distinct. Callback/lifetime guards also
account for destruction triggered by accessibility or delegate callbacks.
These are explicit controller, view, and host responsibilities, not a single
render loop deciding visibility from the current pointer coordinates.
[Chromium MenuController source](https://chromium.googlesource.com/chromium/src/+/2ea935d201ac9d5ae13b62728c37add6ac206cf2/ui/views/controls/menu/menu_controller.cc)

## Wayland constraints belong in presentation

The inspected upstream `xdg-shell` XML declares interface version 6. It requires
a popup's parent to be mapped first, defines popup stacking, and requires
topmost-first destruction. Explicit grabs depend on a user-event serial and
give the topmost grabbing popup keyboard focus. `popup_done` reports compositor
dismissal. Since version 3, repositioning an already-mapped popup has its own
token/configure/acknowledgment sequence. Surface focus, dismissal, placement,
and destruction therefore cannot be inferred solely from editor hover state.
These are protocol requirements; support in Neomacs's current winit integration
must be verified separately.
[Wayland xdg-shell, upstream main snapshot accessed 2026-09-09](https://github.com/wayland-mirror/wayland-protocols/blob/main/stable/xdg-shell/xdg-shell.xml)

## Recommended Neomacs contract

The following is an architectural inference and proposed contract, not a claim
that the referenced projects use Neomacs's asynchronous Lisp protocol.

| Owner | Authoritative state and boundary |
| --- | --- |
| Menu interaction controller | Session, active heading, requested heading, selected item/path, pending switch, dismissal, navigation timers |
| Lisp evaluator bridge | Correlated content requests/replies and command execution; no independent pointer-hover lifecycle |
| Frame chrome | Hit geometry and display of controller state; transient hover is not the active heading |
| Presentation host | Native popup identity, parent chain, actual configured placement, grab/focus events, surface lifetime |
| Renderer | Scene painting into the supplied target; no menu-open or close decisions |

Use one reducer-like transition boundary for pointer, keyboard, content reply,
and native dismissal events. It should emit semantic effects, not mutate
visibility from several callers. Existing active and pending state must both
participate in duplicate suppression: while Help is requested but its contents
are still loading, another Help hover must not send a second request.

Every asynchronous response and native close event needs enough identity to
validate its ownership. Reject stale replies after a switch or dismissal; an
old surface's close must not end a newer pending session. This correlation is
additional to submenu intent timers, not implemented by those timers.

Keep the common prefix of the visible popup path alive. A highlight-only
change repaints; a submenu-path change closes/opens only the changed suffix.
Content or geometry changes may require reconciliation, and platform rules may
require replacement; do not promise eternal native handles across all changes.
Use actual configured geometry when implementing pointer-navigation corridors.

Keep platform capability differences explicit. Linux Wayland verification
does not establish correctness on X11, macOS, Windows, Android, or WASM. This
research does not justify an in-window fallback or a global numeric popup
z-index.

## Regression checks

1. Repeated hover on an active heading emits no new content request or native
   creation, including after editor hover is cleared or focus enters a popup.
2. Repeated hover on a pending heading emits one request; A → B → C with replies
   delivered out of order cannot reopen A or B.
3. Native dismissal/content callbacks from an old session cannot close the
   current request; dismissal cancels that session's pending timers.
4. Repeated motion within a submenu row preserves the surface identity of the
   unchanged chain. Changing a leaf selection does not recreate ancestors.
5. Keyboard selection, pointer selection, outside dismissal, and command
   activation all use the same controller transition boundary.
6. After lifecycle correctness, separately test diagonal submenu navigation,
   left-opening/RTL placement, scrolling, and scale/configure changes using
   deterministic time and explicit geometry.

Unit tests should observe requests and lifecycle effects, not just final
highlight colors. Native Wayland checks should count surface creation and
destruction alongside session/path transitions. Use `cargo nextest`, not
`cargo test`, for this repository's verification.
