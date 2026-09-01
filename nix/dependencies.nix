{
  lib,
  pkgs,
  wpeWebkit,
}:
let
  baseBuildInputs =
    with pkgs;
    [
      ncurses
      gnutls
      zlib
      libxml2
      fontconfig
      freetype
      harfbuzz
      cairo
      pango
      glib
      libsoup_3
      glib-networking
      libjpeg
      libtiff
      giflib
      libpng
      librsvg
      libwebp
      poppler
      dbus
      sqlite
      tree-sitter
      gmp
    ]
    ++ lib.optionals pkgs.stdenv.isLinux (
      with pkgs;
      [
        # Rust dependencies may link C++ libraries even though Neomacs itself
        # is Rust. Keep libstdc++ in both the package and development runtime
        # closure so freshly linked bootstrap executables are runnable.
        stdenv.cc.cc.lib
        libotf
        alsa-lib
        libselinux
        libGL
        vulkan-loader
        libxkbcommon
        mesa
        libdrm
        libgbm
        wayland
        wayland-protocols
        libx11
        libxpm
        libxcursor
        libxrandr
        libxi
        libxinerama
      ]
    );

  videoBuildInputs =
    with pkgs;
    [
      gst_all_1.gstreamer
      gst_all_1.gst-plugins-base
      gst_all_1.gst-plugins-good
      gst_all_1.gst-plugins-bad
      gst_all_1.gst-plugins-ugly
      gst_all_1.gst-libav
      gst_all_1.gst-plugins-rs
    ]
    ++ lib.optionals pkgs.stdenv.isLinux (
      with pkgs;
      [
        gst_all_1.gst-vaapi
        libva
      ]
    );

  webviewBuildInputs = lib.optionals pkgs.stdenv.isLinux (
    assert lib.assertMsg (wpeWebkit != null) "Linux webview dependencies require a WPE package";
    [
      wpeWebkit
      pkgs.libwpe
      pkgs.libwpe-fdo
      pkgs.weston
      pkgs.xdg-dbus-proxy
    ]
  );
in
{
  inherit baseBuildInputs videoBuildInputs webviewBuildInputs;

  # Development exposes every optional native capability. Distribution
  # packages select capabilities using the typed production policy.
  developmentBuildInputs = baseBuildInputs ++ videoBuildInputs ++ webviewBuildInputs;

  productionBuildInputs =
    capabilities:
    baseBuildInputs
    ++ lib.optionals (builtins.elem "video" capabilities.cargoFeatures) videoBuildInputs
    ++ lib.optionals (builtins.elem "webview" capabilities.cargoFeatures) webviewBuildInputs;
}
