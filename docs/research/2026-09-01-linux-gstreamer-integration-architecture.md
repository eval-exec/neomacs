# Optional Linux GStreamer integration: architecture review

Date: 2026-09-01

## Decision

The default long-term design should be **normal Rust linkage to GStreamer in a full build, plus a separately built minimal Neomacs without video**. This keeps GStreamer behind Neomacs's existing Rust domain interface while deleting a custom unsafe ABI, runtime discovery, and a second Rust runtime image.

Implementation status: accepted and implemented. The Linux backend now lives
directly in `neomacs-video`; the former adapter and ABI crates were deleted.

Retain the private `libneomacs_video_gstreamer.so` only if this product requirement is firm:

> The exact same `neomacs` executable must start and provide batch/TUI operation on a machine with neither the adapter nor GStreamer installed, and acquire video later when the adapter is installed.

At the start of this investigation, the no-GStreamer CI job built one release
executable without the adapter, verified that the executable had no `libgst*`
`DT_NEEDED`, and ran batch and TUI in a GStreamer-free container. The accepted
implementation preserves that test as the separately built minimal product;
it no longer treats the full and minimal artifacts as one product. Normal
direct linkage cannot make the full executable itself tolerate absent
libraries: the ELF dynamic loader finds and loads the objects needed by a
program before running it, while a `dlopen`ed object's dependency closure is
loaded only when that object is opened ([`ld.so(8)`](https://man7.org/linux/man-pages/man8/ld.so.8.html), [`dlopen(3)`](https://man7.org/linux/man-pages/man3/dlopen.3.html)).

Therefore:

- If “optional” may mean **a full package and a minimal package**, use direct linkage and compile-time variants.
- If “optional” must mean **one binary whose runtime gains/loses video**, the current private adapter is justified and is the best of the runtime-loading choices reviewed here.
- Moving GStreamer out of process is a separate security/reliability project, not a simpler way to make it optional.

## Facts that constrain the design

1. GStreamer 1.x is already an API- and ABI-stable series. The private Neomacs library is therefore not needed to shield the editor from routine GStreamer ABI churn ([GStreamer releases](https://gstreamer.freedesktop.org/releases/), [GStreamer versioning FAQ](https://gstreamer.freedesktop.org/documentation/frequently-asked-questions/developing.html)).
2. The official Rust bindings are intended for both GStreamer applications and plugins. They require the corresponding system development libraries to build; actual format coverage additionally depends on separately distributed plugin families ([gstreamer-rs installation](https://gstreamer.pages.freedesktop.org/gstreamer-rs/stable/latest/docs/gstreamer/), [Linux installation](https://gstreamer.freedesktop.org/documentation/installing/on-linux.html)).
3. Neomacs is an application consuming decoded samples. GStreamer's `appsink` exists specifically to let an application obtain pipeline data ([appsink](https://gstreamer.freedesktop.org/documentation/app/appsink.html)). Neomacs is not currently adding a reusable source, filter, sink, decoder, or other `GstElement` to the GStreamer ecosystem.
4. A Rust ABI cannot be the dynamic boundary: Rust's native ABI has no stability guarantee, whereas `extern "C"` selects the platform C ABI ([Rust Reference: external blocks](https://doc.rust-lang.org/reference/items/external-blocks.html)). Cargo's `cdylib` is the artifact intended for a dynamic system library loaded across a foreign boundary ([Rust Reference: linkage](https://doc.rust-lang.org/reference/linkage.html)).

## Alternatives

| Design | Missing GStreamer | Compile-time checking | Crash isolation | Relative complexity | Neomacs fit |
|---|---|---|---|---|---|
| Direct Rust linkage | Full executable cannot start | Best; ordinary Rust types and linker checks | None | Lowest | Preferred if full/minimal builds are acceptable |
| Private runtime-loaded Rust `cdylib` | Same executable still starts | Strong within each side; unsafe C ABI at one narrow seam | None | Medium | Justified only for one-binary runtime optionality |
| Directly `dlopen` upstream GStreamer | Same executable still starts | Weakest; every used symbol needs manual typing/resolution | None | High | Reject |
| GStreamer plugin | Core GStreamer is still needed by the application | Good inside plugin | None | Medium/high | Wrong extension point for the current job |
| Out-of-process helper | Main executable still starts | Versioned IPC can be typed locally | Strongest | Highest | Reserve for an explicit sandbox/crash-containment goal |

### 1. Direct Rust linkage to system GStreamer

This is the conventional application design. Make the Linux implementation of `neomacs-video` depend normally on the GStreamer Rust crates, keep Neomacs's platform-neutral model and backend trait as the internal seam, and let Cargo/rustc/the system linker check the complete call graph.

Its only architectural drawback here is decisive: the full executable's ELF dependency closure includes GStreamer, so a missing runtime library prevents all startup, even when the user only requested `--batch` or `-nw`. This is correct if the full package promises video and declares all runtime dependencies.

The clean optionality model is then two build outputs:

- `neomacs` (or `neomacs-full`): `video` enabled and directly linked;
- `neomacs-minimal`: `video` disabled and no GStreamer build/runtime closure.

This is preferable unless users truly need to add/remove video from one already-built executable. It makes Rust enums, ownership, and traits effective across the whole implementation rather than serializing them into numeric C records.

### 2. Private runtime-loaded Rust `cdylib`

The former structure at baseline commit `cc80c1e` was disciplined:

- [`neomacs-video`](https://github.com/eval-exec/neomacs/blob/cc80c1e61cc575d22f4bfb6dc670c9a4e388cde2/neomacs-video/src/platform/linux/loader.rs) owned discovery and library lifetime but had no GStreamer dependency;
- [`neomacs-video-backend-abi`](https://github.com/eval-exec/neomacs/blob/cc80c1e61cc575d22f4bfb6dc670c9a4e388cde2/neomacs-video-backend-abi/src/lib.rs) contained only `#[repr(C)]` records, integer tags, opaque handles, and function pointers;
- [`neomacs-video-gstreamer`](https://github.com/eval-exec/neomacs/blob/cc80c1e61cc575d22f4bfb6dc670c9a4e388cde2/neomacs-video-gstreamer/src/lib.rs) linked GStreamer normally, exported one versioned C entry point, and caught Rust panics at every foreign call boundary;
- DMA-BUF ownership crosses the seam by duplicating file descriptors, and the host retains the library while its function pointers and frame handles can exist.

This is the right shape **if one-binary runtime optionality is a hard requirement**. It localizes manual FFI to a Neomacs-sized command/event/frame protocol, while the adapter continues to use checked gstreamer-rs APIs and ordinary linker resolution internally. `dlopen` recursively loads the adapter's declared dependencies, so Neomacs does not have to reproduce GStreamer's linker surface ([`dlopen(3)`](https://man7.org/linux/man-pages/man3/dlopen.3.html)).

Costs remain substantial:

- every model change needs an encoder/decoder and unknown-value handling;
- the boundary is `unsafe`, and malformed trusted code can still violate pointer/size contracts;
- `usize` fields make this a target-architecture ABI, not a portable wire protocol;
- two Rust artifacts contain separate copies of Rust dependencies/runtime support;
- there is no process isolation: an adapter, codec, driver, or GStreamer plugin crash still terminates Neomacs.

Treat this as a **private, same-release adapter ABI**, not a public third-party plugin SDK. The ABI version should reject mismatches; it should not imply compatibility across independently upgraded packages.

### 3. Directly `dlopen`ing upstream GStreamer

Reject this option. It replaces the generated `gstreamer-sys` link contract with a hand-maintained table of `dlsym` calls spanning GStreamer core, app, video, allocators, GLib, and GObject. Foreign declarations are unchecked imports in Rust, so symbol names and signatures become Neomacs's responsibility ([Rust Reference: FFI safety](https://doc.rust-lang.org/reference/items/external-blocks.html)). It still loads the same GStreamer dependencies and runtime plugins, but loses normal link-time failure and much of the value of gstreamer-rs.

GStreamer's stable 1.x ABI makes manual resolution even less defensible. If runtime optionality is required, load one narrow Neomacs-owned adapter that itself links GStreamer normally.

### 4. A GStreamer plugin

Reject this as the primary boundary. GStreamer plugins encapsulate `GstElement` implementations—sources, filters, sinks, bins, and related registry features—and are loaded when their elements are requested ([plugin foundations](https://gstreamer.freedesktop.org/documentation/plugin-development/introduction/basics.html), [`GstPlugin`](https://gstreamer.freedesktop.org/documentation/gstreamer/gstplugin.html)). GStreamer's own guide says plugin development is not the relevant guide when an application only uses existing functionality ([plugin guide preface](https://gstreamer.freedesktop.org/documentation/plugin-development/introduction/preface.html)).

Neomacs presently manages playback sessions and receives frames through `appsink`; turning that application control plane into a registry plugin would not remove the application's need to initialize/link GStreamer. It would also expose renderer callbacks and Neomacs lifecycle concerns as a GStreamer element API.

A plugin becomes appropriate only if Neomacs develops a genuinely reusable element, such as a wgpu-backed sink meant to compose in arbitrary GStreamer pipelines. Even then, an application-specific element may be embedded and registered without a dynamically discovered plugin ([GStreamer compiling guide](https://gstreamer.freedesktop.org/documentation/application-development/appendix/compiling.html)); it does not replace the outer optional-dependency decision.

### 5. Out-of-process helper

This is the only option that adds a real crash and address-space boundary. It can also be packaged independently. However, it requires a versioned IPC protocol for commands, events, errors, clocks, seeking, lifecycle, and recovery. CPU frames incur extra transport/copy costs. Linux can pass DMA-BUF descriptors through Unix sockets using `SCM_RIGHTS`, but the receiver gets duplicated open-file references and the application must still define metadata, synchronization, device compatibility, and lifetime rules ([`unix(7)`](https://man7.org/linux/man-pages/man7/unix.7.html)).

Choose a helper only after stating an explicit threat/reliability objective such as sandboxing untrusted codecs or surviving decoder/plugin crashes. If chosen, implement a real helper against the GStreamer API; GStreamer explicitly says `gst-launch-1.0` is a debugging tool and applications should not be built on it ([`gst-launch-1.0`](https://gstreamer.freedesktop.org/documentation/tools/gst-launch.html)).

## Concrete Neomacs plan

### Preferred plan: compile-time full/minimal products

1. Confirm that “same executable starts without GStreamer” is not a required user-facing promise.
2. Keep the GStreamer implementation inside the Linux platform module of `neomacs-video`, reached through the existing typed Rust backend abstraction; remove the adapter crate, C ABI, and loader.
3. Publish explicit full and minimal packages/build outputs. Test the full ELF for the intended GStreamer dependencies and test the minimal ELF/container for their absence.
4. Let distro tooling calculate dependencies from the full executable. Do not manually suppress them. Codec availability remains a runtime capability: report missing GStreamer elements/plugins with actionable diagnostics rather than claiming that the core library supports every format. GStreamer documents a dedicated missing-plugin mechanism for applications and distributions ([install-plugins API](https://gstreamer.freedesktop.org/documentation/pbutils/gstpbutilsinstallplugins.html)).

### If the one-binary contract is retained

Keep the adapter, then harden its product and packaging contract:

1. **Package it separately.** A base `neomacs` package should contain the GStreamer-free executable. A matching `neomacs-video-gstreamer` package should contain the adapter, depend on the exact compatible Neomacs version, and carry automatically generated GStreamer shared-library dependencies. The base may `Recommends`/`Suggests` the adapter. Debian policy requires dependency calculation for every packaged executable, library, or loadable module, including `dlopen` users ([Debian Policy §8.6](https://www.debian.org/doc/debian-policy/ch-sharedlibs.html), [`dpkg-shlibdeps(1)`](https://manpages.debian.org/bookworm/dpkg-dev/dpkg-shlibdeps.1.en.html)).
2. **Install it as a private library.** For distro packages use an architecture-specific private directory such as `/usr/lib/<triplet>/neomacs/<version>/`, mode `0644`, stripped, with no public SONAME or linker-name symlink. Debian policy says non-public shared objects belong in a subdirectory of `/usr/lib` or `/usr/lib/<triplet>` and must not be executable ([Debian Policy §10](https://www.debian.org/doc/debian-policy/ch-files.html)). A relocatable tarball may use a private `$ORIGIN`-relative `lib/neomacs/<version>/<triple>` directory. Pass the resolved absolute path to the loader; never search the current working directory or a bare soname.
3. **Load fail-fast and locally.** Use `RTLD_NOW | RTLD_LOCAL`: `RTLD_NOW` rejects unresolved symbols before `dlopen` returns and `RTLD_LOCAL` avoids exporting adapter symbols to subsequently loaded objects ([`dlopen(3)`](https://man7.org/linux/man-pages/man3/dlopen.3.html)). Keep the adapter resident until all workers and opaque frames are destroyed; the current `LoadedBackend` lifetime discipline already does this.
4. **Keep the ABI small and closed.** Preserve C-compatible fixed-width tags, explicit version/size validation, opaque ownership, panic containment, and exhaustive Rust conversion into domain enums on both sides. Add capability/build identity only when the host needs to negotiate it; do not turn discovery into a general plugin marketplace.
5. **Test the installation states.** Cover absent adapter, missing adapter dependencies, incompatible ABI, truncated/missing operations, successful decode, missing codec plugin, and teardown with outstanding frames. Test the built distro package, not only `target/release`.

## Baseline repository findings and resolution

These packaging defects were present at baseline commit `cc80c1e`; the accepted
implementation resolves all five:

1. `scripts/package-deb.sh` shipped the adapter but wrote a manual `Depends` list with no generated GStreamer dependencies. It now derives full-product shared-library dependencies with `dpkg-shlibdeps`.
2. `scripts/package-rpm.sh` filtered all `libgst*.so.*` automatic requirements while shipping the adapter. That suppression is gone.
3. `scripts/package-release.sh` called the adapter “optional” while making it mandatory in every Linux release artifact. It now packages explicit full and minimal products.
4. Linux packaging installed the adapter `.so` with mode `0755` in GNU's `libexec` archlib. The adapter no longer exists.
5. `scripts/test-linux-release-artifacts.sh` required a stale ABI symbol. It now audits the full executable for direct GStreamer linkage and the minimal executable for the absence of any `libgst*` dependency.

The first two findings were highest priority: optional loading changes which
package owns the dependency; it does not make the adapter's own `DT_NEEDED`
libraries optional once the adapter package is installed.
