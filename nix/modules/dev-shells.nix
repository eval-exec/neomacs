{ ... }:
{
  perSystem =
    { pkgs, ... }:
    {
      devShells.default = import ../dev-shell.nix {
        inherit pkgs;
        rustToolchain = pkgs.rust-neomacs;
        wpeWebkit = if pkgs.stdenv.hostPlatform.isLinux then pkgs.neomacs-wpewebkit else null;
      };

      # Build environment for the GNU Emacs parity oracle.  It exists so a
      # local oracle can be compiled by the same recipe as CI's
      # (scripts/ci/build-gnu-oracle.sh); see ../gnu-oracle-shell.nix.
      devShells.gnu-oracle = import ../gnu-oracle-shell.nix { inherit pkgs; };
    };
}
