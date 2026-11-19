{
  description = "Thunderstorm: High-performance SNOMED CT UK Clinical Ontology Engine";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" "clippy" "rustfmt" ];
        };
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "thunderstorm";
          version = "0.1.0";
          src = ./.;
          cargoLock = {
            lockFile = ./Cargo.lock;
          };
          nativeBuildInputs = [ pkgs.pkg-config ];
        };

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rustToolchain
            pkg-config
          ];

          shellHook = ''
            echo "⚡ Thunderstorm SNOMED CT Engine Loaded"
            echo "Rust:  $(rustc --version)"
            echo "Cargo: $(cargo --version)"
          '';
        };

        devShell = self.devShells.${system}.default;
      }
    );
}
