{
  pkgs,
  lib,
  config,
  inputs,
  # autoAddDriverRunpath,
  ...
}: {
  # https://devenv.sh/basics/
  env.GREET = "devenv";

  # https://devenv.sh/packages/
  packages = [
    pkgs.git
    pkgs.gcc
    pkgs.openssl.dev
    pkgs.libpkgconf
    pkgs.pkg-config
    pkgs.xorg.libxcb.dev
    pkgs.wayland
  ];

  env.RUSTFLAGS = lib.mkForce "-C link-args=-Wl,-fuse-ld=mold,-rpath,${with pkgs;
    lib.makeLibraryPath [
      libGL
      libxkbcommon
      wayland
      xorg.libX11
      xorg.libXcursor
      xorg.libXi
      xorg.libXrandr
      # linuxPackages.nvidia_x11
      # linuxKernel.packages.linux_xanmod_latest.nvidia_x11
      autoAddDriverRunpath
    ]}";

  env.RUST_BACKTRACE = "1";
  # env.RUSTFLAGS = "-C link-args=-Wl,-fuse-ld=mold,-rpath,$(devenv makeLibraryPath pkgs.libGL)";

  # -C link-arg=-fuse-ld=mold

  # env.LD_LIBRARY_PATH = with pkgs;
  #   lib.makeLibraryPath [
  #     libGL
  #     libxkbcommon
  #     wayland
  #     xorg.libX11
  #     xorg.libXcursor
  #     xorg.libXi
  #     xorg.libXrandr
  #   ];
  # ...
  # LD_LIBRARY_PATH = libPath;

  # https://devenv.sh/languages/
  languages.rust.enable = true;

  # https://devenv.sh/processes/
  # processes.cargo-watch.exec = "cargo-watch";

  # https://devenv.sh/services/
  # services.postgres.enable = true;

  # https://devenv.sh/scripts/

  # scripts.nushell-greet = {
  #   exec = ''
  #     def greet [name] {
  #       ["hello" $name]
  #     }
  #     greet "world"
  #   '';
  #   package = pkgs.nushell;
  #   binary = "nu";
  #   description = "Greet in Nu Shell";
  # };

  #enterShell = ''
  #  nu
  #'';

  # https://devenv.sh/tasks/
  # tasks = {
  #   "myproj:setup".exec = "mytool build";
  #   "devenv:enterShell".after = [ "myproj:setup" ];
  # };

  # https://devenv.sh/tests/
  enterTest = ''
    echo "Running tests"
    git --version | grep --color=auto "${pkgs.git.version}"
  '';

  # https://devenv.sh/pre-commit-hooks/
  # pre-commit.hooks.shellcheck.enable = true;

  # See full reference at https://devenv.sh/reference/options/
}
