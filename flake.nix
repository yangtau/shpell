{
  description = "shpell: write shell commands in natural language";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      inherit (nixpkgs) lib;
      systems = [ "x86_64-linux" "aarch64-linux" "x86_64-darwin" "aarch64-darwin" ];
      forAllSystems = f: lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
      prebuilt = builtins.fromJSON (builtins.readFile ./nix/prebuilt-hashes.json);

      meta = pkgs: {
        description = "Write shell commands in natural language";
        homepage = "https://github.com/yangtau/shpell";
        license = pkgs.lib.licenses.mit;
        platforms = systems;
        mainProgram = "shpell";
      };

      shpellFromSource = pkgs: pkgs.rustPlatform.buildRustPackage {
        pname = "shpell";
        version = "0.1.0";
        src = self;
        cargoLock.lockFile = ./Cargo.lock;
        meta = meta pkgs;
      };

      shpellPrebuilt = pkgs: system: hash: pkgs.stdenv.mkDerivation {
        pname = "shpell";
        version = builtins.substring 0 7 prebuilt.rev;
        src = pkgs.fetchurl {
          url = "https://github.com/yangtau/shpell/releases/download/prebuilt/shpell-${system}-${prebuilt.rev}.tar.gz";
          inherit hash;
        };
        nativeBuildInputs = lib.optionals pkgs.stdenv.isLinux [ pkgs.autoPatchelfHook ];
        buildInputs = lib.optionals pkgs.stdenv.isLinux [ pkgs.stdenv.cc.cc.lib ];
        dontUnpack = true;
        dontConfigure = true;
        dontBuild = true;
        dontStrip = true;
        installPhase = ''
          runHook preInstall
          mkdir -p $out/bin
          tar -xzf $src -C $out/bin
          test -x "$out/bin/shpell"
          runHook postInstall
        '';
        meta = meta pkgs;
      };
    in
    {
      packages = forAllSystems (pkgs:
        let
          system = pkgs.stdenv.hostPlatform.system;
          fromSource = shpellFromSource pkgs;
          hash = prebuilt.hashes.${system} or null;
          # Clean trees download the last CI tarball. Dirty trees compile.
          usePrebuilt = hash != null && self ? rev;
          pkg = if usePrebuilt then shpellPrebuilt pkgs system hash else fromSource;
        in
        {
          shpell = pkg;
          shpell-from-source = fromSource;
          default = pkg;
        });

      devShells = forAllSystems (pkgs: {
        default = pkgs.mkShell {
          packages = with pkgs; [ cargo rustc rustfmt clippy rust-analyzer ];
        };
      });
    };
}
