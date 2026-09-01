{ inputs, ... }:
{
  perSystem =
    { pkgs, ... }:
    {
      formatter = pkgs.nixfmt-tree;

      checks.nix-format =
        pkgs.runCommand "neomacs-nix-format"
          {
            nativeBuildInputs = [ pkgs.nixfmt ];
            source = inputs.self;
          }
          ''
            find "$source" -type f -name '*.nix' -print0 \
              | xargs -0 nixfmt --check
            touch "$out"
          '';
    };
}
