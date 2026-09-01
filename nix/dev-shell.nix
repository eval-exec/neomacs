{
  pkgs,
  rustToolchain,
  wpeWebkit,
}:
let
  inherit (pkgs) lib;
  dependencies = import ./dependencies.nix {
    inherit lib pkgs wpeWebkit;
  };
  isLinux = pkgs.stdenv.isLinux;
  isDarwin = pkgs.stdenv.isDarwin;
  # ncurses remains RPATH-resolved because putting it in the shell's global
  # library path can contaminate the system shell's glibc.
  runtimeLibraryPath = pkgs.lib.makeLibraryPath (
    lib.remove pkgs.ncurses dependencies.developmentBuildInputs
  );
in
pkgs.mkShell {
  name = "neomacs-dev";

  nativeBuildInputs = [
    rustToolchain
    pkgs.pkg-config
    pkgs.llvmPackages.clang
    # Frozen wall clock for date/time-sensitive oracle tests.
    pkgs.libfaketime
  ]
  ++ lib.optionals isLinux [
    # Linux-only reverse debugger for the JIT wild-store hunt.
    pkgs.rr
  ];

  buildInputs =
    dependencies.developmentBuildInputs
    ++ lib.optionals isLinux [
      pkgs.gcc
      pkgs.xwininfo
    ];

  # Keep the explicit development header set visible to pkg-config. This is
  # intentionally separate from the production runtime closure.
  PKG_CONFIG_PATH = pkgs.lib.makeSearchPath "lib/pkgconfig" (
    with pkgs;
    [
      glib.dev
      cairo.dev
      pango.dev
      gst_all_1.gstreamer.dev
      gst_all_1.gst-plugins-base.dev
      fontconfig.dev
      freetype.dev
      harfbuzz.dev
      libxml2.dev
      gnutls.dev
      zlib.dev
      ncurses.dev
      dbus.dev
      sqlite.dev
      tree-sitter
      gmp.dev
      libsoup_3.dev
      poppler.dev
    ]
    ++ lib.optionals isLinux [
      alsa-lib.dev
      libva
      libselinux.dev
      libGL.dev
      libxkbcommon.dev
      libdrm.dev
      mesa
      wayland.dev
      wpeWebkit
      libwpe
      libwpe-fdo
    ]
  );

  LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";

  shellHook = ''
    export RUST_BACKTRACE=1

    # Pin the shared object path so oracle tests never infer it from PATH.
    export NEOVM_LIBFAKETIME_SO="${pkgs.libfaketime}/lib/libfaketime.so.1"

    echo "=== Neomacs Development Environment ==="
    echo ""
    echo "Rust: $(rustc --version)"
    echo "Cargo: $(cargo --version)"
    echo "GStreamer: $(pkg-config --modversion gstreamer-1.0 2>/dev/null || echo 'not found')"
  ''
  + lib.optionalString isLinux ''
    echo "xkbcommon: $(pkg-config --modversion xkbcommon 2>/dev/null || echo 'not found')"
    echo "WPE WebKit: $(pkg-config --modversion wpe-webkit-2.0 2>/dev/null || echo 'not found')"
    echo ""

    # Library path for runtime. The linker supplies ncurses' RPATH.
    export LD_LIBRARY_PATH="${runtimeLibraryPath}''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"

    export VK_DRIVER_FILES="$(echo ${pkgs.mesa}/share/vulkan/icd.d/*.json | tr ' ' ':')"

    export WPE_BACKEND_LIBRARY="${pkgs.libwpe-fdo}/lib/libWPEBackend-fdo-1.0.so"
    export GIO_MODULE_DIR="${pkgs.glib-networking}/lib/gio/modules"
    export WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1
    export WEBKIT_USE_SINGLE_WEB_PROCESS=1
    export PATH="${wpeWebkit}/libexec/wpe-webkit-2.0:$PATH"

    # nix develop may lose display variables. Recover them from the active
    # desktop session when possible so GUI probes fail quickly and clearly.
    _detect_display_env() {
      local _pid
      _pid=$(pgrep -u "$USER" kwin_x11 2>/dev/null | head -1)
      [ -z "$_pid" ] && _pid=$(pgrep -u "$USER" gnome-shell 2>/dev/null | head -1)
      [ -z "$_pid" ] && _pid=$(pgrep -u "$USER" Xorg 2>/dev/null | head -1)
      [ -z "$_pid" ] && _pid=$(pgrep -u "$USER" sway 2>/dev/null | head -1)
      [ -z "$_pid" ] && _pid=$(pgrep -u "$USER" Hyprland 2>/dev/null | head -1)
      if [ -n "$_pid" ] && [ -r "/proc/$_pid/environ" ]; then
        if [ -z "$DISPLAY" ]; then
          DISPLAY=$(tr '\0' '\n' < /proc/$_pid/environ | grep '^DISPLAY=' | head -1 | cut -d= -f2-)
          [ -n "$DISPLAY" ] && export DISPLAY
        fi
        if [ -z "$XAUTHORITY" ] && [ -n "$DISPLAY" ]; then
          XAUTHORITY=$(tr '\0' '\n' < /proc/$_pid/environ | grep '^XAUTHORITY=' | head -1 | cut -d= -f2-)
          if [ -n "$XAUTHORITY" ] && [ -f "$XAUTHORITY" ]; then
            export XAUTHORITY
          elif [ -f "$HOME/.Xauthority" ]; then
            export XAUTHORITY="$HOME/.Xauthority"
          fi
        fi
        if [ -z "$WAYLAND_DISPLAY" ]; then
          WAYLAND_DISPLAY=$(tr '\0' '\n' < /proc/$_pid/environ | grep '^WAYLAND_DISPLAY=' | head -1 | cut -d= -f2-)
          [ -n "$WAYLAND_DISPLAY" ] && export WAYLAND_DISPLAY
        fi
      fi
    }
    _detect_display_env
    unset -f _detect_display_env

    if [ -n "$DISPLAY" ]; then
      echo "Display: DISPLAY=$DISPLAY  XAUTHORITY=''${XAUTHORITY:-(unset)}"
      if ! timeout 2s ${pkgs.xdpyinfo}/bin/xdpyinfo >/dev/null 2>&1; then
        export NEOMACS_X11_UNUSABLE=1
        echo "Warning: X11 display handshake failed for DISPLAY=$DISPLAY."
        echo "         GUI clients like winit/Neomacs may hang before the first window appears."
        echo "         Run from a working desktop terminal, set a valid DISPLAY/XAUTHORITY,"
        echo "         or use a private X server like Xvfb for automated probes."
      fi
    else
      echo "Display: (no X11/Wayland display detected)"
    fi
  ''
  + lib.optionalString isDarwin ''
    echo ""
    echo "Note: WPE WebKit is not available on macOS."
    echo "      WebKit-based features will be disabled."
  ''
  + ''
    export RUST_LOG="''${RUST_LOG:-debug}"

    echo ""
    echo "Build commands:"
    echo "  1. cargo xtask fresh-build --release"
    echo "  2. ./target/release/neomacs"
    echo ""
    echo "Logging (set before entering nix develop to override):"
    echo "  RUST_LOG=$RUST_LOG  (trace|debug|info|warn|error)"
    echo ""
  '';
}
