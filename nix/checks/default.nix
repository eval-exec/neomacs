{
  lib,
  pkgs,
  homeManagerLib,
  package,
  app,
  devShell,
}:
let
  system = pkgs.stdenv.hostPlatform.system;
  productionCapabilities = package.productionCapabilities;
  startupContract = import ./startup-contract.nix;
  outputContract =
    assert lib.assertMsg (pkgs ? neomacs) "overlays.default must expose pkgs.neomacs";
    assert lib.assertMsg (
      pkgs.neomacs == package
    ) "packages.${system}.default must be built through overlays.default";
    assert lib.assertMsg (
      !pkgs.stdenv.isLinux || pkgs ? neomacs-wpewebkit
    ) "overlays.default must expose its pinned WPE package under a Neomacs-specific name";
    assert lib.assertMsg (
      package.type or null == "derivation"
    ) "packages.${system}.default must be a derivation";
    assert lib.assertMsg (app.type or null == "app") "apps.${system}.default must be an app";
    assert lib.assertMsg (
      devShell.type or null == "derivation"
    ) "devShells.${system}.default must be a derivation";
    assert lib.assertMsg (
      productionCapabilities ? cargoFeatures
    ) "packages.${system}.default must publish its Cargo capability set";
    assert lib.assertMsg (
      productionCapabilities ? videoBackend
    ) "packages.${system}.default must publish its video backend policy";
    pkgs.runCommand "neomacs-${system}-flake-output-contract" { } ''
      touch "$out"
    '';

  canRunPackage = pkgs.stdenv.buildPlatform.canExecute pkgs.stdenv.hostPlatform;

  packageContract =
    { checkedPackage }:
    pkgs.runCommand "neomacs-${system}-installed-package-contract"
      {
        nativeBuildInputs = [
          pkgs.binutils
          pkgs.coreutils
          pkgs.gnugrep
        ];
      }
      ''
        test -x ${checkedPackage}/bin/neomacs
        test -x ${checkedPackage}/bin/neomacsclient
        test -L ${checkedPackage}/bin/emacs
        test -L ${checkedPackage}/bin/emacsclient
        test -d ${checkedPackage}/share/neomacs/lisp
        test -d ${checkedPackage}/share/neomacs/etc
        test -f ${checkedPackage}/share/emacs/site-lisp/site-start.el
        test -f ${checkedPackage}/share/emacs/site-lisp/subdirs.el
        test -d ${checkedPackage}/share/applications
        test -d ${checkedPackage}/share/icons
        test -d ${checkedPackage}/share/info
        test -d ${checkedPackage}/share/man
        ${lib.optionalString pkgs.stdenv.isLinux ''
          test -f ${checkedPackage}/share/applications/neomacs.desktop
          test -f ${checkedPackage}/share/icons/hicolor/scalable/apps/neomacs.svg
        ''}
        test -f ${checkedPackage}/bin/neomacs.pdump
        test ! -e ${checkedPackage}/bin/libneomacs_video_gstreamer.so

        readelf --dynamic ${checkedPackage}/bin/neomacs \
          | grep -Eq 'Shared library: \[libgstreamer-1[.]0[.]so'

        fingerprint="$(${checkedPackage}/bin/neomacs --fingerprint | tr -d '[:space:]')"
        if ! [[ "$fingerprint" =~ ^[[:xdigit:]]{64}$ ]]; then
          echo "invalid installed Neomacs fingerprint: $fingerprint" >&2
          exit 1
        fi
        test -f "${checkedPackage}/bin/neomacs-$fingerprint.pdump"

        ${lib.optionalString pkgs.stdenv.isLinux ''
          export GST_REGISTRY="$PWD/gstreamer-registry.bin"
          export GST_PLUGIN_SYSTEM_PATH_1_0="${checkedPackage.gstreamerRuntime.pluginSystemPath}"
          export GST_PLUGIN_SCANNER_1_0="${checkedPackage.gstreamerRuntime.pluginScanner}"
          ${checkedPackage.gstreamerRuntime.inspect} typefind >/dev/null
          ${checkedPackage.gstreamerRuntime.inspect} decodebin >/dev/null
          ${checkedPackage.gstreamerRuntime.inspect} playbin >/dev/null
        ''}

        ${startupContract {
          executable = "${checkedPackage}/bin/neomacs";
          marker = "nix installed-package contract ok";
        }}

        touch "$out"
      '';
in
{
  flake-output-contract = outputContract;
}
// lib.optionalAttrs canRunPackage {
  installed-package-contract = packageContract { checkedPackage = package; };
  home-manager-contract = import ./home-manager.nix {
    inherit homeManagerLib pkgs package;
  };
}
