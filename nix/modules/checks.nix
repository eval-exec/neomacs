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
        app = config.apps.default;
        devShell = config.devShells.default;
      };
    };
}
