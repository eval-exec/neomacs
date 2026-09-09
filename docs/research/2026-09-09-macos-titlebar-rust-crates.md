# Rust crate and abstraction references for macOS native titlebar color

Research date: 2026-09-09. Scope: Neomacs issue #365, retaining native traffic-light controls while matching the frame background. This is primary-source research, not native macOS verification; the available machine is Linux Wayland. No runtime code or dependencies were changed.

## Recommendation

Keep winit as window owner. Use its existing macOS titlebar attributes and a small macOS-only adapter built with the already-selected objc2 ecosystem to synchronize the native background. Learn the policy distinction from Tauri and the platform-adapter boundary from egui-winit. Do not add Tauri, Tao, egui, or window-vibrancy as dependencies just for this feature.

This recommendation is an inference from the capabilities below. Matching a native titlebar's color and extending GPU content beneath that titlebar are separate operations. Tauri demonstrates the former. The literal full-size-content request in [issue #365](https://github.com/eval-exec/neomacs/issues/365) requires the latter too; a color-only implementation must not be presented as equivalent without establishing the acceptance criteria.

## Directly useful crates

### Existing winit fork: creation policy and native geometry

Neomacs's Cargo.lock selects winit **0.31.0-beta.3**, fork revision **dc7af19d15de375f980687f650174989f4b668a8**; the local checkout matches. This is not the stable 0.30 API shown by many online examples. The fork exposes `WindowAttributesMacOS`, with separate controls for titlebar transparency, full-size content, title visibility, button visibility, and background dragging. Preserve decorations and buttons. Choose content extent explicitly instead of accidentally changing it alongside color. The documented interaction with `with_decorations` means attribute composition must be deliberate. [Pinned attribute implementation](https://github.com/eval-exec/winit/blob/dc7af19d15de375f980687f650174989f4b668a8/winit-appkit/src/lib.rs).

