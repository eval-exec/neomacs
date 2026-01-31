{ pkgs ? import <nixpkgs> {} }:

let
  # WPE WebKit from eval-exec's nixpkgs PR #449108
  # Required because webkitgtk dropped offscreen rendering support
  wpewebkitPkgs = import (builtins.fetchTarball {
    url = "https://github.com/eval-exec/nixpkgs/archive/wpewebkit.tar.gz";
  }) { inherit (pkgs) system; };
  
  wpewebkit = wpewebkitPkgs.wpewebkit or null;
  # libwpe and wpebackend-fdo from standard nixpkgs (they're stable)
  libwpe = pkgs.libwpe;
  wpebackendFdo = pkgs.libwpe-fdo;
in

pkgs.mkShell {
  buildInputs = with pkgs; [
    # Standard Emacs build dependencies
    pkg-config
    autoconf
    automake
    texinfo
    ncurses
    gnutls
    zlib
    libxml2
    
    # Font support
    fontconfig
    freetype
    harfbuzz
    
    # Cairo
    cairo
    
    # GTK4 and dependencies for Neomacs
    gtk4
    glib
    graphene
    pango
    gdk-pixbuf
    
    # GStreamer for video support
    gst_all_1.gstreamer
    gst_all_1.gst-plugins-base
    gst_all_1.gst-plugins-good
    gst_all_1.gst-plugins-bad
    gst_all_1.gst-plugins-ugly
    gst_all_1.gst-plugins-rs  # For gtk4paintablesink (DMA-BUF zero-copy)
    
    # libsoup for HTTP
    libsoup_3
    
    # Image libraries
    libjpeg
    libtiff
    giflib
    libpng
    librsvg
    libwebp
    
    # Other useful libraries
    dbus
    sqlite
    libselinux
    tree-sitter
    
    # GMP for bignum support
    gmp
    
    # For native compilation
    libgccjit
    
    # EGL for WPE
    libGL
    libxkbcommon
  ] ++ (if wpewebkit != null then [ wpewebkit ] else [])
    ++ [ libwpe wpebackendFdo ];

  # Set up environment for pkg-config
  PKG_CONFIG_PATH = pkgs.lib.makeSearchPath "lib/pkgconfig" ([
    pkgs.gtk4.dev
    pkgs.glib.dev
    pkgs.graphene
    pkgs.pango.dev
    pkgs.cairo.dev
    pkgs.gdk-pixbuf.dev
    pkgs.gst_all_1.gstreamer.dev
    pkgs.gst_all_1.gst-plugins-base.dev
    pkgs.fontconfig.dev
    pkgs.freetype.dev
    pkgs.harfbuzz.dev
    pkgs.libxml2.dev
    pkgs.gnutls.dev
    pkgs.zlib.dev
    pkgs.ncurses.dev
    pkgs.dbus.dev
    pkgs.sqlite.dev
    pkgs.libselinux.dev
    pkgs.tree-sitter
    pkgs.gmp.dev
    pkgs.libsoup_3.dev
    pkgs.libGL.dev
    pkgs.libxkbcommon.dev
  ] ++ (if wpewebkit != null then [ wpewebkit.dev or wpewebkit ] else [])
    ++ [ libwpe wpebackendFdo ]);

  shellHook = ''
    echo "Emacs/Neomacs build environment"
    echo "GTK4 version: $(pkg-config --modversion gtk4 2>/dev/null || echo 'not found')"
    echo "GStreamer version: $(pkg-config --modversion gstreamer-1.0 2>/dev/null || echo 'not found')"
    ${if wpewebkit != null then ''
    echo "WPE WebKit: $(pkg-config --modversion wpe-webkit-2.0 2>/dev/null || echo 'available')"
    echo "libwpe: $(pkg-config --modversion wpe-1.0 2>/dev/null || echo 'not in pkg-config')"
    echo "wpebackend-fdo: $(pkg-config --modversion wpebackend-fdo-1.0 2>/dev/null || echo 'not in pkg-config')"
    '' else ''
    echo "WPE WebKit: BUILDING (first run takes ~1 hour, from PR #449108)"
    ''}
    
    # Set the library path
    export LD_LIBRARY_PATH="${pkgs.lib.makeLibraryPath ([
      pkgs.gtk4
      pkgs.glib
      pkgs.cairo
      pkgs.pango
      pkgs.gdk-pixbuf
      pkgs.graphene
      pkgs.gst_all_1.gstreamer
      pkgs.gst_all_1.gst-plugins-base
      pkgs.fontconfig
      pkgs.freetype
      pkgs.harfbuzz
      pkgs.libxml2
      pkgs.gnutls
      pkgs.ncurses
      pkgs.libjpeg
      pkgs.libtiff
      pkgs.giflib
      pkgs.libpng
      pkgs.librsvg
      pkgs.libwebp
      pkgs.dbus
      pkgs.sqlite
      pkgs.gmp
      pkgs.libgccjit
      pkgs.libsoup_3
      pkgs.libGL
      pkgs.mesa
      pkgs.libxkbcommon
    ] ++ (if wpewebkit != null then [ wpewebkit ] else [])
      ++ [ libwpe wpebackendFdo ])}:$LD_LIBRARY_PATH"
    
    echo ""
    echo "To configure with Neomacs:"
    echo "  ./configure --with-neomacs"
    echo ""
    ${if wpewebkit != null then ''
    echo "WPE WebKit environment ready"
    '' else ""}
  '';
}
