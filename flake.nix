{
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
        rustToolchain = pkgs.rust-bin.stable.latest.default;
      in {
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            rustToolchain
            pkg-config

            # egui/eframe native deps
            libxkbcommon
            libGL
            wayland
            libx11
            libxcursor
            libxrandr
            libxi
            vulkan-loader

            # fontconfig for system font fallback
            fontconfig
            freetype
          ];

          LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath (with pkgs; [
            libxkbcommon
            libGL
            wayland
            libx11
            libxcursor
            libxrandr
            libxi
            vulkan-loader
            fontconfig
            freetype
          ]);
        };
      });
}
