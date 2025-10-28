# Release Builds

This project provides helper scripts and configuration for reproducible release builds on macOS and Windows.

## Prerequisites

- Rust toolchain with the appropriate targets installed:
  - `rustup target add aarch64-apple-darwin`
  - `rustup target add x86_64-apple-darwin`
  - `rustup target add x86_64-pc-windows-msvc`
- For Windows packaging: WiX Toolset 3.11+ and the `cargo-wix` subcommand (`cargo install cargo-wix`).

## macOS

1. Run `scripts/build-macos-release.sh [target-triple]`.  
   The default target is `aarch64-apple-darwin`. Pass `x86_64-apple-darwin` to build for Intel Macs.
2. The script places the binary in `target/dist/report-builder-<version>-<target>/` and produces a `tar.gz` archive in `target/dist/`.
3. Distribute the archive. Users can unpack it and move the `report-builder` binary to a directory on their `PATH` (for example `/usr/local/bin`).

Release builds use thin LTO and strip debug symbols to keep the binary size small.

## Windows

1. Ensure the WiX Toolset binaries are on `PATH` and that `cargo wix` is installed.
2. Run the helper script from PowerShell: `scripts\build-windows-installer.ps1`.
3. The script produces a release build for the `x86_64-pc-windows-msvc` target and then invokes `cargo wix` to create an MSI at `target\wix\report-builder-x86_64-pc-windows-msvc.msi`.
4. The installer adds the installation directory to the system `PATH`. After installation, `report-builder` is accessible from any PowerShell or Command Prompt session.

## Verification

- Run `cargo test` and any ad-hoc verification before packaging.
- Install artifacts on a clean virtual machine when possible to confirm the PATH integration and CLI functionality.
