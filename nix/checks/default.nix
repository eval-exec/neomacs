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
      !pkgs.stdenv.hostPlatform.isLinux || pkgs ? neomacs-wpewebkit
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
        test -s ${checkedPackage}/share/neomacs/info/dir
        test -s ${checkedPackage}/share/neomacs/info/.neomacs-info-manifest
        info_files=(${checkedPackage}/share/neomacs/info/*.info)
        test "''${#info_files[@]}" -eq 66
        for manual in "''${info_files[@]}"; do
          test -s "$manual"
          test -s "${checkedPackage}/share/info/''${manual##*/}"
        done
        test -d ${checkedPackage}/share/man
        ${lib.optionalString pkgs.stdenv.hostPlatform.isLinux ''
          test -f ${checkedPackage}/share/applications/neomacs.desktop
          test -f ${checkedPackage}/share/icons/hicolor/scalable/apps/neomacs.svg
        ''}
        test -f ${checkedPackage}/bin/neomacs.pdump
        test ! -e ${checkedPackage}/bin/libneomacs_video_gstreamer.so

        # bin/neomacs is the wrapProgram shell script (nix/package.nix); the
        # linked executable it execs is bin/.neomacs-wrapped.
        test -x ${checkedPackage}/bin/.neomacs-wrapped
        readelf --dynamic ${checkedPackage}/bin/.neomacs-wrapped \
          | grep -Eq 'Shared library: \[libgstreamer-1[.]0[.]so'

        fingerprint="$(${checkedPackage}/bin/neomacs --fingerprint | tr -d '[:space:]')"
        if ! [[ "$fingerprint" =~ ^[[:xdigit:]]{64}$ ]]; then
          echo "invalid installed Neomacs fingerprint: $fingerprint" >&2
          exit 1
        fi
        test -f "${checkedPackage}/bin/neomacs-$fingerprint.pdump"

        ${lib.optionalString pkgs.stdenv.hostPlatform.isLinux ''
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

        # Let the wrapper choose its own runtime root. Neither a user INFOPATH
        # nor another editor's manuals may satisfy the installed Info contract.
        info_output="$(env -u INFOPATH -u NEOMACS_RUNTIME_ROOT \
          ${checkedPackage}/bin/neomacs -Q --batch --eval \
          '(progn
             (require (quote info))
             (dolist (manual (quote ("emacs" "eintr" "elisp")))
               (Info-goto-node (concat "(" manual ")Top"))
               (unless (and
                        (equal (file-name-directory (file-truename Info-current-file))
                               "${checkedPackage}/share/neomacs/info/")
                        (equal (file-name-base Info-current-file) manual)
                        (equal (file-name-directory
                                (file-truename
                                 (concat "${checkedPackage}/share/neomacs/info/"
                                         manual ".info")))
                               "${checkedPackage}/share/neomacs/info/"))
                 (error "Info used an external manual: %S" Info-current-file)))
             (Info-goto-node "(elisp)Hooks")
             (unless (equal Info-current-node "Hooks")
               (error "Missing installed elisp Hooks node"))
             (princ "nix installed Info contract ok\n"))')"
        grep -Fqx 'nix installed Info contract ok' <<<"$info_output"

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
