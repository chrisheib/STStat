let
  # 1. Import nixpkgs
  pkgs = import <nixpkgs> {
    overlays = [
      # 2. Pull in the rust-overlay
      (import (
        builtins.fetchTarball {
          url = "https://github.com/oxalica/rust-overlay/archive/master.tar.gz";
        }
      ))
    ];
  };

  # 3. Use one Rust toolchain and include rust-src for rust-analyzer.
  rust = pkgs.rust-bin.stable.latest.default.override {
    extensions = [
      "rust-src"
      "rust-analyzer"
      "clippy"
      "rustfmt"
    ];
  };

  buildInputs = with pkgs; [
    autoAddDriverRunpath
    binutils
    libGL
    libxkbcommon
    pkg-config
    rust
    wayland
    wayland-protocols
    libX11
    libxcb
    libxcb.dev
    libXcursor
    libXi
    libXrandr
    mold
    openssl
    ripgrep
  ];
in
pkgs.mkShell {
  inherit buildInputs;
  LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath buildInputs;
  RUST_SRC_PATH = "${rust}/lib/rustlib/src/rust/library";
  RUSTFLAGS = "-C link-args=-Wl,--no-rosegment,-fuse-ld=mold,-rpath,${pkgs.lib.makeLibraryPath buildInputs}";
  shellHook = ''
    echo "Rust $(rustc --version)"
    echo "Wayland libs: $(pkg-config --modversion wayland-client)"
    unset TEMP TMP TEMPDIR TMPDIR
    export RUST_BACKTRACE=1
  '';
}
