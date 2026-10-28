# The build environment for the GNU Emacs parity oracle.
#
# `.github/actions/setup-gnu-emacs/action.yml` installs this same dependency
# set with apt on CI; this file is its NixOS translation, and both call the
# one recipe in scripts/ci/build-gnu-oracle.sh.  The oracle's identity is the
# pinned mirror commit *plus* the configure line plus the libraries configure
# finds, so a local oracle built with a different set would silently be a
# third reference that CI never compares against.
#
# It is pinned to the flake's nixpkgs: nix/modules/dev-shells.nix imports this
# file with the flake's `pkgs`, never <nixpkgs>, so the library set moves only
# when flake.lock does.
#
# CFLAGS are deliberately NOT set here.  GNU's configure defaults to "-g -O2"
# on CI; -march=native would let GCC fuse FMAs differently and -flto would
# change optimization, and the parity records would then encode this host's
# code generation instead of the reference build's Lisp semantics.
{ pkgs }:
let
  inherit (pkgs) lib;

  # The configure tests that decide the oracle's feature set.  Names match the
  # apt packages in the action: gtk3 + X11 for the window system,
  # tree-sitter/dbus/gnutls/xml2/gmp for the native primitives the oracle
  # tests exercise, and the image/colour libraries for the face and image
  # fixtures.
  # Items after gtk3 are the GTK stack pieces `emacs' links DIRECTLY (readelf
  # -d bin/emacs: libatk, libpango, libpangocairo, libgdk_pixbuf, libgio,
  # libgobject, libglib, libxcb, libXfixes, libXcomposite), so they must be on
  # its RUNPATH even though gtk3 would find them through its own.  configure
  # finds them through gtk3's pkg-config regardless; listing them is about
  # run time, not feature detection.
  buildInputs = with pkgs; [
    gtk3
    glib
    atk
    pango
    gdk-pixbuf
    dbus
    tree-sitter
    gnutls
    libxml2
    gmp
    ncurses
    cairo
    harfbuzz
    freetype
    fontconfig
    libpng
    libjpeg_turbo
    giflib
    libtiff
    libwebp
    librsvg
    lcms2
    zlib
    libxcb
    libxfixes
    libxcomposite
    xorg.libX11
    xorg.libXpm
    xorg.libXext
    xorg.libXinerama
    xorg.libXi
    xorg.libXrandr
    xorg.libXrender
    xorg.libICE
    xorg.libSM
    xorg.xorgproto
  ];

  # Where the local emacs-mirror checkout lives.  Overridable; the build
  # script still fetches the pinned commit by hash, so this only chooses the
  # transport, never the source identity.
  mirror = "/home/exec/Projects/github.com/emacs-mirror/emacs";
in
pkgs.mkShell {
  name = "gnu-emacs-oracle";

  nativeBuildInputs = with pkgs; [
    autoconf
    automake
    texinfo
    pkg-config
    gnumake
    gcc
    git
    gettext
    patchelf
    # build-gnu-oracle.sh fetches the pinned Tree-sitter tarball.
    curl
    # D-Bus notification fixtures start a private bus.
    dbus
  ];

  inherit buildInputs;

  # `make install` leaves the executable without a RUNPATH (CI's apt
  # libraries resolve through the system loader; NixOS has no such thing).
  # build-gnu-oracle.sh bakes this set into the installed executables as an
  # ELF RUNPATH when EMACS_ORACLE_RPATH is set, so the oracle runs from any
  # shell and not only from this one.  It changes ELF metadata only: GNU's
  # dump fingerprint is a compiled-in byte array, not a hash of the file.
  EMACS_ORACLE_RPATH = lib.makeLibraryPath buildInputs;

  EMACS_ORACLE_REPO = mirror;

  shellHook = ''
    echo "GNU Emacs oracle build shell (pinned to the flake's nixpkgs)"
    echo "  build: scripts/ci/build-gnu-oracle.sh <commit> <prefix>"
    echo "  check: scripts/ci/check-gnu-oracle.sh <prefix>/bin/emacs 31.1 <prefix>/share/gnu-emacs-oracle/build-manifest"
  '';
}
