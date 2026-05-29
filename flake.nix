{
  description = "dotagent — egui agent manager — migrated";

  # To activate:
  #   cp flake.nix flake.nix.bak && cp flake.nix.proposed flake.nix
  #   nix flake update config
  # To revert:
  #   cp flake.nix.bak flake.nix && rm flake.nix.bak

  inputs = {
    config.url = "github:jaycee1285/config";
    nixpkgs.follows = "config/nixpkgs";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, config, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };
        libs = config.lib.runtimeLibs pkgs;
        rustToolchain = pkgs.rust-bin.stable.latest.default;
        rustPlatform = pkgs.makeRustPlatform {
          cargo = rustToolchain;
          rustc = rustToolchain;
        };
        nativeDeps = libs.egui;
        cleanSrc = pkgs.lib.cleanSourceWith {
          src = ./.;
          filter = path: type:
            let base = builtins.baseNameOf path; in
            !builtins.elem base [ "target" "result" ".git" ];
        };
      in {
        libs.declared = {
          categories = [ "egui" ];
          local = [];
        };

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rustToolchain pkg-config
          ] ++ nativeDeps;

          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath nativeDeps;
        };

        packages.default = rustPlatform.buildRustPackage {
          pname = "dotagent";
          version = "0.1.0";
          src = cleanSrc;
          cargoHash = "sha256-IySoaAILWv7EHw8tFCctvSIrYUtlc5JJD6jFbrtpyzI=";

          nativeBuildInputs = with pkgs; [ pkg-config ];
          buildInputs = nativeDeps;

          postInstall = ''
            install -d $out/share/dotagent/fonts
            cp -r ${./fonts}/. $out/share/dotagent/fonts/
          '';
        };

        apps.default = flake-utils.lib.mkApp {
          drv = self.packages.${system}.default;
        };
      });
}
