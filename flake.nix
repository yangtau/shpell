{
  description = "shpell: write shell commands in natural language";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  inputs.nix-prebuilt.url = "github:yangtau/nix-prebuilt";

  outputs =
    { self, nixpkgs, nix-prebuilt }:
    let
      inherit (nixpkgs) lib;
      systems = [ "aarch64-darwin" "aarch64-linux" "x86_64-linux" ];
      meta = {
        description = "Write shell commands in natural language";
        homepage = "https://github.com/yangtau/shpell";
        license = lib.licenses.mit;
      };
    in
    {
      packages = nix-prebuilt.lib.mkPackages {
        inherit self nixpkgs meta systems;
        pname = "shpell";
        owner = "yangtau";
        repo = "shpell";
        hashes = ./.nix/prebuilt-hashes.json;
        fromSource =
          pkgs:
          pkgs.rustPlatform.buildRustPackage {
            pname = "shpell";
            version = "0.1.0";
            src = self;
            cargoLock.lockFile = ./Cargo.lock;
            meta = meta // {
              platforms = systems;
              mainProgram = "shpell";
            };
          };
      };

      devShells = lib.genAttrs systems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = pkgs.mkShell {
            packages = with pkgs; [
              cargo
              rustc
              rustfmt
              clippy
              rust-analyzer
            ];
          };
        }
      );
    };
}
