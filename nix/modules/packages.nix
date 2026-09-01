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
      neomacsMinimal = pkgs.neomacs-minimal;
      neomacsApp = {
        type = "app";
        program = lib.getExe neomacs;
        meta.description = neomacs.meta.description;
      };
      neomacsMinimalApp = {
        type = "app";
        program = lib.getExe neomacsMinimal;
        meta.description = "Minimal Neomacs without optional native capabilities";
      };
    in
    {
      packages = {
        default = neomacs;
        inherit neomacs;
        neomacs-minimal = neomacsMinimal;
      };

      apps = {
        default = neomacsApp;
        neomacs = neomacsApp;
        neomacs-minimal = neomacsMinimalApp;
      };
    };
}
