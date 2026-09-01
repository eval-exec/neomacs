{
  description = "Neomacs - GPU-accelerated Emacs written in Rust with a modern, multithreaded architecture";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

    flake-parts = {
      url = "github:hercules-ci/flake-parts";
      inputs.nixpkgs-lib.follows = "nixpkgs";
    };

    crane.url = "github:ipetkov/crane";

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    home-manager = {
      url = "github:nix-community/home-manager";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    # WPE WebKit keeps its own pinned nixpkgs because its Cachix artifacts are
    # built against that revision. Following our nixpkgs would force an
    # expensive source rebuild.
    nix-wpe-webkit.url = "github:eval-exec/nix-wpe-webkit";
  };

  outputs =
    inputs@{ flake-parts, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "aarch64-darwin"
        "x86_64-darwin"
      ];

      imports = [
        ./nix/modules/overlays.nix
        ./nix/modules/packages.nix
        ./nix/modules/dev-shells.nix
        ./nix/modules/checks.nix
        ./nix/modules/formatter.nix
      ];
    };

  nixConfig = {
    extra-substituters = [
      "https://eval-exec.cachix.org"
      "https://nix-wpe-webkit.cachix.org"
    ];
    extra-trusted-public-keys = [
      "eval-exec.cachix.org-1:xvopUI7X7+Vt1gaSsWJ0PQFPP66vs8v5iIaz6boxf64="
      "nix-wpe-webkit.cachix.org-1:ItCjHkz1Y5QcwqI9cTGNWHzcox4EqcXqKvOygxpwYHE="
    ];
  };
}
