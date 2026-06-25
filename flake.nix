{
  description = "Neomacs - GPU-accelerated Emacs written in Rust with a modern, multithreaded architecture";

  nixConfig = {
    extra-substituters = [
      "https://eval-exec.cachix.org"
      "https://nix-wpe-webkit.cachix.org"
    ];
    extra-trusted-public-keys = [
      "eval-exec.cachix.org-1:xvopUI7X7+Vt1gaSsWJ0PQFPP66vs8v5iIaz6boxf64="
      "nix-wpe-webkit.cachix.org-1:ItCjHkz1Y5QcwqI9cTGNWHzcox4EqcXqKvOygxpwYHE="
    ];
  };

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    # Rust toolchain
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
    };

    # Crane for incremental Rust builds (caches deps separately from source)
    crane.url = "github:ipetkov/crane";

    # WPE WebKit standalone flake with Cachix binary cache
    # Do NOT use `inputs.nixpkgs.follows` here — the Cachix binary was built
    # with nix-wpe-webkit's own pinned nixpkgs, so follows would change the
    # derivation hash and cause a cache miss (rebuilding from source ~1 hour).
    nix-wpe-webkit = {
      url = "github:eval-exec/nix-wpe-webkit";
    };
  };

  outputs = { self, nixpkgs, rust-overlay, crane, nix-wpe-webkit }:
    let
      lib = nixpkgs.lib;

      supportedSystems = [ "x86_64-linux" "aarch64-linux" "aarch64-darwin" "x86_64-darwin" ];

      forAllSystems = lib.genAttrs supportedSystems;

      # Create pkgs with overlays for each system
      pkgsFor = system: import nixpkgs {
        inherit system;
        overlays = [
          rust-overlay.overlays.default
          self.overlays.default
        ];
      };

      commonBuildInputsFor = pkgs: with pkgs; [
        ncurses
        gnutls
        zlib
        libxml2
        fontconfig
        freetype
        harfbuzz
        libotf
        cairo
        glib
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
        poppler
        dbus
        sqlite
        tree-sitter
        gmp
      ] ++ lib.optionals pkgs.stdenv.isLinux (with pkgs; [
        alsa-lib
        gst_all_1.gst-vaapi
        libva
        libselinux
        libGL
        vulkan-loader
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
        xdg-dbus-proxy
        libx11
        libxpm
        libxcursor
        libxrandr
        libxi
        libxinerama
      ]);

      commonNativeBuildInputsFor = pkgs: [
        pkgs.rust-neomacs
        pkgs.rust-cbindgen
        pkgs.pkg-config
        pkgs.llvmPackages.clang
        pkgs.makeWrapper
      ];

      mkNeomacsPackage = system:
        let
          pkgs = pkgsFor system;
          craneLib = (crane.mkLib pkgs).overrideToolchain pkgs.rust-neomacs;
          cargoSrc = craneLib.cleanCargoSource ./.;
          packageSrc = builtins.path {
            path = ./.;
            name = "neomacs-source";
            filter = path: _type:
              let
                base = builtins.baseNameOf path;
              in
              !(builtins.elem base [ ".git" ".direnv" ".github" "target" "result" "flake.nix" "flake.lock" ]);
          };
          pname = "neomacs";
          version = self.shortRev or self.dirtyShortRev or self.lastModifiedDate or "0.0.1";
          runtimeLibs = commonBuildInputsFor pkgs;
          commonArgs = {
            inherit pname version;
            src = cargoSrc;
            strictDeps = true;
            cargoExtraArgs = "-p neomacs";
            nativeBuildInputs = commonNativeBuildInputsFor pkgs;
            buildInputs = runtimeLibs;
            doCheck = false;
          };
          depsArgs = commonArgs // {
            # Keep the dependency-artifact derivation stable across commits.
            # The final package still uses the git-derived version, but Rust
            # dependency compilation should only change when Cargo metadata or
            # build inputs change.
            version = "0.0.0";
            dummySrc = craneLib.mkDummySrc {
              src = cargoSrc;
              cleanCargoTomlFilter = craneLib.filters.cargoTomlAggressive;
            };
          };
          cargoArtifacts = craneLib.buildDepsOnly depsArgs;
          hostEmulator = pkgs.stdenv.hostPlatform.emulator pkgs.buildPackages;
          fingerprintRunner = lib.optionalString (hostEmulator != null) "${hostEmulator} ";
          linuxWrapArgs = lib.optionals pkgs.stdenv.isLinux [
            "--set-default" "VK_DRIVER_FILES" "$(echo ${pkgs.mesa}/share/vulkan/icd.d/*.json | tr ' ' ':')"
            "--set-default" "WPE_BACKEND_LIBRARY" "${pkgs.libwpe-fdo}/lib/libWPEBackend-fdo-1.0.so"
            "--set-default" "GIO_MODULE_DIR" "${pkgs.glib-networking}/lib/gio/modules"
            "--set-default" "WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS" "1"
            "--set-default" "WEBKIT_USE_SINGLE_WEB_PROCESS" "1"
            "--prefix" "PATH" ":" "${pkgs.wpewebkit}/libexec/wpe-webkit-2.0"
          ];
        in
        craneLib.buildPackage (commonArgs
          // {
            src = packageSrc;
            inherit cargoArtifacts;

            # After crane builds the Rust binaries, run the xtask bootstrap
            # pipeline (--skip-build reuses the binaries crane just built):
            # pbootstrap → COMPILE_FIRST → loaddefs → pdump
            postBuild = ''
              cargo xtask fresh-build --release --skip-build
            '';

            postInstall = ''
              mkdir -p "$out/share/neomacs"
              cp -r lisp "$out/share/neomacs/"
              cp -r etc "$out/share/neomacs/"
              chmod -R u+w "$out/share/neomacs"

              final_pdump="target/release/neomacs.pdump"
              if [ ! -f "$final_pdump" ]; then
                echo "missing final pdump image: $final_pdump" >&2
                exit 1
              fi
              fingerprint="$(${fingerprintRunner}$out/bin/neomacs --fingerprint | tr -d '[:space:]')"
              if ! [[ "$fingerprint" =~ ^[[:xdigit:]]{64}$ ]]; then
                echo "invalid final pdump fingerprint: $fingerprint" >&2
                exit 1
              fi
              install -m 0644 "$final_pdump" "$out/bin/neomacs.pdump"
              install -m 0644 "$final_pdump" "$out/bin/neomacs-$fingerprint.pdump"

              wrapProgram "$out/bin/neomacs" \
                --prefix LD_LIBRARY_PATH : "${pkgs.lib.makeLibraryPath runtimeLibs}" \
                --set-default RUST_LOG info \
                --set-default NEOMACS_RUNTIME_ROOT "$out/share/neomacs" \
                ${lib.concatStringsSep " \\\n                " linuxWrapArgs}
            '';
          });

    in {
      # Overlay that provides wpewebkit (Linux only) and rust toolchain
      overlays.default = final: prev: {
        # Rust toolchain from rust-toolchain.toml (with extra extensions)
        rust-neomacs = (final.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml).override {
          extensions = [ "rust-src" "rust-analyzer" ];
        };
      } // (lib.optionalAttrs prev.stdenv.isLinux {
        # WPE WebKit from nix-wpe-webkit flake (with Cachix binary cache)
        # Only available on Linux — WPE WebKit does not support macOS.
        wpewebkit = nix-wpe-webkit.packages.${final.stdenv.hostPlatform.system}.wpewebkit;
      });

    # Development shell
      devShells = forAllSystems (system:
        let
          pkgs = pkgsFor system;
          isLinux = pkgs.stdenv.isLinux;
          isDarwin = pkgs.stdenv.isDarwin;
        in {
          default = pkgs.mkShell {
            name = "neomacs-dev";

            nativeBuildInputs = [
              # Rust toolchain
              pkgs.rust-neomacs
              pkgs.rust-cbindgen

              # Build tools
              pkgs.pkg-config

              # For bindgen (generates Rust bindings from C headers)
              pkgs.llvmPackages.clang
            ];

            buildInputs = commonBuildInputsFor pkgs
              ++ lib.optionals isLinux (with pkgs; [
                gcc
                xwininfo
              ]);

            # pkg-config paths for dev headers
            PKG_CONFIG_PATH = pkgs.lib.makeSearchPath "lib/pkgconfig" (with pkgs; [
              glib.dev
              cairo.dev
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
              wpewebkit
              libwpe
              libwpe-fdo
            ]);

            # For bindgen to find libclang
            LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";

            shellHook = ''
              export RUST_BACKTRACE=1
              echo "=== Neomacs Development Environment ==="
              echo ""
              echo "Rust: $(rustc --version)"
              echo "Cargo: $(cargo --version)"
              echo "GStreamer: $(pkg-config --modversion gstreamer-1.0 2>/dev/null || echo 'not found')"
            ''
            # Linux-specific shell hook
            + lib.optionalString isLinux ''
              echo "xkbcommon: $(pkg-config --modversion xkbcommon 2>/dev/null || echo 'not found')"
              echo "WPE WebKit: $(pkg-config --modversion wpe-webkit-2.0 2>/dev/null || echo 'not found')"
              echo ""

              # Library path for runtime — DO NOT include ncurses here,
              # it causes glibc version contamination with system shell.
              # The linker adds RPATH for ncurses during compilation.
              export LD_LIBRARY_PATH="${pkgs.lib.makeLibraryPath (with pkgs; [
                glib
                cairo
                gst_all_1.gstreamer
                gst_all_1.gst-plugins-base
                fontconfig
                freetype
                harfbuzz
                libotf
                libxml2
                gnutls
                zlib
                libjpeg
                libtiff
                giflib
                libpng
                librsvg
                libwebp
                dbus
                sqlite
                gmp
                alsa-lib
                libsoup_3
                libGL
                vulkan-loader
                mesa
                libdrm
                libxkbcommon
                libgbm
                # Display libs dynamically loaded by winit
                wayland
                libx11
                libxpm
                libxcursor
                libxrandr
                libxi
                libxinerama
                wpewebkit
                libwpe
                libwpe-fdo
              ])}''${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"

              # Vulkan ICD discovery — tell the Vulkan loader where Mesa's
              # driver JSON files are (e.g. intel_icd.x86_64.json for anv).
              # Without this, wgpu can't find Vulkan drivers and falls back to OpenGL.
              export VK_DRIVER_FILES="$(echo ${pkgs.mesa}/share/vulkan/icd.d/*.json | tr ' ' ':')"

              # WPE WebKit environment
              export WPE_BACKEND_LIBRARY="${pkgs.libwpe-fdo}/lib/libWPEBackend-fdo-1.0.so"
              export GIO_MODULE_DIR="${pkgs.glib-networking}/lib/gio/modules"
              export WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1
              export WEBKIT_USE_SINGLE_WEB_PROCESS=1
              export PATH="${pkgs.wpewebkit}/libexec/wpe-webkit-2.0:$PATH"

              # X11/Wayland display — preserve from parent env or detect from running session.
              # nix develop sanitizes env, so DISPLAY/XAUTHORITY may be lost.
              # Detect them from a running desktop session via /proc/<pid>/environ.
              _detect_display_env() {
                local _pid
                # NixOS wraps binaries, so process names may be e.g. ".kwin_x11-wrapp";
                # use substring match (no -x flag) to handle this.
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
            # Darwin-specific shell hook
            + lib.optionalString isDarwin ''
              echo ""
              echo "Note: WPE WebKit is not available on macOS."
              echo "      WebKit-based features will be disabled."
            ''
            # Common shell hook (both platforms)
            + ''
              # Set default log levels (can be overridden before entering nix develop)
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
          };
        }
      );

      packages = forAllSystems (system:
        let
          neomacs = mkNeomacsPackage system;
        in {
          default = neomacs;
          neomacs = neomacs;
        });

      apps = forAllSystems (system:
        let
          pkg = self.packages.${system}.default;
        in {
          default = {
            type = "app";
            program = "${pkg}/bin/neomacs";
          };
          neomacs = {
            type = "app";
            program = "${pkg}/bin/neomacs";
          };
        });
    };
}
