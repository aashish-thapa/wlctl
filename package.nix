{ lib, rustPlatform }:

let
  cargo = (lib.importTOML ./Cargo.toml).package;
in
rustPlatform.buildRustPackage {
  pname = cargo.name;
  version = cargo.version;

  src = ./.;

  cargoLock = {
    lockFile = ./Cargo.lock;
  };

  meta = {
    description = cargo.description;
    homepage = cargo.repository;
    license = lib.licenses.gpl3Only;
    mainProgram = "wlctl";
    platforms = lib.platforms.linux;
  };
}
