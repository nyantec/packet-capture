{
  description = "packet-capture";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable-small";

  outputs = { self, nixpkgs }: let
    overlay = final: prev: {
      packet-capture = final.callPackage (
        { rustPlatform }:

        rustPlatform.buildRustPackage {
          pname = "packet-capture";
          version = self.shortRev or "dirty-${toString self.lastModifiedDate}";
          src = self;
          cargoLock.lockFile = ./Cargo.lock;
        }
      ) {};
    };
  in {
    inherit overlay;
    packages.x86_64-linux = import nixpkgs {
      system = "x86_64-linux";
      overlays = [ overlay ];
    };
    defaultPackage.x86_64-linux = self.packages.x86_64-linux.packet-capture;
  };
}
