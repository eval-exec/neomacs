# Weston cached-detach follow-up

On 2026-09-13, the unchanged tracked raw-protocol probe still reproduces the
cached-child detach error on installed Weston 15.0.0. The parent-commit control
passes. This is separate from the resolved slow 8K desktop-background readiness
problem and from Pixman crashes causing connection resets in other tests.

## Fresh reproduction

Repository HEAD inspected: `084aaec74c20d9446133266bcd85647fc08dfad7`.
The [existing probe](../../crates/neomacs-gui-tests/examples/wayland_subsurface_scale.rs)
was run with:

```sh
WAYLAND_DEBUG=client cargo run -p neomacs-gui-tests --example wayland_subsurface_scale -- cached weston
WAYLAND_DEBUG=client cargo run -p neomacs-gui-tests --example wayland_subsurface_scale -- before-detach weston
```

Each command creates and destroys its own stock headless Weston session. Both
used Pixman, 1280×800, output scale 1, and the repository's solid-background
`weston-headless.ini`. No real desktop session was used. Neomacs itself was not
rebuilt or launched. A second run of the resulting example executable confirmed
the same outcomes and recorded exit codes:

| Sequence | Result |
| --- | --- |
| Cached scale update, then null detach | Exit 101: probe panics on `wl_surface` error 2 |
| Apply cached scale update through parent before detach | Exit 0: detach completes |

The client trace identifies `wl_surface#7` and reports:

```text
surface size (833x44) was not an integer multiple of scale (2)
```

The applied child starts with an 833×44 buffer at scale 1. A valid 1666×88
buffer at scale 2 is then committed to the synchronized child and cached.
Without another parent commit, the client attaches null, commits the child,
and commits the parent. The failing validation therefore references the first
buffer's dimensions despite the intervening valid cached state. The control
changes only whether the parent applies the scale-2 state before detach.

Artifacts are under
`target/diagnostics/issue-360/startup-fonts/weston-cached-detach-followup/`:
`cached.log`, `before-detach.log`, their corresponding `*-weston-headless.log`,
`*-confirm.log`, and `exit-statuses.json`. These are local artifacts, not git
attachments. Both private compositor sessions exited; no compositor remains
running from this investigation.

## Protocol and source check

[Wayland 1.24's surface attachment contract](https://gitlab.freedesktop.org/wayland/wayland/-/blob/1.24.0/protocol/wayland.xml#L1462)
requires buffer dimensions divisible by scale and specifies that explicit null
attachment removes content at commit. Both non-null attachments satisfy the
divisibility rule. Its
[synchronized-subsurface contract](https://gitlab.freedesktop.org/wayland/wayland/-/blob/1.24.0/protocol/wayland.xml#L3230)
accumulates child commits until the parent applies them; it does not require a
parent commit between those child commits. The inferred final state is detached,
with no old content to validate at scale 2.

The probe does not reuse or mutate its three buffers. Destroying each shm pool
after buffer creation is supported because buffers retain the pool. Its
roundtrips establish request processing, not presentation. See the official
[buffer-pool](https://wayland.freedesktop.org/docs/html/apa.html#protocol-spec-wl_shm_pool)
and [display sync](https://wayland.freedesktop.org/docs/html/apa.html#protocol-spec-wl_display)
contracts. The roleless parent provides no visible desktop-window evidence.

In [Weston 15 `surface_attach`](https://gitlab.freedesktop.org/wayland/weston/-/blob/15.0.0/libweston/compositor.c#L4759),
null attachment sets `WESTON_SURFACE_DIRTY_BUFFER`. However,
[`surface_commit`](https://gitlab.freedesktop.org/wayland/weston/-/blob/15.0.0/libweston/compositor.c#L5030)
tests every null pending-buffer pointer against dimensions reconstructed from
the applied surface and the pending scale. That branch ignores whether an
explicit detach was requested. It rejects the 833-pixel width before calling
the state-commit machinery.

[State merging](https://gitlab.freedesktop.org/wayland/weston/-/blob/15.0.0/libweston/surface-state.c#L610)
does distinguish attachment changes: a dirty buffer replaces the cached buffer,
including replacement with null.
[`weston_surface_commit`](https://gitlab.freedesktop.org/wayland/weston/-/blob/15.0.0/libweston/surface-state.c#L831)
merges synchronized child state into its cache and waits for parent application.
Thus the valid detach cannot reach the layer that would correctly replace cached
content. This is a source-supported diagnosis, not upstream maintainer confirmation.

The same null-buffer validation branch remains in upstream source at
[`ac7bf0d16bda9786df058e44d5edce23756cc496`](https://gitlab.freedesktop.org/wayland/weston/-/blob/ac7bf0d16bda9786df058e44d5edce23756cc496/libweston/compositor.c#L5451),
the `main` revision retrieved during this investigation (commit dated
2026-09-11). This was source inspection only; upstream main was not built or
tested. Downloaded pinned sources and their SHA-256 values are retained beside
the logs. The prior [official-source audit](2026-09-13-weston-official-docs-audit.md)
contains historical issue/MR searches; those searches were not repeated here.

## Ownership and recommendation

The evidence continues to identify compositor validation as the repair boundary.
Explicit detach must be distinguished from an omitted attachment; retained-content
validation must account for effective cached/queued state. Validating every
null pending pointer against the applied buffer loses those distinctions.
A compositor regression should retain rejection of actually nondivisible
buffer/scale combinations while covering cached detach and retained-buffer cases.

Keep the raw example and this diagnosis as the local reproducible artifact.
The successful extra-parent-commit control establishes causality; it is not a
recommended production workaround. No Neomacs timing delay, forced parent commit,
renderer change, patched Weston, system-compositor replacement, or external post
was made. An upstream fix and its runtime verification remain external work;
this follow-up does not claim the retained Weston failure is fixed.

## Independent peer resets: compositor crash evidence

System coredumps identify the three previous private test compositor resets as
SIGSEGV, not an inferred timeout or memory-pressure kill. PIDs 3268232 and
3268233 at 03:21:56 EDT belong to the HiDPI resize/startup runs; PID 4010975
at 04:28:56 EDT belongs to `native-scale-startup`. All use stock Weston 15,
the Pixman renderer and output scale 2. Each captured stack begins with
`fast_composite_scaled_bilinear_sse2_8888_8888_pad_OVER`, then
`pixman_image_composite32`, Weston `repaint_region` and `repaint_surfaces`.

These stacks localize the failure to compositor image rendering. They do not
yet establish the invalid image state or a source-level fix. Unlike the cached
detach case, this is a compositor process crash rather than a client protocol
error. Serial GUI reruns passed, but that observation does not prove concurrency
caused the crash. No client delay or compositor patch was added.
