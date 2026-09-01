{ inputs, lib, ... }:
let
  mkNeomacs =
    final: inputs: minimal:
    import ../package.nix {
      inherit (inputs) crane;
      inherit minimal;
      pkgs = final;
      rustToolchain = final.rust-neomacs;
      source = ../..;
      version =
        inputs.self.shortRev or inputs.self.dirtyShortRev or inputs.self.lastModifiedDate or "0.0.1";
      wpeWebkit = if final.stdenv.isLinux then final.neomacs-wpewebkit else null;
    };
  neomacsOverlay =
    final: prev:
    {
      rust-neomacs = (final.rust-bin.fromRustupToolchainFile ../../rust-toolchain.toml).override {
        extensions = [
          "rust-src"
          "rust-analyzer"
        ];
      };

      neomacs = mkNeomacs final inputs false;
      neomacs-minimal = mkNeomacs final inputs true;
    }
    // lib.optionalAttrs prev.stdenv.isLinux {
      # Keep nix-wpe-webkit's pinned nixpkgs so this resolves to its cache,
      # while avoiding an override of the consumer's generic wpewebkit.
      neomacs-wpewebkit = inputs.nix-wpe-webkit.packages.${prev.stdenv.hostPlatform.system}.wpewebkit;
    };
in
{
  # One self-contained public overlay: consumers get both the pinned Rust
  # toolchain and pkgs.neomacs by applying only overlays.default.
  flake.overlays.default = lib.composeManyExtensions [
    inputs.rust-overlay.overlays.default
    neomacsOverlay
  ];

  perSystem =
    { system, ... }:
    {
      _module.args.pkgs = import inputs.nixpkgs {
        inherit system;
        config = { };
        overlays = [ inputs.self.overlays.default ];
      };
    };
}
