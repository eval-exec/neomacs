{ ... }:
{
  perSystem =
    { pkgs, ... }:
    {
      devShells.default = import ../dev-shell.nix {
        inherit pkgs;
        rustToolchain = pkgs.rust-neomacs;
        wpeWebkit = if pkgs.stdenv.isLinux then pkgs.neomacs-wpewebkit else null;
      };
    };
}
