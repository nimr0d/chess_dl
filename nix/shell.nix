{
  mkShell,
  callPackage,
  rustc,
  rust-analyzer,
  rustfmt,
  clippy,
  cargo,
  ...
}:
mkShell {
  inputsFrom = [(callPackage ./. {})];
  buildInputs = [
    cargo
    rustc
    rust-analyzer
    rustfmt
    clippy
  ];
}