The native implementation independently applies `FullSizeContentView` and `setTitlebarAppearsTransparent`. Its `set_transparent` also calls `setBackgroundColor`, selecting either clear or system background. Therefore a one-time external color assignment can later be overwritten by opacity changes. Synchronization needs one owner and a defined order. Its safe-area calculation uses `NSView::safeAreaInsets` where available and falls back to `contentLayoutRect`. Full-size-content changes require a geometry notification; a source comment explicitly tracks this concern. [Pinned native implementation](https://github.com/eval-exec/winit/blob/dc7af19d15de375f980687f650174989f4b668a8/winit-appkit/src/window_delegate.rs).

`Window::safe_area` returns physical insets. Do not substitute a hard-coded titlebar height: consume measured geometry if overlay content is later supported. Also do not claim this solves all mobile safe areas: this revision documents Android, Orbital, Wayland, Windows and X11 as returning zero insets. [Pinned window contract](https://github.com/eval-exec/winit/blob/dc7af19d15de375f980687f650174989f4b668a8/winit-core/src/window.rs).

The fork's unified-titlebar option is not a color API: it installs an `NSToolbar` and selects unified toolbar styling. Avoid enabling it merely to match backgrounds. There is no general native background-color setter in the inspected winit window interface, so a narrow AppKit bridge remains useful. Winit is Apache-2.0. [Native implementation](https://github.com/eval-exec/winit/blob/dc7af19d15de375f980687f650174989f4b668a8/winit-appkit/src/window_delegate.rs), [manifest](https://github.com/eval-exec/winit/blob/dc7af19d15de375f980687f650174989f4b668a8/Cargo.toml).

### objc2 + objc2-app-kit: typed native adapter

Neomacs already resolves objc2 **0.6.4** and objc2-app-kit **0.3.2**. `NSWindow` provides typed `setBackgroundColor`, `setTitlebarAppearsTransparent`, `standardWindowButton`, and `contentLayoutRect`; `NSWindow` is main-thread-only and not Send/Sync. These bindings let the adapter use public AppKit methods without Objective-C selector strings spread throughout Neomacs. [NSWindow API](https://docs.rs/objc2-app-kit/0.3.2/objc2_app_kit/struct.NSWindow.html).

Require a `MainThreadMarker` at the native boundary. Its safe constructor checks the current thread; passing it through APIs records the main-thread requirement in types. It does not prove arbitrary raw pointers valid, so pointer provenance and window lifetime still need a small documented unsafe boundary. [MainThreadMarker API](https://docs.rs/objc2/0.6.4/objc2/struct.MainThreadMarker.html).

Prefer explicit `NSColor` color-space construction matching Neomacs's resolved frame color over treating every float triplet as interchangeable. Exact alpha/color-space policy still needs design against Neomacs rendering, not blind copying of sample code. The needed AppKit symbols have Cargo feature gates; add only target-specific features/dependencies actually used. This is integration work, not a replacement UI framework. [NSColor API](https://docs.rs/objc2-app-kit/0.3.2/objc2_app_kit/struct.NSColor.html).

License metadata checked in downloaded crate manifests: objc2 0.6.4 is MIT; objc2-app-kit 0.3.2 is Zlib OR Apache-2.0 OR MIT. These are distinct package licenses, not one assumed ecosystem license. [objc2 package](https://docs.rs/crate/objc2/0.6.4), [AppKit package](https://docs.rs/crate/objc2-app-kit/0.3.2).

### raw-window-handle: interoperability, not styling

Existing raw-window-handle **0.6.2** exposes the AppKit **NSView pointer**, not an NSWindow pointer. Recover the containing window through that view on the main thread, using the borrowed handle's lifetime as the provenance boundary. The crate's example illustrates this route but uses an older objc2 `Id` name; Neomacs's 0.6 generation uses `Retained`. Do not copy the example verbatim. Raw handles do not synchronize color or own native resources. [AppKit handle contract](https://docs.rs/raw-window-handle/0.6.2/raw_window_handle/struct.AppKitWindowHandle.html).

License: MIT OR Apache-2.0 OR Zlib. [Manifest](https://github.com/rust-windowing/raw-window-handle/blob/master/Cargo.toml).

## Design references, not new dependencies

### Tauri: the strongest exact-feature reference

Tauri's official guide has a dedicated example combining `TitleBarStyle::Transparent` with `NSWindow::setBackgroundColor` through objc2-app-kit 0.3.2. It explicitly contrasts this native approach with custom titlebars, which can lose OS-provided window behavior. This closely matches #365's requested appearance without requiring HTML or a webview in Neomacs. [Official transparent-titlebar example](https://v2.tauri.app/learn/window-customization/#macos-transparent-titlebar-with-custom-window-background-color).

More useful than importing the framework is its `TitleBarStyle` enum: `Visible`, `Transparent`, and `Overlay`. Transparent exposes the window background; Overlay extends content underneath controls and introduces geometry/drag caveats. This is a clear domain boundary to learn from. The inspected tauri-utils 2.9.3 docs warn that titlebar height varies by OS and title color follows system theme. Do not generalize its webview-specific focus/drag caveat to winit without reproducing it. [Typed titlebar policy](https://docs.rs/tauri-utils/2.9.3/tauri_utils/enum.TitleBarStyle.html).

Tao separately exposes full-size-content and titlebar-transparency setters, plus traffic-light insets. It is useful source material if Neomacs later needs repositioning, but native default controls do not require that complexity. Its exposed NSWindow pointer is tied to window lifetime. Inspected dev revision: **27fe28c73ace307eb0b1d1abf457508410f1340e**. Tao source is Apache-2.0; Tauri's workspace declares Apache-2.0 OR MIT. [Tao macOS interface](https://github.com/tauri-apps/tao/blob/27fe28c73ace307eb0b1d1abf457508410f1340e/src/platform/macos.rs), [Tauri manifest](https://github.com/tauri-apps/tauri/blob/dev/Cargo.toml).

### egui / egui-winit: isolate policy from platform mapping

The inspected egui-winit adapter maps semantic viewport attributes to platform-specific winit calls in one place. Its macOS mapping independently handles title visibility, buttons, titlebar transparency, full-size content, background movement and shadow. Learn this localization of platform knowledge; importing egui-winit itself would also import an egui event/input integration layer Neomacs does not need. Inspected main revision: **7053d3917af714090c3f363764efde0744c19fcb**. It uses the winit 0.30-style API, unlike Neomacs's fork. [Adapter implementation](https://github.com/emilk/egui/blob/7053d3917af714090c3f363764efde0744c19fcb/crates/egui-winit/src/lib.rs).

The viewport builder explicitly warns that whole-background dragging can interfere with draggable widgets. For an editor, infer that text selection, scrollbar dragging and toolbar interactions must not become window drag regions. Retain native titlebar hit-testing for the color-only feature. [Viewport API](https://docs.rs/egui/latest/egui/viewport/struct.ViewportBuilder.html). Egui-winit's license is MIT OR Apache-2.0. [Package manifest](https://docs.rs/crate/egui-winit/0.36.1/source/Cargo.toml).

## Not needed for #365: window-vibrancy

Window-vibrancy implements materials/effects, including macOS vibrancy and newer Liquid Glass, rather than a solid titlebar/frame color contract. Its README lists compositor-dependent Linux behavior as unsupported. Adding it would not remove the need for titlebar policy, safe-area handling, or frame-color synchronization. Reserve it for a separately requested material/translucency feature. [Project README](https://github.com/tauri-apps/window-vibrancy/blob/e9f765a4c5a291d8eb636ffabf638b39d9783ebe/README.md).

Inspected dev manifest revision **e9f765a4c5a291d8eb636ffabf638b39d9783ebe** declares version 0.8.0, raw-window-handle 0.6, objc2 0.6 and AppKit 0.3.2; this is a source snapshot, not a claim that every dev API is in a published release. License: Apache-2.0 OR MIT. [Manifest](https://github.com/tauri-apps/window-vibrancy/blob/e9f765a4c5a291d8eb636ffabf638b39d9783ebe/Cargo.toml).

## Suggested Neomacs boundary and verification

Design inference: use an enum for native chrome policy, keeping system-default and frame-background-colored native chrome distinct. Keep content extension a separate layout policy; do not expose an unconstrained bag of platform booleans. A resolved per-frame color should feed both renderer and chrome adapter. The adapter owns main-thread native application and handles creation, color changes, and operations that reset native appearance. Platform capability reporting should not pretend unsupported backends implemented macOS coloring.

First establish tests for policy lowering, frame-specific color propagation, unchanged geometry under color-only updates, and reapplication after transparency changes. Native macOS tests/manual checks must cover traffic lights, focus appearance, title legibility, theme changes, multiple frames, fullscreen, resize and Retina scaling. Linux tests can validate dataflow/invariants, but cannot establish AppKit visual correctness. Full-size content additionally requires overlay geometry and input tests; traffic-light relocation is not required merely to retain native controls. Rust tests belong in `*_test.rs` or `tests/`, run with `cargo nextest`; native presentation tests belong in GUI tests, not TUI tests.

## Editor references and platform contract

### GNU Emacs: propagate changes, not just creation attributes

The inspected GNU checkout is `a360712c9d272d950d8d8255ef74570f7e90b7d9`. `ns_set_background_color` updates the frame's background and the live NSWindow background, adjusts native opacity, and refreshes the frame. `ns_set_transparent_titlebar` independently changes the native titlebar transparency. This is a reference for live per-frame synchronization, not evidence that setting one creation flag reproduces every aspect of #365. [Background update implementation](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/nsfns.m), [titlebar implementation](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/nsterm.m).

### Neovide: closest editor behavior reference

Current upstream inspected revision: `ade2d9cda777879975b1852f77dc672f5ff43b78`. Its macOS implementation now lives in `src/platform/macos/mod.rs`, unlike the older local checkout. It retains a typed NSWindow, measures titlebar height, updates pixel padding after scale changes, removes extra padding in fullscreen, and installs a native titlebar click handler. Its native background is governed by opacity/shadow policy, so do not describe it as simply assigning the editor RGB to NSWindow. [Native implementation](https://github.com/neovide/neovide/blob/ade2d9cda777879975b1852f77dc672f5ff43b78/src/platform/macos/mod.rs).

The window wrapper adds this native titlebar reservation to the editor's top padding. The useful lesson is an explicit relationship between native occupied space and editor layout, rather than scattered magic offsets. Neomacs should prefer its own pinned winit's measured safe-area contract where sufficient; it need not copy Neovide's measurement or click-handler implementation wholesale. [Layout integration](https://github.com/neovide/neovide/blob/ade2d9cda777879975b1852f77dc672f5ff43b78/src/window/window_wrapper.rs).

### Zed / GPUI: desired policy separated from native implementation

Inspected revision: `fceace0b84325d8706732f75a6d17192f288bd5f`. GPUI models titlebar options separately from a typed `WindowBackgroundAppearance` policy and puts platform behavior behind its window interface. This is an architectural reference, not a drop-in chrome library for winit. [Platform interface](https://github.com/zed-industries/zed/blob/fceace0b84325d8706732f75a6d17192f288bd5f/crates/gpui/src/platform.rs).

Its macOS adapter coordinates renderer transparency with native opacity/background and blur-view lifecycle. It also manages native traffic-light state around layout/fullscreen changes. Do not copy its private-selector workarounds; use public AppKit methods and Neomacs's winit lifecycle. [macOS implementation](https://github.com/zed-industries/zed/blob/fceace0b84325d8706732f75a6d17192f288bd5f/crates/gpui_macos/src/window.rs). The inspected GPUI package declares Apache-2.0; this is not a blanket licensing statement about all Zed editor code. [GPUI manifest](https://github.com/zed-industries/zed/blob/fceace0b84325d8706732f75a6d17192f288bd5f/crates/gpui/Cargo.toml).

### Apple contract and remaining uncertainty

Apple describes transparent titlebars in conjunction with full-size content. Its full-size-content documentation directs applications to `contentLayoutRect`/`contentLayoutGuide` for unobscured content. This is why Tauri's documented color-only technique should not be generalized into a promise that transparency alone implements under-titlebar GPU drawing on every supported macOS version. Both techniques need native verification for their respective acceptance criteria. [Titlebar transparency](https://developer.apple.com/documentation/appkit/nswindow/titlebarappearstransparent), [full-size content](https://developer.apple.com/documentation/appkit/nswindow/stylemask-swift.struct/fullsizecontentview).

## Recommended Neomacs design, inferred from the research

The inspected Neomacs primary and secondary frame creation paths currently set generic decorations/transparency, without macOS titlebar configuration. Frame parameter identities already include `NsAppearance` and `NsTransparentTitlebar`; reuse that typed vocabulary when implementing compatible behavior rather than introducing another string protocol. Local references: `crates/neomacs-display-runtime/src/render_thread/lifecycle.rs`, `crates/neomacs-display-runtime/src/render_thread/frame_windows.rs`, and `crates/neovm-core/src/window/frame_params.rs`.

Keep the implementation behind a small frame-chrome module:

1. A typed policy distinguishes system chrome, native frame-colored chrome, and native overlay chrome, with variant-specific data as necessary. This is proposed Neomacs terminology, not an existing shared framework API. Use exhaustive matches; use strum only for actual enum/string conversion or iteration needs.
2. Resolve each frame's background/opacity once and supply it to both rendering and the native adapter. Color changes must not accidentally become layout changes.
3. For overlay chrome, distinguish the full drawing surface from the unobscured editor rectangle. Render the background across the former; lay out editor UI in the latter. Rendering, pointer conversion, and popup anchors must share the same geometry transformation.
4. Keep AppKit objects and operations on the event-loop main thread. Pass owned color/policy/geometry data across threads, not raw native pointers. Main-thread tokens improve compile-time checking; they do not eliminate the need to validate borrowed raw handles.
5. Apply the same path to primary and subsequent frames and refresh observations after resize, scale and fullscreen transitions. Keep macOS specifics in its adapter; do not claim equivalent titlebar support on Android/browser or other platforms without an implementation.

This seam is justified by existing platform differences. A new general-purpose GUI framework, toolbar implementation, native popup rewrite, or winit fork change is not required by the capabilities found in this research. The next implementation step should begin with focused failing policy/dataflow tests, followed by the smallest vertical implementation for the agreed color-only or full-size-content behavior. Native macOS visual acceptance remains unverified on this Linux Wayland machine.
