# Weston 15 cached-subsurface detach failure

## Finding

The 8K/2x Neomacs fullscreen regression has been reduced to a raw Wayland
protocol sequence. The standalone Rust probe uses neither Neomacs's evaluator
nor winit, sctk-adwaita, or wgpu. It also fails on a standard-resolution Weston
output: the GUI's 8K configuration exposes the timing but is not required by
the underlying protocol failure.

Probe: `crates/neomacs-gui-tests/examples/wayland_subsurface_scale.rs`.
It creates a roleless parent and a child with a subsurface role, never a visible
desktop window or focus request.
`CommitTiming` and `ProbeDisplay` are typed/strum-parsed diagnostic choices.
Its dependencies are Linux-only development dependencies of the GUI-test crate.

## Minimal sequence and controls

1. Apply a synchronized child surface with an 833x44 buffer at scale 1 by
   committing its parent.
2. Commit a valid 1666x88 buffer at scale 2 to the child without committing
   the parent. This state is cached.
3. Explicitly attach a null buffer and commit the child, then its parent.

| Compositor / sequence | Observed result |
| --- | --- |
| Weston 15.0.0, cached scale update followed by detach | Protocol error 2, old 833x44 dimensions rejected at scale 2 |
| Weston 15.0.0, commit parent after scale update and before detach | Success |
| Current KDE Wayland desktop, unchanged cached-update sequence | Success |

The Weston failure also reproduces with the standard 1280x800, scale-1
headless output. The child explicitly requests buffer scale 2; no monitor
scale event is needed.

Commands (from repository root):

```sh
cargo nextest run -p neomacs-gui-tests --test desktop_font_startup -E 'test(hidpi_8k)' --run-ignored all
cargo run -p neomacs-gui-tests --example wayland_subsurface_scale -- cached weston
cargo run -p neomacs-gui-tests --example wayland_subsurface_scale -- before-detach weston
cargo run -p neomacs-gui-tests --example wayland_subsurface_scale -- cached desktop
```

The first two commands intentionally reproduce failures with the installed
Weston; the last two controls succeed. This is a diagnostic example, not a
test that blesses the compositor's failure as correct behavior. The existing
public-Lisp GUI regression remains failing on that compositor.

## Source explanation

The [Wayland protocol](https://gitlab.freedesktop.org/wayland/wayland/-/blob/1.24.0/protocol/wayland.xml)
defines explicit null-buffer attachment as content removal. Synchronized
subsurface commits accumulate in a cache until the parent's state is applied.
The cached scale update and subsequent detach are therefore valid operations;
clients need not insert a parent commit between them.

In [Weston 15's `surface_commit`](https://gitlab.freedesktop.org/wayland/weston/-/blob/15.0.0/libweston/compositor.c#L5030),
the `!buffer` path validates the previously applied surface dimensions against
the pending scale. It does not distinguish an explicit detach from a commit
that retains content. With the scale-2 update still cached, the applied state
is the old scale-1 buffer, and its odd width triggers a spurious error.

GNU Emacs's `src/pgtkterm.c:set_fullscreen_state` (local source lines
4512–4545) calls `gtk_window_fullscreen` / `gtk_window_unfullscreen`; GTK owns
the decoration surface lifecycle. No evaluator-level resize or fullscreen
state workaround is indicated by this reproduction.

## Correct ownership and remaining work

The fix belongs in compositor surface-state validation: explicit detach must
not validate stale content, and any retained-content validation must use the
effective queued/cached state rather than blindly using the applied buffer.
A complete compositor regression should also preserve rejection of genuinely
invalid retained-buffer scale changes.

Do not force extra parent commits in Neomacs merely to make the headless test
pass: that changes synchronization semantics and conceals the compositor bug.
No production Neomacs, winit, sctk-adwaita, or system-compositor change was made
in this diagnosis. The user rejected building a patched Weston; that proposal
is not an approved next step. The subsequent
[official-documentation audit](2026-09-13-weston-official-docs-audit.md)
rechecks protocol legality, harness configuration, and possible client misuse.
No upstream PR or issue comment has been posted.

Artifacts are under `target/diagnostics/issue-360/startup-fonts/`:
`decoration-8k-red.log`, `decoration-raw-wayland.log`,
`decoration-standard-weston-red.log`, `decoration-parent-commit-control.log`,
and `decoration-desktop-control.log`.
