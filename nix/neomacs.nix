{ lib
, stdenv
, rustPlatform
, rust-cbindgen
, pkg-config
, autoconf
, automake
, texinfo
, llvmPackages
, ncurses
, gnutls
, zlib
, libxml2
, fontconfig
, freetype
, harfbuzz
, cairo
, gtk4
, glib
, graphene
, pango
, gdk-pixbuf
, gst_all_1
, libsoup_3
, glib-networking
, libjpeg
, libtiff
, giflib
, libpng
, librsvg
, libwebp
, dbus
, sqlite
, libselinux
, tree-sitter
, gmp
, libgccjit
, libGL
, libxkbcommon
, mesa
, libdrm
, libgbm
, libva
, wayland
, wayland-protocols
, wpewebkit
, libwpe
, libwpe-fdo
, weston
, makeWrapper
}:

let
  # Build the Rust library first
  neomacs-display = rustPlatform.buildRustPackage {
    pname = "neomacs-display";
    version = "0.1.0";

    src = ../rust/neomacs-display;

    cargoLock = {
      lockFile = ../rust/neomacs-display/Cargo.lock;
    };

    # Only build the library, not examples (examples may be out of sync)
    cargoBuildFlags = [ "--lib" ];
    cargoTestFlags = [ "--lib" ];
    doCheck = false;  # Skip tests for now

    nativeBuildInputs = [
      pkg-config
      rust-cbindgen
      llvmPackages.libclang
    ];

    buildInputs = [
      gtk4
      glib
      graphene
      pango
      cairo
      gdk-pixbuf
      gst_all_1.gstreamer
      gst_all_1.gst-plugins-base
      gst_all_1.gst-plugins-bad  # Contains gst-va plugin (gstva-1.0)
      libva                       # VA-API for hardware video decoding
      libGL
      libxkbcommon
      wayland
      wayland-protocols
      wpewebkit
      libwpe
      libwpe-fdo
      libsoup_3
    ];

    # pkg-config path for finding WPE and other libraries
    PKG_CONFIG_PATH = lib.makeSearchPath "lib/pkgconfig" [
      gtk4.dev
      glib.dev
      graphene
      pango.dev
      cairo.dev
      gdk-pixbuf.dev
      gst_all_1.gstreamer.dev
      gst_all_1.gst-plugins-base.dev
      gst_all_1.gst-plugins-bad.dev  # For gstva-1.0
      libva                           # For libva
      libGL.dev
      libxkbcommon.dev
      wayland.dev
      wpewebkit
      libwpe
      libwpe-fdo
      libsoup_3.dev
    ];

    LIBCLANG_PATH = "${llvmPackages.libclang.lib}/lib";

    # Bindgen needs to find C standard headers and library headers
    BINDGEN_EXTRA_CLANG_ARGS = builtins.concatStringsSep " " [
      "-isystem ${stdenv.cc.libc.dev}/include"
      "-isystem ${libxkbcommon.dev}/include"
      "-isystem ${glib.dev}/include/glib-2.0"
      "-isystem ${glib.out}/lib/glib-2.0/include"
      "-isystem ${gtk4.dev}/include/gtk-4.0"
      "-isystem ${cairo.dev}/include/cairo"
      "-isystem ${pango.dev}/include/pango-1.0"
      "-isystem ${graphene}/include/graphene-1.0"
      "-isystem ${gdk-pixbuf.dev}/include/gdk-pixbuf-2.0"
      "-isystem ${wayland.dev}/include"
      "-isystem ${libGL.dev}/include"
    ];

    # Generate C headers after build
    postBuild = ''
      cbindgen --config cbindgen.toml --crate neomacs-display --output include/neomacs_display.h || true
    '';

    postInstall = ''
      mkdir -p $out/include
      cp -r include/* $out/include/ || true
    '';
  };

in stdenv.mkDerivation {
  pname = "neomacs";
  version = "30.0.50-neomacs";

  src = ./..;

  nativeBuildInputs = [
    pkg-config
    autoconf
    automake
    texinfo
    makeWrapper
  ];

  buildInputs = [
    ncurses
    gnutls
    zlib
    libxml2
    fontconfig
    freetype
    harfbuzz
    cairo
    gtk4
    glib
    graphene
    pango
    gdk-pixbuf
    gst_all_1.gstreamer
    gst_all_1.gst-plugins-base
    gst_all_1.gst-plugins-good
    gst_all_1.gst-plugins-bad
    gst_all_1.gst-plugins-ugly
    gst_all_1.gst-libav
    gst_all_1.gst-plugins-rs
    libsoup_3
    glib-networking
    libjpeg
    libtiff
    giflib
    libpng
    librsvg
    libwebp
    dbus
    sqlite
    libselinux
    tree-sitter
    gmp
    libgccjit
    libGL
    libxkbcommon
    mesa
    libdrm
    libgbm
    wayland
    wayland-protocols
    wpewebkit
    libwpe
    libwpe-fdo
    weston
    # Link against our Rust library
    neomacs-display
  ];

  # Point to the pre-built Rust library
  NEOMACS_DISPLAY_LIB = "${neomacs-display}/lib";
  NEOMACS_DISPLAY_INCLUDE = "${neomacs-display}/include";

  preConfigure = ''
    echo "Using pre-built neomacs-display from: ${neomacs-display}"
    export NEOMACS_DISPLAY_LIB="${neomacs-display}/lib"
    export NEOMACS_DISPLAY_INCLUDE="${neomacs-display}/include"
    ./autogen.sh
  '';

  configureFlags = [
    "--with-neomacs"
    "--with-native-compilation=no"  # Disabled temporarily due to libgccjit nix issue
    "--with-gnutls"
    "--with-xml2"
    "--with-tree-sitter"
    "--with-modules"
    "CFLAGS=-I${neomacs-display}/include"
    "LDFLAGS=-L${neomacs-display}/lib"
  ];

  # Set up environment for WPE WebKit
  preBuild = ''
    export WPE_BACKEND_LIBRARY="${libwpe-fdo}/lib/libWPEBackend-fdo-1.0.so"
    export GIO_MODULE_DIR="${glib-networking}/lib/gio/modules"
  '';

  # Wrap the binary with required environment variables
  postInstall = ''
    wrapProgram $out/bin/emacs \
      --set WPE_BACKEND_LIBRARY "${libwpe-fdo}/lib/libWPEBackend-fdo-1.0.so" \
      --set GIO_MODULE_DIR "${glib-networking}/lib/gio/modules" \
      --set WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS "1" \
      --prefix PATH : "${wpewebkit}/libexec/wpe-webkit-2.0"
  '';

  meta = with lib; {
    description = "Neomacs - GPU-accelerated Emacs with GTK4, GStreamer, and WPE WebKit";
    homepage = "https://github.com/eval-exec/neomacs";
    license = licenses.gpl3Plus;
    platforms = platforms.linux;
    # `nix run` uses this when no explicit `apps.<system>.default` is defined.
    mainProgram = "emacs";
    maintainers = [ ];
  };
}
