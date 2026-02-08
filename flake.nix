{
  description = "Neomacs - GPU-accelerated Emacs with GTK4 and WebKit";

  nixConfig = {
    extra-substituters = [ "https://nix-wpe-webkit.cachix.org" ];
    extra-trusted-public-keys = [ "nix-wpe-webkit.cachix.org-1:ItCjHkz1Y5QcwqI9cTGNWHzcox4EqcXqKvOygxpwYHE=" ];
  };

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    # Rust toolchain
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
    };

    # WPE WebKit standalone flake with Cachix binary cache
    # Do NOT use `inputs.nixpkgs.follows` here — the Cachix binary was built
    # with nix-wpe-webkit's own pinned nixpkgs, so follows would change the
    # derivation hash and cause a cache miss (rebuilding from source ~1 hour).
    nix-wpe-webkit = {
      url = "github:eval-exec/nix-wpe-webkit";
    };
  };

  outputs = { self, nixpkgs, rust-overlay, nix-wpe-webkit }:
    let
      supportedSystems = [ "x86_64-linux" "aarch64-linux" ];

      forAllSystems = nixpkgs.lib.genAttrs supportedSystems;

      # Create pkgs with overlays for each system
      pkgsFor = system: import nixpkgs {
        inherit system;
        overlays = [
          rust-overlay.overlays.default
          self.overlays.default
        ];
      };

    in {
      # Overlay that provides wpewebkit and rust toolchain
      overlays.default = final: prev: {
        # WPE WebKit from nix-wpe-webkit flake (with Cachix binary cache)
        wpewebkit = nix-wpe-webkit.packages.${final.system}.wpewebkit;

        # Rust toolchain with components needed for neomacs-display
        rust-neomacs = final.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" ];
        };
      };

      # Development shell
      devShells = forAllSystems (system:
        let
          pkgs = pkgsFor system;
        in {
          default = pkgs.mkShell {
            name = "neomacs-dev";

            nativeBuildInputs = [
              # Rust toolchain
              pkgs.rust-neomacs
              pkgs.rust-cbindgen

              # Build tools
              pkgs.pkg-config
              pkgs.autoconf
              pkgs.automake
              pkgs.texinfo

              # For bindgen (generates Rust bindings from C headers)
              pkgs.llvmPackages.clang
            ];

            buildInputs = with pkgs; [
              # Standard Emacs build dependencies
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
              gst_all_1.gst-libav
              gst_all_1.gst-plugins-rs
              gst_all_1.gst-vaapi

              # VA-API for hardware video decoding (used by gst-va plugin)
              libva

              # libsoup for HTTP
              libsoup_3

              # GLib networking for TLS/HTTPS support
              glib-networking

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

              # EGL/GPU for WPE
              libGL
              libxkbcommon
              mesa
              libdrm
              libgbm

              # Wayland
              wayland
              wayland-protocols

              # WPE WebKit
              wpewebkit
              libwpe
              libwpe-fdo

              # Weston for WPE backend
              weston

              # xdg-dbus-proxy for WebKit sandbox
              xdg-dbus-proxy
              gcc
            ];

            # pkg-config paths for dev headers
            PKG_CONFIG_PATH = pkgs.lib.makeSearchPath "lib/pkgconfig" (with pkgs; [
              gtk4.dev
              glib.dev
              graphene
              pango.dev
              cairo.dev
              gdk-pixbuf.dev
              gst_all_1.gstreamer.dev
              gst_all_1.gst-plugins-base.dev
              libva
              fontconfig.dev
              freetype.dev
              harfbuzz.dev
              libxml2.dev
              gnutls.dev
              zlib.dev
              ncurses.dev
              dbus.dev
              sqlite.dev
              libselinux.dev
              tree-sitter
              gmp.dev
              libsoup_3.dev
              libGL.dev
              libxkbcommon.dev
              libdrm.dev
              mesa
              wayland.dev
              wpewebkit
              libwpe
              libwpe-fdo
            ]);

            # For bindgen to find libclang
            LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";

            shellHook = ''
              echo "=== Neomacs Development Environment ==="
              echo ""
              echo "Rust: $(rustc --version)"
              echo "Cargo: $(cargo --version)"
              echo "GTK4: $(pkg-config --modversion gtk4 2>/dev/null || echo 'not found')"
              echo "GStreamer: $(pkg-config --modversion gstreamer-1.0 2>/dev/null || echo 'not found')"
              echo "WPE WebKit: $(pkg-config --modversion wpe-webkit-2.0 2>/dev/null || echo 'not found')"
              echo ""

              # Library path for runtime — DO NOT include ncurses here,
              # it causes glibc version contamination with system shell.
              # The linker adds RPATH for ncurses during compilation.
              export LD_LIBRARY_PATH="${pkgs.lib.makeLibraryPath (with pkgs; [
                gtk4
                glib
                cairo
                pango
                gdk-pixbuf
                graphene
                gst_all_1.gstreamer
                gst_all_1.gst-plugins-base
                fontconfig
                freetype
                harfbuzz
                libxml2
                gnutls
                libjpeg
                libtiff
                giflib
                libpng
                librsvg
                libwebp
                dbus
                sqlite
                gmp
                libgccjit
                libsoup_3
                libGL
                mesa
                libdrm
                libxkbcommon
                libgbm
                wpewebkit
                libwpe
                libwpe-fdo
              ])}''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"

              # WPE WebKit environment
              export WPE_BACKEND_LIBRARY="${pkgs.libwpe-fdo}/lib/libWPEBackend-fdo-1.0.so"
              export GIO_MODULE_DIR="${pkgs.glib-networking}/lib/gio/modules"
              export WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1
              export WEBKIT_USE_SINGLE_WEB_PROCESS=1
              export PATH="${pkgs.wpewebkit}/libexec/wpe-webkit-2.0:$PATH"

              echo "Build commands:"
              echo "  1. cd rust/neomacs-display && cargo build --release"
              echo "  2. ./autogen.sh"
              echo "  3. ./configure --with-neomacs"
              echo "  4. make -j$(nproc)"
              echo ""
            '';
          };
        }
      );

      # Package (for nix build)
      packages = forAllSystems (system:
        let
          pkgs = pkgsFor system;
        in {
          default = self.packages.${system}.neomacs;

          neomacs = pkgs.callPackage ./nix/neomacs.nix { };
        }
      );
    };
}
