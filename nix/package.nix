{
  crane,
  pkgs,
  rustToolchain,
  source,
  version,
  wpeWebkit,
}:
let
  inherit (pkgs) lib;
  craneLib = (crane.mkLib pkgs).overrideToolchain rustToolchain;
  cargoSrc = craneLib.cleanCargoSource source;
  productionCapabilities = import ./production-capabilities.nix {
    inherit lib pkgs source;
  };
  dependencies = import ./dependencies.nix {
    inherit lib pkgs wpeWebkit;
  };
  cargoPackages = [
    "-p"
    "neomacs"
  ];
  cargoFeatures = map (feature: "neomacs/${feature}") productionCapabilities.cargoFeatures;
  cargoFeatureArgs = lib.optionals (cargoFeatures != [ ]) [
    "--features"
    (lib.concatStringsSep "," cargoFeatures)
  ];
  cargoBuildArgs = lib.concatStringsSep " " (cargoPackages ++ cargoFeatureArgs);
  runtimeLibs = dependencies.productionBuildInputs productionCapabilities;
  videoEnabled = builtins.elem "video" productionCapabilities.cargoFeatures;
  gstreamerRuntime = import ./gstreamer-runtime.nix {
    inherit lib pkgs;
    pluginInputs = dependencies.videoPluginInputs;
  };
  commonArgs = {
    pname = "neomacs";
    inherit version;
    src = cargoSrc;
    strictDeps = true;
    cargoExtraArgs = cargoBuildArgs;
    nativeBuildInputs = [
      rustToolchain
      pkgs.binutils
      pkgs.pkg-config
      pkgs.llvmPackages.clang
      pkgs.makeWrapper
    ]
    ++ lib.optionals pkgs.stdenv.isDarwin [
      # xtask's fresh-build pipeline re-signs role binaries after patching
      # the pdump fingerprint. sigtool supplies codesign in the sandbox.
      pkgs.darwin.sigtool
    ];
    buildInputs = runtimeLibs;
    doCheck = false;
  };
  cargoArtifacts = craneLib.buildDepsOnly (
    commonArgs
    // {
      # Keep dependency artifacts stable across commits. Let buildDepsOnly
      # synthesize its dummy source to avoid import-from-derivation.
      version = "0.0.0";
    }
  );
  hostEmulator = pkgs.stdenv.hostPlatform.emulator pkgs.buildPackages;
  fingerprintRunner = lib.optionalString (hostEmulator != null) "${hostEmulator} ";
  linuxWrapArgs =
    lib.optionals pkgs.stdenv.isLinux [
      "--set-default"
      "VK_DRIVER_FILES"
      "$(echo ${pkgs.mesa}/share/vulkan/icd.d/*.json | tr ' ' ':')"
    ]
    ++ lib.optionals (pkgs.stdenv.isLinux && videoEnabled) [
      "--set-default"
      "GST_PLUGIN_SYSTEM_PATH_1_0"
      gstreamerRuntime.pluginSystemPath
      "--set-default"
      "GST_PLUGIN_SCANNER_1_0"
      gstreamerRuntime.pluginScanner
    ]
    ++
      lib.optionals (pkgs.stdenv.isLinux && builtins.elem "webview" productionCapabilities.cargoFeatures)
        [
          "--set-default"
          "WPE_BACKEND_LIBRARY"
          "${pkgs.libwpe-fdo}/lib/libWPEBackend-fdo-1.0.so"
          "--set-default"
          "GIO_MODULE_DIR"
          "${pkgs.glib-networking}/lib/gio/modules"
          "--set-default"
          "WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS"
          "1"
          "--set-default"
          "WEBKIT_USE_SINGLE_WEB_PROCESS"
          "1"
          "--prefix"
          "PATH"
          ":"
          "${wpeWebkit}/libexec/wpe-webkit-2.0"
        ];
in
craneLib.buildPackage (
  commonArgs
  // {
    # The flake is already a content-addressed, Git-filtered source tree. The
    # full tree is required for Lisp/runtime assets; Cargo-only filtering is
    # used solely for the dependency derivation above.
    src = source;
    inherit cargoArtifacts;

    postBuild = ''
      cargo xtask fresh-build --release --skip-build
    '';

    postInstall = ''
      mkdir -p "$out/share/neomacs"
      cp -r lisp "$out/share/neomacs/"
      cp -r etc "$out/share/neomacs/"
      chmod -R u+w "$out/share/neomacs"

      # GNU Emacs installs this version-independent site-lisp root.
      # Nixpkgs' emacsPackagesFor wrapper composes its generated site-start
      # with the wrapped editor's original file at this exact path.
      mkdir -p "$out/share/emacs/site-lisp"
      printf '%s\n' \
        ';;; site-start.el --- Nix Emacs package compatibility  -*- lexical-binding: t; -*-' \
        ';;; Commentary:' \
        ';; Neomacs runtime paths are configured by its executable wrapper.' \
        ';;; Code:' \
        ';;; site-start.el ends here' \
        > "$out/share/emacs/site-lisp/site-start.el"
      printf '%s\n' \
        ';;; subdirs.el --- Nix Emacs package compatibility  -*- lexical-binding: t; -*-' \
        > "$out/share/emacs/site-lisp/subdirs.el"

      mkdir -p \
        "$out/share/applications" \
        "$out/share/icons" \
        "$out/share/info" \
        "$out/share/man"
      ${lib.optionalString pkgs.stdenv.isLinux ''
        bash scripts/install-linux-desktop-assets.sh "$out"
      ''}

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

      ln -s neomacs "$out/bin/emacs"
      ln -s neomacsclient "$out/bin/emacsclient"

      wrapProgram "$out/bin/neomacs" \
        --prefix LD_LIBRARY_PATH : "${pkgs.lib.makeLibraryPath runtimeLibs}" \
        --set-default RUST_LOG info \
        --set-default NEOMACS_RUNTIME_ROOT "$out/share/neomacs" \
        ${lib.concatStringsSep " \\\n        " linuxWrapArgs}
    '';

    passthru = {
      inherit gstreamerRuntime productionCapabilities;
    };

    meta = {
      description = "GPU-accelerated Emacs-compatible editor written in Rust";
      homepage = "https://github.com/eval-exec/neomacs";
      license = lib.licenses.gpl3Plus;
      mainProgram = "neomacs";
    };
  }
)
