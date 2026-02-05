{
  description = "Neomacs - GPU-accelerated Emacs with GTK4 and WebKit";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

    # Rust toolchain
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    # WPE WebKit from eval-exec's nixpkgs fork (PR #449108)
    # Contains wpewebkit with ENABLE_WPE_PLATFORM for GPU rendering
    nixpkgs-wpewebkit = {
      url = "github:eval-exec/nixpkgs/b861f05af3a7a2c2c47a5ea3b20b78dadd40b192";
    };
  };

  outputs = { self, nixpkgs, rust-overlay, nixpkgs-wpewebkit }:
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
        # WPE WebKit from custom nixpkgs
        wpewebkit = (import nixpkgs-wpewebkit {
          inherit (final) system;
        }).wpewebkit;

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
              pkgs.llvmPackages.libclang
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

              # Library path for runtime
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
                ncurses
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
