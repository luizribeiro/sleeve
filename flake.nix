{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
    git-hooks = {
      url = "github:cachix/git-hooks.nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    {
      nixpkgs,
      flake-utils,
      git-hooks,
      rust-overlay,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };
        toolchain = pkgs.rust-bin.stable.latest.minimal.override {
          extensions = [
            "clippy"
            "rust-analyzer"
            "rust-src"
            "rustfmt"
          ];
          targets = [
            "wasm32-unknown-unknown"
            "wasm32-wasip2"
          ];
        };
        msrvToolchain = pkgs.rust-bin.stable."1.96.0".minimal.override {
          targets = [
            "wasm32-unknown-unknown"
            "wasm32-wasip2"
          ];
        };
        cargoFiles = "(^|/)(Cargo\\.(toml|lock)|.*\\.rs)$";
        cargoHook =
          {
            name,
            text,
            cargoToolchain ? toolchain,
            runtimeInputs ? [ ],
            files ? cargoFiles,
          }:
          {
            enable = true;
            entry = "${
              pkgs.writeShellApplication {
                inherit name text;
                runtimeInputs = [ cargoToolchain ] ++ runtimeInputs;
              }
            }/bin/${name}";
            inherit files;
            pass_filenames = false;
          };
        cargoHooks = {
          rustfmt = cargoHook {
            name = "rustfmt-hook";
            text = ''
              cargo fmt --all -- --check
              cargo fmt --all --manifest-path guests/Cargo.toml -- --check
            '';
          };
          clippy = cargoHook {
            name = "clippy-hook";
            text = ''
              cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
              cargo clippy --manifest-path guests/Cargo.toml --workspace --all-targets --target wasm32-wasip2 --locked -- -D warnings
            '';
          };
          cargo-nextest = cargoHook {
            name = "cargo-nextest-hook";
            runtimeInputs = [ pkgs.cargo-nextest ];
            text = "cargo nextest run --release --workspace --all-features --locked --no-tests pass";
          };
          cargo-deny = cargoHook {
            name = "cargo-deny-hook";
            runtimeInputs = [ pkgs.cargo-deny ];
            files = "(^|/)(Cargo\\.(toml|lock)|deny\\.toml)$";
            text = "cargo deny check bans licenses sources";
          };
          doctests = cargoHook {
            name = "doctests-hook";
            text = "cargo test --doc --workspace --all-features --locked";
          };
          docs = cargoHook {
            name = "docs-hook";
            text = ''
              RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features --locked
            '';
          };
          msrv =
            (cargoHook {
              name = "msrv-hook";
              cargoToolchain = msrvToolchain;
              text = "cargo check --workspace --all-targets --locked";
            })
            // {
              stages = [ "pre-push" ];
            };
          jco-cross-host =
            (cargoHook {
              name = "jco-cross-host-hook";
              runtimeInputs = [ pkgs.nodejs_24 ];
              text = ''
                npm ci --prefix hosts/jco --ignore-scripts --offline
                cargo test --release -p jco-runner --test cross_host --locked
              '';
            })
            // {
              stages = [ "pre-push" ];
            };
        };
        offlineHooks = {
          nixfmt.enable = true;
          deadnix.enable = true;
          statix.enable = true;
          taplo.enable = true;
          actionlint.enable = true;
          typos.enable = true;
          check-merge-conflicts.enable = true;
          end-of-file-fixer.enable = true;
          trim-trailing-whitespace.enable = true;
          check-yaml.enable = true;
          check-toml.enable = true;
        };
        hookDefinitions = offlineHooks // cargoHooks;
        gitHooks = git-hooks.lib.${system}.run {
          src = ./.;
          hooks = hookDefinitions;
        };
      in
      {
        checks.pre-commit = git-hooks.lib.${system}.run {
          src = ./.;
          hooks = offlineHooks;
        };

        devShells.default = pkgs.mkShell {
          packages = [
            toolchain
            pkgs.wasmtime
            pkgs.wasm-tools
            pkgs.wac-cli
            pkgs.nodejs_24
            pkgs.cargo-nextest
            pkgs.cargo-deny
            pkgs.git-absorb
            pkgs.libiconv
          ]
          ++ gitHooks.enabledPackages;
          inherit (gitHooks) shellHook;
        };
      }
    );
}
