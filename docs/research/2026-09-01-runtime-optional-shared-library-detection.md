# Runtime detection of an optional shared library

Date: 2026-09-01

> Superseded by the accepted full/minimal product decision in
> [Optional Linux GStreamer integration](2026-09-01-linux-gstreamer-integration-architecture.md).
> This note remains the analysis of the alternative one-executable requirement;
> Neomacs no longer builds or loads a private GStreamer shared object.

## Recommendation

If the **same Neomacs executable** must run when GStreamer is not installed, no GStreamer-referencing object can be in that executable's startup `DT_NEEDED` closure. Keep the GStreamer code in the private `libneomacs_video_gstreamer.so`, load that adapter lazily with `libloading` on the first video request, and degrade only the video capability when loading or initialization fails.

The private adapter is preferable to directly loading upstream GStreamer because it exposes one small, versioned Neomacs ABI while using normal gstreamer-rs linkage and type checking internally. An out-of-process helper is preferable only when codec/plugin crash containment or sandboxing is an explicit requirement; it is unnecessary complexity for dependency absence alone.

The authoritative runtime tests are operations, not probes:

1. ask the platform loader to load the exact adapter;
2. ask it for the exact versioned entry symbol;
3. validate the returned ABI table;
4. call GStreamer's fallible initialization API inside the adapter;
5. let GStreamer report media-specific missing plugins through its registry/pipeline bus.

## Five distinct failure stages

| Stage | Authoritative operation | Meaning | Recovery scope |
|---|---|---|---|
| Program startup | ELF interpreter resolves `DT_NEEDED` | A mandatory object or its closure is absent/incompatible | Neomacs code cannot recover; `main` has not run |
| Optional adapter load | `dlopen` / `Library::open` | Adapter or a transitive dependency could not be loaded/relocated | Disable video; editor continues |
| Adapter ABI | `dlsym` plus table version/size validation | A library exists but is not a compatible Neomacs adapter | Packaging/install mismatch; disable video |
| GStreamer initialization | `gst_init_check` / `gst::init()` | Libraries loaded, but GStreamer could not initialize | Backend unavailable; preserve diagnostic |
| GStreamer element/codec | registry lookup and pipeline bus messages | Backend works, but this element/protocol/media format is unavailable | Fail that media session, not the backend |

### 1. A missing `DT_NEEDED` library fails before `main`

For a dynamically linked ELF executable, the interpreter named by `PT_INTERP` loads the needed shared objects as part of execution. The dynamic loader's job is to find and load the objects needed by the program and only then run it ([`execve(2)`](https://man7.org/linux/man-pages/man2/execve.2.html), [`ld.so(8)`](https://man7.org/linux/man-pages/man8/ld.so.8.html)). Consequently, application logic cannot catch a missing direct GStreamer dependency. A Rust `Result` in `main`, a GStreamer feature test, or a panic handler all run too late.

This is why a directly linked full build and a no-GStreamer build can be two valid products, but one directly linked executable cannot dynamically tolerate the library's absence.

### 2. `dlopen` is the runtime availability test

POSIX defines `dlopen` as the operation that makes an executable object's symbols available. It also loads embedded dependencies; failure to find, read, recognize, or relocate the object produces a null result with detail from `dlerror` ([POSIX `dlopen`](https://pubs.opengroup.org/onlinepubs/9699919799/functions/dlopen.html)). Linux recursively loads a `dlopen`ed object's dependencies and supports:

- `RTLD_NOW`, which resolves all undefined symbols before returning and fails immediately if resolution is impossible;
- `RTLD_LOCAL`, which keeps the object's symbols out of the lookup scope for subsequently loaded objects ([`dlopen(3)`](https://man7.org/linux/man-pages/man3/dlopen.3.html)).

