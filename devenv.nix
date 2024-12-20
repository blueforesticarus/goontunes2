{ pkgs, lib, config, inputs, ... }:

{
  # https://devenv.sh/basics/
  env.GREET = "(rust devenv)";
  env.LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
  env.RUSTC_WRAPPER = "${pkgs.sccache}/bin/sccache";

  languages.rust = {
    enable = true;
    # https://devenv.sh/reference/options/#languagesrustchannel
    channel = "nightly";
    mold.enable = true;

    components = [ "rustc" "cargo" "clippy" "rustfmt" "rust-analyzer" "rustc-codegen-cranelift-preview"];
  };

  # https://devenv.sh/packages/
  packages = with pkgs; [ 
    cargo-udeps #check for unused deps in cargo.toml

    rustc.llvmPackages.clang

    pkg-config
    openssl
    openssl.dev
    udev.dev
    cacert
    sqlite # for matrix_rust_sdk
    sqlite.dev

    sccache
  ];

  enterShell = ''
    unset CFLAGS
  '';
}
