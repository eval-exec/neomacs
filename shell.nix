{ pkgs ? import <nixpkgs> {} }:

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
  ];

  # Set up environment for pkg-config
  PKG_CONFIG_PATH = pkgs.lib.makeSearchPath "lib/pkgconfig" [
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
  ];

  shellHook = ''
    echo "Emacs/Neomacs build environment"
    echo "GTK4 version: $(pkg-config --modversion gtk4 2>/dev/null || echo 'not found')"
    echo "GStreamer version: $(pkg-config --modversion gstreamer-1.0 2>/dev/null || echo 'not found')"
    
    # Set the library path
    export LD_LIBRARY_PATH="${pkgs.lib.makeLibraryPath [
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
    ]}:$LD_LIBRARY_PATH"
    
    echo ""
    echo "To configure with Neomacs:"
    echo "  ./configure --with-neomacs"
    echo ""
  '';
}
