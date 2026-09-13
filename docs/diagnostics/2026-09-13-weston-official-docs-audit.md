# Weston official documentation audit: scale changes and subsurface detachment

## Scope and conclusion

The user challenged the earlier compositor diagnosis and explicitly rejected
building a patched Weston. This audit used official online documentation,
version-pinned protocol/source, and the upstream GitLab API. No compositor was
patched or built; no external issue or comment was posted.

The narrow raw-protocol reproduction remains consistent with a Weston 15
validation defect. It does **not** establish that every Neomacs/winit scale,
resize, or presentation decision is correct. The earlier blanket exoneration
of the client stack was too broad.

## Protocol requirements versus the standalone probe

The authoritative version checked was [Wayland 1.24.0
`protocol/wayland.xml`](https://gitlab.freedesktop.org/wayland/wayland/-/blob/1.24.0/protocol/wayland.xml),
especially `wl_surface.attach`, `set_buffer_scale`, `commit`,
`wl_subcompositor.get_subsurface`, and `wl_subsurface.set_sync`.

| Potential misuse | Finding |
| --- | --- |
| Missing parent commit between child commits | Synchronized child commits accumulate; application waits for the parent. A parent commit after every child commit is not required. |
| Invalid scale/buffer pairing | The probe commits 833×44 at scale 1, then 1666×88 at scale 2. Both pairings are divisible. |
| Null attachment retaining the old buffer | Explicit null attachment removes content; it is not equivalent to omitting `attach`. |
| Roleless parent | No parent-role prerequisite appears in `get_subsurface`. A roleless parent cannot establish visible desktop mapping. The child **does** have the subsurface role. |
| Scaling above the output's scale | Matching the output scale is intended for efficiency, not a requirement that every positive client scale equal it. |

The inference is that the final detached state has no content whose odd width
could violate scale 2. This is a protocol interpretation, not an upstream
maintainer confirmation.

The probe retains three distinct `wl_buffer` objects and never reuses or
mutates their storage. Destroying the shm pool immediately after buffer
creation is expressly supported; buffers retain the pool. Thus ignoring
release events in this non-reusing probe does not introduce early buffer
reuse. [Official protocol reference: shm pools and buffer
lifetime](https://wayland.freedesktop.org/docs/html/apa.html#protocol-spec-wl_shm_pool).

The probe's roundtrips are request-processing barriers, not parent commits or
proof that anything was displayed. Its parent is deliberately roleless, and
its buffers are transparent. Passing on KDE is useful differential evidence,
not a specification oracle or visual fullscreen test. See the [tracked
probe](../../crates/neomacs-gui-tests/examples/wayland_subsurface_scale.rs).

## Weston 15 source check

In [`surface_commit`, Weston
15.0.0](https://gitlab.freedesktop.org/wayland/weston/-/blob/15.0.0/libweston/compositor.c#L5030),
the `!buffer` validation branch checks dimensions derived from applied surface
state against pending scale. It does not first distinguish explicit null
attachment from absent attachment. `surface_attach` separately records the
attachment dirty flag, including null attachments.

The [surface-state
implementation](https://gitlab.freedesktop.org/wayland/weston/-/blob/15.0.0/libweston/surface-state.c)
merges committed synchronized state into the child's cache. Its merge logic
preserves explicit null attachment as a buffer-state change. Therefore the
validation runs against older applied dimensions before this valid detach
can reach the cache. This explains the raw probe's old 833×44 error despite
the intervening valid 1666×88 commit. The result does not depend on GPU
rendering, fullscreen, or a high-resolution output.

This source reading supports a specific validation mismatch. It is not a
reason to disable divisibility checks generally, force parent commits, or
change Neomacs's frame model.

## Official upstream discussions found

- [Weston issue 367](https://gitlab.freedesktop.org/wayland/weston/-/issues/367)
  and [MR 531](https://gitlab.freedesktop.org/wayland/weston/-/merge_requests/531)
  discuss why nondivisible buffer sizes must be rejected, with a viewport
  destination-size exception. They establish that this error can identify
  genuine client misuse; the message alone cannot assign blame.
- [MR 1943](https://gitlab.freedesktop.org/wayland/weston/-/merge_requests/1943),
  merged 2026-01-22, corrects stale transformed dimensions after a scale or
  transform change without a new buffer. Its regression splits buffer
  attachment and transformation across commits. Its change is already
  present in Weston 15's `surface-state.c`. It does **not** fix or confirm
  this cached-null-attachment case.
- [Issue 1077](https://gitlab.freedesktop.org/wayland/weston/-/issues/1077)
  documents a separate case where Weston accepted an invalid xdg-shell
  startup sequence that other compositors rejected. This is a concrete
  caution against treating one compositor's acceptance as proof of client
  correctness. The raw probe creates no xdg role, so that startup handshake
  does not apply to it.

Searches of official issue/MR titles and descriptions for scale, buffer, and
subsurface did not identify an exact upstream report for this detach
sequence. That is not proof none exists: discussion comments and older
paginated results were not exhaustively searched. Browser retrieval of some
GitLab pages failed; read-only `curl` requests to the official raw-file and
GitLab API endpoints succeeded.

## Headless configuration: an actual harness mistake

Weston's [running
documentation](https://wayland.pages.freedesktop.org/weston/toc/running-weston.html)
explicitly supports headless use for testing. The [official
manual](https://cgit.freedesktop.org/wayland/weston/tree/man/weston.man)
describes headless width, height, and scale arguments, but its generic
“pixels” wording does not disambiguate logical versus physical dimensions.

Weston 15's [`headless_output_set_size`
implementation](https://gitlab.freedesktop.org/wayland/weston/-/blob/15.0.0/libweston/backend-headless/headless.c#L483)
multiplies configured width and height by output scale for the physical
mode. Consequently:

| Arguments | Physical output | Logical extent |
| --- | --- | --- |
| `--width=1920 --height=1080 --scale=2` | 3840×2160 | 1920×1080 |
| `--width=3840 --height=2160 --scale=2` | 7680×4320 | 3840×2160 |

The earlier label calling the second case 4K was our harness/documentation
mistake. Correct dimensions are important, but do not explain the standalone
failure: it also reproduces on the standard 1280×800 scale-1 headless output.
The [preceding diagnostic](2026-09-13-weston-subsurface-scale-detach.md)
records those local observations.

## Safe next boundary

Keep the compositor unchanged. Preserve the narrow raw reproduction and
audit Neomacs's actual scale-change event ordering and parent presentation
against winit's contract separately. Do not turn a suspected compositor
defect into an unconditional client workaround.

## Neomacs integration audit and fresh mapped-window traces

The primary agent reran the actual GUI cases with `WAYLAND_DEBUG=client`
against the unchanged release executable and installed Weston. The 4K/2x
decorated test passes; the 8K/2x decorated test still fails. Logs:
`target/diagnostics/issue-360/startup-fonts/decoration-docs-trace.log` and
`decoration-docs-4k-control.log`; per-scenario protocol traces remain under
`target/neomacs-gui-tests/native-resize-decorations-{8k,hidpi}/linux-wayland/`.

In the 8K trace, the real parent `wl_surface#20` attaches and commits its first
buffer at timestamp 1998439.786. Its requested frame callback `#122` does not
complete before failure, and there is no subsequent parent buffer commit.
The CSD child `#24` commits valid scale-2 buffers at 1998442 and 1999488, then
explicitly attaches null at 2000489. The error still reports old 833x44
dimensions. In the 4K control the parent commits repeatedly and receives frame
callbacks. This links the raw cached-state sequence to the mapped GUI case,
but leaves the cause of the 8K callback/presentation stall unresolved.

The main presentation path calls `window.pre_present_notify()` after drawing
and immediately before `queue.present(output)` on the corresponding surface.
That matches both the pinned winit source contract (`winit-core/src/window.rs`,
lines 759–792) and [wgpu 30's Wayland presentation
contract](https://docs.rs/wgpu/30.0.1/wgpu/struct.Queue.html#method.present).
No missing pre-present notification was found in this path.

There is a separate Neomacs concern: `window_events.rs:ScaleFactorChanged`
updates the scale, then reports geometry using the still-old native content
size. The trace exposes a transient 373x345 logical resize at scale 2 before
the settled 745x688 resize. The pinned winit event contract permits accepting
the default suggested size without writing `SurfaceSizeWriter`; ignoring that
writer is not itself misuse. Treating the scale event as already-applied
resized geometry is the behavior that needs a focused regression. This is
not needed to trigger the raw Wayland failure and has not been proved to
cause the CSD error. No production fix was made during this research turn.
