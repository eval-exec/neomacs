{ inputs, ... }:
{
  perSystem =
    {
      config,
      lib,
      pkgs,
      ...
    }:
    {
      checks = import ../checks {
        inherit lib pkgs;
        homeManagerLib = inputs.home-manager.lib;
        package = config.packages.default;
        minimalPackage = config.packages.neomacs-minimal;
        app = config.apps.default;
        devShell = config.devShells.default;
      };
    };
}
