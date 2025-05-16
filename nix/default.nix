{
  lib,
  stdenv,
  buildPackages,
  rustPlatform,
  installShellFiles,
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
    ];

    cargoLock.lockFile = ../Cargo.lock;

    nativeBuildInputs = [installShellFiles];

    meta = {
      inherit (manifest) description;
      homepage = manifest.repository;
      license = lib.licenses.mit;
      platforms = lib.platforms.all;
    };

    postInstall = lib.optionalString (stdenv.hostPlatform.emulatorAvailable buildPackages) (
      let
        emulator = stdenv.hostPlatform.emulator buildPackages;
      in ''
        installShellCompletion --cmd chess_dl \
          --bash <(${emulator} $out/bin/chess_dl completions bash) \
          --fish <(${emulator} $out/bin/chess_dl completions fish) \
          --zsh <(${emulator} $out/bin/chess_dl completions zsh) \
          --elvish <(${emulator} $out/bin/chess_dl completions elvish) \
          --powershell <(${emulator} $out/bin/chess_dl completions powershell)
      ''
    );
  }
