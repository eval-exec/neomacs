{ ... }:
{
  perSystem =
    {
      config,
      lib,
      pkgs,
      ...
    }:
    let
      neomacs = pkgs.neomacs;
      neomacsApp = {
        type = "app";
        program = lib.getExe neomacs;
        meta.description = neomacs.meta.description;
      };
    in
    {
      packages = {
        default = neomacs;
        inherit neomacs;

        # The external programs the package parity suites shell out to, as one
        # PATH-shaped environment.  The suites' records are only valid for the
        # tool *versions* they were taken against, so this exists to be the
        # provider of record rather than whatever the host has: a lock row in
        # `neomacs-infra` names this attribute with `nix:melpa-tools`, and the
        # resolved store path's `bin` leads the sessions' PATH.
        #
        # The nixpkgs revision in `flake.lock` is the pin.  A tool whose record
        # needs an *exact* version the current revision no longer has wants a
        # dedicated nixpkgs input beside this one, the way `nix-wpe-webkit`
        # above is pinned to the revision its Cachix artifacts were built
        # against.
        melpa-tools = pkgs.buildEnv {
          name = "melpa-tools";
          paths = [
            pkgs.git
            pkgs.nodejs
            pkgs.jdk21
            pkgs.python3
            pkgs.ruby
            pkgs.ripgrep
            pkgs.gnupg
            pkgs.pandoc
          ];
        };
      };

      apps = {
        default = neomacsApp;
        neomacs = neomacsApp;
      };
    };
}