Use `RTLD_NOW | RTLD_LOCAL` for the adapter. The current cross-platform `libloading::Library::new` defaults to `RTLD_LAZY | RTLD_LOCAL` on Unix, so it can defer an unused unresolved function until a later call. `libloading::os::unix::Library::open` exposes the explicit flags ([libloading Unix `Library`](https://docs.rs/libloading/latest/libloading/os/unix/struct.Library.html)).

Pass an absolute, application-owned adapter path. `libloading` warns that pathless library search is platform-specific and recommends an absolute or explicit relative path unless loading a system library ([libloading `Library`](https://docs.rs/libloading/latest/libloading/struct.Library.html)). Neomacs's current absolute environment override and executable-relative/versioned candidates are directionally correct; the current working directory must never be an implicit search location.

A load error is not synonymous with “not installed.” It can mean that the adapter is absent, a transitive GStreamer/GLib object is absent, the ELF is for the wrong architecture, permissions forbid access, initialization code failed, or relocation failed. POSIX deliberately defines no portable `errno` set for `dlopen`; retain the loader diagnostic rather than parsing its wording into false precision.

For known private candidate paths, filesystem metadata may improve the message—`NotFound` can be reported as “adapter not installed”—but it must not be treated as proof that a present file is loadable. Other metadata errors should not be collapsed into absence, and the subsequent load result remains authoritative.

### 3. Symbols and ABI versions are a separate check

After a successful load, use `dlsym`/`Library::get` for one versioned entry such as `neomacs_video_backend_v2`. POSIX specifies that `dlsym` searches the object and the dependencies loaded with it ([POSIX `dlsym`](https://pubs.opengroup.org/onlinepubs/009604299/functions/dlsym.html)). `libloading` emphasizes that the caller must provide the exact symbol type and that a wrong type is undefined behavior ([libloading Unix `Library::get`](https://docs.rs/libloading/latest/libloading/os/unix/struct.Library.html)).

Neomacs should continue validating, in order:

1. entry symbol exists;
2. returned pointer is non-null;
3. fixed header reports the expected ABI version;
4. table size covers the required prefix;
5. every required operation is present.

An absent entry or incompatible table means **adapter rejected**, not **GStreamer absent**. Keep the `Library` alive as long as copied function pointers, backend instances, callbacks, or opaque frames can exist. `libloading::Symbol` normally ties symbol lifetime to its `Library`; extracting raw function pointers gives up that compiler-enforced relationship ([libloading `Symbol`](https://docs.rs/libloading/latest/libloading/struct.Symbol.html)). The current `LoadedBackend` field that owns the library is therefore essential.

Loading executes shared-library initialization routines, so `libloading` marks the operation unsafe and describes it as conceptually calling an unknown foreign function ([libloading `Library`](https://docs.rs/libloading/latest/libloading/struct.Library.html)). An exact same-vendor path and ABI check reduce accidental mismatch; they do not create a security boundary.

### 4. Library initialization can fail after loading

GStreamer explicitly distinguishes fatal `gst_init` from fallible `gst_init_check`: applications that want fallback must use the latter, which returns an error rather than terminating ([GStreamer initialization API](https://gstreamer.freedesktop.org/documentation/gstreamer/gst.html)). The official Rust `gst::init()` already calls `gst_init_check` and returns `Result<(), glib::Error>` ([gstreamer-rs source](https://docs.rs/gstreamer/latest/src/gstreamer/lib.rs.html#312-329)).

Neomacs's adapter already calls `gst::init()` from backend creation and propagates its error. Preserve that stage as `InitializationFailed`, distinct from loader and ABI failures. Query `gst::version()` only for diagnostics; do not infer runtime compatibility from filenames. Normal linkage plus generated package dependencies should enforce the minimum symbols used by the adapter.

Initialization should remain lazy. GStreamer initialization sets up search paths, loads the registry, and may rescan plugins ([GStreamer initialization API](https://gstreamer.freedesktop.org/documentation/gstreamer/gst.html)). The renderer's current `VideoSystemState::Deferred` correctly avoids doing that during renderer/editor startup.

### 5. Codec/plugin absence is media-specific

Loading `libgstreamer-1.0.so` does not establish support for a codec, container, or URI. GStreamer distributes functionality as plugins, and registry contents vary by installation. The registry cache's location and format are explicitly internal; applications should use `GstRegistry`, element factories, and bus messages instead of parsing it ([GStreamer registry](https://gstreamer.freedesktop.org/documentation/gstreamer/gstregistry.html)).

Use two mechanisms:

- For a small fixed prerequisite such as `playbin3`, use `ElementFactory::find` or `gst_registry_check_feature_version`; the latter checks both feature presence and minimum version ([`GstRegistry`](https://gstreamer.freedesktop.org/documentation/gstreamer/gstregistry.html)).
- For source-dependent decoders, demuxers, and URI handlers, build the pipeline and retain missing-plugin element messages posted by `decodebin`/`playbin` on the bus. GStreamer provides descriptions and installer-detail strings specifically so applications can report or request installation of the missing component ([missing-plugin API](https://gstreamer.freedesktop.org/documentation/pbutils/gstpbutilsmissingplugins.html), [install-plugins API](https://gstreamer.freedesktop.org/documentation/pbutils/gstpbutilsinstallplugins.html)).

The current worker consumes generic GStreamer errors but discards element messages. It should preserve missing-plugin messages and return a typed `MissingPlugin { description, installer_detail }` for that video session. The backend remains available for other formats. If a supported platform installer adds plugins, GStreamer directs the application to update the registry and retry the media operation.

## Recommended Neomacs state model

Do not erase these stages into `Result<_, String>`. A closed Rust model makes invalid fallback decisions difficult:

```rust
enum VideoBackendState {
    Deferred(Initializer),
    Ready(Arc<LoadedBackend>),
    Unavailable(VideoBackendUnavailable),
    Initializing,
}

enum VideoBackendUnavailable {
    AdapterNotInstalled { attempted: Vec<PathBuf> },
    AdapterInaccessible { path: PathBuf, diagnostic: String },
    AdapterLoadRejected { path: PathBuf, diagnostic: String },
    EntryMissing { path: PathBuf },
    AbiMismatch { path: PathBuf, expected: u32, actual: u32 },
    AbiTruncated { path: PathBuf, expected: usize, actual: usize },
    OperationMissing { path: PathBuf, operation: BackendOperation },
    InitializationFailed { diagnostic: String },
}

enum VideoSessionFailure {
    MissingPlugin { description: String, installer_detail: Option<String> },
    UnsupportedMedia { diagnostic: String },
    Pipeline { diagnostic: String },
}
```

`BackendOperation` should itself be an enum, not a free-form string. Convert these errors to stable public/Lisp-facing diagnostics at the outer boundary; keep the typed reason internally for retry, telemetry, tests, and UI decisions.

Probe once on the first video command, cache success, and avoid retrying every frame. If installing the adapter while Neomacs remains open is a supported workflow, expose one explicit `retry_video_backend` transition from `Unavailable` to `Deferred`; otherwise the current cached failure is appropriate. Plugin installation is different: update GStreamer's registry and retry only the affected session without unloading the adapter.

Degradation should be visible but scoped: batch, TUI, editing, and non-video rendering continue; the first requested video reports one actionable failure. Do not silently pretend a requested video succeeded.

## Anti-patterns

- **Probing `/usr/lib/libgstreamer*.so` or the adapter with only `exists()`.** This ignores multiarch layouts, Nix/store paths, loader search rules, architecture, dependency closure, relocation, and ABI. File metadata may classify a known private path but cannot establish loadability.
- **Parsing `ldconfig -p`.** `ldconfig -p` prints candidates in the current cache only ([`ldconfig(8)`](https://man7.org/linux/man-pages/man8/ldconfig.8.html)); the actual loader also considers explicit paths, `RPATH`/`RUNPATH`, `LD_LIBRARY_PATH`, cache, and default directories ([`ld.so(8)`](https://man7.org/linux/man-pages/man8/ld.so.8.html)).
- **Running `pkg-config` at runtime.** `pkg-config` manages compile/link flags and development metadata, not whether the runtime loader can open an object ([freedesktop.org pkg-config](https://www.freedesktop.org/wiki/Software/pkgconfig/), [GStreamer development FAQ](https://gstreamer.freedesktop.org/documentation/frequently-asked-questions/developing.html)). Runtime images need not contain the executable or `.pc` files.
- **Using `gst-launch-1.0`/`gst-inspect-1.0` as the application probe.** Tools may be packaged separately and run under a different environment. GStreamer explicitly calls `gst-launch-1.0` a debugging tool on which applications should not be built ([GStreamer `gst-launch-1.0`](https://gstreamer.freedesktop.org/documentation/tools/gst-launch.html)).
- **Checking only the core GStreamer library and claiming codec support.** Codec/protocol support is plugin- and source-specific; consume registry/pipeline results.
- **Treating every loader failure as normal absence.** A missing optional adapter is expected. A present but unloadable or ABI-incompatible adapter is an actionable broken installation.
- **Loading a pathless private library or searching the current directory.** Use a resolved application-owned absolute path; search-path behavior is platform-specific and can load the wrong object.
- **Using `RTLD_LAZY` for compatibility validation.** It can postpone unresolved function failures. Use `RTLD_NOW` for a small optional adapter.

## Packaging and cross-platform notes

Runtime optionality must also exist at the package boundary. If the adapter is inside the base distro package, that package's tooling should discover and require the adapter's GStreamer dependencies. Debian policy requires dependency computation for binaries, shared libraries, and loadable modules, including `dlopen` users ([Debian Policy §8.6](https://www.debian.org/doc/debian-policy/ch-sharedlibs.html)). To permit a genuinely GStreamer-free install, ship a base `neomacs` package and an exact-version `neomacs-video-gstreamer` addon whose generated dependencies cover its `DT_NEEDED` closure.

The staged model is portable even though loader APIs differ. Windows `LoadLibraryExW` fails if the named DLL or one of its dependencies cannot be found and supports restricted search flags for an absolute module path ([Microsoft `LoadLibraryExW`](https://learn.microsoft.com/en-us/windows/win32/api/libloaderapi/nf-libloaderapi-loadlibraryexw)). macOS provides `dlopen`/`dlsym` with analogous try-load semantics ([Apple `dlopen(3)`](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man3/dlopen.3.html)). Keep platform-specific loader/search policy behind the same typed availability model.

An out-of-process helper can also let the editor survive missing GStreamer: the helper itself may fail in its loader before its `main`, while the parent observes spawn/exit/IPC failure. That buys a crash boundary, but adds a versioned IPC protocol, process supervision, and frame/descriptor transport. For Neomacs's stated requirement—optional presence, not sandboxing—the lazy private adapter is the smaller and faster abstraction.
