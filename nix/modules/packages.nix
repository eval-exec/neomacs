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
      };

      apps = {
        default = neomacsApp;
        neomacs = neomacsApp;
      };
    };
}
