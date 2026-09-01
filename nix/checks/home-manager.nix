{
  homeManagerLib,
  pkgs,
  package,
}:
let
  startupContract = import ./startup-contract.nix;
  configuration = homeManagerLib.homeManagerConfiguration {
    inherit pkgs;
    modules = [
      {
        home.username = "neomacs-nix-check";
        home.homeDirectory = "/tmp/neomacs-home-manager-contract";
        home.stateVersion = "24.11";
        # This rolling contract intentionally follows the flake's locked
        # nixpkgs while exercising Home Manager's current Emacs module.  Their
        # development version labels can differ between release branch points.
        home.enableNixpkgsReleaseCheck = false;

        programs.emacs = {
          enable = true;
          package = package;
        };

        # Keep this fixture focused on the package integration contract.
        manual.manpages.enable = false;
        news.display = "silent";
      }
    ];
  };
  finalPackage = configuration.config.programs.emacs.finalPackage;
in
pkgs.runCommand "neomacs-home-manager-contract"
  {
    nativeBuildInputs = [
      pkgs.coreutils
      pkgs.gnugrep
    ];
  }
  ''
    test -x ${configuration.activationPackage}/activate
    test -x ${finalPackage}/bin/emacs
    test -x ${finalPackage}/bin/emacsclient

    ${startupContract {
      executable = "${finalPackage}/bin/emacs";
      marker = "home-manager neomacs contract ok";
    }}

    touch "$out"
  ''
