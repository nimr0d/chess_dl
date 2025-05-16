{
  installShellFiles,
  lib,
  rustPlatform,
}: let
  manifest = (lib.importTOML ../Cargo.toml).package;
in
  rustPlatform.buildRustPackage {
    pname = "chess_dl";
    inherit (manifest) version;

    src = lib.sourceByRegex ../. [
      "^Cargo.toml$"
      "^Cargo.lock$"
      "^src.*$"
      "^build.rs$"
    ];

    cargoLock.lockFile = ../Cargo.lock;

    nativeBuildInputs = [installShellFiles];

    meta = {
      inherit (manifest) description;
      homepage = manifest.repository;
      license = lib.licenses.mit;
      platforms = lib.platforms.all;
    };

    postInstall = ''
      installShellCompletion --bash $releaseDir/build/chess_dl-*/out/chess_dl.bash
      installShellCompletion --fish $releaseDir/build/chess_dl-*/out/chess_dl.fish
      installShellCompletion --zsh $releaseDir/build/chess_dl-*/out/_chess_dl
    '';
  }
